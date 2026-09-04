//! The JSON payload Claude Code writes to the status line command's stdin.
//!
//! Every field that the docs list as "may be absent" or "may be null" is an
//! `Option`. Unknown fields are ignored so newer Claude Code versions never
//! break parsing. See `SPEC.md` § 2.2 for the contract.

use serde::Deserialize;

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
    pub total_cost_usd: Option<f64>,
    /// Wall-clock milliseconds since the session started.
    pub total_duration_ms: Option<u64>,
    /// Milliseconds spent waiting on the API.
    pub total_api_duration_ms: Option<u64>,
    /// Lines added this session.
    pub total_lines_added: Option<u64>,
    /// Lines removed this session.
    pub total_lines_removed: Option<u64>,
}

/// `context_window` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ContextWindow {
    /// Input tokens currently in the window (includes cache reads/writes).
    pub total_input_tokens: Option<u64>,
    /// Output tokens from the most recent response.
    pub total_output_tokens: Option<u64>,
    /// Window size in tokens: 200 000 or 1 000 000.
    pub context_window_size: Option<u64>,
    /// Percentage used, computed from input tokens only. Null early on.
    pub used_percentage: Option<f64>,
    /// Percentage remaining.
    pub remaining_percentage: Option<f64>,
    /// Per-component usage of the last API call. Null before the first call.
    pub current_usage: Option<CurrentUsage>,
}

/// `context_window.current_usage` object.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CurrentUsage {
    /// Fresh (uncached) input tokens.
    pub input_tokens: Option<u64>,
    /// Output tokens.
    pub output_tokens: Option<u64>,
    /// Tokens written to the prompt cache.
    pub cache_creation_input_tokens: Option<u64>,
    /// Tokens read from the prompt cache.
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
    pub expires_at: Option<i64>,
    /// API requests made for the main conversation.
    pub requests: Option<u64>,
    /// Requests that re-processed content the cache already held.
    pub misses: Option<u64>,
    /// Cache rebuilds after compaction or tool-result clearing.
    pub expected_rebuilds: Option<u64>,
    /// Cache read tokens as a fraction of all input tokens (0..1).
    pub hit_ratio: Option<f64>,
    /// Tokens written to the cache this session.
    pub cache_write_tokens: Option<u64>,
    /// Tokens written by requests counted as misses.
    pub miss_recache_tokens: Option<u64>,
    /// Epoch seconds of the last miss.
    pub last_miss_at: Option<i64>,
    /// Tokens the next request would re-cache if cold.
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
    pub used_percentage: Option<f64>,
    /// Epoch seconds when the window resets.
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
    fn subscription_detected_from_rate_limits() {
        let p = Payload::parse(
            r#"{"rate_limits": {"five_hour": {"used_percentage": 1.5, "resets_at": 10}}}"#,
        )
        .unwrap();
        assert!(p.is_subscription());
        let rl = p.rate_limits.unwrap();
        assert_eq!(rl.five_hour.unwrap().resets_at, Some(10));
        assert!(rl.seven_day.is_none());
    }

    #[test]
    fn current_dir_prefers_workspace() {
        let p = Payload::parse(r#"{"cwd": "/a", "workspace": {"current_dir": "/b"}}"#).unwrap();
        assert_eq!(p.current_dir(), Some("/b"));
        let p = Payload::parse(r#"{"cwd": "/a"}"#).unwrap();
        assert_eq!(p.current_dir(), Some("/a"));
    }

    #[test]
    fn not_an_object_is_an_error() {
        assert!(Payload::parse("[1,2]").is_err());
        assert!(Payload::parse("").is_err());
    }
}
