//! `garnish doctor`: a diagnostic report for "why does my status line look like that".

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::ansi::display_width;
use crate::cache::{Cache, Entry, Status};
use crate::claude_settings::{self, FileKeys, FileState, Tui};
use crate::config::{self, Config};
use crate::icons::IconSet;
use crate::modules::SCHEMAS;

/// Build the report from the process environment.
///
/// The settings chain is the current directory's (Claude Code's project
/// directory when `doctor` runs where the session was started) and the
/// user's (`CLAUDE_CONFIG_DIR`, else `~/.claude`), the managed file first
/// (the platform's, or what `GARNISH_MANAGED_SETTINGS` says).
///
/// The config is the one `config path` prints ([`config::read_target`]),
/// the file the status line's ticks read.
#[must_use]
pub fn report(config_path: Option<&Path>) -> String {
    let home = claude_settings::home_dir();
    let user = claude_settings::user_dir(home.as_deref());
    let managed = claude_settings::managed_settings_path();
    report_with(
        &config::read_target(config_path),
        &Cache::from_env(),
        managed.as_deref(),
        std::env::current_dir().ok().as_deref(),
        user.as_deref(),
    )
}

/// Build the report against explicit cache, managed-file, project and
/// Claude user-directory locations.
///
/// `managed` is `None` for a report that must not read the machine's
/// organisation file, `user` (the directory holding the user
/// `settings.json`, [`claude_settings::user_dir`]) when there is no home
/// directory.
#[must_use]
pub fn report_with(
    config_file: &config::ReadTarget,
    cache: &Cache,
    managed: Option<&Path>,
    project: Option<&Path>,
    user: Option<&Path>,
) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "garnish {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        o,
        "binary   {}",
        std::env::current_exe().map_or_else(|_| "?".into(), |p| tilde(&p))
    );
    let on_path = crate::install::on_path("garnish", std::env::var_os("PATH").as_deref());
    let _ = writeln!(
        o,
        "on PATH  {}",
        if on_path { "yes" } else { "no (run `make install` or `garnish install --absolute`)" }
    );
    let _ = writeln!(o, "git      {}", git_version());
    let _ = writeln!(o);
    // A `--config` the status line command passes that names no one file,
    // or that a checkout chooses, is said once; the report then shows what
    // a bare lookup finds.
    let (config_path, unresolved) = match config_file {
        config::ReadTarget::File(p) => (Some(p.as_path()), None),
        config::ReadTarget::Defaults => (None, None),
        config::ReadTarget::Unresolved { settings, word } => {
            let refusal = crate::install::Refusal::UnresolvedConfig {
                settings: settings.clone(),
                word: word.clone(),
            };
            (None, Some(refusal.to_string()))
        }
        config::ReadTarget::Checkout(checkout) => {
            let refusal = crate::install::Refusal::CheckoutConfig(checkout.clone());
            (None, Some(refusal.to_string()))
        }
    };
    let loaded = config::load(config_path, &SCHEMAS);
    let chain = read_chain(&claude_settings::settings_chain(managed, project, user));
    for row in settings_rows(&chain, project, &loaded.config, crate::time::animate_from_env()) {
        let _ = writeln!(o, "{row}");
    }
    let _ = writeln!(o);
    if let Some(note) = &unresolved {
        let _ = writeln!(o, "config   {}", crate::ansi::plain_text(note));
    }
    config_section(&mut o, &loaded, unresolved.is_some());
    cache_section(&mut o, cache);
    environment_section(&mut o);
    glyph_section(&mut o, &loaded.config);
    o
}

/// One file of the settings chain as `doctor` reads it: its label
/// (`managed`, `local`, `project`, `user`), its path, what it holds as
/// Claude Code reads it, and whether it is longer than the
/// [`claude_settings::MAX_SETTINGS_BYTES`] a tick reads, so that the keys
/// garnish reads on the tick skip it.
pub type ChainEntry = (&'static str, PathBuf, FileState, bool);

/// A settings chain (the labelled paths of
/// [`claude_settings::settings_chain`]), read.
///
/// Whole, within [`claude_settings::MAX_COMMAND_SETTINGS_BYTES`], as
/// Claude Code reads it: most rows are about what Claude Code does with a
/// key (follow-up review of 2026-09-25: past the tick's cap the report said
/// the status line was not configured while Claude Code ran it).
#[must_use]
pub fn read_chain(chain: &[(&'static str, PathBuf)]) -> Vec<ChainEntry> {
    chain
        .iter()
        .map(|(label, path)| {
            let state =
                claude_settings::read_file_up_to(path, claude_settings::MAX_COMMAND_SETTINGS_BYTES);
            let past_tick_cap = matches!(state, FileState::Keys(_))
                && std::fs::metadata(path)
                    .is_ok_and(|m| m.len() > claude_settings::MAX_SETTINGS_BYTES);
            (*label, path.clone(), state, past_tick_cap)
        })
        .collect()
}

/// Most characters of a string from a settings file the report echoes
/// (`statusLine.command`, a `tui` value that is neither name): the file may
/// come with a cloned repository, so the value is plain text cut to a
/// line's worth, never the row-breaking original.
const MAX_VALUE_CHARS: usize = 200;

/// `plain` cut to [`MAX_VALUE_CHARS`], with `…` when something was cut.
fn line_of(plain: &str) -> String {
    let mut shown: String = plain.chars().take(MAX_VALUE_CHARS).collect();
    if plain.chars().nth(MAX_VALUE_CHARS).is_some() {
        shown.push('…');
    }
    shown
}

/// The `claude settings` rows of the report (SPEC § 7).
///
/// One row per file of the chain, saying whether it is there and parses
/// (the project's files relative to `project`, the directory the chain was
/// built for), then the keys that change what the line can show, each with
/// the file it comes from and, where the config calls for another value,
/// the suggestion. `session_animate` is the session switch
/// (`GARNISH_ANIMATE`), which decides with the config and the chain
/// whether anything animates at all.
#[must_use]
pub fn settings_rows(
    chain: &[ChainEntry],
    project: Option<&Path>,
    config: &Config,
    session_animate: bool,
) -> Vec<String> {
    let row = |key: &str, text: &str| format!("{key:<24}{text}");
    let scope = project
        .map_or_else(|| "no project directory".to_owned(), |dir| format!("for {}", tilde(dir)));
    let mut rows =
        vec![row("claude settings", &format!("({scope}; the first file that sets a key wins)"))];
    for (label, path, state, past_tick_cap) in chain {
        let status = match state {
            FileState::Absent => "absent".to_owned(),
            FileState::Unreadable(e) => format!("unreadable: {e}"),
            FileState::Invalid(e) => format!("{e}; garnish reads none of it"),
            FileState::Keys(keys) => match claude_settings::rejected(label, keys) {
                Some(why) => format!("{why}; garnish reads none of it"),
                None if *past_tick_cap => format!(
                    "ok, but longer than the {} bytes a tick reads: the keys a tick reads (prefersReducedMotion, the badges, auto-compaction) skip it",
                    claude_settings::MAX_SETTINGS_BYTES
                ),
                None => "ok".to_owned(),
            },
        };
        // Only the project's own files are named relative to it: a user or
        // managed file under it (a project at `/`) would lose its root.
        let shown = project
            .filter(|_| matches!(*label, "local" | "project"))
            .and_then(|dir| path.strip_prefix(dir).ok())
            .map_or_else(|| tilde(path), |rel| rel.display().to_string());
        rows.push(format!("  {label:<8} {shown}  {status}"));
    }
    if !chain.iter().any(|(label, ..)| *label == "user") {
        rows.push("  user     unknown: HOME is not set".to_owned());
    }
    match resolved(chain, |k| k.status_line_command.clone()) {
        Some((command, from)) => {
            let shown = line_of(&crate::ansi::plain_text(&command));
            rows.push(row("statusLine", &format!("command={} ({from})", tilde(Path::new(&shown)))));
        }
        None => rows.push(row("statusLine", "not configured (run `garnish install`)")),
    }
    let reduced = resolved_on_tick(chain, |k| k.reduced_motion);
    let reduced_on = reduced.as_ref().is_some_and(|(on, _)| *on);
    let animating = session_animate && config.animate.unwrap_or(!reduced_on);
    let interval = resolved(chain, |k| k.refresh_interval);
    // Claude Code drops a value below 1 (its schema's minimum), so such a
    // file re-runs the line on events only, like one without the key.
    let mut text = match &interval {
        None => "unset".to_owned(),
        Some((secs, from)) if *secs < 1.0 => {
            format!("{secs} ({from}), below 1 so Claude Code ignores it")
        }
        Some((secs, from)) => format!("{secs} ({from})"),
    };
    let every_second = interval.as_ref().is_some_and(|(secs, _)| secs.total_cmp(&1.0).is_eq());
    if ticks_every_second(config, animating) && !every_second {
        text.push_str(
            "; set 1 so the clock, the countdowns and the animations move every second (`garnish install` writes it)",
        );
    }
    rows.push(row("  refreshInterval", &text));
    let hide_vim = resolved(chain, |k| k.hide_vim_mode);
    let mut text = hide_vim
        .as_ref()
        .map_or_else(|| "unset".to_owned(), |(hide, from)| format!("{hide} ({from})"));
    if placed(config, "vim") && hide_vim.as_ref().is_none_or(|(hide, _)| !hide) {
        text.push_str(
            "; set true: the vim module already shows the mode, so Claude Code's own indicator would repeat it",
        );
    }
    rows.push(row("  hideVimModeIndicator", &text));
    rows.push(row("  padding", &padding_text(chain, config)));
    let text = match resolved(chain, |k| k.disable_all_hooks) {
        Some((true, from)) => {
            format!(
                "true ({from}): Claude Code does not run the status line command while it is set"
            )
        }
        Some((false, from)) => format!("false ({from})"),
        None => "unset".to_owned(),
    };
    rows.push(row("disableAllHooks", &text));
    let mut text =
        reduced.as_ref().map_or_else(|| "unset".to_owned(), |(on, from)| format!("{on} ({from})"));
    if reduced.as_ref().is_some_and(|(on, _)| *on) {
        text.push_str(match config.animate {
            None => ": animations are frozen (`animate` in the config would decide instead)",
            Some(true) => ", overridden by `animate = true` in the config",
            Some(false) => "; `animate = false` in the config freezes them anyway",
        });
    }
    rows.push(row("prefersReducedMotion", &text));
    let sandbox = switch_row(chain, config, "sandbox", |k| k.sandbox_enabled);
    rows.push(row("sandbox.enabled", &sandbox));
    let voice = switch_row(chain, config, "voice", |k| k.voice_enabled);
    rows.push(row("voice.enabled", &voice));
    rows.push(row("tui", &tui_row(chain)));
    rows
}

/// The `statusLine.padding` row: the value and its file and, when the
/// config's `padding` is not twice it, the value that fits. The harness
/// pads both sides, so the box is `COLUMNS − 4 − 2N` wide (SPEC § 2.1) and
/// a config short of `2N` draws every full-width row too wide.
fn padding_text(chain: &[ChainEntry], config: &Config) -> String {
    let Some((cells, from)) = resolved(chain, |k| k.padding) else { return "unset".to_owned() };
    let fits = cells.saturating_mul(2);
    let has = u64::try_from(config.padding).unwrap_or(u64::MAX);
    let mut text = format!("{cells} ({from})");
    let why = match has.cmp(&fits) {
        std::cmp::Ordering::Less => "or rows are cut with …",
        std::cmp::Ordering::Greater => "so the rows fill the box",
        std::cmp::Ordering::Equal => return text,
    };
    let _ = write!(text, "; set `padding = {fits}` in the config, {why}");
    text
}

/// A `sandbox.enabled` or `voice.enabled` row: the value and the file it
/// comes from, and what the badge module of that name (SPEC § 3.8) makes
/// of it when the config places it.
fn switch_row(
    chain: &[ChainEntry],
    config: &Config,
    id: &str,
    pick: impl Fn(&FileKeys) -> Option<bool>,
) -> String {
    let value = resolved_on_tick(chain, pick);
    let on = value.as_ref().is_some_and(|(on, _)| *on);
    let mut text =
        value.as_ref().map_or_else(|| "unset".to_owned(), |(on, from)| format!("{on} ({from})"));
    if placed(config, id) {
        let _ = if on {
            write!(text, "; the {id} module shows it")
        } else {
            write!(text, "; nothing for the {id} module to show")
        };
    }
    text
}

/// The `tui` row: which renderer the settings ask for, which decides what
/// a tall status line does (SPEC § 2.1).
///
/// Claude Code's schema takes only the two names. Another value in the
/// managed file is dropped on its own; in any other file it has Claude
/// Code reject the whole file. Neither decides, so the search goes on to
/// the next file that sets the key and the row names what was skipped.
fn tui_row(chain: &[ChainEntry]) -> String {
    let mut skipped = Vec::new();
    let mut decided = None;
    for (label, _, state, _) in chain {
        let FileState::Keys(keys) = state else { continue };
        match &keys.tui {
            None => {}
            Some(Tui::Fullscreen) => {
                decided = Some((true, *label));
                break;
            }
            Some(Tui::Default) => {
                decided = Some((false, *label));
                break;
            }
            Some(Tui::Other(value)) => {
                // A string is shown quoted, so an empty or blank one is
                // visible; another JSON value as the JSON it is.
                let shown = value.as_str().map_or_else(
                    || line_of(&value.to_string()),
                    |s| format!("{:?}", line_of(&crate::ansi::plain_text(s))),
                );
                skipped.push(if *label == "managed" {
                    format!(
                        "{shown} (managed) is not `default` or `fullscreen`, so Claude Code ignores it there"
                    )
                } else {
                    format!(
                        "{shown} ({label}) is not `default` or `fullscreen`, so Claude Code rejects that file and reads none of its keys"
                    )
                });
            }
        }
    }
    let mut text = match decided {
        Some((true, from)) => format!(
            "fullscreen ({from}): asks for the alternate-screen renderer, where the prompt box and the status line share at most half the terminal's rows and a taller status line loses its last rows (more while a long prompt is being typed); an environment switch or the terminal can override it"
        ),
        Some((false, from)) => format!(
            "default ({from}): asks for the classic renderer, which cuts nothing and scrolls instead; every status line row costs a row of transcript; `CLAUDE_CODE_NO_FLICKER=1` overrides it"
        ),
        None => "unset: Claude Code picks the renderer (a new install starts in fullscreen; after its first sessions the server gates decide, classic by default); `/tui` shows and sets it".to_owned(),
    };
    for note in skipped {
        text.push_str("; ");
        text.push_str(&note);
    }
    text
}

/// The first file of the chain that sets a key, with the file's label:
/// Claude Code's own precedence for one key (managed > local > project >
/// user), a file it rejects ([`claude_settings::rejected`]) skipped.
fn resolved<T>(
    chain: &[ChainEntry],
    pick: impl Fn(&FileKeys) -> Option<T>,
) -> Option<(T, &'static str)> {
    first_set(chain.iter(), pick)
}

/// [`resolved`] for a key garnish reads on the tick, which skips a file
/// longer than it reads ([`claude_settings::read_keys`]).
fn resolved_on_tick<T>(
    chain: &[ChainEntry],
    pick: impl Fn(&FileKeys) -> Option<T>,
) -> Option<(T, &'static str)> {
    first_set(chain.iter().filter(|(.., past_tick_cap)| !past_tick_cap), pick)
}

/// The first of `entries` that sets the key `pick` names.
fn first_set<'a, T>(
    mut entries: impl Iterator<Item = &'a ChainEntry>,
    pick: impl Fn(&FileKeys) -> Option<T>,
) -> Option<(T, &'static str)> {
    entries.find_map(|(label, _, state, _)| match state {
        FileState::Keys(keys) if claude_settings::rejected(label, keys).is_none() => {
            pick(keys).map(|value| (value, *label))
        }
        _ => None,
    })
}

/// Whether a module id is on a line and enabled.
fn placed(config: &Config, id: &str) -> bool {
    let enabled = id.strip_prefix(crate::modules::text::PREFIX).map_or_else(
        || config.modules.get(id).is_some_and(|m| m.enabled),
        |name| config.texts.get(name).is_some_and(|m| m.enabled),
    );
    enabled && config.rows.iter().any(|r| r.ids().any(|m| m == id))
}

/// Whether the config shows something that changes every second, which is
/// what `statusLine.refreshInterval = 1` is for: a module whose value ticks
/// whatever the animation switch says (the clock, the elapsed times, the
/// cache's warm countdown, a limit's reset while it counts, its `eta`) or,
/// while animations run (`animating`), an animation (SPEC § 4.2: the
/// ticker, a rule pattern, separator or icon frames, a text module whose
/// text is wider than its box and not clipped).
fn ticks_every_second(config: &Config, animating: bool) -> bool {
    const TICKING: [&str; 4] = ["clock", "session", "api", "cache"];
    const COUNTDOWNS: [&str; 3] = ["limit5h", "limit7d", "spend"];
    // `reset = "absolute"` is a wall-clock time, which does not move; the
    // `eta` is a duration, which does, whatever `show_reset` says.
    let counts = |m: &config::schema::ModuleCfg| {
        (m.bool("show_reset") && m.str("reset") != "absolute") || m.bool("eta")
    };
    let ticking = TICKING.iter().any(|id| placed(config, id))
        || COUNTDOWNS
            .iter()
            .any(|id| placed(config, id) && config.modules.get(id).is_some_and(counts));
    let scrolls = |m: &config::schema::ModuleCfg| {
        let width = m.size("width");
        m.str("overflow") != "clip" && width > 0 && display_width(m.str("text")) > width
    };
    let animated = animating
        && (config.overflow == config::Overflow::Ticker
            || !config.frame.fill_pattern.is_empty()
            || !config.frame.separator_frames.is_empty()
            || config
                .modules
                .iter()
                .any(|(id, m)| placed(config, id) && !m.all_icon_frames().is_empty())
            || config.texts.iter().any(|(name, m)| {
                placed(config, &format!("{}{name}", crate::modules::text::PREFIX)) && scrolls(m)
            }));
    ticking || animated
}

/// The `config` rows: the file and its problems. `refused` says the
/// status line command's `--config` was refused above, where `config init`
/// refuses too, so the hint for an absent file names the flag instead.
fn config_section(o: &mut String, loaded: &config::Loaded, refused: bool) {
    match (&loaded.path, loaded.errors.is_empty()) {
        (None, _) if refused => {
            let _ = writeln!(
                o,
                "config   none (built-in defaults); `garnish --config <FILE> config init` writes one"
            );
        }
        (None, _) => {
            let _ = writeln!(
                o,
                "config   none (built-in defaults); `garnish config init` writes one to {}",
                config::default_path()
                    .map_or_else(|| "… nowhere: HOME is not set".to_owned(), |p| tilde(&p))
            );
        }
        (Some(p), true) => {
            let _ = writeln!(o, "config   {} ok", tilde(p));
        }
        (Some(p), false) => {
            // A whole-file problem has no key path: a syntax error has a
            // line, a file that cannot be read none.
            let whole = |line: bool| {
                loaded.errors.iter().any(|e| e.path.is_empty() && e.line.is_some() == line)
            };
            if whole(true) {
                let _ = writeln!(
                    o,
                    "config   {} does not parse; the built-in defaults are in effect",
                    tilde(p)
                );
            } else if whole(false) {
                let _ = writeln!(
                    o,
                    "config   {} cannot be read; the built-in defaults are in effect",
                    tilde(p)
                );
            } else {
                let _ = writeln!(
                    o,
                    "config   {} has {} problem(s); the built-in default stands in for each bad key",
                    tilde(p),
                    loaded.errors.len()
                );
            }
            for e in &loaded.errors {
                let _ = writeln!(o, "         {e}");
            }
        }
    }
    let c = &loaded.config;
    let _ = writeln!(
        o,
        "preset={} icons={} theme={} frame={} rows={}",
        c.preset.name(),
        c.icons.name(),
        c.theme_name,
        c.frame.style.name(),
        c.rows.len()
    );
    let _ = writeln!(o);
}

/// Bytes of `debug.log` read for its tail: the log rotates past 1 MiB.
const MAX_DEBUG_LOG_BYTES: u64 = 2 * 1024 * 1024;

fn cache_section(o: &mut String, cache: &Cache) {
    let root = cache.root();
    let state = cache.refused().map_or_else(
        || probe(root, |from, to| std::fs::hard_link(from, to)),
        |why| format!("REFUSED: {why}; nothing is cached and no worker runs"),
    );
    let _ = writeln!(o, "cache    {} ({state})", tilde(root));
    let sessions = count_dirs(&root.join("sessions"));
    let repos = count_dirs(&root.join("repos"));
    let _ = writeln!(o, "         {sessions} session dir(s), {repos} repo dir(s)");
    let failures = failed_entries(root);
    let fetches = fetch_failures(root);
    if failures.is_empty() && fetches.is_empty() {
        let _ = writeln!(o, "         no failed refreshes");
    }
    // The text came from a command run in a repository nobody here built,
    // and this goes to a terminal: plain text, one line's worth.
    for (path, entry) in failures {
        let _ = writeln!(
            o,
            "         FAILED {} ({}s ago): {}",
            path,
            entry.age_ms() / 1000,
            line_of(&crate::ansi::plain_text(&entry.error))
        );
    }
    // A fetch that fails keeps the counts (they come from the refs on
    // disk), so its entry is `ok` and this is the only place it shows.
    for (path, entry) in fetches {
        let tried = entry.get("fetch_attempt").and_then(|t| t.parse::<i64>().ok());
        let ago = tried.map_or(0, |t| crate::time::now_secs().saturating_sub(t).max(0));
        let error = entry.get("fetch_error").unwrap_or_default();
        let _ = writeln!(
            o,
            "         FETCH FAILED {path} ({ago}s ago): {}",
            line_of(&crate::ansi::plain_text(error))
        );
    }
    let log = claude_settings::read_regular(&root.join("debug.log"), MAX_DEBUG_LOG_BYTES);
    if let Ok(Some(bytes)) = log {
        let log = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = log.lines().collect();
        let tail = lines.iter().rev().take(10).rev();
        let _ = writeln!(
            o,
            "         debug.log (last {} of {} lines):",
            tail.len().min(10),
            lines.len()
        );
        for line in tail {
            let _ = writeln!(o, "           {}", crate::ansi::plain_text(line));
        }
    }
    let _ = writeln!(o);
}

/// Whether workers can use the cache root: it takes a file, and a hard
/// link to one, which is how a lock is taken (`link` is `hard_link`, or a
/// test's stand-in). A filesystem without hard links (exFAT, some SMB
/// mounts) is writable and still locks nothing. The root is made as the
/// cache makes it (`0700`), and the probe as the cache makes a temporary
/// file: never through a link planted at its predictable name.
fn probe(root: &Path, link: impl Fn(&Path, &Path) -> std::io::Result<()>) -> String {
    let probe = root.join(format!(".probe.{}", std::process::id()));
    let linked = root.join(format!(".probe.{}.link", std::process::id()));
    let writable = crate::cache::create_private_dir(root).is_ok()
        && crate::cache::create_fresh(&probe).is_ok();
    let state = if !writable {
        "NOT writable".to_owned()
    } else if let Err(e) = link(&probe, &linked) {
        format!("writable, but no hard links ({e}): workers cannot lock")
    } else {
        "writable".to_owned()
    };
    let _ = std::fs::remove_file(&linked);
    let _ = std::fs::remove_file(&probe);
    state
}

/// Every `GARNISH_*` test hook, named by the constant each reader uses so a
/// new hook cannot be added without a row here (SPEC § 9 Test hooks; a unit
/// test scans the source for a hook this list forgot).
pub const TEST_HOOKS: [&str; 10] = [
    config::CONFIG_ENV,
    crate::cache::CACHE_DIR_ENV,
    crate::time::NOW_ENV,
    crate::spawn::NO_SPAWN_ENV,
    crate::cli::COLUMNS_ENV,
    crate::debug::DEBUG_ENV,
    crate::time::ANIMATE_ENV,
    crate::claude_settings::MANAGED_SETTINGS_ENV,
    crate::cli::STDIN_TTY_ENV,
    crate::cli::TEST_PANIC_ENV,
];

fn environment_section(o: &mut String) {
    let _ = writeln!(o, "environment");
    // The terminal's own variables, garnish's test hooks, then the settings
    // and renderer switches Claude Code reads (SPEC § 2.1, § 2.3, § 4.2).
    let keys =
        ["COLUMNS", "LINES", "NO_COLOR", "TZ", "TZDIR"].into_iter().chain(TEST_HOOKS).chain([
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
            "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE",
            "DISABLE_AUTO_COMPACT",
            "DISABLE_COMPACT",
            "CLAUDE_CODE_NO_FLICKER",
            "CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN",
            "CLAUDE_CODE_DECSTBM",
            claude_settings::CONFIG_DIR_ENV,
        ]);
    for key in keys {
        if let Ok(v) = std::env::var(key) {
            // The path-valued hooks may carry the home directory.
            let v = if key.ends_with("_CONFIG")
                || key.ends_with("_DIR")
                || key.ends_with("_SETTINGS")
            {
                tilde(Path::new(&v))
            } else {
                v
            };
            // It moves Claude Code's own files, so it decides where the
            // chain's user row, `install` and the skills look.
            let moved = if key == claude_settings::CONFIG_DIR_ENV && !v.is_empty() {
                " (Claude Code's user settings.json, skills and .claude.json live here)"
            } else {
                ""
            };
            let _ = writeln!(o, "         {key}={v}{moved}");
        }
    }
    let _ = writeln!(o);
}

fn glyph_section(o: &mut String, config: &config::Config) {
    let _ = writeln!(
        o,
        "glyph test: every `|` should sit in a column with the ones above and below.\n\
         A `|` pushed out of its column marks a glyph your terminal draws wider or\n\
         narrower than garnish counts (the number after it); a box means your font\n\
         lacks the glyph. The `config` rows are the icons your config resolves to,\n\
         overrides included. Override a glyph under [modules.<id>.icons], or paste\n\
         this block into a feedback issue."
    );
    for set in IconSet::ALL {
        for row in glyph_rows(set) {
            let _ = writeln!(o, "{row}");
        }
    }
    for row in config_glyph_rows(config) {
        let _ = writeln!(o, "{row}");
    }
}

/// The glyph-test rows for one built-in icon set, one per module.
///
/// Each single-character icon is padded to two cells and followed by `|` and
/// garnish's cell count, so every field is four cells wide and a glyph the
/// terminal draws wider than counted pushes its `|` out of the column.
/// Multi-character icons (spinner frames, the effort scale, ASCII words) are
/// not single cells and are left out.
#[must_use]
pub fn glyph_rows(set: IconSet) -> Vec<String> {
    rows(set.name(), |_, icon| (icon.glyph.get(set).to_owned(), false))
}

/// The glyph-test rows for the icons a loaded config resolves to (icon set,
/// presets and per-module overrides applied), labelled `config`.
#[must_use]
pub fn config_glyph_rows(config: &config::Config) -> Vec<String> {
    rows("config", |schema, icon| {
        let glyph =
            config.modules.get(schema.id).map_or_else(String::new, |m| m.icon(icon.key).to_owned());
        let overridden = glyph != icon.glyph.get(config.icons);
        (glyph, overridden)
    })
}

/// One row per module: `<label> <module> <field> <field> …`, each field a
/// glyph padded to two cells, `|`, and the cell count garnish uses.
///
/// `glyph_of` returns the glyph and whether it is a person's override. A
/// built-in glyph that is not a single character of one or two cells (the
/// spinner's frame string, the effort dots) has no field: it is not a
/// glyph. An override always keeps its field, so a row overriding real
/// glyphs lines up with the set row above it: one that is not a single
/// glyph shows as `?` with its cell count (capped at 9), an empty one as
/// `∅ |0`. (An override of a non-glyph icon such as the spinner string has
/// no set field to line up with; it still shows.)
fn rows(
    label: &str,
    glyph_of: impl Fn(
        &crate::config::schema::ModuleSchema,
        &crate::config::schema::IconSpec,
    ) -> (String, bool),
) -> Vec<String> {
    SCHEMAS
        .iter()
        .filter_map(|schema| {
            let fields: Vec<String> = schema
                .icons
                .iter()
                .filter_map(|icon| {
                    let (g, overridden) = glyph_of(schema, icon);
                    let cells = display_width(&g);
                    let single = g.chars().filter(|c| *c != '\u{fe0f}').count() == 1;
                    if single && (1..=2).contains(&cells) {
                        Some(format!("{g}{}|{cells}", " ".repeat(2_usize.saturating_sub(cells))))
                    } else if overridden && g.is_empty() {
                        Some("∅ |0".to_owned())
                    } else if overridden {
                        Some(format!("? |{}", cells.min(9)))
                    } else {
                        None
                    }
                })
                .collect();
            (!fields.is_empty())
                .then(|| format!("  {label:<8} {:<13} {}", schema.id, fields.join(" ")))
        })
        .collect()
}

/// A path for a report that may be pasted into a public issue: the home
/// directory (which carries the username) collapsed to `~`.
fn tilde(path: &Path) -> String {
    crate::modules::repo::tildify_path(path, crate::claude_settings::home_dir().as_deref())
}

fn git_version() -> String {
    crate::git::version().unwrap_or_else(|e| format!("not available ({e})"))
}

fn count_dirs(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |d| d.flatten().filter(|e| e.path().is_dir()).count())
}

/// Every `err` cache entry under the root, as `(scope/module, entry)`.
#[must_use]
pub fn failed_entries(root: &Path) -> Vec<(String, Entry)> {
    entries_where(root, |e| e.status == Status::Err)
}

/// Every `ok` entry carrying a failed opt-in fetch (`fetch_error`), as
/// `(scope/module, entry)`.
#[must_use]
pub fn fetch_failures(root: &Path) -> Vec<(String, Entry)> {
    entries_where(root, |e| e.status == Status::Ok && e.get("fetch_error").is_some())
}

/// Every cache entry under the root that `keep` accepts, sorted by name.
fn entries_where(root: &Path, keep: impl Fn(&Entry) -> bool) -> Vec<(String, Entry)> {
    let mut out = Vec::new();
    for kind in ["sessions", "repos"] {
        let Ok(dirs) = std::fs::read_dir(root.join(kind)) else { continue };
        for d in dirs.flatten() {
            let Ok(files) = std::fs::read_dir(d.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().is_none_or(|e| e != "cache") {
                    continue;
                }
                if let Some(entry) = crate::cache::read_entry(&p)
                    && keep(&entry)
                {
                    let name = format!(
                        "{kind}/{}/{}",
                        d.file_name().to_string_lossy(),
                        p.file_stem()
                            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
                    );
                    out.push((name, entry));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Scope;
    use std::collections::BTreeMap;

    /// Every `GARNISH_*` hook the code reads is in [`TEST_HOOKS`] (so
    /// `doctor` prints it when it is set) and in the SPEC § 9 table (so a
    /// reader can find out what it does). The source scan is the guard: a
    /// hook added with its own constant but no row here would otherwise be
    /// invisible in a bug report. The other way too: a hook SPEC names or
    /// [`TEST_HOOKS`] lists is one the code reads.
    #[test]
    fn every_garnish_hook_is_reported_and_specified() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found: Vec<String> = Vec::new();
        let mut files = vec![root.join("src")];
        while let Some(dir) = files.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    files.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                for part in source.split("\"GARNISH_").skip(1) {
                    let tail: String = part
                        .chars()
                        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                        .collect();
                    // The scan's own pattern in this file has nothing after
                    // the prefix; a real hook always does.
                    let name = format!("GARNISH_{tail}");
                    if !tail.is_empty() && !found.contains(&name) {
                        found.push(name);
                    }
                }
            }
        }
        found.sort();
        assert!(found.len() >= TEST_HOOKS.len(), "the scan found nothing: {found:?}");
        let spec = std::fs::read_to_string(root.join("SPEC.md")).unwrap();
        for hook in &found {
            assert!(TEST_HOOKS.contains(&hook.as_str()), "{hook} is not in doctor::TEST_HOOKS");
            assert!(spec.contains(&format!("`{hook}`")), "{hook} is not in SPEC § 9");
        }
        for hook in TEST_HOOKS {
            assert!(found.iter().any(|f| f == hook), "{hook} is listed but nothing reads it");
        }
        for part in spec.split("`GARNISH_").skip(1) {
            let tail: String =
                part.chars().take_while(|c| c.is_ascii_uppercase() || *c == '_').collect();
            let name = format!("GARNISH_{tail}");
            assert!(
                tail.is_empty() || found.contains(&name),
                "SPEC names {name}, nothing reads it"
            );
        }
    }

    /// `statusLine.padding` is behind most rows cut with `…` (the box is
    /// `COLUMNS − 4 − 2N` wide, SPEC § 2.1): the row names the file that
    /// sets it and, when the config's `padding` is not twice it, the value
    /// that fits.
    #[test]
    fn the_padding_row_names_the_config_value_that_fits() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join(".claude");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(user.join("settings.json"), r#"{"statusLine": {"padding": 1}}"#).unwrap();
        let chain = read_chain(&claude_settings::settings_chain(None, None, Some(&user)));
        let row_of = |config: &str| {
            let (cfg, _) = config::parse(config, &SCHEMAS);
            let rows = settings_rows(&chain, None, &cfg, true);
            rows.into_iter().find(|r| r.starts_with("  padding ")).unwrap()
        };
        let text = row_of("");
        assert!(
            text.ends_with("1 (user); set `padding = 2` in the config, or rows are cut with …")
        );
        assert!(row_of("padding = 2\n").ends_with("1 (user)"), "it fits");
        std::fs::write(user.join("settings.json"), "{}").unwrap();
        let chain = read_chain(&claude_settings::settings_chain(None, None, Some(&user)));
        let (cfg, _) = config::parse("padding = 4\n", &SCHEMAS);
        let rows = settings_rows(&chain, None, &cfg, true);
        assert!(
            rows.iter().any(|r| r.starts_with("  padding ") && r.ends_with("unset")),
            "{rows:?}"
        );
    }

    /// A project at `/` (doctor run from the root directory) strips only
    /// its own files' prefix: the user and managed files keep their whole
    /// path, collapsed to `~` when under the home, never a bare
    /// `home/<user>/…`.
    #[test]
    fn only_the_project_files_are_shown_relative_to_it() {
        let chain = vec![
            ("managed", PathBuf::from("/etc/m.json"), FileState::Absent, false),
            ("local", PathBuf::from("/.claude/settings.local.json"), FileState::Absent, false),
            (
                "user",
                PathBuf::from("/nobody-home/u/.claude/settings.json"),
                FileState::Absent,
                false,
            ),
        ];
        let (cfg, _) = config::parse("", &SCHEMAS);
        let rows = settings_rows(&chain, Some(Path::new("/")), &cfg, true).join("\n");
        assert!(rows.contains("  managed  /etc/m.json  absent"), "{rows}");
        assert!(rows.contains("  local    .claude/settings.local.json  absent"), "{rows}");
        assert!(rows.contains("  user     /nobody-home/u/.claude/settings.json  absent"), "{rows}");
    }

    #[test]
    fn failed_entries_lists_only_err_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().to_path_buf());
        cache.write(&Scope::Repo("r1".into()), "sync", &Entry::err(1, "git timed out")).unwrap();
        cache.write(&Scope::Repo("r1".into()), "branch", &Entry::ok(1, BTreeMap::new())).unwrap();
        cache.write(&Scope::Session("s1".into()), "x", &Entry::err(1, "boom")).unwrap();
        let failed = failed_entries(dir.path());
        let names: Vec<&str> = failed.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["repos/r1/sync", "sessions/s1/x"]);
        assert_eq!(failed[0].1.error, "git timed out");
    }

    /// The cache section goes to a terminal, and what it prints came from
    /// outside: a failed entry's text (a command run in a repository nobody
    /// here built, written by any version of garnish) and the debug log.
    /// Neither may carry an escape sequence or a bell through.
    #[test]
    fn the_cache_section_prints_plain_text_only() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().to_path_buf());
        let entry = dir.path().join("repos").join("0123456789abcdef").join("sync.cache");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, "v1 1 1000 err\nclear\u{1b}[2J bell\u{7} title\u{1b}]0;x\u{7}\n")
            .unwrap();
        std::fs::write(dir.path().join("debug.log"), "1 pid=2 odd\u{1b}[31m red\n").unwrap();
        let mut o = String::new();
        cache_section(&mut o, &cache);
        assert!(!o.contains('\u{1b}') && !o.contains('\u{7}'), "{o:?}");
        assert!(o.contains("FAILED repos/0123456789abcdef/sync") && o.contains("clear bell title"));
        assert!(o.contains("odd red"), "{o}");
    }

    /// A fetch that fails keeps the counts, so its entry is `ok` and the
    /// failure rode in `fetch_error` where nothing read it: `doctor` said
    /// "no failed refreshes" while an expired token failed every fetch.
    #[test]
    fn a_failed_fetch_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().to_path_buf());
        let values: BTreeMap<String, String> = [
            ("ahead", "1"),
            ("fetch_attempt", "1"),
            ("fetch_error", "fatal: Authentication failed"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let scope = Scope::Repo("0123456789abcdef".into());
        cache.write(&scope, "sync", &Entry::ok(1, values)).unwrap();
        cache.write(&scope, "branch", &Entry::ok(1, BTreeMap::new())).unwrap();
        let mut o = String::new();
        cache_section(&mut o, &cache);
        assert!(!o.contains("no failed refreshes"), "{o}");
        assert!(
            o.contains("FETCH FAILED repos/0123456789abcdef/sync (")
                && o.contains("s ago): fatal: Authentication failed"),
            "{o}"
        );
        assert_eq!(fetch_failures(dir.path()).len(), 1);
        assert_eq!(failed_entries(dir.path()).len(), 0);
    }

    /// A cache on a filesystem without hard links (exFAT, some SMB mounts)
    /// takes files and refuses every lock; the report used to call it
    /// writable while no worker could ever run there.
    #[test]
    fn the_probe_names_a_root_that_cannot_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        let no_links = |_: &Path, _: &Path| Err(std::io::Error::other("links unsupported"));
        let state = probe(&root, no_links);
        assert!(state.contains("no hard links") && state.contains("cannot lock"), "{state}");
        assert_eq!(probe(&root, |a, b| std::fs::hard_link(a, b)), "writable");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "the probes are removed");
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, "").unwrap();
        assert_eq!(probe(&blocked.join("cache"), |a, b| std::fs::hard_link(a, b)), "NOT writable");
    }

    /// The probe writes a file at a name anyone who can write the root can
    /// predict, so it goes the way every temporary file in the cache goes:
    /// unlinked first and created exclusively, never through a link. It
    /// used `fs::write`, which followed a planted link and truncated its
    /// target; and a root it had to create came out with the umask's mode,
    /// where the cache's own are `0700` (review 2026-09-25).
    #[test]
    fn the_probe_never_follows_a_planted_link() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        std::fs::create_dir_all(&root).unwrap();
        let precious = dir.path().join("precious");
        std::fs::write(&precious, "keep me").unwrap();
        let name = format!(".probe.{}", std::process::id());
        std::os::unix::fs::symlink(&precious, root.join(&name)).unwrap();
        let state = probe(&root, |a, b| std::fs::hard_link(a, b));
        assert_eq!(state, "writable");
        assert_eq!(std::fs::read_to_string(&precious).unwrap(), "keep me", "through the link");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "the probes are removed");
        let fresh = dir.path().join("new").join("cache");
        assert_eq!(probe(&fresh, |a, b| std::fs::hard_link(a, b)), "writable");
        for made in [dir.path().join("new"), fresh] {
            let mode = std::fs::metadata(&made).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", made.display());
        }
    }

    #[test]
    fn glyph_grid_fields_are_four_cells_and_config_rows_follow_the_config() {
        for set in IconSet::ALL {
            for row in glyph_rows(set) {
                // `  <label:8> <module:13> ` is 25 ASCII bytes.
                let (_, fields) = row.split_at(25);
                // Fields are four cells plus one space apart, so every `|`
                // lands on cell 2 of a five-cell period: the column property
                // the grid promises.
                // (The ASCII marker glyph is `|` itself, so check the cells,
                // not the characters.)
                let mut col = 0_usize;
                let mut bars = 0_usize;
                for c in fields.chars() {
                    if col % 5 == 2 {
                        assert_eq!(c, '|', "{row:?}: cell {col}");
                        bars += 1;
                    }
                    col += crate::ansi::char_width(c);
                }
                assert!(bars > 0, "{row:?}: no fields");
                assert_eq!(col, bars * 5 - 1, "{row:?}: total width");
            }
        }
        let (default, _) = config::parse("", &SCHEMAS);
        let expected: Vec<String> = glyph_rows(IconSet::Nerd)
            .iter()
            .map(|r| r.replacen("nerd    ", "config  ", 1))
            .collect();
        assert_eq!(config_glyph_rows(&default), expected);
        let (custom, _) = config::parse(
            "icons = \"unicode\"\n[modules.branch.icons]\nbranch = \"B\"\n",
            &SCHEMAS,
        );
        let rows = config_glyph_rows(&custom);
        let branch = rows.iter().find(|r| r.contains(" branch ")).unwrap();
        assert!(branch.contains("B |1"), "override shows in the config row: {branch}");
        assert!(branch.contains("✱ |1"), "the rest follows the unicode set: {branch}");
        // An override that is not one glyph keeps its field (`?` and the
        // cell count), an empty one shows `∅ |0`, so the config row has as
        // many fields as the set row above it (whole-stack review).
        let (odd, _) = config::parse(
            "[modules.model.icons]\nmodel = \"ab\"\n[modules.branch.icons]\nbranch = \"\"\n",
            &SCHEMAS,
        );
        let rows = config_glyph_rows(&odd);
        let model = rows.iter().find(|r| r.contains(" model ")).unwrap();
        assert!(model.contains("? |2"), "{model}");
        let branch = rows.iter().find(|r| r.contains(" branch ")).unwrap();
        assert!(branch.contains("∅ |0"), "{branch}");
        let set_fields = |rows: &[String], id: &str| {
            rows.iter()
                .find(|r| r.contains(&format!(" {id} ")))
                .unwrap()
                .split_at(25)
                .1
                .split(' ')
                .count()
        };
        assert_eq!(set_fields(&rows, "model"), set_fields(&glyph_rows(IconSet::Nerd), "model"));
        assert_eq!(set_fields(&rows, "branch"), set_fields(&glyph_rows(IconSet::Nerd), "branch"));
    }

    #[test]
    fn report_paths_collapse_the_home_directory() {
        // Doctor output is pasted into public issues by the feedback skill.
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return;
        }
        assert_eq!(
            tilde(Path::new(&format!("{home}/.claude/settings.json"))),
            "~/.claude/settings.json"
        );
        assert_eq!(tilde(Path::new(&home)), "~");
        assert_eq!(tilde(Path::new("/usr/bin/garnish")), "/usr/bin/garnish");
        assert_eq!(
            tilde(Path::new(&format!("{home}2/x"))),
            format!("{home}2/x"),
            "prefix, not string"
        );
    }

    /// SPEC § 7: the settings rows list every file of the chain with
    /// whether it parses, resolve each key by Claude Code's precedence
    /// (the first file that sets it), and suggest `refreshInterval = 1`
    /// and `hideVimModeIndicator = true` only when the config calls for
    /// them.
    // One settings fixture, many rows to check: splitting the test would
    // repeat the fixture's setup in every half.
    #[allow(clippy::too_many_lines)]
    #[test]
    fn settings_rows_follow_the_chain_and_suggest_from_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        let user_dir = home.join(".claude");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        let user = user_dir.join("settings.json");
        let project = proj.join(".claude/settings.json");
        let local = proj.join(".claude/settings.local.json");
        std::fs::write(
            &user,
            r#"{"statusLine": {"type": "command", "command": "garnish", "refreshInterval": 5}, "prefersReducedMotion": true, "tui": "fullscreen"}"#,
        )
        .unwrap();
        std::fs::write(
            &project,
            r#"{"statusLine": {"hideVimModeIndicator": false}, "disableAllHooks": true}"#,
        )
        .unwrap();
        std::fs::write(&local, "{ broken").unwrap();
        // No managed file: the test must not see the machine's.
        let chain = |p: Option<&Path>, h: Option<&Path>| {
            let user = claude_settings::user_dir_in(None, h);
            read_chain(&claude_settings::settings_chain(None, p, user.as_deref()))
        };
        let read = chain(Some(&proj), Some(&home));
        let labels: Vec<&str> = read.iter().map(|(l, ..)| *l).collect();
        assert_eq!(labels, ["local", "project", "user"]);
        assert!(matches!(read[0].2, FileState::Invalid(_)), "{:?}", read[0]);
        let (cfg, errs) = config::parse("[[line]]\nmodules = [\"vim\", \"clock\"]\n", &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        // A row is its key in the first column and the text after it.
        let row = |rows: &[String], key: &str, text: &str| {
            rows.iter().any(|r| {
                r.trim_start().starts_with(key)
                    && r.get(24..).is_some_and(|value| value.starts_with(text))
            })
        };
        let rows = settings_rows(&read, Some(&proj), &cfg, true);
        let text = rows.join("\n");
        assert!(text.starts_with("claude settings") && text.contains("proj; the first"), "{text}");
        // The project's files are shown relative to the project directory.
        assert!(text.contains("  local    .claude/settings.local.json  not valid JSON"), "{text}");
        assert!(text.contains("  project  .claude/settings.json  ok"), "{text}");
        assert!(row(&rows, "statusLine", "command=garnish (user)"), "{text}");
        assert!(row(&rows, "refreshInterval", "5 (user); set 1 so the clock"), "{text}");
        assert!(row(&rows, "hideVimModeIndicator", "false (project); set true"), "{text}");
        assert!(
            row(&rows, "disableAllHooks", "true (project): Claude Code does not run"),
            "{text}"
        );
        assert!(row(&rows, "prefersReducedMotion", "true (user): animations are frozen"), "{text}");
        assert!(
            row(&rows, "tui", "fullscreen (user): asks for the alternate-screen renderer"),
            "{text}"
        );
        // The key column is one width, so the values line up.
        assert!(rows.iter().skip(4).all(|r| r.get(23..24) == Some(" ")), "{text}");
        // A key set in two files: the higher file wins, key by key.
        std::fs::write(
            &local,
            r#"{"statusLine": {"command": "/opt/garnish", "refreshInterval": 2}, "prefersReducedMotion": false, "tui": "default"}"#,
        )
        .unwrap();
        let rows = settings_rows(&chain(Some(&proj), Some(&home)), Some(&proj), &cfg, true);
        let text = rows.join("\n");
        assert!(row(&rows, "statusLine", "command=/opt/garnish (local)"), "{text}");
        assert!(row(&rows, "refreshInterval", "2 (local); set 1"), "{text}");
        assert!(row(&rows, "hideVimModeIndicator", "false (project)"), "{text}");
        assert!(row(&rows, "prefersReducedMotion", "false (local)"), "{text}");
        assert!(row(&rows, "tui", "default (local): asks for the classic renderer"), "{text}");
        // A value that is neither name never decides: the next file that
        // sets the key does (here the user's `fullscreen`), and the row
        // says the file Claude Code rejects for it, the value quoted and
        // reduced to plain text, or cut, so a blank or a huge one is seen.
        let tui_row = |local_json: &str| {
            std::fs::write(&local, local_json).unwrap();
            let rows = settings_rows(&chain(Some(&proj), Some(&home)), Some(&proj), &cfg, true);
            rows.iter().find(|r| r.starts_with("tui ")).cloned().unwrap()
        };
        let text = tui_row("{\"tui\": \"x\\u001b[31my\"}");
        assert!(text.contains("fullscreen (user): asks"), "{text}");
        assert!(
            text.contains(
                "; \"xy\" (local) is not `default` or `fullscreen`, so Claude Code rejects that file and reads none of its keys"
            ),
            "{text}"
        );
        assert!(tui_row("{\"tui\": \"\"}").contains("; \"\" (local) is not"), "empty, quoted");
        assert!(tui_row("{\"tui\": 1}").contains("; 1 (local) is not"), "a number as JSON");
        let long = tui_row(&format!("{{\"tui\": \"{}\"}}", "x".repeat(300)));
        let cut = format!("; \"{}…\" (local) is not", "x".repeat(MAX_VALUE_CHARS));
        assert!(long.contains(&cut) && !long.contains(&"x".repeat(201)), "{long}");
        // The managed file's stray value is dropped on its own and the user
        // file decides; with nothing else setting the key, unset.
        let managed = dir.path().join("managed.json");
        std::fs::write(&managed, r#"{"tui": "FULL"}"#).unwrap();
        std::fs::write(&local, "{}").unwrap();
        std::fs::write(&user, r#"{"tui": "default"}"#).unwrap();
        let with_managed = read_chain(&claude_settings::settings_chain(
            Some(&managed),
            Some(&proj),
            Some(&user_dir),
        ));
        let rows = settings_rows(&with_managed, Some(&proj), &cfg, true);
        let text = rows.iter().find(|r| r.starts_with("tui ")).unwrap();
        assert!(text.contains("default (user): asks for the classic"), "{text}");
        assert!(
            text.contains("; \"FULL\" (managed) is not `default` or `fullscreen`, so Claude Code ignores it there"),
            "{text}"
        );
        let rows = settings_rows(&chain(Some(&proj), None), Some(&proj), &cfg, true);
        assert!(row(&rows, "tui", "unset: Claude Code picks"), "{}", rows.join("\n"));
        std::fs::write(
            &user,
            r#"{"statusLine": {"type": "command", "command": "garnish", "refreshInterval": 5}, "prefersReducedMotion": true, "tui": "fullscreen"}"#,
        )
        .unwrap();
        std::fs::write(&local, "{ broken").unwrap();
        // No ticking module and no vim: no suggestion; an explicit `animate`
        // changes the reduced-motion note.
        let (quiet, _) =
            config::parse("animate = true\n[[line]]\nmodules = [\"model\"]\n", &SCHEMAS);
        let text = settings_rows(&read, Some(&proj), &quiet, true).join("\n");
        assert!(!text.contains("set 1") && !text.contains("set true"), "{text}");
        assert!(text.contains("overridden by `animate = true`"), "{text}");
        // An animation wants the one-second tick only while it runs: not
        // under the reduced-motion setting (`read` says true), not under
        // `animate = false`, not under the session switch.
        let dots = "[frame]\nfill_pattern = \"·  \"\n[[line]]\nmodules = [\"model\"]\n";
        let suggests = |text: &str, session: bool| {
            let (c, errs) = config::parse(text, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            settings_rows(&read, Some(&proj), &c, session).join("\n").contains("set 1")
        };
        assert!(!suggests(dots, true), "reduced motion freezes the dots");
        assert!(suggests(&format!("animate = true\n{dots}"), true));
        assert!(!suggests(&format!("animate = true\n{dots}"), false), "GARNISH_ANIMATE=0");
        assert!(!suggests(&format!("animate = false\n{dots}"), true));
        // A scrolling text module counts only when its text is wider than its
        // box; a limit's countdown only with `show_reset`.
        let scroll = |extra: &str| {
            suggests(
                &format!(
                    "animate = true\n[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\ntext = \"hello there\"\n{extra}"
                ),
                true,
            )
        };
        assert!(scroll("width = 4\n"));
        assert!(!scroll("width = 4\noverflow = \"clip\"\n"));
        assert!(!scroll("width = 20\n"), "fits its box");
        assert!(!scroll("width = 0\n"), "a box as wide as the text");
        assert!(suggests("[[line]]\nmodules = [\"limit5h\"]\n", true), "the default countdown");
        let limit = |options: &str| {
            suggests(
                &format!("[[line]]\nmodules = [\"limit5h\"]\n[modules.limit5h]\n{options}"),
                true,
            )
        };
        assert!(!limit("show_reset = false\n"));
        assert!(!limit("reset = \"absolute\"\n"), "a wall-clock time does not tick");
        assert!(limit("reset = \"elapsed\"\n"), "the elapsed time does");
        assert!(limit("show_reset = false\neta = true\n"), "the eta counts down");
        // A value below 1 is dropped by Claude Code and says so; 1 fits;
        // `false` is a value.
        std::fs::write(&user, r#"{"statusLine": {"refreshInterval": 0.5}}"#).unwrap();
        std::fs::remove_file(&project).unwrap();
        std::fs::remove_file(&local).unwrap();
        let rows = settings_rows(&chain(Some(&proj), Some(&home)), Some(&proj), &cfg, true);
        assert!(
            row(&rows, "refreshInterval", "0.5 (user), below 1 so Claude Code ignores it; set 1"),
            "{}",
            rows.join("\n")
        );
        std::fs::write(&user, r#"{"statusLine": {"refreshInterval": 1, "hideVimModeIndicator": true}, "disableAllHooks": false, "prefersReducedMotion": false}"#).unwrap();
        let rows = settings_rows(&chain(Some(&proj), Some(&home)), Some(&proj), &cfg, true);
        let text = rows.join("\n");
        for (key, value) in [
            ("refreshInterval", "1 (user)"),
            ("hideVimModeIndicator", "true (user)"),
            ("disableAllHooks", "false (user)"),
            ("prefersReducedMotion", "false (user)"),
        ] {
            let exact = rows.iter().any(|r| r.trim_start().starts_with(key) && r.ends_with(value));
            assert!(exact, "{key}: {text}");
        }
        assert!(row(&rows, "statusLine", "not configured"), "{text}");
        // A command from a file nobody controls reaches the row as plain
        // text, cut to a line's worth.
        std::fs::write(
            &user,
            serde_json::json!({"statusLine": {"command": format!("garnish\u{1b}[2J\n{}", "x".repeat(300))}})
                .to_string(),
        )
        .unwrap();
        let rows = settings_rows(&chain(Some(&proj), Some(&home)), Some(&proj), &cfg, true);
        let command = rows
            .iter()
            .find(|r| r.contains("command="))
            .unwrap_or_else(|| panic!("no command row:\n{}", rows.join("\n")));
        assert!(!command.contains('\u{1b}') && !command.contains('\n'), "{command:?}");
        assert!(command.contains("garnishxxx") && command.contains("… (user)"), "{command}");
        assert!(command.chars().count() < 260, "{command}");
        // Without a home the user file is unknown, without a project the
        // heading says so, and a managed file is listed first.
        let managed = dir.path().join("managed.json");
        std::fs::write(&managed, "{}").unwrap();
        let rows = settings_rows(
            &read_chain(&claude_settings::settings_chain(Some(&managed), None, None)),
            None,
            &cfg,
            true,
        );
        let text = rows.join("\n");
        assert!(text.contains("(no project directory;"), "{text}");
        assert!(text.contains("  managed  ") && text.contains("HOME is not set"), "{text}");
        assert!(row(&rows, "refreshInterval", "unset; set 1"), "{text}");
    }

    #[test]
    fn report_mentions_every_section() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("cache"));
        std::fs::create_dir_all(cache.root()).unwrap();
        std::fs::write(cache.root().join("debug.log"), "1 pid=1 spawn sync failed: x\n").unwrap();
        let user = dir.path().join(".claude");
        let none = config::ReadTarget::File(dir.path().join("none.toml"));
        let r = report_with(&none, &cache, None, None, Some(&user));
        assert!(r.contains("  user     ") && r.contains("  absent"), "{r}");
        assert!(r.contains("not configured (run `garnish install`)"), "{r}");
        assert!(r.contains("debug.log (last 1 of 1 lines)"), "{r}");
        assert!(r.contains("(writable)"), "{r}");
        // A file that cannot be read is not a file with one bad key.
        assert!(r.contains("none.toml cannot be read; the built-in defaults are in effect"), "{r}");
        assert!(!r.contains("problem(s)"), "{r}");
        for needle in [
            "garnish ",
            "claude settings",
            "config",
            "cache",
            "environment",
            "glyph test",
            "nerd",
            "ascii",
        ] {
            assert!(r.contains(needle), "{needle}\n{r}");
        }
    }

    /// SPEC § 7: the `sandbox.enabled` and `voice.enabled` rows resolve
    /// as the other keys do (the first file that sets one wins) and say
    /// what the badge module of that name makes of the value, once the
    /// config places it.
    #[test]
    fn switch_rows_name_the_badge_modules() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        let user = home.join(".claude/settings.json");
        std::fs::write(&user, r#"{"sandbox": {"enabled": true}, "voice": {"enabled": true}}"#)
            .unwrap();
        std::fs::write(proj.join(".claude/settings.json"), r#"{"voice": {"enabled": false}}"#)
            .unwrap();
        // No managed file: the test must not see the machine's.
        let rows_for = |config: &str| {
            let user = home.join(".claude");
            let chain =
                read_chain(&claude_settings::settings_chain(None, Some(&proj), Some(&user)));
            let (cfg, errs) = config::parse(config, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            settings_rows(&chain, Some(&proj), &cfg, true)
        };
        let find = |rows: &[String], key: &str| {
            rows.iter()
                .find(|r| r.starts_with(key))
                .cloned()
                .unwrap_or_else(|| panic!("no {key} row:\n{}", rows.join("\n")))
        };
        let rows = rows_for("[[line]]\nmodules = [\"model\"]\n");
        assert!(find(&rows, "sandbox.enabled").ends_with("true (user)"), "{rows:?}");
        assert!(find(&rows, "voice.enabled").ends_with("false (project)"), "{rows:?}");
        let rows = rows_for("[[line]]\nmodules = [\"sandbox\", \"voice\"]\n");
        assert!(
            find(&rows, "sandbox.enabled").ends_with("true (user); the sandbox module shows it"),
            "{rows:?}"
        );
        assert!(
            find(&rows, "voice.enabled")
                .ends_with("false (project); nothing for the voice module to show"),
            "{rows:?}"
        );
        // A placed module whose key no file sets, and one the config turned off.
        std::fs::remove_file(&user).unwrap();
        let rows = rows_for(
            "[[line]]\nmodules = [\"sandbox\", \"voice\"]\n[modules.voice]\nenabled = false\n",
        );
        assert!(
            find(&rows, "sandbox.enabled")
                .ends_with("unset; nothing for the sandbox module to show"),
            "{rows:?}"
        );
        assert!(find(&rows, "voice.enabled").ends_with("false (project)"), "{rows:?}");
        // The key column is one width, so the values line up.
        assert!(rows.iter().skip(4).all(|r| r.get(23..24) == Some(" ")), "{rows:?}");
    }

    /// A file whose `tui` is neither name is one Claude Code rejects whole,
    /// so its row is not `ok` and none of its keys resolve: the report said
    /// "rejects that file" in the `tui` row while the file's own row said
    /// `ok` and `prefersReducedMotion` came from it.
    #[test]
    fn a_rejected_file_is_neither_ok_nor_read() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        std::fs::write(
            proj.join(".claude/settings.local.json"),
            r#"{"tui": "full", "prefersReducedMotion": true}"#,
        )
        .unwrap();
        std::fs::write(home.join(".claude/settings.json"), r#"{"prefersReducedMotion": false}"#)
            .unwrap();
        let user = home.join(".claude");
        let chain = read_chain(&claude_settings::settings_chain(None, Some(&proj), Some(&user)));
        let (cfg, _) = config::parse("", &SCHEMAS);
        let rows = settings_rows(&chain, Some(&proj), &cfg, true);
        let text = rows.join("\n");
        let local = rows.iter().find(|r| r.starts_with("  local ")).unwrap();
        assert!(!local.ends_with(" ok") && local.contains("rejects this file"), "{text}");
        let motion = rows.iter().find(|r| r.starts_with("prefersReducedMotion")).unwrap();
        assert!(motion.ends_with("false (user)"), "{text}");
    }

    /// Follow-up review of 2026-09-25: a settings file past the tick's
    /// read cap is still one Claude Code reads, so its status line command
    /// and its other Claude Code keys count (the report said "not
    /// configured" while Claude Code ran the command), and only the keys a
    /// tick reads skip it.
    #[test]
    fn a_settings_file_past_the_tick_cap_counts_for_claude_code_only() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("proj");
        let user = dir.path().join("home/.claude");
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        let cap = usize::try_from(claude_settings::MAX_SETTINGS_BYTES).unwrap();
        let big = serde_json::json!({"filler": "x".repeat(cap), "prefersReducedMotion": true,
            "sandbox": {"enabled": true},
            "statusLine": {"type": "command", "command": "garnish", "padding": 1}});
        std::fs::write(proj.join(".claude/settings.local.json"), big.to_string()).unwrap();
        // The user file is exactly at the cap, which a tick still reads.
        let small = r#"{"prefersReducedMotion": false, "sandbox": {"enabled": false}, "#;
        let at_cap = format!("{small}\"f\": \"{}\"}}", "x".repeat(cap - small.len() - 8));
        assert_eq!(at_cap.len(), cap);
        std::fs::write(user.join("settings.json"), at_cap).unwrap();
        let chain = read_chain(&claude_settings::settings_chain(None, Some(&proj), Some(&user)));
        let (cfg, _) = config::parse("padding = 2\n", &SCHEMAS);
        let rows = settings_rows(&chain, Some(&proj), &cfg, true);
        let text = rows.join("\n");
        let row = |key: &str| rows.iter().find(|r| r.starts_with(key)).unwrap();
        assert!(row("  local ").contains("ok, but longer than"), "{text}");
        assert!(row("  user ").ends_with(" ok"), "{text}");
        assert!(row("statusLine").ends_with("command=garnish (local)"), "{text}");
        assert!(row("  padding").ends_with("1 (local)"), "{text}");
        assert!(row("prefersReducedMotion").ends_with("false (user)"), "{text}");
        assert!(row("sandbox.enabled").ends_with("false (user)"), "{text}");
    }
}
