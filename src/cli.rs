//! Command-line interface.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result, eyre};

use crate::config::{self, ColorChoice, Overlay, presets::TopPreset};
use crate::icons::IconSet;
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
    /// Terminal width to lay out for (defaults to `COLUMNS`, then 120); the
    /// lines come out 4 cells narrower, the width of Claude Code's box.
    #[arg(long, value_name = "N")]
    pub width: Option<usize>,
}

impl RenderArgs {
    fn overlay(&self) -> Result<Overlay> {
        let preset = self
            .preset
            .as_deref()
            .map(|p| TopPreset::parse(p).ok_or_else(|| eyre!("unknown preset {p:?}")))
            .transpose()?;
        let icons = self
            .icons
            .as_deref()
            .map(|i| IconSet::parse(i).ok_or_else(|| eyre!("unknown icon set {i:?}")))
            .transpose()?;
        let color = self
            .color
            .as_deref()
            .map(|c| match c {
                "auto" => Ok(ColorChoice::Auto),
                "always" => Ok(ColorChoice::Always),
                "never" => Ok(ColorChoice::Never),
                "256" => Ok(ColorChoice::Ansi256),
                "truecolor" => Ok(ColorChoice::TrueColor),
                other => Err(eyre!("unknown color mode {other:?}")),
            })
            .transpose()?;
        Ok(Overlay { preset, icons, theme: self.theme.clone(), color })
    }
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Render the status line from the JSON payload on stdin (the default).
    Render,
    /// Render a payload fixture file (or every fixture in a directory).
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
        #[arg(long)]
        lock_held: bool,
    },
    /// Remove cache directories of sessions idle for more than a day.
    Gc,
    /// Regenerate the reference documentation from the module schemas.
    Docs {
        /// Output directory (default `docs`).
        #[arg(long, default_value = "docs")]
        out: PathBuf,
    },
    /// Wire garnish into Claude Code's settings.json (a backup is kept).
    Install {
        /// Settings file (default `~/.claude/settings.json`).
        #[arg(long, value_name = "FILE")]
        settings: Option<PathBuf>,
        /// `statusLine.refreshInterval` in seconds.
        #[arg(long, default_value_t = 1)]
        refresh_interval: u64,
        /// `statusLine.padding`; the generated config gets `padding = 2N`
        /// to match (the harness pads both sides).
        #[arg(long, value_name = "N", value_parser = clap::value_parser!(u64).range(0..=32_767))]
        padding: Option<u64>,
        /// Write the absolute path of this binary instead of `garnish`.
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
}

/// `garnish skills …`.
#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// Write the skills to `<dir>/<name>/SKILL.md` (default `~/.claude/skills`).
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
        /// Overwrite an existing file.
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
        Err(e) => Err(e),
    }
}

fn run_command() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.as_deref();
    let command = cli.command.unwrap_or(Command::Render);
    // The render path cannot return an error, so it skips color-eyre's
    // report handler installation; every other subcommand gets pretty errors.
    if !matches!(command, Command::Render) {
        color_eyre::install()?;
    }
    match command {
        Command::Render => {
            // The render path never fails and never prints nothing (SPEC § 5):
            // unreadable or non-UTF-8 stdin becomes a warning line, and a
            // closed stdout (EPIPE) is not worth an error report.
            let mut bytes = Vec::with_capacity(8 * 1024);
            let input = match std::io::stdin().read_to_end(&mut bytes) {
                Ok(_) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => {
                    eprintln!("garnish: reading stdin: {e}");
                    String::new()
                }
            };
            let req = Request {
                payload_json: &input,
                config_path,
                overlay: Overlay::default(),
                columns: env_columns(),
                no_color: std::env::var_os("NO_COLOR").is_some(),
            };
            let out = render::render(&req);
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(out.as_bytes());
            let _ = stdout.flush();
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
                let needs = p.needs.as_deref().map_or(String::new(), |n| format!(" [{n}]"));
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
        } => install(
            settings,
            refresh_interval,
            padding,
            InstallSkip { config: no_config, skills: no_skills },
            absolute,
            dry_run,
            config_path,
        ),
        Command::Skills { action } => skills(action),
        Command::Doctor => {
            std::io::stdout().lock().write_all(crate::doctor::report(config_path).as_bytes())?;
            Ok(())
        }
        Command::Gc => {
            let cache = crate::cache::Cache::from_env();
            let n = cache.gc_sessions(crate::cache::GC_MAX_AGE_MS, usize::MAX);
            writeln!(
                std::io::stdout().lock(),
                "removed {n} idle session dir(s) under {}",
                cache.root().display()
            )?;
            Ok(())
        }
    }
}

fn refresh(
    module: Option<&str>,
    all: bool,
    session: &str,
    cwd: &Path,
    lock_held: bool,
    config_path: Option<&Path>,
) -> Result<()> {
    use crate::modules::{REGISTRY, RefreshCtx, run_refresh};
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
    let results: Vec<Result<()>> = targets
        .par_iter()
        .map(|entry| {
            let Some(cfg) = loaded.config.modules.get(entry.schema.id) else { return Ok(()) };
            let scope = entry.module.scope(session, cwd);
            // Hold (or inherit) the lock while working so ticks do not spawn twice.
            let guard = if lock_held {
                crate::cache::LockGuard::adopt(cache.lock_path(&scope, entry.schema.id))
            } else {
                match cache.lock(&scope, entry.schema.id) {
                    crate::cache::LockOutcome::Acquired(g) => g,
                    crate::cache::LockOutcome::Held => return Ok(()),
                    crate::cache::LockOutcome::Unavailable(e) => return Err(e.into()),
                }
            };
            let ctx = RefreshCtx { session, cwd, cfg, cache: &cache };
            run_refresh(entry.module.as_ref(), &ctx)
                .with_context(|| format!("refreshing {}", entry.schema.id))?;
            drop(guard);
            Ok(())
        })
        .collect();
    results.into_iter().collect::<Result<Vec<()>>>().map(|_| ())
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
                return Err(no_home("--dir <DIR>", "the skills go"));
            };
            let report = crate::skills::install(&dir)
                .with_context(|| format!("writing skills to {}", dir.display()))?;
            writeln!(stdout, "{}", report.summary())?;
        }
    }
    Ok(())
}

/// The parts of `garnish install` a flag can switch off.
#[derive(Debug, Clone, Copy)]
struct InstallSkip {
    /// `--no-config`: leave the default config file alone.
    config: bool,
    /// `--no-skills`: leave `~/.claude/skills` alone.
    skills: bool,
}

fn install(
    settings: Option<PathBuf>,
    refresh_interval: u64,
    padding: Option<u64>,
    skip: InstallSkip,
    absolute: bool,
    dry_run: bool,
    config_path: Option<&Path>,
) -> Result<()> {
    use crate::install::{self as inst, Plan};
    let mut stdout = std::io::stdout().lock();
    let command = if absolute {
        std::env::current_exe().context("locating this binary")?.display().to_string()
    } else {
        "garnish".to_owned()
    };
    let Some(settings) = settings.or_else(inst::default_settings_path) else {
        return Err(no_home("--settings <FILE>", "settings.json is"));
    };
    let plan = Plan { settings, command, refresh_interval: refresh_interval.max(1), padding };
    if !absolute && !inst::on_path("garnish", std::env::var_os("PATH").as_deref()) {
        eprintln!("warning: `garnish` is not on PATH; run `make install` first or use --absolute");
    }
    let existing = inst::read_existing(&plan.settings).map_err(|e| eyre!(e))?;
    let merged = inst::merge(existing.as_deref().unwrap_or(""), &plan).map_err(|e| eyre!(e))?;
    if dry_run {
        writeln!(stdout, "would write {}:", plan.settings.display())?;
        stdout.write_all(merged.as_bytes())?;
    } else {
        let outcome = inst::apply(&plan).map_err(|e| eyre!(e))?;
        match (outcome.changed, outcome.backup) {
            (false, _) => writeln!(stdout, "{} already up to date", plan.settings.display())?,
            (true, Some(b)) => {
                writeln!(stdout, "updated {} (backup: {})", plan.settings.display(), b.display())?;
            }
            (true, None) => writeln!(stdout, "wrote {}", plan.settings.display())?,
        }
    }
    if !skip.config {
        install_default_config(&mut stdout, config_path, padding, dry_run)?;
    }
    // The skills (SPEC § 13) go next to the settings file, in ~/.claude/skills.
    // They come last: they are the optional part, so a problem with them
    // never leaves the settings updated and the config unwritten.
    if !skip.skills {
        let dir = crate::skills::default_dir(&plan.settings);
        if dry_run {
            writeln!(
                stdout,
                "would write {} skill(s) to {}",
                crate::skills::SKILLS.len(),
                dir.display()
            )?;
        } else {
            let report = crate::skills::install(&dir)
                .with_context(|| format!("writing skills to {}", dir.display()))?;
            writeln!(stdout, "{}", report.summary())?;
        }
    }
    Ok(())
}

/// The config half of `garnish install`: write the annotated default file
/// when none exists, seeded with `padding` when `--padding` was given.
fn install_default_config(
    stdout: &mut impl Write,
    config_path: Option<&Path>,
    padding: Option<u64>,
    dry_run: bool,
) -> Result<()> {
    let Some(target) = config_target(config_path) else {
        return Err(no_home("--config <FILE>", "the config goes"));
    };
    // The harness pads both sides, so the config mirrors statusLine.padding doubled (SPEC § 2.1).
    let config_padding = padding.map(|p| p.saturating_mul(2));
    let seeded = config_padding.map_or_else(String::new, |p| format!(" (padding = {p})"));
    if target.exists() {
        // stderr, like the PATH warning: --dry-run's stdout is the settings preview.
        if let Some(p) = config_padding {
            eprintln!(
                "note: {} already exists; set `padding = {p}` in it to match statusLine.padding",
                target.display()
            );
        }
    } else if dry_run {
        writeln!(stdout, "would write a default config to {}{seeded}", target.display())?;
    } else {
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let seed = config_padding.map_or_else(String::new, |p| format!("padding = {p}\n"));
        let (cfg, _) = config::parse(&seed, &SCHEMAS);
        std::fs::write(&target, crate::docs::config_toml(&cfg, true))
            .with_context(|| format!("writing {}", target.display()))?;
        writeln!(stdout, "wrote default config to {}{seeded}", target.display())?;
    }
    Ok(())
}

/// Where a written config goes: `--config`, then `GARNISH_CONFIG`, then the
/// default location; `None` without a home directory (SPEC § 5: never
/// guess the current directory).
fn config_target(explicit: Option<&Path>) -> Option<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .or_else(|| {
            std::env::var_os(config::CONFIG_ENV).filter(|v| !v.is_empty()).map(PathBuf::from)
        })
        .or_else(config::default_path)
}

/// The one-line refusal for a writing command run without `HOME`.
fn no_home(flag: &str, what: &str) -> color_eyre::Report {
    eprintln!("HOME is not set; pass {flag} to say where {what}");
    Quiet.into()
}

/// `COLUMNS`, then `GARNISH_COLUMNS`.
#[must_use]
pub fn env_columns() -> Option<usize> {
    ["COLUMNS", "GARNISH_COLUMNS"].iter().find_map(|k| std::env::var(k).ok()?.trim().parse().ok())
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
            no_color: std::env::var_os("NO_COLOR").is_some(),
        };
        stdout.write_all(render::render(&req).as_bytes())?;
    }
    Ok(())
}

fn config_cmd(action: &ConfigAction, config_path: Option<&Path>) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    match action {
        ConfigAction::Path => {
            let Some(p) = config::locate(config_path).or_else(|| config_target(config_path)) else {
                return Err(no_home("--config <FILE>", "the config is"));
            };
            writeln!(stdout, "{}", p.display())?;
        }
        ConfigAction::Check => {
            let loaded = config::load(config_path, &SCHEMAS);
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
            let loaded = config::load(config_path, &SCHEMAS);
            stdout.write_all(crate::docs::config_toml(&loaded.config, false).as_bytes())?;
        }
        ConfigAction::Init { force, preset } => {
            // A built-in name gets the annotated default file for that preset;
            // a gallery name gets the preset's file without its tooling header.
            let text = if let Some(top) = TopPreset::parse(preset) {
                let (cfg, _) = config::parse_with(
                    "",
                    &SCHEMAS,
                    &Overlay { preset: Some(top), ..Default::default() },
                );
                crate::docs::config_toml(&cfg, true)
            } else {
                {
                    let Some(p) = crate::gallery::find(preset) else {
                        // A typo, not a fault: one line, no report (bug 7).
                        eprintln!(
                            "unknown preset {preset:?}; expected default, minimal, full, compact or a gallery name ({})",
                            crate::gallery::PRESETS
                                .iter()
                                .map(|p| p.name)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        return Err(Quiet.into());
                    };
                    crate::gallery::body(p.source)
                }
            };
            let Some(target) = config_target(config_path) else {
                return Err(no_home("--config <FILE>", "the config goes"));
            };
            if target.exists() && !force {
                eprintln!("{} exists; pass --force to overwrite", target.display());
                return Err(Quiet.into());
            }
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            std::fs::write(&target, text)
                .with_context(|| format!("writing {}", target.display()))?;
            writeln!(stdout, "wrote {}", target.display())?;
        }
    }
    Ok(())
}
