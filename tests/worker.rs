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

fn sync_entry(env: &Env) -> String {
    std::fs::read_dir(env.cache.join("repos"))
        .unwrap()
        .flatten()
        .find_map(|d| std::fs::read_to_string(d.path().join("sync.cache")).ok())
        .expect("sync.cache written")
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
        .env("GARNISH_MANAGED_SETTINGS", "")
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
            let rest = text.split_once('\n').map(|(_, r)| r.to_owned()).unwrap_or_default();
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

    let mut child = Command::new(bin())
        .env("GARNISH_CACHE_DIR", &env.cache)
        .env("GARNISH_NOW", NOW)
        .env("GARNISH_CONFIG", env.work.join("garnish.toml"))
        .env("COLUMNS", "120")
        .env("NO_COLOR", "1")
        .env("PATH", &path)
        .env_remove("GARNISH_NO_SPAWN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
