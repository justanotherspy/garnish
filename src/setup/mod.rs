//! `garnish setup` (SPEC § 14): a full-screen picker and builder with a live
//! preview at the real box width, and its non-interactive twin
//! `setup --preset <name> [--install]`.
//!
//! The screen lives here and is never entered on the render path. The
//! draft it edits is the config file as a table ([`draft`]), every editor
//! is generated from the schemas ([`form`]), and the preview goes through
//! [`crate::render::render_tree_at`] like a tick, painted into ratatui
//! spans by the same rules as the tick's bytes ([`paint`]).

use std::io::Write as _;
use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;

pub mod app;
pub mod builder;
pub mod draft;
pub mod form;
pub mod fuzzy;
pub mod paint;
pub mod pick;
pub mod preview;
pub mod term;
pub mod ui;

pub use app::{App, Input, Key, Mouse};
pub use draft::Draft;
pub use preview::Preview;

/// What `garnish setup` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args<'a> {
    /// `--preset`: write this preset without opening the screen.
    pub preset: Option<String>,
    /// `--install`: hook garnish up too (with `--preset`).
    pub install: bool,
    /// `--config`.
    pub config_path: Option<&'a Path>,
}

/// Run `garnish setup`.
///
/// # Errors
/// The quiet refusals of SPEC § 5 (no home, an unparsable file, no
/// terminal), or a terminal or file error worth a report.
pub fn run(args: &Args<'_>) -> Result<()> {
    let target = crate::cli::config_target_or_quiet(args.config_path)?;
    if let Some(preset) = &args.preset {
        return preset_twin(preset, &target, args.install, args.config_path);
    }
    if !crate::cli::stdout_is_terminal() {
        eprintln!(
            "garnish setup needs a terminal on stdout; pass --preset <name> to write a preset without one"
        );
        return Err(crate::cli::Quiet.into());
    }
    let draft = Draft::open(Some(target));
    let options = crate::install::Options {
        config_path: crate::config::explicit(args.config_path),
        ..crate::install::Options::default()
    };
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let home = crate::claude_settings::home_dir();
    let mut app = App::new(draft, Preview::live(), options, home, no_color);
    term::run(&mut app)?;
    Ok(())
}

/// `setup --preset <name> [--install]`: the preset is written with the § 5
/// backup (as `config init --preset <name> --force` does) and, with
/// `--install`, the settings are wired up, for scripts and the skill.
fn preset_twin(
    preset: &str,
    target: &Path,
    install: bool,
    config_path: Option<&Path>,
) -> Result<()> {
    let text = crate::cli::preset_text(preset)?;
    let backup = crate::install::write_config(target, &text, true).map_err(crate::cli::refusal)?;
    writeln!(
        std::io::stdout().lock(),
        "{}",
        crate::install::wrote_line(target, backup.as_deref())
    )?;
    if install {
        let options = crate::install::Options {
            config_path: crate::config::explicit(config_path),
            ..crate::install::Options::default()
        };
        let steps = crate::install::Steps::plan(&options).map_err(crate::cli::refusal)?;
        crate::cli::print_install(&steps, false)?;
    }
    Ok(())
}

/// The screen drawn into a `width × height` buffer, as text: one line per
/// terminal row, trailing spaces trimmed. The snapshot tests compare this
/// with the goldens under `tests/golden/setup/`.
///
/// # Panics
/// Never: a `TestBackend` cannot fail to draw.
#[must_use]
pub fn snapshot(app: &mut App, width: u16, height: u16) -> String {
    let backend = ratatui::backend::TestBackend::new(width, height);
    // A `TestBackend` cannot fail: its error type is `Infallible`.
    let Ok(mut terminal) = ratatui::Terminal::new(backend);
    app.input(Input::Resize(width, height));
    let Ok(_drawn) = terminal.draw(|frame| app.draw(frame));
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut line = String::new();
        let mut x = 0_u16;
        while x < width {
            let Some(cell) = buffer.cell(ratatui::layout::Position::new(x, y)) else { break };
            let symbol = cell.symbol();
            line.push_str(symbol);
            // A wide glyph owns the cell after it, which the buffer leaves
            // empty; skip it so the text keeps the screen's own cells.
            let w = crate::ansi::display_width(symbol).max(1);
            x = x.saturating_add(u16::try_from(w).unwrap_or(1));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// A screen for the tests.
///
/// `text` as the config (with no file behind it unless `path` says so), a
/// pinned clock, the install plan aimed at `home` (and at no config file,
/// as `setup` plans it: the draft is the config).
#[must_use]
pub fn for_test(text: &str, path: Option<PathBuf>, home: &Path) -> App {
    let draft = path.map_or_else(|| Draft::from_text(text), |p| Draft::open(Some(p)));
    let options = crate::install::Options {
        settings: Some(home.join(".claude").join("settings.json")),
        write_config: false,
        ..crate::install::Options::default()
    };
    let preview = Preview::new(crate::render::Clock::fixed(), true);
    App::new(draft, preview, options, Some(home.to_path_buf()), false)
}
