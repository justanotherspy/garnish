//! Opt-in diagnostics: with `GARNISH_DEBUG` set, one-line notes are appended
//! to `<cache root>/debug.log` (rotated at 1 MiB). Nothing is written otherwise.

use std::io::Write as _;

/// Environment variable that enables the log.
pub const DEBUG_ENV: &str = "GARNISH_DEBUG";

/// Rotate the log once it grows past this many bytes.
const MAX_BYTES: u64 = 1024 * 1024;

/// Whether logging is enabled (Claude Code's truthy rule).
#[must_use]
pub fn enabled() -> bool {
    crate::claude_settings::env_truthy(std::env::var(DEBUG_ENV).ok().as_ref())
}

/// Append one line to the debug log when enabled.
pub fn log(message: &str) {
    if !enabled() {
        return;
    }
    let root = crate::cache::Cache::from_env();
    let path = root.root().join("debug.log");
    let _ = std::fs::create_dir_all(root.root());
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&path) {
        let _ = writeln!(f, "{} pid={} {message}", crate::time::now_millis(), std::process::id());
    }
}
