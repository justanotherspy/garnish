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

/// Read HEAD.
#[must_use]
pub fn head(dirs: &Dirs) -> Option<Head> {
    let text = std::fs::read_to_string(dirs.git_dir.join("HEAD")).ok()?;
    let line = text.lines().next()?.trim();
    if let Some(r) = line.strip_prefix("ref:") {
        let r = r.trim();
        return Some(Head::Branch(r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()));
    }
    (!line.is_empty()).then(|| Head::Detached(line.to_owned()))
}

/// Resolve a full ref name (`refs/heads/main`) to a commit id via loose refs
/// or `packed-refs`. `None` for reftable repositories or unknown refs.
#[must_use]
pub fn resolve_ref(dirs: &Dirs, refname: &str) -> Option<String> {
    if dirs.uses_reftable() {
        return None;
    }
    for base in [&dirs.git_dir, &dirs.common_dir] {
        if let Ok(text) = std::fs::read_to_string(base.join(refname)) {
            let line = text.lines().next()?.trim();
            if let Some(r) = line.strip_prefix("ref:") {
                return resolve_ref(dirs, r.trim());
            }
            if !line.is_empty() {
                return Some(line.to_owned());
            }
        }
    }
    let packed = std::fs::read_to_string(dirs.common_dir.join("packed-refs")).ok()?;
    packed.lines().filter(|l| !l.starts_with('#') && !l.starts_with('^')).find_map(|l| {
        l.split_once(' ').filter(|(_, name)| name.trim() == refname).map(|(sha, _)| sha.to_owned())
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

/// Seconds since the last fetch (`FETCH_HEAD` mtime), if any.
#[must_use]
pub fn fetch_age(dirs: &Dirs) -> Option<u64> {
    let meta = std::fs::metadata(dirs.common_dir.join("FETCH_HEAD")).ok()?;
    let modified = meta.modified().ok()?;
    let secs = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let now = u64::try_from(crate::time::now_secs()).ok()?;
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
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child.wait_with_output().map_err(|e| e.to_string())?;
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
                return if status.success() {
                    Ok(stdout)
                } else {
                    Err(if stderr.is_empty() {
                        format!("git {} failed", args.join(" "))
                    } else {
                        stderr
                    })
                };
            }
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

/// `git fetch --quiet <remote>`.
///
/// # Errors
/// Propagates git failures.
pub fn fetch(cwd: &Path, remote: &str, timeout: Duration) -> Result<(), String> {
    use command_run::Command;
    let mut cmd = Command::with_args("git", ["fetch", "--quiet", remote]);
    cmd.set_dir(cwd);
    cmd.env.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
    cmd.enable_capture();
    cmd.log_command = false;
    let _ = timeout; // command-run has no timeout; fetch is only run opt-in in the worker.
    cmd.run().map(|_| ()).map_err(|e| e.to_string())
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
        assert!(fetch_age(&discover(&work).unwrap()).is_some());
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
