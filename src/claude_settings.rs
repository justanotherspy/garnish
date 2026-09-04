//! Reading the few Claude Code settings garnish needs (auto-compaction), with
//! the same precedence Claude Code uses: env > local > project > user.

use std::path::Path;

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

/// The managed (organisation-deployed) settings file for this platform.
#[must_use]
pub fn managed_settings_path() -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        std::path::PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json")
    } else {
        std::path::PathBuf::from("/etc/claude-code/managed-settings.json")
    }
}

/// Settings files in precedence order (highest first) for a project directory:
/// managed > `.claude/settings.local.json` > `.claude/settings.json` > user.
#[must_use]
pub fn settings_files(project_dir: Option<&Path>, home: Option<&Path>) -> Vec<std::path::PathBuf> {
    let mut files = vec![managed_settings_path()];
    if let Some(dir) = project_dir {
        files.push(dir.join(".claude").join("settings.local.json"));
        files.push(dir.join(".claude").join("settings.json"));
    }
    if let Some(h) = home {
        files.push(h.join(".claude").join("settings.json"));
    }
    files
}

/// Extract `autoCompactWindow` / `autoCompactEnabled` from one settings JSON text.
#[must_use]
pub fn from_settings_json(text: &str) -> (Option<u64>, Option<bool>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return (None, None) };
    let window = v.get("autoCompactWindow").and_then(serde_json::Value::as_u64);
    let enabled = v.get("autoCompactEnabled").and_then(serde_json::Value::as_bool);
    (window, enabled)
}

/// Resolve auto-compaction for a working directory.
#[must_use]
pub fn resolve(env: &Env, cwd: Option<&Path>, home: Option<&Path>) -> AutoCompact {
    let mut window: Option<u64> = None;
    let mut enabled: Option<bool> = None;
    for file in settings_files(cwd, home) {
        if window.is_some() && enabled.is_some() {
            break;
        }
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let (w, e) = from_settings_json(&text);
        window = window.or(w);
        enabled = enabled.or(e);
    }
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
        assert_eq!(from_settings_json(r#"{"autoCompactWindow": 500000}"#), (Some(500_000), None));
        assert_eq!(from_settings_json(r#"{"autoCompactEnabled": false}"#), (None, Some(false)));
        assert_eq!(from_settings_json("nope"), (None, None));
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
        let ac = resolve(&Env::default(), Some(&proj), Some(&home));
        assert_eq!(ac, AutoCompact { enabled: true, window: Some(400_000), pct_override: None });
        std::fs::write(
            proj.join(".claude/settings.local.json"),
            r#"{"autoCompactEnabled": false}"#,
        )
        .unwrap();
        let ac = resolve(&Env::default(), Some(&proj), Some(&home));
        assert_eq!(ac.window, Some(400_000));
        assert!(!ac.enabled);
        let env = Env {
            window: Some("250000".into()),
            pct: Some("80".into()),
            disable: None,
            disable_all: None,
        };
        let ac = resolve(&env, None, Some(&home));
        assert_eq!(
            ac,
            AutoCompact { enabled: true, window: Some(250_000), pct_override: Some(80.0) }
        );
        for on in ["1", "true", "YES", " On "] {
            let env = Env { disable: Some(on.into()), ..Default::default() };
            assert!(!resolve(&env, None, None).enabled, "{on}");
        }
        for off in ["0", "false", "no", "off", "", "maybe"] {
            let env = Env { disable: Some(off.into()), ..Default::default() };
            assert!(resolve(&env, None, None).enabled, "{off}");
        }
        assert_eq!(settings_files(None, None), vec![managed_settings_path()]);
    }
}
