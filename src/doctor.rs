//! `garnish doctor`: a diagnostic report for "why does my status line look like that".

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::ansi::display_width;
use crate::cache::{Cache, Entry, Status};
use crate::claude_settings::{self, FileKeys, FileState};
use crate::config::{self, Config};
use crate::icons::IconSet;
use crate::modules::SCHEMAS;

/// Build the report from the process environment: the settings chain of
/// the current directory (Claude Code's project directory when `doctor`
/// runs where the session was started) and the home.
#[must_use]
pub fn report(config_path: Option<&Path>) -> String {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    report_with(
        config_path,
        &Cache::from_env(),
        std::env::current_dir().ok().as_deref(),
        home.as_deref(),
    )
}

/// Build the report against explicit cache, project and home locations
/// (`home` is `None` when there is no home directory to look in).
#[must_use]
pub fn report_with(
    config_path: Option<&Path>,
    cache: &Cache,
    project: Option<&Path>,
    home: Option<&Path>,
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
    let loaded = config::load(config_path, &SCHEMAS);
    for row in settings_rows(&settings_chain(project, home), &loaded.config) {
        let _ = writeln!(o, "{row}");
    }
    let _ = writeln!(o);
    config_section(&mut o, &loaded);
    cache_section(&mut o, cache);
    environment_section(&mut o);
    glyph_section(&mut o, &loaded.config);
    o
}

/// One file of the settings chain as `doctor` reads it: its label
/// (`managed`, `local`, `project`, `user`), its path and what it holds.
pub type ChainEntry = (&'static str, PathBuf, FileState);

/// The settings chain of a project directory and a home, read.
#[must_use]
pub fn settings_chain(project: Option<&Path>, home: Option<&Path>) -> Vec<ChainEntry> {
    claude_settings::settings_chain(project, home)
        .into_iter()
        .map(|(label, path)| {
            let state = claude_settings::read_file(&path);
            (label, path, state)
        })
        .collect()
}

/// The `claude settings` rows of the report (SPEC § 7).
///
/// One row per file of the chain, saying whether it is there and parses,
/// then the keys that change what the line can show, each with the file it
/// comes from and, where the config calls for another value, the
/// suggestion.
#[must_use]
pub fn settings_rows(chain: &[ChainEntry], config: &Config) -> Vec<String> {
    let row = |key: &str, text: &str| format!("{key:<24}{text}");
    let mut rows = vec![row("claude settings", "(the first file that sets a key wins)")];
    for (label, path, state) in chain {
        let status = match state {
            FileState::Absent => "absent".to_owned(),
            FileState::Unreadable(e) => format!("unreadable: {e}"),
            FileState::Invalid(e) => format!("{e}; garnish reads none of it"),
            FileState::Keys(_) => "ok".to_owned(),
        };
        rows.push(format!("  {label:<8} {}  {status}", tilde(path)));
    }
    if !chain.iter().any(|(label, _, _)| *label == "user") {
        rows.push("  user     unknown: HOME is not set".to_owned());
    }
    match resolved(chain, |k| k.status_line_command.clone()) {
        Some((command, from)) => {
            rows.push(row(
                "statusLine",
                &format!("command={} ({from})", tilde(Path::new(&command))),
            ));
        }
        None => rows.push(row("statusLine", "not configured (run `garnish install`)")),
    }
    let interval = resolved(chain, |k| k.refresh_interval);
    let mut text = interval
        .as_ref()
        .map_or_else(|| "unset".to_owned(), |(secs, from)| format!("{secs} ({from})"));
    if ticks_every_second(config) && interval.as_ref().is_none_or(|(secs, _)| *secs > 1.0) {
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
            "; set true: the vim module shows the mode, so Claude Code's own indicator shows it twice",
        );
    }
    rows.push(row("  hideVimModeIndicator", &text));
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
    let reduced = resolved(chain, |k| k.reduced_motion);
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
    rows
}

/// The first file of the chain that sets a key, with the file's label:
/// Claude Code's own precedence for one key (managed > local > project >
/// user).
fn resolved<T>(
    chain: &[ChainEntry],
    pick: impl Fn(&FileKeys) -> Option<T>,
) -> Option<(T, &'static str)> {
    chain.iter().find_map(|(label, _, state)| match state {
        FileState::Keys(keys) => pick(keys).map(|value| (value, *label)),
        _ => None,
    })
}

/// Whether a module id is on a line and enabled.
fn placed(config: &Config, id: &str) -> bool {
    let enabled = id.strip_prefix(crate::modules::text::PREFIX).map_or_else(
        || config.modules.get(id).is_some_and(|m| m.enabled),
        |name| config.texts.get(name).is_some_and(|m| m.enabled),
    );
    enabled && config.lines.iter().any(|l| l.left.iter().chain(&l.right).any(|m| m == id))
}

/// Whether the config shows something that changes every second, which is
/// what `statusLine.refreshInterval = 1` is for: a module whose value ticks
/// (the clock, the elapsed times, the countdowns) or an animation (SPEC
/// § 4.2: the ticker, a rule pattern, separator or icon frames, a scrolling
/// text module).
fn ticks_every_second(config: &Config) -> bool {
    const TICKING: [&str; 7] = ["clock", "session", "api", "cache", "limit5h", "limit7d", "spend"];
    TICKING.iter().any(|id| placed(config, id))
        || config.overflow == config::Overflow::Ticker
        || !config.frame.fill_pattern.is_empty()
        || !config.frame.separator_frames.is_empty()
        || config
            .modules
            .iter()
            .any(|(id, m)| placed(config, id) && !m.all_icon_frames().is_empty())
        || config.texts.iter().any(|(name, m)| {
            placed(config, &format!("{}{name}", crate::modules::text::PREFIX))
                && m.str("overflow") != "clip"
        })
}

fn config_section(o: &mut String, loaded: &config::Loaded) {
    match (&loaded.path, loaded.errors.is_empty()) {
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
            // A syntax error is the one problem with a line and no path.
            let syntax = loaded.errors.iter().any(|e| e.path.is_empty() && e.line.is_some());
            if syntax {
                let _ = writeln!(
                    o,
                    "config   {} does not parse; the built-in defaults are in effect",
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
        "preset={} icons={} theme={} frame={} lines={}",
        c.preset.name(),
        c.icons.name(),
        c.theme_name,
        c.frame.style.name(),
        c.lines.len()
    );
    let _ = writeln!(o);
}

fn cache_section(o: &mut String, cache: &Cache) {
    let root = cache.root();
    let probe = root.join(format!(".probe.{}", std::process::id()));
    let writable = std::fs::create_dir_all(root).is_ok() && std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    let _ = writeln!(
        o,
        "cache    {} ({})",
        tilde(root),
        if writable { "writable" } else { "NOT writable" }
    );
    let sessions = count_dirs(&root.join("sessions"));
    let repos = count_dirs(&root.join("repos"));
    let _ = writeln!(o, "         {sessions} session dir(s), {repos} repo dir(s)");
    let failures = failed_entries(root);
    if failures.is_empty() {
        let _ = writeln!(o, "         no failed refreshes");
    } else {
        for (path, entry) in failures {
            let _ = writeln!(
                o,
                "         FAILED {} ({}s ago): {}",
                path,
                entry.age_ms() / 1000,
                entry.error
            );
        }
    }
    if let Ok(log) = std::fs::read_to_string(root.join("debug.log")) {
        let lines: Vec<&str> = log.lines().collect();
        let tail = lines.iter().rev().take(10).rev();
        let _ = writeln!(
            o,
            "         debug.log (last {} of {} lines):",
            tail.len().min(10),
            lines.len()
        );
        for line in tail {
            let _ = writeln!(o, "           {line}");
        }
    }
    let _ = writeln!(o);
}

fn environment_section(o: &mut String) {
    let _ = writeln!(o, "environment");
    for key in [
        "COLUMNS",
        "LINES",
        "NO_COLOR",
        "TZ",
        "GARNISH_CONFIG",
        "GARNISH_CACHE_DIR",
        "GARNISH_NOW",
        "GARNISH_NO_SPAWN",
        "GARNISH_COLUMNS",
        "GARNISH_DEBUG",
        "GARNISH_ANIMATE",
        "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
        "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE",
        "DISABLE_AUTO_COMPACT",
        "DISABLE_COMPACT",
    ] {
        if let Ok(v) = std::env::var(key) {
            // The two path-valued hooks may carry the home directory.
            let v = if key.ends_with("_CONFIG") || key.ends_with("_DIR") {
                tilde(Path::new(&v))
            } else {
                v
            };
            let _ = writeln!(o, "         {key}={v}");
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
    let home = std::env::var("HOME").ok();
    crate::modules::repo::tildify(&path.display().to_string(), home.as_deref())
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
                if let Some(entry) = std::fs::read_to_string(&p).ok().and_then(|t| Entry::parse(&t))
                    && entry.status == Status::Err
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
    #[test]
    fn settings_rows_follow_the_chain_and_suggest_from_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        let user = home.join(".claude/settings.json");
        let project = proj.join(".claude/settings.json");
        let local = proj.join(".claude/settings.local.json");
        std::fs::write(
            &user,
            r#"{"statusLine": {"type": "command", "command": "garnish", "refreshInterval": 5}, "prefersReducedMotion": true}"#,
        )
        .unwrap();
        std::fs::write(
            &project,
            r#"{"statusLine": {"hideVimModeIndicator": false}, "disableAllHooks": true}"#,
        )
        .unwrap();
        std::fs::write(&local, "{ broken").unwrap();
        let chain = settings_chain(Some(&proj), Some(&home));
        let labels: Vec<&str> = chain.iter().map(|(l, _, _)| *l).collect();
        assert_eq!(labels, ["managed", "local", "project", "user"]);
        assert!(matches!(chain[1].2, FileState::Invalid(_)), "{:?}", chain[1]);
        let (cfg, errs) = config::parse("[[line]]\nmodules = [\"vim\", \"clock\"]\n", &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        // A row is its key in the first column and the text after it.
        let row = |rows: &[String], key: &str, text: &str| {
            rows.iter().any(|r| {
                r.trim_start().starts_with(key)
                    && r.get(24..).is_some_and(|value| value.starts_with(text))
            })
        };
        let rows = settings_rows(&chain, &cfg);
        let text = rows.join("\n");
        assert!(text.starts_with("claude settings"), "{text}");
        assert!(text.contains("  local    ") && text.contains("not valid JSON"), "{text}");
        assert!(text.contains("  project  ") && text.contains("  ok"), "{text}");
        assert!(row(&rows, "statusLine", "command=garnish (user)"), "{text}");
        assert!(row(&rows, "refreshInterval", "5 (user); set 1 so the clock"), "{text}");
        assert!(row(&rows, "hideVimModeIndicator", "false (project); set true"), "{text}");
        assert!(
            row(&rows, "disableAllHooks", "true (project): Claude Code does not run"),
            "{text}"
        );
        assert!(row(&rows, "prefersReducedMotion", "true (user): animations are frozen"), "{text}");
        // The key column is one width, so the values line up.
        assert!(rows.iter().skip(5).all(|r| r.get(23..24) == Some(" ")), "{text}");
        // No ticking module and no vim: no suggestion; an explicit `animate`
        // changes the reduced-motion note.
        let (quiet, _) =
            config::parse("animate = true\n[[line]]\nmodules = [\"model\"]\n", &SCHEMAS);
        let text = settings_rows(&chain, &quiet).join("\n");
        assert!(!text.contains("set 1") && !text.contains("set true"), "{text}");
        assert!(text.contains("overridden by `animate = true`"), "{text}");
        // An animation alone wants the one-second tick; a scrolling text
        // module counts, a clipped one does not.
        let (dots, _) = config::parse(
            "[frame]\nfill_pattern = \"·  \"\n[[line]]\nmodules = [\"model\"]\n",
            &SCHEMAS,
        );
        assert!(settings_rows(&chain, &dots).join("\n").contains("set 1"));
        let (scroll, _) = config::parse(
            "[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\ntext = \"hi\"\nwidth = 1\n",
            &SCHEMAS,
        );
        assert!(settings_rows(&chain, &scroll).join("\n").contains("set 1"));
        let (clip, _) = config::parse(
            "[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\ntext = \"hi\"\noverflow = \"clip\"\n",
            &SCHEMAS,
        );
        assert!(!settings_rows(&chain, &clip).join("\n").contains("set 1"));
        // Values that already fit get no suggestion, and `false` is a value.
        std::fs::write(&user, r#"{"statusLine": {"refreshInterval": 1, "hideVimModeIndicator": true}, "disableAllHooks": false, "prefersReducedMotion": false}"#).unwrap();
        std::fs::remove_file(&project).unwrap();
        std::fs::remove_file(&local).unwrap();
        let rows = settings_rows(&settings_chain(Some(&proj), Some(&home)), &cfg);
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
        // Without a home the user file is unknown and the managed file is
        // still listed first.
        let rows = settings_rows(&settings_chain(None, None), &cfg);
        let text = rows.join("\n");
        assert!(text.contains("  managed  ") && text.contains("HOME is not set"), "{text}");
        assert!(row(&rows, "refreshInterval", "unset; set 1"), "{text}");
    }

    #[test]
    fn report_mentions_every_section() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("cache"));
        std::fs::create_dir_all(cache.root()).unwrap();
        std::fs::write(cache.root().join("debug.log"), "1 pid=1 spawn sync failed: x\n").unwrap();
        let r = report_with(Some(&dir.path().join("none.toml")), &cache, None, Some(dir.path()));
        assert!(r.contains("  user     ") && r.contains("  absent"), "{r}");
        assert!(r.contains("not configured (run `garnish install`)"), "{r}");
        assert!(r.contains("debug.log (last 1 of 1 lines)"), "{r}");
        assert!(r.contains("(writable)"), "{r}");
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
}
