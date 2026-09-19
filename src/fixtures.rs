//! The payload fixtures embedded in the binary (SPEC § 14).
//!
//! `tests/fixtures/payloads/*.json` are the saved Claude Code payloads every
//! golden renders from. They are embedded here too, so the documentation
//! samples, the benchmarks and the `setup` preview pane all render from the
//! same files, and a `cargo install` with no repository at hand still has a
//! session to show. A unit test keeps this table equal to the directory.

use crate::payload::Payload;

/// One embedded payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixture {
    /// The file stem under `tests/fixtures/payloads/`.
    pub name: &'static str,
    /// What the payload shows, for the preview pane's status line.
    pub summary: &'static str,
    /// The JSON text.
    pub text: &'static str,
}

macro_rules! fixture {
    ($name:literal, $summary:literal) => {
        Fixture {
            name: $name,
            summary: $summary,
            text: include_str!(concat!("../tests/fixtures/payloads/", $name, ".json")),
        }
    };
}

/// Every embedded payload, the ones the preview cycles through first: a
/// subscription with everything set, then what an absent field does to a
/// layout.
pub const FIXTURES: [Fixture; 26] = [
    fixture!("subscription-full", "a Pro/Max session with every field set"),
    fixture!("api-key", "an API key session: cost instead of rate limits"),
    fixture!("pre-first-response", "before the first response: no usage yet"),
    fixture!("no-git", "outside a repository"),
    fixture!("pr-pending", "an open pull request awaiting review"),
    fixture!("pr-approved", "an approved pull request"),
    fixture!("pr-changes-requested", "a pull request with changes requested"),
    fixture!("pr-draft", "a draft pull request"),
    fixture!("pr-mr", "a GitLab merge request"),
    fixture!("pr-absent", "no pull request"),
    fixture!("ctx-1m-96", "a 1M context window at 96 %"),
    fixture!("ctx-1m-80", "a 1M context window at 80 %"),
    fixture!("ctx-1m-50", "a 1M context window at 50 %"),
    fixture!("ctx-1m-3", "a 1M context window at 3 %"),
    fixture!("ctx-200k", "a 200k context window"),
    fixture!("fast-mode", "fast mode on"),
    fixture!("spend-limit", "a spend limit behind a gateway"),
    fixture!("cache-cold", "a cold prompt cache"),
    fixture!("no-effort", "a model without an effort level"),
    fixture!("no-session-name", "a session with the default name"),
    fixture!("output-style", "a custom output style"),
    fixture!("vim", "vim mode on"),
    fixture!("agent", "a named agent"),
    fixture!("worktree-session", "a Claude Code worktree session"),
    fixture!("git-worktree", "a linked git worktree"),
    fixture!("subdir-added-dirs", "a subdirectory with added directories"),
];

/// The fixture called `name`, if any.
#[must_use]
pub fn find(name: &str) -> Option<&'static Fixture> {
    FIXTURES.iter().find(|f| f.name == name)
}

/// The parsed payload of the fixture called `name`, or an empty payload for
/// a name this table does not carry.
#[must_use]
pub fn payload(name: &str) -> Payload {
    find(name).and_then(|f| Payload::parse(f.text).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table lists exactly the files on disk, every one parses, and the
    /// summaries are one short line each.
    #[test]
    fn embedded_fixtures_match_the_directory_and_parse() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/payloads");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut embedded: Vec<&str> = FIXTURES.iter().map(|f| f.name).collect();
        embedded.sort_unstable();
        assert_eq!(embedded, on_disk, "src/fixtures.rs must list every payload fixture");
        for f in FIXTURES {
            let text = std::fs::read_to_string(dir.join(format!("{}.json", f.name))).unwrap();
            assert_eq!(text, f.text, "{}: embedded text differs from the file", f.name);
            assert!(Payload::parse(f.text).is_ok(), "{}: does not parse", f.name);
            assert!(!f.summary.is_empty() && f.summary.len() < 60, "{}: summary", f.name);
        }
        assert_eq!(FIXTURES.first().map(|f| f.name), Some("subscription-full"));
        assert!(payload("subscription-full").rate_limits.is_some());
        assert_eq!(payload("nope"), Payload::default());
        assert!(find("api-key").is_some());
    }
}
