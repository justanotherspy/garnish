//! garnish — a fast, cached, beautifully themed status line for Claude Code.
//!
//! The binary in `main.rs` is a thin wrapper over [`cli::run`]. Everything
//! else lives here so it can be unit-tested and benchmarked in-process.

pub mod ansi;
pub mod cache;
pub(crate) mod claude_settings;
pub mod cli;
pub mod config;
pub(crate) mod debug;
pub(crate) mod docs;
pub(crate) mod doctor;
pub mod fixtures;
pub(crate) mod frame;
pub mod gallery;
pub mod git;
pub mod icons;
pub(crate) mod install;
pub mod layout;
pub mod modules;
pub mod num;
pub mod payload;
pub mod render;
pub mod setup;
pub(crate) mod skills;
pub(crate) mod spawn;
pub(crate) mod theme;
pub(crate) mod time;
