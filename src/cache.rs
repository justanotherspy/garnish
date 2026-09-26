//! On-disk cache for module results, shared by ticks and background workers.
//!
//! One small file per (scope, module). The format needs no parser:
//!
//! ```text
//! v1 <computed_at_ms> <ttl_ms> ok|err
//! key=value
//! …
//! ```
//!
//! Writes go to a temporary file followed by `rename`, so readers never see a
//! torn entry. A malformed or truncated file simply counts as a miss.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use crate::time::now_millis;

/// Environment variable overriding the cache root.
pub const CACHE_DIR_ENV: &str = "GARNISH_CACHE_DIR";

/// Locks older than this are considered abandoned.
///
/// Shorter where pid liveness cannot be checked (`/proc` is Linux-only),
/// but still longer than a worker can hold one (a `sync` worker's fetch
/// and count, checked where those timeouts live): only a killed worker
/// leaves a lock there, since the tick never takes one off Linux.
pub const LOCK_STALE_MS: i64 = if cfg!(target_os = "linux") { 60_000 } else { 30_000 };

/// Bytes of an entry file read: a few values and at most two
/// [`MAX_ERROR_CHARS`] texts, so anything near this is not an entry.
const MAX_ENTRY_BYTES: u64 = 64 * 1024;

/// Bytes of a lock file read: `pid epoch_ms` and a newline.
const MAX_LOCK_BYTES: u64 = 256;

/// Locks younger than this are trusted without checking the pid (hand-over window).
pub const LOCK_GRACE_MS: i64 = 2_000;

/// Session directories idle for longer than this are swept.
pub const GC_MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;

/// Upper bound on directories removed per sweep.
pub const GC_MAX_PER_SWEEP: usize = 50;

/// Characters of a failed refresh's error text an entry keeps (SPEC § 5:
/// every string from outside is bounded). Enough for a git message,
/// nowhere near enough to matter to a tick that reads the file.
pub const MAX_ERROR_CHARS: usize = 500;

/// The cache root a set of environment values names (SPEC § 6), highest
/// precedence first: `GARNISH_CACHE_DIR`, `$XDG_RUNTIME_DIR/garnish`,
/// `$XDG_CACHE_HOME/garnish`, `~/.cache/garnish` (macOS:
/// `~/Library/Caches/garnish`); `None` when none is set, which leaves
/// [`private_root`] under the temp directory.
///
/// The lookup and the platform are parameters so the chain can be tested
/// without setting process-wide variables — which a test cannot do at all
/// here, `set_var` being `unsafe`. Moving the root strands the locks and
/// entries of every worker already running against the old one, so the
/// order is worth pinning; the macOS arm has no other coverage than CI's
/// macOS job.
fn root_from(lookup: impl Fn(&str) -> Option<PathBuf>, macos: bool) -> Option<PathBuf> {
    let xdg = |key: &str| crate::config::xdg_base(lookup(key)).map(|d| d.join("garnish"));
    lookup(CACHE_DIR_ENV)
        .or_else(|| xdg("XDG_RUNTIME_DIR"))
        .or_else(|| xdg("XDG_CACHE_HOME"))
        .or_else(|| {
            lookup("HOME").map(|h| {
                if macos {
                    h.join("Library").join("Caches").join("garnish")
                } else {
                    h.join(".cache").join("garnish")
                }
            })
        })
}

/// The last-resort root, `<base>/garnish-<uid>` (`base` being the temp
/// directory), or why it cannot be used, with the path it would have had.
///
/// The temp directory is shared by every user, and a root another user
/// made first would let them read the entries, plant them, or aim the
/// sweep and the temp-file writes through a link at this user's files. So
/// the directory is per user, created `0700`, and refused unless it is a
/// real directory this user owns that nobody else can write to. The uid
/// comes from a file this process creates, there being no `libc` here.
fn private_root(base: &Path) -> Result<PathBuf, (PathBuf, String)> {
    let probe = base.join(format!(".garnish-uid.{}.{}", std::process::id(), now_millis()));
    let uid = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .and_then(|f| f.metadata())
        .map(|m| m.uid());
    let _ = fs::remove_file(&probe);
    let uid = uid.map_err(|e| (base.join("garnish"), format!("no user id: {e}")))?;
    let root = base.join(format!("garnish-{uid}"));
    let refuse = |why: String| Err((root.clone(), why));
    match fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return refuse(e.to_string()),
    }
    match fs::symlink_metadata(&root) {
        Ok(m) if !m.is_dir() => refuse("not a directory".to_owned()),
        Ok(m) if m.uid() != uid => refuse(format!("owned by uid {}", m.uid())),
        Ok(m) if m.mode() & 0o022 != 0 => refuse("writable by other users".to_owned()),
        Ok(_) => Ok(root),
        Err(e) => refuse(e.to_string()),
    }
}

/// Create `dir` and its missing parents `0700`: an entry may carry the
/// account's email address, and nobody else needs to list the rest.
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

/// A new file at `path`, never one that was there: whatever sits at the
/// name (a leftover of a killed process, or a link planted to aim the
/// write at another file) is unlinked first, and `create_new` refuses the
/// name if anything reappears, rather than following it.
pub(crate) fn create_fresh(path: &Path) -> std::io::Result<fs::File> {
    let _ = fs::remove_file(path);
    fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

/// Where an entry lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Per Claude Code session.
    Session(String),
    /// Per repository worktree: a hash of the git common dir and the
    /// per-worktree git dir ([`crate::git::Dirs::cache_key`]).
    Repo(String),
}

impl Scope {
    fn dir(&self, root: &Path) -> PathBuf {
        match self {
            Self::Session(id) => root.join("sessions").join(sanitize(id)),
            Self::Repo(hash) => root.join("repos").join(sanitize(hash)),
        }
    }
}

fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() { "_".to_owned() } else { cleaned }
}

/// A stable hash for cache keys (FNV-1a over UTF-8), rendered as hex.
#[must_use]
pub fn key_hash(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for b in part.bytes().chain(std::iter::once(0)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// Entry status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The last refresh succeeded.
    Ok,
    /// The last refresh failed; the body holds the error text.
    Err,
}

/// Text from outside (a command's stderr, a fetch's complaint) as a cache
/// entry keeps it.
///
/// Whitespace controls become spaces, every other control character and
/// escape sequence goes (`doctor` prints the text to a terminal, and git's
/// own messages are not the only ones in there), and at most
/// [`MAX_ERROR_CHARS`] characters are kept, since the tick parses the file
/// on every render (SPEC § 5).
#[must_use]
pub fn bounded_text(text: &str) -> String {
    let spaced: String =
        text.chars().map(|c| if matches!(c, '\n' | '\r' | '\t') { ' ' } else { c }).collect();
    crate::ansi::plain_text(&spaced).chars().take(MAX_ERROR_CHARS).collect()
}

/// A cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// When it was computed, epoch milliseconds.
    pub computed_at_ms: i64,
    /// TTL the writer used, milliseconds. Informational: freshness is
    /// always judged against the reader's own TTL.
    pub ttl_ms: u64,
    /// Status.
    pub status: Status,
    /// Values (empty for `Err`).
    pub values: BTreeMap<String, String>,
    /// Error text (for `Err`).
    pub error: String,
}

impl Entry {
    /// A successful entry computed now.
    #[must_use]
    pub fn ok(ttl_ms: u64, values: BTreeMap<String, String>) -> Self {
        Self {
            computed_at_ms: now_millis(),
            ttl_ms,
            status: Status::Ok,
            values,
            error: String::new(),
        }
    }

    /// A failed entry computed now.
    ///
    /// The text is the failing command's stderr, or a message garnish
    /// built around the command's arguments (a tracking ref read from
    /// `.git/config`, any bytes at all). Every warm tick reads and parses
    /// the file and `doctor` prints it, so it is reduced by
    /// [`bounded_text`] like every other external string (SPEC § 5).
    #[must_use]
    pub fn err(ttl_ms: u64, error: impl AsRef<str>) -> Self {
        Self {
            computed_at_ms: now_millis(),
            ttl_ms,
            status: Status::Err,
            values: BTreeMap::new(),
            error: bounded_text(error.as_ref()),
        }
    }

    /// Age in milliseconds, for display; never negative.
    #[must_use]
    pub fn age_ms(&self) -> i64 {
        now_millis().saturating_sub(self.computed_at_ms).max(0)
    }

    /// Whether the entry is within `ttl_ms`.
    ///
    /// An entry stamped in the future is never fresh. The clock can step
    /// backwards (a VM resuming, NTP correcting a bad RTC), and clamping a
    /// negative age to zero would have made such an entry fresh for every
    /// TTL until the wall clock caught up — the value frozen with no `⟳`
    /// and no worker ever spawned.
    #[must_use]
    pub fn is_fresh(&self, ttl_ms: u64) -> bool {
        let age = now_millis().saturating_sub(self.computed_at_ms);
        u64::try_from(age).is_ok_and(|age| age <= ttl_ms)
    }

    /// A value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Serialize.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "v1 {} {} {}\n",
            self.computed_at_ms,
            self.ttl_ms,
            match self.status {
                Status::Ok => "ok",
                Status::Err => "err",
            }
        );
        match self.status {
            Status::Ok => {
                for (k, v) in &self.values {
                    out.push_str(k);
                    out.push('=');
                    out.push_str(&v.replace('\n', " "));
                    out.push('\n');
                }
            }
            Status::Err => {
                out.push_str(&self.error.replace('\n', " "));
                out.push('\n');
            }
        }
        out
    }

    /// Parse; `None` for anything malformed.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let header = lines.next()?;
        let mut parts = header.split(' ');
        if parts.next()? != "v1" {
            return None;
        }
        let computed_at_ms: i64 = parts.next()?.parse().ok()?;
        let ttl_ms: u64 = parts.next()?.parse().ok()?;
        let status = match parts.next()? {
            "ok" => Status::Ok,
            "err" => Status::Err,
            _ => return None,
        };
        if !text.ends_with('\n') {
            // A write in progress or a truncated file: never trust it.
            return None;
        }
        match status {
            Status::Ok => {
                let mut values = BTreeMap::new();
                for line in lines {
                    let (k, v) = line.split_once('=')?;
                    values.insert(k.to_owned(), v.to_owned());
                }
                Some(Self { computed_at_ms, ttl_ms, status, values, error: String::new() })
            }
            Status::Err => Some(Self {
                computed_at_ms,
                ttl_ms,
                status,
                values: BTreeMap::new(),
                error: lines.collect::<Vec<_>>().join(" "),
            }),
        }
    }
}

/// What a tick learns when it looks a module up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    /// The entry, if any (possibly stale or failed).
    pub entry: Option<Entry>,
    /// Whether the entry is within the TTL the caller asked about.
    pub fresh: bool,
    /// A worker currently holds the lock.
    pub in_progress: bool,
}

/// The cache root plus helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    root: PathBuf,
    /// Why the root may not be used, when it may not ([`private_root`]):
    /// nothing is then read, written or locked there, and a render spawns
    /// no worker.
    refused: Option<String>,
}

/// Outcome of trying to take a lock.
#[derive(Debug)]
pub enum LockOutcome {
    /// Acquired; dropping the guard releases it.
    Acquired(LockGuard),
    /// Another live process holds it.
    Held,
    /// The lock could not be created (I/O error); treat as held.
    Unavailable(std::io::Error),
}

/// Removes the lock file on drop.
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    armed: bool,
}

impl LockGuard {
    /// Adopt an existing lock file created by the process that spawned us,
    /// re-stamping it with our own pid and time so liveness checks track us.
    /// The stamp is written to a temporary file and renamed over the lock so
    /// no reader ever sees an empty (apparently abandoned) lock.
    #[must_use]
    pub fn adopt(path: PathBuf) -> Self {
        let tmp = path.with_extension(format!("lock.adopt.{}", std::process::id()));
        if write_fresh(&tmp, &lock_text()).is_ok() && fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
        Self { path, armed: true }
    }

    /// Whether the lock file still carries this process's pid.
    fn owned_by_us(&self) -> bool {
        read_lock(&self.path)
            .and_then(|t| t.split_whitespace().next()?.parse::<u32>().ok())
            .is_some_and(|pid| pid == std::process::id())
    }

    /// Keep the lock file after drop (hand-over to a spawned worker).
    pub const fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // A lock reclaimed by someone else while we were running (we went
        // past LOCK_STALE_MS) is theirs now; never unlink it from under them.
        if self.armed && self.owned_by_us() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Removes a temporary file on drop.
struct TmpFile(PathBuf);

impl Drop for TmpFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl Cache {
    /// A cache rooted at an explicit directory.
    #[must_use]
    pub const fn at(root: PathBuf) -> Self {
        Self { root, refused: None }
    }

    /// Resolve the root from the environment, highest precedence first:
    /// `GARNISH_CACHE_DIR`, `$XDG_RUNTIME_DIR/garnish`,
    /// `$XDG_CACHE_HOME/garnish`, `~/.cache/garnish` (macOS:
    /// `~/Library/Caches/garnish`), then a private directory under the
    /// temp directory (`garnish-<uid>`, `0700`), which is refused rather than
    /// shared.
    #[must_use]
    pub fn from_env() -> Self {
        root_from(crate::config::env_path, cfg!(target_os = "macos")).map_or_else(
            || match private_root(&std::env::temp_dir()) {
                Ok(root) => Self::at(root),
                Err((root, why)) => Self { root, refused: Some(why) },
            },
            Self::at,
        )
    }

    /// The root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Why the root may not be used, if it may not: then nothing is read,
    /// written or locked under it, and renders spawn no worker.
    #[must_use]
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// The error every write and lock returns under a refused root.
    fn refusal(&self) -> Option<std::io::Error> {
        self.refused.as_ref().map(|why| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("cache root {} refused: {why}", self.root.display()),
            )
        })
    }

    /// Path of an entry file.
    #[must_use]
    pub fn entry_path(&self, scope: &Scope, module: &str) -> PathBuf {
        scope.dir(&self.root).join(format!("{}.cache", sanitize(module)))
    }

    /// Path of a lock file.
    #[must_use]
    pub fn lock_path(&self, scope: &Scope, module: &str) -> PathBuf {
        scope.dir(&self.root).join(format!("{}.lock", sanitize(module)))
    }

    /// Read an entry (miss on absence or malformation).
    #[must_use]
    pub fn read(&self, scope: &Scope, module: &str) -> Option<Entry> {
        if self.refused.is_some() {
            return None;
        }
        read_entry(&self.entry_path(scope, module))
    }

    /// Look a module up: entry, freshness against `ttl_ms`, and lock state.
    /// A failed entry is fresh for its TTL like any other, so a persistent
    /// failure is retried once per TTL rather than once per tick. The lock
    /// is read only for an entry that is not fresh: a fresh one spawns
    /// nothing whoever holds it.
    #[must_use]
    pub fn lookup(&self, scope: &Scope, module: &str, ttl_ms: u64) -> Lookup {
        let entry = self.read(scope, module);
        let fresh = entry.as_ref().is_some_and(|e| e.is_fresh(ttl_ms));
        let in_progress = !fresh && self.lock_is_live(&self.lock_path(scope, module));
        Lookup { entry, fresh, in_progress }
    }

    /// Write an entry atomically. Creates the scope directory on demand. An
    /// entry whose file would not read back as the same entry is stored as
    /// a failure saying why (`stored_text`).
    ///
    /// The first entry a module writes in a scope also runs the bounded
    /// sweep (SPEC § 6). Only workers write entries, so this keeps the
    /// sweep off the tick; and it is the *entry* that is new, not the
    /// directory, which the lock (taken first, by the tick on Linux) has
    /// always created already, so a sweep keyed on it never ran.
    ///
    /// # Errors
    /// Propagates I/O errors, and refuses under a refused root.
    pub fn write(&self, scope: &Scope, module: &str, entry: &Entry) -> std::io::Result<()> {
        if let Some(e) = self.refusal() {
            return Err(e);
        }
        let path = self.entry_path(scope, module);
        let dir = path.parent().map_or_else(|| self.root.clone(), Path::to_path_buf);
        let first = !path.exists();
        create_private_dir(&dir)?;
        let tmp = dir.join(format!(".{}.tmp.{}", sanitize(module), std::process::id()));
        // Removed on every failure path, as `lock` and `install::replace_file`
        // do: otherwise a full disk leaves one temp file per failed refresh,
        // and only a first entry's sweep or `garnish gc` removes them.
        let _cleanup = TmpFile(tmp.clone());
        {
            let mut f = create_fresh(&tmp)?;
            f.write_all(stored_text(entry).as_bytes())?;
            f.sync_data().ok();
        }
        fs::rename(&tmp, &path)?;
        if first {
            self.gc_sessions(GC_MAX_AGE_MS, GC_MAX_PER_SWEEP);
        }
        Ok(())
    }

    /// Try to take the lock for a module.
    #[must_use]
    pub fn lock(&self, scope: &Scope, module: &str) -> LockOutcome {
        if let Some(e) = self.refusal() {
            return LockOutcome::Unavailable(e);
        }
        let path = self.lock_path(scope, module);
        if let Some(dir) = path.parent()
            && let Err(e) = create_private_dir(dir)
        {
            return LockOutcome::Unavailable(e);
        }
        // Write the content first, then link it into place: `hard_link` fails
        // with AlreadyExists when the lock exists, and a reader never sees an
        // empty lock file (which would look abandoned and get reclaimed).
        let tmp = path.with_extension(format!("lock.tmp.{}", std::process::id()));
        let _cleanup = TmpFile(tmp.clone());
        if let Err(e) = write_fresh(&tmp, &lock_text()) {
            return LockOutcome::Unavailable(e);
        }
        for attempt in 0..2 {
            match fs::hard_link(&tmp, &path) {
                Ok(()) => return LockOutcome::Acquired(LockGuard { path, armed: true }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                    match read_lock(&path) {
                        // Gone meanwhile: its holder let go, so link again.
                        None if !path.exists() => {}
                        Some(seen) if !is_live(&seen) => {
                            if !reclaim(&path, &seen) {
                                return LockOutcome::Held;
                            }
                        }
                        _ => return LockOutcome::Held,
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return LockOutcome::Held;
                }
                Err(e) => return LockOutcome::Unavailable(e),
            }
        }
        LockOutcome::Held
    }

    /// Whether a lock file exists and belongs to a live, recent process: one
    /// younger than [`LOCK_GRACE_MS`] (the hand-over window), else one whose
    /// pid exists (Linux) and that is younger than [`LOCK_STALE_MS`].
    #[must_use]
    pub fn lock_is_live(&self, path: &Path) -> bool {
        read_lock(path).is_some_and(|text| is_live(&text))
    }

    /// Remove session and repo directories whose newest file is older than
    /// `max_age_ms`, plus temporary lock/entry files left behind by killed
    /// processes. Returns how many directories were removed (at most `max`).
    /// File mtimes are wall clock, so this compares against the real clock
    /// even under `GARNISH_NOW`.
    ///
    /// The root may be a directory garnish shares with others
    /// (`GARNISH_CACHE_DIR=~/.cache`), so only what garnish would have made
    /// is touched: `sessions` and `repos` themselves and each directory in
    /// them as real directories, never through a link; a repo directory
    /// only by its hash's shape, a session one only by a session id's; and
    /// either only when every file in it has one of garnish's own names.
    pub fn gc_sessions(&self, max_age_ms: i64, max: usize) -> usize {
        if self.refused.is_some() {
            return 0;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(i64::MAX);
        let mut removed = 0;
        let dirs = ["sessions", "repos"]
            .into_iter()
            .filter(|kind| fs::symlink_metadata(self.root.join(kind)).is_ok_and(|m| m.is_dir()))
            .filter_map(|kind| Some((kind, fs::read_dir(self.root.join(kind)).ok()?)))
            .flat_map(|(kind, dirs)| dirs.filter_map(Result::ok).map(move |d| (kind, d)));
        for (kind, entry) in dirs {
            if removed >= max {
                break;
            }
            let path = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) || !ours(kind, &entry.file_name()) {
                continue;
            }
            let Ok(files) = fs::read_dir(&path) else { continue };
            let files: Vec<fs::DirEntry> = files.filter_map(Result::ok).collect();
            if !files.iter().all(|f| is_cache_file(&f.file_name())) {
                continue;
            }
            sweep_temp_files(&files, now);
            let newest = files
                .iter()
                .filter_map(|f| f.metadata().ok())
                .map(|m| mtime_ms(&m))
                .max()
                .or_else(|| entry.metadata().ok().map(|m| mtime_ms(&m)));
            let idle = newest.map_or(i64::MAX, |n| now.saturating_sub(n));
            if idle > max_age_ms && fs::remove_dir_all(&path).is_ok() {
                removed = removed.saturating_add(1);
            }
        }
        removed
    }
}

/// The file [`Cache::write`] stores for `entry`: its text when that reads
/// back as the same entry, else a failed entry saying why.
///
/// A value holding a line break is written with a space, a trailing
/// carriage return is lost to the reader's line split, and a file past
/// [`MAX_ENTRY_BYTES`] reads as a miss. Stored as it came, such an entry is
/// never the one a render's check asks for, or never read at all, and
/// every tick spawns another worker; a failed entry is fresh for its TTL.
/// The workers keep their values within these bounds, so this is the belt,
/// not the rule.
fn stored_text(entry: &Entry) -> String {
    let text = entry.to_text();
    let fits = u64::try_from(text.len()).is_ok_and(|n| n <= MAX_ENTRY_BYTES);
    let back = Entry::parse(&text);
    if fits && back.as_ref() == Some(entry) {
        return text;
    }
    // A failure's own text, bounded by `Entry::err`, always reads back.
    let why = match (entry.status, fits) {
        (Status::Err, _) => entry.error.clone(),
        (Status::Ok, false) => {
            format!("the refresh's values are past the {MAX_ENTRY_BYTES} bytes of an entry")
        }
        (Status::Ok, true) => {
            let changed = entry
                .values
                .iter()
                .find(|(k, v)| back.as_ref().and_then(|b| b.get(k)) != Some(v.as_str()));
            changed.map_or_else(
                || "the refresh's values cannot be stored as they are".to_owned(),
                |(k, _)| format!("{k}: a value the cache entry cannot keep as it is"),
            )
        }
    };
    Entry { computed_at_ms: entry.computed_at_ms, ..Entry::err(entry.ttl_ms, why) }.to_text()
}

/// An entry file read and parsed: a miss when it is absent, not a regular
/// file (a FIFO would block the tick in `open`), longer than 64 KiB, or
/// malformed.
#[must_use]
pub fn read_entry(path: &Path) -> Option<Entry> {
    let bytes = crate::claude_settings::read_regular(path, MAX_ENTRY_BYTES.saturating_add(1))
        .ok()
        .flatten()?;
    if u64::try_from(bytes.len()).is_ok_and(|n| n > MAX_ENTRY_BYTES) {
        return None;
    }
    Entry::parse(&String::from_utf8(bytes).ok()?)
}

/// A lock file's text, read like an entry: `None` for anything that is
/// not a short regular file.
fn read_lock(path: &Path) -> Option<String> {
    let bytes = crate::claude_settings::read_regular(path, MAX_LOCK_BYTES).ok().flatten()?;
    String::from_utf8(bytes).ok()
}

/// What a lock file says: this process and now.
fn lock_text() -> String {
    format!("{} {}\n", std::process::id(), now_millis())
}

/// [`create_fresh`] and write `text` into it.
fn write_fresh(path: &Path, text: &str) -> std::io::Result<()> {
    create_fresh(path)?.write_all(text.as_bytes())
}

/// Whether a lock's text names a live, recent holder.
///
/// A lock younger than [`LOCK_GRACE_MS`] is always live: a tick writes the
/// lock with its own pid, exits, and the spawned worker re-stamps it with
/// the worker's pid a few milliseconds later. Without the grace window the
/// lock would look dead in between and a second worker would be spawned.
fn is_live(text: &str) -> bool {
    let mut parts = text.split_whitespace();
    let pid: Option<u32> = parts.next().and_then(|p| p.parse().ok());
    let stamp: Option<i64> = parts.next().and_then(|p| p.parse().ok());
    let age = stamp.map_or(i64::MAX, |s| now_millis().saturating_sub(s));
    // A stamp in the future is a clock that stepped backwards, not a
    // live lock: without this the negative age passes the staleness
    // check and then satisfies the grace window, so the lock reads live
    // for ever and the module is never refreshed again.
    if !(0..=LOCK_STALE_MS).contains(&age) {
        return false;
    }
    if age <= LOCK_GRACE_MS {
        return true;
    }
    match pid {
        Some(p) if cfg!(target_os = "linux") => Path::new("/proc").join(p.to_string()).exists(),
        Some(_) => true,
        None => false,
    }
}

/// Move a lock judged dead out of the way, unless it changed since: `seen`
/// is the text that was judged.
///
/// Judging and moving are two steps, so two processes can both judge the
/// same stale lock dead; the first moves it and links its own, and the
/// second's rename would then take that *fresh* lock. So the moved file is
/// read back, and one that is not what was judged goes back where it was
/// (a third process that linked meanwhile keeps its own, and this link
/// fails harmlessly). At most one process wins each reclaim.
fn reclaim(path: &Path, seen: &str) -> bool {
    let stale = path.with_extension(format!("stale.{}", std::process::id()));
    if fs::rename(path, &stale).is_err() {
        return false;
    }
    let moved = read_lock(&stale);
    let ours = moved.as_deref() == Some(seen);
    if !ours {
        let _ = fs::hard_link(&stale, path);
    }
    let _ = fs::remove_file(&stale);
    ours
}

/// A file's mtime in epoch milliseconds, `0` when there is none to read
/// (so an unreadable time never makes a directory look recent).
fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Whether a directory under `sessions` or `repos` has a name garnish
/// gives one: a sanitised session id, or a [`key_hash`].
fn ours(kind: &str, name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else { return false };
    match kind {
        "repos" => name.len() == 16 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        _ => {
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }
    }
}

/// Whether a file name is one garnish writes in a scope directory: an
/// entry, a lock, or one of their temporary names.
fn is_cache_file(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.ends_with(".cache") || name.ends_with(".lock") || is_temp_name(&name)
}

/// The temporary names of an entry write, a lock and its reclaim and hand-over.
fn is_temp_name(name: &str) -> bool {
    [".tmp.", ".stale.", ".adopt."].iter().any(|m| name.contains(m))
}

/// Temporary files older than this are leftovers of a killed process.
const TEMP_FILE_MAX_AGE_MS: i64 = 60 * 60 * 1000;

/// Delete the `*.tmp.*`, `*.stale.*` and `*.adopt.*` files among `files`
/// that are older than an hour.
fn sweep_temp_files(files: &[fs::DirEntry], now_ms: i64) {
    for f in files {
        let age = f.metadata().map_or(0, |m| now_ms.saturating_sub(mtime_ms(&m)));
        if is_temp_name(&f.file_name().to_string_lossy()) && age > TEMP_FILE_MAX_AGE_MS {
            let _ = fs::remove_file(f.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, Cache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().to_path_buf());
        (dir, cache)
    }

    #[test]
    fn entry_round_trips_and_rejects_garbage() {
        let mut values = BTreeMap::new();
        values.insert("branch".to_owned(), "main".to_owned());
        values.insert("note".to_owned(), "two\nlines".to_owned());
        let e = Entry {
            computed_at_ms: 5,
            ttl_ms: 7,
            status: Status::Ok,
            values,
            error: String::new(),
        };
        let text = e.to_text();
        assert_eq!(text, "v1 5 7 ok\nbranch=main\nnote=two lines\n");
        let back = Entry::parse(&text).unwrap();
        assert_eq!(back.get("branch"), Some("main"));
        assert_eq!(back.get("note"), Some("two lines"));
        let err = Entry {
            computed_at_ms: 1,
            ttl_ms: 2,
            status: Status::Err,
            values: BTreeMap::new(),
            error: "boom".into(),
        };
        assert_eq!(Entry::parse(&err.to_text()).unwrap().error, "boom");
        for bad in [
            "",
            "v2 1 2 ok\n",
            "v1 x 2 ok\n",
            "v1 1 2 maybe\n",
            "v1 1 2 ok\nnoequals\n",
            "v1 1 2 ok\nk=v",
        ] {
            assert!(Entry::parse(bad).is_none(), "{bad:?}");
        }
    }

    /// SPEC § 5: every string from outside is bounded. A failed refresh
    /// carries the command's whole stderr, which every later tick reads and
    /// parses until the TTL passes — a `git` wrapper with a long policy
    /// message used to leave hundreds of kilobytes in the entry.
    #[test]
    fn a_failed_entry_bounds_the_error_text() {
        let long = "é".repeat(MAX_ERROR_CHARS * 3);
        let entry = Entry::err(1000, long.clone());
        assert_eq!(entry.error.chars().count(), MAX_ERROR_CHARS);
        assert!(long.starts_with(&entry.error), "the head is kept");
        assert_eq!(Entry::err(1000, "short").error, "short");
        // The cut survives the file round trip.
        let (_d, cache) = temp();
        let scope = Scope::Session("s".into());
        cache.write(&scope, "m", &entry).unwrap();
        let back = cache.read(&scope, "m").unwrap();
        assert_eq!(back.error.chars().count(), MAX_ERROR_CHARS);
    }

    /// An entry is stored only as a file that reads back as the same entry.
    /// A value holding a line break was written with a space and one past
    /// the entry's cap was read as a miss, so the tick never found the
    /// entry the worker meant and spawned another worker on every render
    /// (review 2026-09-25); such a refresh is stored as a failure, which is
    /// fresh for its TTL like any other.
    #[test]
    fn cache_a_value_the_entry_cannot_carry_is_stored_as_a_failure() {
        let (_d, cache) = temp();
        let scope = Scope::Repo("0123456789abcdef".into());
        let stored = |key: &str, value: String| {
            let entry = Entry::ok(5_000, [(key.to_owned(), value)].into());
            cache.write(&scope, "m", &entry).unwrap();
            let back = cache.read(&scope, "m").expect("the file reads back");
            assert_eq!(back.computed_at_ms, entry.computed_at_ms);
            back
        };
        for bad in ["ma\nin", "main\r", "\r\n"] {
            let back = stored("upstream", bad.to_owned());
            assert_eq!(back.status, Status::Err, "{bad:?}");
            assert!(back.error.starts_with("upstream: "), "{}", back.error);
        }
        let limit = usize::try_from(MAX_ENTRY_BYTES).unwrap();
        let back = stored("head", "a".repeat(limit));
        assert_eq!(back.status, Status::Err);
        assert!(back.error.contains("bytes"), "{}", back.error);
        // What does round-trip is stored as it is.
        let back = stored("upstream", "ma\rin \u{1} é".to_owned());
        assert_eq!((back.status, back.get("upstream")), (Status::Ok, Some("ma\rin \u{1} é")));
        let back = stored("head", "a".repeat(limit / 2));
        assert_eq!(back.status, Status::Ok);
        // A failure whose text would not read back is stored bounded.
        let raw = Entry {
            computed_at_ms: 3,
            ttl_ms: 4,
            status: Status::Err,
            values: BTreeMap::new(),
            error: "two\nlines\r".to_owned(),
        };
        cache.write(&scope, "m", &raw).unwrap();
        let back = cache.read(&scope, "m").unwrap();
        assert_eq!((back.status, back.error.as_str()), (Status::Err, "two lines "));
    }

    /// A stamp in the future is a clock that stepped backwards, not a very
    /// recent one: a negative age clamped to zero made such an entry fresh
    /// for every TTL and such a lock live for ever.
    #[test]
    fn a_future_stamp_is_neither_fresh_nor_live() {
        let ahead = now_millis().saturating_add(60 * 60 * 1000);
        let entry = Entry {
            computed_at_ms: ahead,
            ttl_ms: 1000,
            status: Status::Ok,
            values: BTreeMap::new(),
            error: String::new(),
        };
        assert!(!entry.is_fresh(1000));
        assert!(!entry.is_fresh(u64::MAX));
        assert_eq!(entry.age_ms(), 0, "the display age stays non-negative");
        let (_d, cache) = temp();
        let path = cache.root().join("m.lock");
        std::fs::create_dir_all(cache.root()).unwrap();
        std::fs::write(&path, format!("{} {ahead}\n", std::process::id())).unwrap();
        assert!(!cache.lock_is_live(&path));
        std::fs::write(&path, format!("{} {}\n", std::process::id(), now_millis())).unwrap();
        assert!(cache.lock_is_live(&path));
    }

    #[test]
    fn write_read_lookup_and_ttl() {
        let (_d, cache) = temp();
        let scope = Scope::Session("s1".into());
        assert!(cache.read(&scope, "m").is_none());
        let mut values = BTreeMap::new();
        values.insert("k".to_owned(), "v".to_owned());
        cache.write(&scope, "m", &Entry::ok(5_000, values)).unwrap();
        let l = cache.lookup(&scope, "m", 5_000);
        assert!(l.fresh && !l.in_progress);
        assert_eq!(l.entry.unwrap().get("k"), Some("v"));
        // an entry from long ago is stale
        let old = Entry {
            computed_at_ms: now_millis() - 10_000,
            ttl_ms: 5_000,
            status: Status::Ok,
            values: BTreeMap::new(),
            error: String::new(),
        };
        cache.write(&scope, "m", &old).unwrap();
        assert!(!cache.lookup(&scope, "m", 5_000).fresh);
        // an err entry is fresh for its TTL like any other (no retry storm)
        cache.write(&scope, "m", &Entry::err(5_000, "nope")).unwrap();
        let l = cache.lookup(&scope, "m", 5_000);
        assert!(l.fresh);
        assert_eq!(l.entry.unwrap().status, Status::Err);
        // no stray temp files
        let names: Vec<String> = fs::read_dir(cache.entry_path(&scope, "m").parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["m.cache".to_owned()]);
    }

    #[test]
    fn truncated_files_are_misses() {
        let (_d, cache) = temp();
        let scope = Scope::Repo("abc".into());
        let path = cache.entry_path(&scope, "git");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "v1 1 2 ok\nk=v").unwrap();
        assert!(cache.read(&scope, "git").is_none());
        fs::write(&path, "").unwrap();
        assert!(cache.read(&scope, "git").is_none());
    }

    #[test]
    fn locks_are_exclusive_and_stale_locks_are_reclaimed() {
        let (_d, cache) = temp();
        let scope = Scope::Session("s".into());
        let guard = match cache.lock(&scope, "m") {
            LockOutcome::Acquired(g) => g,
            other => panic!("{other:?}"),
        };
        assert!(matches!(cache.lock(&scope, "m"), LockOutcome::Held));
        assert!(cache.lookup(&scope, "m", 1).in_progress);
        drop(guard);
        assert!(!cache.lock_path(&scope, "m").exists());
        assert!(!cache.lookup(&scope, "m", 1).in_progress);
        // stale by age
        fs::write(
            cache.lock_path(&scope, "m"),
            format!("{} {}", std::process::id(), now_millis() - LOCK_STALE_MS - 1),
        )
        .unwrap();
        assert!(matches!(cache.lock(&scope, "m"), LockOutcome::Acquired(_)));
        // dead pid (Linux) once the grace window has passed
        if cfg!(target_os = "linux") {
            let past_grace = now_millis() - LOCK_GRACE_MS - 1;
            fs::write(cache.lock_path(&scope, "m"), format!("4000000000 {past_grace}")).unwrap();
            assert!(!cache.lock_is_live(&cache.lock_path(&scope, "m")));
            assert!(matches!(cache.lock(&scope, "m"), LockOutcome::Acquired(_)));
            // a dead pid inside the grace window is still trusted (hand-over)
            fs::write(cache.lock_path(&scope, "m"), format!("4000000000 {}", now_millis()))
                .unwrap();
            assert!(cache.lock_is_live(&cache.lock_path(&scope, "m")));
            assert!(matches!(cache.lock(&scope, "m"), LockOutcome::Held));
            fs::remove_file(cache.lock_path(&scope, "m")).unwrap();
        }
        // adopting re-stamps the file with our pid (via rename, never truncating in place)
        fs::write(cache.lock_path(&scope, "m"), "1 1").unwrap();
        let g = LockGuard::adopt(cache.lock_path(&scope, "m"));
        let stamped = fs::read_to_string(cache.lock_path(&scope, "m")).unwrap();
        assert!(stamped.starts_with(&format!("{} ", std::process::id())), "{stamped}");
        assert!(cache.lock_is_live(&cache.lock_path(&scope, "m")));
        assert!(
            fs::read_dir(cache.lock_path(&scope, "m").parent().unwrap())
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().contains("adopt"))
        );
        drop(g);
        assert!(!cache.lock_path(&scope, "m").exists());
        // a guard never unlinks a lock that another process has since taken over
        let g = match cache.lock(&scope, "m") {
            LockOutcome::Acquired(g) => g,
            other => panic!("{other:?}"),
        };
        fs::write(cache.lock_path(&scope, "m"), format!("4000000001 {}", now_millis())).unwrap();
        drop(g);
        assert!(cache.lock_path(&scope, "m").exists());
        fs::remove_file(cache.lock_path(&scope, "m")).unwrap();
        // disarmed guard keeps the file
        let mut g = match cache.lock(&scope, "m") {
            LockOutcome::Acquired(g) => g,
            other => panic!("{other:?}"),
        };
        g.disarm();
        drop(g);
        assert!(cache.lock_path(&scope, "m").exists());
    }

    #[test]
    fn gc_sweeps_idle_sessions_within_bounds() {
        let (_d, cache) = temp();
        // A live session first: creating it must not be swept later.
        cache.write(&Scope::Session("fresh".into()), "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        for i in 0..5 {
            let scope = Scope::Session(format!("old{i}"));
            cache.write(&scope, "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        }
        // Age them only after all exist: a first entry sweeps idle ones.
        let old = std::time::SystemTime::now() - std::time::Duration::from_hours(48);
        for i in 0..5 {
            let dir = cache
                .entry_path(&Scope::Session(format!("old{i}")), "m")
                .parent()
                .unwrap()
                .to_path_buf();
            for f in fs::read_dir(&dir).unwrap() {
                fs::File::options()
                    .write(true)
                    .open(f.unwrap().path())
                    .unwrap()
                    .set_modified(old)
                    .unwrap();
            }
        }
        assert_eq!(cache.gc_sessions(GC_MAX_AGE_MS, 2), 2);
        assert_eq!(cache.gc_sessions(GC_MAX_AGE_MS, 50), 3);
        assert!(cache.entry_path(&Scope::Session("fresh".into()), "m").exists());
        assert_eq!(cache.gc_sessions(GC_MAX_AGE_MS, 50), 0);
        // A brand-new session's first entry sweeps idle ones automatically.
        let scope = Scope::Session("stale".into());
        cache.write(&scope, "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_hours(48);
        for f in fs::read_dir(cache.entry_path(&scope, "m").parent().unwrap()).unwrap() {
            fs::File::options()
                .write(true)
                .open(f.unwrap().path())
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        cache.write(&Scope::Session("newer".into()), "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        assert!(!cache.entry_path(&scope, "m").exists());
        // repo dirs are swept too, and stale temp files inside live dirs go away
        let repo = Scope::Repo(key_hash(&["common", "git"]));
        cache.write(&repo, "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        let dir = cache.entry_path(&repo, "m").parent().unwrap().to_path_buf();
        let leftover = dir.join(".m.tmp.999");
        fs::write(&leftover, "x").unwrap();
        fs::File::options().write(true).open(&leftover).unwrap().set_modified(old).unwrap();
        assert_eq!(cache.gc_sessions(GC_MAX_AGE_MS, 50), 0);
        assert!(!leftover.exists() && cache.entry_path(&repo, "m").exists());
        for f in fs::read_dir(&dir).unwrap().flatten() {
            fs::File::options().write(true).open(f.path()).unwrap().set_modified(old).unwrap();
        }
        assert_eq!(cache.gc_sessions(GC_MAX_AGE_MS, 50), 1);
        assert!(!dir.exists());
    }

    #[test]
    fn key_hash_and_sanitize_are_stable() {
        assert_eq!(key_hash(&["a", "b"]), key_hash(&["a", "b"]));
        assert_ne!(key_hash(&["a", "b"]), key_hash(&["ab"]));
        assert_eq!(sanitize("../x y"), "___x_y");
        assert_eq!(sanitize(""), "_");
    }

    /// SPEC § 6: the documented order, and both platforms' home arm. The
    /// test named for this used to assert `key_hash` and `sanitize` instead,
    /// so the chain itself had no coverage at all.
    #[test]
    fn root_resolution_follows_the_documented_order() {
        let set = |pairs: &[(&str, &str)]| {
            let owned: Vec<(String, PathBuf)> =
                pairs.iter().map(|(k, v)| ((*k).to_owned(), PathBuf::from(v))).collect();
            move |k: &str| owned.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone())
        };
        let all = [
            (CACHE_DIR_ENV, "/explicit"),
            ("XDG_RUNTIME_DIR", "/run/u"),
            ("XDG_CACHE_HOME", "/xdg"),
            ("HOME", "/home/d"),
        ];
        let path = |p: &str| Some(PathBuf::from(p));
        assert_eq!(root_from(set(&all), false), path("/explicit"));
        assert_eq!(root_from(set(&all[1..]), false), path("/run/u/garnish"));
        assert_eq!(root_from(set(&all[2..]), false), path("/xdg/garnish"));
        assert_eq!(root_from(set(&all[3..]), false), path("/home/d/.cache/garnish"));
        assert_eq!(
            root_from(set(&all[3..]), true),
            path("/home/d/Library/Caches/garnish"),
            "macOS puts it under Library/Caches"
        );
        // Nothing set at all: no root named, so the private temp one.
        assert_eq!(root_from(set(&[]), false), None);
        // A relative XDG base is ignored (the XDG spec calls it invalid): it
        // would put the cache in whatever repository the tick runs in.
        let relative = [("XDG_RUNTIME_DIR", "run"), ("XDG_CACHE_HOME", "rel"), ("HOME", "/home/d")];
        assert_eq!(root_from(set(&relative), false), path("/home/d/.cache/garnish"));
        assert_eq!(root_from(set(&relative[1..]), false), path("/home/d/.cache/garnish"));
    }

    /// The last-resort root sits in the temp directory every user shares, so
    /// it is per user, created `0700`, and refused (no cache at all) when it
    /// is a link, is writable by others, or belongs to someone else: a root
    /// another user made first let them read and plant entries and aim the
    /// sweep and the temp-file writes at this user's files.
    #[test]
    fn cache_the_temp_root_is_private_or_refused() {
        use std::os::unix::fs::PermissionsExt as _;
        let base = tempfile::tempdir().unwrap();
        let root = private_root(base.path()).unwrap();
        let uid = fs::metadata(&root).unwrap().uid();
        assert_eq!(root, base.path().join(format!("garnish-{uid}")));
        assert_eq!(fs::metadata(&root).unwrap().mode() & 0o777, 0o700);
        assert_eq!(private_root(base.path()), Ok(root.clone()), "a second call reuses it");
        assert!(fs::read_dir(base.path()).unwrap().count() == 1, "the uid probe is gone");

        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(private_root(base.path()).is_err(), "writable by others");
        fs::remove_dir(&root).unwrap();
        let elsewhere = base.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &root).unwrap();
        assert!(private_root(base.path()).is_err(), "a link");
        fs::remove_file(&root).unwrap();
        // Only root can hand a directory to another user.
        if uid == 0 {
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            std::os::unix::fs::chown(&root, Some(65534), None).unwrap();
            let (_, why) = private_root(base.path()).unwrap_err();
            assert!(why.contains("owned by uid 65534"), "{why}");
        }

        // A refused root is no cache: no entry, no write, no lock, no sweep.
        let refused = Cache { root, refused: Some("test".into()) };
        let scope = Scope::Session("s".into());
        assert!(refused.write(&scope, "m", &Entry::ok(1, BTreeMap::new())).is_err());
        assert!(matches!(refused.lock(&scope, "m"), LockOutcome::Unavailable(_)));
        assert!(refused.read(&scope, "m").is_none());
        assert_eq!(refused.gc_sessions(0, 50), 0);
    }

    /// Temp files have predictable names, so a link planted at one must not
    /// aim the write at another file: what is there is unlinked, and the
    /// file is created afresh.
    #[test]
    fn cache_a_planted_link_at_a_temp_name_is_never_followed() {
        let (d, cache) = temp();
        let scope = Scope::Repo("0123456789abcdef".into());
        let dir = cache.entry_path(&scope, "m").parent().unwrap().to_path_buf();
        fs::create_dir_all(&dir).unwrap();
        let precious = d.path().join("precious");
        fs::write(&precious, "keep me").unwrap();
        let pid = std::process::id();
        for name in [format!(".m.tmp.{pid}"), format!("m.lock.tmp.{pid}")] {
            std::os::unix::fs::symlink(&precious, dir.join(name)).unwrap();
        }
        cache.write(&scope, "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        let guard = match cache.lock(&scope, "m") {
            LockOutcome::Acquired(g) => g,
            other => panic!("{other:?}"),
        };
        std::os::unix::fs::symlink(&precious, dir.join(format!("m.lock.adopt.{pid}"))).unwrap();
        drop(LockGuard::adopt(cache.lock_path(&scope, "m")));
        drop(guard);
        assert_eq!(fs::read_to_string(&precious).unwrap(), "keep me");
        assert!(cache.read(&scope, "m").is_some());
        let mode = fs::metadata(cache.entry_path(&scope, "m")).unwrap().mode();
        assert_eq!(mode & 0o077, 0, "entries are the user's alone: {mode:o}");
    }

    /// Every file the tick reads goes through a bounded regular-file read:
    /// a FIFO at an entry or a lock (a shared root, a planted file) would
    /// block the tick in `open`, and an entry past the cap is no entry.
    #[test]
    fn cache_a_fifo_or_an_oversized_entry_is_a_miss() {
        let (_d, cache) = temp();
        let scope = Scope::Session("s".into());
        let entry = cache.entry_path(&scope, "m");
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        if crate::claude_settings::tests::fifo(&entry).is_none() {
            return;
        }
        crate::claude_settings::tests::fifo(&cache.lock_path(&scope, "m")).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let c = cache.clone();
        let s = scope.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.lookup(&s, "m", 1000));
        });
        let l = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("lookup blocked");
        assert_eq!(l, Lookup { entry: None, fresh: false, in_progress: false });
        assert!(matches!(cache.lock(&scope, "m"), LockOutcome::Held), "never reclaimed");
        fs::remove_file(&entry).unwrap();
        let big = format!("v1 {} 1000 ok\nk={}\n", now_millis(), "x".repeat(70_000));
        fs::write(&entry, big).unwrap();
        assert!(cache.read(&scope, "m").is_none());
    }

    /// Judging a lock dead and moving it are two steps, so a second process
    /// that judged the same stale lock could move the first one's *fresh*
    /// lock and both would run. A moved lock that is not what was judged
    /// goes back.
    #[test]
    fn cache_a_reclaim_never_takes_a_lock_that_changed_since_it_was_judged() {
        let (_d, cache) = temp();
        let scope = Scope::Session("s".into());
        let path = cache.lock_path(&scope, "m");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "1 2\n").unwrap();
        assert!(!reclaim(&path, "4000000000 1\n"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "1 2\n", "put back");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1, "no stale left");
        assert!(reclaim(&path, "1 2\n"));
        assert!(!path.exists());
    }

    /// The sweep touches only what garnish would have made: a root shared
    /// with other tools (`GARNISH_CACHE_DIR=~/.cache`) keeps their
    /// `repos/<x>` trees, and a link never leads the sweep elsewhere.
    #[test]
    fn gc_never_sweeps_what_garnish_did_not_make() {
        let (d, cache) = temp();
        let old = std::time::SystemTime::now() - std::time::Duration::from_hours(48);
        let age = |p: &Path| {
            fs::File::options().write(true).open(p).unwrap().set_modified(old).unwrap();
        };
        let repos = cache.root().join("repos");
        fs::create_dir_all(repos.join("other")).unwrap();
        fs::write(repos.join("other/notes.txt"), "x").unwrap();
        age(&repos.join("other/notes.txt"));
        // A hash-shaped directory holding a file garnish never writes.
        fs::create_dir_all(repos.join("00000000000000aa")).unwrap();
        fs::write(repos.join("00000000000000aa/notes.txt"), "x").unwrap();
        age(&repos.join("00000000000000aa/notes.txt"));
        // A hash-shaped link to a directory with an aged temp file in it.
        let target = d.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("x.tmp.1"), "x").unwrap();
        age(&target.join("x.tmp.1"));
        std::os::unix::fs::symlink(&target, repos.join("0123456789abcdef")).unwrap();
        // `sessions` itself a link to a directory with an aged session.
        let elsewhere = d.path().join("elsewhere");
        fs::create_dir_all(elsewhere.join("victim")).unwrap();
        fs::write(elsewhere.join("victim/m.cache"), "x").unwrap();
        age(&elsewhere.join("victim/m.cache"));
        std::os::unix::fs::symlink(&elsewhere, cache.root().join("sessions")).unwrap();

        assert_eq!(cache.gc_sessions(GC_MAX_AGE_MS, 50), 0);
        assert!(repos.join("other/notes.txt").exists());
        assert!(repos.join("00000000000000aa/notes.txt").exists());
        assert!(target.join("x.tmp.1").exists());
        assert!(elsewhere.join("victim/m.cache").exists());
    }

    /// SPEC § 6's automatic sweep, in production order: the lock is taken
    /// first (by the tick on Linux, by the worker elsewhere), and taking
    /// it creates the scope directory, so a sweep keyed on a new
    /// *directory* never ran for any scope. It is keyed on the first
    /// *entry* instead, which only a worker writes, for either scope.
    #[test]
    fn gc_runs_when_a_worker_writes_a_scope_s_first_entry() {
        let (_d, cache) = temp();
        let old = std::time::SystemTime::now() - std::time::Duration::from_hours(48);
        // Laid out by hand, so making them sweeps nothing.
        let aged = |scope: &Scope| {
            let entry = cache.entry_path(scope, "m");
            fs::create_dir_all(entry.parent().unwrap()).unwrap();
            fs::write(&entry, Entry::ok(1, BTreeMap::new()).to_text()).unwrap();
            fs::File::options().write(true).open(&entry).unwrap().set_modified(old).unwrap();
            entry.parent().unwrap().to_path_buf()
        };
        let idle_repo = aged(&Scope::Repo(key_hash(&["idle"])));
        let idle_session = aged(&Scope::Session("idle".into()));
        for (i, scope) in
            [Scope::Repo(key_hash(&["new"])), Scope::Session("new".into())].iter().enumerate()
        {
            let _guard = match cache.lock(scope, "m") {
                LockOutcome::Acquired(g) => g,
                other => panic!("{other:?}"),
            };
            cache.write(scope, "m", &Entry::ok(1, BTreeMap::new())).unwrap();
            let gone = [&idle_repo, &idle_session][i];
            assert!(!gone.exists(), "{} survived the first entry of {scope:?}", gone.display());
        }
        // A rewrite of an existing entry does not sweep again.
        let idle_again = aged(&Scope::Session("idle-again".into()));
        cache.write(&Scope::Session("new".into()), "m", &Entry::ok(1, BTreeMap::new())).unwrap();
        assert!(idle_again.exists());
    }

    /// Text from outside reaches `doctor`'s terminal: an escape sequence or
    /// a bell in a failed entry must not survive the file.
    #[test]
    fn cache_error_text_is_plain() {
        let (_d, cache) = temp();
        let scope = Scope::Session("s".into());
        let entry = Entry::err(1, "a\u{1b}]52;c;eA==\u{7}b\nc\td");
        assert_eq!(entry.error, "ab c d");
        cache.write(&scope, "m", &entry).unwrap();
        assert_eq!(cache.read(&scope, "m").unwrap().error, "ab c d");
        assert_eq!(bounded_text("\u{1b}[2Jx"), "x");
    }
}
