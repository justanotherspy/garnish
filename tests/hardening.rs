//! Hostile-environment tests: the tick must print something sensible and
//! exit 0 whatever stdin, the environment, the config or the cache look like.

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

const PAYLOAD: &str = include_str!("fixtures/payloads/subscription-full.json");

fn tick(stdin: &[u8], env: &[(&str, &str)], home: &Path) -> (String, String, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.env("HOME", home)
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("GARNISH_CONFIG")
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_NOW", "1738425600")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "100")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        if v.is_empty() {
            cmd.env_remove(k);
        } else {
            cmd.env(k, v);
        }
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

#[test]
fn hostile_stdin_never_blanks_the_line_or_fails() {
    let dir = tempfile::tempdir().unwrap();
    for (name, input) in [
        ("empty", &b""[..]),
        ("whitespace", b"  \n"),
        ("array", b"[1,2]"),
        ("truncated", &PAYLOAD.as_bytes()[..PAYLOAD.len() / 2]),
        ("non-utf8", b"\xff\xfe{}"),
        ("null-object", b"null"),
        ("huge-number", br#"{"cost":{"total_duration_ms":1e400}}"#),
        ("deep", br#"{"a":{"b":{"c":{"d":{"e":1}}}}}"#),
    ] {
        let (out, _, ok) = tick(input, &[], dir.path());
        assert!(ok, "{name}: exit code");
        assert!(!out.trim().is_empty(), "{name}: printed nothing");
        assert!(out.ends_with('\n'), "{name}: no trailing newline");
    }
    let (out, _, _) = tick(b"[1,2]", &[], dir.path());
    assert_eq!(out, "⚠ garnish: bad payload\n");
    let (out, _, ok) = tick(b"\xff\xfe{}", &[], dir.path());
    assert!(ok);
    assert_eq!(out, "⚠ garnish: bad payload\n", "invalid UTF-8 is a bad payload, not a crash");
    let (out, _, _) =
        tick("{\"model\":{\"display_name\":\"Op\u{fffd}s\"}}".as_bytes(), &[], dir.path());
    assert!(out.contains("Op\u{fffd}s"), "{out}");
}

#[test]
fn hostile_environment_is_tolerated() {
    let dir = tempfile::tempdir().unwrap();
    for cols in ["0", "1", "5", "12", "abc", "-4", "100000", ""] {
        let (out, _, ok) = tick(PAYLOAD.as_bytes(), &[("COLUMNS", cols)], dir.path());
        assert!(ok, "COLUMNS={cols}");
        // Claude Code's box is 4 cells narrower than COLUMNS (SPEC § 2.1); floor 10.
        let limit: usize = cols.parse::<usize>().map_or(116, |c| c.saturating_sub(4).max(10));
        for line in out.lines() {
            assert!(width(line) <= limit, "COLUMNS={cols}: {line:?} is {} wide", width(line));
        }
    }
    let (out, err, ok) = tick(PAYLOAD.as_bytes(), &[("GARNISH_NOW", "yesterday")], dir.path());
    assert!(ok, "exit status\n{out}\n{err}");
    assert_eq!(out.lines().count(), 4, "{out}\n{err}");
    assert!(err.contains("GARNISH_NOW"), "{err}");
    let (out, _, ok) = tick(PAYLOAD.as_bytes(), &[("HOME", ""), ("TZ", "Not/AZone")], dir.path());
    assert!(ok && out.lines().count() == 4, "{out}");
}

#[test]
fn unreadable_config_and_unwritable_cache_still_render() {
    let dir = tempfile::tempdir().unwrap();
    let as_dir = dir.path().join("garnish.toml");
    std::fs::create_dir_all(&as_dir).unwrap();
    let cfg = as_dir.to_string_lossy().into_owned();
    let (out, _, ok) = tick(PAYLOAD.as_bytes(), &[("GARNISH_CONFIG", cfg.as_str())], dir.path());
    assert!(ok, "{out}");
    assert!(out.lines().last().unwrap().contains("config"), "{out}");
    assert_eq!(out.lines().count(), 5, "{out}");
    let missing = dir.path().join("nope.toml").to_string_lossy().into_owned();
    let (out, _, ok) =
        tick(PAYLOAD.as_bytes(), &[("GARNISH_CONFIG", missing.as_str())], dir.path());
    assert!(ok && out.lines().last().unwrap().contains("cannot read"), "{out}");

    let ro = dir.path().join("ro");
    std::fs::create_dir_all(&ro).unwrap();
    let mut perms = std::fs::metadata(&ro).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&ro, perms).unwrap();
    let cache = ro.join("cache").to_string_lossy().into_owned();
    let (out, _, ok) =
        tick(PAYLOAD.as_bytes(), &[("GARNISH_CACHE_DIR", cache.as_str())], dir.path());
    assert!(ok && out.lines().count() >= 4, "{out}");
}
