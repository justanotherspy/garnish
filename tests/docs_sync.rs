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
    let status = Command::new(env!("CARGO_BIN_EXE_garnish"))
        .args(["docs", "--out", tmp.path().to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
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
