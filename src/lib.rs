//! garnish — a fast, cached, beautifully themed status line for Claude Code.
//!
//! The binary in `main.rs` is a thin wrapper over [`cli::run`]. Everything
//! else lives here so it can be unit-tested and benchmarked in-process.

pub mod ansi;
pub mod cache;
pub mod claude_settings;
pub mod cli;
pub mod config;
pub mod debug;
pub mod docs;
pub mod doctor;
pub mod frame;
pub mod gallery;
pub mod git;
pub mod icons;
pub mod install;
pub mod modules;
pub mod num;
pub mod payload;
pub mod render;
pub mod spawn;
pub mod theme;
pub mod time;
