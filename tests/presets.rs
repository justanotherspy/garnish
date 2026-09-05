//! Every file under `presets/` is a complete, valid config with the header
//! SPEC § 12 asks for, and renders at its declared width.

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn header(text: &str, key: &str) -> Option<String> {
    let prefix = format!("# {key}: ");
    text.lines().find_map(|l| l.strip_prefix(&prefix).map(str::to_owned))
}

#[test]
fn every_preset_has_a_header_validates_and_renders() {
    let dir = root().join("presets");
    let mut names = BTreeSet::new();
    let mut count = 0;
    let mut failures: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        count += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let name = header(&text, "name").unwrap_or_else(|| panic!("{stem}: no `# name:`"));
        assert_eq!(name, stem, "{stem}: header name must match the filename");
        assert!(names.insert(name), "{stem}: duplicate name");
        assert!(header(&text, "summary").is_some(), "{stem}: no `# summary:`");
        let columns: usize = header(&text, "columns")
            .and_then(|c| c.parse().ok())
            .unwrap_or_else(|| panic!("{stem}: `# columns:` must be an integer"));
        assert!((60..=400).contains(&columns), "{stem}: columns {columns} out of range");

        let check = Command::new(env!("CARGO_BIN_EXE_garnish"))
            .args(["--config", path.to_str().unwrap(), "config", "check"])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{stem}: config check failed:\n{}",
            String::from_utf8_lossy(&check.stdout)
        );

        let tmp = tempfile::tempdir().unwrap();
        let render = Command::new(env!("CARGO_BIN_EXE_garnish"))
            .args([
                "--config",
                path.to_str().unwrap(),
                "preview",
                root().join("tests/fixtures/payloads/subscription-full.json").to_str().unwrap(),
                "--color",
                "never",
                "--width",
                &columns.to_string(),
            ])
            .env("GARNISH_NOW", "1738425600")
            .env("GARNISH_CACHE_DIR", tmp.path().join("cache"))
            .env("GARNISH_NO_SPAWN", "1")
            .env("HOME", "/home/dev")
            .env_remove("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
            .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
            .env_remove("DISABLE_AUTO_COMPACT")
            .env_remove("DISABLE_COMPACT")
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&render.stdout);
        assert!(render.status.success(), "{stem}: preview failed:\n{out}");
        // The first line is preview's `── name` header; the rest is the render,
        // which at the declared width must fit Claude Code's box uncut (SPEC § 12).
        let rows: Vec<&str> = out.lines().skip(1).collect();
        assert!(!rows.is_empty(), "{stem}: preview printed nothing:\n{out}");
        for row in &rows {
            if row.contains('…') {
                failures.push(format!("{stem}: cut at its declared width {columns}:\n{row}"));
            }
            let width = garnish::ansi::display_width(row);
            if width > columns - 4 {
                failures.push(format!(
                    "{stem}: {width} cells, wider than the {}-cell box at {columns} columns:\n{row}",
                    columns - 4
                ));
            }
        }
    }
    assert!(count >= 10, "expected the seed presets, found {count}");
    assert!(
        failures.is_empty(),
        "{} preset(s) do not fit:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
