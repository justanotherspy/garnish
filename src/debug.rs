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

/// Whether logging is enabled (the boolean hook rule, SPEC § 9).
#[must_use]
pub fn enabled() -> bool {
    crate::claude_settings::env_flag(DEBUG_ENV) == Some(true)
}

/// Write one line to stderr, ignoring a failed write.
///
/// What the render path says on stderr goes through here (SPEC § 5):
/// `eprintln!` panics when the write fails, and with a stderr nobody reads
/// (a pipe whose reader is gone) that panic cost the tick its row, and an
/// empty stdout clears the status line.
pub fn stderr_line(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
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

    /// SPEC § 5: the render path writes to stderr through [`stderr_line`]
    /// alone, since `eprint!` and `eprintln!` panic on a failed write and a
    /// panic there cost the tick its row. The macros are left to the files
    /// whose writers run off the render path: the other commands in
    /// `cli.rs` (whose render-path functions call [`stderr_line`]) and
    /// `setup/`.
    #[test]
    fn nothing_on_the_render_path_writes_stderr_with_a_macro_that_panics() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let macros = [concat!("eprint", "!("), concat!("eprint", "ln!(")];
        let mut dirs = vec![src.clone()];
        let mut scanned = 0_u32;
        while let Some(dir) = dirs.pop() {
            for path in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()) {
                if path == src.join("setup") || path == src.join("cli.rs") {
                    continue;
                }
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    scanned += 1;
                    let source = std::fs::read_to_string(&path).unwrap();
                    for m in macros {
                        assert!(!source.contains(m), "{}: {m}", path.display());
                    }
                }
            }
        }
        assert!(scanned > 20, "the scan found {scanned} files");
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
