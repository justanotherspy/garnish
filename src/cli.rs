//! Command-line interface.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result, eyre};

use crate::config::{self, ColorChoice, Overlay, presets::TopPreset};
use crate::icons::IconSet;
use crate::install::Refusal;
use crate::modules::SCHEMAS;
use crate::render::{self, Request};

/// garnish — a fast, cached status line for Claude Code.
#[derive(Debug, Parser)]
#[command(name = "garnish", version, about, long_about = None)]
pub struct Cli {
    /// Path to the config file (overrides `GARNISH_CONFIG` and the default location).
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Subcommand; with none, garnish renders the status line from stdin.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Rendering overrides shared by `preview`.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct RenderArgs {
    /// Top-level preset override.
    #[arg(long, value_name = "default|minimal|full|compact")]
    pub preset: Option<String>,
    /// Icon set override.
    #[arg(long, value_name = "nerd|unicode|emoji|ascii")]
    pub icons: Option<String>,
    /// Theme override.
    #[arg(long, value_name = "NAME")]
    pub theme: Option<String>,
    /// Color mode override.
    #[arg(long, value_name = "auto|always|never|256|truecolor")]
    pub color: Option<String>,
    /// Terminal width to lay out for (defaults to `COLUMNS`, then
    /// `GARNISH_COLUMNS`, then 120); the
    /// lines come out 4 cells narrower, the width of Claude Code's box.
    #[arg(long, value_name = "N")]
    pub width: Option<usize>,
}

impl RenderArgs {
    /// The overrides as a config overlay. A typo is a one-line note on
    /// stderr and a [`Quiet`] failure, not an error report, and each of the
    /// four flags is checked here: a theme left to the config's resolver
    /// was reported under every fixture as a problem of the config file.
    fn overlay(&self) -> Result<Overlay> {
        let typo = |what: &str, value: &str, expected: &str| {
            eprintln!("unknown {what} {value:?}; expected {expected}");
            color_eyre::Report::from(Quiet)
        };
        // `a, b or c`.
        let either = |names: Vec<&str>| match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
            _ => names.join(""),
        };
        let preset = self
            .preset
            .as_deref()
            .map(|p| {
                TopPreset::parse(p).ok_or_else(|| {
                    typo("preset", p, &either(TopPreset::ALL.map(TopPreset::name).to_vec()))
                })
            })
            .transpose()?;
        let icons = self
            .icons
            .as_deref()
            .map(|i| {
                IconSet::parse(i).ok_or_else(|| {
                    typo("icon set", i, &either(IconSet::ALL.map(IconSet::name).to_vec()))
                })
            })
            .transpose()?;
        let color = self
            .color
            .as_deref()
            .map(|c| {
                ColorChoice::parse(c).ok_or_else(|| {
                    typo("color mode", c, &either(ColorChoice::ALL.map(ColorChoice::name).to_vec()))
                })
            })
            .transpose()?;
        let theme = self
            .theme
            .as_deref()
            .map(|t| {
                crate::theme::palette(t).map(|_| t.to_owned()).ok_or_else(|| {
                    typo(
                        "theme",
                        t,
                        &either(crate::theme::PALETTES.iter().map(|p| p.name).collect()),
                    )
                })
            })
            .transpose()?;
        Ok(Overlay { preset, icons, theme, color })
    }
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Render the status line from the JSON payload on stdin (the default).
    Render,
    /// Render a payload fixture file (or every fixture in a directory), drawn faint as Claude Code draws the status line.
    Preview {
        /// Fixture file, or a directory of `*.json` fixtures.
        path: PathBuf,
        /// Overrides.
        #[command(flatten)]
        args: RenderArgs,
    },
    /// List the built-in modules.
    Modules,
    /// List the gallery presets: complete example configs for `config init --preset`.
    Presets,
    /// Background worker: recompute one module's cache entry (or all cached modules).
    #[command(hide = true)]
    Refresh {
        /// Module id; omit with `--all`.
        #[arg(long, required_unless_present = "all")]
        module: Option<String>,
        /// Refresh every cached module.
        #[arg(long)]
        all: bool,
        /// Session id.
        #[arg(long)]
        session: String,
        /// Working directory the tick reported.
        #[arg(long)]
        cwd: PathBuf,
        /// The caller already holds the module lock; release it when done.
        ///
        /// Only ever passed with `--module`, by the tick that took that one
        /// lock (`spawn::Job::args`). With `--all` it would adopt a
        /// lock per module — inventing one where there was none and taking
        /// over one a live worker still holds — so the two are exclusive.
        #[arg(long, conflicts_with = "all")]
        lock_held: bool,
    },
    /// Remove cache directories (sessions and repositories) idle for more
    /// than a day, and leftover temporary and stale lock files.
    Gc,
    /// Regenerate the reference documentation from the module schemas (for
    /// maintainers; `make docs` does it through the docs-sync test).
    #[command(hide = true)]
    Docs {
        /// Output directory; there is no default, since the pages replace
        /// same-named files in it.
        #[arg(long)]
        out: PathBuf,
    },
    /// Wire garnish into Claude Code's settings.json (a backup is kept).
    Install {
        /// Settings file (default `~/.claude/settings.json`, or
        /// `$CLAUDE_CONFIG_DIR/settings.json` when that is set).
        #[arg(long, value_name = "FILE")]
        settings: Option<PathBuf>,
        /// `statusLine.refreshInterval` in seconds (Claude Code's minimum is 1).
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..))]
        refresh_interval: u64,
        /// `statusLine.padding`; the generated config gets `padding = 2N`
        /// to match (the harness pads both sides).
        #[arg(
            long,
            value_name = "N",
            value_parser = clap::value_parser!(u64).range(0..=crate::install::MAX_PADDING)
        )]
        padding: Option<u64>,
        /// Write the path this binary is found by (the launcher on PATH,
        /// not the file it links to) instead of `garnish`.
        #[arg(long)]
        absolute: bool,
        /// Do not write a default config file when none exists.
        #[arg(long)]
        no_config: bool,
        /// Do not write the bundled skills to `~/.claude/skills`.
        #[arg(long)]
        no_skills: bool,
        /// Print what would change without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// The bundled Claude Code skills: list them or write them to `~/.claude/skills`.
    Skills {
        /// What to do.
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Print a diagnostic report: versions, settings, config, cache, environment, glyphs.
    Doctor,
    /// Inspect or create the configuration.
    Config {
        /// What to do.
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Set garnish up full-screen: pick a preset, build a layout, install.
    Setup {
        /// Write this preset (a built-in or a gallery name) without opening
        /// the screen, keeping a backup of the file it replaces.
        #[arg(long, value_name = "NAME")]
        preset: Option<String>,
        /// With `--preset`: hook garnish into Claude Code's settings.json too.
        #[arg(long, requires = "preset")]
        install: bool,
    },
}

/// `garnish skills …`.
#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// Write the skills to `<dir>/<name>/SKILL.md` (default `~/.claude/skills`,
    /// or `$CLAUDE_CONFIG_DIR/skills` when that is set).
    Install {
        /// Target directory.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// List the bundled skills with their descriptions.
    List,
}

/// `garnish config …`.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the path of the config file in effect (or where one would be created).
    Path,
    /// Validate the config file and report every problem.
    Check,
    /// Print the fully resolved configuration as TOML.
    Show,
    /// Write a fully annotated default config file.
    Init {
        /// Replace an existing file, keeping a timestamped backup next to
        /// it; a file that does not parse is refused.
        #[arg(long)]
        force: bool,
        /// A built-in preset (default | minimal | full | compact) or a gallery
        /// preset name (`garnish presets`), whose file is written as is.
        #[arg(long, default_value = "default", value_name = "NAME")]
        preset: String,
    },
}

/// A failure that was already reported to the user.
///
/// The problem list is on stdout or a one-line note on stderr, so the process
/// exits non-zero without an error report: a source location would only
/// obscure the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quiet;

impl std::fmt::Display for Quiet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("reported above")
    }
}

impl std::error::Error for Quiet {}

/// Entry point used by `main`: the exit code, or an error worth a report.
///
/// # Errors
/// Returns an error for unexpected subcommand failures; a [`Quiet`] failure
/// becomes [`std::process::ExitCode::FAILURE`] instead, and `render` itself
/// never fails.
pub fn run() -> Result<std::process::ExitCode> {
    match run_command() {
        Ok(()) => Ok(std::process::ExitCode::SUCCESS),
        Err(e) if e.downcast_ref::<Quiet>().is_some() => Ok(std::process::ExitCode::FAILURE),
        // A reader that stopped reading (`garnish presets | head`) wanted
        // no more; that is not a failure worth a report.
        Err(e) if broken_pipe(&e) => Ok(std::process::ExitCode::SUCCESS),
        Err(e) => Err(e),
    }
}

/// Whether an error is, at bottom, a write to a pipe nobody reads.
fn broken_pipe(e: &color_eyre::Report) -> bool {
    e.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

/// A command line clap refused.
///
/// On the render path (no subcommand word, and stdin not a terminal: the
/// harness running `statusLine.command`) a non-zero exit would clear the
/// status line without a word, so the error's first line becomes the
/// `⚠ garnish:` row and the exit is 0 (SPEC § 5), the whole error going
/// to stderr. Anywhere else, and for `--help` and `--version`, clap
/// reports it and exits as it always does.
fn parse_failure(e: &clap::Error) -> Result<()> {
    use clap::error::ErrorKind;
    let shown = matches!(
        e.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    if shown || names_a_subcommand() || stdin_is_terminal() {
        e.exit();
    }
    let text = e.render().to_string();
    let first = text.lines().next().unwrap_or_default();
    let first = first.strip_prefix("error: ").unwrap_or(first);
    let row = writeln!(std::io::stdout().lock(), "⚠ garnish: {}", crate::ansi::plain_text(first));
    crate::debug::stderr_line(text.trim_end());
    Ok(row?)
}

/// Whether the command line names a subcommand other than `render`, so it
/// is not the render path whatever else is wrong with it. `render` is the
/// render path spelled out, for a settings file that wants a subcommand.
fn names_a_subcommand() -> bool {
    use clap::CommandFactory as _;
    let command = Cli::command();
    let other = |arg: &std::ffi::OsStr| {
        command.get_subcommands().any(|s| s.get_name() != "render" && arg == s.get_name())
    };
    std::env::args_os().skip(1).any(|arg| other(&arg))
}

/// Debug builds only: set, a tick panics before it renders, so the
/// internal-error row of SPEC § 5 is testable through the binary.
pub const TEST_PANIC_ENV: &str = "GARNISH_TEST_PANIC";

/// Make a panic on the render path what SPEC § 5 promises: a `⚠ garnish:
/// internal error` row and exit 0, since a non-zero exit clears the status
/// line. The release build aborts on a panic, after this hook has run.
///
/// The row goes out before the note on stderr, and neither write may fail
/// the hook: a panic inside it aborts the process with nothing printed.
// A panic hook cannot return to the program, and an abort or an unwind
// both exit non-zero: `exit(0)` is the one way to keep the status line.
#[allow(clippy::exit)]
fn render_panics_as_a_row() {
    std::panic::set_hook(Box::new(|info| {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all("⚠ garnish: internal error\n".as_bytes());
        let _ = stdout.flush();
        crate::debug::stderr_line(&format!("garnish: {info}"));
        std::process::exit(0);
    }));
}

/// The [`TEST_PANIC_ENV`] hook: a panic is its whole job, and a release
/// build compiles it out.
fn test_panic() {
    let armed =
        cfg!(debug_assertions) && crate::claude_settings::env_flag(TEST_PANIC_ENV) == Some(true);
    assert!(!armed, "{TEST_PANIC_ENV} is set");
}

fn run_command() -> Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return parse_failure(&e),
    };
    let config_path = cli.config.as_deref();
    let command = match cli.command {
        Some(command) => command,
        // A bare `garnish` at a terminal is a person, not the harness: no
        // payload is coming, so point at `setup` instead of waiting for
        // one (SPEC § 14). The explicit `render` always reads stdin.
        None if stdin_is_terminal() => {
            let mut stdout = std::io::stdout().lock();
            writeln!(
                stdout,
                "garnish renders the status line from the JSON Claude Code pipes to it.\n\
                 Run `garnish setup` to configure it, or `garnish --help` for the commands."
            )?;
            return Ok(());
        }
        None => Command::Render,
    };
    // The render path cannot return an error, so it skips color-eyre's
    // report handler installation; every other subcommand gets pretty errors.
    if !matches!(command, Command::Render) {
        color_eyre::install()?;
    }
    match command {
        Command::Render => {
            render_panics_as_a_row();
            test_panic();
            render_stdin(config_path);
            Ok(())
        }
        Command::Preview { path, args } => preview(&path, config_path, &args),
        Command::Modules => {
            let mut stdout = std::io::stdout().lock();
            for s in SCHEMAS.iter() {
                writeln!(stdout, "{:<13} {}", s.id, s.summary)?;
            }
            let text = &*crate::modules::text::SCHEMA;
            writeln!(stdout, "{:<13} {}", "text.<name>", text.summary)?;
            Ok(())
        }
        Command::Presets => {
            let mut stdout = std::io::stdout().lock();
            for p in crate::gallery::PRESETS.iter() {
                let needs = p.needs.map_or(String::new(), |n| format!(" [{n}]"));
                writeln!(stdout, "{:<24} {} ({} cols){needs}", p.name, p.summary, p.columns)?;
            }
            Ok(())
        }
        Command::Config { action } => config_cmd(&action, config_path),
        Command::Refresh { module, all, session, cwd, lock_held } => {
            refresh(module.as_deref(), all, &session, &cwd, lock_held, config_path)
        }
        Command::Docs { out } => {
            let written = crate::docs::generate(&out)
                .with_context(|| format!("writing {}", out.display()))?;
            writeln!(std::io::stdout().lock(), "wrote {written} file(s) under {}", out.display())?;
            Ok(())
        }
        Command::Install {
            settings,
            refresh_interval,
            padding,
            absolute,
            no_config,
            no_skills,
            dry_run,
        } => {
            let options = crate::install::Options {
                settings,
                refresh_interval,
                padding,
                absolute,
                write_config: !no_config,
                write_skills: !no_skills,
                config_path: explicit_or_quiet(config_path)?,
                config_written: None,
            };
            let steps = crate::install::Steps::plan(&options).map_err(refusal)?;
            print_install(&steps, dry_run)
        }
        Command::Skills { action } => skills(action),
        Command::Setup { preset, install } => {
            crate::setup::run(&crate::setup::Args { preset, install, config_path })
        }
        Command::Doctor => {
            std::io::stdout().lock().write_all(crate::doctor::report(config_path).as_bytes())?;
            Ok(())
        }
        Command::Gc => {
            let cache = crate::cache::Cache::from_env();
            let n = cache.gc_sessions(crate::cache::GC_MAX_AGE_MS, usize::MAX);
            writeln!(
                std::io::stdout().lock(),
                "removed {n} idle cache dir(s) (sessions and repositories) under {}",
                cache.root().display()
            )?;
            Ok(())
        }
    }
}

/// The tick: the payload on stdin rendered to stdout.
///
/// The render path never fails (SPEC § 5): unreadable or non-UTF-8 stdin
/// becomes a warning line, and a closed stdout (EPIPE) is not worth an
/// error report. A render whose rows all hid prints one empty line, which
/// Claude Code trims to nothing and so clears the status line until a
/// module has something to show.
fn render_stdin(config_path: Option<&Path>) {
    let mut bytes = Vec::with_capacity(8 * 1024);
    let input = match std::io::stdin().read_to_end(&mut bytes) {
        Ok(_) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) => {
            crate::debug::stderr_line(&format!("garnish: reading stdin: {e}"));
            String::new()
        }
    };
    let req = Request {
        payload_json: &input,
        config_path,
        overlay: Overlay::default(),
        columns: env_columns(),
        no_color: config::no_color_env(),
        dim: false,
        workers: true,
    };
    let out = render::render(&req);
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
    tick_note(&input, req.columns, &out);
}

fn refresh(
    module: Option<&str>,
    all: bool,
    session: &str,
    cwd: &Path,
    lock_held: bool,
    config_path: Option<&Path>,
) -> Result<()> {
    use crate::modules::{REGISTRY, RefreshCtx, record_lock_failure, run_refresh};
    use rayon::prelude::*;
    let loaded = config::load(config_path, &SCHEMAS);
    let cache = crate::cache::Cache::from_env();
    let targets: Vec<&crate::modules::Entry> = REGISTRY
        .iter()
        .filter(|e| if all { e.schema.refresh > 0 } else { Some(e.schema.id) == module })
        .collect();
    if targets.is_empty() {
        return Err(eyre!("unknown module {}", module.unwrap_or("?")));
    }
    if let Some(entry) = targets.iter().find(|e| e.schema.refresh == 0) {
        eprintln!(
            "{} renders from the payload every tick; there is nothing to refresh",
            entry.schema.id
        );
        return Err(Quiet.into());
    }
    let results: Vec<Result<()>> = targets
        .par_iter()
        .map(|entry| {
            let Some(cfg) = loaded.config.modules.get(entry.schema.id) else { return Ok(()) };
            let scope = entry.module.scope(session, cwd);
            let ctx = RefreshCtx { session, cwd, cfg, cache: &cache };
            // Hold (or inherit) the lock while working so ticks do not spawn twice.
            let guard = if lock_held {
                crate::cache::LockGuard::adopt(cache.lock_path(&scope, entry.schema.id))
            } else {
                match cache.lock(&scope, entry.schema.id) {
                    crate::cache::LockOutcome::Acquired(g) => g,
                    crate::cache::LockOutcome::Held => return Ok(()),
                    crate::cache::LockOutcome::Unavailable(e) => {
                        let _ = record_lock_failure(entry.module.as_ref(), &ctx, &e);
                        return Err(e.into());
                    }
                }
            };
            run_refresh(entry.module.as_ref(), &ctx)
                .with_context(|| format!("refreshing {}", entry.schema.id))?;
            drop(guard);
            Ok(())
        })
        .collect();
    results.into_iter().collect()
}

/// `garnish skills list | install [--dir D]` (SPEC § 13).
fn skills(action: SkillsAction) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    match action {
        SkillsAction::List => {
            for (name, text) in crate::skills::SKILLS {
                writeln!(stdout, "{name:<24} {}", crate::skills::description(text))?;
            }
        }
        SkillsAction::Install { dir } => {
            let Some(dir) = dir.or_else(|| {
                crate::install::default_settings_path().map(|s| crate::skills::default_dir(&s))
            }) else {
                return Err(refusal(Refusal::NoHome {
                    flag: "--dir <DIR>",
                    what: "the skills go",
                }));
            };
            let report = crate::skills::install(&dir)
                .with_context(|| format!("writing skills to {}", dir.display()))?;
            writeln!(stdout, "{}", report.summary())?;
        }
    }
    Ok(())
}

/// Print an install plan's notes on stderr, then apply it (or, for
/// `--dry-run`, describe it) and print what it did on stdout: `garnish
/// install` and `setup --preset P --install` alike.
///
/// # Errors
/// An apply's refusal ([`refusal`]), or a closed stdout.
pub(crate) fn print_install(steps: &crate::install::Steps, dry_run: bool) -> Result<()> {
    // Advice goes to stderr: --dry-run's stdout is the settings preview.
    for note in steps.notes() {
        eprintln!("{note}");
    }
    let lines = if dry_run { steps.dry_run() } else { steps.apply().map_err(refusal)? };
    let mut stdout = std::io::stdout().lock();
    for line in lines {
        writeln!(stdout, "{line}")?;
    }
    Ok(())
}

/// A refusal as every command reports it (SPEC § 5): the one line
/// [`Refusal`]'s `Display` words on stderr and a [`Quiet`] exit for what a
/// person can fix (no home, a file that does not parse, one that exists),
/// an error report for an I/O failure.
pub(crate) fn refusal(r: Refusal) -> color_eyre::Report {
    match r {
        Refusal::Io(e) => eyre!(e),
        other => {
            eprintln!("{other}");
            Quiet.into()
        }
    }
}

/// The per-tick diagnostic line of SPEC § 5, written only with
/// `GARNISH_DEBUG` set: what the tick was given and what it produced, which
/// is what a report of "the status line looks wrong" needs and a screenshot
/// does not carry. Costs one environment read when the hook is off.
fn tick_note(input: &str, columns: Option<usize>, out: &str) {
    if !crate::debug::enabled() {
        return;
    }
    let widest = out
        .lines()
        .map(|line| crate::ansi::display_width(&crate::ansi::strip_ansi(line)))
        .max()
        .unwrap_or(0);
    crate::debug::log(&format!(
        "tick stdin={}B columns={} rows={} widest={widest}",
        input.len(),
        columns.map_or_else(|| "unset".to_owned(), |c| c.to_string()),
        out.lines().count(),
    ));
}

/// Environment variable naming the width when `COLUMNS` is absent.
pub const COLUMNS_ENV: &str = "GARNISH_COLUMNS";

/// Environment variable that overrides the "is stdin a terminal" check of
/// the bare `garnish` (`1` or `0`), so both paths are testable without a
/// pty (SPEC § 9).
pub const STDIN_TTY_ENV: &str = "GARNISH_STDIN_TTY";

/// Whether stdin is a terminal, as the bare `garnish` decides it: the
/// [`STDIN_TTY_ENV`] hook first, then the descriptor itself.
#[must_use]
pub fn stdin_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    crate::claude_settings::env_flag(STDIN_TTY_ENV)
        .unwrap_or_else(|| std::io::stdin().is_terminal())
}

/// Whether stdout is a terminal, which the `setup` screen needs.
#[must_use]
pub fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdout().is_terminal()
}

/// `COLUMNS`, then [`COLUMNS_ENV`].
#[must_use]
pub fn env_columns() -> Option<usize> {
    ["COLUMNS", COLUMNS_ENV].iter().find_map(|k| std::env::var(k).ok()?.trim().parse().ok())
}

fn preview(path: &Path, config_path: Option<&Path>, args: &RenderArgs) -> Result<()> {
    let mut files: Vec<PathBuf> = if path.is_dir() {
        std::fs::read_dir(path)
            .with_context(|| format!("reading {}", path.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect()
    } else {
        vec![path.to_path_buf()]
    };
    files.sort();
    let overlay = args.overlay()?;
    // The config the status line reads (SPEC § 4), not only the one a
    // bare lookup finds: a preview is a person asking what their line
    // looks like.
    let config_file = read_config_or_quiet(config_path)?;
    let config_path = config_file.as_deref();
    let columns = args.width.or_else(env_columns);
    let mut stdout = std::io::stdout().lock();
    for file in files {
        let input = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        writeln!(stdout, "\x1b[2m── {name}\x1b[0m")?;
        let req = Request {
            payload_json: &input,
            config_path,
            overlay: overlay.clone(),
            columns,
            no_color: config::no_color_env(),
            // Drawn as the screen draws it: every row faint (SPEC § 2.1).
            dim: true,
            // A preview is not a tick: no cache, no worker (SPEC § 14).
            workers: false,
        };
        stdout.write_all(render::render(&req).as_bytes())?;
    }
    Ok(())
}

fn config_cmd(action: &ConfigAction, config_path: Option<&Path>) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    match action {
        ConfigAction::Path => {
            let p = target_or_quiet(config_path, "the config is")?;
            writeln!(stdout, "{}", p.display())?;
        }
        ConfigAction::Check => {
            let path = read_config_or_quiet(config_path)?;
            let loaded = config::load(path.as_deref(), &SCHEMAS);
            match (&loaded.path, loaded.errors.is_empty()) {
                (None, _) => {
                    writeln!(stdout, "no config file found; built-in defaults are in effect")?;
                }
                (Some(p), true) => {
                    writeln!(stdout, "{}: ok", p.display())?;
                }
                (Some(p), false) => {
                    for e in &loaded.errors {
                        writeln!(stdout, "{}: {e}", p.display())?;
                    }
                    writeln!(stdout, "{} problem(s) found", loaded.errors.len())?;
                    return Err(Quiet.into());
                }
            }
        }
        ConfigAction::Show => {
            let path = read_config_or_quiet(config_path)?;
            let loaded = config::load(path.as_deref(), &SCHEMAS);
            let mut cfg = loaded.config;
            // The animation switch in effect for this directory (SPEC
            // § 4.2): the file, else Claude Code's prefersReducedMotion.
            // The session variable stays out of it: `show` prints a config,
            // and GARNISH_ANIMATE=0 belongs to a session, not a file.
            cfg.animate = Some(cfg.animate.unwrap_or_else(|| {
                let cwd = std::env::current_dir().ok();
                let home = crate::claude_settings::home_dir();
                !crate::claude_settings::reduced_motion(&crate::claude_settings::keys_for(
                    cwd.as_deref(),
                    home.as_deref(),
                ))
            }));
            stdout.write_all(crate::docs::config_toml(&cfg, false).as_bytes())?;
        }
        ConfigAction::Init { force, preset } => {
            let text = preset_text(preset)?;
            let target = config_target_or_quiet(config_path)?;
            let backup = crate::install::write_config(&target, &text, *force).map_err(refusal)?;
            writeln!(stdout, "{}", crate::install::wrote_line(&target, backup.as_deref()))?;
        }
    }
    Ok(())
}

/// The file `config init --preset <name>` and `setup --preset <name>` write:
/// the annotated default file for a built-in preset, a gallery preset's file
/// without its tooling header.
///
/// # Errors
/// An unknown name is a one-line note and a [`Quiet`] failure.
pub fn preset_text(preset: &str) -> Result<String> {
    if let Some(top) = TopPreset::parse(preset) {
        let (cfg, _) =
            config::parse_with("", &SCHEMAS, &Overlay { preset: Some(top), ..Default::default() });
        return Ok(crate::docs::config_toml(&cfg, true));
    }
    let Some(p) = crate::gallery::find(preset) else {
        // A typo, not a fault: one line, no report.
        eprintln!(
            "unknown preset {preset:?}; expected {} or a gallery name ({})",
            TopPreset::ALL.map(TopPreset::name).join(", "),
            crate::gallery::PRESETS.iter().map(|p| p.name).collect::<Vec<_>>().join(", ")
        );
        return Err(Quiet.into());
    };
    Ok(crate::gallery::body(p.source))
}

/// Where the config a command writes goes ([`config::write_target`]: the
/// file the tick reads, else the default location), or a [`Quiet`]
/// refusal.
///
/// # Errors
/// [`Quiet`] after the one-line note, without a home or when the
/// `statusLine.command` passes a `--config` that names no one file or that
/// a checkout's own settings choose.
pub fn config_target_or_quiet(explicit: Option<&Path>) -> Result<PathBuf> {
    target_or_quiet(explicit, "the config goes")
}

/// The config named explicitly ([`config::hand_explicit`]: `--config`,
/// else a `GARNISH_CONFIG` the person set), or a [`Quiet`] refusal when a
/// checkout's settings set that variable.
///
/// # Errors
/// [`Quiet`] after the one-line note.
pub fn explicit_or_quiet(flag: Option<&Path>) -> Result<Option<PathBuf>> {
    config::hand_explicit(flag).map_err(|checkout| refusal(Refusal::CheckoutConfig(checkout)))
}

/// The config a command run by hand reads ([`config::read_target`]: the
/// file `config path` prints, or `None` for the built-in defaults), or a
/// [`Quiet`] refusal where `config path` refuses.
fn read_config_or_quiet(explicit: Option<&Path>) -> Result<Option<PathBuf>> {
    match config::read_target(explicit) {
        config::ReadTarget::File(path) => Ok(Some(path)),
        config::ReadTarget::Defaults => Ok(None),
        config::ReadTarget::Unresolved { settings, key, word } => {
            Err(refusal(Refusal::UnresolvedConfig { settings, key, word }))
        }
        config::ReadTarget::Checkout(checkout) => Err(refusal(Refusal::CheckoutConfig(checkout))),
    }
}

/// [`config_target_or_quiet`], with `what` finishing the no-home note.
fn target_or_quiet(explicit: Option<&Path>, what: &'static str) -> Result<PathBuf> {
    match config::write_target(explicit, config::CommandFrom::Chain) {
        config::WriteTarget::File(path) => Ok(path),
        config::WriteTarget::NoHome => {
            Err(refusal(Refusal::NoHome { flag: "--config <FILE>", what }))
        }
        config::WriteTarget::Unresolved { settings, key, word } => {
            Err(refusal(Refusal::UnresolvedConfig { settings, key, word }))
        }
        config::WriteTarget::Checkout(checkout) => Err(refusal(Refusal::CheckoutConfig(checkout))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Vocab;

    /// cfg-14: the `preview` flags that take a vocabulary name its words in
    /// their help as the parser lists them (the attribute has to be a
    /// literal, so this is what keeps it honest).
    #[test]
    fn the_overlay_flags_name_the_parsers_words() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let preview = cli.find_subcommand("preview").unwrap();
        let value_name = |id: &str| {
            let arg = preview.get_arguments().find(|a| a.get_id() == id).unwrap();
            arg.get_value_names().unwrap().iter().map(ToString::to_string).collect::<String>()
        };
        assert_eq!(value_name("preset"), TopPreset::names().join("|"));
        assert_eq!(value_name("icons"), IconSet::names().join("|"));
        assert_eq!(value_name("color"), ColorChoice::names().join("|"));
    }
}
