//! The JSON payload Claude Code writes to the status line command's stdin.
//!
//! Every field that the docs list as "may be absent" or "may be null" is an
//! `Option`. Unknown fields are ignored so newer Claude Code versions never
//! break parsing, and numbers are parsed leniently (a fractional millisecond
//! count or a numeric string must never blank the status line). See
//! `SPEC.md` § 2.2 for the contract.

use serde::{Deserialize, Deserializer};

/// Accept any JSON number (or numeric string) for an unsigned counter:
/// fractional values are floored, negatives clamp to zero, `null` is `None`.
///
/// # Errors
/// Only when the value is neither a number, a numeric string, nor `null`.
fn lenient_u64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        U(u64),
        I(i64),
        F(f64),
        S(String),
    }
    Ok(match Option::<Num>::deserialize(d)? {
        None => None,
        Some(Num::U(n)) => Some(n),
        Some(Num::I(n)) => Some(u64::try_from(n).unwrap_or(0)),
        Some(Num::F(f)) => Some(crate::num::floor_to_u64(f)),
        Some(Num::S(s)) => s.trim().parse::<f64>().ok().map(crate::num::floor_to_u64),
    })
}

/// Accept any JSON number (or numeric string) for a signed epoch timestamp.
///
/// # Errors
/// Only when the value is neither a number, a numeric string, nor `null`.
fn lenient_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        I(i64),
        F(f64),
        S(String),
    }
    let to_i64 = |f: f64| -> i64 {
        if f.is_nan() {
            0
        } else if f < 0.0 {
            i64::try_from(crate::num::floor_to_u64(-f)).map_or(i64::MIN, i64::saturating_neg)
        } else {
            i64::try_from(crate::num::floor_to_u64(f)).unwrap_or(i64::MAX)
        }
    };
    Ok(match Option::<Num>::deserialize(d)? {
        None => None,
        Some(Num::I(n)) => Some(n),
        Some(Num::F(f)) => Some(to_i64(f)),
        Some(Num::S(s)) => s.trim().parse::<f64>().ok().map(to_i64),
    })
}

/// Accept any JSON number (or numeric string) for a float; `null` is `None`.
///
/// # Errors
/// Only when the value is neither a number, a numeric string, nor `null`.
fn lenient_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        F(f64),
        S(String),
    }
    Ok(match Option::<Num>::deserialize(d)? {
        None => None,
        Some(Num::F(f)) => Some(f),
        Some(Num::S(s)) => s.trim().parse::<f64>().ok(),
    })
}

/// `null` for a list means "no entries".
///
/// # Errors
/// Only when the value is neither a list of strings nor `null`.
fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

/// Top-level payload.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Payload {
    /// Current working directory (same as `workspace.current_dir`).
    pub cwd: Option<String>,
    /// Stable session identifier; used as the cache key.
    pub session_id: Option<String>,
    /// Custom or AI-generated session name. Absent for default names.
    pub session_name: Option<String>,
    /// UUID of the prompt being processed.
    pub prompt_id: Option<String>,
    /// Path to the JSONL transcript (unused).
    pub transcript_path: Option<String>,
    /// Claude Code version.
    pub version: Option<String>,
    /// Model identity.
    pub model: Option<Model>,
    /// Workspace directories and repo identity.
    pub workspace: Option<Workspace>,
    /// Output style.
    pub output_style: Option<OutputStyle>,
    /// Cost and duration counters.
    pub cost: Option<Cost>,
    /// Context window usage.
    pub context_window: Option<ContextWindow>,
    /// Whether the last response exceeded 200k tokens in total.
    pub exceeds_200k_tokens: Option<bool>,
    /// Prompt cache statistics (Claude Code ≥ 2.1.251).
    pub prompt_cache: Option<PromptCache>,
    /// Fast mode enabled.
    pub fast_mode: Option<bool>,
    /// Reasoning effort; absent when the model does not support it.
    pub effort: Option<Effort>,
    /// Extended thinking state.
    pub thinking: Option<Thinking>,
    /// Rate limits; present only for subscription users after the first API response.
    pub rate_limits: Option<RateLimits>,
    /// Vim mode; present only when vim mode is enabled.
    pub vim: Option<Vim>,
    /// Agent identity when running with `--agent`.
    pub agent: Option<Agent>,
    /// Open pull/merge request for the current branch.
    pub pr: Option<Pr>,
    /// Claude Code worktree session.
    pub worktree: Option<Worktree>,
}

/// `model` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Model {
    /// Model identifier, e.g. `claude-opus-5`.
    pub id: Option<String>,
    /// Human-readable name, e.g. `Opus`.
    pub display_name: Option<String>,
}

/// `workspace` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Workspace {
    /// Current directory.
    pub current_dir: Option<String>,
    /// Directory where Claude Code was launched.
    pub project_dir: Option<String>,
    /// Directories added with `/add-dir`.
    #[serde(deserialize_with = "null_as_empty")]
    pub added_dirs: Vec<String>,
    /// Linked git worktree name; absent in the main working tree.
    pub git_worktree: Option<String>,
    /// Repository identity parsed from `origin`.
    pub repo: Option<Repo>,
}

/// `workspace.repo` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Repo {
    /// Host, e.g. `github.com`.
    pub host: Option<String>,
    /// Owner or namespace.
    pub owner: Option<String>,
    /// Repository name.
    pub name: Option<String>,
}

/// `output_style` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OutputStyle {
    /// Style name, e.g. `default`.
    pub name: Option<String>,
}

/// `cost` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cost {
    /// Estimated session cost in USD.
    #[serde(deserialize_with = "lenient_f64")]
    pub total_cost_usd: Option<f64>,
    /// Wall-clock milliseconds since the session started.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_duration_ms: Option<u64>,
    /// Milliseconds spent waiting on the API.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_api_duration_ms: Option<u64>,
    /// Lines added this session.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_lines_added: Option<u64>,
    /// Lines removed this session.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_lines_removed: Option<u64>,
}

/// `context_window` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ContextWindow {
    /// Input tokens currently in the window (includes cache reads/writes).
    #[serde(deserialize_with = "lenient_u64")]
    pub total_input_tokens: Option<u64>,
    /// Output tokens from the most recent response.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_output_tokens: Option<u64>,
    /// Window size in tokens: 200 000 or 1 000 000.
    #[serde(deserialize_with = "lenient_u64")]
    pub context_window_size: Option<u64>,
    /// Percentage used, computed from input tokens only. Null early on.
    #[serde(deserialize_with = "lenient_f64")]
    pub used_percentage: Option<f64>,
    /// Percentage remaining.
    #[serde(deserialize_with = "lenient_f64")]
    pub remaining_percentage: Option<f64>,
    /// Per-component usage of the last API call. Null before the first call.
    pub current_usage: Option<CurrentUsage>,
}

/// `context_window.current_usage` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CurrentUsage {
    /// Fresh (uncached) input tokens.
    #[serde(deserialize_with = "lenient_u64")]
    pub input_tokens: Option<u64>,
    /// Output tokens.
    #[serde(deserialize_with = "lenient_u64")]
    pub output_tokens: Option<u64>,
    /// Tokens written to the prompt cache.
    #[serde(deserialize_with = "lenient_u64")]
    pub cache_creation_input_tokens: Option<u64>,
    /// Tokens read from the prompt cache.
    #[serde(deserialize_with = "lenient_u64")]
    pub cache_read_input_tokens: Option<u64>,
}

/// `prompt_cache` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct PromptCache {
    /// Whether the cached prefix is within its TTL.
    pub warm: Option<bool>,
    /// Whether any response reported cache tokens.
    pub caching_observed: Option<bool>,
    /// Cache lifetime: `"5m"` or `"1h"`.
    pub ttl: Option<String>,
    /// Epoch seconds when the cached prefix goes cold.
    #[serde(deserialize_with = "lenient_i64")]
    pub expires_at: Option<i64>,
    /// API requests made for the main conversation.
    #[serde(deserialize_with = "lenient_u64")]
    pub requests: Option<u64>,
    /// Requests that re-processed content the cache already held.
    #[serde(deserialize_with = "lenient_u64")]
    pub misses: Option<u64>,
    /// Cache rebuilds after compaction or tool-result clearing.
    #[serde(deserialize_with = "lenient_u64")]
    pub expected_rebuilds: Option<u64>,
    /// Cache read tokens as a fraction of all input tokens (0..1).
    #[serde(deserialize_with = "lenient_f64")]
    pub hit_ratio: Option<f64>,
    /// Tokens written to the cache this session.
    #[serde(deserialize_with = "lenient_u64")]
    pub cache_write_tokens: Option<u64>,
    /// Tokens written by requests counted as misses.
    #[serde(deserialize_with = "lenient_u64")]
    pub miss_recache_tokens: Option<u64>,
    /// Epoch seconds of the last miss.
    #[serde(deserialize_with = "lenient_i64")]
    pub last_miss_at: Option<i64>,
    /// Tokens the next request would re-cache if cold.
    #[serde(deserialize_with = "lenient_u64")]
    pub recache_tokens_if_cold: Option<u64>,
}

/// `effort` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Effort {
    /// `low`, `medium`, `high`, `xhigh`, or `max`.
    pub level: Option<String>,
}

/// `thinking` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Thinking {
    /// Whether extended thinking is enabled.
    pub enabled: Option<bool>,
}

/// `rate_limits` object. Each window may be independently absent.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RateLimits {
    /// Rolling five-hour window.
    pub five_hour: Option<RateWindow>,
    /// Rolling seven-day window.
    pub seven_day: Option<RateWindow>,
    /// Gateway spend limit; percentage may exceed 100.
    pub spend_limit: Option<RateWindow>,
}

/// A single rate-limit window.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RateWindow {
    /// Percentage consumed.
    #[serde(deserialize_with = "lenient_f64")]
    pub used_percentage: Option<f64>,
    /// Epoch seconds when the window resets.
    #[serde(deserialize_with = "lenient_i64")]
    pub resets_at: Option<i64>,
}

/// `vim` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Vim {
    /// `NORMAL`, `INSERT`, `VISUAL`, or `VISUAL LINE`.
    pub mode: Option<String>,
}

/// `agent` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Agent {
    /// Agent name.
    pub name: Option<String>,
}

/// `pr` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Pr {
    /// PR or MR number.
    #[serde(deserialize_with = "lenient_u64")]
    pub number: Option<u64>,
    /// Full URL.
    pub url: Option<String>,
    /// `approved`, `pending`, `changes_requested`, or `draft`.
    pub review_state: Option<String>,
    /// `mr` for GitLab merge requests; absent for GitHub.
    pub kind: Option<String>,
}

/// `worktree` object (Claude Code worktree session).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Worktree {
    /// Worktree name.
    pub name: Option<String>,
    /// Absolute path.
    pub path: Option<String>,
    /// Branch checked out in the worktree.
    pub branch: Option<String>,
    /// Directory before entering the worktree.
    pub original_cwd: Option<String>,
    /// Branch before entering the worktree.
    pub original_branch: Option<String>,
}

impl Payload {
    /// Parse the JSON payload. Unknown fields are ignored; every field is optional.
    ///
    /// # Errors
    /// Returns the serde error when the input is not a JSON object.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// True when the harness reports subscription rate limits.
    #[must_use]
    pub const fn is_subscription(&self) -> bool {
        self.rate_limits.is_some()
    }

    /// The current directory, preferring `workspace.current_dir`.
    #[must_use]
    pub fn current_dir(&self) -> Option<&str> {
        self.workspace.as_ref().and_then(|w| w.current_dir.as_deref()).or(self.cwd.as_deref())
    }

    /// The directory Claude Code was launched in, falling back to the current directory.
    #[must_use]
    pub fn project_dir(&self) -> Option<&str> {
        self.workspace
            .as_ref()
            .and_then(|w| w.project_dir.as_deref())
            .filter(|p| !p.is_empty())
            .or_else(|| self.current_dir())
    }

    /// The context window size, defaulting to one million tokens.
    #[must_use]
    pub fn context_window_size(&self) -> u64 {
        self.context_window
            .as_ref()
            .and_then(|c| c.context_window_size)
            .filter(|&n| n > 0)
            .unwrap_or(1_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_object_parses_to_defaults() {
        let p = Payload::parse("{}").unwrap();
        assert_eq!(p, Payload::default());
        assert!(!p.is_subscription());
        assert_eq!(p.context_window_size(), 1_000_000);
    }

    #[test]
    fn unknown_fields_are_ignored_and_nulls_are_none() {
        let p = Payload::parse(
            r#"{"future_field": 1, "context_window": {"used_percentage": null, "context_window_size": 200000, "current_usage": null}}"#,
        )
        .unwrap();
        let cw = p.context_window.unwrap();
        assert_eq!(cw.used_percentage, None);
        assert_eq!(cw.current_usage, None);
        assert_eq!(cw.context_window_size, Some(200_000));
    }

    #[test]
    fn numbers_are_parsed_leniently() {
        let p = Payload::parse(
            r#"{"cost": {"total_duration_ms": 4320000.7, "total_cost_usd": "1.5", "total_lines_added": -3},
                "rate_limits": {"five_hour": {"used_percentage": "23.5", "resets_at": 1738433620.0}},
                "prompt_cache": {"expires_at": -1.5, "requests": "14"},
                "pr": {"number": 42.0},
                "workspace": {"added_dirs": null},
                "context_window": {"context_window_size": 0}}"#,
        )
        .unwrap();
        let cost = p.cost.as_ref().unwrap();
        assert_eq!(cost.total_duration_ms, Some(4_320_000));
        assert_eq!(cost.total_cost_usd, Some(1.5));
        assert_eq!(cost.total_lines_added, Some(0));
        let rl = p.rate_limits.as_ref().unwrap().five_hour.as_ref().unwrap();
        assert_eq!(rl.used_percentage, Some(23.5));
        assert_eq!(rl.resets_at, Some(1_738_433_620));
        let pc = p.prompt_cache.as_ref().unwrap();
        assert_eq!(pc.expires_at, Some(-1));
        assert_eq!(pc.requests, Some(14));
        assert_eq!(p.pr.as_ref().unwrap().number, Some(42));
        assert_eq!(p.workspace.as_ref().unwrap().added_dirs.len(), 0);
        assert_eq!(p.context_window_size(), 1_000_000);
        let p = Payload::parse(r#"{"pr": {"number": "forty-two"}}"#).unwrap();
        assert_eq!(p.pr.unwrap().number, None);
        assert!(Payload::parse(r#"{"pr": {"number": true}}"#).is_err());
    }

    #[test]
    fn subscription_detected_from_rate_limits() {
        let p = Payload::parse(
            r#"{"rate_limits": {"five_hour": {"used_percentage": 1.5, "resets_at": 10}}}"#,
        )
        .unwrap();
        assert!(p.is_subscription());
        let rl = p.rate_limits.unwrap();
        assert_eq!(rl.five_hour.unwrap().resets_at, Some(10));
        assert!(rl.seven_day.is_none());
        let p = Payload::parse(r#"{"rate_limits": {}}"#).unwrap();
        assert!(p.is_subscription());
        let p = Payload::parse(r#"{"rate_limits": null}"#).unwrap();
        assert!(!p.is_subscription());
    }

    #[test]
    fn current_dir_prefers_workspace() {
        let p = Payload::parse(r#"{"cwd": "/a", "workspace": {"current_dir": "/b"}}"#).unwrap();
        assert_eq!(p.current_dir(), Some("/b"));
        assert_eq!(p.project_dir(), Some("/b"));
        let p = Payload::parse(r#"{"cwd": "/a"}"#).unwrap();
        assert_eq!(p.current_dir(), Some("/a"));
        let p = Payload::parse(r#"{"cwd": "/a", "workspace": {"project_dir": "/p"}}"#).unwrap();
        assert_eq!(p.project_dir(), Some("/p"));
    }

    #[test]
    fn not_an_object_is_an_error() {
        assert!(Payload::parse("[1,2]").is_err());
        assert!(Payload::parse("").is_err());
    }
}
