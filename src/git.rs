//! Reading git state.
//!
//! Everything the *tick* needs (top level, HEAD, upstream, fetch age) is read
//! straight from the `.git` directory with a handful of small file reads, so
//! no process is spawned per second. Anything that needs the object database
//! (ahead/behind counts, dirty state, fetching) runs in the background worker
//! through the `git` binary.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The directories that make up a repository checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// The working tree root (the directory that contains `.git`).
    pub toplevel: PathBuf,
    /// The per-worktree git directory (`.git`, or the linked worktree's dir).
    pub git_dir: PathBuf,
    /// The shared git directory (`git_dir` for the main worktree).
    pub common_dir: PathBuf,
    /// True inside a linked worktree.
    pub linked_worktree: bool,
}

impl Dirs {
    /// A stable cache key for this checkout.
    #[must_use]
    pub fn cache_key(&self) -> String {
        crate::cache::key_hash(&[
            &self.common_dir.to_string_lossy(),
            &self.git_dir.to_string_lossy(),
        ])
    }

    /// True when the repository uses the reftable backend (refs are not files).
    #[must_use]
    pub fn uses_reftable(&self) -> bool {
        self.common_dir.join("reftable").is_dir()
    }
}

/// Locate the repository containing `path` by walking up to the root.
#[must_use]
pub fn discover(path: &Path) -> Option<Dirs> {
    let mut dir = if path.is_dir() { path.to_path_buf() } else { path.parent()?.to_path_buf() };
    for _ in 0..64 {
        let dot = dir.join(".git");
        if dot.is_dir() {
            let common = common_dir(&dot);
            let linked = common != dot;
            return Some(Dirs {
                toplevel: dir,
                git_dir: dot,
                common_dir: common,
                linked_worktree: linked,
            });
        }
        if dot.is_file() {
            let text = std::fs::read_to_string(&dot).ok()?;
            let target = text.trim().strip_prefix("gitdir:")?.trim();
            let git_dir = if Path::new(target).is_absolute() {
                PathBuf::from(target)
            } else {
                dir.join(target)
            };
            let git_dir = normalize(&git_dir);
            let common = common_dir(&git_dir);
            return Some(Dirs {
                toplevel: dir,
                linked_worktree: common != git_dir,
                git_dir,
                common_dir: common,
            });
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn common_dir(git_dir: &Path) -> PathBuf {
    let Ok(text) = std::fs::read_to_string(git_dir.join("commondir")) else {
        return git_dir.to_path_buf();
    };
    let target = text.trim();
    if target.is_empty() {
        return git_dir.to_path_buf();
    }
    let p =
        if Path::new(target).is_absolute() { PathBuf::from(target) } else { git_dir.join(target) };
    normalize(&p)
}

/// Collapse `.` and `..` components without touching the filesystem.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Where HEAD points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// A branch (`refs/heads/<name>` → `name`).
    Branch(String),
    /// Detached at a commit.
    Detached(String),
}

/// Read HEAD. `None` for reftable repositories, whose `HEAD` file is a
/// placeholder (`ref: refs/heads/.invalid`) rather than the real head.
#[must_use]
pub fn head(dirs: &Dirs) -> Option<Head> {
    if dirs.uses_reftable() {
        return None;
    }
    let text = std::fs::read_to_string(dirs.git_dir.join("HEAD")).ok()?;
    let line = text.lines().next()?.trim();
    if let Some(r) = line.strip_prefix("ref:") {
        let r = r.trim();
        return Some(Head::Branch(r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()));
    }
    (!line.is_empty()).then(|| Head::Detached(line.to_owned()))
}

/// Symbolic refs deeper than this are treated as broken (git's own limit).
const SYMREF_MAX_DEPTH: usize = 5;

/// Resolve a full ref name (`refs/heads/main`) to a commit id.
///
/// Loose refs are tried first, then `packed-refs`. `None` for reftable
/// repositories, unknown refs, and symbolic-ref chains longer than five
/// (cycles included), matching git's own limit.
#[must_use]
pub fn resolve_ref(dirs: &Dirs, refname: &str) -> Option<String> {
    if dirs.uses_reftable() {
        return None;
    }
    let mut name = refname.to_owned();
    for _ in 0..SYMREF_MAX_DEPTH {
        let mut next: Option<String> = None;
        for base in [&dirs.git_dir, &dirs.common_dir] {
            let Ok(text) = std::fs::read_to_string(base.join(&name)) else { continue };
            let line = text.lines().next().unwrap_or("").trim();
            if let Some(r) = line.strip_prefix("ref:") {
                next = Some(r.trim().to_owned());
                break;
            }
            if !line.is_empty() {
                return Some(line.to_owned());
            }
        }
        match next {
            Some(n) => name = n,
            None => return packed_ref(dirs, &name),
        }
    }
    None
}

/// Look a ref up in `packed-refs`, stopping at the first match. The file is
/// scanned as bytes so a multi-megabyte packed-refs costs one read plus a
/// linear scan with no per-line allocation.
fn packed_ref(dirs: &Dirs, refname: &str) -> Option<String> {
    let packed = std::fs::read(dirs.common_dir.join("packed-refs")).ok()?;
    let want = refname.as_bytes();
    packed
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty() && l.first() != Some(&b'#') && l.first() != Some(&b'^'))
        .find_map(|l| {
            let space = l.iter().position(|b| *b == b' ')?;
            let (sha, rest) = l.split_at(space);
            let name = rest.get(1..)?.trim_ascii_end();
            (name == want).then(|| String::from_utf8_lossy(sha).into_owned())
        })
}

/// The HEAD commit id, if it can be read without git.
#[must_use]
pub fn head_commit(dirs: &Dirs) -> Option<String> {
    match head(dirs)? {
        Head::Detached(sha) => Some(sha),
        Head::Branch(name) => resolve_ref(dirs, &format!("refs/heads/{name}")),
    }
}

/// The upstream of a branch: `(remote, remote-tracking ref)` such as
/// `("origin", "refs/remotes/origin/main")`.
#[must_use]
pub fn upstream(dirs: &Dirs, branch: &str) -> Option<(String, String)> {
    let text = std::fs::read_to_string(dirs.common_dir.join("config")).ok()?;
    let mut in_section = false;
    let mut remote: Option<String> = None;
    let mut merge: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_section = line == format!("[branch \"{branch}\"]");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "remote" => remote = Some(v.trim().to_owned()),
                "merge" => merge = Some(v.trim().to_owned()),
                _ => {}
            }
        }
    }
    let remote = remote?;
    let merge = merge?;
    let short = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
    if remote == "." {
        return Some((remote, format!("refs/heads/{short}")));
    }
    Some((remote.clone(), format!("refs/remotes/{remote}/{short}")))
}

/// Seconds between the last fetch (`FETCH_HEAD` mtime) and `now_epoch_secs`,
/// if a fetch ever happened. `FETCH_HEAD` is per worktree, so the linked
/// worktree's own git dir is checked first.
#[must_use]
pub fn fetch_age(dirs: &Dirs, now_epoch_secs: i64) -> Option<u64> {
    let modified = [&dirs.git_dir, &dirs.common_dir]
        .into_iter()
        .find_map(|d| std::fs::metadata(d.join("FETCH_HEAD")).ok()?.modified().ok())?;
    let secs = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let now = u64::try_from(now_epoch_secs).ok()?;
    Some(now.saturating_sub(secs))
}

/// Run `git` with arguments in `cwd`, killing it after `timeout`.
///
/// # Errors
/// Returns the stderr text (or a timeout message) on failure.
pub fn run_git(cwd: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    run_program(Path::new("git"), cwd, args, timeout)
}

/// [`run_git`] with an explicit program (tests use a fake git).
///
/// # Errors
/// Returns the stderr text (or a timeout message) on failure.
pub fn run_program(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    use std::io::Read as _;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git: {e}"))?;
    // Drain both pipes on their own threads: a child that writes more than
    // the pipe buffer (64 KiB) before exiting would otherwise block forever.
    let stdout = child.stdout.take().map(|mut p| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = p.read_to_end(&mut buf);
            buf
        })
    });
    let stderr = child.stderr.take().map(|mut p| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = p.read_to_end(&mut buf);
            buf
        })
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "git {} timed out after {}s",
                    args.join(" "),
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = stdout.and_then(|t| t.join().ok()).unwrap_or_default();
    let err = stderr.and_then(|t| t.join().ok()).unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err).trim().to_owned();
    if status.success() {
        Ok(String::from_utf8_lossy(&out).into_owned())
    } else if stderr.is_empty() {
        Err(format!("git {} failed", args.join(" ")))
    } else {
        Err(stderr)
    }
}

/// Ahead/behind counts of HEAD against `upstream_ref`.
///
/// # Errors
/// Propagates git failures.
pub fn ahead_behind(
    cwd: &Path,
    upstream_ref: &str,
    timeout: Duration,
) -> Result<(u64, u64), String> {
    let out = run_git(
        cwd,
        &["rev-list", "--left-right", "--count", &format!("HEAD...{upstream_ref}")],
        timeout,
    )?;
    let mut parts = out.split_whitespace();
    let ahead = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("unexpected rev-list output {out:?}"))?;
    let behind = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("unexpected rev-list output {out:?}"))?;
    Ok((ahead, behind))
}

/// Whether the working tree has staged or unstaged changes (untracked files ignored).
///
/// # Errors
/// Propagates git failures.
pub fn is_dirty(cwd: &Path, timeout: Duration) -> Result<bool, String> {
    let out = run_git(
        cwd,
        &["status", "--porcelain=v2", "--untracked-files=no", "--no-renames"],
        timeout,
    )?;
    Ok(!out.trim().is_empty())
}

/// `git fetch --quiet <remote>`, killed after `timeout` (a hung network
/// fetch must not pin the worker and its lock).
///
/// # Errors
/// Propagates git failures.
pub fn fetch(cwd: &Path, remote: &str, timeout: Duration) -> Result<(), String> {
    run_git(cwd, &["fetch", "--quiet", remote], timeout).map(|_| ())
}

/// `git --version`, for `doctor` (killed after two seconds like every other
/// git call: a hung `git` wrapper must not hang the report).
///
/// # Errors
/// When git is missing, fails, or hangs.
pub fn version() -> Result<String, String> {
    run_git(Path::new("."), &["--version"], Duration::from_secs(2)).map(|v| v.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let st = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(st.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
    }

    /// A repo with a local bare origin, one commit pushed, on branch `main`.
    fn repo() -> (tempfile::TempDir, PathBuf) {
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
        (dir, work)
    }

    #[test]
    fn discovers_dirs_head_upstream_and_commit() {
        let (_d, work) = repo();
        let dirs =
            discover(&work.join("sub").join("deeper")).unwrap_or_else(|| discover(&work).unwrap());
        assert_eq!(dirs.toplevel, work);
        assert!(!dirs.linked_worktree);
        assert_eq!(head(&dirs), Some(Head::Branch("main".into())));
        assert_eq!(
            upstream(&dirs, "main"),
            Some(("origin".into(), "refs/remotes/origin/main".into()))
        );
        let sha = head_commit(&dirs).unwrap();
        assert_eq!(sha.len(), 40);
        assert_eq!(resolve_ref(&dirs, "refs/remotes/origin/main"), Some(sha.clone()));
        // packed refs still resolve
        git(&work, &["pack-refs", "--all"]);
        assert_eq!(resolve_ref(&dirs, "refs/heads/main"), Some(sha));
        assert_eq!(upstream(&dirs, "nope"), None);
        assert!(discover(Path::new("/")).is_none());
    }

    #[test]
    fn worker_helpers_report_ahead_behind_and_dirty() {
        let (_d, work) = repo();
        let t = Duration::from_secs(5);
        assert_eq!(ahead_behind(&work, "refs/remotes/origin/main", t), Ok((0, 0)));
        assert_eq!(is_dirty(&work, t), Ok(false));
        std::fs::write(work.join("a.txt"), "b\n").unwrap();
        assert_eq!(is_dirty(&work, t), Ok(true));
        git(&work, &["commit", "-q", "-am", "two"]);
        assert_eq!(ahead_behind(&work, "refs/remotes/origin/main", t), Ok((1, 0)));
        assert!(ahead_behind(&work, "refs/remotes/origin/ghost", t).is_err());
        assert!(run_git(&work, &["sleep-forever-not-a-command"], t).is_err());
        assert!(fetch(&work, "origin", t).is_ok());
        assert!(fetch_age(&discover(&work).unwrap(), crate::time::now_secs()).is_some());
    }

    #[test]
    fn linked_worktrees_and_detached_heads() {
        let (d, work) = repo();
        let wt = d.path().join("wt");
        git(&work, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
        let dirs = discover(&wt).unwrap();
        assert!(dirs.linked_worktree);
        assert_eq!(dirs.toplevel, wt);
        assert_eq!(dirs.common_dir, work.join(".git"));
        assert_eq!(head(&dirs), Some(Head::Branch("feature".into())));
        assert!(head_commit(&dirs).is_some());
        assert_ne!(dirs.cache_key(), discover(&work).unwrap().cache_key());
        git(&work, &["checkout", "-q", "--detach"]);
        let main = discover(&work).unwrap();
        assert!(matches!(head(&main), Some(Head::Detached(s)) if s.len() == 40));
    }

    #[test]
    fn symref_cycles_and_reftable_repos_do_not_break_the_reader() {
        let (tmp, work) = repo();
        let dirs = discover(&work).unwrap();
        std::fs::write(work.join(".git/refs/heads/loop"), "ref: refs/heads/loop\n").unwrap();
        assert_eq!(resolve_ref(&dirs, "refs/heads/loop"), None);
        std::fs::write(work.join(".git/refs/heads/a"), "ref: refs/heads/b\n").unwrap();
        std::fs::write(work.join(".git/refs/heads/b"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(resolve_ref(&dirs, "refs/heads/a"), head_commit(&dirs));
        std::fs::write(work.join(".git/HEAD"), "ref: refs/heads/loop\n").unwrap();
        assert_eq!(head(&dirs), Some(Head::Branch("loop".into())));
        assert_eq!(head_commit(&dirs), None);

        let rt = tmp.path().join("rt");
        let out = Command::new("git")
            .args(["init", "-q", "--ref-format=reftable", rt.to_str().unwrap()])
            .output()
            .unwrap();
        if out.status.success() {
            let dirs = discover(&rt).unwrap();
            assert!(dirs.uses_reftable());
            assert_eq!(head(&dirs), None, "reftable HEAD is a placeholder, never `.invalid`");
            assert_eq!(resolve_ref(&dirs, "refs/heads/main"), None);
        }
    }

    #[test]
    fn packed_refs_scan_handles_peeled_tags_and_stops_at_first_match() {
        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        let sha = head_commit(&dirs).unwrap();
        git(&work, &["tag", "-a", "-m", "t", "v1"]);
        git(&work, &["pack-refs", "--all"]);
        let packed = std::fs::read_to_string(work.join(".git/packed-refs")).unwrap();
        assert!(packed.lines().any(|l| l.starts_with('^')), "{packed}");
        assert_eq!(resolve_ref(&dirs, "refs/heads/main"), Some(sha));
        assert!(resolve_ref(&dirs, "refs/tags/v1").is_some());
        assert_eq!(resolve_ref(&dirs, "refs/heads/mai"), None);
    }

    #[test]
    fn fetch_age_is_per_worktree() {
        let (d, work) = repo();
        let wt = d.path().join("wt");
        git(&work, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
        git(&wt, &["fetch", "-q", "origin"]);
        let linked = discover(&wt).unwrap();
        assert!(linked.git_dir.join("FETCH_HEAD").exists());
        let now = crate::time::now_secs();
        assert!(fetch_age(&linked, now).is_some());
        assert!(fetch_age(&linked, now).unwrap() < 60);
    }

    #[test]
    fn large_output_does_not_deadlock_the_worker() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("git");
        std::fs::write(&fake, "#!/bin/sh\nhead -c 300000 /dev/zero | tr '\\0' 'x'\nexit 0\n")
            .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = std::time::Instant::now();
        let out = run_program(&fake, dir.path(), &["status"], Duration::from_secs(5)).unwrap();
        assert_eq!(out.len(), 300_000);
        assert!(started.elapsed() < Duration::from_secs(4));
        let noisy = dir.path().join("noisy");
        std::fs::write(&noisy, "#!/bin/sh\nhead -c 200000 /dev/zero >&2\nexit 3\n").unwrap();
        std::fs::set_permissions(&noisy, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(run_program(&noisy, dir.path(), &["x"], Duration::from_secs(5)).is_err());
    }

    #[test]
    fn timeout_kills_slow_git() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("git");
        std::fs::write(&fake, "#!/bin/sh\nsleep 5\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = std::time::Instant::now();
        let r = run_program(&fake, dir.path(), &["status"], Duration::from_millis(200));
        assert!(r.is_err(), "{r:?}");
        assert!(r.unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
