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
    let cache = tmp.path().join("cache");
    let status = Command::new(env!("CARGO_BIN_EXE_garnish"))
        .args(["docs", "--out", tmp.path().to_str().unwrap()])
        .env("GARNISH_CACHE_DIR", &cache)
        .env("GARNISH_NO_SPAWN", "1")
        .env("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "100000")
        .env("DISABLE_AUTO_COMPACT", "1")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!cache.exists(), "docs generation must not touch the cache or spawn workers");
    if std::env::var_os("UPDATE_DOCS").is_some() {
        for f in files(tmp.path()) {
            let rel = f.strip_prefix(tmp.path()).unwrap();
            let dst = docs.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(&f, &dst).unwrap();
        }
        return;
    }
    let mut mismatches = Vec::new();
    for f in files(tmp.path()) {
        let rel = f.strip_prefix(tmp.path()).unwrap();
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

#[test]
fn example_config_matches_config_init() {
    let example = root().join("examples").join("garnish.toml");
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("garnish.toml");
    let status = Command::new(env!("CARGO_BIN_EXE_garnish"))
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
