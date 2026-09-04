//! garnish — a fast, cached status line for Claude Code.

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    garnish::cli::run()
}
