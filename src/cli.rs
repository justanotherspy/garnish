//! Command-line interface.

use std::io::{Read, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result};

use crate::payload::Payload;

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

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Render the status line from the JSON payload on stdin (the default).
    Render,
    /// Render a payload fixture file (or every fixture in a directory).
    Preview {
        /// Fixture file, or a directory of `*.json` fixtures.
        path: PathBuf,
    },
    /// List the built-in modules.
    Modules,
}

/// Entry point used by `main`.
///
/// # Errors
/// Returns an error for subcommand failures; `render` itself never fails.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Render) {
        Command::Render => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).context("reading stdin")?;
            let out = render_text(&input);
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(out.as_bytes())?;
            Ok(())
        }
        Command::Preview { path } => preview(&path),
        Command::Modules => {
            let mut stdout = std::io::stdout().lock();
            writeln!(stdout, "(modules arrive in Phase 3)")?;
            Ok(())
        }
    }
}

/// Render a payload string to the status line text (with trailing newline).
///
/// Never fails: a bad payload renders a warning line instead.
#[must_use]
pub fn render_text(input: &str) -> String {
    Payload::parse(input)
        .map_or_else(|_| "⚠ garnish: bad payload\n".to_owned(), |p| placeholder_line(&p))
}

fn placeholder_line(p: &Payload) -> String {
    let model = p.model.as_ref().and_then(|m| m.display_name.as_deref()).unwrap_or("?");
    let pct = p
        .context_window
        .as_ref()
        .and_then(|c| c.used_percentage)
        .map_or_else(|| "–".to_owned(), |v| format!("{}%", crate::num::round_to_u64(v)));
    let dir = p.current_dir().unwrap_or("?");
    format!("{dir} · {model} · {pct}\n")
}

fn preview(path: &PathBuf) -> Result<()> {
    let mut files: Vec<PathBuf> = if path.is_dir() {
        std::fs::read_dir(path)
            .with_context(|| format!("reading {}", path.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect()
    } else {
        vec![path.clone()]
    };
    files.sort();
    let mut stdout = std::io::stdout().lock();
    for file in files {
        let input = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        writeln!(stdout, "── {name}")?;
        stdout.write_all(render_text(&input).as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_payload_renders_warning_not_error() {
        assert_eq!(render_text("not json"), "⚠ garnish: bad payload\n");
    }

    #[test]
    fn placeholder_uses_payload_fields() {
        let out = render_text(
            r#"{"cwd":"/x","model":{"display_name":"Opus"},"context_window":{"used_percentage":41.6}}"#,
        );
        assert_eq!(out, "/x · Opus · 42%\n");
    }
}
