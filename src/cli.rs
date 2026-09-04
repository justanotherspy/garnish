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
    /// Width override (defaults to `COLUMNS`, then 120).
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
    /// Inspect or create the configuration.
    Config {
        /// What to do.
        #[command(subcommand)]
        action: ConfigAction,
    },
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
        /// Top-level preset to start from.
        #[arg(long, default_value = "default")]
        preset: String,
    },
}

/// Entry point used by `main`.
///
/// # Errors
/// Returns an error for subcommand failures; `render` itself never fails.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.as_deref();
    match cli.command.unwrap_or(Command::Render) {
        Command::Render => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).context("reading stdin")?;
            let req = Request {
                payload_json: &input,
                config_path,
                overlay: Overlay::default(),
                columns: env_columns(),
                no_color: std::env::var_os("NO_COLOR").is_some(),
            };
            let out = render::render(&req);
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(out.as_bytes())?;
            Ok(())
        }
        Command::Preview { path, args } => preview(&path, config_path, &args),
        Command::Modules => {
            let mut stdout = std::io::stdout().lock();
            for s in SCHEMAS.iter() {
                writeln!(stdout, "{:<13} {}", s.id, s.summary)?;
            }
            Ok(())
        }
        Command::Config { action } => config_cmd(&action, config_path),
    }
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
            let p = config::locate(config_path).unwrap_or_else(config::default_path);
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
                    return Err(eyre!("{} problem(s) found", loaded.errors.len()));
                }
            }
        }
        ConfigAction::Show => {
            let loaded = config::load(config_path, &SCHEMAS);
            stdout.write_all(crate::docs::config_toml(&loaded.config, false).as_bytes())?;
        }
        ConfigAction::Init { force, preset } => {
            let top = TopPreset::parse(preset).ok_or_else(|| eyre!("unknown preset {preset:?}"))?;
            let target = config_path.map_or_else(config::default_path, Path::to_path_buf);
            if target.exists() && !force {
                return Err(eyre!("{} exists; pass --force to overwrite", target.display()));
            }
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            let (cfg, _) = config::parse_with(
                "",
                &SCHEMAS,
                &Overlay { preset: Some(top), ..Default::default() },
            );
            std::fs::write(&target, crate::docs::config_toml(&cfg, true))
                .with_context(|| format!("writing {}", target.display()))?;
            writeln!(stdout, "wrote {}", target.display())?;
        }
    }
    Ok(())
}
