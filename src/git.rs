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
/// git directory by git's own test (setup.c `is_git_directory`) before it is
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

/// A stamp that changes whenever a reftable repository's refs do, `HEAD`
/// included; `None` when there is nothing to stamp.
///
/// The mtimes of the `tables.list` of the worktree's own stack (where a
/// linked worktree keeps `HEAD`) and of the common one, which git replaces
/// on every update: two `stat`s, so the tick can tell whether what the
/// worker asked git is still current.
#[must_use]
pub fn reftable_stamp(dirs: &Dirs) -> Option<String> {
    let nanos = |dir: &Path| {
        let at =
            std::fs::metadata(dir.join("reftable").join("tables.list")).ok()?.modified().ok()?;
        Some(at.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos())
    };
    match (nanos(&dirs.git_dir), nanos(&dirs.common_dir)) {
        (None, None) => None,
        (own, common) => Some(format!("{}.{}", own.unwrap_or(0), common.unwrap_or(0))),
    }
}

/// Characters of a branch name the worker records: git's own limit on a
/// ref name is the path length, and the entry has a cap to stay under.
const MAX_BRANCH_CHARS: usize = 4096;

/// `HEAD` asked of git, for a repository whose refs are not files
/// (reftable): `symbolic-ref -q --short HEAD` for a branch, and when that
/// says it is detached, `rev-parse --verify HEAD` for the commit.
///
/// # Errors
/// Propagates git failures (an unborn detached `HEAD` among them).
pub fn head_from_git(cwd: &Path, timeout: Duration) -> Result<Head, String> {
    let args = ["symbolic-ref", "-q", "--short", "HEAD"];
    let asked = git_call(git_program()?, cwd, &args, &[], timeout, Stdout::Read)?;
    let first_line = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes).lines().next().unwrap_or("").trim().to_owned()
    };
    match asked.status.code() {
        Some(0) => {
            let name: String = first_line(&asked.stdout).chars().take(MAX_BRANCH_CHARS).collect();
            if name.is_empty() {
                Err("git symbolic-ref printed no branch".to_owned())
            } else {
                Ok(Head::Branch(name))
            }
        }
        Some(1) => {
            let sha = run_git(cwd, &["rev-parse", "--verify", "HEAD"], timeout)?;
            let sha: String = first_line(sha.as_bytes()).chars().take(MAX_BRANCH_CHARS).collect();
            Ok(Head::Detached(sha))
        }
        _ => Err(git_failed(&args, asked)),
    }
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
/// would not follow (only `refs/…` and capitalised pseudo-refs).
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

/// Bytes of `.git/config` read. git writes the file and it grows with every
/// tracked branch and submodule, so the cap is generous where a ref's is
/// tight; a cut or a stray byte that is not UTF-8 loses what it touches,
/// never the whole file.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// The upstream of a branch: `(remote, remote-tracking ref)` such as
/// `("origin", "refs/remotes/origin/main")`.
///
/// The tracking ref assumes the remote's default fetch refspec
/// (`refs/heads/*:refs/remotes/<remote>/*`), as `git clone` writes it.
#[must_use]
pub fn upstream(dirs: &Dirs, branch: &str) -> Option<(String, String)> {
    // Through the same contained, bounded reader as every ref: `config` sits
    // under the git directory and is as symlinkable as `HEAD` is.
    let bytes = read_ref_bytes(&dirs.common_dir, "config", MAX_CONFIG_BYTES)?;
    upstream_in(&String::from_utf8_lossy(&bytes), branch)
}

/// [`upstream`] over the text of a config file: `branch.<name>.remote`
/// (the last one, as git keeps it) and `branch.<name>.merge` (the first,
/// which is what `@{upstream}` follows).
fn upstream_in(text: &str, branch: &str) -> Option<(String, String)> {
    let mut remote: Option<String> = None;
    let mut merge: Option<String> = None;
    let wanted = |section: &str, sub: Option<&str>| {
        section.eq_ignore_ascii_case("branch") && sub == Some(branch)
    };
    for entry in config_entries(text, wanted) {
        match (entry.key.to_ascii_lowercase().as_str(), entry.value) {
            ("remote", Some(v)) => remote = Some(v),
            ("merge", Some(v)) if merge.is_none() => merge = Some(v),
            _ => {}
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

/// One `key = value` of a git config file.
struct ConfigEntry {
    /// The key, as written; git compares it without case.
    key: String,
    /// The value, unquoted and unescaped; `None` for a bare key.
    value: Option<String>,
}

/// The entries of a git config text in the sections `wanted` accepts
/// (given the section name as written and the subsection), in order,
/// parsed as git parses them (config.c `get_base_var`, `get_value`,
/// `parse_value`): a value may be quoted anywhere in it, knows the escapes
/// `\"`, `\\`, `\t`, `\n`, `\b` and a backslash-newline continuation, ends
/// at a `;` or `#` outside quotes, and loses its leading and trailing
/// blanks. git refuses the whole file over one malformed line; here that
/// line alone is skipped.
fn config_entries(text: &str, wanted: impl Fn(&str, Option<&str>) -> bool) -> Vec<ConfigEntry> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    let mut in_wanted = false;
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '#' | ';' => skip_line(&mut chars),
            '[' => {
                if let Some((section, sub)) = config_header(&mut chars) {
                    in_wanted = wanted(&section, sub.as_deref());
                } else {
                    in_wanted = false;
                    skip_line(&mut chars);
                }
            }
            c if c.is_ascii_alphabetic() => {
                let mut key = String::from(c);
                while let Some(k) = chars.next_if(|k| k.is_ascii_alphanumeric() || *k == '-') {
                    key.push(k);
                }
                while chars.next_if(|b| matches!(b, ' ' | '\t' | '\r')).is_some() {}
                let value = match chars.peek() {
                    None | Some('\n') => Some(None),
                    Some('=') => {
                        chars.next();
                        config_value(&mut chars).map(Some)
                    }
                    Some(_) => {
                        skip_line(&mut chars);
                        None
                    }
                };
                if let Some(value) = value.filter(|_| in_wanted) {
                    out.push(ConfigEntry { key, value });
                }
            }
            _ => skip_line(&mut chars),
        }
    }
    out
}

/// Consume through the end of the line.
fn skip_line(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for c in chars.by_ref() {
        if c == '\n' {
            break;
        }
    }
}

/// A section header after its `[`: `(section, subsection)`, with the old
/// `[section.sub]` form's subsection lowercased as git does; `None` when
/// malformed.
fn config_header(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Option<(String, Option<String>)> {
    let mut name = String::new();
    loop {
        match chars.next()? {
            ']' => {
                return Some(match name.split_once('.') {
                    Some((s, sub)) => (s.to_owned(), Some(sub.to_ascii_lowercase())),
                    None => (name, None),
                });
            }
            c if c.is_whitespace() => break,
            c if c.is_ascii_alphanumeric() || c == '-' || c == '.' => name.push(c),
            _ => return None,
        }
    }
    while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
    if chars.next()? != '"' {
        return None;
    }
    let mut sub = String::new();
    loop {
        match chars.next()? {
            '"' => break,
            '\n' => return None,
            '\\' => match chars.next()? {
                '\n' => return None,
                c => sub.push(c),
            },
            c => sub.push(c),
        }
    }
    (chars.next()? == ']').then_some((name, Some(sub)))
}

/// A value after its `=`, through the end of its (possibly continued)
/// line; `None` when malformed (an unknown escape, an open quote).
fn config_value(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut value = String::new();
    let mut quoted = false;
    let mut comment = false;
    // The length before a run of unquoted blanks, dropped if the run ends
    // the value.
    let mut trim_to: Option<usize> = None;
    loop {
        let c = match chars.next() {
            None | Some('\n') => {
                if quoted {
                    return None;
                }
                if let Some(len) = trim_to {
                    value.truncate(len);
                }
                return Some(value);
            }
            Some(c) => c,
        };
        if comment {
            continue;
        }
        if c.is_whitespace() && !quoted {
            if trim_to.is_none() {
                trim_to = Some(value.len());
            }
            if !value.is_empty() {
                value.push(c);
            }
            continue;
        }
        if !quoted && (c == ';' || c == '#') {
            comment = true;
            continue;
        }
        trim_to = None;
        match c {
            '\\' => match chars.next()? {
                '\n' => {}
                't' => value.push('\t'),
                'b' => value.push('\u{8}'),
                'n' => value.push('\n'),
                e @ ('\\' | '"') => value.push(e),
                _ => return None,
            },
            '"' => quoted = !quoted,
            c => value.push(c),
        }
    }
}

/// What `FETCH_HEAD` says about fetching, in epoch seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FetchStamps {
    /// When a fetch was last *tried*: git truncates `FETCH_HEAD` before it
    /// contacts the remote, so a fetch that fails still moves this.
    pub attempt: Option<i64>,
    /// When a fetch last *worked*: a successful fetch writes a line per
    /// ref, so only a `FETCH_HEAD` with something in it counts.
    pub success: Option<i64>,
}

/// [`FetchStamps`] from the `FETCH_HEAD` of the worktree's git dir and of
/// the common dir, the newest of each (a `stat` apiece, never a read).
///
/// `FETCH_HEAD` is written per worktree but the remote-tracking refs the
/// counts use are shared, so a fetch from any worktree freshens them; the
/// first file found used to win, and a linked worktree with an old
/// `FETCH_HEAD` of its own went on reporting that age.
#[must_use]
pub fn fetch_stamps(dirs: &Dirs) -> FetchStamps {
    let stamps: Vec<(i64, bool)> = [&dirs.git_dir, &dirs.common_dir]
        .into_iter()
        .filter_map(|d| {
            let meta = std::fs::metadata(d.join("FETCH_HEAD")).ok()?;
            let at = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
            Some((i64::try_from(at.as_secs()).ok()?, meta.len() > 0))
        })
        .collect();
    FetchStamps {
        attempt: stamps.iter().map(|(at, _)| *at).max(),
        success: stamps.iter().filter(|(_, ok)| *ok).map(|(at, _)| *at).max(),
    }
}

/// Seconds from `at` to `now` (both epoch seconds), or `None` when `at`
/// is ahead of `now`.
///
/// A stamp from the future is a clock that moved (a resumed VM, NTP
/// correcting a bad RTC), never a very recent event, and reading it as
/// age 0 froze auto-fetch until the wall clock caught up.
#[must_use]
pub fn age(at: i64, now: i64) -> Option<u64> {
    u64::try_from(now.checked_sub(at)?).ok()
}

/// Whether the full ref `refname` exists: a file read, or in a reftable
/// repository (whose refs are not files) `git show-ref --verify`, which
/// takes a ref name and nothing else (no revision syntax).
///
/// # Errors
/// Propagates git failures (reftable only).
pub fn ref_exists(dirs: &Dirs, refname: &str, timeout: Duration) -> Result<bool, String> {
    if !dirs.uses_reftable() {
        return Ok(resolve_ref(dirs, refname).is_some());
    }
    if !refname.starts_with("refs/") {
        return Ok(false);
    }
    let args = ["show-ref", "--verify", "--quiet", refname];
    match git_answer(&dirs.toplevel, &args, timeout)? {
        Answer::No => Ok(true),
        Answer::Yes => Ok(false),
        Answer::Failed(e) => Err(e),
    }
}

/// Config keys cleared on every git call because git would run their value
/// as a command, and the repository's `.git/config` is not the user's file.
///
/// Only `core.fsmonitor` is cleared here: a command `git status` and
/// `diff-files` would start on their own, and clearing it costs nothing
/// (the monitor is a speed hint and git falls back to walking the tree,
/// which is what the 2 s timeout is for). `-c` on the command line beats
/// the file. This is *not* every command a config can name:
///
/// - `filter.<driver>.clean`/`process` run whenever git hashes a worktree
///   file; [`is_dirty`] is built so git never does (plumbing, and the stat
///   rule pinned), rather than by clearing drivers one name at a time;
/// - `core.hooksPath` and `.git/hooks`, `credential.helper`,
///   `core.askPass`, `core.alternateRefsCommand`, `core.sshCommand`,
///   `core.gitProxy` and an `ext::` URL are reached only by [`fetch`],
///   which the user opts into (`fetch_interval`); [`fetch`] turns off the
///   maintenance and submodule recursion it would start, and PLAN's
///   backlog carries the decision on the rest;
/// - lazy fetching in a partial clone is off through `GIT_NO_LAZY_FETCH`
///   (git 2.44 and later; an older git ignores it).
///
/// The user typing `git status` in that checkout would run all of these
/// too; what is new is that garnish runs git on a *timer*, unasked.
const NO_COMMAND_HOOKS: [&str; 2] = ["-c", "core.fsmonitor="];

/// Variables that point git at a repository, index or object store other
/// than the one it would find from its working directory. A worker started
/// by a harness that exported one (a hook, an alias) would otherwise count
/// another repository's changes next to this one's branch; git discovers
/// the repository from `cwd` exactly as the tick did. `GIT_DIR` is never
/// *set* either: git's `safe.directory` ownership check applies to
/// discovery, not to an explicit `GIT_DIR`.
const DISCOVERY_ENV: [&str; 10] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
];

/// The first executable regular file called `name` in an *absolute* entry
/// of `path` (a `PATH` value).
///
/// std resolves a bare program name in the child after `current_dir`, so an
/// empty or relative entry (`:/usr/bin`, `.`) would run `<repository>/git`,
/// a file the checkout ships, on a timer; Go's `exec.LookPath` refuses the
/// same result (`ErrDot`). Such entries are skipped.
fn find_program(name: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    std::env::split_paths(path?).filter(|dir| dir.is_absolute()).map(|dir| dir.join(name)).find(
        |p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0),
    )
}

/// `git`, looked up once per process by [`find_program`].
fn git_program() -> Result<&'static Path, String> {
    static GIT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    GIT.get_or_init(|| find_program("git", std::env::var_os("PATH").as_deref()))
        .as_deref()
        .ok_or_else(|| "git: not found on PATH".to_owned())
}

/// A program that fails, for `SSH_ASKPASS`: `false` looked up like `git`,
/// or a path that does not exist, which fails as surely.
fn failing_program() -> &'static Path {
    static FALSE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FALSE.get_or_init(|| {
        find_program("false", std::env::var_os("PATH").as_deref())
            .unwrap_or_else(|| PathBuf::from("/bin/false"))
    })
}

/// Whether the caller reads what the child wrote to stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stdout {
    /// The output is the answer, so failing to read it is a failure.
    Read,
    /// Only the exit status matters: a stdout read that has to be abandoned
    /// is not a failure. `fetch` runs `--quiet` and is precisely the call
    /// whose pipes an ssh `ControlPersist` master holds open, so treating
    /// that as an error recorded a fetch that worked as one that did not.
    Discard,
}

/// What a program that ran to its end left behind.
#[derive(Debug)]
pub struct Finished {
    /// How it exited.
    pub status: std::process::ExitStatus,
    /// Its stdout, at most [`MAX_STDOUT`] bytes (empty when discarded).
    pub stdout: Vec<u8>,
    /// Whether stdout went on past [`MAX_STDOUT`].
    pub truncated: bool,
    /// Its stderr, trimmed, at most [`MAX_STDERR`] bytes of it.
    pub stderr: String,
}

/// Why a program did not run to its end.
#[derive(Debug)]
pub enum Failure {
    /// It could not be started.
    Start(std::io::Error),
    /// Waiting on it failed.
    Wait(std::io::Error),
    /// It was killed at the timeout.
    TimedOut(Duration),
    /// It exited, but its stdout could not be read before the deadline.
    Unread,
}

impl Failure {
    /// The failure as a message about `command` (`git rev-list …`), which
    /// the caller names: the arguments it asked for, not the ones added on
    /// its behalf.
    #[must_use]
    pub fn describe(&self, command: &str) -> String {
        match self {
            Self::Start(e) | Self::Wait(e) => format!("{command}: {e}"),
            Self::TimedOut(t) => format!("{command} timed out after {} ms", t.as_millis()),
            Self::Unread => format!("{command} wrote no output before the timeout"),
        }
    }
}

/// Bytes of a command's stdout kept: git's answers here are a line or two.
pub const MAX_STDOUT: u64 = 1024 * 1024;

/// Bytes of a command's stderr kept: it only decorates a failure, and a
/// failed entry keeps 500 characters of it.
pub const MAX_STDERR: u64 = 64 * 1024;

/// How long the pipes are still read after the child has exited and the
/// timeout is already spent. The child's own ends close with it, so this
/// bounds one case only: a descendant still holding them (see [`drain`]).
const DRAIN_FLOOR: Duration = Duration::from_millis(250);

/// Read at most `cap` bytes of a pipe on its own thread, then discard the
/// rest so the child can finish, delivering the bytes and whether any were
/// discarded once.
///
/// A channel rather than a join handle, so the caller can put a deadline on
/// the read. Joining has none: the write end stays open while *any*
/// descendant holds it, not only the child — ssh's `ControlPersist` master
/// outlives the `git fetch` that started it — and the worker would then sit
/// in the read for ever with its lock held, whatever timeout was asked
/// for. A thread left behind does not hold the process up; it is dropped
/// when the worker exits. Stopping the read at the cap instead would leave
/// the child blocked on a full pipe and turn a big but quick answer into a
/// timeout.
fn drain<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
    cap: u64,
) -> std::sync::mpsc::Receiver<(Vec<u8>, bool)> {
    use std::io::Read as _;
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(mut pipe) = pipe {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.by_ref().take(cap).read_to_end(&mut buf);
            let truncated = std::io::copy(&mut pipe, &mut std::io::sink()).is_ok_and(|n| n > 0);
            let _ = tx.send((buf, truncated));
        });
    }
    rx
}

/// Run `program` with `args` and the extra `env` in `cwd`, killing it after
/// `timeout`.
///
/// The one way garnish runs an external command (every git call, tests'
/// fake gits). Its stdin is null and both pipes are drained with a cap,
/// git's environment is cut down to the repository `cwd` names (no
/// `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` or kin from the harness),
/// and git is told never to prompt, lock optionally, fetch lazily or
/// translate.
///
/// # Errors
/// When the program cannot be started or waited on, times out, or (with
/// [`Stdout::Read`]) exits without its stdout arriving in time. A non-zero
/// exit is not an error here; the caller reads [`Finished::status`].
pub fn run_program(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    env: &[(&str, &std::ffi::OsStr)],
    timeout: Duration,
    want: Stdout,
) -> Result<Finished, Failure> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in DISCOVERY_ENV {
        cmd.env_remove(key);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().map_err(Failure::Start)?;
    // Drain both pipes on their own threads: a child that writes more than
    // the pipe buffer (64 KiB) before exiting would otherwise block forever.
    let stdout = drain(child.stdout.take(), MAX_STDOUT);
    let stderr = drain(child.stderr.take(), MAX_STDERR);
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::TimedOut(timeout));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(Failure::Wait(e)),
        }
    };
    // What is left of the budget, never below a floor: the child has exited,
    // so its pipes are normally closed already and both arrive at once.
    let left = || timeout.saturating_sub(start.elapsed()).max(DRAIN_FLOOR);
    // A read that gave up is an error, never an empty answer, for a caller
    // that reads it: an empty answer can read as "nothing to report", which
    // would put a fabricated value in the cache for a whole TTL instead of
    // a `✗`. For a caller that discards it, nothing was lost.
    let (stdout, truncated) = match (stdout.recv_timeout(left()), want) {
        (Ok(out), _) => out,
        (Err(_), Stdout::Discard) => (Vec::new(), false),
        (Err(_), Stdout::Read) => return Err(Failure::Unread),
    };
    // stderr only decorates a failure, so a lost one costs the message, not
    // the answer.
    let (err, _) = stderr.recv_timeout(left()).unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err).trim().to_owned();
    Ok(Finished { status, stdout, truncated, stderr })
}

/// `git <args>` through [`run_program`] with the command hooks cleared,
/// its failures described in terms of the caller's own `args`.
fn git_call(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    env: &[(&str, &std::ffi::OsStr)],
    timeout: Duration,
    want: Stdout,
) -> Result<Finished, String> {
    let full: Vec<&str> = NO_COMMAND_HOOKS.iter().chain(args).copied().collect();
    run_program(program, cwd, &full, env, timeout, want)
        .map_err(|f| f.describe(&format!("git {}", args.join(" "))))
}

/// A finished git call that exited non-zero, as the message to record:
/// git's own words when it wrote any.
fn git_failed(args: &[&str], finished: Finished) -> String {
    if finished.stderr.is_empty() {
        format!("git {} failed", args.join(" "))
    } else {
        finished.stderr
    }
}

/// Run `git` with arguments in `cwd`, killing it after `timeout`, and
/// return its stdout.
///
/// # Errors
/// git's stderr (or a message naming the command) when it cannot be run,
/// times out, exits non-zero, or writes more than [`MAX_STDOUT`].
pub fn run_git(cwd: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let finished = git_call(git_program()?, cwd, args, &[], timeout, Stdout::Read)?;
    if !finished.status.success() {
        return Err(git_failed(args, finished));
    }
    if finished.truncated {
        return Err(format!("git {} wrote more than {MAX_STDOUT} bytes", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&finished.stdout).into_owned())
}

/// The exit code of a git command asked a yes-or-no question (`--quiet`
/// plumbing: 0 = no difference, 1 = difference), with the message to
/// record for any other exit.
fn git_answer(cwd: &Path, args: &[&str], timeout: Duration) -> Result<Answer, String> {
    let finished = git_call(git_program()?, cwd, args, &[], timeout, Stdout::Discard)?;
    Ok(match finished.status.code() {
        Some(0) => Answer::No,
        Some(1) => Answer::Yes,
        _ => Answer::Failed(git_failed(args, finished)),
    })
}

/// What [`git_answer`] heard.
enum Answer {
    /// Exit 0.
    No,
    /// Exit 1.
    Yes,
    /// Anything else, with the message.
    Failed(String),
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

/// Whether the working tree has staged or unstaged changes (untracked files
/// ignored), asked of plumbing that never reads a worktree file's content.
///
/// `git status` re-hashes every file whose stat data no longer matches the
/// index, through the `clean`/`process` filter driver `.gitattributes`
/// names and `.git/config` defines, so in a checkout the user did not
/// build (an unpacked archive, whose stat data never matches) it ran the
/// repository's command on every refresh (review 2026-09-25). Instead:
///
/// - `diff-index --cached --quiet HEAD` for staged changes (index against
///   the commit; no worktree at all). With no commit yet, anything in the
///   index is staged;
/// - `diff-files --quiet` for unstaged ones, which compares stat data and
///   stops at the first difference without reading content, with
///   `core.checkStat=default` pinned so a repository cannot relax the
///   comparison (`minimal`) until an archive's files match their index by
///   mtime and size and git hashes them as "racily clean", and with
///   `--ignore-submodules=dirty`, since a submodule's own dirtiness is a
///   `git status` run inside it.
///
/// The trade-off, accepted: a file whose stat data changed and content did
/// not (touched, rewritten with the same bytes) reads as dirty until the
/// user's own git refreshes the index.
///
/// # Errors
/// Propagates git failures.
pub fn is_dirty(cwd: &Path, timeout: Duration) -> Result<bool, String> {
    let staged =
        match git_answer(cwd, &["diff-index", "--cached", "--quiet", "HEAD", "--"], timeout)? {
            Answer::No => false,
            Answer::Yes => true,
            Answer::Failed(e) => {
                match git_answer(cwd, &["rev-parse", "-q", "--verify", "HEAD"], timeout)? {
                    // HEAD names no commit yet: everything in the index is staged.
                    Answer::Yes => {
                        let args = ["ls-files", "--cached", "-z"];
                        let listed =
                            git_call(git_program()?, cwd, &args, &[], timeout, Stdout::Read)?;
                        if !listed.status.success() {
                            return Err(git_failed(&args, listed));
                        }
                        listed.truncated || !listed.stdout.is_empty()
                    }
                    _ => return Err(e),
                }
            }
        };
    if staged {
        return Ok(true);
    }
    let unstaged =
        ["-c", "core.checkStat=default", "diff-files", "--quiet", "--ignore-submodules=dirty"];
    match git_answer(cwd, &unstaged, timeout)? {
        Answer::No => Ok(false),
        Answer::Yes => Ok(true),
        Answer::Failed(e) => Err(e),
    }
}

/// The arguments of [`fetch`] for `remote` (a plain name, checked there).
///
/// `--no-auto-maintenance` and `--recurse-submodules=no` because neither
/// is anything a status line needs: the first would start `gc --auto` (and
/// its hook) detached, outliving the timeout, and the second walks into
/// submodules with configs of their own.
const fn fetch_args(remote: &str) -> [&str; 8] {
    [
        "fetch",
        "--quiet",
        "--no-auto-maintenance",
        "--recurse-submodules=no",
        "--upload-pack",
        "git-upload-pack",
        "--",
        remote,
    ]
}

/// `git fetch --quiet <remote>`, killed after `timeout` (a hung network
/// fetch must not pin the worker and its lock).
///
/// The remote is the one named in the repository's own `.git/config`, which
/// is not the user's file in a checkout they did not create. Several things
/// follow from that:
///
/// - a name starting with `-` would be read by git as an option rather than
///   a remote, and `--upload-pack=<cmd>` runs `<cmd>`, so the name is
///   refused and passed after `--`;
/// - the same file can set `remote.<name>.uploadpack`, which needs no
///   suspicious name at all. `--upload-pack` on the command line beats it.
///   Overriding it loses nothing but a per-remote server path, which is rare
///   where a hostile checkout getting a command run is not;
/// - the worker keeps Claude Code's controlling terminal, and ssh opens
///   `/dev/tty` for a host key or a passphrase whatever
///   `GIT_TERMINAL_PROMPT` says, so its prompt would be drawn into the
///   harness's screen: `SSH_ASKPASS_REQUIRE=force` with an `SSH_ASKPASS`
///   that fails makes ssh ask a program instead, and fail (OpenSSH 8.4 and
///   later; an older ssh ignores both).
///
/// `core.sshCommand`, `core.gitProxy`, an `ext::` URL, hooks and credential
/// helpers remain: `fetch_interval` defaults to 0, so nothing reaches them
/// until the user opts in, and PLAN's backlog carries that decision.
///
/// # Errors
/// Propagates git failures; refuses a remote that is not a plain name.
pub fn fetch(cwd: &Path, remote: &str, timeout: Duration) -> Result<(), String> {
    if remote.is_empty() || remote.starts_with('-') {
        return Err(format!("refusing to fetch from remote {remote:?}"));
    }
    let args = fetch_args(remote);
    let env = [
        ("SSH_ASKPASS_REQUIRE", std::ffi::OsStr::new("force")),
        ("SSH_ASKPASS", failing_program().as_os_str()),
    ];
    let finished = git_call(git_program()?, cwd, &args, &env, timeout, Stdout::Discard)?;
    if finished.status.success() { Ok(()) } else { Err(git_failed(&args, finished)) }
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
        let stamps = fetch_stamps(&discover(&work).unwrap());
        assert!(stamps.success.is_some() && stamps.attempt == stamps.success, "{stamps:?}");
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

    /// git writes a value holding `#` or `;` in double quotes, so `git push
    /// -u origin fix/#12` stores `merge = "refs/heads/fix/#12"`; read raw,
    /// the quotes made the tracking ref `refs/remotes/origin/"refs/…"` and
    /// `sync` showed `✗` for good. A config past the old 64 KiB cap, or with
    /// one byte that is not UTF-8, lost every upstream.
    #[test]
    fn upstream_reads_the_config_the_way_git_writes_it() {
        let (_d, work) = repo();
        git(&work, &["checkout", "-q", "-b", "fix/#12"]);
        git(&work, &["push", "-q", "-u", "origin", "fix/#12"]);
        let dirs = discover(&work).unwrap();
        let config = std::fs::read_to_string(work.join(".git/config")).unwrap();
        assert!(config.contains("\"refs/heads/fix/#12\""), "git quotes it: {config}");
        assert_eq!(
            upstream(&dirs, "fix/#12"),
            Some(("origin".into(), "refs/remotes/origin/fix/#12".into()))
        );
        let main = Some(("origin".to_owned(), "refs/remotes/origin/main".to_owned()));
        // Padded past 64 KiB with a comment of two-byte characters, ahead of
        // every section, so the old cut fell inside one.
        let pad = format!("# {}\n", "é".repeat(40 * 1024));
        std::fs::write(work.join(".git/config"), format!("{pad}{config}")).unwrap();
        assert_eq!(upstream(&dirs, "main"), main, "a long config");
        // One Latin-1 byte in a comment.
        let mut bytes = b"# caf\xe9\n".to_vec();
        bytes.extend_from_slice(config.as_bytes());
        std::fs::write(work.join(".git/config"), bytes).unwrap();
        assert_eq!(upstream(&dirs, "main"), main, "a byte that is not UTF-8");
    }

    /// git's value syntax (config.c `parse_value`, `get_base_var`): quotes
    /// anywhere, the four escapes, `;`/`#` comments outside quotes, trailing
    /// blanks trimmed, section and key names in any case, an escaped
    /// subsection, the old `[branch.name]` form, the first `merge` and the
    /// last `remote`.
    #[test]
    fn the_config_parser_follows_git() {
        let up = |text: &str, branch: &str| upstream_in(text, branch);
        let origin = |r: &str| Some(("origin".to_owned(), format!("refs/remotes/origin/{r}")));
        assert_eq!(
            up("[branch \"a\"]\n\tremote = origin\n\tmerge = refs/heads/a ; why\n", "a"),
            origin("a")
        );
        assert_eq!(
            up("[Branch \"a\"]\n  Remote=origin\n  MERGE = \"refs/heads/a#1\"  # c\n", "a"),
            origin("a#1")
        );
        assert_eq!(
            up("[branch \"q\\\"t\"]\nremote = origin\nmerge = refs/heads/q\\\"t\n", "q\"t"),
            origin("q\"t")
        );
        assert_eq!(
            up("[branch.main]\nremote = origin\nmerge = refs/heads/main\n", "main"),
            origin("main")
        );
        assert_eq!(up("[branch.Main]\nremote = origin\nmerge = refs/heads/x\n", "Main"), None);
        assert_eq!(
            up(
                "[branch \"a\"]\nremote = up\nremote = origin\nmerge = refs/heads/one\nmerge = refs/heads/two\n",
                "a"
            ),
            origin("one")
        );
        assert_eq!(up("[branch \"a\"] remote = origin\nmerge = refs/heads/a\n", "a"), origin("a"));
        assert_eq!(
            up("[branch \"a\"]\nremote = origin\nmerge = refs/heads/\\\na\n", "a"),
            origin("a"),
            "a continued line"
        );
        assert_eq!(up("[branch \"a\"]\nremote = origin\nmerge = \"refs/heads/a\n", "a"), None);
        assert_eq!(up("[branch \"b\"]\nremote = origin\nmerge = refs/heads/b\n", "a"), None);
        assert_eq!(
            up("[branch \"a\"]\nremote = .\nmerge = refs/heads/main\n", "a"),
            Some((".".to_owned(), "refs/heads/main".to_owned()))
        );
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
        let half = Duration::from_millis(500);
        let started = std::time::Instant::now();
        let out = fake_git(&leaky, &work, &["status"], half, Stdout::Read);
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
        let err = out.expect_err("a drained-out read must not pass as an empty answer");
        assert!(err.contains("wrote no output before the timeout"), "{err}");

        // The same output without the grandchild arrives in full: the
        // deadline is on the pipes, not on every call.
        let clean = write("git-clean", "#!/bin/sh\nprintf 'M file\\n'\nexit 0\n");
        let out = fake_git(&clean, &work, &["status"], half, Stdout::Read);
        assert_eq!(out.as_deref(), Ok("M file\n"));

        // A caller that discards stdout loses nothing when the read is
        // abandoned, so the same grandchild must not turn a command that
        // *worked* into a failure. `fetch` is that caller, runs `--quiet`,
        // and is the very call an ssh `ControlPersist` master outlives.
        let started = std::time::Instant::now();
        let out = fake_git(&leaky, &work, &["fetch"], half, Stdout::Discard);
        assert_eq!(out.as_deref(), Ok(""), "a discarded read is not a failure");
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
        // It still fails when the command itself does, in git's own words
        // (nothing holds the pipes here, so the stderr does arrive).
        let bad = write("git-bad", "#!/bin/sh\necho boom >&2\nexit 1\n");
        let out = fake_git(&bad, &work, &["fetch"], half, Stdout::Discard);
        assert_eq!(out.as_deref().map_err(String::as_str), Err("boom"));
    }

    /// A fake git run the way [`run_git`] runs the real one: its stdout on
    /// success, git's words (or the described failure) otherwise.
    fn fake_git(
        program: &Path,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
        want: Stdout,
    ) -> Result<String, String> {
        let finished = git_call(program, cwd, args, &[], timeout, want)?;
        if finished.status.success() {
            Ok(String::from_utf8_lossy(&finished.stdout).into_owned())
        } else {
            Err(git_failed(args, finished))
        }
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

    /// `FETCH_HEAD` is per worktree but the tracking refs are shared, so
    /// the newest of the two files counts: a linked worktree with an old
    /// `FETCH_HEAD` of its own used to report that age after the main
    /// worktree had fetched.
    #[test]
    fn fetch_stamps_take_the_newest_fetch_of_any_worktree() {
        let (d, work) = repo();
        let wt = d.path().join("wt");
        git(&work, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
        git(&wt, &["fetch", "-q", "origin"]);
        let linked = discover(&wt).unwrap();
        let own = linked.git_dir.join("FETCH_HEAD");
        assert!(own.exists());
        // The clock is read at each check, after the fetch it measures: a
        // `now` taken before the second fetch put that fetch in the future
        // whenever a second boundary fell between them, and a future stamp
        // has no age by design.
        let age_of = |s: Option<i64>| s.and_then(|t| age(t, crate::time::now_secs()));
        assert!(age_of(fetch_stamps(&linked).success).is_some_and(|a| a < 60));
        let three_days = std::time::SystemTime::now() - Duration::from_hours(72);
        std::fs::File::options().write(true).open(&own).unwrap().set_modified(three_days).unwrap();
        assert!(age_of(fetch_stamps(&linked).success).is_some_and(|a| a > 86_400));
        git(&work, &["fetch", "-q", "origin"]);
        assert!(age_of(fetch_stamps(&linked).success).is_some_and(|a| a < 60), "the main one's");
    }

    /// git truncates `FETCH_HEAD` before it contacts the remote, so a fetch
    /// that fails moves the attempt clock and leaves the file empty: it is
    /// no successful fetch, and the hint must not read it as one.
    #[test]
    fn a_failed_fetch_moves_the_attempt_clock_only() {
        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        git(&work, &["fetch", "-q", "origin"]);
        let old = std::time::SystemTime::now() - Duration::from_hours(24);
        let head = work.join(".git/FETCH_HEAD");
        std::fs::File::options().write(true).open(&head).unwrap().set_modified(old).unwrap();
        let before = fetch_stamps(&dirs);
        git(&work, &["remote", "set-url", "origin", "/nonexistent/origin.git"]);
        assert!(fetch(&work, "origin", Duration::from_secs(5)).is_err());
        assert_eq!(std::fs::metadata(&head).unwrap().len(), 0, "git truncates it first");
        let after = fetch_stamps(&dirs);
        assert!(after.attempt > before.attempt, "{before:?} → {after:?}");
        assert_eq!(after.success, None, "an empty FETCH_HEAD is no success");
    }

    /// A stamp from the future is a clock that moved, not age 0: read as
    /// 0 it kept auto-fetch "not due" until the wall clock caught up.
    #[test]
    fn a_future_fetch_head_has_no_age() {
        assert_eq!(age(100, 160), Some(60));
        assert_eq!(age(100, 100), Some(0));
        assert_eq!(age(160, 100), None);
        let (_d, work) = repo();
        git(&work, &["fetch", "-q", "origin"]);
        let ahead = std::time::SystemTime::now() + Duration::from_secs(3_600);
        let head = work.join(".git/FETCH_HEAD");
        std::fs::File::options().write(true).open(&head).unwrap().set_modified(ahead).unwrap();
        let stamps = fetch_stamps(&discover(&work).unwrap());
        assert_eq!(stamps.attempt.and_then(|t| age(t, crate::time::now_secs())), None);
    }

    /// Output past the pipe buffer never deadlocks the worker, and output
    /// past [`MAX_STDOUT`] is not held in memory: the rest is read and
    /// dropped so the child finishes, and the cut is reported.
    #[test]
    fn large_output_does_not_deadlock_the_worker() {
        let dir = tempfile::tempdir().unwrap();
        let script = |name: &str, body: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, body).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        let five = Duration::from_secs(5);
        let fake = script("git", "#!/bin/sh\nhead -c 300000 /dev/zero | tr '\\0' 'x'\nexit 0\n");
        let started = std::time::Instant::now();
        let out = fake_git(&fake, dir.path(), &["status"], five, Stdout::Read).unwrap();
        assert_eq!(out.len(), 300_000);
        assert!(started.elapsed() < Duration::from_secs(4));
        let noisy = script("noisy", "#!/bin/sh\nhead -c 200000 /dev/zero >&2\nexit 3\n");
        let err = fake_git(&noisy, dir.path(), &["x"], five, Stdout::Read).unwrap_err();
        assert!(u64::try_from(err.len()).is_ok_and(|n| n <= MAX_STDERR), "{}", err.len());

        let huge = script("huge", "#!/bin/sh\nhead -c 5000000 /dev/zero | tr '\\0' 'x'\nexit 0\n");
        let started = std::time::Instant::now();
        let run = run_program(&huge, dir.path(), &[], &[], five, Stdout::Read).unwrap();
        assert!(started.elapsed() < Duration::from_secs(4), "took {:?}", started.elapsed());
        assert!(run.status.success() && run.truncated);
        assert_eq!(u64::try_from(run.stdout.len()).unwrap(), MAX_STDOUT);
    }

    /// The timeout kills the child, and the message names the command the
    /// caller asked for, in milliseconds: not the `-c core.fsmonitor=` put
    /// in front of it (whose `=` and path made `doctor` lines unreadable),
    /// and not `after 0s` for a sub-second limit.
    #[test]
    fn timeout_kills_slow_git() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("git");
        std::fs::write(&fake, "#!/bin/sh\nsleep 5\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = std::time::Instant::now();
        let r = fake_git(&fake, dir.path(), &["status"], Duration::from_millis(200), Stdout::Read);
        assert_eq!(r, Err("git status timed out after 200 ms".to_owned()));
        assert!(started.elapsed() < Duration::from_secs(3));
        let missing = dir.path().join("nope");
        let r = run_program(&missing, dir.path(), &[], &[], Duration::from_secs(1), Stdout::Read);
        let err = r.map(|_| ()).unwrap_err().describe("nope --version");
        assert!(err.starts_with("nope --version: "), "{err}");
    }

    /// std looks a bare program name up after the child's `chdir`, so an
    /// empty or relative `PATH` entry would find a `git` the checkout ships.
    #[test]
    fn programs_are_looked_up_on_absolute_path_entries_only() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(bin.join("tool.d")).unwrap();
        std::fs::write(bin.join("tool"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(bin.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(bin.join("plain"), "").unwrap();
        let find = |path: String| find_program("tool", Some(std::ffi::OsStr::new(&path)));
        let b = bin.display();
        for path in [format!(":{b}"), format!(".:{b}"), format!("relative:{b}"), format!("{b}:")] {
            assert_eq!(find(path.clone()), Some(bin.join("tool")), "{path}");
        }
        assert_eq!(find(":.:relative".to_owned()), None);
        assert_eq!(find_program("plain", Some(bin.as_os_str())), None, "not executable");
        assert_eq!(find_program("tool.d", Some(bin.as_os_str())), None, "a directory");
        assert_eq!(find_program("tool", None), None);
    }

    /// The dirty check never hashes a worktree file, so a filter driver
    /// the repository's own config defines never runs: not on a clean
    /// tree, not on a file whose stat data changed (an unpacked archive's
    /// every file), and not on an entry an attacker made "racily clean"
    /// under a `core.checkStat = minimal` of the repository's choosing.
    #[test]
    fn the_dirty_check_never_runs_a_filter_driver() {
        let (d, work) = repo();
        let t = Duration::from_secs(5);
        let set_mtime = |p: &Path, t: std::time::SystemTime| {
            std::fs::File::options().write(true).open(p).unwrap().set_modified(t).unwrap();
        };
        let old = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        std::fs::write(work.join("b.txt"), "b\n").unwrap();
        git(&work, &["add", "b.txt"]);
        git(&work, &["commit", "-q", "-m", "b"]);
        // The index records an mtime well before its own, so nothing is racy.
        set_mtime(&work.join("a.txt"), old);
        set_mtime(&work.join("b.txt"), old);
        git(&work, &["update-index", "--refresh"]);
        let marker = d.path().join("marker");
        let config = work.join(".git/config");
        let text = std::fs::read_to_string(&config).unwrap();
        let m = marker.display();
        let drivers = format!(
            "[filter \"c\"]\n\tclean = \"sh -c 'touch {m}; cat'\"\n[filter \"p\"]\n\tprocess = \"sh -c 'touch {m}; exit 1'\"\n"
        );
        std::fs::write(&config, format!("{text}{drivers}")).unwrap();
        std::fs::write(work.join(".gitattributes"), "a.txt filter=c\nb.txt filter=p\n").unwrap();

        assert_eq!(is_dirty(&work, t), Ok(false), "a clean tree");
        assert!(!marker.exists(), "a clean tree ran the filter");
        // Stat data changed, content did not: dirty (the accepted
        // trade-off), and still no filter.
        set_mtime(&work.join("a.txt"), std::time::SystemTime::now());
        assert_eq!(is_dirty(&work, t), Ok(true));
        assert!(!marker.exists(), "a stat-dirty file ran the filter");

        // The crafted case: the repository relaxes the stat rule to mtime
        // and size, both files are replaced by same-content copies with
        // the same mtime (a new inode, as extraction gives), and the index
        // is older than its entries, so git would take them for racily
        // clean and hash them.
        git(&work, &["config", "core.checkStat", "minimal"]);
        git(&work, &["config", "core.trustctime", "false"]);
        for name in ["a.txt", "b.txt"] {
            let path = work.join(name);
            let body = std::fs::read(&path).unwrap();
            let copy = work.join(format!("{name}.copy"));
            std::fs::write(&copy, body).unwrap();
            set_mtime(&copy, old);
            std::fs::rename(&copy, &path).unwrap();
        }
        set_mtime(
            &work.join(".git/index"),
            std::time::UNIX_EPOCH + Duration::from_secs(1_000_000_000),
        );
        let _ = std::fs::remove_file(&marker);
        assert_eq!(is_dirty(&work, t), Ok(true));
        assert!(!marker.exists(), "a racily clean entry ran the filter");
        // Without the pinned stat rule, git really would have run it: the
        // case above is not vacuous.
        let _ = Command::new("git")
            .args(["diff-files", "--quiet"])
            .current_dir(&work)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(marker.exists(), "the relaxed rule hashes the file");
    }

    /// Since git 2.35, porcelain `status` prints `# stash <n>` when
    /// `status.showStash` is on, which the old check read as a change. The
    /// plumbing prints nothing, so a stash never reads as dirty.
    #[test]
    fn a_stash_is_not_a_change() {
        let (_d, work) = repo();
        let t = Duration::from_secs(5);
        git(&work, &["config", "status.showStash", "true"]);
        std::fs::write(work.join("a.txt"), "b\n").unwrap();
        git(&work, &["stash", "push", "-q"]);
        assert_eq!(is_dirty(&work, t), Ok(false));
        std::fs::write(work.join("a.txt"), "c\n").unwrap();
        assert_eq!(is_dirty(&work, t), Ok(true));
    }

    /// Before the first commit `HEAD` names nothing, so `diff-index HEAD`
    /// fails; anything in the index is then staged.
    #[test]
    fn an_unborn_head_is_dirty_only_with_something_staged() {
        let dir = tempfile::tempdir().unwrap();
        let t = Duration::from_secs(5);
        git(dir.path(), &["init", "-q"]);
        assert_eq!(is_dirty(dir.path(), t), Ok(false));
        std::fs::write(dir.path().join("x.txt"), "x\n").unwrap();
        assert_eq!(is_dirty(dir.path(), t), Ok(false), "untracked files do not count");
        git(dir.path(), &["add", "x.txt"]);
        assert_eq!(is_dirty(dir.path(), t), Ok(true));
        assert!(is_dirty(&dir.path().join("nowhere"), t).is_err());
    }

    /// `fetch` never starts maintenance or walks into submodules, and never
    /// lets ssh prompt on the terminal it inherited: ssh is told to ask a
    /// program instead, and the program fails.
    #[test]
    fn fetch_starts_no_maintenance_and_lets_ssh_prompt_nowhere() {
        let args = fetch_args("origin");
        assert!(
            args.contains(&"--no-auto-maintenance") && args.contains(&"--recurse-submodules=no")
        );
        assert_eq!(args.last(), Some(&"origin"));
        let (d, work) = repo();
        let seen = d.path().join("seen");
        let shim = d.path().join("ssh-shim");
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\nprintf '%s\\n%s\\n' \"$SSH_ASKPASS_REQUIRE\" \"$SSH_ASKPASS\" > '{}'\nexit 1\n",
                seen.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(&work, &["config", "core.sshCommand", shim.to_str().unwrap()]);
        git(&work, &["remote", "add", "far", "ssh://example.invalid/x.git"]);
        assert!(fetch(&work, "far", Duration::from_secs(5)).is_err());
        let seen = std::fs::read_to_string(&seen).unwrap();
        let mut lines = seen.lines();
        assert_eq!(lines.next(), Some("force"), "{seen}");
        let askpass = PathBuf::from(lines.next().unwrap());
        assert!(askpass.is_absolute(), "{}", askpass.display());
        let ran = Command::new(&askpass).output();
        assert!(ran.is_err() || ran.is_ok_and(|o| !o.status.success()), "the askpass must fail");
    }
}
