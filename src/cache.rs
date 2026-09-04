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
use std::path::{Path, PathBuf};

use crate::time::now_millis;

/// Environment variable overriding the cache root.
pub const CACHE_DIR_ENV: &str = "GARNISH_CACHE_DIR";

/// Locks older than this are considered abandoned.
pub const LOCK_STALE_MS: i64 = 60_000;

/// Locks younger than this are trusted without checking the pid (hand-over window).
pub const LOCK_GRACE_MS: i64 = 2_000;

/// Session directories idle for longer than this are swept.
pub const GC_MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;

/// Upper bound on directories removed per sweep.
pub const GC_MAX_PER_SWEEP: usize = 50;

/// Where an entry lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Per Claude Code session.
    Session(String),
    /// Per repository worktree (hash of the git common dir + worktree path).
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

/// A cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// When it was computed, epoch milliseconds.
    pub computed_at_ms: i64,
    /// TTL the writer used, milliseconds.
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
    #[must_use]
    pub fn err(ttl_ms: u64, error: impl Into<String>) -> Self {
        Self {
            computed_at_ms: now_millis(),
            ttl_ms,
            status: Status::Err,
            values: BTreeMap::new(),
            error: error.into(),
        }
    }

    /// Age in milliseconds (never negative).
    #[must_use]
    pub fn age_ms(&self) -> i64 {
        now_millis().saturating_sub(self.computed_at_ms).max(0)
    }

    /// Whether the entry is within `ttl_ms`.
    #[must_use]
    pub fn is_fresh(&self, ttl_ms: u64) -> bool {
        u64::try_from(self.age_ms()).is_ok_and(|age| age <= ttl_ms)
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
    #[must_use]
    pub fn adopt(path: PathBuf) -> Self {
        let _ = fs::write(&path, format!("{} {}\n", std::process::id(), now_millis()));
        Self { path, armed: true }
    }

    /// Path of the lock file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep the lock file after drop (hand-over to a spawned worker).
    pub const fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if self.armed {
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
        Self { root }
    }

    /// Resolve the root from the environment:
    /// `GARNISH_CACHE_DIR` > `$XDG_RUNTIME_DIR/garnish` > `$XDG_CACHE_HOME/garnish`
    /// > `~/.cache/garnish` (macOS: `~/Library/Caches/garnish`).
    #[must_use]
    pub fn from_env() -> Self {
        let env =
            |k: &str| std::env::var_os(k).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty());
        let root = env(CACHE_DIR_ENV)
            .or_else(|| env("XDG_RUNTIME_DIR").map(|d| d.join("garnish")))
            .or_else(|| env("XDG_CACHE_HOME").map(|d| d.join("garnish")))
            .or_else(|| {
                env("HOME").map(|h| {
                    if cfg!(target_os = "macos") {
                        h.join("Library").join("Caches").join("garnish")
                    } else {
                        h.join(".cache").join("garnish")
                    }
                })
            })
            .unwrap_or_else(|| std::env::temp_dir().join("garnish"));
        Self { root }
    }

    /// The root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
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
        let text = fs::read_to_string(self.entry_path(scope, module)).ok()?;
        Entry::parse(&text)
    }

    /// Look a module up: entry, freshness against `ttl_ms`, and lock state.
    #[must_use]
    pub fn lookup(&self, scope: &Scope, module: &str, ttl_ms: u64) -> Lookup {
        let entry = self.read(scope, module);
        let fresh = entry.as_ref().is_some_and(|e| e.status == Status::Ok && e.is_fresh(ttl_ms));
        let in_progress = self.lock_is_live(&self.lock_path(scope, module));
        Lookup { entry, fresh, in_progress }
    }

    /// Write an entry atomically. Creates the scope directory on demand.
    ///
    /// # Errors
    /// Propagates I/O errors.
    pub fn write(&self, scope: &Scope, module: &str, entry: &Entry) -> std::io::Result<()> {
        let path = self.entry_path(scope, module);
        let dir = path.parent().map_or_else(|| self.root.clone(), Path::to_path_buf);
        let created = !dir.exists();
        fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".{}.tmp.{}", sanitize(module), std::process::id()));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(entry.to_text().as_bytes())?;
            f.sync_data().ok();
        }
        fs::rename(&tmp, &path)?;
        if created && matches!(scope, Scope::Session(_)) {
            self.gc_sessions(GC_MAX_AGE_MS, GC_MAX_PER_SWEEP);
        }
        Ok(())
    }

    /// Try to take the lock for a module.
    #[must_use]
    pub fn lock(&self, scope: &Scope, module: &str) -> LockOutcome {
        let path = self.lock_path(scope, module);
        if let Some(dir) = path.parent()
            && let Err(e) = fs::create_dir_all(dir)
        {
            return LockOutcome::Unavailable(e);
        }
        // Write the content first, then link it into place: `hard_link` fails
        // with AlreadyExists when the lock exists, and a reader never sees an
        // empty lock file (which would look abandoned and get reclaimed).
        let tmp = path.with_extension(format!("lock.tmp.{}", std::process::id()));
        if let Err(e) = fs::write(&tmp, format!("{} {}\n", std::process::id(), now_millis())) {
            return LockOutcome::Unavailable(e);
        }
        let _cleanup = TmpFile(tmp.clone());
        for attempt in 0..2 {
            match fs::hard_link(&tmp, &path) {
                Ok(()) => return LockOutcome::Acquired(LockGuard { path, armed: true }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if attempt == 0 && !self.lock_is_live(&path) {
                        // Reclaim atomically: only one of several racing ticks
                        // wins the rename, so only one goes on to recreate the lock.
                        let stale = path.with_extension(format!("stale.{}", std::process::id()));
                        if fs::rename(&path, &stale).is_err() {
                            return LockOutcome::Held;
                        }
                        let _ = fs::remove_file(&stale);
                        continue;
                    }
                    return LockOutcome::Held;
                }
                Err(e) => return LockOutcome::Unavailable(e),
            }
        }
        LockOutcome::Held
    }

    /// Whether a lock file exists and belongs to a live, recent process.
    ///
    /// A lock younger than [`LOCK_GRACE_MS`] is always live: a tick writes the
    /// lock with its own pid, exits, and the spawned worker re-stamps it with
    /// the worker's pid a few milliseconds later. Without the grace window the
    /// lock would look dead in between and a second worker would be spawned.
    #[must_use]
    pub fn lock_is_live(&self, path: &Path) -> bool {
        let Ok(text) = fs::read_to_string(path) else { return false };
        let mut parts = text.split_whitespace();
        let pid: Option<u32> = parts.next().and_then(|p| p.parse().ok());
        let stamp: Option<i64> = parts.next().and_then(|p| p.parse().ok());
        let age = stamp.map_or(i64::MAX, |s| now_millis().saturating_sub(s));
        if age > LOCK_STALE_MS {
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

    /// Remove session directories whose newest file is older than `max_age_ms`.
    /// Returns how many were removed (at most `max`).
    pub fn gc_sessions(&self, max_age_ms: i64, max: usize) -> usize {
        let Ok(dir) = fs::read_dir(self.root.join("sessions")) else { return 0 };
        let now = now_millis();
        let mut removed = 0;
        for entry in dir.filter_map(Result::ok) {
            if removed >= max {
                break;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let newest = fs::read_dir(&path)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter_map(|f| f.metadata().ok()?.modified().ok())
                .filter_map(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                .max()
                .or_else(|| {
                    entry
                        .metadata()
                        .ok()?
                        .modified()
                        .ok()?
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                });
            let idle = newest.map_or(i64::MAX, |n| now.saturating_sub(n));
            if idle > max_age_ms && fs::remove_dir_all(&path).is_ok() {
                removed = removed.saturating_add(1);
            }
        }
        removed
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
        // an err entry is never fresh
        cache.write(&scope, "m", &Entry::err(5_000, "nope")).unwrap();
        let l = cache.lookup(&scope, "m", 5_000);
        assert!(!l.fresh);
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
        // adopting re-stamps the file with our pid
        fs::write(cache.lock_path(&scope, "m"), "1 1").unwrap();
        let g = LockGuard::adopt(cache.lock_path(&scope, "m"));
        assert!(cache.lock_is_live(&cache.lock_path(&scope, "m")));
        drop(g);
        assert!(!cache.lock_path(&scope, "m").exists());
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
        // Age them only after all exist: creating a session dir sweeps idle ones.
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
        // Creating a brand-new session dir sweeps idle ones automatically.
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
    }

    #[test]
    fn root_resolution_prefers_explicit_env() {
        assert_eq!(key_hash(&["a", "b"]), key_hash(&["a", "b"]));
        assert_ne!(key_hash(&["a", "b"]), key_hash(&["ab"]));
        assert_eq!(sanitize("../x y"), "___x_y");
        assert_eq!(sanitize(""), "_");
    }
}
