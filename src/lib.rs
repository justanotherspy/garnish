//! garnish — a fast, cached, beautifully themed status line for Claude Code.
//!
//! The binary in `main.rs` is a thin wrapper over [`cli::run`]. Everything
//! else lives here so it can be unit-tested and benchmarked in-process.

pub mod ansi;
pub mod cli;
pub mod num;
pub mod payload;
pub mod time;
