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

/// At its declared width a preset fits Claude Code's box uncut (SPEC § 12):
/// no `…`, no row wider than `columns − 4`.
fn fit_failures(label: &str, columns: usize, rows: &[String]) -> Vec<String> {
    let box_width = columns.saturating_sub(4);
    let mut failures = Vec::new();
    for row in rows {
        if row.contains('…') {
            failures.push(format!("{label}: cut at its declared width {columns}:\n{row}"));
        }
        let width = garnish::ansi::display_width(row);
        if width > box_width {
            failures.push(format!(
                "{label}: {width} cells, wider than the {box_width}-cell box at {columns} columns:\n{row}"
            ));
        }
    }
    failures
}

/// A line ticker slides exactly `ticker_step` cells per tick: after the
/// frame's fixed prefix, the 601 window is the 600 window shifted by that
/// many cells (whole-stack review: with `compact` durations the period
/// changes between ticks and the window jumps; the presets use `fixed`).
fn ticker_advance_failures(
    stem: &str,
    text: &str,
    rows: &[String],
    later: &[String],
) -> Vec<String> {
    // Not a `#[test]` fn, so the crate's panic-path lints apply here.
    let cells: usize = text
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("ticker_step"))
        .and_then(|rest| rest.trim_start_matches([' ', '=']).trim().parse().ok())
        .unwrap_or(1);
    let mut failures = Vec::new();
    for (before, after) in rows.iter().zip(later) {
        let a: Vec<char> = before.chars().collect();
        let b: Vec<char> = after.chars().collect();
        if a == b {
            continue; // a line that fits does not scroll
        }
        let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let shifted = prefix.saturating_add(cells);
        let window = 20.min(a.len().saturating_sub(shifted).saturating_sub(1));
        let moved = a.get(shifted..shifted.saturating_add(window));
        let stayed = b.get(prefix..prefix.saturating_add(window));
        if window < 5 || moved != stayed {
            failures.push(format!(
                "{stem}: the ticker did not advance exactly {cells} cell(s) between two ticks:\n{before}\n{after}"
            ));
        }
    }
    failures
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
        let render_at = |now: &str| {
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
                .env("GARNISH_NOW", now)
                .env("GARNISH_CACHE_DIR", tmp.path().join("cache"))
                .env("GARNISH_NO_SPAWN", "1")
                .env("HOME", "/home/dev")
                .env_remove("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
                .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
                .env_remove("DISABLE_AUTO_COMPACT")
                .env_remove("DISABLE_COMPACT")
                .output()
                .unwrap();
            let out = String::from_utf8_lossy(&render.stdout).into_owned();
            assert!(render.status.success(), "{stem}: preview failed:\n{out}");
            // The first line is preview's `── name` header; the rest is the render.
            out.lines().skip(1).map(str::to_owned).collect::<Vec<String>>()
        };
        let rows = render_at("1738425600");
        assert!(!rows.is_empty(), "{stem}: preview printed nothing");
        // At the declared width the render must fit Claude Code's box uncut
        // (SPEC § 12). A ticker preset never shows `…`, so for it the promise
        // is different: its row is exactly the box and it moves between ticks.
        // A preset that promises movement (a ticker, a rule pattern, separator
        // or icon frames) must render differently one second later.
        let ticker = text.lines().any(|l| {
            let l = l.trim_start();
            (l.starts_with("overflow") && l.contains("\"ticker\""))
                || l.starts_with("fill_pattern")
                || l.starts_with("separator_frames")
                || (l.contains("_frames") && !l.starts_with('#'))
        });
        // The fit promise holds at every instant, not only the first: a
        // compact duration changes width between ticks (whole-stack review).
        let later = render_at("1738425601");
        let much_later = render_at("1738425605");
        for (when, rows) in [("600", &rows), ("601", &later), ("605", &much_later)] {
            failures.extend(fit_failures(&format!("{stem}@{when}"), columns, rows));
        }
        if ticker {
            if later == rows {
                failures.push(format!(
                    "{stem}: promises an animation but nothing moves between two ticks at {columns} columns"
                ));
            }
            if text.lines().any(|l| l.trim_start().starts_with("overflow")) {
                failures.extend(ticker_advance_failures(&stem, &text, &rows, &later));
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
