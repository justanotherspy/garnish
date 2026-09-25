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

/// `git` in a temp repository, cut off from the developer's own git config.
///
/// `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` point at `/dev/null` because this
/// project mandates signed commits (CLAUDE.md § Session protocol), so the
/// machines that run this suite are exactly the machines with
/// `commit.gpgsign = true` — and a `git commit` here cannot reach a pinentry
/// from a test process. `core.hooksPath` and `commit.template` would bite
/// the same way.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
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

/// A second clone of the origin that pushes one commit to `main`, so the
/// first clone is behind once it fetches. Returns the clone's path so the
/// test can push again.
fn push_from_a_second_clone(env: &Env, name: &str) -> PathBuf {
    let root = env.work.parent().unwrap();
    let other = root.join("other");
    if !other.exists() {
        let origin = root.join("origin.git");
        git(root, &["clone", "-q", origin.to_str().unwrap(), other.to_str().unwrap()]);
    }
    std::fs::write(other.join(format!("{name}.txt")), format!("{name}\n")).unwrap();
    git(&other, &["add", "."]);
    git(&other, &["commit", "-q", "-m", name]);
    git(&other, &["push", "-q", "origin", "main"]);
    other
}

fn sync_entry_path(env: &Env) -> PathBuf {
    std::fs::read_dir(env.cache.join("repos"))
        .unwrap()
        .flatten()
        .map(|d| d.path().join("sync.cache"))
        .find(|p| p.is_file())
        .expect("sync.cache written")
}

fn sync_entry(env: &Env) -> String {
    std::fs::read_to_string(sync_entry_path(env)).expect("sync.cache readable")
}

fn payload(work: &Path) -> String {
    format!(
        r#"{{"cwd":"{w}","session_id":"sess-worker","workspace":{{"current_dir":"{w}","project_dir":"{w}","added_dirs":[]}},"model":{{"id":"m","display_name":"Opus"}},"cost":{{"total_cost_usd":0.1,"total_duration_ms":1000,"total_api_duration_ms":100,"total_lines_added":0,"total_lines_removed":0}},"context_window":{{"context_window_size":1000000,"used_percentage":10}}}}"#,
        w = work.display()
    )
}

/// The binary in the hermetic environment every test here needs (CLAUDE.md
/// § Cache and worker invariants): its own cache root, a frozen clock, no
/// spawning, no managed settings file, and a `HOME` that is not the
/// developer's. The one builder, so a test that needs one thing different
/// overrides only that (a hand-rolled `Command` used to drop three of these).
fn cmd(env: &Env, args: &[&str]) -> Command {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("GARNISH_CACHE_DIR", &env.cache)
        .env("GARNISH_NOW", NOW)
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_CONFIG", env.work.join("garnish.toml"))
        .env("COLUMNS", "120")
        .env("NO_COLOR", "1")
        .env("HOME", env.work.parent().unwrap())
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn garnish(
    env: &Env,
    args: &[&str],
    stdin: Option<&str>,
    extra_env: &[(&str, &str)],
) -> (String, String, bool) {
    let mut cmd = cmd(env, args);
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
        .map_or_default(|s| s.lines().map(str::to_owned).collect())
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
    assert!(!out.contains('\u{f111}'), "{out}");
    std::fs::write(env.work.join("a.txt"), "changed\n").unwrap();
    let (_, err, ok) = garnish(
        &env,
        &["refresh", "--module", "branch", "--session", "sess-worker", "--cwd", &w],
        None,
        &[],
    );
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains('\u{f111}'), "{out}");
    // Advance the clock past the TTL: the value is still shown, dimmed with ⟳, and a spawn is logged.
    let later = (NOW.parse::<u64>().unwrap() + 60).to_string();
    let before = spawns(&env).len();
    let (out, _, _) =
        garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", later.as_str())]);
    assert!(out.contains('\u{f111}') && out.contains('⟳'), "{out}");
    assert_eq!(spawns(&env).len(), before + 2, "{:?}", spawns(&env));
}

/// SPEC § 3.6: a value past its TTL spawns a worker but renders unchanged
/// until it is `stale_after` TTLs overdue; `stale_after = 1` restores the
/// old dim-at-TTL behaviour.
#[test]
fn worker_overdue_value_renders_plain_until_stale_after_ttls() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    let (_, err, ok) =
        garnish(&env, &["refresh", "--all", "--session", "sess-worker", "--cwd", &w], None, &[]);
    assert!(ok, "{err}");
    let now: u64 = NOW.parse().unwrap();
    // TTL is 5 s: 8 s later the entry is past its TTL (spawn) but well inside
    // the default 5-TTL grace, so no dimming and no ⟳.
    let before = spawns(&env).len();
    let t8 = (now + 8).to_string();
    let (out, _, _) =
        garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", t8.as_str())]);
    assert!(out.contains("main") && !out.contains('⟳'), "{out}");
    assert!(!out.contains("\x1b[2m"), "no dim escape expected: {out:?}");
    assert_eq!(spawns(&env).len(), before + 2, "{:?}", spawns(&env));
    // 26 s later it is more than 5 TTLs overdue: dimmed with ⟳.
    let t26 = (now + 26).to_string();
    let (out, _, _) =
        garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", t26.as_str())]);
    assert!(out.contains('⟳'), "{out}");
    // stale_after = 1: dim as soon as the TTL passes.
    config(&env, &format!("stale_after = 1\n{ONE_LINE}"));
    let (out, _, _) =
        garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", t8.as_str())]);
    assert!(out.contains('⟳'), "{out}");
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
    // now) says a worker is already on it: the tick must not spawn. The
    // stamp is in the tick's own frozen timeline — `GARNISH_NOW` moves
    // `now_millis`, so a wall-clock stamp would read as a lock from the
    // future, which is a clock that stepped backwards, not a live worker.
    let later_secs = NOW.parse::<i64>().unwrap() + 60;
    let later = later_secs.to_string();
    write_locks(&env, &format!("{} {}", std::process::id(), later_secs * 1000));
    garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", later.as_str())]);
    assert_eq!(spawns(&env).len(), 0, "{:?}", spawns(&env));
    // Locks stale by age are reclaimed: the tick spawns again.
    write_locks(&env, "4000000000 1");
    garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", later.as_str())]);
    assert_eq!(spawns(&env).len(), 2, "{:?}", spawns(&env));
}

/// A lock and an entry stamped in the future are a clock that stepped back
/// (a resumed VM, NTP correcting a bad RTC), not a live worker and not a
/// fresh value. Treating a negative age as "very recent" froze the module:
/// every tick saw a live lock and a fresh entry, so nothing refreshed and
/// no `⟳` ever appeared, until the wall clock caught up.
#[test]
fn cache_a_future_stamp_is_never_live_nor_fresh() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    let (_, err, ok) =
        garnish(&env, &["refresh", "--all", "--session", "sess-worker", "--cwd", &w], None, &[]);
    assert!(ok, "{err}");
    // An hour ahead of the tick's clock, in both the lock and the entries.
    let ahead_secs = NOW.parse::<i64>().unwrap() + 3_600;
    write_locks(&env, &format!("{} {}", std::process::id(), ahead_secs * 1000));
    for d in std::fs::read_dir(env.cache.join("repos")).unwrap().flatten() {
        for module in ["branch", "sync"] {
            let path = d.path().join(format!("{module}.cache"));
            let text = std::fs::read_to_string(&path).unwrap();
            let rest = text.split_once('\n').map_or_default(|(_, r)| r.to_owned());
            std::fs::write(&path, format!("v1 {} 5000 ok\n{rest}", ahead_secs * 1000)).unwrap();
        }
    }
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", NOW)]);
    assert_eq!(spawns(&env).len(), 2, "a future lock must not suppress the refresh: {out}");
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
            let mut child =
                cmd(&env, &[]).stdin(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
            child.stdin.take().unwrap().write_all(p.as_bytes()).unwrap();
            child
        })
        .collect();
    for c in children {
        assert!(c.wait_with_output().unwrap().status.success());
    }
    let s = spawns(&env);
    if cfg!(target_os = "linux") {
        // The tick takes the lock and hands it over, so the 32 ticks agree on
        // exactly one worker per module.
        assert_eq!(s.len(), 2, "{s:?}");
    } else {
        // Elsewhere the worker takes the lock itself (see `spawn_refresh`):
        // every tick may spawn, the workers dedupe, and no tick hands over.
        assert!(s.len() >= 2 && s.len() <= 64, "{s:?}");
        assert!(s.iter().all(|line| !line.contains("--lock-held")), "{s:?}");
        for module in ["branch", "sync"] {
            assert!(s.iter().any(|line| line.contains(&format!("--module {module} "))), "{s:?}");
        }
    }
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

    // Ticks never touch git: fast even with a hanging git on PATH. The
    // bound is far above the 3 ms budget of SPEC § 8 on purpose — this
    // proves the tick does not *wait* for the 30 s git, and a debug binary
    // starting on a loaded shared runner is not a budget measurement
    // (`bench/run.sh` is). A tighter bound flaked here.
    let started = Instant::now();
    let (out, _, ok) = garnish(&env, &[], Some(&payload(&env.work)), &[("PATH", path.as_str())]);
    assert!(ok && out.contains("main"), "{out}");
    assert!(started.elapsed() < Duration::from_secs(10), "tick took {:?}", started.elapsed());

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
    // Well under git's 30 s sleep: the worker's own 2 s timeout fired.
    assert!(started.elapsed() < Duration::from_secs(20), "worker took {:?}", started.elapsed());
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

/// SPEC § 9: behind, diverged and no-upstream repositories end to end. The
/// counts come from the worker; the no-upstream glyph needs no worker at all.
#[test]
fn worker_behind_diverged_and_no_upstream_render() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    push_from_a_second_clone(&env, "theirs");
    git(&env.work, &["fetch", "-q", "origin"]);
    // Diverged: the unpushed commit `two` against the fetched `theirs`.
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1") && out.contains("⇣1"), "{out}");
    // Behind only: drop the local commit.
    git(&env.work, &["reset", "-q", "--hard", "HEAD~1"]);
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇣1") && !out.contains('⇡'), "{out}");
    // No upstream: the glyph, no counts, and nothing for `sync` to refresh.
    // (`branch` was never refreshed here, and without the Linux lock
    // hand-over it spawns on every tick, so only `sync` spawns are counted.)
    git(&env.work, &["checkout", "-q", "-b", "local"]);
    let sync_spawns =
        |env: &Env| spawns(env).iter().filter(|l| l.contains("--module sync")).count();
    let before = sync_spawns(&env);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains('\u{f127}') && !out.contains('⇣') && !out.contains('⇡'), "{out}");
    assert_eq!(sync_spawns(&env), before, "{:?}", spawns(&env));
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    assert!(sync_entry(&env).contains("no upstream"), "{}", sync_entry(&env));
}

/// `sync` is the one cached module that carries a measure (its counts), so
/// it is the only place a `hide` rule meets a cached value. At 0/0 with
/// `hide = ["zero"]` it leaves the row, and it stays gone once its value
/// is overdue: a stale value a rule hides is hidden, not marked `⟳`.
#[test]
fn worker_hide_zero_on_sync_hides_even_count_and_stale_value() {
    let env = setup();
    let line = "icons = \"unicode\"\n[[line]]\nmodules = [\"branch\", \"sync\"]\n\
                [modules.sync]\nshow_zero = true\nshow_upstream = true\n";
    config(&env, line);
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1") && out.contains("origin/main"), "one ahead, shown: {out}");
    // Even: the upstream shows with its zero counts until a rule hides it.
    git(&env.work, &["push", "-q", "origin", "main"]);
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("origin/main"), "shown without a rule: {out}");
    config(&env, &format!("{line}hide = [\"zero\"]\n"));
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("main") && !out.contains("origin/main"), "hidden at 0/0: {out}");
    // More than five TTLs overdue, the value is stale and still hidden.
    let stale = (NOW.parse::<u64>().unwrap() + 26).to_string();
    let (out, _, _) =
        garnish(&env, &[], Some(&payload(&env.work)), &[("GARNISH_NOW", stale.as_str())]);
    assert!(!out.contains("origin/main") && !out.contains('⟳'), "hidden when stale: {out}");
}

/// git quotes a config value holding `#` or `;`, so the upstream of a
/// branch pushed with `git push -u origin fix/#12` is stored as
/// `merge = "refs/heads/fix/#12"`. Read raw, the quotes went into the
/// tracking ref, the worker's `rev-list` failed, and `sync` showed `✗` for
/// good on a perfectly ordinary branch.
#[test]
fn worker_a_quoted_upstream_counts_like_any_other() {
    let env = setup();
    config(&env, ONE_LINE);
    git(&env.work, &["checkout", "-q", "-b", "fix/#12"]);
    git(&env.work, &["push", "-q", "-u", "origin", "fix/#12"]);
    std::fs::write(env.work.join("c.txt"), "c\n").unwrap();
    git(&env.work, &["add", "."]);
    git(&env.work, &["commit", "-q", "-m", "three"]);
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains("upstream=refs/remotes/origin/fix/#12\n"), "{entry}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1") && out.contains("origin/fix/#12"), "{out}");
    assert!(!out.contains('✗') && !out.contains('"'), "{out}");
}

/// The tick reads the payload's repository from `.git` directly; the
/// worker's git follows `GIT_DIR` and friends first. A harness started
/// with one exported (by a hook, an alias) made `sync` count another
/// repository's commits next to this one's branch.
#[test]
fn worker_git_ignores_an_inherited_git_dir() {
    let env = setup();
    config(&env, ONE_LINE);
    let other = push_from_a_second_clone(&env, "theirs");
    let w = env.work.to_str().unwrap().to_owned();
    let other_git = other.join(".git");
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(
        &env,
        refresh,
        None,
        &[("GIT_DIR", other_git.to_str().unwrap()), ("GIT_WORK_TREE", other.to_str().unwrap())],
    );
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains("ahead=1\n") && entry.contains("behind=0\n"), "{entry}");
}

/// SPEC § 6: `fetch_interval` runs `git fetch` in the worker, once per
/// interval, so a commit pushed elsewhere shows as `behind` without any
/// fetch by hand.
#[test]
fn worker_fetch_interval_fetches_once_per_interval() {
    let env = setup();
    config(
        &env,
        "preset = \"minimal\"\n[[line]]\nmodules = [\"sync\"]\n[modules.sync]\npreset = \"full\"\nfetch_interval = 300\n",
    );
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    push_from_a_second_clone(&env, "theirs");
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains("fetch_attempt=1738425600"), "{entry}");
    assert!(!entry.contains("fetch_error="), "{entry}");
    assert!(entry.contains("ahead=1") && entry.contains("behind=1"), "{entry}");
    assert!(env.work.join(".git").join("FETCH_HEAD").exists());
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1") && out.contains("⇣1"), "{out}");
    // Inside the interval the worker does not fetch: a second push stays unseen.
    push_from_a_second_clone(&env, "again");
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    assert!(sync_entry(&env).contains("behind=1"), "{}", sync_entry(&env));
    // Past the interval (measured from the attempt and from FETCH_HEAD's
    // wall-clock mtime) it fetches again.
    let wall =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let later = (wall + 400).to_string();
    let (_, err, ok) = garnish(&env, refresh, None, &[("GARNISH_NOW", later.as_str())]);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains(&format!("fetch_attempt={later}")), "{entry}");
    assert!(entry.contains("behind=2"), "{entry}");

    // A stamp *ahead* of the clock is a clock that stepped backwards (a
    // resumed VM, NTP correcting a bad RTC), not an attempt from the future.
    // Its age is negative, so a plain `age >= interval` never came true and
    // auto-fetch stayed frozen, silently, until the wall clock caught up.
    // Here the entry is stamped an hour ahead and the next refresh must
    // still fetch, which the third push proves.
    push_from_a_second_clone(&env, "third");
    let ahead_stamp = (wall + 4000).to_string();
    let path = sync_entry_path(&env);
    let poisoned = sync_entry(&env)
        .replace(&format!("fetch_attempt={later}"), &format!("fetch_attempt={ahead_stamp}"));
    assert!(poisoned.contains(&format!("fetch_attempt={ahead_stamp}")), "{poisoned}");
    std::fs::write(&path, &poisoned).unwrap();
    let (_, err, ok) = garnish(&env, refresh, None, &[("GARNISH_NOW", later.as_str())]);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains(&format!("fetch_attempt={later}")), "the future stamp stays: {entry}");
    assert!(entry.contains("behind=3"), "the fetch must not be frozen: {entry}");

    // The same rule for `FETCH_HEAD`'s own mtime, the other half of `due`:
    // an hour ahead of the worker's clock read as age 0, "not due", and
    // froze auto-fetch just the same.
    push_from_a_second_clone(&env, "fourth");
    let later2 = (wall + 800).to_string();
    let ahead = std::time::UNIX_EPOCH + Duration::from_secs(wall + 800 + 3_600);
    std::fs::File::options()
        .write(true)
        .open(env.work.join(".git").join("FETCH_HEAD"))
        .unwrap()
        .set_modified(ahead)
        .unwrap();
    let (_, err, ok) = garnish(&env, refresh, None, &[("GARNISH_NOW", later2.as_str())]);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains("behind=4"), "a future FETCH_HEAD must not freeze the fetch: {entry}");
}

/// git truncates `FETCH_HEAD` before it contacts the remote, so every
/// failing fetch looked like one that had just happened: with the remote
/// unreachable the fetch-age hint never appeared while the counts aged. The
/// worker now records its last good fetch, and the hint counts from that.
#[test]
fn worker_a_failing_fetch_is_not_a_recent_one() {
    let env = setup();
    config(
        &env,
        "preset = \"minimal\"\n[[line]]\nmodules = [\"sync\"]\n[modules.sync]\npreset = \"full\"\nfetch_interval = 300\n",
    );
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    assert!(sync_entry(&env).contains(&format!("fetch_ok_at={NOW}\n")), "{}", sync_entry(&env));
    git(&env.work, &["remote", "set-url", "origin", "/nonexistent/origin.git"]);
    let wall =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let later = (wall + 400).to_string();
    let at_later = [("GARNISH_NOW", later.as_str())];
    let (_, err, ok) = garnish(&env, refresh, None, &at_later);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(entry.contains("fetch_error=") && entry.contains(&format!("fetch_ok_at={NOW}\n")));
    assert_eq!(std::fs::metadata(env.work.join(".git").join("FETCH_HEAD")).unwrap().len(), 0);
    // The last good fetch was a year and more before `later`: the hint
    // shows, where the truncated FETCH_HEAD said "400 s ago".
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &at_later);
    assert!(out.contains('\u{f017}'), "{out}");
}

/// A pruned upstream (the branch merged and deleted on the forge, then
/// `fetch --prune`) is the ordinary state after a pull request. The config
/// still names it, the tracking ref is gone, and `rev-list` failed: `sync`
/// showed `✗` for good and a failing worker ran every TTL. It is no
/// upstream now, and says so with that glyph.
#[test]
fn worker_a_gone_upstream_is_no_upstream() {
    let env = setup();
    config(&env, ONE_LINE);
    git(&env.work, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let entry = sync_entry(&env);
    assert!(
        entry.lines().next().unwrap().ends_with(" ok") && entry.contains("gone=1\n"),
        "{entry}"
    );
    let sync_spawns =
        |env: &Env| spawns(env).iter().filter(|l| l.contains("--module sync")).count();
    let before = sync_spawns(&env);
    for _ in 0..2 {
        let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
        assert!(out.contains('\u{f127}') && !out.contains('✗') && !out.contains('⇡'), "{out}");
    }
    assert_eq!(sync_spawns(&env), before, "a fresh entry spawns nothing");
}

/// Branches that share an upstream (`checkout -b feat --track
/// origin/main`, common for short-lived work) shared `sync`'s entry: the
/// previous branch's counts showed as fresh after a switch. The branch is
/// half the key now, so the switch is a miss.
#[test]
fn worker_branches_sharing_an_upstream_do_not_share_counts() {
    let env = setup();
    config(&env, ONE_LINE);
    let w = env.work.to_str().unwrap().to_owned();
    let refresh = &["refresh", "--module", "sync", "--session", "sess-worker", "--cwd", &w];
    let (_, err, ok) = garnish(&env, refresh, None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1") && !out.contains('⟳'), "{out}");
    git(&env.work, &["checkout", "-q", "-b", "feat", "--track", "origin/main"]);
    let sync_spawns =
        |env: &Env| spawns(env).iter().filter(|l| l.contains("--module sync")).count();
    let before = sync_spawns(&env);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("feat") && out.contains('⟳'), "{out}");
    assert_eq!(sync_spawns(&env), before + 1, "{:?}", spawns(&env));
    // Run as the logged spawn would: on Linux the tick handed its lock over.
    let mut handed = refresh.to_vec();
    if cfg!(target_os = "linux") {
        handed.push("--lock-held");
    }
    let (_, err, ok) = garnish(&env, &handed, None, &[]);
    assert!(ok, "{err}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(!out.contains('⇡') && !out.contains('⟳'), "{out}");
}

/// SPEC § 7: `--config` is global, so a status line command
/// `garnish --config ~/work.toml` renders with that file, and its workers
/// must read it too. They were spawned without it and re-resolved the
/// config from the environment, so `sync.fetch_interval` came from another
/// file: set only in the `--config` one, no fetch ever ran.
#[test]
fn worker_reads_the_config_file_its_tick_read() {
    let env = setup();
    config(&env, ONE_LINE);
    let own = env.work.parent().unwrap().join("own.toml");
    std::fs::write(
        &own,
        "preset = \"minimal\"\n[[line]]\nmodules = [\"sync\"]\n[modules.sync]\nfetch_interval = 1\n",
    )
    .unwrap();
    let own_arg = own.to_str().unwrap();
    let (_, err, ok) = garnish(&env, &["--config", own_arg], Some(&payload(&env.work)), &[]);
    assert!(ok, "{err}");
    let s = spawns(&env);
    assert_eq!(s.len(), 1, "{s:?}");
    assert!(s[0].starts_with(&format!("--config {own_arg} refresh --module sync ")), "{s:?}");
    // Run the logged line as the worker would be run: it fetches.
    let args: Vec<&str> = s[0].split(' ').collect();
    let (_, err, ok) = garnish(&env, &args, None, &[]);
    assert!(ok, "{err}");
    assert!(sync_entry(&env).contains("fetch_attempt="), "{}", sync_entry(&env));
}

/// Run the workers `modules` as their logged spawns would (on Linux the
/// tick handed each its lock).
fn run_workers(env: &Env, modules: &[&str]) {
    run_workers_with(env, modules, &[]);
}

/// [`run_workers`] with `extra_env` in the workers' environment.
fn run_workers_with(env: &Env, modules: &[&str], extra_env: &[(&str, &str)]) {
    let w = env.work.to_str().unwrap().to_owned();
    for module in modules {
        let mut args = vec!["refresh", "--module", module, "--session", "sess-worker", "--cwd", &w];
        if cfg!(target_os = "linux") {
            args.push("--lock-held");
        }
        let (_, err, ok) = garnish(env, &args, None, extra_env);
        assert!(ok, "{module}: {err}");
    }
}

/// A `PATH` whose first `git` fails every call, as a git that cannot read
/// the repository does (too old for its format, a `safe.directory`
/// refusal).
fn failing_git_path(env: &Env) -> String {
    let shim = env.work.parent().unwrap().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    std::fs::write(shim.join("git"), "#!/bin/sh\necho 'fatal: nope' >&2\nexit 128\n").unwrap();
    std::fs::set_permissions(shim.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    format!("{}:{}", shim.display(), std::env::var("PATH").unwrap_or_default())
}

/// SPEC § 6: a repository whose refs are not files (reftable) has no HEAD
/// the tick can read, so `branch` and `sync` fall back to their workers,
/// which ask git, and whose entries are keyed on the ref store's
/// `tables.list` stamp. Simulated with a `reftable/` directory the
/// installed git ignores, so it runs on any git; the real format is
/// `worker_a_reftable_repository_shows_its_branch_and_counts`.
#[test]
fn worker_refs_that_are_not_files_fall_back_to_the_worker() {
    let env = setup();
    config(&env, ONE_LINE);
    let tables = env.work.join(".git").join("reftable").join("tables.list");
    std::fs::create_dir_all(tables.parent().unwrap()).unwrap();
    std::fs::write(&tables, "t\n").unwrap();
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(!out.contains("main"), "the tick cannot read this HEAD: {out}");
    assert_eq!(spawns(&env).len(), 2, "{:?}", spawns(&env));
    run_workers(&env, &["branch", "sync"]);
    let sha = std::fs::read_to_string(env.work.join(".git/refs/heads/main")).unwrap();
    let short: String = sha.chars().take(7).collect();
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("main") && out.contains(&short) && out.contains("⇡1"), "{out}");
    assert!(!out.contains('⟳') && !out.contains('✗'), "{out}");
    assert_eq!(spawns(&env).len(), 2, "a fresh entry spawns nothing");
    // Any ref update rewrites `tables.list`: the entries are for another
    // state of the refs now.
    let later = std::time::SystemTime::now() + Duration::from_secs(60);
    std::fs::File::options().write(true).open(&tables).unwrap().set_modified(later).unwrap();
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("main") && out.contains('⟳'), "{out}");
    assert_eq!(spawns(&env).len(), 4, "{:?}", spawns(&env));
    // A worker whose git fails leaves nothing to name, and still its mark.
    let failing = failing_git_path(&env);
    run_workers_with(&env, &["branch", "sync"], &[("PATH", failing.as_str())]);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert_eq!(out.matches('✗').count(), 2, "{out}");
}

/// A reftable worker that fails (a git before 2.45 meeting the
/// `refStorage` extension, a `safe.directory` refusal, the timeout) writes
/// a failed entry, and a failed entry carries no `tables` stamp. The
/// fallback's check compared the stamp alone, so the failure was never
/// fresh and every tick spawned another worker (review 2026-09-25): a
/// failed entry is fresh for its TTL like any other.
#[test]
fn worker_a_failed_reftable_worker_is_fresh_for_its_ttl() {
    let env = setup();
    config(&env, ONE_LINE);
    let tables = env.work.join(".git").join("reftable").join("tables.list");
    std::fs::create_dir_all(tables.parent().unwrap()).unwrap();
    std::fs::write(&tables, "").unwrap();
    let (_, _, ok) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(ok);
    assert_eq!(spawns(&env).len(), 2, "{:?}", spawns(&env));
    let failing = failing_git_path(&env);
    run_workers_with(&env, &["branch", "sync"], &[("PATH", failing.as_str())]);
    for _ in 0..3 {
        let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
        assert_eq!(out.matches('✗').count(), 2, "{out}");
        assert!(!out.contains('⟳'), "{out}");
    }
    assert_eq!(spawns(&env).len(), 2, "a failed entry spawns nothing: {:?}", spawns(&env));
}

/// The real reftable format, where the installed git has it (2.45 and
/// later); skipped otherwise. An upstream is made without a server: the
/// tracking ref and the two config keys `push -u` would write.
#[test]
fn worker_a_reftable_repository_shows_its_branch_and_counts() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let init = Command::new("git")
        .args(["init", "-q", "-b", "main", "--ref-format=reftable", work.to_str().unwrap()])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    if !init.status.success() {
        return; // this git has no reftable
    }
    std::fs::write(work.join("a.txt"), "a\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "one"]);
    let head = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&work).output().unwrap();
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    git(&work, &["update-ref", "refs/remotes/origin/main", &sha]);
    git(&work, &["config", "branch.main.remote", "origin"]);
    git(&work, &["config", "branch.main.merge", "refs/heads/main"]);
    std::fs::write(work.join("b.txt"), "b\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "two"]);
    let cache = dir.path().join("cache");
    let env = Env { _dir: dir, work, cache };
    config(&env, ONE_LINE);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(!out.contains("main"), "{out}");
    run_workers(&env, &["branch", "sync"]);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("main") && out.contains("⇡1"), "{out}");
    assert!(!out.contains('⟳') && !out.contains('✗'), "{out}");
    // A branch switch is a ref update: stale at once, then the new branch,
    // which has no upstream.
    git(&env.work, &["checkout", "-q", "-b", "other"]);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains('⟳'), "{out}");
    run_workers(&env, &["branch", "sync"]);
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("other") && out.contains('\u{f127}') && !out.contains('⟳'), "{out}");
}

/// A HEAD the tick refuses to read (here a link out of the git directory,
/// which git itself follows) with a payload that names the branch: the
/// render keyed the dirty entry on the payload's name while the worker
/// stored an empty `head`, so no entry ever matched and every tick
/// spawned a worker under a permanent `⟳`.
#[test]
fn worker_an_unreadable_head_with_a_payload_branch_settles() {
    let env = setup();
    config(&env, ONE_LINE);
    let outside = env.work.parent().unwrap().join("HEAD-outside");
    std::fs::write(&outside, "ref: refs/heads/main\n").unwrap();
    let head = env.work.join(".git").join("HEAD");
    std::fs::remove_file(&head).unwrap();
    std::os::unix::fs::symlink(&outside, &head).unwrap();
    let with_branch = payload(&env.work)
        .replace(r#""model":"#, r#""worktree":{"name":"w","branch":"main"},"model":"#);
    // Without the payload's name there is nothing to show, and so nothing
    // to spawn a worker for.
    let (out, _, ok) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(ok && !out.contains("main"), "{out}");
    assert!(spawns(&env).is_empty(), "{:?}", spawns(&env));
    let w = env.work.to_str().unwrap().to_owned();
    let (_, err, ok) = garnish(
        &env,
        &["refresh", "--module", "branch", "--session", "sess-worker", "--cwd", &w],
        None,
        &[],
    );
    assert!(ok, "{err}");
    for _ in 0..2 {
        let (out, _, _) = garnish(&env, &[], Some(&with_branch), &[]);
        assert!(out.contains("main") && !out.contains('⟳'), "{out}");
    }
    assert!(spawns(&env).is_empty(), "{:?}", spawns(&env));
}

/// The first executable `git` on `PATH`.
fn real_git() -> PathBuf {
    std::env::var("PATH")
        .unwrap()
        .split(':')
        .map(|d| Path::new(d).join("git"))
        .find(|p| {
            p.is_file() && std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        })
        .expect("git on PATH")
}

/// SPEC § 2.1 / § 9: Claude Code cancels an in-flight status line script,
/// and the worker the tick spawned must complete regardless. The tick runs
/// as the leader of its own process group and the whole group is killed
/// once it has spawned the worker; the worker, in a group of its own with a
/// slow git, still writes the entry.
#[test]
fn spawn_worker_outlives_the_ticks_process_group() {
    use std::os::unix::process::CommandExt as _;
    let env = setup();
    config(
        &env,
        "preset = \"minimal\"\n[[line]]\nmodules = [\"sync\"]\n[modules.sync]\npreset = \"full\"\n",
    );
    let shim = env.work.parent().unwrap().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    let fake = shim.join("git");
    std::fs::write(&fake, format!("#!/bin/sh\nsleep 1\nexec '{}' \"$@\"\n", real_git().display()))
        .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", shim.display(), std::env::var("PATH").unwrap_or_default());

    let mut child = cmd(&env, &[])
        .env("PATH", &path)
        .env_remove("GARNISH_NO_SPAWN")
        .stdin(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let pgid = child.id();
    child.stdin.take().unwrap().write_all(payload(&env.work).as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("main"));
    assert!(repo_cache_files(&env).iter().all(|f| f != "sync.cache"), "no entry yet");

    // The tick has exited with its worker still in git's sleep; kill
    // everything left in the tick's process group, as a cancelled script's
    // process tree would be. A worker in that group dies here. (The `kill`
    // binary: dash's builtin takes no `--` and no negative pid.)
    let killed = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&killed.stderr);
    assert!(
        killed.status.success() || stderr.contains("No such process"),
        "kill the tick's group: {stderr}"
    );
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        if repo_cache_files(&env).iter().any(|f| f == "sync.cache") {
            let entry = sync_entry(&env);
            assert!(entry.lines().next().unwrap().ends_with(" ok"), "{entry}");
            assert!(entry.contains("ahead=1"), "{entry}");
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the worker never wrote sync.cache: {:?}", repo_cache_files(&env));
}

#[test]
fn cache_leftover_temp_and_truncated_entries_are_ignored() {
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

/// SPEC § 6: the bounded sweep runs on its own, off the tick, when a
/// worker writes a scope's first entry. It never ran: it waited for a new
/// session *directory*, and the lock (taken first, by the tick on Linux)
/// always made that directory before the entry was written.
#[test]
fn gc_runs_when_a_worker_writes_its_first_entry() {
    let env = setup();
    config(&env, ONE_LINE);
    let t = std::time::SystemTime::now() - Duration::from_hours(48);
    let mut idle = Vec::new();
    for dir in ["sessions/ancient", "repos/00000000000000aa"] {
        let dir = env.cache.join(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.cache"), "v1 1 1 ok\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(dir.join("m.cache"))
            .unwrap()
            .set_modified(t)
            .unwrap();
        idle.push(dir);
    }
    // The tick spawns (and on Linux takes the lock, making the scope's
    // directory); the worker then writes the first entry.
    let (_, _, ok) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(ok);
    assert!(idle.iter().all(|d| d.exists()), "the tick never sweeps");
    run_workers(&env, &["sync"]);
    for dir in &idle {
        assert!(!dir.exists(), "{} survived", dir.display());
    }
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
    // Inside the interval nothing is fetched, and the failure is carried
    // over: it used to last one TTL, until the next refresh rewrote the
    // entry without it.
    assert!(
        again.contains("fetch_attempt=1738425600") && again.contains("fetch_error="),
        "{again}"
    );
    assert!(!again.contains("fetch_ok_at="), "nothing has worked yet: {again}");
    let (out, _, _) = garnish(&env, &[], Some(&payload(&env.work)), &[]);
    assert!(out.contains("⇡1"), "{out}");
}

/// SPEC § 9: the repo modules against a real repository, at every preset and
/// icon set.
///
/// Every pinned render in the suite runs with `Clock::fixed()`, whose
/// `git: false` makes `Ctx::git_dirs()` `None`, and every payload fixture's
/// `cwd` is a path that does not exist — so `sync` returned nothing and
/// `branch` lost its sha and dirty halves in every golden and in the
/// schema matrix. This is the one place they render with git on.
///
/// Serial (`worker_`): a shared cache root and a temp repository.
#[test]
fn worker_repo_modules_render_in_every_preset_and_icon_set() {
    let env = setup();
    let w = env.work.to_str().unwrap().to_owned();
    // A *tracked* file, changed: `git status --untracked-files=no` ignores
    // the `garnish.toml` each `config()` call drops in, so without this the
    // tree is clean, `dirty=0` goes into the cache and the `full` preset's
    // dirty badge never renders in the whole suite.
    std::fs::write(env.work.join("a.txt"), "changed\n").unwrap();
    let (_, err, ok) =
        garnish(&env, &["refresh", "--all", "--session", "sess-worker", "--cwd", &w], None, &[]);
    assert!(ok, "{err}");
    let p = payload(&env.work);
    for preset in ["minimal", "default", "full"] {
        for icons in ["nerd", "unicode", "emoji", "ascii"] {
            let label = format!("{preset}/{icons}");
            config(
                &env,
                &format!(
                    "icons = \"{icons}\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"path\", \"branch\", \"sync\"]\n[modules.path]\npreset = \"{preset}\"\n[modules.branch]\npreset = \"{preset}\"\n[modules.sync]\npreset = \"{preset}\"\n"
                ),
            );
            let (out, _, ok) = garnish(&env, &[], Some(&p), &[]);
            assert!(ok, "{label}: {out}");
            let row = out.lines().next().unwrap_or_default();
            // The repo really is read: the branch, and the commit `setup`
            // left unpushed as one ahead — with the set's own glyph, so a
            // module that rendered nothing cannot satisfy this.
            assert!(row.contains("main"), "{label}: no branch in {row:?}");
            let set = garnish::icons::IconSet::parse(icons).unwrap();
            let schema_glyph = |id: &str, key: &str| {
                garnish::modules::entry(id).unwrap().schema.icon(key).unwrap().glyph.get(set)
            };
            let glyph = schema_glyph("sync", "ahead");
            assert!(row.contains(&format!("{glyph}1")), "{label}: no {glyph:?}1 in {row:?}");
            // The dirty marker is the one badge whose only render is here.
            if preset == "full" {
                let dirty = schema_glyph("branch", "dirty");
                assert!(row.contains(dirty), "{label}: no dirty {dirty:?} in {row:?}");
            }
            assert!(
                unicode_width::UnicodeWidthStr::width(row) <= 116,
                "{label}: {row:?} is wider than the box"
            );
            if icons == "ascii" {
                assert!(row.is_ascii(), "{label}: the ascii set emitted {row:?}");
            }
        }
    }
    // `branch.link` needs a branch: SPEC § 3.1 gives a detached HEAD no page,
    // and the unit level cannot reach one (it has no repository on disk).
    config(
        &env,
        "color = \"always\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"branch\"]\n[modules.branch]\nlink = true\n",
    );
    let repo = r#""repo":{"host":"github.com","owner":"o","name":"r"}"#;
    let linked =
        payload(&env.work).replace(r#""added_dirs":[]"#, &format!(r#""added_dirs":[],{repo}"#));
    let no_color = [("NO_COLOR", "")];
    let (out, _, ok) = garnish(&env, &[], Some(&linked), &no_color);
    assert!(ok && out.contains("\x1b]8;;https://github.com/o/r/tree/main"), "{out:?}");
    let head = env.work.join(".git").join("HEAD");
    let sha = std::fs::read_to_string(env.work.join(".git/refs/heads/main")).unwrap();
    std::fs::write(&head, &sha).unwrap();
    let (out, _, ok) = garnish(&env, &[], Some(&linked), &no_color);
    assert!(ok && !out.contains("\x1b]8;;"), "a detached HEAD has no page: {out:?}");
    std::fs::write(&head, "ref: refs/heads/main\n").unwrap();

    // `max_length` cuts the branch name with the icon set's own mark, which
    // no other test reaches: the matrix never sets it (it is an integer, and
    // only booleans and enums are swept) and every golden runs without git.
    for (icons, mark) in [("unicode", "…"), ("ascii", "..")] {
        config(
            &env,
            &format!(
                "icons = \"{icons}\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"branch\"]\n[modules.branch]\nshow_icon = false\nmax_length = 3\n"
            ),
        );
        let (out, _, ok) = garnish(&env, &[], Some(&p), &[]);
        let row = out.lines().next().unwrap_or_default().trim_end();
        assert!(ok && row.ends_with(mark), "{icons}: {row:?} does not end in {mark:?}");
        if icons == "ascii" {
            assert!(row.is_ascii(), "the ascii set emitted {row:?}");
        }
    }
}

const ACCOUNT_LINE: &str = "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"account\"]\n";

/// The `account` worker for the session of `payload`, run as the logged
/// spawn would run it, and the entry it wrote.
fn refresh_account(env: &Env, extra_env: &[(&str, &str)], lock_held: bool) -> String {
    let w = env.work.to_str().unwrap().to_owned();
    let mut args = vec!["refresh", "--module", "account", "--session", "sess-worker", "--cwd", &w];
    if lock_held {
        args.push("--lock-held");
    }
    let (_, err, ok) = garnish(env, &args, None, extra_env);
    assert!(ok, "{err}");
    std::fs::read_to_string(env.cache.join("sessions").join("sess-worker").join("account.cache"))
        .expect("account.cache written")
}

/// One tick of the `account` line, plain.
fn account_row(env: &Env) -> String {
    let (out, _, ok) = garnish(env, &[], Some(&payload(&env.work)), &[]);
    assert!(ok, "{out}");
    out.trim_end().to_owned()
}

/// SPEC § 3.8: `account` is the cached module outside the repo group. The
/// first tick shows nothing and spawns its worker; the worker reads
/// `~/.claude.json` (a realistic 300 KB of the harness's own state around
/// the field) into a session-scoped entry; the next tick shows the
/// address, or its user part under `style = "user"`, and spawns nothing.
#[test]
fn worker_account_shows_the_email_once_its_worker_has_run() {
    let env = setup();
    let home = env.work.parent().unwrap().to_path_buf();
    let pad = "x".repeat(300 * 1024);
    std::fs::write(
        home.join(".claude.json"),
        format!(
            r#"{{"numStartups": 9, "oauthAccount": {{"accountUuid": "u", "emailAddress": "dev@example.com"}}, "projects": {{"/p": {{"history": "{pad}"}}}}}}"#
        ),
    )
    .unwrap();
    config(&env, ACCOUNT_LINE);
    assert_eq!(account_row(&env), "", "before the worker: nothing");
    let s = spawns(&env);
    assert_eq!(s.len(), 1, "{s:?}");
    assert!(s[0].contains("--module account"), "{s:?}");
    // Only Linux hands the lock to the worker; elsewhere the worker takes it.
    let handover = cfg!(target_os = "linux");
    assert!(s[0].ends_with("--lock-held") == handover, "{s:?}");
    let entry = refresh_account(&env, &[], handover);
    assert!(entry.starts_with("v1 ") && entry.contains(" 600000 ok\n"), "{entry:?}");
    assert!(entry.ends_with("email=dev@example.com\n"), "{entry:?}");
    assert_eq!(account_row(&env), "@ dev@example.com");
    assert_eq!(spawns(&env).len(), 1, "a fresh entry spawns nothing");
    config(
        &env,
        &format!("{ACCOUNT_LINE}[modules.account]\nstyle = \"user\"\nshow_icon = false\n"),
    );
    assert_eq!(account_row(&env), "dev");
}

/// SPEC § 3.8: no `~/.claude.json` is an `ok` entry with no address (an
/// API-key session: nothing to show, never `✗`); `CLAUDE_CONFIG_DIR` moves
/// the file when it is set and non-empty; a file that does not parse is a
/// failed entry, and the tick marks the row.
#[test]
fn worker_account_follows_the_config_dir_and_marks_a_broken_file() {
    let env = setup();
    let home = env.work.parent().unwrap().to_path_buf();
    config(&env, ACCOUNT_LINE);
    let entry = refresh_account(&env, &[], false);
    assert!(entry.contains(" ok\n") && !entry.contains("email="), "{entry:?}");
    assert_eq!(account_row(&env), "");
    let cfg_dir = home.join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(
        cfg_dir.join(".claude.json"),
        r#"{"oauthAccount": {"emailAddress": "moved@example.com"}}"#,
    )
    .unwrap();
    std::fs::write(
        home.join(".claude.json"),
        r#"{"oauthAccount": {"emailAddress": "home@example.com"}}"#,
    )
    .unwrap();
    let moved = refresh_account(&env, &[("CLAUDE_CONFIG_DIR", cfg_dir.to_str().unwrap())], false);
    assert!(moved.ends_with("email=moved@example.com\n"), "{moved:?}");
    assert_eq!(account_row(&env), "@ moved@example.com");
    let empty = refresh_account(&env, &[("CLAUDE_CONFIG_DIR", "")], false);
    assert!(empty.ends_with("email=home@example.com\n"), "an empty variable is unset: {empty:?}");
    assert_eq!(account_row(&env), "@ home@example.com");
    std::fs::write(home.join(".claude.json"), "{ broken").unwrap();
    let broken = refresh_account(&env, &[], false);
    assert!(broken.contains(" err\n") && broken.contains("not valid JSON"), "{broken:?}");
    assert_eq!(account_row(&env), "– ✗", "a broken file is never silent");
    assert!(spawns(&env).is_empty(), "every tick found a fresh entry: {:?}", spawns(&env));
}
