//! End-to-end tests of the cache / worker / spawn machinery through the real
//! binary: a temp repository with a local bare origin, a private cache dir, a
//! frozen clock, and `GARNISH_NO_SPAWN` so ticks log intended spawns instead
//! of starting processes. Tests named `cache_*`, `worker_*`, `spawn_*` run
//! serially (see `.config/nextest.toml`).

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const NOW: &str = "1738425600";

const fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_garnish")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

struct Env {
    _dir: tempfile::TempDir,
    work: PathBuf,
    cache: PathBuf,
}

/// Repo with a local bare origin, `main` pushed, plus one unpushed commit.
fn setup() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin.git");
    let work = dir.path().join("work");
    git(dir.path(), &["init", "--bare", "-q", "-b", "main", origin.to_str().unwrap()]);
    git(dir.path(), &["clone", "-q", origin.to_str().unwrap(), work.to_str().unwrap()]);
    git(&work, &["checkout", "-q", "-b", "main"]);
    std::fs::write(work.join("a.txt"), "a\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "one"]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    std::fs::write(work.join("b.txt"), "b\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "two"]);
    let cache = dir.path().join("cache");
    Env { _dir: dir, work, cache }
}

fn payload(work: &Path) -> String {
    format!(
        r#"{{"cwd":"{w}","session_id":"sess-worker","workspace":{{"current_dir":"{w}","project_dir":"{w}","added_dirs":[]}},"model":{{"id":"m","display_name":"Opus"}},"cost":{{"total_cost_usd":0.1,"total_duration_ms":1000,"total_api_duration_ms":100,"total_lines_added":0,"total_lines_removed":0}},"context_window":{{"context_window_size":1000000,"used_percentage":10}}}}"#,
        w = work.display()
    )
}

fn garnish(
    env: &Env,
    args: &[&str],
    stdin: Option<&str>,
    extra_env: &[(&str, &str)],
) -> (String, String, bool) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("GARNISH_CACHE_DIR", &env.cache)
        .env("GARNISH_NOW", NOW)
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_CONFIG", env.work.join("garnish.toml"))
        .env("COLUMNS", "120")
        .env("NO_COLOR", "1")
        .env("HOME", env.work.parent().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn().unwrap();
    if let Some(s) = stdin {
        child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn config(env: &Env, text: &str) {
    std::fs::write(env.work.join("garnish.toml"), text).unwrap();
}

fn spawns(env: &Env) -> Vec<String> {
    std::fs::read_to_string(env.cache.join("spawns.log"))
        .map(|s| s.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn repo_cache_files(env: &Env) -> Vec<String> {
    let repos = env.cache.join("repos");
    let Ok(dirs) = std::fs::read_dir(&repos) else { return Vec::new() };
    let mut names = Vec::new();
    for d in dirs.flatten() {
        for f in std::fs::read_dir(d.path()).unwrap().flatten() {
            names.push(f.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

const ONE_LINE: &str = "preset = \"minimal\"\n[[line]]\nmodules = [\"branch\", \"sync\"]\n[modules.branch]\npreset = \"full\"\n[modules.sync]\npreset = \"full\"\n";

#[test]
fn worker_first_tick_spawns_then_refresh_fills_cache() {
    let env = setup();
    config(&env, ONE_LINE);
    let (out, _, ok) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(ok);
    // Branch is read directly, counts are not yet known, one spawn per cached module.
    assert!(out.contains("main"), "{out}");
    assert!(!out.contains("⇡"), "{out}");
    let s = spawns(&env);
    assert_eq!(s.len(), 2, "{s:?}");
    assert!(
        s.iter().any(|l| l.contains("--module branch"))
            && s.iter().any(|l| l.contains("--module sync"))
    );
    // Only Linux hands the lock to the worker; elsewhere the worker takes it.
    let handover = cfg!(target_os = "linux");
    assert!(s.iter().all(|l| l.ends_with("--lock-held") == handover), "{s:?}");
    let files = repo_cache_files(&env);
    let expected: Vec<&str> = if handover { vec!["branch.lock", "sync.lock"] } else { vec![] };
    assert_eq!(files, expected, "{files:?}");

    // Run the worker synchronously exactly as the logged spawn would; it writes
    // the entry and releases the lock.
    let w = env.work.to_str().unwrap().to_owned();
    for module in ["sync", "branch"] {
        let mut args = vec!["refresh", "--module", module, "--session", "sess-worker", "--cwd", &w];
        if handover {
            args.push("--lock-held");
        }
        let (_, err, ok) = garnish(&env, &args, None, &[]);
        assert!(ok, "{err}");
    }
    let files = repo_cache_files(&env);
    assert_eq!(files, vec!["branch.cache", "sync.cache"], "{files:?}");

    // Next tick renders fresh counts (one ahead) and spawns nothing new.
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1"), "{out}");
    assert!(!out.contains("⟳"), "{out}");
    assert_eq!(spawns(&env).len(), 2);
}

#[test]
fn worker_dirty_flag_and_stale_marker() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    let (_, err, ok) =
        garnish(&env, &["refresh", "--all", "--session", "sess-worker", "--cwd", &w], None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(!out.contains('●'), "{out}");
    std::fs::write(env.work.join("a.txt"), "changed\n").unwrap();
    let (_, err, ok) = garnish(
        &env,
        &["refresh", "--module", "branch", "--session", "sess-worker", "--cwd", &w],
        None,
        &[],
    );
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains('●'), "{out}");
    // Advance the clock past the TTL: the value is still shown, dimmed with ⟳, and a spawn is logged.
    let later = (NOW.parse::<u64>().unwrap() + 60).to_string();
    let before = spawns(&env).len();
    let (out, _, _) =
        garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", later.as_str())]);
    assert!(out.contains('●') && out.contains('⟳'), "{out}");
    assert_eq!(spawns(&env).len(), before + 2, "{:?}", spawns(&env));
}

#[test]
fn cache_live_lock_suppresses_spawn_and_dead_lock_is_reclaimed() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    let (_, err, ok) =
        garnish(&env, &["refresh", "--all", "--session", "sess-worker", "--cwd", &w], None, &[]);
    assert!(ok, "{err}");
    // Entries are past their TTL, but a live lock (this test's pid, stamped
    // now) says a worker is already on it: the tick must not spawn.
    let later = (NOW.parse::<u64>().unwrap() + 60).to_string();
    let now_ms =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    write_locks(&env, &format!("{} {now_ms}", std::process::id()));
    garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", later.as_str())]);
    assert_eq!(spawns(&env).len(), 0, "{:?}", spawns(&env));
    // Locks stale by age are reclaimed: the tick spawns again.
    write_locks(&env, "4000000000 1");
    garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", later.as_str())]);
    assert_eq!(spawns(&env).len(), 2, "{:?}", spawns(&env));
}

/// Overwrite every module lock in the repo cache with `text`.
fn write_locks(env: &Env, text: &str) {
    let repos = env.cache.join("repos");
    for d in std::fs::read_dir(&repos).unwrap().flatten() {
        for module in ["branch", "sync"] {
            std::fs::write(d.path().join(format!("{module}.lock")), text).unwrap();
        }
    }
}

#[test]
fn spawn_thirty_two_concurrent_ticks_produce_one_worker_per_module() {
    let env = setup();
    config(&env, ONE_LINE);
    let p = payload(&env.work);
    let children: Vec<_> = (0..32)
        .map(|_| {
            let mut child = Command::new(bin())
                .env("GARNISH_CACHE_DIR", &env.cache)
                .env("GARNISH_NOW", NOW)
                .env("GARNISH_NO_SPAWN", "1")
                .env("GARNISH_CONFIG", env.work.join("garnish.toml"))
                .env("COLUMNS", "120")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(p.as_bytes()).unwrap();
            child
        })
        .collect();
    for c in children {
        assert!(c.wait_with_output().unwrap().status.success());
    }
    let s = spawns(&env);
    assert_eq!(s.len(), 2, "{s:?}");
}

#[test]
fn worker_slow_git_never_blocks_a_tick_and_records_failure() {
    let env = setup();
    config(&env, ONE_LINE);
    // A fake git that hangs, first on PATH for the worker only.
    let shim = env.work.parent().unwrap().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    let fake = shim.join("git");
    std::fs::write(&fake, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", shim.display(), std::env::var("PATH").unwrap_or_default());

    // Ticks never touch git: fast even with a hanging git on PATH.
    let started = Instant::now();
    let (out, _, ok) = garnish(&env, &[], Some(&payload(&env.work)), &[("PATH", path.as_str())]);
    assert!(ok && out.contains("main"), "{out}");
    assert!(started.elapsed() < Duration::from_secs(2), "tick took {:?}", started.elapsed());

    // The worker gives up after its 2 s timeout and records an err entry…
    let w = env.work.to_str().unwrap().to_owned();
    let started = Instant::now();
    let (_, err, ok) = garnish(
        &env,
        &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w, "--lock-held"],
        None,
        &[("PATH", path.as_str())],
    );
    assert!(ok, "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
    let entry = std::fs::read_dir(env.cache.join("repos")).unwrap().flatten().find_map(|d| {
        let p = d.path().join("sync.cache");
        std::fs::read_to_string(p).ok()
    });
    let entry = entry.expect("sync.cache written");
    assert!(entry.starts_with("v1 ") && entry.lines().next().unwrap().ends_with(" err"), "{entry}");
    assert!(entry.contains("timed out"), "{entry}");

    // …which the next tick shows as a failure marker without blocking.
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[("PATH", path.as_str())]);
    assert!(out.contains('✗'), "{out}");
}

#[test]
fn cache_tick_killed_midway_does_not_corrupt_entries() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    garnish(&env, &["refresh", "--all", "--session", "sess-worker", "--cwd", &w], None, &[]);
    // A leftover temp file and a truncated entry must be ignored, never trusted.
    let repo_dir =
        std::fs::read_dir(env.cache.join("repos")).unwrap().flatten().next().unwrap().path();
    std::fs::write(repo_dir.join(".sync.tmp.999"), "v1 1 1 ok\nahead=99\n").unwrap();
    std::fs::write(repo_dir.join("sync.cache"), "v1 1738425600000 5000 ok\nahead=7\nbehind=0")
        .unwrap();
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(!out.contains("⇡7") && !out.contains("⇡99"), "{out}");
}

#[test]
fn gc_subcommand_sweeps_idle_sessions() {
    let env = setup();
    let old = env.cache.join("sessions").join("ancient");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("m.cache"), "v1 1 1 ok\n").unwrap();
    let t = std::time::SystemTime::now() - Duration::from_hours(48);
    std::fs::File::options()
        .write(true)
        .open(old.join("m.cache"))
        .unwrap()
        .set_modified(t)
        .unwrap();
    // File ages are wall clock: a frozen or future GARNISH_NOW must not matter.
    let future = (NOW.parse::<u64>().unwrap() + 100_000_000).to_string();
    let live = env.cache.join("sessions").join("live");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join("m.cache"), "v1 1 1 ok\n").unwrap();
    let (out, _, ok) = garnish(&env, &["gc"], None, &[("GARNISH_NOW", future.as_str())]);
    assert!(ok && out.contains("removed 1"), "{out}");
    assert!(!old.exists());
    assert!(live.exists(), "a live session dir survives gc under a future clock");
}

#[test]
fn worker_failed_entry_is_not_retried_every_tick_and_branch_change_invalidates() {
    let env = setup();
    config(&env, ONE_LINE);
    let shim = env.work.parent().unwrap().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    let fake = shim.join("git");
    std::fs::write(&fake, "#!/bin/sh\necho 'fatal: nope' >&2\nexit 128\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", shim.display(), std::env::var("PATH").unwrap_or_default());
    let w = env.work.to_str().unwrap().to_owned();
    let (_, _, ok) = garnish(
        &env,
        &["refresh", "--all", "--session", "sess-worker", "--cwd", &w],
        None,
        &[("PATH", path.as_str())],
    );
    assert!(ok);
    let before = spawns(&env).len();
    for _ in 0..3 {
        let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
        assert!(out.contains('✗'), "{out}");
    }
    assert_eq!(spawns(&env).len(), before, "a failed entry within its TTL spawns nothing");

    // A real refresh, then a branch switch: the entry is for another upstream, so it is stale.
    let (_, _, ok) = garnish(
        &env,
        &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w],
        None,
        &[],
    );
    assert!(ok);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1") && !out.contains('⟳'), "{out}");
    git(&env.work, &["checkout", "-q", "-b", "feature"]);
    git(&env.work, &["push", "-q", "-u", "origin", "feature"]);
    let before = spawns(&env).len();
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("feature") && out.contains('⟳'), "{out}");
    assert_eq!(spawns(&env).len(), before + 1, "{:?}", spawns(&env));
}

#[test]
fn worker_fetch_failure_keeps_counts_and_is_not_retried_within_the_interval() {
    let env = setup();
    config(
        &env,
        "preset = \"minimal\"\n[[line]]\nmodules = [\"sync\"]\n[modules.sync]\npreset = \"full\"\nfetch_interval = 300\n",
    );
    git(&env.work, &["remote", "set-url", "origin", "/nonexistent/origin.git"]);
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let entry = std::fs::read_dir(env.cache.join("repos"))
        .unwrap()
        .flatten()
        .find_map(|d| std::fs::read_to_string(d.path().join("sync.cache")).ok())
        .unwrap();
    assert!(entry.lines().next().unwrap().ends_with(" ok"), "{entry}");
    assert!(entry.contains("ahead=1"), "{entry}");
    assert!(entry.contains("fetch_error="), "{entry}");
    assert!(entry.contains("fetch_attempt=1738425600"), "{entry}");
    let (_, _, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok);
    let again = std::fs::read_dir(env.cache.join("repos"))
        .unwrap()
        .flatten()
        .find_map(|d| std::fs::read_to_string(d.path().join("sync.cache")).ok())
        .unwrap();
    assert!(
        again.contains("fetch_attempt=1738425600") && !again.contains("fetch_error="),
        "{again}"
    );
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1"), "{out}");
}
