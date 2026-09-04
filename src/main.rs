//! garnish — a fast, cached status line for Claude Code.
//!
//! This is the Phase 0 scaffold: the binary reads stdin and prints a single
//! placeholder line. See `PLAN.md` for what comes next.

use std::io::{Read, Write};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "garnish v{} ({} bytes of payload)", env!("CARGO_PKG_VERSION"), input.len())?;
    Ok(())
}
