//! The presets gallery (SPEC § 12): every `presets/<name>.toml` embedded in
//! the binary, so `garnish presets` and `garnish config init --preset <name>`
//! work from a `cargo install` without the repository at hand.
//!
//! Each file starts with a comment header the tooling parses (`# name:`,
//! `# summary:`, `# columns:`, optional `# needs:` and `# author:`); the rest
//! is an ordinary config. `tests/presets.rs` checks the files on disk, and a
//! unit test here checks that this table lists exactly those files.

use std::sync::LazyLock;

/// The embedded files, `(name, text)`, in alphabetical order (the unit test
/// compares against the sorted directory listing).
const FILES: [(&str, &str); 15] = [
    ("animated-dots", include_str!("../presets/animated-dots.toml")),
    ("bars-and-limits", include_str!("../presets/bars-and-limits.toml")),
    ("compact-aligned", include_str!("../presets/compact-aligned.toml")),
    ("dracula-256", include_str!("../presets/dracula-256.toml")),
    ("emoji-overrides", include_str!("../presets/emoji-overrides.toml")),
    ("full-aligned", include_str!("../presets/full-aligned.toml")),
    ("labels-and-placeholders", include_str!("../presets/labels-and-placeholders.toml")),
    ("minimal-clean", include_str!("../presets/minimal-clean.toml")),
    ("motd-ticker", include_str!("../presets/motd-ticker.toml")),
    ("packed-heavy", include_str!("../presets/packed-heavy.toml")),
    ("session-detail", include_str!("../presets/session-detail.toml")),
    ("single-line-full", include_str!("../presets/single-line-full.toml")),
    ("tall-eight-lines", include_str!("../presets/tall-eight-lines.toml")),
    ("three-lines-double", include_str!("../presets/three-lines-double.toml")),
    ("two-lines-powerline", include_str!("../presets/two-lines-powerline.toml")),
];

/// The header keys the tooling owns; `body` strips them from a written file.
const HEADER_KEYS: [&str; 5] = ["name", "summary", "columns", "needs", "author"];

/// Terminal width a preset is rendered at when its header names none.
pub const DEFAULT_COLUMNS: usize = 120;

/// One gallery preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    /// Kebab-case name, equal to the file stem.
    pub name: &'static str,
    /// One-line summary.
    pub summary: String,
    /// The terminal width the sample is rendered at.
    pub columns: usize,
    /// `nerd-font`, `emoji`, or nothing.
    pub needs: Option<String>,
    /// GitHub handle of the contributor, when given.
    pub author: Option<String>,
    /// The whole file, header included.
    pub source: &'static str,
}

/// Every gallery preset, in table order.
pub static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|(name, source)| Preset {
            name,
            summary: header(source, "summary").unwrap_or("").to_owned(),
            columns: header(source, "columns")
                .and_then(|c| c.parse().ok())
                .unwrap_or(DEFAULT_COLUMNS),
            needs: header(source, "needs").map(str::to_owned),
            author: header(source, "author").map(str::to_owned),
            source,
        })
        .collect()
});

/// The preset called `name`, if any.
#[must_use]
pub fn find(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// The header block: the comment lines before the first blank line (SPEC
/// § 12, "each file starts with a comment header"). A `# needs: …` remark
/// further down is an ordinary comment.
fn header_block(text: &str) -> &str {
    text.split("\n\n").next().unwrap_or(text)
}

fn is_tooling_line(line: &str) -> bool {
    HEADER_KEYS.iter().any(|k| line.starts_with(&format!("# {k}:")))
}

/// The value of a `# key: value` line in the header block.
#[must_use]
pub fn header<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("# {key}: ");
    header_block(text).lines().find_map(|l| l.strip_prefix(prefix.as_str())).map(str::trim)
}

/// The file without the tooling lines of its header block and the blank
/// lines that follow them: what `config init --preset <name>` writes.
#[must_use]
pub fn body(text: &str) -> String {
    let head = header_block(text);
    let rest = text.get(head.len()..).unwrap_or("");
    let lines = head
        .lines()
        .filter(|l| !is_tooling_line(l))
        .chain(rest.lines())
        .skip_while(|l| l.trim().is_empty());
    let mut out = String::with_capacity(text.len());
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::modules::SCHEMAS;

    /// The table lists exactly the files under `presets/`, every header is
    /// complete, and every body is a config without problems.
    #[test]
    fn embedded_presets_match_the_directory_and_validate() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let embedded: Vec<&str> = PRESETS.iter().map(|p| p.name).collect();
        assert_eq!(
            embedded, on_disk,
            "src/gallery.rs FILES must list every presets/*.toml, alphabetically"
        );
        for preset in PRESETS.iter() {
            assert_eq!(header(preset.source, "name"), Some(preset.name), "{}", preset.name);
            assert!(!preset.summary.is_empty(), "{}: no summary", preset.name);
            let declared: Option<usize> =
                header(preset.source, "columns").and_then(|c| c.parse().ok());
            assert_eq!(
                declared,
                Some(preset.columns),
                "{}: `# columns:` must be an integer",
                preset.name
            );
            assert!((60..=400).contains(&preset.columns), "{}: columns", preset.name);
            let body = body(preset.source);
            assert!(!body.contains("# name:") && !body.contains("# columns:"), "{body}");
            let (_, errs) = config::parse(&body, &SCHEMAS);
            assert_eq!(errs, Vec::new(), "{}", preset.name);
            // The explanatory comments (anything that is not a tooling line) stay.
            let comments = preset
                .source
                .lines()
                .filter(|l| {
                    l.starts_with("# ")
                        && !HEADER_KEYS.iter().any(|k| l.starts_with(&format!("# {k}:")))
                })
                .count();
            assert_eq!(
                body.lines().filter(|l| l.starts_with("# ")).count(),
                comments,
                "{}",
                preset.name
            );
        }
        assert!(find("minimal-clean").is_some() && find("nope").is_none());
        // Only the header block is tooling: a `# needs:` remark in the body stays.
        let text = "# name: x\n# summary: s\n# columns: 80\n\n# needs: a wide terminal, really\npreset = \"minimal\"\n";
        assert_eq!(body(text), "# needs: a wide terminal, really\npreset = \"minimal\"\n");
        assert_eq!(header(text, "needs"), None);
        assert_eq!(find("motd-ticker").unwrap().needs.as_deref(), Some("nerd-font"));
    }
}
