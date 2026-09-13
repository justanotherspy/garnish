//! Reading the few Claude Code settings garnish needs (auto-compaction and
//! reduced motion), with the same precedence Claude Code uses: env >
//! managed > local > project > user.

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
pub fn env_truthy(v: Option<&String>) -> bool {
    v.is_some_and(|s| matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
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
#[must_use]
pub fn managed_settings_from(hook: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    match hook {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(PathBuf::from(v)),
        None => Some(platform_managed_settings()),
    }
}

/// Where Claude Code reads the managed settings file on this platform.
fn platform_managed_settings() -> PathBuf {
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

/// Settings files in precedence order (highest first) for a project
/// directory, each with the name `doctor` labels it by.
///
/// `managed` (the organisation file, [`managed_settings_path`] for a real
/// run and `None` for a pinned one) > `local`
/// (`.claude/settings.local.json`) > `project` (`.claude/settings.json`) >
/// `user` (`~/.claude/settings.json`).
#[must_use]
pub fn settings_chain(
    managed: Option<&Path>,
    project_dir: Option<&Path>,
    home: Option<&Path>,
) -> Vec<(&'static str, PathBuf)> {
    let mut files = Vec::with_capacity(4);
    if let Some(m) = managed {
        files.push(("managed", m.to_path_buf()));
    }
    if let Some(dir) = project_dir {
        files.push(("local", dir.join(".claude").join("settings.local.json")));
        files.push(("project", dir.join(".claude").join("settings.json")));
    }
    if let Some(h) = home {
        files.push(("user", h.join(".claude").join("settings.json")));
    }
    files
}

/// The paths of [`settings_chain`], highest precedence first.
#[must_use]
pub fn settings_files(
    managed: Option<&Path>,
    project_dir: Option<&Path>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    settings_chain(managed, project_dir, home).into_iter().map(|(_, path)| path).collect()
}

/// Most bytes garnish reads from one settings file (SPEC § 5: sizes are
/// bounded); a larger file is skipped like one that does not parse, so a
/// file nobody controls cannot make a tick slow.
pub const MAX_SETTINGS_BYTES: u64 = 1 << 20;

/// The keys garnish reads from one settings file (SPEC § 2.3, § 4.2 and
/// the `doctor` report of § 7); each is `None` when the file does not set
/// it.
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
    /// `disableAllHooks`.
    pub disable_all_hooks: Option<bool>,
    /// `tui`: which renderer draws the screen (`"fullscreen"` or
    /// `"default"`), which decides what a tall status line does (SPEC § 2.1).
    pub tui: Option<String>,
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
                disable_all_hooks: v.get("disableAllHooks").and_then(serde_json::Value::as_bool),
                tui: v.get("tui").and_then(serde_json::Value::as_str).map(str::to_owned),
            })
        }
        Ok(_) => Err("not a JSON object".to_owned()),
        Err(e) => Err(format!("not valid JSON: {e}")),
    }
}

/// Extract the keys garnish reads from one settings JSON text; a text that
/// does not parse sets none of them.
#[must_use]
pub fn from_settings_json(text: &str) -> FileKeys {
    parse_settings_json(text).unwrap_or_default()
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

/// Read one settings file of the chain, at most [`MAX_SETTINGS_BYTES`] of it.
#[must_use]
pub fn read_file(path: &Path) -> FileState {
    use std::io::Read as _;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileState::Absent,
        Err(e) => return FileState::Unreadable(e.to_string()),
    };
    let mut text = String::new();
    // One byte past the cap tells an over-long file from one exactly at it.
    match file.take(MAX_SETTINGS_BYTES.saturating_add(1)).read_to_string(&mut text) {
        Ok(n) if u64::try_from(n).is_ok_and(|n| n > MAX_SETTINGS_BYTES) => {
            FileState::Invalid(format!("longer than the {MAX_SETTINGS_BYTES} bytes garnish reads"))
        }
        Ok(_) => parse_settings_json(&text).map_or_else(FileState::Invalid, FileState::Keys),
        Err(e) => FileState::Unreadable(e.to_string()),
    }
}

/// The keys of the files of a chain that are there and parse, in the
/// chain's order (highest precedence first); a file that is absent, too
/// long or does not parse contributes nothing.
#[must_use]
pub fn read_keys(files: &[PathBuf]) -> Vec<FileKeys> {
    files
        .iter()
        .filter_map(|file| match read_file(file) {
            FileState::Keys(keys) => Some(keys),
            _ => None,
        })
        .collect()
}

/// The keys a command run in `project` reads: the whole chain, the
/// managed file ([`managed_settings_path`]) included (a render goes
/// through `Clock` instead, which may forbid the read).
#[must_use]
pub fn keys_for(project: Option<&Path>, home: Option<&Path>) -> Vec<FileKeys> {
    read_keys(&settings_files(managed_settings_path().as_deref(), project, home))
}

/// `prefersReducedMotion` over a chain's keys: the first file that sets
/// it wins, as for the auto-compaction keys, and no file setting it means
/// `false` (SPEC § 4.2).
#[must_use]
pub fn reduced_motion(keys: &[FileKeys]) -> bool {
    keys.iter().find_map(|k| k.reduced_motion).unwrap_or(false)
}

/// Resolve auto-compaction from the environment and a chain's keys.
#[must_use]
pub fn resolve(env: &Env, keys: &[FileKeys]) -> AutoCompact {
    let window = keys.iter().find_map(|k| k.auto_compact_window);
    let enabled = keys.iter().find_map(|k| k.auto_compact_enabled);
    let env_window = env.window.as_deref().and_then(|s| s.trim().parse::<u64>().ok());
    AutoCompact {
        enabled: enabled.unwrap_or(true)
            && !env_truthy(env.disable.as_ref())
            && !env_truthy(env.disable_all.as_ref()),
        window: env_window.or(window),
        pct_override: env.pct.as_deref().and_then(|s| s.trim().parse::<f64>().ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let keys = from_settings_json(r#"{"autoCompactWindow": 500000}"#);
        assert_eq!(keys, FileKeys { auto_compact_window: Some(500_000), ..Default::default() });
        let keys = from_settings_json(r#"{"autoCompactEnabled": false}"#);
        assert_eq!(keys, FileKeys { auto_compact_enabled: Some(false), ..Default::default() });
        let keys = from_settings_json(r#"{"prefersReducedMotion": true, "theme": "dark"}"#);
        assert_eq!(keys, FileKeys { reduced_motion: Some(true), ..Default::default() });
        assert_eq!(from_settings_json(r#"{"prefersReducedMotion": "yes"}"#), FileKeys::default());
        assert_eq!(from_settings_json("nope"), FileKeys::default());
        assert_eq!(from_settings_json("[1]"), FileKeys::default());
        // The doctor's keys, a BOM tolerated as `install` tolerates it, and
        // the two ways a file fails, named.
        let keys = parse_settings_json(
            "\u{feff}{\"statusLine\": {\"type\": \"command\", \"command\": \"garnish\", \"refreshInterval\": 2, \"hideVimModeIndicator\": true}, \"disableAllHooks\": false, \"tui\": \"fullscreen\"}",
        )
        .unwrap();
        assert_eq!(keys.status_line_command.as_deref(), Some("garnish"));
        assert_eq!(keys.refresh_interval, Some(2.0));
        assert_eq!(keys.hide_vim_mode, Some(true));
        assert_eq!(keys.disable_all_hooks, Some(false));
        assert_eq!(keys.tui.as_deref(), Some("fullscreen"));
        assert_eq!(from_settings_json(r#"{"tui": 1}"#).tui, None);
        assert!(parse_settings_json("{ broken").unwrap_err().starts_with("not valid JSON: "));
        assert_eq!(parse_settings_json("[1]").unwrap_err(), "not a JSON object");
        // An empty file is what a fresh `touch` leaves and what `install`
        // treats as `{}`: no keys, no problem.
        assert_eq!(parse_settings_json(""), Ok(FileKeys::default()));
        assert_eq!(parse_settings_json(" \n"), Ok(FileKeys::default()));
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_file(&dir.path().join("none.json")), FileState::Absent);
        assert!(matches!(read_file(dir.path()), FileState::Unreadable(_)));
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
        assert_eq!(read_keys(&[huge, at_cap, dir.path().join("none.json")]).len(), 1);
        let chain = settings_chain(
            Some(Path::new("/m/managed.json")),
            Some(Path::new("/p")),
            Some(Path::new("/h")),
        );
        let labels: Vec<&str> = chain.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, ["managed", "local", "project", "user"]);
        assert_eq!(chain[3].1, Path::new("/h/.claude/settings.json"));
        assert_eq!(settings_files(None, None, None), Vec::<PathBuf>::new());
        assert_eq!(settings_files(None, Some(Path::new("/p")), None).len(), 2);
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
        let chain = |p: Option<&Path>, h: Option<&Path>| read_keys(&settings_files(None, p, h));
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
        assert!(!reduced_motion(&read_keys(&settings_files(
            Some(&managed),
            Some(&proj),
            Some(&home)
        ))));
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
        let chain = |p: Option<&Path>, h: Option<&Path>| read_keys(&settings_files(None, p, h));
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
        // The managed file: the platform's, the hook's, or none at all.
        let platform = managed_settings_from(None).unwrap();
        assert!(platform.ends_with("managed-settings.json"), "{}", platform.display());
        assert_eq!(managed_settings_from(Some(std::ffi::OsStr::new(""))), None);
        assert_eq!(
            managed_settings_from(Some(std::ffi::OsStr::new("/tmp/m.json"))),
            Some(PathBuf::from("/tmp/m.json"))
        );
        assert_eq!(settings_files(Some(&platform), None, None), vec![platform]);
        assert_eq!(settings_files(None, None, None), Vec::<PathBuf>::new());
    }
}
