//! The committed `docs/` must match what `garnish docs` generates.
//! Regenerate with `UPDATE_DOCS=1 cargo nextest run --test docs_sync` (or `make docs`).

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The binary under the rule for tests that run it (CLAUDE.md § Cache and
/// worker invariants, SPEC § 9): its own cache and working directory, no
/// worker, no managed settings file, a fixed home, and none of the
/// developer's `CLAUDE_*`, `DISABLE_*` or `GARNISH_*` variables, so nothing
/// of the machine generating the files can reach them.
fn garnish(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if ["CLAUDE_", "DISABLE_", "GARNISH_"].iter().any(|p| name.starts_with(p)) {
            cmd.env_remove(&key);
        }
    }
    cmd.current_dir(dir)
        .env("GARNISH_CACHE_DIR", dir.join("cache"))
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env("HOME", "/home/dev");
    cmd
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(files(&p));
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
    out.sort();
    out
}

#[test]
fn generated_docs_match_committed_docs() {
    let docs = root().join("docs");
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("docs");
    let cache = tmp.path().join("cache");
    // The two compaction variables are set on purpose: the docs render
    // with the pinned clock, which must not read them.
    let status = garnish(tmp.path())
        .args(["docs", "--out", out.to_str().unwrap()])
        .env("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "100000")
        .env("DISABLE_AUTO_COMPACT", "1")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!cache.exists(), "docs generation must not touch the cache or spawn workers");
    if std::env::var_os("UPDATE_DOCS").is_some() {
        for f in files(&out) {
            let rel = f.strip_prefix(&out).unwrap();
            let dst = docs.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(&f, &dst).unwrap();
        }
        return;
    }
    let mut mismatches = Vec::new();
    for f in files(&out) {
        let rel = f.strip_prefix(&out).unwrap();
        let expected = std::fs::read_to_string(docs.join(rel)).unwrap_or_default();
        let actual = std::fs::read_to_string(&f).unwrap();
        if expected != actual {
            mismatches.push(rel.display().to_string());
        }
    }
    assert!(
        mismatches.is_empty(),
        "docs out of date: {mismatches:?} (run UPDATE_DOCS=1 cargo nextest run --test docs_sync)"
    );

    // The other direction: a committed page nothing generates any more (a
    // renamed module leaves its old page behind) would ship linked from
    // nowhere, and `UPDATE_DOCS=1` only copies, it never deletes.
    // `guide.md` is the one hand-written page (CLAUDE.md § Conventions).
    let generated: std::collections::BTreeSet<PathBuf> =
        files(&out).iter().map(|f| f.strip_prefix(&out).unwrap().into()).collect();
    let orphans: Vec<String> = files(&docs)
        .iter()
        .map(|f| f.strip_prefix(&docs).unwrap().to_path_buf())
        .filter(|rel| rel != Path::new("guide.md") && !generated.contains(rel))
        .map(|rel| rel.display().to_string())
        .collect();
    assert!(orphans.is_empty(), "docs/ pages nothing generates (delete them): {orphans:?}");
}

/// Every rendered status line block in `README.md` is pasted from
/// `docs/config.md`, so a render change that regenerates the docs must be
/// carried into the README by hand; this catches the drift.
#[test]
fn readme_render_blocks_match_generated_docs() {
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    let config_md = std::fs::read_to_string(root().join("docs").join("config.md")).unwrap();
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in readme.lines() {
        match (current.as_mut(), line) {
            (None, "```text") => current = Some(Vec::new()),
            (Some(block), "```") => {
                blocks.push(block.join("\n"));
                current = None;
            }
            (Some(block), l) => block.push(l),
            (None, _) => {}
        }
    }
    let renders: Vec<&String> =
        blocks.iter().filter(|b| b.contains('─') || b.contains("16:00")).collect();
    assert!(renders.len() >= 4, "expected the preset samples in README, found {}", renders.len());
    for block in renders {
        assert!(
            config_md.contains(block.as_str()),
            "README render block is not in docs/config.md (regenerate and paste):\n{block}"
        );
    }
}

/// spec-07: the README's group table and the guide's module table name
/// every module (the guide stopped at 21 when Phase 23 added four).
#[test]
fn readme_and_guide_name_every_module() {
    for page in ["README.md", "docs/guide.md"] {
        let text = std::fs::read_to_string(root().join(page)).unwrap();
        let missing: Vec<&str> = garnish::modules::SCHEMAS
            .iter()
            .map(|s| s.id)
            .filter(|id| !text.contains(&format!("`{id}`")))
            .collect();
        assert!(missing.is_empty(), "{page} never names these modules: {missing:?}");
    }
}

/// The lines of a markdown page outside its fenced code blocks.
fn prose(text: &str) -> Vec<&str> {
    let mut fenced = false;
    text.lines()
        .filter(|line| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                return false;
            }
            !fenced
        })
        .collect()
}

/// The anchors GitHub gives a page's headings: lowercased, everything but
/// letters, digits, `_`, `-` and spaces dropped, spaces turned into `-`,
/// and a repeated slug numbered from `-1`.
fn anchors(text: &str) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    let mut out = std::collections::BTreeSet::new();
    for line in prose(text) {
        let hashes = line.chars().take_while(|&c| c == '#').count();
        let Some(title) = line.get(hashes..).and_then(|t| t.strip_prefix(' ')) else { continue };
        if !(1..=6).contains(&hashes) {
            continue;
        }
        let slug: String = title
            .trim()
            .to_lowercase()
            .chars()
            .filter(|&c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
            .map(|c| if c == ' ' { '-' } else { c })
            .collect();
        let n = seen.entry(slug.clone()).or_default();
        out.insert(if *n == 0 { slug } else { format!("{slug}-{n}") });
        *n = n.saturating_add(1);
    }
    out
}

/// spec-08: every relative link in the README and under `docs/` reaches a
/// file, and every `#anchor` names a heading of its target (two links
/// pointed at `#row-col`, a slug GitHub never makes from `[[row.col]]`).
#[test]
fn every_relative_link_and_anchor_resolves() {
    let mut pages = vec![root().join("README.md")];
    pages.extend(files(&root().join("docs")));
    let mut broken = Vec::new();
    let mut checked = 0_usize;
    for page in &pages {
        let text = std::fs::read_to_string(page).unwrap();
        let dir = page.parent().unwrap();
        for line in prose(&text) {
            for (_, rest) in line.match_indices("](").map(|(i, _)| line.split_at(i + 2)) {
                let Some(link) = rest.split(')').next() else { continue };
                if link.contains("://") || link.starts_with("mailto:") {
                    continue;
                }
                let (path, anchor) = link.split_once('#').unwrap_or((link, ""));
                let target = if path.is_empty() { page.clone() } else { dir.join(path) };
                let name = page.strip_prefix(root()).unwrap().display();
                if !target.exists() {
                    broken.push(format!("{name}: {link} (no such file)"));
                    continue;
                }
                let is_md = target.extension().is_some_and(|e| e == "md");
                if anchor.is_empty() || !is_md {
                    continue;
                }
                checked = checked.saturating_add(1);
                if !anchors(&std::fs::read_to_string(&target).unwrap()).contains(anchor) {
                    broken.push(format!("{name}: {link} (no such heading)"));
                }
            }
        }
    }
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
    // The gallery's index alone links 32 anchors; far fewer means the scan broke.
    assert!(checked > 40, "only {checked} anchors checked");
}

/// The slug rule the link check relies on, on the headings that broke it.
#[test]
fn anchors_follow_githubs_slug_rule() {
    let page = "# Title\n## `[[row.col]]`\n### 7. Troubleshooting\n## Top-level presets\n\
                ```toml\n# name: not-a-heading\n```\n## Title\n#nospace\n";
    let got = anchors(page);
    let want = ["title", "rowcol", "7-troubleshooting", "top-level-presets", "title-1"];
    assert_eq!(got, want.iter().map(|s| (*s).to_owned()).collect());
}

#[test]
fn example_config_matches_config_init() {
    let example = root().join("examples").join("garnish.toml");
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("garnish.toml");
    let status = garnish(tmp.path())
        .args(["--config", target.to_str().unwrap(), "config", "init"])
        .status()
        .unwrap();
    assert!(status.success());
    let generated = std::fs::read_to_string(&target).unwrap();
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::create_dir_all(example.parent().unwrap()).unwrap();
        std::fs::write(&example, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&example).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "examples/garnish.toml is out of date (UPDATE_DOCS=1 regenerates)"
    );
}
