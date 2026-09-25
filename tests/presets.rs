//! Every file under `presets/` is a complete, valid config with the header
//! SPEC § 12 asks for, renders at its declared width uncut, and moves
//! exactly where it promises to (CLAUDE.md § Conventions: a gallery preset
//! is a promise).

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use garnish::ansi::{Painter, display_width};
use garnish::config::{Config, Overflow};
use garnish::layout::{Elem, Line, Piece};
use garnish::modules::SCHEMAS;
use garnish::num::{floor_to_u64, u64_to_usize};
use garnish::render::{Clock, render_tree_at};
use rayon::prelude::*;

/// The instant every render starts from, a minute boundary, and two later
/// ticks inside the same minute.
const NOW: i64 = 1_738_425_600;
const LATER: i64 = NOW + 1;
const MUCH_LATER: i64 = NOW + 5;

/// The ticks a motion promise is looked for in: enough for a half-speed
/// step to advance three times.
const TICKS: [i64; 7] = [NOW, NOW + 1, NOW + 2, NOW + 3, NOW + 4, NOW + 5, NOW + 6];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The binary under the rule for tests that run it (CLAUDE.md § Cache and
/// worker invariants, SPEC § 9): its own cache and working directory, no
/// worker, no managed settings file, a fixed home, and none of the
/// developer's `CLAUDE_*`, `DISABLE_*` or `GARNISH_*` variables.
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

/// One file under `presets/`, as it is on disk.
struct PresetFile {
    stem: String,
    path: PathBuf,
    text: String,
}

fn preset_files() -> Vec<PresetFile> {
    let mut files: Vec<PresetFile> = std::fs::read_dir(root().join("presets"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .map(|path| PresetFile {
            stem: path.file_stem().unwrap().to_string_lossy().into_owned(),
            text: std::fs::read_to_string(&path).unwrap(),
            path,
        })
        .collect();
    files.sort_by(|a, b| a.stem.cmp(&b.stem));
    files
}

/// The config a preset's file resolves to, as `config init --preset` writes it.
fn config_of(text: &str) -> Config {
    garnish::config::parse(&garnish::gallery::body(text), &SCHEMAS).0
}

/// Every line of a render at `columns` and `secs`, in process, with the
/// pinned clock of the docs and animations running as in a live session.
fn tree(cfg: &Config, columns: usize, secs: i64) -> Vec<Line> {
    let clock =
        Clock { now: jiff::Timestamp::from_second(secs).unwrap(), animate: true, ..Clock::fixed() };
    let payload = garnish::fixtures::payload("subscription-full");
    render_tree_at(&payload, cfg, Some(columns), &clock)
        .into_iter()
        .flat_map(|(_, lines)| lines)
        .collect()
}

fn text(piece: &Piece) -> String {
    piece.segs.iter().map(garnish::ansi::Segment::text).collect()
}

fn plain(lines: &[Line]) -> String {
    lines.iter().map(|l| Painter::PLAIN.paint(&l.segments())).collect::<Vec<_>>().join("\n")
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
        let width = display_width(row);
        if width > box_width {
            failures.push(format!(
                "{label}: {width} cells, wider than the {box_width}-cell box at {columns} columns:\n{row}"
            ));
        }
    }
    failures
}

/// What the declared width cut, found by structure rather than by glyph
/// (sch-08: the ascii set cuts with `..`, which is also its pending-PR
/// glyph, and a cut row is exactly the box wide): a run the layout cut or
/// recut is a `Group` piece, and a module, a title or a column dropped or
/// shortened for room makes the modules and titles differ from the same
/// render 200 columns wider.
fn cut_failures(label: &str, declared: &[Line], wide: &[Line]) -> Vec<String> {
    let mut failures = Vec::new();
    if declared.iter().flat_map(|l| &l.pieces).any(|p| matches!(p.elem, Elem::Group(_))) {
        failures.push(format!("{label}: a run is cut at its declared width:\n{}", plain(declared)));
    }
    let content = |lines: &[Line]| -> Vec<(String, String)> {
        lines
            .iter()
            .flat_map(|l| &l.pieces)
            .filter_map(|p| match &p.elem {
                Elem::Module(id) => Some((id.clone(), text(p))),
                Elem::Title => Some(("title".to_owned(), text(p))),
                _ => None,
            })
            .collect()
    };
    if content(declared) != content(wide) {
        failures.push(format!(
            "{label}: its modules or titles differ from a render 200 columns wider:\n{}\n---\n{}",
            plain(declared),
            plain(wide)
        ));
    }
    failures
}

/// A line ticker slides exactly the cells its step covers between two
/// ticks: after the frame's fixed prefix, the later window is the earlier
/// one shifted by that many cells (whole-stack review: with `compact`
/// durations the period changes between ticks and the window jumps; the
/// presets use `fixed`).
fn ticker_advance_failures(
    stem: &str,
    cells: usize,
    rows: &[String],
    later: &[String],
) -> Vec<String> {
    // Not a `#[test]` fn, so the crate's panic-path lints apply here.
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

/// At each of [`TICKS`], each line's pieces that `pick` selects as one run
/// of characters.
fn runs(ticks: &[Vec<Line>], pick: &dyn Fn(&Elem) -> bool) -> Vec<Vec<Vec<char>>> {
    ticks
        .iter()
        .map(|lines| {
            lines
                .iter()
                .map(|l| {
                    l.pieces
                        .iter()
                        .filter(|p| pick(&p.elem))
                        .flat_map(|p| text(p).chars().collect::<Vec<_>>())
                        .collect()
                })
                .collect()
        })
        .collect()
}

/// Whether the pieces `pick` selects change over [`TICKS`], compared cell
/// by cell over what two ticks both draw: a rule that only grew or shrank
/// because a neighbour changed width (a `compact` countdown) has not
/// moved, since a line's rule cells are numbered from its first (SPEC
/// § 4.3).
fn moves(ticks: &[Vec<Line>], pick: &dyn Fn(&Elem) -> bool) -> bool {
    let runs = runs(ticks, pick);
    let Some(first) = runs.first() else { return false };
    runs.iter()
        .any(|tick| tick.iter().zip(first).any(|(a, b)| a.iter().zip(b).any(|(x, y)| x != y)))
}

/// sch-09: each promise of motion checked on its own, from the parsed
/// config, so one moving part cannot hide a dead one (a travelling rule
/// hid `animated-dots`' blank model frames), and a preset that turns
/// animation off stays still.
fn motion_failures(
    stem: &str,
    cfg: &Config,
    columns: usize,
    rows: &[String],
    later: &[String],
    much_later: &[String],
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut promises: Vec<(String, bool)> = Vec::new();
    if cfg.overflow == Overflow::Ticker {
        promises.push(("a line ticker".to_owned(), later != rows));
        let per_tick = cfg.ticker_step;
        let before = floor_to_u64(garnish::num::i64_to_f64(NOW) * per_tick);
        let after = floor_to_u64(garnish::num::i64_to_f64(LATER) * per_tick);
        let cells = u64_to_usize(after.saturating_sub(before));
        failures.extend(ticker_advance_failures(stem, cells, rows, later));
    }
    // Rendered once, and only for a preset that promises more than a ticker.
    let ticks: std::cell::OnceCell<Vec<Vec<Line>>> = std::cell::OnceCell::new();
    let ticks = || ticks.get_or_init(|| TICKS.iter().map(|t| tree(cfg, columns, *t)).collect());
    if !cfg.frame.fill_pattern.is_empty() {
        let moved = moves(ticks(), &|e| *e == Elem::Rule);
        promises.push(("a travelling rule (`fill_pattern`)".to_owned(), moved));
    }
    if !cfg.frame.separator_frames.is_empty() {
        let moved = moves(ticks(), &|e| *e == Elem::Separator);
        promises.push(("separator frames".to_owned(), moved));
    }
    for (id, m) in &cfg.modules {
        let frames: Vec<&String> = m.all_icon_frames().values().flatten().collect();
        if frames.is_empty() {
            continue;
        }
        let own = |e: &Elem| matches!(e, Elem::Module(m) if m == id);
        // Which frames each tick shows: one that comes and goes is the icon
        // cycling, whatever else the module changes (a clock's seconds).
        let seen: Vec<Vec<bool>> = runs(ticks(), &own)
            .iter()
            .map(|lines| {
                let shown: String = lines.iter().flatten().collect();
                frames.iter().map(|f| !f.is_empty() && shown.contains(f.as_str())).collect()
            })
            .collect();
        // A frame of an icon the fixture never draws (`fast` outside fast
        // mode) promises nothing here.
        if seen.iter().flatten().any(|s| *s) {
            let cycles = seen.iter().any(|s| Some(s) != seen.first());
            promises.push((format!("{id}'s icon frames"), cycles));
        }
    }
    for (name, m) in &cfg.texts {
        let scrolls = m.str("overflow") != "clip"
            && m.size("width") > 0
            && display_width(m.str("text")) > m.size("width");
        if scrolls {
            let id = format!("text.{name}");
            let own = |e: &Elem| matches!(e, Elem::Module(m) if *m == id);
            promises.push((format!("{id} scrolling in its box"), moves(ticks(), &own)));
        }
    }
    for (what, moved) in promises {
        if !moved {
            failures
                .push(format!("{stem}: promises {what} but it does not move at {columns} columns"));
        }
    }
    if cfg.animate == Some(false) && later != much_later {
        failures.push(format!(
            "{stem}: animate = false, but the render changes inside a minute:\n{}\n---\n{}",
            later.join("\n"),
            much_later.join("\n")
        ));
    }
    failures
}

/// Everything one preset promises, as failures.
fn check(p: &PresetFile) -> Vec<String> {
    let stem = p.stem.as_str();
    let mut failures = Vec::new();
    let head = |key: &str| garnish::gallery::header(&p.text, key);
    if head("name") != Some(stem) {
        failures.push(format!("{stem}: `# name:` must match the filename"));
    }
    if head("summary").is_none_or(str::is_empty) {
        failures.push(format!("{stem}: no `# summary:`"));
    }
    let Some(columns) =
        head("columns").and_then(|c| c.parse::<usize>().ok()).filter(|c| (60..=400).contains(c))
    else {
        failures.push(format!("{stem}: `# columns:` must be an integer from 60 to 400"));
        return failures;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = p.path.to_str().unwrap();
    let check = garnish(tmp.path()).args(["--config", path, "config", "check"]).output().unwrap();
    if !check.status.success() {
        failures.push(format!(
            "{stem}: config check failed:\n{}",
            String::from_utf8_lossy(&check.stdout)
        ));
    }
    let payload = root().join("tests/fixtures/payloads/subscription-full.json");
    let preview = |now: i64| -> Vec<String> {
        let width = columns.to_string();
        let args =
            ["--config", path, "preview", payload.to_str().unwrap(), "--color", "never", "--width"];
        let out = garnish(tmp.path())
            .args(args)
            .arg(&width)
            .env("GARNISH_NOW", now.to_string())
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "{stem}: preview failed:\n{text}");
        // The first line is preview's `── name` header; the rest is the render.
        text.lines().skip(1).map(str::to_owned).collect()
    };
    let (rows, later, much_later) = (preview(NOW), preview(LATER), preview(MUCH_LATER));
    if rows.is_empty() {
        failures.push(format!("{stem}: preview printed nothing"));
        return failures;
    }
    // The fit promise holds at every instant, not only the first: a
    // compact duration changes width between ticks (whole-stack review).
    for (when, rows) in [(NOW, &rows), (LATER, &later), (MUCH_LATER, &much_later)] {
        failures.extend(fit_failures(&format!("{stem}@{when}"), columns, rows));
    }
    let cfg = config_of(&p.text);
    // A line ticker never shows `…` and its row is exactly the box: it
    // scrolls instead, which the motion checks hold it to.
    if cfg.overflow != Overflow::Ticker {
        for secs in [NOW, LATER, MUCH_LATER] {
            let (declared, wide) =
                (tree(&cfg, columns, secs), tree(&cfg, columns.saturating_add(200), secs));
            failures.extend(cut_failures(&format!("{stem}@{secs}"), &declared, &wide));
        }
    }
    failures.extend(motion_failures(stem, &cfg, columns, &rows, &later, &much_later));
    failures
}

/// The presets run side by side: each spawns the binary four times, which
/// in sequence took half a minute of a slow CI runner's 60 s budget.
#[test]
fn every_preset_has_a_header_validates_and_renders() {
    let presets = preset_files();
    assert!(presets.len() >= 10, "expected the seed presets, found {}", presets.len());
    let failures = presets.par_iter().map(check).collect::<Vec<Vec<String>>>().concat();
    assert!(
        failures.is_empty(),
        "{} preset promise(s) broken:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// sch-08: the cut detector finds a cut by structure, so an ascii cut
/// (`..`, which the `…` check cannot see) counts: `ascii-only` at 72
/// columns, where its second and third rows are cut, is reported, and at
/// its declared 100 it is not. (At 80 its rule shrinks and nothing is cut.)
#[test]
fn the_cut_detector_sees_an_ascii_cut() {
    let text = std::fs::read_to_string(root().join("presets/ascii-only.toml")).unwrap();
    let cfg = config_of(&text);
    let narrow = tree(&cfg, 72, NOW);
    let shown = plain(&narrow);
    assert!(!shown.contains('…'), "{shown}");
    let rows: Vec<String> = shown.lines().map(str::to_owned).collect();
    assert!(fit_failures("ascii-only@72", 72, &rows).is_empty(), "the glyph check is blind here");
    let cut = cut_failures("ascii-only@72", &narrow, &tree(&cfg, 272, NOW));
    assert!(!cut.is_empty(), "{shown}");
    let fits = cut_failures("ascii-only@100", &tree(&cfg, 100, NOW), &tree(&cfg, 300, NOW));
    assert!(fits.is_empty(), "{fits:?}");
}

/// sch-09: each motion check finds its own promise dead when that part is
/// frozen, whatever else moves: every preset below passes as shipped and
/// is reported, promise by promise, with `animate = false` put in front.
#[test]
fn each_motion_check_sees_its_promise_die() {
    for (stem, promises) in [
        (
            "slow-motion",
            &["`fill_pattern`", "separator frames", "clock's icon", "context's icon", "text.note"]
                [..],
        ),
        ("animated-dots", &["`fill_pattern`", "separator frames", "model's icon"][..]),
        ("motd-ticker", &["text.motd"][..]),
    ] {
        let text = std::fs::read_to_string(root().join(format!("presets/{stem}.toml"))).unwrap();
        let columns = 100;
        let live = motion_failures(stem, &config_of(&text), columns, &[], &[], &[]);
        assert_eq!(live, Vec::<String>::new(), "{stem}");
        let frozen = config_of(&format!("animate = false\n{text}"));
        let dead = motion_failures(stem, &frozen, columns, &[], &[], &[]);
        assert_eq!(dead.len(), promises.len(), "{stem}: {dead:#?}");
        for (failure, promise) in dead.iter().zip(promises) {
            assert!(failure.contains(promise), "{stem}: {failure} is not about {promise}");
        }
    }
    // A still preset that is not still is reported too.
    let text = std::fs::read_to_string(root().join("presets/still-life.toml")).unwrap();
    let cfg = config_of(&text);
    let (a, b) = (vec!["x".to_owned()], vec!["y".to_owned()]);
    assert_eq!(motion_failures("still-life", &cfg, 110, &a, &a, &a), Vec::<String>::new());
    assert_eq!(motion_failures("still-life", &cfg, 110, &a, &a, &b).len(), 1);
}

/// sch-01: no frame list in the spec's examples is blank, which is how an
/// editor that drops private-use glyphs leaves one.
#[test]
fn no_frame_list_in_the_spec_is_blank() {
    let spec = std::fs::read_to_string(root().join("SPEC.md")).unwrap();
    assert!(!spec.contains("_frames = [\"\""), "SPEC.md has a frame list of empty strings");
}
