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
///
/// A `.git` *file* (a linked worktree, a submodule) names its git
/// directory, and a `commondir` file inside that names the shared one.
/// Both come from the checkout, which is not the user's file, and every
/// later read is contained in the directories they name, so each must be a
/// git directory by git's own test ([`is_git_directory`]) before it is
/// used: a `.git` file naming anything else is no repository (git says
/// "not a git repository" there too), and a `commondir` naming anything
/// else is ignored.
#[must_use]
pub fn discover(path: &Path) -> Option<Dirs> {
    let mut dir = if path.is_dir() { path.to_path_buf() } else { path.parent()?.to_path_buf() };
    for _ in 0..64 {
        let dot = dir.join(".git");
        if dot.is_dir() {
            let common = common_dir(&dot);
            return Some(Dirs { toplevel: dir, git_dir: dot, common_dir: common });
        }
        if dot.is_file() {
            let text = String::from_utf8(read_bounded(&dot, MAX_REF_BYTES)?).ok()?;
            let target = text.lines().next()?.trim().strip_prefix("gitdir:")?.trim();
            let git_dir = if Path::new(target).is_absolute() {
                PathBuf::from(target)
            } else {
                dir.join(target)
            };
            let git_dir = normalize(&git_dir);
            let common = common_dir(&git_dir);
            if !is_git_directory(&git_dir, &common) {
                return None;
            }
            return Some(Dirs { toplevel: dir, git_dir, common_dir: common });
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

/// The shared git directory `git_dir/commondir` names, or `git_dir` itself
/// when there is no such file, it cannot be read as a short regular file,
/// or what it names has no `objects/` and `refs/` of its own.
fn common_dir(git_dir: &Path) -> PathBuf {
    let named = read_bounded(&git_dir.join("commondir"), MAX_REF_BYTES)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| {
            let target = text.lines().next()?.trim().to_owned();
            (!target.is_empty()).then(|| {
                normalize(&if Path::new(&target).is_absolute() {
                    PathBuf::from(target)
                } else {
                    git_dir.join(target)
                })
            })
        });
    named.filter(|common| has_object_store(common)).unwrap_or_else(|| git_dir.to_path_buf())
}

/// git's `is_git_directory` (setup.c), by `stat` alone: a `HEAD` in the
/// per-worktree directory and an object store in the common one.
fn is_git_directory(git_dir: &Path, common: &Path) -> bool {
    git_dir.join("HEAD").is_file() && has_object_store(common)
}

/// The half of [`is_git_directory`] a common directory answers for.
fn has_object_store(common: &Path) -> bool {
    common.join("objects").is_dir() && common.join("refs").is_dir()
}

/// At most `max` bytes of the regular file at `path`, or `None` when
/// there is none or it is not one.
///
/// Every file under `.git` is read through here. `open` on a FIFO waits
/// for a writer that never comes, and a link to `/dev/zero` never ends;
/// an archive can carry either, and the tick would repeat the read every
/// second. [`crate::claude_settings::read_regular`] refuses what is not a
/// regular file before opening it and stops at the cap.
fn read_bounded(path: &Path, max: u64) -> Option<Vec<u8>> {
    crate::claude_settings::read_regular(path, max).ok().flatten()
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
    let text = read_ref_file(&dirs.git_dir, "HEAD")?;
    let line = text.lines().next()?.trim();
    if let Some(r) = line.strip_prefix("ref:") {
        let r = r.trim();
        return Some(Head::Branch(r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()));
    }
    (!line.is_empty()).then(|| Head::Detached(line.to_owned()))
}

/// Symbolic refs deeper than this are treated as broken (git's own limit).
const SYMREF_MAX_DEPTH: usize = 5;

/// Whether a ref name may be joined onto the git directory.
///
/// A ref is read by opening `<git dir>/<name>`, so a name holding `..` walks
/// out of the repository: a `.git/HEAD` saying `ref: ../../../secret` made
/// `branch` render the first seven characters of that file as the short SHA
/// of a checkout the user never created (an unpacked archive, a shared
/// directory). Every `ref:` hop goes through here, so a chain cannot smuggle
/// one in either. This is git's own `check-ref-format` rule, narrowed to
/// what the join needs: no empty, `.` or `..` component, and nothing
/// absolute.
///
/// A name rule alone is not enough, because the same archive can carry
/// symlinks: [`read_ref_file`] is the path rule that goes with it.
fn joinable_ref(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Where a symbolic ref may point, as git's `refname_is_safe` has it: under
/// `refs/`, or a one-level pseudo-ref in capitals (`HEAD`, `FETCH_HEAD`).
///
/// [`joinable_ref`] keeps a hop inside the git directory; this keeps it to
/// the files git itself would follow, so `ref: config` or `ref: key` in a
/// directory a hostile `commondir` named is not read as a commit id.
fn safe_symref(name: &str) -> bool {
    name.starts_with("refs/")
        || (!name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
}

/// Bytes of a ref file read: one short line, so anything near this is not
/// one (see [`read_ref_file`]).
const MAX_REF_BYTES: u64 = 64 * 1024;

/// `packed-refs` is a real file in a real repository and a large one in a
/// big repository, so its bound is generous where a ref's is tight.
const MAX_PACKED_REFS_BYTES: u64 = 16 * 1024 * 1024;

/// Read a ref file, but only when it really is inside the git directory.
///
/// [`joinable_ref`] keeps `..` out of the *name*; this keeps the *file* in,
/// which is the property actually wanted. A tar archive may carry symlinks,
/// so `.git/HEAD` or `refs/heads/main` can be a link to any file on disk, and
/// a link on an intermediate directory (`.git/refs/heads` → `/etc`) needs no
/// suspicious name at all. Resolving both sides and comparing catches every
/// shape of that. Git has not written a symbolic ref as a symlink since
/// `core.prefersymlinkrefs` was deprecated, so nothing legitimate is refused.
/// The [`MAX_REF_BYTES`] cap is also what keeps a hostile `.git/HEAD` from
/// becoming a branch name the size of the file: `head` takes the whole
/// first line, and every render that cuts it (`branch.max_length`) works
/// over its clusters.
fn read_ref_file(base: &Path, name: &str) -> Option<String> {
    String::from_utf8(read_ref_bytes(base, name, MAX_REF_BYTES)?).ok()
}

/// [`read_ref_file`] as bytes, with the size cap the caller needs.
fn read_ref_bytes(base: &Path, name: &str, max: u64) -> Option<Vec<u8>> {
    read_under(&base.canonicalize().ok()?, name, max)
}

/// [`read_ref_bytes`] given an already-resolved directory.
///
/// `resolve_ref` reads up to five hops across two directories, so resolving
/// the *base* inside the read would repeat the same `realpath` up to ten
/// times on a warm tick, where the old code did none. It is resolved once
/// per directory and only the target is resolved per read.
fn read_under(root: &Path, name: &str, max: u64) -> Option<Vec<u8>> {
    let path = root.join(name).canonicalize().ok()?;
    if !path.starts_with(root) {
        return None;
    }
    read_bounded(&path, max)
}

/// Resolve a full ref name (`refs/heads/main`) to a commit id.
///
/// Loose refs are tried first, then `packed-refs`. `None` for reftable
/// repositories, unknown refs, symbolic-ref chains longer than five
/// (cycles included), matching git's own limit, and a hop to anything git
/// would not follow ([`safe_symref`]).
#[must_use]
pub fn resolve_ref(dirs: &Dirs, refname: &str) -> Option<String> {
    if dirs.uses_reftable() {
        return None;
    }
    // Resolved once for the whole walk, not once per hop per directory; a
    // main worktree's two directories are one.
    let mut roots: Vec<PathBuf> = [&dirs.git_dir, &dirs.common_dir]
        .into_iter()
        .filter_map(|d| d.canonicalize().ok())
        .collect();
    roots.dedup();
    let mut name = refname.to_owned();
    for _ in 0..SYMREF_MAX_DEPTH {
        if !joinable_ref(&name) || !safe_symref(&name) {
            return None;
        }
        let mut next: Option<String> = None;
        for base in &roots {
            let Some(text) =
                read_under(base, &name, MAX_REF_BYTES).and_then(|b| String::from_utf8(b).ok())
            else {
                continue;
            };
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
///
/// It goes through [`read_ref_bytes`] like every other ref read: it is the
/// fallback whenever a loose ref is absent, so a symlinked `packed-refs`
/// would be the same way out of the repository as a symlinked `HEAD`.
fn packed_ref(dirs: &Dirs, refname: &str) -> Option<String> {
    let packed = read_ref_bytes(&dirs.common_dir, "packed-refs", MAX_PACKED_REFS_BYTES)?;
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

/// The commit id `head` (what [`head`] read) points at, if it can be read
/// without git. The head is passed in so a tick that read it once reads
/// the same one everywhere, even while a checkout rewrites `HEAD`.
#[must_use]
pub fn head_commit(dirs: &Dirs, head: &Head) -> Option<String> {
    match head {
        Head::Detached(sha) => Some(sha.clone()),
        Head::Branch(name) => resolve_ref(dirs, &format!("refs/heads/{name}")),
    }
}

/// The upstream of a branch: `(remote, remote-tracking ref)` such as
/// `("origin", "refs/remotes/origin/main")`.
#[must_use]
pub fn upstream(dirs: &Dirs, branch: &str) -> Option<(String, String)> {
    // Through the same contained, bounded reader as every ref: `config` sits
    // under the git directory and is as symlinkable as `HEAD` is.
    let text = read_ref_file(&dirs.common_dir, "config")?;
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

/// Config keys that make git run a command, cleared on every call.
///
/// The repository's `.git/config` is not the user's file in a checkout they
/// did not create, and `core.fsmonitor` is a command `git status` starts on
/// its own. The user typing `git status` there would run it too, so this is
/// not a new trust boundary; what is new is that garnish runs git on a
/// *timer*, without anyone asking. `-c` on the command line beats the file.
///
/// Clearing it costs nothing: the monitor is a speed hint for large working
/// trees and git falls back to walking them, which is what the 2 s timeout
/// is for. Keys that only a network transport reaches (`core.sshCommand`,
/// `core.gitProxy`, an `ext::` URL) are not cleared, because each is a
/// setting a user may legitimately want honoured and only an opted-in
/// `fetch_interval` reaches them; PLAN's backlog carries that decision.
const NO_COMMAND_HOOKS: [&str; 2] = ["-c", "core.fsmonitor="];

/// Run `git` with arguments in `cwd`, killing it after `timeout`.
///
/// # Errors
/// Returns the stderr text (or a timeout message) on failure.
pub fn run_git(cwd: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let args: Vec<&str> = NO_COMMAND_HOOKS.into_iter().chain(args.iter().copied()).collect();
    run_program_wanting(Path::new("git"), cwd, &args, timeout, Stdout::Read)
}

/// [`run_git`] for a command whose stdout the caller throws away.
///
/// Only the exit status matters, so a stdout read that has to be abandoned
/// is not a failure: `fetch` runs `--quiet` and is precisely the call whose
/// pipes an ssh `ControlPersist` master holds open, so treating that as an
/// error recorded a fetch that worked as one that did not.
///
/// # Errors
/// Returns the stderr text (or a timeout message) on failure.
fn run_git_quiet(cwd: &Path, args: &[&str], timeout: Duration) -> Result<(), String> {
    let args: Vec<&str> = NO_COMMAND_HOOKS.into_iter().chain(args.iter().copied()).collect();
    run_program_wanting(Path::new("git"), cwd, &args, timeout, Stdout::Discard).map(|_| ())
}

/// Whether the caller reads what the child wrote to stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stdout {
    /// The output is the answer, so failing to read it is a failure.
    Read,
    /// Only the exit status matters.
    Discard,
}

/// How long the pipes are still read after the child has exited and the
/// timeout is already spent. The child's own ends close with it, so this
/// bounds one case only: a descendant still holding them (see [`drain`]).
const DRAIN_FLOOR: Duration = Duration::from_millis(250);

/// Read a pipe to the end on its own thread, delivering the bytes once.
///
/// A channel rather than a join handle, so the caller can put a deadline on
/// the read. Joining has none: the write end stays open while *any*
/// descendant holds it, not only the child — ssh's `ControlPersist` master
/// outlives the `git fetch` that started it — and the worker would then sit
/// in `read_to_end` for ever with its lock held, whatever timeout was asked
/// for. A thread left behind does not hold the process up; it is dropped
/// when the worker exits.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(mut pipe) = pipe {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    rx
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
    run_program_wanting(program, cwd, args, timeout, Stdout::Read)
}

/// [`run_program`], told whether the caller will read the output.
fn run_program_wanting(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    timeout: Duration,
    want: Stdout,
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
    // Drain both pipes on their own threads: a child that writes more than
    // the pipe buffer (64 KiB) before exiting would otherwise block forever.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
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
    // What is left of the budget, never below a floor: the child has exited,
    // so its pipes are normally closed already and both arrive at once.
    let left = || timeout.saturating_sub(start.elapsed()).max(DRAIN_FLOOR);
    // A read that gave up is an error, never an empty answer, for a caller
    // that reads it: `is_dirty` takes "no output" for "clean", so `Ok("")`
    // here would put a fabricated value in the cache for a whole TTL
    // instead of a `✗`. For a caller that discards it, nothing was lost.
    let out = match (stdout.recv_timeout(left()), want) {
        (Ok(out), _) => out,
        (Err(_), Stdout::Discard) => Vec::new(),
        (Err(_), Stdout::Read) => {
            return Err(format!("git {} wrote no output before the timeout", args.join(" ")));
        }
    };
    // stderr only decorates a failure, so a lost one costs the message, not
    // the answer.
    let err = stderr.recv_timeout(left()).unwrap_or_default();
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
/// The remote is the one named in the repository's own `.git/config`, which
/// is not the user's file in a checkout they did not create. Two things
/// follow from that, and the first is not the whole of it:
///
/// - a name starting with `-` would be read by git as an option rather than
///   a remote, and `--upload-pack=<cmd>` runs `<cmd>`, so the name is
///   refused and passed after `--`;
/// - the same file can set `remote.<name>.uploadpack`, which needs no
///   suspicious name at all. `--upload-pack` on the command line beats it.
///   Overriding it loses nothing but a per-remote server path, which is rare
///   where a hostile checkout getting a command run is not.
///
/// `core.sshCommand`, `core.gitProxy` and an `ext::` URL remain: each is a
/// setting a user may legitimately want honoured, and `fetch_interval`
/// defaults to 0, so nothing reaches them until the user opts in. PLAN's
/// backlog carries that decision.
///
/// # Errors
/// Propagates git failures; refuses a remote that is not a plain name.
pub fn fetch(cwd: &Path, remote: &str, timeout: Duration) -> Result<(), String> {
    if remote.is_empty() || remote.starts_with('-') {
        return Err(format!("refusing to fetch from remote {remote:?}"));
    }
    let args = ["fetch", "--quiet", "--upload-pack", "git-upload-pack", "--", remote];
    run_git_quiet(cwd, &args, timeout)
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

    /// `git` in a temp repository, cut off from the developer's own config:
    /// this project mandates signed commits, so the machines that run this
    /// suite are the machines with `commit.gpgsign = true`, and a commit
    /// here cannot reach a pinentry.
    fn git(dir: &Path, args: &[&str]) {
        let st = Command::new("git")
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

    /// HEAD's commit, read the way the tick reads it.
    fn commit_of(dirs: &Dirs) -> Option<String> {
        head(dirs).and_then(|h| head_commit(dirs, &h))
    }

    #[test]
    fn discovers_dirs_head_upstream_and_commit() {
        let (_d, work) = repo();
        let dirs =
            discover(&work.join("sub").join("deeper")).unwrap_or_else(|| discover(&work).unwrap());
        assert_eq!(dirs.toplevel, work);
        assert_eq!(dirs.common_dir, dirs.git_dir);
        assert_eq!(head(&dirs), Some(Head::Branch("main".into())));
        assert_eq!(
            upstream(&dirs, "main"),
            Some(("origin".into(), "refs/remotes/origin/main".into()))
        );
        let sha = commit_of(&dirs).unwrap();
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

    /// SPEC § 9: behind and diverged, against a commit pushed from a second
    /// clone. The counts read the remote-tracking ref on disk, so nothing
    /// changes until a fetch brings the other clone's commit in.
    #[test]
    fn behind_and_diverged_counts_follow_the_fetched_tracking_ref() {
        let (d, work) = repo();
        let t = Duration::from_secs(5);
        let origin = d.path().join("origin.git");
        let other = d.path().join("other");
        git(d.path(), &["clone", "-q", origin.to_str().unwrap(), other.to_str().unwrap()]);
        std::fs::write(other.join("o.txt"), "o\n").unwrap();
        git(&other, &["add", "."]);
        git(&other, &["commit", "-q", "-m", "theirs"]);
        git(&other, &["push", "-q", "origin", "main"]);
        assert_eq!(ahead_behind(&work, "refs/remotes/origin/main", t), Ok((0, 0)), "not fetched");
        assert!(fetch(&work, "origin", t).is_ok());
        assert_eq!(ahead_behind(&work, "refs/remotes/origin/main", t), Ok((0, 1)), "behind");
        std::fs::write(work.join("m.txt"), "m\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "mine"]);
        assert_eq!(ahead_behind(&work, "refs/remotes/origin/main", t), Ok((1, 1)), "diverged");
        // A branch without an upstream has no tracking ref to count against.
        git(&work, &["checkout", "-q", "-b", "local"]);
        assert_eq!(upstream(&discover(&work).unwrap(), "local"), None);
    }

    #[test]
    fn linked_worktrees_and_detached_heads() {
        let (d, work) = repo();
        let wt = d.path().join("wt");
        git(&work, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
        let dirs = discover(&wt).unwrap();
        assert_ne!(dirs.common_dir, dirs.git_dir);
        assert_eq!(dirs.toplevel, wt);
        // git reports the common dir canonicalised; on macOS the temp dir
        // sits behind the `/var` → `/private/var` symlink.
        assert_eq!(
            dirs.common_dir.canonicalize().unwrap(),
            work.join(".git").canonicalize().unwrap()
        );
        assert_eq!(head(&dirs), Some(Head::Branch("feature".into())));
        assert!(commit_of(&dirs).is_some());
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
        assert_eq!(resolve_ref(&dirs, "refs/heads/a"), commit_of(&dirs));
        std::fs::write(work.join(".git/HEAD"), "ref: refs/heads/loop\n").unwrap();
        assert_eq!(head(&dirs), Some(Head::Branch("loop".into())));
        assert_eq!(commit_of(&dirs), None);

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

    /// A ref name is joined onto the git directory, so it must never walk
    /// out of it. A checkout the user did not create (an unpacked archive,
    /// a shared directory) could put `ref: ../../../secret` in `.git/HEAD`
    /// and see the first seven characters of that file rendered as the
    /// branch's short SHA.
    #[test]
    fn a_ref_name_can_never_walk_out_of_the_git_directory() {
        for bad in [
            "refs/heads/../../../secret",
            "refs/heads/..",
            "../../secret",
            "/etc/hostname",
            "refs//heads/main",
            "refs/heads/./main",
            "",
        ] {
            assert!(!joinable_ref(bad), "{bad:?} must be refused");
        }
        for good in ["refs/heads/main", "refs/remotes/origin/feature/x", "HEAD", "refs/tags/v1"] {
            assert!(joinable_ref(good), "{good:?} must be allowed");
        }

        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        let secret = work.join("secret.txt");
        std::fs::write(&secret, "SECRETVALUE\n").unwrap();
        // `.git/refs/heads/../../../secret.txt` is `<work>/secret.txt`.
        std::fs::write(work.join(".git/HEAD"), "ref: ../../../secret.txt\n").unwrap();
        assert_eq!(head(&dirs), Some(Head::Branch("../../../secret.txt".into())));
        assert_eq!(commit_of(&dirs), None, "the file must not be read");
        // Nor through a symbolic-ref hop.
        std::fs::write(work.join(".git/HEAD"), "ref: refs/heads/hop\n").unwrap();
        std::fs::write(work.join(".git/refs/heads/hop"), "ref: ../../../secret.txt\n").unwrap();
        assert_eq!(commit_of(&dirs), None, "the file must not be read through a hop");
    }

    /// The name rule is not the whole of it: the same archive that carries a
    /// hostile `.git` can carry symlinks, and a link needs no suspicious
    /// name. Three shapes, all of which read a file outside the git
    /// directory through `read_to_string`, which follows links.
    #[test]
    fn a_symlinked_ref_can_never_read_a_file_outside_the_git_directory() {
        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        let secret = work.join("secret.txt");
        std::fs::write(&secret, "SECRETVALUE\n").unwrap();
        let link = |from: &Path, to: &Path| {
            let _ = std::fs::remove_file(from);
            std::os::unix::fs::symlink(to, from).unwrap();
        };

        // 1. HEAD itself is a link.
        link(&work.join(".git/HEAD"), &secret);
        assert_eq!(head(&dirs), None, "a linked HEAD must not be read");

        // 2. HEAD is honest and the ref it names is a link.
        std::fs::write(work.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        link(&work.join(".git/refs/heads/main"), &secret);
        assert_eq!(commit_of(&dirs), None, "a linked ref must not be read");

        // 3. Nothing on the path is suspicious and a *directory* is the link.
        std::fs::remove_file(work.join(".git/refs/heads/main")).unwrap();
        std::fs::remove_dir_all(work.join(".git/refs/heads")).unwrap();
        link(&work.join(".git/refs/heads"), &work);
        assert_eq!(resolve_ref(&dirs, "refs/heads/secret.txt"), None, "through a linked dir");

        // 4. `packed-refs` is the fallback whenever a loose ref is absent,
        //    so a link there is the same door.
        let (_d2, work2) = repo();
        let dirs2 = discover(&work2).unwrap();
        let sha = commit_of(&dirs2).unwrap();
        git(&work2, &["pack-refs", "--all"]);
        assert_eq!(resolve_ref(&dirs2, "refs/heads/main"), Some(sha), "packed refs still resolve");
        let elsewhere = work2.join("packed-elsewhere");
        std::fs::write(&elsewhere, "deadbeef refs/heads/main\n").unwrap();
        link(&work2.join(".git/packed-refs"), &elsewhere);
        assert_eq!(resolve_ref(&dirs2, "refs/heads/main"), None, "a linked packed-refs is refused");
    }

    /// A ref file is one short line, so a huge one is not a ref. The cap is
    /// what stops a hostile `.git/HEAD` becoming a branch name the size of
    /// the file, which every render that cuts it then walks cluster by
    /// cluster on the tick path.
    #[test]
    fn a_ref_file_is_bounded_so_a_huge_head_cannot_become_a_branch_name() {
        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        let huge = "a".repeat(usize::try_from(MAX_REF_BYTES).unwrap_or(0) * 2);
        std::fs::write(work.join(".git/HEAD"), format!("ref: refs/heads/{huge}\n")).unwrap();
        let name = match head(&dirs) {
            Some(Head::Branch(n)) => n,
            other => panic!("expected a branch, got {other:?}"),
        };
        assert!(
            u64::try_from(name.len()).is_ok_and(|n| n <= MAX_REF_BYTES),
            "the name is {} bytes, past the cap",
            name.len()
        );
    }

    /// `f` on a thread, or `None` when it has not returned within five
    /// seconds: the shape of a read that blocks for ever.
    fn within_5s<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(5)).ok()
    }

    /// Every file under `.git` is read on the tick path, so none may block
    /// or read without bound: `open` on a FIFO waits for a writer that never
    /// comes, and a `commondir` linked to `/dev/zero` reads until the
    /// allocation fails. An archive carries both (tar extracts FIFOs for
    /// anyone), and the tick repeats the read every second.
    #[test]
    fn a_fifo_or_an_endless_file_under_git_never_blocks_a_read() {
        let fifo = crate::claude_settings::tests::fifo;
        let (_d, work) = repo();
        let git_dir = work.join(".git");

        // 1. HEAD is a FIFO.
        std::fs::rename(git_dir.join("HEAD"), git_dir.join("HEAD.real")).unwrap();
        if fifo(&git_dir.join("HEAD")).is_none() {
            return;
        }
        let dirs = discover(&work).unwrap();
        let d = dirs.clone();
        assert_eq!(within_5s(move || head(&d)), Some(None), "a FIFO HEAD must not block");
        std::fs::remove_file(git_dir.join("HEAD")).unwrap();
        std::fs::rename(git_dir.join("HEAD.real"), git_dir.join("HEAD")).unwrap();

        // 2. `packed-refs` is a FIFO: the fallback of every absent loose ref.
        fifo(&git_dir.join("packed-refs")).unwrap();
        let d = dirs.clone();
        assert_eq!(within_5s(move || resolve_ref(&d, "refs/heads/nope")), Some(None));
        std::fs::remove_file(git_dir.join("packed-refs")).unwrap();

        // 3. `config` is a FIFO.
        std::fs::rename(git_dir.join("config"), git_dir.join("config.real")).unwrap();
        fifo(&git_dir.join("config")).unwrap();
        assert_eq!(within_5s(move || upstream(&dirs, "main")), Some(None));
        std::fs::remove_file(git_dir.join("config")).unwrap();
        std::fs::rename(git_dir.join("config.real"), git_dir.join("config")).unwrap();

        // 4. `commondir` is a FIFO, then a link to an endless file.
        fifo(&git_dir.join("commondir")).unwrap();
        let w = work.clone();
        let found = within_5s(move || discover(&w)).expect("discover blocked on a FIFO commondir");
        assert_eq!(found.map(|d| d.common_dir), Some(git_dir.clone()));
        std::fs::remove_file(git_dir.join("commondir")).unwrap();
        std::os::unix::fs::symlink("/dev/zero", git_dir.join("commondir")).unwrap();
        let w = work.clone();
        let found = within_5s(move || discover(&w)).expect("discover read /dev/zero");
        assert_eq!(found.map(|d| d.common_dir), Some(git_dir.clone()));
        std::fs::remove_file(git_dir.join("commondir")).unwrap();

        // 5. A linked worktree's `.git` file is bounded too: a sparse one of
        //    64 GiB is not a gitdir line.
        let wt = work.parent().unwrap().join("wt");
        git(&work, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
        let dot = std::fs::File::options().write(true).open(wt.join(".git")).unwrap();
        dot.set_len(1 << 36).unwrap();
        let found = within_5s(move || discover(&wt)).expect("a huge .git file was read whole");
        // Canonicalised: macOS's temp dir sits behind `/var` → `/private/var`.
        let common = found.and_then(|d| d.common_dir.canonicalize().ok());
        assert_eq!(common, git_dir.canonicalize().ok(), "the gitdir line still counts");
    }

    /// The containment rule is relative to directories the checkout names
    /// itself (`commondir`, a `.git` file's `gitdir:`), so those must be git
    /// directories before anything is read under them, and a symbolic ref
    /// may only point where git lets one point: `refs/…` or a pseudo-ref.
    /// Otherwise `commondir: /home/u/.ssh` plus a ref `ref: id_ed25519`
    /// renders the key's first line as a short SHA.
    #[test]
    fn a_commondir_or_gitdir_that_is_not_a_git_directory_is_never_read() {
        let (d, work) = repo();
        let secret = d.path().join("secret");
        std::fs::create_dir_all(&secret).unwrap();
        std::fs::write(secret.join("key"), "SECRETVALUE\n").unwrap();
        let git_dir = work.join(".git");
        std::fs::write(git_dir.join("commondir"), format!("{}\n", secret.display())).unwrap();
        std::fs::write(git_dir.join("refs/heads/x"), "ref: key\n").unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/x\n").unwrap();
        let dirs = discover(&work).unwrap();
        assert_ne!(dirs.common_dir, secret, "a commondir without objects/ and refs/ is refused");
        assert_eq!(head_commit(&dirs, &Head::Branch("x".into())), None);
        // The hop is refused even where the directory is a real one: `key`
        // is neither under `refs/` nor a pseudo-ref.
        std::fs::remove_file(git_dir.join("commondir")).unwrap();
        std::fs::write(git_dir.join("key"), "SECRETVALUE\n").unwrap();
        assert_eq!(resolve_ref(&dirs, "refs/heads/x"), None);
        assert!(safe_symref("refs/heads/main") && safe_symref("HEAD") && safe_symref("FETCH_HEAD"));
        assert!(!safe_symref("key") && !safe_symref("Head") && !safe_symref(""));

        // A `.git` file naming a directory that is not a git directory is
        // no repository at all, as git says.
        let fake = d.path().join("fake");
        std::fs::create_dir_all(&fake).unwrap();
        std::fs::write(fake.join(".git"), format!("gitdir: {}\n", secret.display())).unwrap();
        assert_eq!(discover(&fake), None);
    }

    /// The remote comes from the repository's own `.git/config`, so a name
    /// git would read as an option (`--upload-pack=<cmd>` runs `<cmd>`) is
    /// refused before the process starts.
    #[test]
    fn fetch_refuses_a_remote_that_git_would_read_as_an_option() {
        let (_d, work) = repo();
        for bad in ["--upload-pack=touch /tmp/pwned", "-o", ""] {
            let err = fetch(&work, bad, Duration::from_secs(2)).unwrap_err();
            assert!(err.starts_with("refusing to fetch"), "{bad:?}: {err}");
        }
        // A plain name still reaches git (there is no such remote here, so
        // git itself reports it — the point is that it ran).
        let err = fetch(&work, "nope", Duration::from_secs(5)).unwrap_err();
        assert!(!err.starts_with("refusing to fetch"), "{err}");
    }

    /// The timeout bounds the whole call, not only the wait: a child that
    /// exits while a grandchild keeps the pipes open used to leave the
    /// worker in `read_to_end` for ever with its lock held.
    ///
    /// Giving up on the read is an *error*, never an empty answer, and both
    /// halves need asserting: the first version of this test ran a fake git
    /// that printed nothing and checked only `is_ok()` and the clock, so it
    /// passed just as happily while `run_program` swallowed real output and
    /// returned `Ok("")`, which `is_dirty` reads as a clean tree.
    #[test]
    fn a_grandchild_holding_the_pipes_cannot_outlast_the_timeout() {
        let (tmp, work) = repo();
        let write = |name: &str, body: &str| {
            let p = tmp.path().join(name);
            std::fs::write(&p, body).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        // The grandchild inherits the pipes and outlives the child, so the
        // write end never closes and the read has to be abandoned.
        let leaky = write("git-leaky", "#!/bin/sh\nsleep 30 &\nprintf 'M file\\n'\nexit 0\n");
        let started = std::time::Instant::now();
        let out = run_program(&leaky, &work, &["status"], Duration::from_millis(500));
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
        let err = out.expect_err("a drained-out read must not pass as an empty answer");
        assert!(err.contains("wrote no output before the timeout"), "{err}");

        // The same output without the grandchild arrives in full: the
        // deadline is on the pipes, not on every call.
        let clean = write("git-clean", "#!/bin/sh\nprintf 'M file\\n'\nexit 0\n");
        let out = run_program(&clean, &work, &["status"], Duration::from_millis(500));
        assert_eq!(out.as_deref(), Ok("M file\n"));

        // A caller that discards stdout loses nothing when the read is
        // abandoned, so the same grandchild must not turn a command that
        // *worked* into a failure. `fetch` is that caller, runs `--quiet`,
        // and is the very call an ssh `ControlPersist` master outlives.
        let started = std::time::Instant::now();
        let out = run_program_wanting(
            &leaky,
            &work,
            &["fetch"],
            Duration::from_millis(500),
            Stdout::Discard,
        );
        assert_eq!(out.as_deref(), Ok(""), "a discarded read is not a failure");
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
        // It still fails when the command itself does, in git's own words
        // (nothing holds the pipes here, so the stderr does arrive).
        let bad = write("git-bad", "#!/bin/sh\necho boom >&2\nexit 1\n");
        let out = run_program_wanting(
            &bad,
            &work,
            &["fetch"],
            Duration::from_millis(500),
            Stdout::Discard,
        );
        assert_eq!(out.as_deref().map_err(String::as_str), Err("boom"));
    }

    #[test]
    fn packed_refs_scan_handles_peeled_tags_and_stops_at_first_match() {
        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        let sha = commit_of(&dirs).unwrap();
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
