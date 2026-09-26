//! Reading the Claude Code settings garnish needs.
//!
//! Auto-compaction, reduced motion, the `statusLine` keys,
//! `disableAllHooks`, the `sandbox` and `voice` switches and the `tui`
//! renderer choice, with the same precedence Claude Code uses: env >
//! managed > local > project > user. Also where `~/.claude.json`, the file
//! Claude Code keeps for itself, lives (SPEC § 3.8).

use std::path::{Path, PathBuf};

/// Tokens Claude Code reserves for the compaction summary (observed in 2.1.260).
pub const DEFAULT_COMPACT_BUFFER: u64 = 13_000;

/// Resolved auto-compaction state.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoCompact {
    /// Whether auto-compaction is enabled at all.
    pub enabled: bool,
    /// Configured window in tokens, if any (`None` = model default = full window).
    pub window: Option<u64>,
    /// Percentage override from `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, if valid.
    pub pct_override: Option<f64>,
}

impl AutoCompact {
    /// The token count at which compaction fires for a given context window.
    ///
    /// Mirrors Claude Code: `min(window, configured) − buffer`, lowered further
    /// by the percentage override. Returns `None` when disabled.
    #[must_use]
    pub fn threshold(&self, context_window: u64, buffer: u64) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        let effective = self.window.map_or(context_window, |w| w.min(context_window));
        let base = effective.saturating_sub(buffer);
        let with_pct = self
            .pct_override
            .filter(|p| *p > 0.0 && *p <= 100.0)
            .map(|p| crate::num::floor_to_u64(crate::num::u64_to_f64(effective) * p / 100.0))
            .map_or(base, |pct_threshold| pct_threshold.min(base));
        Some(with_pct)
    }
}

/// Environment lookups, abstracted so tests can inject values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `CLAUDE_CODE_AUTO_COMPACT_WINDOW`.
    pub window: Option<String>,
    /// `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`.
    pub pct: Option<String>,
    /// `DISABLE_AUTO_COMPACT`.
    pub disable: Option<String>,
    /// `DISABLE_COMPACT`.
    pub disable_all: Option<String>,
}

impl Env {
    /// Read from the process environment.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            window: std::env::var("CLAUDE_CODE_AUTO_COMPACT_WINDOW").ok(),
            pct: std::env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE").ok(),
            disable: std::env::var("DISABLE_AUTO_COMPACT").ok(),
            disable_all: std::env::var("DISABLE_COMPACT").ok(),
        }
    }
}

/// Claude Code's own rule for boolean environment variables (`isEnvTruthy`):
/// only `1`, `true`, `yes`, `on` (case-insensitive) count as set.
#[must_use]
pub fn env_truthy(v: Option<&str>) -> bool {
    env_bool(v) == Some(true)
}

/// The one reading of a boolean environment variable (SPEC § 9).
///
/// Claude Code's truthy words (`1`, `true`, `yes`, `on`) are `Some(true)`,
/// their opposites (`0`, `false`, `no`, `off`) `Some(false)`, both trimmed
/// and in any case; anything else, unset or empty included, is `None`.
#[must_use]
pub fn env_bool(v: Option<&str>) -> Option<bool> {
    match v?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// [`env_bool`] of the process's variable `key`: how every `GARNISH_*`
/// switch reads.
#[must_use]
pub fn env_flag(key: &str) -> Option<bool> {
    env_bool(std::env::var(key).ok().as_deref())
}

/// The test hook that stands in for the platform's managed settings file
/// (SPEC § 9): a path, or empty for no managed file at all.
pub const MANAGED_SETTINGS_ENV: &str = "GARNISH_MANAGED_SETTINGS";

/// The managed (organisation-deployed) settings file: the platform's,
/// unless [`MANAGED_SETTINGS_ENV`] names another or is empty (then there
/// is none).
#[must_use]
pub fn managed_settings_path() -> Option<PathBuf> {
    managed_settings_from(std::env::var_os(MANAGED_SETTINGS_ENV).as_deref())
}

/// [`managed_settings_path`] for an explicit value of the hook.
///
/// A relative value counts as unset: it would name a file in whatever
/// directory garnish runs from, a checkout's own (verification of
/// 2026-09-26).
#[must_use]
pub fn managed_settings_from(hook: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    match hook {
        Some(v) if v.is_empty() => None,
        Some(v) if Path::new(v).is_absolute() => Some(PathBuf::from(v)),
        Some(_) | None => Some(platform_managed_settings()),
    }
}

/// Where Claude Code reads the managed settings file on this platform.
#[must_use]
pub fn platform_managed_settings() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json")
    } else {
        PathBuf::from("/etc/claude-code/managed-settings.json")
    }
}

/// The home directory every command agrees on: `HOME`, unless it is unset
/// or empty (then there is none, and nothing guesses one; SPEC § 5).
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Claude Code's variable for keeping its home-directory files elsewhere:
/// every `~/.claude` path, `~/.claude.json` included, moves under it.
pub const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// `CLAUDE_CONFIG_DIR` as the process has it: `None` when unset or empty
/// (the SPEC § 5 rule for a path variable), or relative, which would make
/// the checkout garnish runs in the user's own directory (the `XDG_*` rule,
/// `config::xdg_base`).
#[must_use]
pub fn config_dir_from_env() -> Option<PathBuf> {
    crate::config::env_path(CONFIG_DIR_ENV).filter(|dir| dir.is_absolute())
}

/// Where the file Claude Code keeps for itself lives (the sign-in, the MCP
/// servers, per-project state).
///
/// `$CLAUDE_CONFIG_DIR/.claude.json` when that variable is set and
/// non-empty, else `~/.claude.json`; `None` without either. The `account`
/// worker reads it (SPEC § 3.8).
#[must_use]
pub fn claude_json_path(home: Option<&Path>) -> Option<PathBuf> {
    claude_json_in(config_dir_from_env().as_deref(), home)
}

/// [`claude_json_path`] for an explicit config directory.
#[must_use]
pub fn claude_json_in(config_dir: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    config_dir.or(home).map(|dir| dir.join(".claude.json"))
}

/// The directory Claude Code keeps the user's own files in: the user
/// `settings.json`, the `skills/` directory.
///
/// `$CLAUDE_CONFIG_DIR` when set and non-empty, else `~/.claude`; `None`
/// without either. `install` writes there and the settings chain reads
/// there, so the two agree with Claude Code and with each other.
#[must_use]
pub fn user_dir(home: Option<&Path>) -> Option<PathBuf> {
    user_dir_in(config_dir_from_env().as_deref(), home)
}

/// [`user_dir`] for an explicit config directory.
#[must_use]
pub fn user_dir_in(config_dir: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    config_dir.map(Path::to_path_buf).or_else(|| home.map(|h| h.join(".claude")))
}

/// Settings files in precedence order (highest first) for a project
/// directory, each with the name `doctor` labels it by.
///
/// `managed` (the organisation file, [`managed_settings_path`] for a real
/// run and `None` for a pinned one) > `local`
/// (`.claude/settings.local.json`) > `project` (`.claude/settings.json`) >
/// `user` (`settings.json` in `user_dir`, the directory [`user_dir`]
/// names).
#[must_use]
pub fn settings_chain(
    managed: Option<&Path>,
    project_dir: Option<&Path>,
    user_dir: Option<&Path>,
) -> Vec<(&'static str, PathBuf)> {
    let mut files = Vec::with_capacity(4);
    if let Some(m) = managed {
        files.push(("managed", m.to_path_buf()));
    }
    if let Some(dir) = project_dir {
        files.push(("local", dir.join(".claude").join("settings.local.json")));
        files.push(("project", dir.join(".claude").join("settings.json")));
    }
    if let Some(dir) = user_dir {
        files.push(("user", dir.join("settings.json")));
    }
    files
}

/// Most bytes garnish reads from one settings file (SPEC § 5: sizes are
/// bounded); a larger file is skipped like one that does not parse, so a
/// file nobody controls cannot make a tick slow.
pub const MAX_SETTINGS_BYTES: u64 = 1 << 20;

/// Most bytes a command run by hand reads of a settings file to find its
/// `statusLine.command` ([`read_file_up_to`]): no tick waits on it, so the
/// file is read as Claude Code reads it, whole, within a bound that only a
/// file nobody wrote by hand can reach.
pub const MAX_COMMAND_SETTINGS_BYTES: u64 = 64 << 20;

/// The `tui` key: which renderer Claude Code draws the screen with, which
/// decides what a tall status line does (SPEC § 2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tui {
    /// `"fullscreen"`: the alternate-screen renderer.
    Fullscreen,
    /// `"default"`: the classic renderer.
    Default,
    /// Any other value, kept as written. Claude Code's schema takes only
    /// the two names: it drops such a value from the managed file and
    /// rejects any other file that carries one, so it never decides.
    Other(serde_json::Value),
}

impl Tui {
    fn from_json(value: &serde_json::Value) -> Self {
        match value.as_str() {
            Some("fullscreen") => Self::Fullscreen,
            Some("default") => Self::Default,
            _ => Self::Other(value.clone()),
        }
    }
}

/// The keys garnish reads from one settings file (SPEC § 2.1, § 2.3, § 4.2
/// and the `doctor` report of § 7); each is `None` when the file does not
/// set it.
///
/// Claude Code merges settings objects key by key, so the `statusLine`
/// keys are tracked one by one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileKeys {
    /// `autoCompactWindow`.
    pub auto_compact_window: Option<u64>,
    /// `autoCompactEnabled`.
    pub auto_compact_enabled: Option<bool>,
    /// `prefersReducedMotion`.
    pub reduced_motion: Option<bool>,
    /// `statusLine.command`.
    pub status_line_command: Option<String>,
    /// `statusLine.refreshInterval`, in seconds (Claude Code ignores a
    /// value below 1).
    pub refresh_interval: Option<f64>,
    /// `statusLine.hideVimModeIndicator`.
    pub hide_vim_mode: Option<bool>,
    /// `statusLine.padding`: cells the harness pads the status line with
    /// on each side (SPEC § 2.1).
    pub padding: Option<u64>,
    /// `disableAllHooks`.
    pub disable_all_hooks: Option<bool>,
    /// `sandbox.enabled`: Bash commands run isolated (the `sandbox` badge,
    /// SPEC § 3.8).
    pub sandbox_enabled: Option<bool>,
    /// `voice.enabled`: voice dictation is on (the `voice` badge, SPEC
    /// § 3.8).
    pub voice_enabled: Option<bool>,
    /// `tui`, as written: which renderer draws the screen (SPEC § 2.1).
    pub tui: Option<Tui>,
    /// The `GARNISH_*` entries of the `env` block, which Claude Code copies
    /// into the session's environment: a hand-run command tells a
    /// `GARNISH_CONFIG` a checkout set from one the person set by them
    /// (SPEC § 4).
    pub garnish_env: Vec<(String, String)>,
}

impl FileKeys {
    /// The value this file's `env` block gives the variable `name`.
    #[must_use]
    pub fn env(&self, name: &str) -> Option<&str> {
        self.garnish_env.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }
}

/// Parse one settings file's text into the keys garnish reads. An empty
/// (or whitespace-only) file sets none of them and is not a problem: a
/// fresh `touch`ed file is what Claude Code and `install` treat as `{}`.
///
/// # Errors
/// When the text is not valid JSON or not a JSON object, with the problem.
pub fn parse_settings_json(text: &str) -> Result<FileKeys, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return Ok(FileKeys::default());
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(v)) => {
            let status = v.get("statusLine").and_then(serde_json::Value::as_object);
            let status_key = |key: &str| status.and_then(|s| s.get(key));
            // `sandbox` and `voice` are objects like `statusLine`; a switch
            // that is not a boolean (or a table that is not an object) is
            // unset, as for every other key.
            let switch = |table: &str| {
                v.get(table)
                    .and_then(serde_json::Value::as_object)
                    .and_then(|t| t.get("enabled"))
                    .and_then(serde_json::Value::as_bool)
            };
            Ok(FileKeys {
                auto_compact_window: v.get("autoCompactWindow").and_then(serde_json::Value::as_u64),
                auto_compact_enabled: v
                    .get("autoCompactEnabled")
                    .and_then(serde_json::Value::as_bool),
                reduced_motion: v.get("prefersReducedMotion").and_then(serde_json::Value::as_bool),
                status_line_command: status_key("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                refresh_interval: status_key("refreshInterval").and_then(serde_json::Value::as_f64),
                hide_vim_mode: status_key("hideVimModeIndicator")
                    .and_then(serde_json::Value::as_bool),
                padding: status_key("padding").and_then(serde_json::Value::as_u64),
                disable_all_hooks: v.get("disableAllHooks").and_then(serde_json::Value::as_bool),
                sandbox_enabled: switch("sandbox"),
                voice_enabled: switch("voice"),
                tui: v.get("tui").map(Tui::from_json),
                garnish_env: v.get("env").and_then(serde_json::Value::as_object).map_or_else(
                    Vec::new,
                    |env| {
                        env.iter()
                            .filter(|(key, _)| key.starts_with("GARNISH_"))
                            .filter_map(|(key, value)| {
                                value.as_str().map(|value| (key.clone(), value.to_owned()))
                            })
                            .collect()
                    },
                ),
            })
        }
        Ok(_) => Err("not a JSON object".to_owned()),
        Err(e) => Err(format!("not valid JSON: {e}")),
    }
}

/// What one file of the settings chain holds.
#[derive(Debug, Clone, PartialEq)]
pub enum FileState {
    /// No file there.
    Absent,
    /// The file is there but cannot be read (permissions, a directory).
    Unreadable(String),
    /// The file does not parse: the problem.
    Invalid(String),
    /// The keys the file sets.
    Keys(FileKeys),
}

/// `O_NONBLOCK`, with which [`open_regular`] opens: an open that would wait
/// (a FIFO with no writer) returns at once instead, and a regular file
/// reads as it always does. Spelt per platform, there being no `libc`
/// here; on a Unix not listed it is 0, and the check before the open is
/// the only guard.
#[cfg(any(target_os = "linux", target_os = "android"))]
const O_NONBLOCK: i32 = if cfg!(any(
    target_arch = "mips",
    target_arch = "mips32r6",
    target_arch = "mips64",
    target_arch = "mips64r6"
)) {
    0o200
} else if cfg!(any(target_arch = "sparc", target_arch = "sparc64")) {
    0x4000
} else {
    0o4000
};
#[cfg(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const O_NONBLOCK: i32 = 0x0004;
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
const O_NONBLOCK: i32 = 0;

/// The error for a path that is not a regular file.
fn not_regular() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file")
}

/// Open a file garnish reads on a timer, refusing anything that is not a
/// regular file.
///
/// `open` on a FIFO waits for a writer for ever, and a repository nobody
/// here built can put one at `.claude/settings.json` (CLAUDE.md, "The
/// repository is not the user's file"). The path is checked first, so a
/// device or FIFO found there is never opened, and then the handle:
/// whoever can write the directory can swap one in between the two, and
/// then it is opened, but the open does not wait ([`O_NONBLOCK`]) and what
/// it opened is refused by the second check (review 2026-09-25). The file comes with the length its handle gives;
/// `Ok(None)` when there is nothing at `path`; a symlink is followed.
///
/// # Errors
/// The metadata or open error, or one saying the path is not a regular
/// file.
pub fn open_regular(path: &Path) -> std::io::Result<Option<(std::fs::File, u64)>> {
    let found = |r: std::io::Result<std::fs::Metadata>| match r {
        Ok(meta) if meta.is_file() => Ok(Some(meta.len())),
        Ok(_) => Err(not_regular()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    };
    if found(std::fs::metadata(path))?.is_none() {
        return Ok(None);
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(found(file.metadata())?.map(|len| (file, len)))
}

/// At most `limit` bytes of the regular file at `path`.
///
/// [`open_regular`] followed by a bounded read: the one way garnish reads
/// a file it does not own on a timer (settings files, `.claude.json`, every
/// file under `.git`, cache entries). `Ok(None)` when there is nothing at
/// `path`. A caller that must tell an over-long file from one at its cap
/// asks for one byte more and compares. The buffer is sized from the
/// handle, so a large file (a `.git/config` of 500 KB) is one read, not
/// sixteen growing ones.
///
/// # Errors
/// The metadata, open or read error, or one saying the path is not a
/// regular file.
pub fn read_regular(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read as _;
    let Some((file, len)) = open_regular(path)? else { return Ok(None) };
    let mut bytes = Vec::with_capacity(usize::try_from(len.min(limit)).unwrap_or(0));
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// Read one settings file of the chain, at most [`MAX_SETTINGS_BYTES`] of it.
#[must_use]
pub fn read_file(path: &Path) -> FileState {
    read_file_up_to(path, MAX_SETTINGS_BYTES)
}

/// [`read_file`] with another cap: [`MAX_COMMAND_SETTINGS_BYTES`] for a
/// command run by hand looking for the `statusLine.command`.
#[must_use]
pub fn read_file_up_to(path: &Path, limit: u64) -> FileState {
    // Bytes first, then UTF-8: reading straight into a `String` validates
    // the *truncated* stream, so a file over the cap whose cut lands inside
    // a multi-byte character failed as "unreadable: stream did not contain
    // valid UTF-8" and sent the reader looking for corruption that was not
    // there. One byte past the cap tells an over-long file from one at it.
    let bytes = match read_regular(path, limit.saturating_add(1)) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return FileState::Absent,
        Err(e) => return FileState::Unreadable(e.to_string()),
    };
    if u64::try_from(bytes.len()).is_ok_and(|n| n > limit) {
        return FileState::Invalid(format!("longer than the {limit} bytes garnish reads"));
    }
    match String::from_utf8(bytes) {
        Ok(text) => parse_settings_json(&text).map_or_else(FileState::Invalid, FileState::Keys),
        Err(e) => FileState::Unreadable(e.to_string()),
    }
}

/// Why Claude Code rejects the whole file `label` names for the keys it
/// sets, or `None` when it reads it.
///
/// Its schema takes only `default` and `fullscreen` for `tui`: another
/// value in the managed file is dropped on its own, but any other file
/// carrying one is not read at all, so none of its keys may count (SPEC
/// § 7, the `tui` row of `doctor`).
#[must_use]
pub fn rejected(label: &str, keys: &FileKeys) -> Option<&'static str> {
    (label != "managed" && matches!(keys.tui, Some(Tui::Other(_))))
        .then_some("`tui` is not `default` or `fullscreen`, so Claude Code rejects this file")
}

/// The keys of the files of a chain that Claude Code reads, in the
/// chain's order (highest precedence first); a file that is absent, too
/// long, does not parse or is [`rejected`] contributes nothing.
#[must_use]
pub fn read_keys(chain: &[(&'static str, PathBuf)]) -> Vec<FileKeys> {
    chain
        .iter()
        .filter_map(|(label, file)| match read_file(file) {
            FileState::Keys(keys) if rejected(label, &keys).is_none() => Some(keys),
            _ => None,
        })
        .collect()
}

/// The keys a command run in `project` reads: the whole chain.
///
/// The managed file ([`managed_settings_path`]) and `CLAUDE_CONFIG_DIR`
/// ([`user_dir`]) included; a render goes through `Clock` instead, which
/// may forbid the read.
#[must_use]
pub fn keys_for(project: Option<&Path>, home: Option<&Path>) -> Vec<FileKeys> {
    let user = user_dir(home);
    read_keys(&settings_chain(managed_settings_path().as_deref(), project, user.as_deref()))
}

/// A boolean key over a chain's keys: the first file that sets it wins,
/// as for the auto-compaction keys, and no file setting it means `false`.
/// `pick` names the key (`|k| k.sandbox_enabled`).
#[must_use]
pub fn flag(keys: &[FileKeys], pick: impl Fn(&FileKeys) -> Option<bool>) -> bool {
    keys.iter().find_map(pick).unwrap_or(false)
}

/// `prefersReducedMotion` over a chain's keys (SPEC § 4.2): [`flag`] for
/// the key every animation asks about.
#[must_use]
pub fn reduced_motion(keys: &[FileKeys]) -> bool {
    flag(keys, |k| k.reduced_motion)
}

/// Resolve auto-compaction from the environment and a chain's keys.
#[must_use]
pub fn resolve(env: &Env, keys: &[FileKeys]) -> AutoCompact {
    let window = keys.iter().find_map(|k| k.auto_compact_window);
    let enabled = keys.iter().find_map(|k| k.auto_compact_enabled);
    let env_window = env.window.as_deref().and_then(|s| s.trim().parse::<u64>().ok());
    AutoCompact {
        enabled: enabled.unwrap_or(true)
            && !env_truthy(env.disable.as_deref())
            && !env_truthy(env.disable_all.as_deref()),
        window: env_window.or(window),
        pct_override: env.pct.as_deref().and_then(|s| s.trim().parse::<f64>().ok()),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// The keys of a settings text, with an unparsable one contributing
    /// none — the shape `read_file` gives the chain.
    fn keys_of(text: &str) -> FileKeys {
        parse_settings_json(text).unwrap_or_default()
    }

    /// A FIFO at `path`, through the `mkfifo` binary (there is no
    /// dependency for it): `None` where the binary is missing, so the
    /// test that wants one skips rather than fails.
    pub fn fifo(path: &Path) -> Option<PathBuf> {
        let made = std::process::Command::new("mkfifo").arg(path).status().ok()?.success();
        made.then(|| path.to_path_buf())
    }

    /// The check that a path is a regular file and the open that follows
    /// are two steps, and whoever can write the directory (a shared
    /// checkout) can swap a FIFO in between: `open` then waited for a
    /// writer for ever, in the tick or in a worker holding its lock
    /// (review 2026-09-25). The open itself must not block, and the handle
    /// it gives must be the one checked. A thread swaps a file and a FIFO
    /// at one name as fast as it can while reads go on.
    #[test]
    fn a_file_swapped_for_a_fifo_after_the_check_never_blocks_a_read() {
        let dir = tempfile::tempdir().unwrap();
        let Some(pipe) = fifo(&dir.path().join("pipe")) else { return };
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "{}").unwrap();
        let path = dir.path().join("settings.json");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let swapper = {
            let (stop, path) = (stop.clone(), path.clone());
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    for target in [&plain, &pipe] {
                        let _ = std::fs::remove_file(&path);
                        let _ = std::fs::hard_link(target, &path);
                    }
                }
            })
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut read = 0;
            for _ in 0..200_000 {
                if matches!(read_regular(&path, 16), Ok(Some(_))) {
                    read += 1;
                }
            }
            let _ = tx.send(read);
        });
        let read = rx.recv_timeout(std::time::Duration::from_secs(15));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(read.is_ok(), "a read blocked on a FIFO swapped in after the check");
        swapper.join().unwrap();
    }

    #[test]
    fn threshold_math_matches_claude_code() {
        let ac = AutoCompact { enabled: true, window: None, pct_override: None };
        assert_eq!(ac.threshold(1_000_000, 13_000), Some(987_000));
        assert_eq!(ac.threshold(200_000, 13_000), Some(187_000));
        let capped = AutoCompact { enabled: true, window: Some(500_000), pct_override: None };
        assert_eq!(capped.threshold(1_000_000, 13_000), Some(487_000));
        let bigger = AutoCompact { enabled: true, window: Some(5_000_000), pct_override: None };
        assert_eq!(bigger.threshold(1_000_000, 13_000), Some(987_000));
        let pct = AutoCompact { enabled: true, window: None, pct_override: Some(50.0) };
        assert_eq!(pct.threshold(1_000_000, 13_000), Some(500_000));
        let pct_hi = AutoCompact { enabled: true, window: None, pct_override: Some(99.9) };
        assert_eq!(pct_hi.threshold(1_000_000, 13_000), Some(987_000));
        let off = AutoCompact { enabled: false, window: None, pct_override: None };
        assert_eq!(off.threshold(1_000_000, 13_000), None);
    }

    #[test]
    fn settings_json_extraction() {
        let keys = keys_of(r#"{"autoCompactWindow": 500000}"#);
        assert_eq!(keys, FileKeys { auto_compact_window: Some(500_000), ..Default::default() });
        let keys = keys_of(r#"{"autoCompactEnabled": false}"#);
        assert_eq!(keys, FileKeys { auto_compact_enabled: Some(false), ..Default::default() });
        let keys = keys_of(r#"{"prefersReducedMotion": true, "theme": "dark"}"#);
        assert_eq!(keys, FileKeys { reduced_motion: Some(true), ..Default::default() });
        assert_eq!(keys_of(r#"{"prefersReducedMotion": "yes"}"#), FileKeys::default());
        assert_eq!(keys_of("nope"), FileKeys::default());
        assert_eq!(keys_of("[1]"), FileKeys::default());
        // The doctor's keys, a BOM tolerated as `install` tolerates it, and
        // the two ways a file fails, named.
        let keys = parse_settings_json(
            "\u{feff}{\"statusLine\": {\"type\": \"command\", \"command\": \"garnish\", \"refreshInterval\": 2, \"hideVimModeIndicator\": true}, \"disableAllHooks\": false, \"tui\": \"fullscreen\"}",
        )
        .unwrap();
        assert_eq!(keys.status_line_command.as_deref(), Some("garnish"));
        assert_eq!(keys.refresh_interval, Some(2.0));
        assert_eq!(keys.hide_vim_mode, Some(true));
        assert_eq!(keys.padding, None);
        assert_eq!(keys_of(r#"{"statusLine": {"padding": 2}}"#).padding, Some(2));
        assert_eq!(keys.disable_all_hooks, Some(false));
        assert_eq!(keys.tui, Some(Tui::Fullscreen));
        assert_eq!(keys_of(r#"{"tui": "default"}"#).tui, Some(Tui::Default));
        // Anything but the two names is kept as written, a non-string too,
        // so `doctor` can say what Claude Code does with it.
        for other in [r#""FULLSCREEN""#, r#"" fullscreen""#, r#""""#, "1", "null", "[1]"] {
            let value: serde_json::Value = serde_json::from_str(other).unwrap();
            let keys = keys_of(&format!(r#"{{"tui": {other}}}"#));
            assert_eq!(keys.tui, Some(Tui::Other(value)), "{other}");
        }
        assert_eq!(keys_of("{}").tui, None);
        // The two switches of SPEC § 3.8 sit in objects, like `statusLine`;
        // a switch that is not a boolean, or a table that is not an object,
        // is unset, never read as on.
        let keys = keys_of(
            r#"{"sandbox": {"enabled": true, "network": {}}, "voice": {"enabled": false, "mode": "tap"}}"#,
        );
        assert_eq!((keys.sandbox_enabled, keys.voice_enabled), (Some(true), Some(false)));
        for unset in [
            r#"{"sandbox": {"enabled": "true"}, "voice": {"enabled": 1}}"#,
            r#"{"sandbox": true, "voice": "on"}"#,
            r#"{"sandbox": {"mode": "strict"}, "voice": {}}"#,
            r#"{"sandbox": null, "voice": [true]}"#,
        ] {
            let keys = keys_of(unset);
            assert_eq!((keys.sandbox_enabled, keys.voice_enabled), (None, None), "{unset}");
        }
        assert!(parse_settings_json("{ broken").unwrap_err().starts_with("not valid JSON: "));
        assert_eq!(parse_settings_json("[1]").unwrap_err(), "not a JSON object");
        // An empty file is what a fresh `touch` leaves and what `install`
        // treats as `{}`: no keys, no problem.
        assert_eq!(parse_settings_json(""), Ok(FileKeys::default()));
        assert_eq!(parse_settings_json(" \n"), Ok(FileKeys::default()));
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_file(&dir.path().join("none.json")), FileState::Absent);
        assert!(matches!(read_file(dir.path()), FileState::Unreadable(_)));
        // A FIFO where a settings file should be (a cloned repository can
        // carry one): refused without opening, since `open` would wait for
        // a writer for ever, on the tick.
        if let Some(fifo) = fifo(&dir.path().join("fifo.json")) {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(read_file(&fifo));
            });
            let state = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("blocked");
            assert!(
                matches!(state, FileState::Unreadable(ref e) if e.contains("not a regular file")),
                "{state:?}"
            );
        }
        std::fs::write(dir.path().join("bad.json"), "{").unwrap();
        assert!(matches!(read_file(&dir.path().join("bad.json")), FileState::Invalid(_)));
        std::fs::write(dir.path().join("ok.json"), "{}").unwrap();
        assert_eq!(read_file(&dir.path().join("ok.json")), FileState::Keys(FileKeys::default()));
        std::fs::write(dir.path().join("empty.json"), "").unwrap();
        assert_eq!(read_file(&dir.path().join("empty.json")), FileState::Keys(FileKeys::default()));
        // A file past the cap is skipped, not parsed (SPEC § 5).
        let huge = dir.path().join("huge.json");
        let padding = " ".repeat(usize::try_from(MAX_SETTINGS_BYTES).unwrap());
        std::fs::write(&huge, format!("{{\"prefersReducedMotion\": true}}{padding}")).unwrap();
        assert!(matches!(read_file(&huge), FileState::Invalid(ref e) if e.contains("longer")));
        let at_cap = dir.path().join("at-cap.json");
        let body = "{\"prefersReducedMotion\": true}";
        std::fs::write(
            &at_cap,
            format!(
                "{body}{}",
                " ".repeat(usize::try_from(MAX_SETTINGS_BYTES).unwrap() - body.len())
            ),
        )
        .unwrap();
        assert!(
            matches!(read_file(&at_cap), FileState::Keys(ref k) if k.reduced_motion == Some(true))
        );
        // Over the cap and non-ASCII: the cut lands inside a multi-byte
        // character, which used to fail UTF-8 validation first and tell the
        // user their file was "unreadable" rather than too long.
        let wide = dir.path().join("wide.json");
        let fill = "é".repeat(usize::try_from(MAX_SETTINGS_BYTES).unwrap());
        std::fs::write(&wide, format!("{{\"note\": \"{fill}\"}}")).unwrap();
        assert!(matches!(read_file(&wide), FileState::Invalid(ref e) if e.contains("longer")));
        let none = dir.path().join("none.json");
        assert_eq!(read_keys(&[("user", huge), ("user", at_cap), ("user", none)]).len(), 1);
        let chain = settings_chain(
            Some(Path::new("/m/managed.json")),
            Some(Path::new("/p")),
            Some(Path::new("/h/.claude")),
        );
        let labels: Vec<&str> = chain.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, ["managed", "local", "project", "user"]);
        assert_eq!(chain[3].1, Path::new("/h/.claude/settings.json"));
        assert_eq!(settings_chain(None, None, None), Vec::new());
        assert_eq!(settings_chain(None, Some(Path::new("/p")), None).len(), 2);
    }

    /// SPEC § 7: `CLAUDE_CONFIG_DIR` moves the user directory (the settings
    /// file and the skills) as it moves `.claude.json`; without it the
    /// directory is `~/.claude`, and without a home there is none.
    #[test]
    fn the_user_dir_follows_the_config_dir_then_the_home() {
        let home = Path::new("/h");
        let dir = Path::new("/cfg");
        assert_eq!(user_dir_in(None, Some(home)), Some(PathBuf::from("/h/.claude")));
        assert_eq!(user_dir_in(Some(dir), Some(home)), Some(PathBuf::from("/cfg")));
        assert_eq!(user_dir_in(Some(dir), None), Some(PathBuf::from("/cfg")));
        assert_eq!(user_dir_in(None, None), None);
        let user = user_dir_in(Some(dir), Some(home));
        let chain = settings_chain(None, None, user.as_deref());
        assert_eq!(chain, vec![("user", PathBuf::from("/cfg/settings.json"))]);
        if std::env::var_os(CONFIG_DIR_ENV).is_none_or(|v| v.is_empty()) {
            assert_eq!(user_dir(Some(home)), Some(PathBuf::from("/h/.claude")));
        }
    }

    /// A `tui` that is neither name has Claude Code reject a whole file
    /// (only the managed one merely loses the key), so the file's other keys
    /// never count: `doctor` said "rejects that file" while the file's own
    /// row said `ok` and the tick froze animations because of it.
    #[test]
    fn a_file_claude_code_rejects_contributes_no_keys() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("local.json");
        let user = dir.path().join("user.json");
        std::fs::write(&local, r#"{"tui": "full", "prefersReducedMotion": true}"#).unwrap();
        std::fs::write(&user, r#"{"prefersReducedMotion": false}"#).unwrap();
        let keys = read_keys(&[("local", local.clone()), ("user", user.clone())]);
        assert_eq!(keys.len(), 1);
        assert!(!reduced_motion(&keys), "the user file decides");
        let managed = read_keys(&[("managed", local), ("user", user)]);
        assert!(reduced_motion(&managed), "the managed file only loses its `tui`");
        let keys = parse_settings_json(r#"{"tui": "default"}"#).unwrap();
        assert_eq!(rejected("local", &keys), None);
        let keys = parse_settings_json(r#"{"tui": 1}"#).unwrap();
        assert!(rejected("project", &keys).is_some_and(|why| why.contains("rejects")));
        assert_eq!(rejected("managed", &keys), None);
    }

    /// SPEC § 4.2: `prefersReducedMotion` follows the file order of the
    /// auto-compaction keys (local > project > user), the first file that
    /// sets it wins, and an unparsable file is skipped rather than read as
    /// "off".
    #[test]
    fn reduced_motion_follows_the_settings_chain() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        // No managed file: the tests must not depend on the machine's.
        let chain = |p: Option<&Path>, h: Option<&Path>| {
            read_keys(&settings_chain(None, p, user_dir_in(None, h).as_deref()))
        };
        assert!(!reduced_motion(&chain(Some(&proj), Some(&home))), "no file: off");
        std::fs::write(home.join(".claude/settings.json"), r#"{"prefersReducedMotion": true}"#)
            .unwrap();
        assert!(reduced_motion(&chain(Some(&proj), Some(&home))));
        assert!(reduced_motion(&chain(None, Some(&home))));
        assert!(!reduced_motion(&chain(Some(&proj), None)), "the user file needs a home");
        std::fs::write(proj.join(".claude/settings.json"), r#"{"prefersReducedMotion": false}"#)
            .unwrap();
        assert!(!reduced_motion(&chain(Some(&proj), Some(&home))), "the project file wins");
        std::fs::write(
            proj.join(".claude/settings.local.json"),
            r#"{"prefersReducedMotion": true}"#,
        )
        .unwrap();
        assert!(reduced_motion(&chain(Some(&proj), Some(&home))), "the local file wins over both");
        std::fs::write(proj.join(".claude/settings.local.json"), "{ broken").unwrap();
        assert!(!reduced_motion(&chain(Some(&proj), Some(&home))), "a broken file is skipped");
        std::fs::write(proj.join(".claude/settings.json"), r#"{"theme": "dark"}"#).unwrap();
        assert!(
            reduced_motion(&chain(Some(&proj), Some(&home))),
            "a file without the key is skipped"
        );
        // A managed file outranks them all.
        let managed = dir.path().join("managed.json");
        std::fs::write(&managed, r#"{"prefersReducedMotion": false}"#).unwrap();
        assert!(!reduced_motion(&read_keys(&settings_chain(
            Some(&managed),
            Some(&proj),
            Some(&home.join(".claude"))
        ))));
    }

    /// SPEC § 3.8: the `sandbox` and `voice` switches resolve as
    /// `prefersReducedMotion` does (the first file that sets one wins,
    /// no file means off), through the one [`flag`] rule.
    #[test]
    fn flags_follow_the_settings_chain() {
        let user = keys_of(r#"{"sandbox": {"enabled": true}, "voice": {"enabled": true}}"#);
        let project = keys_of(r#"{"sandbox": {"enabled": false}}"#);
        let other = keys_of(r#"{"theme": "dark"}"#);
        assert!(!flag(&[], |k| k.sandbox_enabled), "no file: off");
        assert!(flag(std::slice::from_ref(&user), |k| k.sandbox_enabled));
        assert!(flag(&[other.clone(), user.clone()], |k| k.voice_enabled), "a file without it");
        let chain = [other, project, user];
        assert!(!flag(&chain, |k| k.sandbox_enabled), "the first file that sets it wins");
        assert!(flag(&chain, |k| k.voice_enabled), "key by key");
        assert_eq!(flag(&chain, |k| k.reduced_motion), reduced_motion(&chain));
    }

    /// SPEC § 3.8: `~/.claude.json` moves with `CLAUDE_CONFIG_DIR` when
    /// that is set and non-empty, and there is none without a home.
    #[test]
    fn claude_json_follows_the_config_dir_then_the_home() {
        let home = Path::new("/h");
        let dir = Path::new("/cfg");
        assert_eq!(claude_json_in(None, Some(home)), Some(PathBuf::from("/h/.claude.json")));
        assert_eq!(claude_json_in(Some(dir), Some(home)), Some(PathBuf::from("/cfg/.claude.json")));
        assert_eq!(claude_json_in(Some(dir), None), Some(PathBuf::from("/cfg/.claude.json")));
        assert_eq!(claude_json_in(None, None), None);
        // The process reader applies the empty-means-unset rule of
        // `config::env_path`; with the variable unset it is the home's.
        if std::env::var_os(CONFIG_DIR_ENV).is_none_or(|v| v.is_empty()) {
            assert_eq!(claude_json_path(Some(home)), Some(PathBuf::from("/h/.claude.json")));
            assert_eq!(claude_json_path(None), None);
        }
    }

    #[test]
    fn precedence_env_over_files() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"autoCompactWindow": 300000, "autoCompactEnabled": true}"#,
        )
        .unwrap();
        std::fs::write(proj.join(".claude/settings.json"), r#"{"autoCompactWindow": 400000}"#)
            .unwrap();
        let chain = |p: Option<&Path>, h: Option<&Path>| {
            read_keys(&settings_chain(None, p, user_dir_in(None, h).as_deref()))
        };
        let ac = resolve(&Env::default(), &chain(Some(&proj), Some(&home)));
        assert_eq!(ac, AutoCompact { enabled: true, window: Some(400_000), pct_override: None });
        std::fs::write(
            proj.join(".claude/settings.local.json"),
            r#"{"autoCompactEnabled": false}"#,
        )
        .unwrap();
        let ac = resolve(&Env::default(), &chain(Some(&proj), Some(&home)));
        assert_eq!(ac.window, Some(400_000));
        assert!(!ac.enabled);
        let env = Env {
            window: Some("250000".into()),
            pct: Some("80".into()),
            disable: None,
            disable_all: None,
        };
        let ac = resolve(&env, &chain(None, Some(&home)));
        assert_eq!(
            ac,
            AutoCompact { enabled: true, window: Some(250_000), pct_override: Some(80.0) }
        );
        for on in ["1", "true", "YES", " On "] {
            let env = Env { disable: Some(on.into()), ..Default::default() };
            assert!(!resolve(&env, &[]).enabled, "{on}");
        }
        for off in ["0", "false", "no", "off", "", "maybe"] {
            let env = Env { disable: Some(off.into()), ..Default::default() };
            assert!(resolve(&env, &[]).enabled, "{off}");
        }
        // The one boolean rule of SPEC § 9: both word sets, anything else unset.
        for on in ["1", "true", "YES", " On "] {
            assert_eq!(env_bool(Some(on)), Some(true), "{on}");
        }
        for off in ["0", "false", "No", " OFF "] {
            assert_eq!(env_bool(Some(off)), Some(false), "{off}");
        }
        for unset in [Some(""), Some("maybe"), Some("2"), None] {
            assert_eq!(env_bool(unset), None, "{unset:?}");
        }
        // The managed file: the platform's, the hook's, or none at all.
        let platform = managed_settings_from(None).unwrap();
        assert!(platform.ends_with("managed-settings.json"), "{}", platform.display());
        assert_eq!(managed_settings_from(Some(std::ffi::OsStr::new(""))), None);
        assert_eq!(
            managed_settings_from(Some(std::ffi::OsStr::new("/tmp/m.json"))),
            Some(PathBuf::from("/tmp/m.json"))
        );
        // A relative one would name a checkout's file (verification of
        // 2026-09-26): it counts as unset.
        assert_eq!(
            managed_settings_from(Some(std::ffi::OsStr::new(".claude/settings.json"))),
            Some(platform.clone())
        );
        assert_eq!(settings_chain(Some(&platform), None, None), vec![("managed", platform)]);
        assert_eq!(settings_chain(None, None, None), Vec::new());
    }

    /// The `GARNISH_*` entries of a file's `env` block are kept, strings
    /// only, and nothing else of it.
    #[test]
    fn the_garnish_entries_of_the_env_block_are_read() {
        let keys = keys_of(
            r#"{"env": {"GARNISH_CONFIG": "/x.toml", "GARNISH_ANIMATE": 0, "PATH": "/bin"}}"#,
        );
        assert_eq!(keys.garnish_env, [("GARNISH_CONFIG".to_owned(), "/x.toml".to_owned())]);
        assert_eq!(keys.env("GARNISH_CONFIG"), Some("/x.toml"));
        assert_eq!(keys.env("PATH"), None);
        assert_eq!(keys_of(r#"{"env": []}"#).garnish_env, []);
    }
}
