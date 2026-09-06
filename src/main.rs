//! garnish — a fast, cached status line for Claude Code.

fn main() -> color_eyre::Result<std::process::ExitCode> {
    garnish::cli::run()
}
