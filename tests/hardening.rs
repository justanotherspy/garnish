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
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_NOW", "1738425600")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "100")
        .env("GARNISH_MANAGED_SETTINGS", "")
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
        ("out-of-range", br#"{"cost":{"total_duration_ms":1e400}}"#),
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

/// CLAUDE.md § Style: a render error is a `⚠ garnish:` row on stdout *and a
/// note on stderr*. A bad payload dropped the parser's message, so nothing
/// anywhere said where the JSON went wrong; the note and the debug log now
/// carry it, and stdout keeps the one line SPEC § 5 pins.
#[test]
fn a_bad_payload_says_why_on_stderr_and_in_the_debug_log() {
    let dir = tempfile::tempdir().unwrap();
    let (out, err, ok) = tick(br#"{"session_id": "s","#, &[("GARNISH_DEBUG", "1")], dir.path());
    assert!(ok, "{err}");
    assert_eq!(out, "⚠ garnish: bad payload\n");
    assert!(err.contains("bad payload") && err.contains("line 1"), "{err:?}");
    let log = std::fs::read_to_string(dir.path().join("cache/debug.log")).unwrap_or_default();
    assert!(log.contains("bad payload") && log.contains("line 1"), "{log:?}");
    let (_, err, _) = tick(b"[1,2]", &[], dir.path());
    assert!(err.contains("bad payload") && err.contains("not a JSON object"), "{err:?}");
}

/// SPEC § 5: a payload field of the wrong type is absent, alone. One badge's
/// field changing type in a Claude Code release used to blank every row
/// behind `⚠ garnish: bad payload`.
#[test]
fn a_wrong_typed_payload_field_loses_only_itself() {
    let dir = tempfile::tempdir().unwrap();
    let base = r#""session_id":"s","model":{"display_name":"Opus"}"#;
    let well_typed = format!(r#"{{{base},"workspace":{{"added_dirs":["/a"]}},"cost":{{}}}}"#);
    let (good, _, ok) = tick(well_typed.as_bytes(), &[], dir.path());
    assert!(ok && good.contains("Opus"), "{good}");
    let drifted = format!(
        r#"{{{base},"pr":{{"number":true}},"thinking":true,"effort":"high","session_name":7,"exceeds_200k_tokens":"false","workspace":{{"added_dirs":["/a",null]}},"cost":{{"total_cost_usd":{{}}}}}}"#
    );
    let (out, err, ok) = tick(drifted.as_bytes(), &[], dir.path());
    assert!(ok, "{out}{err}");
    assert!(out.contains("Opus") && !out.contains("bad payload"), "{out}");
    assert_eq!(out, good, "the drifted fields are absent, the rest renders as before");
}

/// An empty `workspace.current_dir` is no directory: `cwd` still names the
/// path (and the repository and settings chain behind it), where the empty
/// string used to win and blank the path row.
#[test]
fn an_empty_current_dir_never_shadows_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let payload =
        br#"{"session_id":"s","cwd":"/nowhere/projects/garnish","workspace":{"current_dir":""}}"#;
    let (out, _, ok) = tick(payload, &[], dir.path());
    assert!(ok && out.contains("projects/garnish"), "{out}");
}

/// An absurd number is a number, printed bounded: `1e300` dollars used to
/// print about 300 digits and a `"inf"` string `$infk` (SPEC § 5).
#[test]
fn an_absurd_payload_number_prints_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let (plain, _, _) = tick(br#"{"session_id":"s","cost":{"total_cost_usd":1}}"#, &[], dir.path());
    for (usd, shown) in [("1e300", "$100.0k"), ("\"inf\"", "$0.00"), ("\"NaN\"", "$0.00")] {
        let payload = format!(r#"{{"session_id":"s","cost":{{"total_cost_usd":{usd}}}}}"#);
        let (out, err, ok) = tick(payload.as_bytes(), &[], dir.path());
        assert!(ok, "{usd}: {out}{err}");
        assert!(!out.contains("bad payload") && !out.contains("inf"), "{usd}: {out}");
        assert!(out.contains(shown), "{usd}: {out}");
        assert_eq!(out.lines().count(), plain.lines().count(), "{usd}: {out}");
    }
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
    // SPEC § 9: `GARNISH_COLUMNS` is the width when `COLUMNS` is absent,
    // and never wins over it.
    let (out, _, ok) =
        tick(PAYLOAD.as_bytes(), &[("COLUMNS", ""), ("GARNISH_COLUMNS", "60")], dir.path());
    assert!(ok, "{out}");
    assert!(out.lines().all(|l| width(l) <= 56), "{out}");
    assert!(out.lines().any(|l| width(l) == 56), "the hook must set the width: {out}");
    let (out, _, ok) =
        tick(PAYLOAD.as_bytes(), &[("COLUMNS", "100"), ("GARNISH_COLUMNS", "60")], dir.path());
    assert!(ok && out.lines().any(|l| width(l) == 96), "COLUMNS wins: {out}");
    // Neither set: the 120-column default of SPEC § 2.1.
    let (out, _, ok) =
        tick(PAYLOAD.as_bytes(), &[("COLUMNS", ""), ("GARNISH_COLUMNS", "")], dir.path());
    assert!(ok && out.lines().any(|l| width(l) == 116), "{out}");

    let (out, err, ok) = tick(PAYLOAD.as_bytes(), &[("GARNISH_NOW", "yesterday")], dir.path());
    assert!(ok, "exit status\n{out}\n{err}");
    assert_eq!(out.lines().count(), 4, "{out}\n{err}");
    assert!(err.contains("GARNISH_NOW"), "{err}");
    let (out, err, ok) = tick(PAYLOAD.as_bytes(), &[("HOME", ""), ("TZ", "Not/AZone")], dir.path());
    assert!(ok && out.lines().count() == 4, "{out}");
    assert!(err.contains("TZ=\"Not/AZone\" names no time zone"), "{err}");
    // A POSIX rule is a zone (SPEC § 3.4); it used to fall through to
    // `/etc/localtime`, UTC in most containers, without a word.
    let (out, err, ok) = tick(PAYLOAD.as_bytes(), &[("TZ", "JST-9")], dir.path());
    assert!(ok && out.contains("01:00:00") && !err.contains("TZ="), "{out}{err}");
}

/// SPEC § 5: the tick prints and exits 0 whatever the repository holds. A
/// `.git/HEAD`, `config` or `packed-refs` that is a FIFO (tar extracts one
/// for anybody) used to block the tick in `open` until the harness gave up,
/// so the status line never updated in that directory again.
#[test]
fn a_fifo_in_the_git_directory_never_hangs_the_tick() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let git = work.join(".git");
    for sub in ["objects", "refs/heads"] {
        std::fs::create_dir_all(git.join(sub)).unwrap();
    }
    for name in ["HEAD", "config", "packed-refs", "commondir"] {
        let made = Command::new("mkfifo").arg(git.join(name)).status();
        if !made.is_ok_and(|s| s.success()) {
            return; // no mkfifo here: nothing to test with
        }
    }
    let w = work.display();
    let payload = format!(
        r#"{{"cwd":"{w}","session_id":"s","workspace":{{"current_dir":"{w}","project_dir":"{w}"}},"worktree":{{"branch":"main"}}}}"#
    );
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.env("HOME", dir.path())
        .env("GARNISH_CACHE_DIR", dir.path().join("cache"))
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .env_remove("GARNISH_CONFIG")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_NOW", "1738425600")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "100")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
    let started = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            panic!("the tick hung on a FIFO under .git");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("main"), "the payload's branch still shows: {text}");
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
    // The row names the file first, and a temporary path can be long.
    let missing = dir.path().join("nope.toml").to_string_lossy().into_owned();
    let env = [("GARNISH_CONFIG", missing.as_str()), ("COLUMNS", "400")];
    let (out, _, ok) = tick(PAYLOAD.as_bytes(), &env, dir.path());
    assert!(ok && out.lines().last().unwrap().contains("cannot read"), "{out}");

    // A cache root that cannot be created for *any* uid: a regular file
    // stands where the parent directory would be, so `create_dir_all` gets
    // ENOTDIR. A read-only directory does not do it — root ignores the mode
    // bits, and this project's own containers run as root, so that case
    // silently exercised an ordinary writable cache.
    let blocked = dir.path().join("not-a-dir");
    std::fs::write(&blocked, "").unwrap();
    let cache = blocked.join("cache").to_string_lossy().into_owned();
    let (out, err, ok) =
        tick(PAYLOAD.as_bytes(), &[("GARNISH_CACHE_DIR", cache.as_str())], dir.path());
    // The promise of SPEC § 5: the normal rows render, nothing is said, and
    // nothing was created where the file is.
    assert!(ok, "{out}{err}");
    assert_eq!(out.lines().count(), 4, "{out}");
    assert!(!out.contains("⚠ garnish:") && !out.contains("! garnish:"), "{out}");
    assert!(blocked.is_file(), "the cache root must not have been created");
}
