//! Config-driven golden renders: every `tests/fixtures/configs/*.toml` is a
//! complete config with a comment header naming the payload fixture, the
//! terminal width, the instants to render at and any extra environment, so a
//! config key can be pinned at two clock values (tickers, animations) or with a
//! test hook set. Compared against `tests/golden/config--<name>--<now>.txt`.
//! Regenerate with `UPDATE_GOLDEN=1`.
//!
//! Header keys (each a `# key: value` comment line anywhere in the file):
//!
//! | key | meaning | default |
//! |---|---|---|
//! | `fixture` | payload under `tests/fixtures/payloads/` | `subscription-full` |
//! | `columns` | terminal width (`--width`) | `100` |
//! | `now` | comma-separated `GARNISH_NOW` instants, one golden each | `1738425600` |
//! | `icons` | icon set override (`--icons`) | none |
//! | `env` | `KEY=VALUE` added to the environment; repeatable | none |

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rayon::prelude::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const HEADER_KEYS: [&str; 5] = ["fixture", "columns", "now", "icons", "env"];

/// The header lines of one fixture, validated: a comment that names a header
/// key must be exactly `# key: value` with a non-empty value (a typo would
/// otherwise fall back to the default silently and pin the wrong render), and
/// every key but `env` may appear once.
struct Header {
    values: Vec<(String, String)>,
}

impl Header {
    fn parse(name: &str, text: &str) -> Self {
        let mut values: Vec<(String, String)> = Vec::new();
        for line in text.lines().filter(|l| l.starts_with('#')) {
            let body = line.trim_start_matches('#').trim_start();
            let Some((key, value)) = body.split_once(':') else { continue };
            let key = key.trim();
            if !HEADER_KEYS.contains(&key) {
                continue;
            }
            let exact = format!("# {key}: ");
            assert!(
                line.starts_with(&exact) && !value.trim().is_empty(),
                "{name}: header line must be `# {key}: <value>`, found {line:?}"
            );
            assert!(
                key == "env" || values.iter().all(|(k, _)| k != key),
                "{name}: duplicate `# {key}:` header"
            );
            values.push((key.to_owned(), value.trim().to_owned()));
        }
        Self { values }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn all(&self, key: &str) -> Vec<&str> {
        self.values.iter().filter(|(k, _)| k == key).map(|(_, v)| v.as_str()).collect()
    }
}

struct Case {
    name: String,
    config: PathBuf,
    fixture: PathBuf,
    columns: String,
    icons: Option<String>,
    now: String,
    env: Vec<(String, String)>,
}

fn cases() -> Vec<Case> {
    let dir = root().join("tests/fixtures/configs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for config in files {
        let name = config.file_stem().unwrap().to_str().unwrap().to_owned();
        let text = std::fs::read_to_string(&config).unwrap();
        let header = Header::parse(&name, &text);
        let fixture = root()
            .join("tests/fixtures/payloads")
            .join(format!("{}.json", header.get("fixture").unwrap_or("subscription-full")));
        assert!(fixture.is_file(), "{name}: unknown payload fixture {}", fixture.display());
        let columns = header.get("columns").unwrap_or("100").to_owned();
        columns
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("{name}: `# columns:` must be an integer"));
        let icons = header.get("icons").map(str::to_owned);
        let env: Vec<(String, String)> = header
            .all("env")
            .iter()
            .map(|kv| {
                let (k, v) = kv
                    .split_once('=')
                    .unwrap_or_else(|| panic!("{name}: `# env:` needs KEY=VALUE"));
                (k.trim().to_owned(), v.trim().to_owned())
            })
            .collect();
        let nows: Vec<&str> = header
            .get("now")
            .unwrap_or("1738425600")
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .collect();
        assert!(!nows.is_empty(), "{name}: `# now:` names no instant");
        for now in nows {
            now.parse::<i64>().unwrap_or_else(|_| panic!("{name}: `# now:` must be epoch seconds"));
            out.push(Case {
                name: name.clone(),
                config: config.clone(),
                fixture: fixture.clone(),
                columns: columns.clone(),
                icons: icons.clone(),
                now: now.to_owned(),
                env: env.clone(),
            });
        }
    }
    out
}

fn render(case: &Case, cache: &Path) -> String {
    // Paths are passed relative to the repository root so a `⚠ config: <path>`
    // line in a golden reads the same on every machine.
    let rel = |p: &Path| p.strip_prefix(root()).unwrap().to_str().unwrap().to_owned();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.current_dir(root())
        .args(["--config", &rel(&case.config), "preview", &rel(&case.fixture)])
        .args(["--color", "never", "--width", &case.columns]);
    if let Some(icons) = &case.icons {
        cmd.args(["--icons", icons]);
    }
    cmd.env("GARNISH_NOW", &case.now)
        .env("GARNISH_CACHE_DIR", cache)
        .env("GARNISH_NO_SPAWN", "1")
        .env("HOME", "/home/dev")
        .env("TZ", "UTC")
        .env_remove("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .env_remove("DISABLE_AUTO_COMPACT")
        .env_remove("DISABLE_COMPACT")
        .env_remove("NO_COLOR")
        .stdin(Stdio::null());
    for (k, v) in &case.env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run garnish");
    assert!(out.status.success(), "{}: {}", case.name, String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    // drop the "── name" header line the preview prints
    text.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n"
}

#[test]
fn config_goldens_match() {
    let golden_dir = root().join("tests/golden");
    std::fs::create_dir_all(&golden_dir).unwrap();
    let cache = std::env::temp_dir().join("garnish-config-golden-cache");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let cases = cases();
    assert!(!cases.is_empty(), "no config fixtures under tests/fixtures/configs");
    let failures: Vec<String> = cases
        .par_iter()
        .filter_map(|case| {
            let actual = render(case, &cache);
            let golden = golden_dir.join(format!("config--{}--{}.txt", case.name, case.now));
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

/// A stale `config--*.txt` whose fixture or instant no longer exists would
/// silently survive `UPDATE_GOLDEN=1`; fail so it gets deleted.
#[test]
fn every_config_golden_has_a_fixture() {
    let expected: std::collections::BTreeSet<String> =
        cases().iter().map(|c| format!("config--{}--{}.txt", c.name, c.now)).collect();
    let orphans: Vec<String> = std::fs::read_dir(root().join("tests/golden"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("config--") && !expected.contains(n))
        .collect();
    assert!(orphans.is_empty(), "config goldens without a fixture (delete them): {orphans:?}");
}
