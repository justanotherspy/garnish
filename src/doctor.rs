//! `garnish doctor`: a diagnostic report for "why does my status line look like that".

use std::fmt::Write as _;
use std::path::Path;

use crate::ansi::display_width;
use crate::cache::{Cache, Entry, Status};
use crate::config;
use crate::icons::IconSet;
use crate::modules::SCHEMAS;

/// Build the report from the process environment.
#[must_use]
pub fn report(config_path: Option<&Path>) -> String {
    report_with(config_path, &Cache::from_env(), &crate::install::default_settings_path())
}

/// Build the report against explicit cache and settings locations.
#[must_use]
pub fn report_with(config_path: Option<&Path>, cache: &Cache, settings: &Path) -> String {
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
    let loaded = config::load(config_path, &SCHEMAS);
    settings_section(&mut o, settings);
    config_section(&mut o, &loaded);
    cache_section(&mut o, cache);
    environment_section(&mut o);
    glyph_section(&mut o, &loaded.config);
    o
}

fn settings_section(o: &mut String, settings: &Path) {
    let _ = writeln!(o, "claude settings  {}", settings.display());
    match std::fs::read_to_string(settings) {
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

fn config_section(o: &mut String, loaded: &config::Loaded) {
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

fn cache_section(o: &mut String, cache: &Cache) {
    let root = cache.root();
    let probe = root.join(format!(".probe.{}", std::process::id()));
    let writable = std::fs::create_dir_all(root).is_ok() && std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
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
    if let Ok(log) = std::fs::read_to_string(root.join("debug.log")) {
        let lines: Vec<&str> = log.lines().collect();
        let tail = lines.iter().rev().take(10).rev();
        let _ = writeln!(
            o,
            "         debug.log (last {} of {} lines):",
            tail.len().min(10),
            lines.len()
        );
        for line in tail {
            let _ = writeln!(o, "           {line}");
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
        "GARNISH_DEBUG",
        "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
        "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE",
        "DISABLE_AUTO_COMPACT",
        "DISABLE_COMPACT",
    ] {
        if let Ok(v) = std::env::var(key) {
            let _ = writeln!(o, "         {key}={v}");
        }
    }
    let _ = writeln!(o);
}

fn glyph_section(o: &mut String, config: &config::Config) {
    let _ = writeln!(
        o,
        "glyph test: every `|` should sit in a column with the ones above and below.\n\
         A `|` pushed out of its column marks a glyph your terminal draws wider or\n\
         narrower than garnish counts (the number after it); a box means your font\n\
         lacks the glyph. The `config` rows are the icons your config resolves to,\n\
         overrides included. Override a glyph under [modules.<id>.icons], or paste\n\
         this block into a feedback issue."
    );
    for set in IconSet::ALL {
        for row in glyph_rows(set) {
            let _ = writeln!(o, "{row}");
        }
    }
    for row in config_glyph_rows(config) {
        let _ = writeln!(o, "{row}");
    }
}

/// The glyph-test rows for one built-in icon set, one per module.
///
/// Each single-character icon is padded to two cells and followed by `|` and
/// garnish's cell count, so every field is four cells wide and a glyph the
/// terminal draws wider than counted pushes its `|` out of the column.
/// Multi-character icons (spinner frames, the effort scale, ASCII words) are
/// not single cells and are left out.
#[must_use]
pub fn glyph_rows(set: IconSet) -> Vec<String> {
    rows(set.name(), |_, icon| icon.glyph.get(set).to_owned())
}

/// The glyph-test rows for the icons a loaded config resolves to (icon set,
/// presets and per-module overrides applied), labelled `config`.
#[must_use]
pub fn config_glyph_rows(config: &config::Config) -> Vec<String> {
    rows("config", |schema, icon| {
        config.modules.get(schema.id).map_or_else(String::new, |m| m.icon(icon.key).to_owned())
    })
}

fn rows(
    label: &str,
    glyph_of: impl Fn(&crate::config::schema::ModuleSchema, &crate::config::schema::IconSpec) -> String,
) -> Vec<String> {
    SCHEMAS
        .iter()
        .filter_map(|schema| {
            let fields: Vec<String> = schema
                .icons
                .iter()
                .filter_map(|icon| {
                    let g = glyph_of(schema, icon);
                    let cells = display_width(&g);
                    let single = g.chars().filter(|c| *c != '\u{fe0f}').count() == 1;
                    (single && (1..=2).contains(&cells)).then(|| {
                        format!("{g}{}|{cells}", " ".repeat(2_usize.saturating_sub(cells)))
                    })
                })
                .collect();
            (!fields.is_empty())
                .then(|| format!("  {label:<8} {:<13} {}", schema.id, fields.join(" ")))
        })
        .collect()
}

fn git_version() -> String {
    crate::git::version().unwrap_or_else(|e| format!("not available ({e})"))
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
    fn glyph_grid_fields_are_four_cells_and_config_rows_follow_the_config() {
        for set in IconSet::ALL {
            for row in glyph_rows(set) {
                // `  <label:8> <module:13> ` is 25 ASCII bytes.
                let (_, fields) = row.split_at(25);
                // Fields are four cells plus one space apart, so every `|`
                // lands on cell 2 of a five-cell period: the column property
                // the grid promises.
                // (The ASCII marker glyph is `|` itself, so check the cells,
                // not the characters.)
                let mut col = 0_usize;
                let mut bars = 0_usize;
                for c in fields.chars() {
                    if col % 5 == 2 {
                        assert_eq!(c, '|', "{row:?}: cell {col}");
                        bars += 1;
                    }
                    col += crate::ansi::char_width(c);
                }
                assert!(bars > 0, "{row:?}: no fields");
                assert_eq!(col, bars * 5 - 1, "{row:?}: total width");
            }
        }
        let (default, _) = config::parse("", &SCHEMAS);
        let expected: Vec<String> = glyph_rows(IconSet::Nerd)
            .iter()
            .map(|r| r.replacen("nerd    ", "config  ", 1))
            .collect();
        assert_eq!(config_glyph_rows(&default), expected);
        let (custom, _) = config::parse(
            "icons = \"unicode\"\n[modules.branch.icons]\nbranch = \"B\"\n",
            &SCHEMAS,
        );
        let rows = config_glyph_rows(&custom);
        let branch = rows.iter().find(|r| r.contains(" branch ")).unwrap();
        assert!(branch.contains("B |1"), "override shows in the config row: {branch}");
        assert!(branch.contains("✱ |1"), "the rest follows the unicode set: {branch}");
    }

    #[test]
    fn report_mentions_every_section() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("cache"));
        std::fs::create_dir_all(cache.root()).unwrap();
        std::fs::write(cache.root().join("debug.log"), "1 pid=1 spawn sync failed: x\n").unwrap();
        let settings = dir.path().join("settings.json");
        let r = report_with(Some(&dir.path().join("none.toml")), &cache, &settings);
        assert!(r.contains("settings file not found"), "{r}");
        assert!(r.contains("debug.log (last 1 of 1 lines)"), "{r}");
        assert!(r.contains("(writable)"), "{r}");
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
