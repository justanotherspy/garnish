//! `garnish doctor`: a diagnostic report for "why does my status line look like that".

use std::fmt::Write as _;
use std::path::Path;

use crate::cache::{Cache, Entry, Status};
use crate::config;
use crate::icons::IconSet;
use crate::modules::SCHEMAS;

/// Build the report.
#[must_use]
pub fn report(config_path: Option<&Path>) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "garnish {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        o,
        "binary   {}",
        std::env::current_exe().map_or_else(|_| "?".into(), |p| p.display().to_string())
    );
    let on_path = crate::install::on_path("garnish", std::env::var_os("PATH").as_deref());
    let _ = writeln!(
        o,
        "on PATH  {}",
        if on_path { "yes" } else { "no (run `make install` or `garnish install --absolute`)" }
    );
    let _ = writeln!(o, "git      {}", git_version());
    let _ = writeln!(o);
    settings_section(&mut o);
    config_section(&mut o, config_path);
    cache_section(&mut o);
    environment_section(&mut o);
    o
}

fn settings_section(o: &mut String) {
    let settings = crate::install::default_settings_path();
    let _ = writeln!(o, "claude settings  {}", settings.display());
    match std::fs::read_to_string(&settings) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => match v.get("statusLine") {
                Some(sl) => {
                    let _ = writeln!(
                        o,
                        "statusLine       command={} refreshInterval={}",
                        sl.get("command").and_then(|c| c.as_str()).unwrap_or("?"),
                        sl.get("refreshInterval")
                            .map_or_else(|| "unset".to_owned(), ToString::to_string)
                    );
                }
                None => {
                    let _ = writeln!(o, "statusLine       not configured (run `garnish install`)");
                }
            },
            Err(e) => {
                let _ = writeln!(o, "statusLine       settings are not valid JSON: {e}");
            }
        },
        Err(_) => {
            let _ = writeln!(o, "statusLine       settings file not found");
        }
    }
    let _ = writeln!(o);
}

fn config_section(o: &mut String, config_path: Option<&Path>) {
    let loaded = config::load(config_path, &SCHEMAS);
    match (&loaded.path, loaded.errors.is_empty()) {
        (None, _) => {
            let _ = writeln!(
                o,
                "config   none (built-in defaults); `garnish config init` writes one to {}",
                config::default_path().display()
            );
        }
        (Some(p), true) => {
            let _ = writeln!(o, "config   {} ok", p.display());
        }
        (Some(p), false) => {
            let _ = writeln!(o, "config   {} INVALID (defaults in effect)", p.display());
            for e in &loaded.errors {
                let _ = writeln!(o, "         {e}");
            }
        }
    }
    let c = &loaded.config;
    let _ = writeln!(
        o,
        "preset={} icons={} theme={} frame={} lines={}",
        c.preset.name(),
        c.icons.name(),
        c.theme_name,
        c.frame.style.name(),
        c.lines.len()
    );
    let _ = writeln!(o);
}

fn cache_section(o: &mut String) {
    let cache = Cache::from_env();
    let root = cache.root();
    let writable = std::fs::create_dir_all(root).is_ok()
        && std::fs::metadata(root).is_ok_and(|m| !m.permissions().readonly());
    let _ = writeln!(
        o,
        "cache    {} ({})",
        root.display(),
        if writable { "writable" } else { "NOT writable" }
    );
    let sessions = count_dirs(&root.join("sessions"));
    let repos = count_dirs(&root.join("repos"));
    let _ = writeln!(o, "         {sessions} session dir(s), {repos} repo dir(s)");
    let failures = failed_entries(root);
    if failures.is_empty() {
        let _ = writeln!(o, "         no failed refreshes");
    } else {
        for (path, entry) in failures {
            let _ = writeln!(
                o,
                "         FAILED {} ({}s ago): {}",
                path,
                entry.age_ms() / 1000,
                entry.error
            );
        }
    }
    let _ = writeln!(o);
}

fn environment_section(o: &mut String) {
    let _ = writeln!(o, "environment");
    for key in [
        "COLUMNS",
        "LINES",
        "NO_COLOR",
        "TZ",
        "GARNISH_CONFIG",
        "GARNISH_CACHE_DIR",
        "GARNISH_NOW",
        "GARNISH_NO_SPAWN",
        "GARNISH_COLUMNS",
        "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
        "DISABLE_AUTO_COMPACT",
    ] {
        if let Ok(v) = std::env::var(key) {
            let _ = writeln!(o, "         {key}={v}");
        }
    }
    let _ = writeln!(o);

    let _ =
        writeln!(o, "glyph test (each line should align; boxes mean your font lacks the glyphs)");
    for set in IconSet::ALL {
        let _ = writeln!(o, "  {:<8} {}", set.name(), crate::docs::glyph_test_line(set));
    }
}

fn git_version() -> String {
    crate::git::run_git(Path::new("."), &["--version"], std::time::Duration::from_secs(2))
        .map_or_else(|e| format!("not available ({e})"), |v| v.trim().to_owned())
}

fn count_dirs(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |d| d.flatten().filter(|e| e.path().is_dir()).count())
}

/// Every `err` cache entry under the root, as `(scope/module, entry)`.
#[must_use]
pub fn failed_entries(root: &Path) -> Vec<(String, Entry)> {
    let mut out = Vec::new();
    for kind in ["sessions", "repos"] {
        let Ok(dirs) = std::fs::read_dir(root.join(kind)) else { continue };
        for d in dirs.flatten() {
            let Ok(files) = std::fs::read_dir(d.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().is_none_or(|e| e != "cache") {
                    continue;
                }
                if let Some(entry) = std::fs::read_to_string(&p).ok().and_then(|t| Entry::parse(&t))
                    && entry.status == Status::Err
                {
                    let name = format!(
                        "{kind}/{}/{}",
                        d.file_name().to_string_lossy(),
                        p.file_stem()
                            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
                    );
                    out.push((name, entry));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Scope;
    use std::collections::BTreeMap;

    #[test]
    fn failed_entries_lists_only_err_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().to_path_buf());
        cache.write(&Scope::Repo("r1".into()), "sync", &Entry::err(1, "git timed out")).unwrap();
        cache.write(&Scope::Repo("r1".into()), "branch", &Entry::ok(1, BTreeMap::new())).unwrap();
        cache.write(&Scope::Session("s1".into()), "x", &Entry::err(1, "boom")).unwrap();
        let failed = failed_entries(dir.path());
        let names: Vec<&str> = failed.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["repos/r1/sync", "sessions/s1/x"]);
        assert_eq!(failed[0].1.error, "git timed out");
    }

    #[test]
    fn report_mentions_every_section() {
        let r = report(None);
        for needle in [
            "garnish ",
            "claude settings",
            "config",
            "cache",
            "environment",
            "glyph test",
            "nerd",
            "ascii",
        ] {
            assert!(r.contains(needle), "{needle}\n{r}");
        }
    }
}
