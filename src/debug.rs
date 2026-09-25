//! Opt-in diagnostics: with `GARNISH_DEBUG` set, one-line notes are appended
//! to `<cache root>/debug.log` (rotated at 1 MiB).
//!
//! Nothing is written otherwise, and never to stdout: a tick's stdout is the
//! status line (SPEC § 5). `garnish doctor` prints the tail of the file, so
//! a line is written for a reader who cannot reproduce the tick — the tick's
//! own shape (§ 7) and the failures a row cannot show, such as a worker that
//! would not start.

use std::io::Write as _;
use std::path::Path;

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
    if enabled() {
        let cache = crate::cache::Cache::from_env();
        if cache.refused().is_none() {
            append(cache.root(), message);
        }
    }
}

/// Append one stamped line to `<root>/debug.log`, rotating it first when it
/// has grown past 1 MiB. Every failure is ignored: diagnostics must
/// never change what a tick prints or whether it succeeds.
///
/// Callers go through [`log`], which checks [`enabled`]; this is the part
/// that can be tested without setting a process-wide variable.
pub fn append(root: &Path, message: &str) {
    let path = root.join("debug.log");
    let _ = std::fs::create_dir_all(root);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&path) {
        let _ = writeln!(f, "{} pid={} {message}", crate::time::now_millis(), std::process::id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_carries_the_clock_the_pid_and_the_message() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), "spawn sync failed: boom");
        append(dir.path(), "second");
        let text = std::fs::read_to_string(dir.path().join("debug.log")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        let mut parts = lines[0].splitn(3, ' ');
        assert!(parts.next().unwrap().parse::<i64>().is_ok(), "{text}");
        assert_eq!(parts.next().unwrap(), format!("pid={}", std::process::id()));
        assert_eq!(parts.next().unwrap(), "spawn sync failed: boom");
        assert!(lines[1].ends_with(" second"));
    }

    /// The file is rotated once, to `debug.log.1`, rather than growing
    /// without bound in a session that leaves the hook on.
    #[test]
    fn the_log_rotates_past_the_size_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, "x".repeat(usize::try_from(MAX_BYTES).unwrap() + 1)).unwrap();
        append(dir.path(), "after");
        let rotated = std::fs::read_to_string(dir.path().join("debug.log.1")).unwrap();
        assert_eq!(rotated.len(), usize::try_from(MAX_BYTES).unwrap() + 1);
        let fresh = std::fs::read_to_string(&path).unwrap();
        assert_eq!(fresh.lines().count(), 1, "{fresh}");
        assert!(fresh.ends_with(" after\n"));
    }

    /// A root that cannot be created (a file sits where the directory would
    /// go) loses the line instead of failing the tick.
    #[test]
    fn an_unwritable_root_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-dir");
        std::fs::write(&blocked, "").unwrap();
        append(&blocked, "dropped");
        assert!(!blocked.join("debug.log").exists());
    }
}
