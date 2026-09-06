//! Golden renders: every payload fixture × every top-level preset × every icon
//! set, rendered at a fixed width with a frozen clock and no color, compared
//! against `tests/golden/*.txt`. Regenerate with `UPDATE_GOLDEN=1`.

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rayon::prelude::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn render(fixture: &Path, preset: &str, icons: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_garnish"))
        .args([
            "preview",
            fixture.to_str().unwrap(),
            "--preset",
            preset,
            "--icons",
            icons,
            "--color",
            "never",
            "--width",
            "100",
        ])
        .env("GARNISH_NOW", "1738425600")
        .env("GARNISH_CONFIG", root().join("tests/fixtures/configs/empty.toml"))
        .env("GARNISH_CACHE_DIR", std::env::temp_dir().join("garnish-golden-cache"))
        .env("GARNISH_NO_SPAWN", "1")
        .env("HOME", "/home/dev")
        .env_remove("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .env_remove("DISABLE_AUTO_COMPACT")
        .env_remove("DISABLE_COMPACT")
        .env("TZ", "UTC")
        .stdin(Stdio::null())
        .output()
        .expect("run garnish");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    // drop the "── name" header line the preview prints
    text.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n"
}

#[test]
fn golden_renders_match() {
    let fixtures_dir = root().join("tests/fixtures/payloads");
    let golden_dir = root().join("tests/golden");
    std::fs::create_dir_all(&golden_dir).unwrap();
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&fixtures_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    fixtures.sort();
    // 400+ binary invocations: fan out so a slow CI runner stays inside the
    // test timeout. Results are collected in a deterministic order.
    let combos: Vec<(&PathBuf, &str, &str)> = fixtures
        .iter()
        .flat_map(|f| {
            ["default", "minimal", "full", "compact"].into_iter().flat_map(move |preset| {
                ["nerd", "unicode", "emoji", "ascii"]
                    .into_iter()
                    .map(move |icons| (f, preset, icons))
            })
        })
        .collect();
    let failures: Vec<String> = combos
        .par_iter()
        .filter_map(|&(fixture, preset, icons)| {
            let name = fixture.file_stem().unwrap().to_str().unwrap();
            let actual = render(fixture, preset, icons);
            let golden = golden_dir.join(format!("{name}--{preset}--{icons}.txt"));
            // An internal error row would otherwise be baked in by UPDATE_GOLDEN.
            if actual.contains("garnish: ") {
                return Some(format!("{}: renders an internal error:\n{actual}", golden.display()));
            }
            if update {
                std::fs::write(&golden, &actual).unwrap();
                return None;
            }
            match std::fs::read_to_string(&golden) {
                Ok(expected) if expected == actual => None,
                Ok(expected) => Some(format!(
                    "{}:\n--- expected\n{expected}--- actual\n{actual}",
                    golden.display()
                )),
                Err(_) => Some(format!("{}: missing (run with UPDATE_GOLDEN=1)", golden.display())),
            }
        })
        .collect();
    assert!(failures.is_empty(), "{} golden mismatches:\n{}", failures.len(), failures.join("\n"));
}
