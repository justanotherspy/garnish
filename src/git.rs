//! Reading git state.
//!
//! Everything the *tick* needs (top level, HEAD, upstream, fetch age) is read
//! straight from the `.git` directory with a handful of small file reads, so
//! no process is spawned per second. Anything that needs the object database
//! (ahead/behind counts, dirty state, fetching) runs in the background worker
//! through the `git` binary.

use std::borrow::Cow;
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
/// placeholder (`ref: refs/heads/.invalid`) rather than the real head, and
/// for a name git cannot have written (`writable_name`).
#[must_use]
pub fn head(dirs: &Dirs) -> Option<Head> {
    if dirs.uses_reftable() {
        return None;
    }
    let text = read_ref_file(&dirs.git_dir, "HEAD")?;
    let line = text.lines().next()?.trim();
    if let Some(r) = line.strip_prefix("ref:") {
        let r = r.trim();
        let name = r.strip_prefix("refs/heads/").unwrap_or(r);
        return writable_name(name).then(|| Head::Branch(name.to_owned()));
    }
    (!line.is_empty() && writable_name(line)).then(|| Head::Detached(line.to_owned()))
}

/// Whether git could have written `name` as a ref name or a config value
/// naming one: no ASCII control character (`check-ref-format` refuses
/// them) and at most [`MAX_BRANCH_CHARS`] characters.
///
/// A name that is neither cannot survive the worker's cache entry as it
/// is (a line break is stored as a space, one past the cap makes an entry
/// read as a miss), so the tick, comparing the entry with the name it
/// read, never found it and spawned a worker on every render.
fn writable_name(name: &str) -> bool {
    !name.bytes().any(|b| b.is_ascii_control()) && name.chars().nth(MAX_BRANCH_CHARS).is_none()
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

/// Characters of a branch name (or an upstream's remote or merge value)
/// garnish takes: git's own limit on a ref name is the path length, and
/// the worker's cache entry has a cap to stay under.
const MAX_BRANCH_CHARS: usize = 4096;

/// `HEAD` asked of git, for a repository whose refs are not files
/// (reftable): `symbolic-ref -q --short HEAD` for a branch, and when that
/// says it is detached, `rev-parse --verify HEAD` for the commit.
///
/// # Errors
/// Propagates git failures (an unborn detached `HEAD` among them).
pub fn head_from_git(cwd: &Path, timeout: Duration) -> Result<Head, String> {
    let args = ["symbolic-ref", "-q", "--short", "HEAD"];
    let asked = git_call(git_program()?, cwd, &args, timeout, Stdout::Read)?;
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
/// The [`MAX_REF_BYTES`] cap bounds the read of a hostile `.git/HEAD`,
/// and [`head`] refuses a first line past [`MAX_BRANCH_CHARS`], so no
/// branch name is the size of the file for every render that cuts it
/// (`branch.max_length`) to walk over its clusters.
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
    read_bounded(&contained(root, name)?, max)
}

/// `root/name` resolved, when it stays inside `root` (itself resolved).
fn contained(root: &Path, name: &str) -> Option<PathBuf> {
    let path = root.join(name).canonicalize().ok()?;
    path.starts_with(root).then_some(path)
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

/// Bytes of `.git/config` read at a time. The tick reads the whole file
/// every second, and a buffer the size of a 500 KB config is fresh memory
/// the kernel faults in page by page each time, which cost as much as the
/// parse; one of this size is faulted in once and reused.
const CONFIG_CHUNK: usize = 64 * 1024;

/// The upstream of a branch: `(remote, remote-tracking ref)` such as
/// `("origin", "refs/remotes/origin/main")`.
///
/// The tracking ref assumes the remote's default fetch refspec
/// (`refs/heads/*:refs/remotes/<remote>/*`), as `git clone` writes it.
#[must_use]
pub fn upstream(dirs: &Dirs, branch: &str) -> Option<(String, String)> {
    // Through the same containment as every ref: `config` sits under the
    // git directory and is as symlinkable as `HEAD` is.
    let path = contained(&dirs.common_dir.canonicalize().ok()?, "config")?;
    let (file, _) = crate::claude_settings::open_regular(&path).ok()??;
    upstream_read(std::io::Read::take(file, MAX_CONFIG_BYTES), branch, CONFIG_CHUNK)
}

/// [`upstream`] over a config file's text read `chunk` bytes at a time:
/// `branch.<name>.remote` (the last one, as git keeps it) and
/// `branch.<name>.merge` (the first, which is what `@{upstream}` follows);
/// `None` when either is a value git cannot have written for a ref
/// ([`writable_name`]), or the text cannot be read.
fn upstream_read(
    reader: impl std::io::Read,
    branch: &str,
    chunk: usize,
) -> Option<(String, String)> {
    let mut remote: Option<String> = None;
    let mut merge: Option<String> = None;
    read_section(reader, b"branch", branch.as_bytes(), chunk, |key, value| {
        let Some(value) = value else { return };
        if key.eq_ignore_ascii_case(b"remote") {
            remote = Some(value);
        } else if key.eq_ignore_ascii_case(b"merge") && merge.is_none() {
            merge = Some(value);
        }
    })?;
    let remote = remote.filter(|r| writable_name(r))?;
    let merge = merge.filter(|m| writable_name(m))?;
    let short = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
    if remote == "." {
        return Some((remote, format!("refs/heads/{short}")));
    }
    Some((remote.clone(), format!("refs/remotes/{remote}/{short}")))
}

/// One `key = value` of a git config file.
struct ConfigEntry<'a> {
    /// The key, as written; git compares it without case.
    key: &'a [u8],
    /// The value, unquoted and unescaped; `None` for a bare key.
    value: Option<String>,
}

/// Read the git config text `reader` gives, `chunk` bytes at a time, and
/// call `each` with the key and value of every entry of the section
/// `[<section> "<sub>"]` ([`SectionParse`]), in order; `None` when a read
/// fails.
///
/// Each piece parsed ends at a line break that ends a line not ending in a
/// backslash, where no value, comment or header can go on, and the rest
/// moves to the front of the buffer for the next read to follow, so the
/// pieces parse as the whole text would. The buffer grows by `chunk` only
/// when a whole one holds no such line break.
fn read_section(
    mut reader: impl std::io::Read,
    section: &[u8],
    sub: &[u8],
    chunk: usize,
    mut each: impl FnMut(&[u8], Option<String>),
) -> Option<()> {
    let chunk = chunk.max(1);
    let mut parse = SectionParse::new(section, sub);
    let mut buf = vec![0; chunk];
    let mut filled = 0;
    let mut first = true;
    loop {
        if filled == buf.len() {
            buf.resize(buf.len().saturating_add(chunk), 0);
        }
        let got = loop {
            match reader.read(buf.get_mut(filled..)?) {
                Ok(got) => break got,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        };
        let kept = filled;
        filled = filled.saturating_add(got);
        let text = buf.get(..filled)?;
        let cut = if got == 0 { filled } else { safe_cut(text, kept) };
        let whole = text.get(..cut)?;
        // A byte-order mark leads the file, and git skips it.
        let whole =
            if first { whole.strip_prefix(b"\xef\xbb\xbf").unwrap_or(whole) } else { whole };
        first &= cut == 0;
        parse.feed(whole, &mut each);
        if buf.get(cut..filled).is_some() {
            buf.copy_within(cut..filled, 0);
        }
        filled = filled.saturating_sub(cut);
        if got == 0 {
            return Some(());
        }
    }
}

/// Where `buf` may be cut: after its last line break that ends a line not
/// ending in a backslash, looking only at the breaks from `from` on (those
/// before were looked at already); 0 when there is none.
fn safe_cut(buf: &[u8], from: usize) -> usize {
    let mut end = buf.len();
    loop {
        let Some(nl) = buf.get(from..end).and_then(|s| s.iter().rposition(|b| *b == b'\n')) else {
            return 0;
        };
        let at = from.saturating_add(nl);
        match buf.get(..=at) {
            Some(line) if !ends_in_backslash(line) => return at.saturating_add(1),
            _ => end = at,
        }
    }
}

/// The entries of one section of a git config, `[<section> "<sub>"]` (the
/// section name in any case), parsed as git parses them (config.c
/// `get_base_var`, `get_value`, `parse_value`): a value may be quoted
/// anywhere in it, knows the escapes `\"`, `\\`, `\t`, `\n`, `\b` and a
/// backslash-newline continuation, ends at a `;` or `#` outside quotes,
/// and loses its leading and trailing blanks; blanks are git's four
/// (`isspace`) and a CRLF is a line break. git refuses the whole file over
/// one malformed line; here that line alone is skipped.
///
/// `sync` reads the config on every tick, and it holds a section per
/// tracked branch (review 2026-09-25: parsing every entry of a 500 KB
/// config cost 3 ms a tick). So the text is bytes, of which only the
/// values kept are decoded (lossily: a stray byte costs what it touches);
/// a header is parsed only when its first bytes do not already rule it
/// out ([`surely_other`]), and a section nobody wants is jumped over by
/// [`next_header`] without being parsed or allocating. The syntax is all
/// ASCII, and no byte of a multi-byte UTF-8 character is, so reading
/// bytes changes nothing.
struct SectionParse<'a> {
    /// The section's name.
    section: &'a [u8],
    /// The subsection's.
    sub: &'a [u8],
    /// Whether a header may be ruled out by [`surely_other`]: not when the
    /// subsection holds a quote or a backslash, which only escapes spell.
    quick: bool,
    /// Whether the text fed so far ends inside the section.
    inside: bool,
}

impl<'a> SectionParse<'a> {
    /// A parse of the section `[<section> "<sub>"]`, before any text.
    fn new(section: &'a [u8], sub: &'a [u8]) -> Self {
        let quick = !section.contains(&b'.') && !sub.iter().any(|b| matches!(b, b'"' | b'\\'));
        Self { section, sub, quick, inside: false }
    }

    /// Parse `text`, which starts where a token may and ends where nothing
    /// goes on past it (see [`read_section`]), calling `each` with the key
    /// and value of every entry in the section.
    fn feed(&mut self, text: &[u8], each: &mut impl FnMut(&[u8], Option<String>)) {
        let mut rest = text;
        loop {
            rest = skip_blanks(rest);
            let Some(&first) = rest.first() else { break };
            if let Some(after) = rest.strip_prefix(b"[") {
                if self.quick && surely_other(after, self.section, self.sub) {
                    self.inside = false;
                    rest = next_header_past(after);
                } else if let Some(header) = config_header(after) {
                    self.inside = header.section.eq_ignore_ascii_case(self.section)
                        && header.sub.as_deref() == Some(self.sub);
                    rest = header.rest;
                } else {
                    self.inside = false;
                    rest = after_line(after);
                }
            } else if !self.inside {
                rest = next_header(rest);
            } else if first.is_ascii_alphabetic() {
                let (entry, next) = config_entry(rest, true);
                if let Some(entry) = entry {
                    each(entry.key, entry.value);
                }
                rest = next;
            } else {
                rest = after_line(rest);
            }
        }
    }
}

/// git's `isspace`: a blank of the config syntax.
const fn is_config_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// `s` past its leading blanks.
fn skip_blanks(s: &[u8]) -> &[u8] {
    let blanks = s.iter().take_while(|b| is_config_space(**b)).count();
    s.get(blanks..).unwrap_or_default()
}

/// The next byte of a config text as git's `get_next_char` reads it (a
/// CRLF is one line break), and the text after it.
const fn next_config_byte(s: &[u8]) -> Option<(u8, &[u8])> {
    match s {
        [b'\r', b'\n', rest @ ..] => Some((b'\n', rest)),
        [b, rest @ ..] => Some((*b, rest)),
        [] => None,
    }
}

/// Where `byte` first occurs in `hay`, eight bytes at a time: a config is
/// searched a section (some 70 bytes) at a time, where a byte loop, or
/// `core`'s `memchr` with its byte-wise head and tail, was the better part
/// of reading a 500 KB config.
///
/// Each word is XOR-ed with `byte` repeated, which zeroes the bytes that
/// match, and `(w - 0x01…) & !w & 0x80…` flags a zero byte: the lowest
/// flag is always a true one (a borrow only runs upwards from a zero
/// byte), and a little-endian word's lowest byte comes first.
fn find_byte(hay: &[u8], byte: u8) -> Option<usize> {
    const ONES: u64 = u64::from_le_bytes([0x01; 8]);
    const HIGHS: u64 = u64::from_le_bytes([0x80; 8]);
    let repeated = u64::from_le_bytes([byte; 8]);
    let (words, tail) = hay.as_chunks::<8>();
    for (i, word) in words.iter().enumerate() {
        let word = u64::from_le_bytes(*word) ^ repeated;
        let zeros = word.wrapping_sub(ONES) & !word & HIGHS;
        if zeros != 0 {
            let within = usize::try_from(zeros.trailing_zeros().checked_div(8)?).ok()?;
            return i.checked_mul(8)?.checked_add(within);
        }
    }
    let within = tail.iter().position(|b| *b == byte)?;
    words.len().checked_mul(8)?.checked_add(within)
}

/// The text after the line `s` starts in.
fn after_line(s: &[u8]) -> &[u8] {
    find_byte(s, b'\n').and_then(|nl| s.get(nl.saturating_add(1)..)).unwrap_or_default()
}

/// Whether `lines`, which ends with a line break (or is empty), ends with
/// a line whose last character is a backslash: the only way git joins a
/// line to the next is a `\` right before the break (a CRLF included).
fn ends_in_backslash(lines: &[u8]) -> bool {
    let line = lines.strip_suffix(b"\n").unwrap_or(lines);
    line.strip_suffix(b"\r").unwrap_or(line).ends_with(b"\\")
}

/// Whether the header `after` (the text after its `[`) is surely not
/// `[<section> "<sub>"]`, told from its first bytes: its name is not the
/// section's, it has no subsection, or its subsection differs from `sub`
/// before any backslash (or it is malformed, which is no match either).
/// `false` when unsure (the old `[section.sub]` form, an escape), and then
/// the header is parsed. The caller keeps a `sub` holding a quote or a
/// backslash, which only escapes can spell, away from here.
fn surely_other(after: &[u8], section: &[u8], sub: &[u8]) -> bool {
    let Some((name, rest)) = after.split_at_checked(section.len()) else { return true };
    if !name.eq_ignore_ascii_case(section) {
        return true;
    }
    match rest.first() {
        Some(b'.') => false,
        Some(b' ' | b'\t' | b'\r') => {
            let blanks = rest.iter().take_while(|b| matches!(b, b' ' | b'\t' | b'\r')).count();
            let Some(body) = rest.get(blanks..).and_then(|r| r.strip_prefix(b"\"")) else {
                return true;
            };
            let same = body.iter().zip(sub).take_while(|(a, b)| a == b).count();
            match body.get(same) {
                Some(b'"') => same < sub.len(),
                Some(b'\\') => false,
                _ => true,
            }
        }
        _ => true,
    }
}

/// The text from the next section header on, given `s` at a place where a
/// token may start (not inside a value): everything in between belongs to
/// a section nobody asked for.
fn next_header(s: &[u8]) -> &[u8] {
    scan_headers(s, true).unwrap_or_default()
}

/// [`next_header`] from inside a header nobody wants, `after` being the
/// text after its `[`, which was never parsed: the header is parsed only
/// when what follows it on its line may matter (another `[`, or a `\` at
/// the end that may join the next line to it).
fn next_header_past(after: &[u8]) -> &[u8] {
    scan_headers(after, false).unwrap_or_else(|| {
        next_header(config_header(after).map_or_else(|| after_line(after), |h| h.rest))
    })
}

/// The text from the next header on, from `s`: a place where a token may
/// start when `at_token`, else the rest of a header's line that was not
/// parsed, in which case `None` says the header must be.
///
/// A header can only open a line (after blanks) or follow another header,
/// and a line starts a token unless the line before it ends in a
/// backslash. So the search is for a `[` (a byte search, which strides
/// over the section) with only blanks before it on its line: when the line
/// before does not end in `\`, that is the header. When it does, whether
/// the `\` joins the lines depends on the value it ends (a comment, a
/// quote, an escaped backslash), so the lines are parsed from the first of
/// the run of such lines, which does start a token. Every byte is searched
/// once and parsed at most once: the search resumes past a line whose `[`
/// has something before it, and past whatever the parse consumed.
fn scan_headers(s: &[u8], at_token: bool) -> Option<&[u8]> {
    let mut from = s;
    let mut at_token = at_token;
    let mut offset = 0;
    loop {
        let Some(found) = from.get(offset..).and_then(|t| find_byte(t, b'[')) else {
            return Some(&[]);
        };
        let at = offset.saturating_add(found);
        let (before, header) = from.split_at_checked(at)?;
        let lead = before.iter().rev().take_while(|b| matches!(b, b' ' | b'\t' | b'\r')).count();
        let (earlier, _) = before.split_at_checked(before.len().saturating_sub(lead))?;
        if !(earlier.ends_with(b"\n") || (earlier.is_empty() && at_token)) {
            if !at_token && find_byte(before, b'\n').is_none() {
                // On the line of the header that was not parsed, where it
                // may follow that header's `]`.
                return None;
            }
            // Every later `[` on this line has this one before it.
            offset = find_byte(header, b'\n').map_or(from.len(), |nl| at.saturating_add(nl));
            continue;
        }
        if !ends_in_backslash(earlier) {
            return Some(header);
        }
        let run = start_of_run(earlier);
        if run == 0 && !at_token {
            return None;
        }
        let rest = skip_tokens(from.get(run..)?, header.len());
        if rest.starts_with(b"[") {
            return Some(rest);
        }
        from = rest;
        at_token = true;
        offset = 0;
    }
}

/// Where the run of lines ending in a backslash that `lines` ends with
/// begins (a byte offset into `lines`): the first line of the run, which
/// starts a token because the line before it does not end in `\`, or the
/// start of `lines`, which the caller knows starts one.
fn start_of_run(lines: &[u8]) -> usize {
    let mut end = lines.len();
    loop {
        let Some(head) = lines.get(..end) else { return 0 };
        let body = head.strip_suffix(b"\n").unwrap_or(head);
        let start = body.iter().rposition(|b| *b == b'\n').map_or(0, |nl| nl.saturating_add(1));
        match lines.get(..start) {
            Some(prev) if start > 0 && ends_in_backslash(prev) => end = start,
            _ => return start,
        }
    }
}

/// Tokens of a section nobody wants, parsed from `s` (where a token may
/// start) and skipped without an allocation, up to a header (returned
/// from its `[`) or to the first token that starts past the point `until`
/// bytes before the end of `s`.
fn skip_tokens(s: &[u8], until: usize) -> &[u8] {
    let mut rest = s;
    loop {
        rest = skip_blanks(rest);
        let Some(&first) = rest.first() else { return rest };
        if first == b'[' || rest.len() < until {
            return rest;
        }
        rest = if first.is_ascii_alphabetic() {
            config_entry(rest, false).1
        } else {
            after_line(rest)
        };
    }
}

/// A section header, as [`config_header`] reads it.
struct Header<'a> {
    /// The section name, as written.
    section: &'a [u8],
    /// The subsection: the old `[section.sub]` form's lowercased, as git
    /// does, and borrowed when it needs no change.
    sub: Option<Cow<'a, [u8]>>,
    /// The text after the `]`.
    rest: &'a [u8],
}

/// A section header after its `[`; `None` when malformed.
fn config_header(s: &[u8]) -> Option<Header<'_>> {
    let name_len =
        s.iter().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.')).count();
    let (name, rest) = s.split_at_checked(name_len)?;
    match next_config_byte(rest)? {
        (b']', after) => {
            let Some(dot) = find_byte(name, b'.') else {
                return Some(Header { section: name, sub: None, rest: after });
            };
            let (section, sub) = name.split_at_checked(dot)?;
            let sub = sub.get(1..)?;
            let sub = if sub.iter().any(u8::is_ascii_uppercase) {
                Cow::Owned(sub.to_ascii_lowercase())
            } else {
                Cow::Borrowed(sub)
            };
            return Some(Header { section, sub: Some(sub), rest: after });
        }
        (b'\n', _) => return None,
        (c, _) if is_config_space(c) => {}
        _ => return None,
    }
    let blanks = rest.iter().take_while(|b| matches!(b, b' ' | b'\t' | b'\r')).count();
    let body = rest.get(blanks..)?.strip_prefix(b"\"")?;
    let (raw, after) = body.split_at_checked(find_byte(body, b'"')?)?;
    let (sub, rest) = if raw.iter().any(|b| matches!(b, b'\\' | b'\n')) {
        let mut sub = Vec::new();
        let mut rest = body;
        loop {
            let (c, next) = next_config_byte(rest)?;
            rest = next;
            match c {
                b'"' => break,
                b'\n' => return None,
                b'\\' => {
                    let (escaped, next) = next_config_byte(rest)?;
                    rest = next;
                    if escaped == b'\n' {
                        return None;
                    }
                    sub.push(escaped);
                }
                c => sub.push(c),
            }
        }
        (Cow::Owned(sub), rest)
    } else {
        (Cow::Borrowed(raw), after.get(1..)?)
    };
    Some(Header { section: name, sub: Some(sub), rest: rest.strip_prefix(b"]")? })
}

/// A `key[ = value]` at its first character: the entry, when it is well
/// formed and `keep` asks for it (a skipped value allocates nothing), and
/// the text after its (possibly continued) line.
fn config_entry(s: &[u8], keep: bool) -> (Option<ConfigEntry<'_>>, &[u8]) {
    let key_len = s.iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'-').count();
    let Some((key, rest)) = s.split_at_checked(key_len) else { return (None, after_line(s)) };
    let blanks = rest.iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
    let rest = rest.get(blanks..).unwrap_or_default();
    match next_config_byte(rest) {
        None => (keep.then_some(ConfigEntry { key, value: None }), rest),
        Some((b'\n', next)) => (keep.then_some(ConfigEntry { key, value: None }), next),
        Some((b'=', next)) => {
            let mut value = Vec::new();
            let (ok, next) = config_value(next, keep.then_some(&mut value));
            let entry = (ok && keep).then(|| ConfigEntry {
                key,
                value: Some(String::from_utf8_lossy(&value).into_owned()),
            });
            (entry, next)
        }
        Some((_, next)) => (None, after_line(next)),
    }
}

/// A value after its `=`, read into `value` when there is one: whether it
/// is well formed (no unknown escape, no open quote) and the text after
/// its (possibly continued) line.
fn config_value<'a>(s: &'a [u8], mut value: Option<&mut Vec<u8>>) -> (bool, &'a [u8]) {
    let mut rest = s;
    let mut quoted = false;
    let mut comment = false;
    // The length before a run of unquoted blanks, dropped if the run ends
    // the value.
    let mut trim_to: Option<usize> = None;
    loop {
        // The end of the text reads as a line break, as it does to git.
        let (c, next) = next_config_byte(rest).unwrap_or((b'\n', rest));
        rest = next;
        if c == b'\n' {
            if let (Some(v), Some(len)) = (value.as_deref_mut(), trim_to) {
                v.truncate(len);
            }
            return (!quoted, rest);
        }
        if comment {
            continue;
        }
        if is_config_space(c) && !quoted {
            if let Some(v) = value.as_deref_mut() {
                trim_to = trim_to.or(Some(v.len()));
                if !v.is_empty() {
                    v.push(c);
                }
            }
            continue;
        }
        if !quoted && matches!(c, b';' | b'#') {
            comment = true;
            continue;
        }
        trim_to = None;
        let c = match c {
            b'\\' => {
                let (escaped, next) = next_config_byte(rest).unwrap_or((b'\n', rest));
                rest = next;
                match escaped {
                    b'\n' => continue,
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'n' => b'\n',
                    b'\\' | b'"' => escaped,
                    _ => return (false, after_line(rest)),
                }
            }
            b'"' => {
                quoted = !quoted;
                continue;
            }
            c => c,
        };
        if let Some(v) = value.as_deref_mut() {
            v.push(c);
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
/// - lazy fetching in a partial clone, which runs the promisor remote's
///   `uploadpack` from the same file, is off through `GIT_NO_LAZY_FETCH`
///   (honoured since the May 2024 security releases, 2.39.4 onward on
///   every maintained line, and by distribution gits that took the fix;
///   an older git ignores it), and every call but [`fetch`] refuses every
///   transport besides ([`NO_TRANSPORT`]), which any git honours.
///
/// The user typing `git status` in that checkout would run all of these
/// too; what is new is that garnish runs git on a *timer*, unasked.
const NO_COMMAND_HOOKS: [&str; 2] = ["-c", "core.fsmonitor="];

/// `GIT_ALLOW_PROTOCOL` empty: no transport at all, on every git call but
/// [`fetch`] (review 2026-09-25). None of them needs one, and a lazy fetch
/// in a hostile partial clone would otherwise start the repository's own
/// `uploadpack` on a git too old for `GIT_NO_LAZY_FETCH`; the variable,
/// unlike `protocol.<name>.allow`, is out of that repository's reach.
/// `fetch` keeps whatever the user set.
const NO_TRANSPORT: (&str, &str) = ("GIT_ALLOW_PROTOCOL", "");

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

/// `git <args>` through [`run_program`] with the command hooks cleared and
/// no transport ([`NO_TRANSPORT`]), its failures described in terms of the
/// caller's own `args`.
fn git_call(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    timeout: Duration,
    want: Stdout,
) -> Result<Finished, String> {
    let (key, value) = NO_TRANSPORT;
    git_with(program, cwd, args, &[(key, std::ffi::OsStr::new(value))], timeout, want)
}

/// [`git_call`] with `env` in place of [`NO_TRANSPORT`]: for [`fetch`],
/// the one call that reaches another repository.
fn git_with(
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
    let finished = git_call(git_program()?, cwd, args, timeout, Stdout::Read)?;
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
    let finished = git_call(git_program()?, cwd, args, timeout, Stdout::Discard)?;
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
                        let listed = git_call(git_program()?, cwd, &args, timeout, Stdout::Read)?;
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
    let finished = git_with(git_program()?, cwd, &args, &env, timeout, Stdout::Discard)?;
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
    /// cluster on the tick path. A name past [`MAX_BRANCH_CHARS`], or one
    /// holding a control character, is no head at all: git writes neither,
    /// and the worker's entry cannot carry either (one is past the entry's
    /// cap, the other loses its line break), so the tick never found the
    /// entry it asked for and spawned a worker every time (review
    /// 2026-09-25).
    #[test]
    fn a_ref_file_is_bounded_so_a_huge_head_cannot_become_a_branch_name() {
        let (_d, work) = repo();
        let dirs = discover(&work).unwrap();
        let set_head = |text: &str| std::fs::write(work.join(".git/HEAD"), text).unwrap();
        let huge = "a".repeat(usize::try_from(MAX_REF_BYTES).unwrap_or(0) * 2);
        set_head(&format!("ref: refs/heads/{huge}\n"));
        assert_eq!(head(&dirs), None);
        let longest = "b".repeat(MAX_BRANCH_CHARS);
        set_head(&format!("ref: refs/heads/{longest}\n"));
        assert_eq!(head(&dirs), Some(Head::Branch(longest.clone())));
        set_head(&format!("ref: refs/heads/{longest}b\n"));
        assert_eq!(head(&dirs), None);
        for bad in ["ref: refs/heads/ma\u{1}in\n", "ref: refs/heads/ma\rin\n", "abc\u{7f}def\n"] {
            set_head(bad);
            assert_eq!(head(&dirs), None, "{bad:?}");
        }
        set_head("ref: refs/heads/é\n");
        assert_eq!(head(&dirs), Some(Head::Branch("é".into())), "only control characters go");
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
        let up = |text: &str, branch: &str| upstream_in(text.as_bytes(), branch);
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

    /// A value git cannot have written for a ref (a control character,
    /// here through the `\n` and `\t` escapes, or a name past
    /// [`MAX_BRANCH_CHARS`]) is no upstream: the worker's entry cannot
    /// carry it as it is, so `sync` found no entry for it on any tick and
    /// spawned a worker on every one (review 2026-09-25).
    #[test]
    fn an_upstream_git_cannot_have_written_is_no_upstream() {
        let up = |remote: &str, merge: &str| {
            let text = format!("[branch \"a\"]\nremote = {remote}\nmerge = {merge}\n");
            upstream_in(text.as_bytes(), "a")
        };
        assert!(up("origin", "refs/heads/a").is_some());
        assert_eq!(up("origin", "\"refs/heads/ma\\nin\""), None);
        assert_eq!(up("origin", "refs/heads/ma\\tin"), None);
        assert_eq!(up("ori\\bgin", "refs/heads/a"), None);
        assert_eq!(up("origin", "refs/heads/a\u{7f}"), None);
        let long = "c".repeat(MAX_BRANCH_CHARS);
        assert!(up("origin", &long).is_some());
        assert_eq!(up("origin", &format!("{long}c")), None);
        assert_eq!(up(&format!("{long}c"), "refs/heads/a"), None);
    }

    /// A section nobody wants is jumped over, not parsed, so the jump must
    /// land where git's parse would: a `\` at a line's end joins the next
    /// line to a value (and a header there is part of it), except in a
    /// comment, after an escaped backslash, or on a header's line; a `[`
    /// inside a value or a comment opens nothing; a header may follow
    /// another on its line; a CRLF is a line break; a byte-order mark
    /// leads. An unknown escape costs its own line and nothing after it on
    /// that line.
    #[test]
    fn a_skipped_section_ends_where_git_says() {
        let up = |text: &str| upstream_in(text.as_bytes(), "a");
        let origin = |r: &str| Some(("origin".to_owned(), format!("refs/remotes/origin/{r}")));
        let real = "[branch \"a\"]\nremote = origin\nmerge = refs/heads/a\n";
        let decoy = "[branch \"a\"]\nremote = decoy\nmerge = refs/heads/decoy\n";
        let decoyed = Some(("decoy".to_owned(), "refs/remotes/decoy/decoy".to_owned()));
        for (skipped, expected) in [
            // The decoy's header continues `x`, so its entries are core's.
            (format!("[core]\nx = y\\\n{decoy}{real}"), origin("a")),
            (format!("[core]\nx = y\\\r\n{decoy}{real}"), origin("a")),
            (
                format!("[core]\nx = \"y\\\n[branch \"a\"]\\\n\"\nremote = decoy\n{real}"),
                origin("a"),
            ),
            (format!("[core]\nx = y\\\n  z\\\n{decoy}{real}"), origin("a")),
            (format!("[core] x = y\\\n{decoy}{real}"), origin("a")),
            (format!("[branch \"b\"] x = y\\\n{decoy}{real}"), origin("a")),
            // Each of these `\` ends something that does not continue.
            (format!("[core]\n# c \\\n{decoy}"), decoyed.clone()),
            (format!("[core]\nx = y ; c \\\n{decoy}"), decoyed.clone()),
            (format!("[core]\nx = y\\\\\n{decoy}"), decoyed.clone()),
            (format!("[branch \"b\"] # c \\\n{decoy}"), decoyed.clone()),
            (format!("[core]\nx = y\\q\\\n{decoy}"), decoyed.clone()),
            // A `[` that opens no header.
            (format!("[core]\nurl = [::1]\n# {decoy}; {decoy}{real}"), origin("a")),
            (
                "[core] [branch \"a\"]\nremote = origin\nmerge = refs/heads/a\n".to_owned(),
                origin("a"),
            ),
            (
                "[branch \"b\"] [branch \"a\"] remote = origin\nmerge = refs/heads/a\n".to_owned(),
                origin("a"),
            ),
            (format!("[branch \"b\"]\r\nx = y\r\n{}", real.replace('\n', "\r\n")), origin("a")),
            (format!("\u{feff}{real}"), origin("a")),
            (format!("[core]\n\t[\n{real}"), origin("a")),
            (
                format!(
                    "[branch \"a\"]\nremote = origin\nmerge = refs/heads/a\\q remote = evil\n{real}"
                ),
                origin("a"),
            ),
            (format!("[branch \"b\n{real}"), origin("a")),
            (format!("[branch \"b\\\n{real}"), origin("a")),
            (format!("[branch\r\n\"b\"]\n{real}"), origin("a")),
            (format!("[branch \"a\\\"]\n{decoy}"), decoyed),
        ] {
            assert_eq!(up(&skipped), expected, "{skipped:?}");
        }
    }

    /// Adversarial shapes stay linear: the search for a header resumes past
    /// every line and value it has looked at.
    #[test]
    fn a_hostile_config_is_skipped_in_linear_time() {
        let started = std::time::Instant::now();
        let real = "[branch \"a\"]\nremote = origin\nmerge = refs/heads/a\n";
        for body in [
            format!("[core]\nx = {}\n", "[".repeat(400_000)),
            format!("[core]\nx = y\\\n{}\n", "[\\\n".repeat(100_000)),
            "# \\\n[x] \\\n".repeat(40_000),
            format!("[branch \"b\"]{}\n", " [".repeat(200_000)),
        ] {
            let text = format!("{body}{real}");
            assert_eq!(upstream_in(text.as_bytes(), "a").map(|u| u.0), Some("origin".to_owned()));
        }
        assert!(started.elapsed() < Duration::from_secs(20), "took {:?}", started.elapsed());
    }

    /// [`upstream`] over a config's whole text.
    fn upstream_in(text: &[u8], branch: &str) -> Option<(String, String)> {
        upstream_read(text, branch, CONFIG_CHUNK)
    }

    /// An entry with its section: name, subsection, key, value.
    type Placed = (Vec<u8>, Option<Vec<u8>>, Vec<u8>, Option<String>);

    /// Every entry of `text` with its section, by the parser's pieces and
    /// no skipping at all: the reference the skipping must agree with.
    fn every_entry(text: &[u8]) -> Vec<Placed> {
        let mut out = Vec::new();
        let mut rest = text.strip_prefix(b"\xef\xbb\xbf").unwrap_or(text);
        let mut section: Option<(Vec<u8>, Option<Vec<u8>>)> = None;
        loop {
            rest = skip_blanks(rest);
            let Some(&first) = rest.first() else { break };
            if let Some(after) = rest.strip_prefix(b"[") {
                if let Some(header) = config_header(after) {
                    section = Some((header.section.to_vec(), header.sub.map(Cow::into_owned)));
                    rest = header.rest;
                } else {
                    section = None;
                    rest = after_line(after);
                }
            } else if first.is_ascii_alphabetic() {
                let (entry, next) = config_entry(rest, true);
                if let (Some(entry), Some((name, sub))) = (entry, &section) {
                    out.push((name.clone(), sub.clone(), entry.key.to_vec(), entry.value));
                }
                rest = next;
            } else {
                rest = after_line(rest);
            }
        }
        out
    }

    /// The skipping (a quick look at each header, a byte search for the
    /// next one, a parse only where a `\` may join lines) agrees with a
    /// parse of every token, over random configs spelt from the pieces
    /// that matter to it.
    #[test]
    fn skipping_agrees_with_parsing_everything() {
        const PIECES: [&str; 30] = [
            "[branch \"a\"]",
            "[branch \"ab\"]",
            "[BRANCH \"a\"]",
            "[branch.a]",
            "[branch \"a",
            "[branch \"\\a\"]",
            "[branch]",
            "[core]",
            "[branch  \"a\"]",
            "[",
            "]",
            "\"",
            "\\",
            "\n",
            "\n",
            "\n",
            "\r\n",
            "\r",
            " ",
            "\t",
            "#",
            ";",
            "k = v",
            "remote = r",
            "merge = m",
            "=",
            "b",
            "\\\n",
            "\\\\",
            "[x] ",
        ];
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = next() % 40;
            let text: String =
                (0..len).map(|_| PIECES[usize::try_from(next() % 30).unwrap()]).collect();
            let chunk = usize::try_from(next() % 24 + 1).unwrap();
            let every = every_entry(text.as_bytes());
            for (section, sub) in [("branch", "a"), ("branch", "ab"), ("core", "x"), ("x", "a")] {
                let expected: Vec<(Vec<u8>, Option<String>)> = every
                    .iter()
                    .filter(|(s, u, _, _)| {
                        s.eq_ignore_ascii_case(section.as_bytes())
                            && u.as_deref() == Some(sub.as_bytes())
                    })
                    .map(|(_, _, k, v)| (k.clone(), v.clone()))
                    .collect();
                for chunk in [CONFIG_CHUNK, chunk] {
                    let mut got: Vec<(Vec<u8>, Option<String>)> = Vec::new();
                    let (section, sub) = (section.as_bytes(), sub.as_bytes());
                    read_section(text.as_bytes(), section, sub, chunk, |key, value| {
                        got.push((key.to_vec(), value));
                    })
                    .unwrap();
                    assert_eq!(got, expected, "{section:?} {sub:?} by {chunk} in {text:?}");
                }
            }
        }
    }

    /// A config read a few bytes at a time gives what it gives read whole:
    /// a piece ends only where no `\` joins its last line to the next, and
    /// the byte-order mark is skipped once, at the start.
    #[test]
    fn a_config_read_in_pieces_reads_as_a_whole() {
        let text = "\u{feff}[core]\nx = y\\\n[branch \"a\"]\nremote = decoy\n[branch \"a\"]\r\n\tremote = origin\n\tmerge = \"refs/heads/\\\na\" ; c\n";
        let whole = upstream_read(text.as_bytes(), "a", CONFIG_CHUNK);
        assert_eq!(whole, Some(("origin".to_owned(), "refs/remotes/origin/a".to_owned())));
        for chunk in 0..=text.len() {
            assert_eq!(upstream_read(text.as_bytes(), "a", chunk), whole, "by {chunk}");
        }
        assert_eq!(safe_cut(b"a\nb\\\nc", 0), 2);
        assert_eq!(safe_cut(b"a\nb\\\r\nc", 0), 2);
        assert_eq!(safe_cut(b"a\nb\\\r\nc\n", 0), 8);
        assert_eq!(safe_cut(b"a\\\nb\\\n", 0), 0);
        assert_eq!(safe_cut(b"a\nb\n", 3), 4);
    }

    /// The word-at-a-time search finds what a byte loop finds, at every
    /// offset and length around a word, whatever the neighbouring bytes
    /// (a zero byte next to a match is where the trick could misfire).
    #[test]
    fn find_byte_finds_what_a_byte_loop_finds() {
        for len in 0..70 {
            for at in 0..=len {
                for (byte, filler) in
                    [(b'[', b'a'), (0, 1), (1, 0), (0x80, 0x7f), (0xff, 0), (b'\n', 0xfe)]
                {
                    let mut hay = vec![filler; len];
                    if let Some(slot) = hay.get_mut(at) {
                        *slot = byte;
                    }
                    if let Some(slot) = hay.get_mut(at + 3) {
                        *slot = byte;
                    }
                    let expected = hay.iter().position(|b| *b == byte);
                    assert_eq!(find_byte(&hay, byte), expected, "{hay:?} {byte}");
                }
            }
        }
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
        let finished = git_call(program, cwd, args, timeout, want)?;
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
