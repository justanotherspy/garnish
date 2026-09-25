//! The JSON payload Claude Code writes to the status line command's stdin.
//!
//! Every field that the docs list as "may be absent" or "may be null" is an
//! `Option`. Unknown fields are ignored so newer Claude Code versions never
//! break parsing, and every known field is read on its own: one of the
//! wrong type is absent, alone, rather than blanking the status line
//! (SPEC § 5). Numbers are parsed leniently too (a fractional millisecond
//! count or a numeric string is a number). See `SPEC.md` § 2.2 for the
//! contract; a field no module reads yet is kept where one plausibly will,
//! and says so.

use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer};

/// A value of the wrong type is `None` rather than an error: the value is
/// read whole and converted on its own, so a harness that changes one
/// field's type loses that field and nothing else.
///
/// # Errors
/// Only on malformed JSON, which fails the whole parse anyway.
fn or_none<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    Ok(serde_json::from_value(serde_json::Value::deserialize(d)?).ok())
}

/// [`or_none`] for a field whose type is a struct: a JSON object, or `None`.
///
/// serde also reads a struct from an array, field by position, and with
/// every field lenient any array passes: `"rate_limits": []` switched the
/// line to subscription mode and `"model": ["claude-x", "Sonnet"]` named
/// the model `Sonnet`.
///
/// # Errors
/// Only on malformed JSON, which fails the whole parse anyway.
fn object_or_none<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        object @ serde_json::Value::Object(_) => serde_json::from_value(object).ok(),
        _ => None,
    })
}

/// A numeric string as a finite float. `f64::from_str` also takes `inf`,
/// `infinity` and `NaN`, which JSON itself can never carry, so they are no
/// number here either.
fn finite(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|f| f.is_finite())
}

/// Accept any JSON number (or numeric string) for an unsigned counter:
/// fractional values are floored, negatives clamp to zero; `null` and any
/// other type are `None`.
///
/// # Errors
/// Only on malformed JSON.
fn lenient_u64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        U(u64),
        I(i64),
        F(f64),
        S(String),
        Other(IgnoredAny),
    }
    Ok(match Option::<Num>::deserialize(d)? {
        None | Some(Num::Other(_)) => None,
        Some(Num::U(n)) => Some(n),
        Some(Num::I(n)) => Some(u64::try_from(n).unwrap_or(0)),
        Some(Num::F(f)) => Some(crate::num::floor_to_u64(f)),
        Some(Num::S(s)) => finite(&s).map(crate::num::floor_to_u64),
    })
}

/// Accept any JSON number (or numeric string) for a signed epoch timestamp;
/// `null` and any other type are `None`.
///
/// # Errors
/// Only on malformed JSON.
fn lenient_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        I(i64),
        F(f64),
        S(String),
        Other(IgnoredAny),
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
        None | Some(Num::Other(_)) => None,
        Some(Num::I(n)) => Some(n),
        Some(Num::F(f)) => Some(to_i64(f)),
        Some(Num::S(s)) => finite(&s).map(to_i64),
    })
}

/// Accept any JSON number (or numeric string) for a float; `null` and any
/// other type are `None`. The result is always finite; how large a number
/// a formatter prints is `num::shown_amount`'s business.
///
/// # Errors
/// Only on malformed JSON.
fn lenient_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        F(f64),
        S(String),
        Other(IgnoredAny),
    }
    Ok(match Option::<Num>::deserialize(d)? {
        None | Some(Num::Other(_)) => None,
        Some(Num::F(f)) => Some(f).filter(|f| f.is_finite()),
        Some(Num::S(s)) => finite(&s),
    })
}

/// A list of strings: `null`, or anything else that is not a list, is no
/// entries, and an entry that is not a string is skipped.
///
/// # Errors
/// Only on malformed JSON.
fn string_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

/// Top-level payload.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Payload {
    /// Current working directory (same as `workspace.current_dir`).
    #[serde(deserialize_with = "or_none")]
    pub cwd: Option<String>,
    /// Stable session identifier; used as the cache key.
    #[serde(deserialize_with = "or_none")]
    pub session_id: Option<String>,
    /// Custom or AI-generated session name. Absent for default names.
    #[serde(deserialize_with = "or_none")]
    pub session_name: Option<String>,
    /// Claude Code version.
    #[serde(deserialize_with = "or_none")]
    pub version: Option<String>,
    /// Model identity.
    #[serde(deserialize_with = "object_or_none")]
    pub model: Option<Model>,
    /// Workspace directories and repo identity.
    #[serde(deserialize_with = "object_or_none")]
    pub workspace: Option<Workspace>,
    /// Output style.
    #[serde(deserialize_with = "object_or_none")]
    pub output_style: Option<OutputStyle>,
    /// Cost and duration counters.
    #[serde(deserialize_with = "object_or_none")]
    pub cost: Option<Cost>,
    /// Context window usage.
    #[serde(deserialize_with = "object_or_none")]
    pub context_window: Option<ContextWindow>,
    /// Whether the last response exceeded 200k tokens in total.
    #[serde(deserialize_with = "or_none")]
    pub exceeds_200k_tokens: Option<bool>,
    /// Prompt cache statistics (Claude Code ≥ 2.1.251).
    #[serde(deserialize_with = "object_or_none")]
    pub prompt_cache: Option<PromptCache>,
    /// Fast mode enabled.
    #[serde(deserialize_with = "or_none")]
    pub fast_mode: Option<bool>,
    /// Reasoning effort; absent when the model does not support it.
    #[serde(deserialize_with = "object_or_none")]
    pub effort: Option<Effort>,
    /// Extended thinking state.
    #[serde(deserialize_with = "object_or_none")]
    pub thinking: Option<Thinking>,
    /// Rate limits; present only for subscription users after the first API response.
    #[serde(deserialize_with = "object_or_none")]
    pub rate_limits: Option<RateLimits>,
    /// Vim mode; present only when vim mode is enabled.
    #[serde(deserialize_with = "object_or_none")]
    pub vim: Option<Vim>,
    /// Agent identity when running with `--agent`.
    #[serde(deserialize_with = "object_or_none")]
    pub agent: Option<Agent>,
    /// Open pull/merge request for the current branch.
    #[serde(deserialize_with = "object_or_none")]
    pub pr: Option<Pr>,
    /// Claude Code worktree session.
    #[serde(deserialize_with = "object_or_none")]
    pub worktree: Option<Worktree>,
}

/// `model` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Model {
    /// Model identifier, e.g. `claude-opus-5`.
    #[serde(deserialize_with = "or_none")]
    pub id: Option<String>,
    /// Human-readable name, e.g. `Opus`.
    #[serde(deserialize_with = "or_none")]
    pub display_name: Option<String>,
}

/// `workspace` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Workspace {
    /// Current directory.
    #[serde(deserialize_with = "or_none")]
    pub current_dir: Option<String>,
    /// Directory where Claude Code was launched.
    #[serde(deserialize_with = "or_none")]
    pub project_dir: Option<String>,
    /// Directories added with `/add-dir`.
    #[serde(deserialize_with = "string_list")]
    pub added_dirs: Vec<String>,
    /// Linked git worktree name; absent in the main working tree.
    #[serde(deserialize_with = "or_none")]
    pub git_worktree: Option<String>,
    /// Repository identity parsed from `origin`.
    #[serde(deserialize_with = "object_or_none")]
    pub repo: Option<Repo>,
}

/// `workspace.repo` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Repo {
    /// Host, e.g. `github.com`.
    #[serde(deserialize_with = "or_none")]
    pub host: Option<String>,
    /// Owner or namespace.
    #[serde(deserialize_with = "or_none")]
    pub owner: Option<String>,
    /// Repository name.
    #[serde(deserialize_with = "or_none")]
    pub name: Option<String>,
}

/// `output_style` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OutputStyle {
    /// Style name, e.g. `default`.
    #[serde(deserialize_with = "or_none")]
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
    /// No module reads it yet.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_input_tokens: Option<u64>,
    /// Output tokens from the most recent response. No module reads it yet.
    #[serde(deserialize_with = "lenient_u64")]
    pub total_output_tokens: Option<u64>,
    /// Window size in tokens: 200 000 or 1 000 000.
    #[serde(deserialize_with = "lenient_u64")]
    pub context_window_size: Option<u64>,
    /// Percentage used, computed from input tokens only. Null early on.
    #[serde(deserialize_with = "lenient_f64")]
    pub used_percentage: Option<f64>,
    /// Per-component usage of the last API call. Null before the first call.
    #[serde(deserialize_with = "object_or_none")]
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

/// `prompt_cache` object. The counters no module reads yet are the ones a
/// cache module would show next, and say so.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct PromptCache {
    /// Whether the cached prefix is within its TTL.
    #[serde(deserialize_with = "or_none")]
    pub warm: Option<bool>,
    /// Whether any response reported cache tokens.
    #[serde(deserialize_with = "or_none")]
    pub caching_observed: Option<bool>,
    /// Cache lifetime: `"5m"` or `"1h"`.
    #[serde(deserialize_with = "or_none")]
    pub ttl: Option<String>,
    /// Epoch seconds when the cached prefix goes cold.
    #[serde(deserialize_with = "lenient_i64")]
    pub expires_at: Option<i64>,
    /// API requests made for the main conversation. No module reads it yet.
    #[serde(deserialize_with = "lenient_u64")]
    pub requests: Option<u64>,
    /// Requests that re-processed content the cache already held.
    #[serde(deserialize_with = "lenient_u64")]
    pub misses: Option<u64>,
    /// Cache rebuilds after compaction or tool-result clearing. No module
    /// reads it yet.
    #[serde(deserialize_with = "lenient_u64")]
    pub expected_rebuilds: Option<u64>,
    /// Cache read tokens as a fraction of all input tokens (0..1).
    #[serde(deserialize_with = "lenient_f64")]
    pub hit_ratio: Option<f64>,
    /// Tokens written to the cache this session.
    #[serde(deserialize_with = "lenient_u64")]
    pub cache_write_tokens: Option<u64>,
    /// Tokens written by requests counted as misses. No module reads it yet.
    #[serde(deserialize_with = "lenient_u64")]
    pub miss_recache_tokens: Option<u64>,
    /// Epoch seconds of the last miss. No module reads it yet.
    #[serde(deserialize_with = "lenient_i64")]
    pub last_miss_at: Option<i64>,
    /// Tokens the next request would re-cache if cold. No module reads it
    /// yet.
    #[serde(deserialize_with = "lenient_u64")]
    pub recache_tokens_if_cold: Option<u64>,
}

/// `effort` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Effort {
    /// `low`, `medium`, `high`, `xhigh`, or `max`.
    #[serde(deserialize_with = "or_none")]
    pub level: Option<String>,
}

/// `thinking` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Thinking {
    /// Whether extended thinking is enabled.
    #[serde(deserialize_with = "or_none")]
    pub enabled: Option<bool>,
}

/// `rate_limits` object. Each window may be independently absent.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RateLimits {
    /// Rolling five-hour window.
    #[serde(deserialize_with = "object_or_none")]
    pub five_hour: Option<RateWindow>,
    /// Rolling seven-day window.
    #[serde(deserialize_with = "object_or_none")]
    pub seven_day: Option<RateWindow>,
    /// Gateway spend limit; percentage may exceed 100.
    #[serde(deserialize_with = "object_or_none")]
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
    #[serde(deserialize_with = "or_none")]
    pub mode: Option<String>,
}

/// `agent` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Agent {
    /// Agent name.
    #[serde(deserialize_with = "or_none")]
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
    #[serde(deserialize_with = "or_none")]
    pub url: Option<String>,
    /// `approved`, `pending`, `changes_requested`, or `draft`.
    #[serde(deserialize_with = "or_none")]
    pub review_state: Option<String>,
    /// `mr` for GitLab merge requests; absent for GitHub.
    #[serde(deserialize_with = "or_none")]
    pub kind: Option<String>,
}

/// `worktree` object (Claude Code worktree session).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Worktree {
    /// Worktree name.
    #[serde(deserialize_with = "or_none")]
    pub name: Option<String>,
    /// Absolute path. No module reads it yet.
    #[serde(deserialize_with = "or_none")]
    pub path: Option<String>,
    /// Branch checked out in the worktree.
    #[serde(deserialize_with = "or_none")]
    pub branch: Option<String>,
    /// Directory before entering the worktree. No module reads it yet.
    #[serde(deserialize_with = "or_none")]
    pub original_cwd: Option<String>,
    /// Branch before entering the worktree.
    #[serde(deserialize_with = "or_none")]
    pub original_branch: Option<String>,
}

impl Payload {
    /// Parse the JSON payload. Unknown fields are ignored, every field is
    /// optional, and a known field of the wrong type is `None` (SPEC § 5).
    ///
    /// # Errors
    /// Returns the serde error when the input is not a JSON object:
    /// malformed JSON, or valid JSON of another type.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        // serde also reads a struct from a JSON array, field by position, and
        // with every field lenient any array would pass for a payload.
        if !json.trim_start_matches([' ', '\t', '\n', '\r']).starts_with('{') {
            return Err(serde::de::Error::custom("the payload is not a JSON object"));
        }
        serde_json::from_str(json)
    }

    /// True when the harness reports subscription rate limits.
    #[must_use]
    pub const fn is_subscription(&self) -> bool {
        self.rate_limits.is_some()
    }

    /// The current directory, preferring `workspace.current_dir`; an empty
    /// one is no directory, so it never shadows the other.
    #[must_use]
    pub fn current_dir(&self) -> Option<&str> {
        self.workspace
            .as_ref()
            .and_then(|w| w.current_dir.as_deref())
            .filter(|p| !p.is_empty())
            .or_else(|| self.cwd.as_deref().filter(|p| !p.is_empty()))
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
        let p = Payload::parse(r#"{"pr": {"number": true, "url": "https://x"}}"#).unwrap();
        assert_eq!(p.pr.as_ref().unwrap().number, None);
        assert_eq!(p.pr.unwrap().url.as_deref(), Some("https://x"), "the siblings stay");
    }

    /// SPEC § 5: a field of the wrong type is absent, alone. A harness that
    /// changes one field's type loses that field, never the status line.
    #[test]
    fn a_wrong_typed_field_loses_only_itself() {
        let p = Payload::parse(
            r#"{"pr": {"number": true}, "thinking": true, "effort": "high", "session_name": 7,
                "exceeds_200k_tokens": "false", "workspace": {"added_dirs": ["/a", null, 3, "/b"]},
                "model": {"id": 5, "display_name": "Opus"}, "cost": {"total_cost_usd": {"x": 1},
                "total_lines_added": [1], "total_lines_removed": 4}, "vim": {"mode": false},
                "rate_limits": {"five_hour": "full", "seven_day": {"used_percentage": 12}},
                "agent": "planner", "version": 2.1, "fast_mode": true}"#,
        )
        .unwrap();
        assert_eq!(p.model.as_ref().unwrap().display_name.as_deref(), Some("Opus"));
        assert_eq!(p.model.as_ref().unwrap().id, None);
        assert_eq!(p.thinking, None);
        assert_eq!(p.effort, None);
        assert_eq!(p.session_name, None);
        assert_eq!(p.exceeds_200k_tokens, None);
        assert_eq!(p.workspace.unwrap().added_dirs, ["/a", "/b"]);
        assert_eq!(p.pr.unwrap().number, None);
        let cost = p.cost.unwrap();
        assert_eq!((cost.total_cost_usd, cost.total_lines_added), (None, None));
        assert_eq!(cost.total_lines_removed, Some(4));
        assert_eq!(p.vim.unwrap().mode, None);
        let rl = p.rate_limits.unwrap();
        assert_eq!(rl.five_hour, None);
        assert_eq!(rl.seven_day.unwrap().used_percentage, Some(12.0));
        assert_eq!((p.agent, p.version, p.fast_mode), (None, None, Some(true)));
        // A list that is not a list is no entries.
        let p = Payload::parse(r#"{"workspace": {"added_dirs": "/a", "project_dir": "/p"}}"#);
        let ws = p.unwrap().workspace.unwrap();
        assert_eq!(ws.added_dirs, Vec::<String>::new());
        assert_eq!(ws.project_dir.as_deref(), Some("/p"));
        // Fields no module reads parse whatever their shape.
        let p = Payload::parse(
            r#"{"transcript_path": {"x": 1}, "prompt_id": 5, "worktree": {"path": [1], "name": "w"}}"#,
        )
        .unwrap();
        assert_eq!(p.worktree.unwrap().name.as_deref(), Some("w"));
    }

    /// `inf`, `infinity` and `NaN` are numbers to `f64::from_str` but never
    /// to JSON: a numeric string that spells one is absent, so no formatter
    /// sees a value that is not finite.
    #[test]
    fn a_numeric_string_that_is_not_finite_is_absent() {
        let p = Payload::parse(
            r#"{"cost": {"total_cost_usd": "inf", "total_duration_ms": "Infinity"},
                "rate_limits": {"spend_limit": {"used_percentage": "NaN", "resets_at": "-inf"}},
                "prompt_cache": {"hit_ratio": " nan "}}"#,
        )
        .unwrap();
        let cost = p.cost.unwrap();
        assert_eq!((cost.total_cost_usd, cost.total_duration_ms), (None, None));
        let spend = p.rate_limits.unwrap().spend_limit.unwrap();
        assert_eq!((spend.used_percentage, spend.resets_at), (None, None));
        assert_eq!(p.prompt_cache.unwrap().hit_ratio, None);
        // A huge finite number is a number: the formatters bound it.
        let p = Payload::parse(r#"{"cost": {"total_cost_usd": 1e300}}"#).unwrap();
        assert_eq!(p.cost.unwrap().total_cost_usd, Some(1e300));
    }

    /// SPEC § 5: serde reads a struct from a JSON array too, field by
    /// position, so `rate_limits: []` switched the line to subscription
    /// mode (the cost hidden) and `model: ["claude-x", "Sonnet"]` named the
    /// model `Sonnet`. An array where an object belongs is absent.
    #[test]
    fn an_array_where_an_object_belongs_is_absent() {
        let p = Payload::parse(r#"{"rate_limits": [], "model": ["claude-x", "Sonnet"]}"#).unwrap();
        assert!(!p.is_subscription(), "rate_limits: [] made this a subscription");
        assert_eq!(p.model, None, "model read by position: {:?}", p.model);
        let p = Payload::parse(r#"{"rate_limits": {"five_hour": [[42, 1738430000]]}}"#).unwrap();
        assert_eq!(p.rate_limits.unwrap().five_hour, None);
    }

    /// Every field the payload carries, each set to a value of its own
    /// type: what [`every_field_of_the_wrong_type_is_absent_alone`] sweeps.
    const FULL: &str = r#"{
        "cwd": "/w", "session_id": "s1", "session_name": "n", "version": "2.1.270",
        "model": {"id": "claude-x", "display_name": "Opus"},
        "workspace": {"current_dir": "/w", "project_dir": "/p", "added_dirs": ["/a"],
            "git_worktree": "wt", "repo": {"host": "github.com", "owner": "o", "name": "r"}},
        "output_style": {"name": "default"},
        "cost": {"total_cost_usd": 1.5, "total_duration_ms": 1000, "total_api_duration_ms": 500,
            "total_lines_added": 3, "total_lines_removed": 2},
        "context_window": {"total_input_tokens": 10, "total_output_tokens": 5,
            "context_window_size": 200000, "used_percentage": 12.5,
            "current_usage": {"input_tokens": 1, "output_tokens": 2,
                "cache_creation_input_tokens": 3, "cache_read_input_tokens": 4}},
        "exceeds_200k_tokens": false,
        "prompt_cache": {"warm": true, "caching_observed": true, "ttl": "5m",
            "expires_at": 1738425600, "requests": 4, "misses": 1, "expected_rebuilds": 0,
            "hit_ratio": 0.5, "cache_write_tokens": 10, "miss_recache_tokens": 2,
            "last_miss_at": 1738425000, "recache_tokens_if_cold": 7},
        "fast_mode": true, "effort": {"level": "high"}, "thinking": {"enabled": true},
        "rate_limits": {"five_hour": {"used_percentage": 42, "resets_at": 1738430000},
            "seven_day": {"used_percentage": 7, "resets_at": 1738900000},
            "spend_limit": {"used_percentage": 120, "resets_at": 1739000000}},
        "vim": {"mode": "NORMAL"}, "agent": {"name": "planner"},
        "pr": {"number": 7, "url": "https://x", "review_state": "approved", "kind": "mr"},
        "worktree": {"name": "w", "path": "/wt", "branch": "b", "original_cwd": "/o",
            "original_branch": "main"}
    }"#;

    /// The path of every key of every object in `value`, depth first.
    fn key_paths(value: &serde_json::Value, prefix: &[String], out: &mut Vec<Vec<String>>) {
        let serde_json::Value::Object(map) = value else { return };
        for (key, child) in map {
            let mut path = prefix.to_vec();
            path.push(key.clone());
            out.push(path.clone());
            key_paths(child, &path, out);
        }
    }

    /// `root` with the key at `path` set to `new`, or removed for `None`.
    fn with_key(
        root: &serde_json::Value,
        path: &[String],
        new: Option<&serde_json::Value>,
    ) -> serde_json::Value {
        let mut root = root.clone();
        let (last, parents) = path.split_last().unwrap();
        let mut here = &mut root;
        for key in parents {
            here = here.get_mut(key).unwrap();
        }
        let map = here.as_object_mut().unwrap();
        match new {
            Some(v) => {
                map.insert(last.clone(), v.clone());
            }
            None => {
                map.remove(last);
            }
        }
        root
    }

    /// SPEC § 5, for every field rather than chosen examples (c-review C4:
    /// a guard taken off `session_id` left the suite green while `{"session_id":
    /// 5}` blanked every row): each key of a payload that sets every field,
    /// given a value of each other JSON type, parses as if the key were
    /// absent, so that field alone is lost and its siblings are unchanged.
    #[test]
    fn every_field_of_the_wrong_type_is_absent_alone() {
        use serde_json::{Value, json};
        let full: Value = serde_json::from_str(FULL).unwrap();
        let parsed = Payload::parse(FULL).unwrap();
        let shown = format!("{parsed:?}");
        assert!(!shown.contains("None"), "FULL leaves a field unset: {shown}");
        let kind = |v: &Value| std::mem::discriminant(v);
        let wrong = [
            json!(5),
            json!(-2.5),
            json!("x"),
            json!(true),
            json!([]),
            json!(["claude-x", 1]),
            json!({"k": 1}),
            json!(null),
        ];
        let mut paths = Vec::new();
        key_paths(&full, &[], &mut paths);
        assert!(paths.len() > 70, "{}", paths.len());
        for path in &paths {
            let mut here = &full;
            for key in path {
                here = &here[key];
            }
            let absent = serde_json::to_string(&with_key(&full, path, None)).unwrap();
            let absent = Payload::parse(&absent).unwrap();
            assert_ne!(absent, parsed, "{path:?}: removing it changes nothing");
            for value in wrong.iter().filter(|w| kind(w) != kind(here) || w.is_null()) {
                let text = serde_json::to_string(&with_key(&full, path, Some(value))).unwrap();
                let got = Payload::parse(&text)
                    .unwrap_or_else(|e| panic!("{path:?} = {value}: the payload failed: {e}"));
                assert_eq!(got, absent, "{path:?} = {value}");
            }
        }
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
        // An empty directory is no directory: it never shadows the other.
        let p = Payload::parse(r#"{"cwd": "/a", "workspace": {"current_dir": ""}}"#).unwrap();
        assert_eq!(p.current_dir(), Some("/a"));
        assert_eq!(p.project_dir(), Some("/a"));
        let p = Payload::parse(r#"{"cwd": "", "workspace": {"current_dir": ""}}"#).unwrap();
        assert_eq!((p.current_dir(), p.project_dir()), (None, None));
        let p = Payload::parse(r#"{"cwd": ""}"#).unwrap();
        assert_eq!((p.current_dir(), p.project_dir()), (None, None));
    }

    #[test]
    fn not_an_object_is_an_error() {
        for input in ["[1,2]", r#"["/a", "s"]"#, "", "null", "5", "\"x\"", "{", r#"{"cwd": "/a""#] {
            assert!(Payload::parse(input).is_err(), "{input:?}");
        }
        assert!(Payload::parse(" \n{}").is_ok());
    }
}
