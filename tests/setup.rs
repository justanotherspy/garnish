//! Snapshot tests of the `setup` screen (SPEC § 9, § 14): every screen
//! drawn into ratatui's `TestBackend` at three terminal sizes and compared
//! with the goldens under `tests/golden/setup/` (`UPDATE_GOLDEN=1`
//! regenerates), with key and mouse sequences driven through the same
//! input path the terminal feeds. The clock is pinned by the test app, so
//! no environment is needed.

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

use garnish::setup::{App, Input, Key, Mouse, for_test, snapshot};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Feed keys: each character as `Key::Char`, with a few named keys spelled
/// as `<enter>`, `<esc>`, `<up>`, `<down>`, `<left>`, `<right>`, `<tab>`.
fn keys(app: &mut App, script: &str) {
    let mut rest = script;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            let (name, tail) = after.split_once('>').expect("a closing >");
            let key = match name {
                "enter" => Key::Enter,
                "esc" => Key::Esc,
                "up" => Key::Up,
                "down" => Key::Down,
                "left" => Key::Left,
                "right" => Key::Right,
                "tab" => Key::Tab,
                "backtab" => Key::BackTab,
                "del" => Key::Delete,
                "bs" => Key::Backspace,
                "home" => Key::Home,
                "end" => Key::End,
                other => panic!("unknown key <{other}>"),
            };
            app.input(Input::Key(key));
            rest = tail;
        } else {
            let mut chars = rest.chars();
            let c = chars.next().unwrap();
            app.input(Input::Key(Key::Char(c)));
            rest = chars.as_str();
        }
    }
}

fn click(app: &mut App, x: u16, y: u16) {
    app.input(Input::Mouse { x, y, kind: Mouse::Click });
}

/// The cell `needle` starts at on a snapshot line: `find` gives a byte
/// offset, and the frame glyphs before it are three bytes each.
fn col(line: &str, needle: &str) -> u16 {
    let byte = line.find(needle).unwrap_or_else(|| panic!("{needle:?} not on {line:?}"));
    u16::try_from(line.char_indices().take_while(|(b, _)| *b < byte).count()).unwrap()
}

/// Compare a screen with its golden, or write it under `UPDATE_GOLDEN=1`.
fn check(name: &str, app: &mut App, width: u16, height: u16) -> String {
    let shot = snapshot(app, width, height);
    let path = root().join("tests/golden/setup").join(format!("{name}--{width}x{height}.txt"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &shot).unwrap();
        return shot;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no golden {} (UPDATE_GOLDEN=1 writes it)", path.display()));
    assert_eq!(shot, want, "{name} at {width}x{height} differs from {}", path.display());
    shot
}

const TWO_ROWS: &str = "icons = \"unicode\"\n[[row]]\nmodules = [\"path\", \"branch\", \"sync\"]\nright = [\"clock\"]\n[[row]]\nmodules = [\"model\", \"context\"]\nright = [\"cost\"]\n";

/// Every golden the tests below write, so a renamed screen cannot leave a
/// stale file behind (`UPDATE_GOLDEN=1` never deletes one).
const GOLDENS: &[&str] = &[
    "builder--140x40",
    "builder--80x24",
    "columns--100x30",
    "confirm--80x24",
    "frame-form--80x24",
    "glyph-picker--80x24",
    "help--80x24",
    "home--80x24",
    "install--80x24",
    "module-form--140x40",
    "module-form--80x24",
    "module-picker--80x24",
    "picker--80x24",
    "picker-gallery--140x40",
    "top-form--80x24",
];

#[test]
fn every_setup_golden_is_written_by_a_test() {
    let mut found: Vec<String> = std::fs::read_dir(root().join("tests/golden/setup"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().trim_end_matches(".txt").to_owned())
        .collect();
    found.sort();
    assert_eq!(found, GOLDENS, "setup goldens without a test (delete them)");
}

#[test]
fn home_picker_and_builder_screens_match_their_goldens() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    // No config file: the home menu, then the picker on the first preset.
    let mut app = for_test("", None, Path::new("/home/dev"));
    let shot = check("home", &mut app, 80, 24);
    assert!(shot.contains("Pick a preset") && shot.contains("no config file yet"), "{shot}");
    keys(&mut app, "<enter>");
    let shot = check("picker", &mut app, 80, 24);
    assert!(shot.contains("default") && shot.contains("preview"), "{shot}");
    keys(&mut app, "jjjjjj");
    let shot = check("picker-gallery", &mut app, 140, 40);
    assert!(shot.contains("designed for"), "a gallery preset states its width: {shot}");
    // The warnings have lines of their own, so a narrow terminal (the one
    // they are for) shows them whole rather than clipping them off the
    // end of the summary: `bars-and-limits` wants 130 columns.
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("⚠ this terminal is 80 wide"), "{shot}");
    assert!(shot.contains("designed for 130 columns"), "{shot}");
    // `boxed-panels` is nine lines; 24 rows keep seven whole.
    keys(&mut app, "j");
    let shot = snapshot(&mut app, 80, 24);
    assert!(
        shot.contains("boxed-panels") && shot.contains("⚠ fullscreen keeps 7 of the 9 lines whole"),
        "{shot}"
    );
    keys(&mut app, "k");
    keys(&mut app, "<esc>");
    assert!(snapshot(&mut app, 80, 24).contains("Pick a preset"), "esc goes home");
    // A config file: the builder opens on it, with the first row's lines
    // marked in the preview.
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    let shot = check("builder", &mut app, 80, 24);
    assert!(shot.contains("row 1") && shot.contains("path") && shot.contains("▶"), "{shot}");
    check("builder", &mut app, 140, 40);
    // The module editor is generated from the schema.
    keys(&mut app, "<right><enter>");
    let shot = check("module-form", &mut app, 80, 24);
    assert!(shot.contains("[modules.path]") && shot.contains("style"), "{shot}");
    check("module-form", &mut app, 140, 40);
    // Enter on an icon key opens the glyph picker: the four sets, then the
    // suggestions, then a custom entry. Fourteen rows down from `enabled`:
    // `preset`, `refresh`, `hide`, the five common keys, path's four options.
    keys(
        &mut app,
        "<down><down><down><down><down><down><down><down><down><down><down><down><down><down>",
    );
    keys(&mut app, "<enter>");
    let shot = check("glyph-picker", &mut app, 80, 24);
    assert!(shot.contains("icons.folder") && shot.contains("custom"), "{shot}");
    keys(&mut app, "<esc>");
    keys(&mut app, "<esc>m");
    keys(&mut app, "sn");
    let shot = check("module-picker", &mut app, 80, 24);
    assert!(shot.contains("session_name"), "{shot}");
    keys(&mut app, "<esc>?");
    let shot = check("help", &mut app, 80, 24);
    assert!(shot.contains("Builder") && shot.contains("save"), "{shot}");
    keys(&mut app, "<esc>1");
    let shot = check("top-form", &mut app, 80, 24);
    assert!(shot.contains("hide_empty_rows"), "{shot}");
    // Enter on a boolean toggles it and writes exactly that key; d unsets
    // it again, and d on a key that is not set is a no-op that does not
    // dirty the draft.
    keys(&mut app, "<down><down><down><down><down><down><down><down><enter>");
    assert_eq!(app.draft().get(&["align"]).and_then(toml::Value::as_bool), Some(true));
    keys(&mut app, "d");
    assert!(app.draft().get(&["align"]).is_none());
    keys(&mut app, "<esc>");
    let mut fresh = for_test("", None, Path::new("/home/dev"));
    fresh.open_builder();
    keys(&mut fresh, "1<down>d");
    assert!(!fresh.draft().is_dirty(), "d on an unset key is not an edit");
    keys(&mut app, "2");
    let shot = check("frame-form", &mut app, 80, 24);
    assert!(shot.contains("[frame]"), "{shot}");
    keys(&mut app, "<esc>3");
    assert!(snapshot(&mut app, 80, 24).contains("[colors]"));
    keys(&mut app, "<esc>");
    // Too small a terminal gets one line, not a broken layout.
    let shot = snapshot(&mut app, 50, 10);
    assert!(shot.contains("needs at least 60×12"), "{shot}");
}

#[test]
fn edits_save_a_small_file_and_quit_asks_only_when_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    // Add a row, put the clock in it through the picker, save.
    keys(&mut app, "<down>am");
    keys(&mut app, "clo<enter>");
    assert!(app.status().unwrap().contains("added clock"), "{:?}", app.status());
    assert!(app.draft().is_dirty());
    keys(&mut app, "s");
    assert!(app.status().unwrap().starts_with("saved"), "{:?}", app.status());
    assert!(!app.draft().is_dirty());
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("modules = [\"clock\"]"), "{saved}");
    assert!(saved.starts_with("icons = \"unicode\""), "the file keeps its order: {saved}");
    assert!(!saved.contains("[modules.context]"), "only what was set is written: {saved}");
    let backups = std::fs::read_dir(home)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
        .count();
    assert_eq!(backups, 1, "the previous file is kept");
    // Editing an option through the form writes exactly that key.
    keys(&mut app, "<up><up><right><enter>");
    keys(&mut app, "<down><down><down><down><down><down><down><down>");
    let shot = snapshot(&mut app, 100, 30);
    assert!(shot.contains("[modules.path]"), "{shot}");
    keys(&mut app, "<esc>");
    // Quit with nothing unsaved leaves at once; dirty asks first.
    assert!(!app.done());
    keys(&mut app, "q");
    assert!(app.done());
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "<right>x");
    assert!(app.draft().is_dirty());
    keys(&mut app, "q");
    assert!(!app.done(), "asks first");
    assert!(snapshot(&mut app, 80, 24).contains("unsaved"));
    keys(&mut app, "n");
    assert!(!app.done());
    keys(&mut app, "q");
    keys(&mut app, "y");
    assert!(app.done());
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"path\""), "not saved");
}

#[test]
fn a_click_in_the_preview_selects_and_then_opens_the_module() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    let shot = snapshot(&mut app, 80, 24);
    // The preview's first rendered line is row 1 of the screen (line 0 is
    // the pane's title); the gutter is two cells, the cap two more, then a
    // pad: cell 7 is inside `path`.
    let line = shot.lines().nth(1).unwrap();
    assert!(line.contains("projects"), "{line}");
    // Cell 4 is the pad after the cap and selects the row, not a module;
    // cell 5 is the module's first cell.
    click(&mut app, 4, 1);
    assert_eq!(app.selected(), None, "{line}");
    click(&mut app, 5, 1);
    assert_eq!(app.selected(), Some("path"), "{line}");
    keys(&mut app, "<left>");
    click(&mut app, 7, 1);
    assert_eq!(app.selected(), Some("path"));
    assert!(app.status().unwrap().starts_with("selected path"), "{:?}", app.status());
    // A click on the second line selects that row.
    let line2 = shot.lines().nth(2).unwrap();
    let x = col(line2, "Opus");
    click(&mut app, x, 2);
    assert_eq!(app.selected(), Some("model"), "{line2}");
    let marked = snapshot(&mut app, 80, 24);
    assert_eq!(marked.lines().position(|l| l.starts_with('▶')), Some(2), "{marked}");
    // A module placed on two rows: the clicked row is the one selected,
    // not the first row holding it.
    let twice = home.join("twice.toml");
    std::fs::write(
        &twice,
        "icons = \"unicode\"\n[[row]]\nmodules = [\"clock\"]\n[[row]]\nmodules = [\"clock\"]\n",
    )
    .unwrap();
    let mut both = for_test("", Some(twice), home);
    let shot = snapshot(&mut both, 80, 24);
    let x = col(shot.lines().nth(2).unwrap(), "16:00");
    click(&mut both, x, 2);
    assert_eq!(both.selected(), Some("clock"));
    let marked = snapshot(&mut both, 80, 24);
    assert_eq!(marked.lines().position(|l| l.starts_with('▶')), Some(2), "{marked}");
    // Back on `path`: the first click selects it, the second opens it.
    click(&mut app, 7, 1);
    assert_eq!(app.selected(), Some("path"));
    click(&mut app, 7, 1);
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("[modules.path]"), "a second click opens the editor: {shot}");
    keys(&mut app, "<esc>");
    // A click on the frame's cap opens the frame form.
    click(&mut app, 2, 1);
    assert!(snapshot(&mut app, 80, 24).contains("[frame]"));
    keys(&mut app, "<esc>");
    // The wheel moves the row cursor.
    app.input(Input::Mouse { x: 10, y: 10, kind: Mouse::Wheel(1) });
    assert!(snapshot(&mut app, 80, 24).lines().any(|l| l.contains("▶") && l.contains("Opus")));
}

#[test]
fn an_unparsable_file_is_never_overwritten_and_a_changed_one_asks() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, "theme = \n").unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    assert!(app.status().unwrap().contains("does not parse"), "{:?}", app.status());
    keys(&mut app, "s");
    assert!(app.status().unwrap().contains("never rewritten"), "{:?}", app.status());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "theme = \n");
    // A readable file that changes under the screen: `s` asks, `y`
    // overwrites, `n` reloads.
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "<right>x");
    std::fs::write(&file, "theme = \"nord\"\n").unwrap();
    keys(&mut app, "s");
    assert!(check("confirm", &mut app, 80, 24).contains("changed on disk"));
    keys(&mut app, "n");
    assert!(!app.draft().is_dirty());
    assert_eq!(app.draft().get(&["theme"]).and_then(toml::Value::as_str), Some("nord"));
    // The top-level form: enter on `preset` opens its list, the second
    // entry is `minimal`; esc closes the form again.
    keys(&mut app, "1<enter>");
    keys(&mut app, "<down><enter>");
    keys(&mut app, "<esc>");
    assert!(app.draft().is_dirty());
    std::fs::write(&file, "theme = \"mono\"\n").unwrap();
    keys(&mut app, "s");
    keys(&mut app, "y");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("preset = \"minimal\""), "{saved}");
}

#[test]
fn the_picker_writes_the_preset_and_offers_the_install_screen() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    let mut app = for_test("", Some(file.clone()), home);
    assert!(!file.exists());
    keys(&mut app, "<enter>");
    keys(&mut app, "<down><down><down><enter>");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("preset = \"compact\""), "{saved}");
    assert!(saved.contains("[[row]]"), "the preset's rows are written out: {saved}");
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("Install into Claude Code"), "no statusLine yet: {shot}");
    check("install", &mut app, 80, 24);
    assert!(shot.contains(".claude/settings.json"), "{shot}");
    keys(&mut app, "<enter>");
    assert!(snapshot(&mut app, 80, 24).contains("statusLine block"), "asks once");
    keys(&mut app, "y");
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("\"command\": \"garnish\""), "{settings}");
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("wrote") && shot.contains("skills"), "{shot}");
    assert!(shot.contains("done; enter or esc goes back"), "the apply's own lines: {shot}");
    keys(&mut app, "<esc>");
    assert!(snapshot(&mut app, 80, 24).contains("rows"), "back to the builder");
    // With the status line configured, applying a preset goes straight to
    // the builder.
    keys(&mut app, "p");
    keys(&mut app, "minimal-clean<enter>");
    assert!(app.status().unwrap().contains("minimal-clean"), "{:?}", app.status());
    assert!(app.draft().is_dirty(), "a loaded preset differs from the file");
    assert!(snapshot(&mut app, 80, 24).contains("(unsaved)"));
    // The preset keeps the file's path, so s saves it there at once.
    keys(&mut app, "s");
    assert!(app.status().unwrap().starts_with("saved"), "{:?}", app.status());
    assert!(std::fs::read_to_string(&file).unwrap().contains("[[row]]"));
    // w previews at another width; an empty width goes back to the real one.
    keys(&mut app, "w");
    keys(&mut app, "60<enter>");
    assert!(snapshot(&mut app, 80, 24).contains("60 cols, box 56"));
    keys(&mut app, "w");
    keys(&mut app, "<bs><bs><enter>");
    assert!(snapshot(&mut app, 80, 24).contains("80 cols, box 76"));
}

#[test]
fn build_a_custom_layout_starts_from_the_preset_rows() {
    let mut app = for_test("", None, Path::new("/home/dev"));
    keys(&mut app, "2");
    let shot = snapshot(&mut app, 80, 24);
    assert!(
        shot.contains("row 1") && shot.contains("row 4") && shot.contains("(unsaved)"),
        "{shot}"
    );
    keys(&mut app, "q");
    assert!(!app.done(), "the rows written out are an edit worth asking about");
}

#[test]
fn a_preset_never_replaces_an_unparsable_file_or_unsaved_edits_unasked() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    // p over a file that does not parse is refused, and s still is.
    std::fs::write(&file, "theme = \n").unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "p");
    keys(&mut app, "compact<enter>");
    assert!(app.status().unwrap().contains("never overwritten"), "{:?}", app.status());
    keys(&mut app, "s");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "theme = \n");
    // A loaded preset is an edit: q asks, and p over unsaved edits asks.
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "p");
    keys(&mut app, "compact<enter>");
    assert!(app.draft().is_dirty());
    keys(&mut app, "q");
    assert!(!app.done(), "q asks before losing the preset");
    assert!(snapshot(&mut app, 80, 24).contains("unsaved"));
    keys(&mut app, "n");
    keys(&mut app, "p");
    keys(&mut app, "minimal<enter>");
    let shot = snapshot(&mut app, 80, 24);
    let _ = &file;
    assert!(shot.contains("Replace them with the minimal preset?"), "{shot}");
    keys(&mut app, "n");
    assert_eq!(app.draft().get(&["preset"]).and_then(toml::Value::as_str), Some("compact"));
    keys(&mut app, "p");
    keys(&mut app, "minimal<enter>y");
    assert_eq!(app.draft().get(&["preset"]).and_then(toml::Value::as_str), Some("minimal"));
}

#[test]
fn edits_the_parser_would_report_are_refused_or_named() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    // A value the parser reports at the key's own path is refused even
    // when the file already had a bad one there.
    std::fs::write(
        &file,
        "[[row]]\n[[row.col]]\nwidth = \"5\"\nmodules = [\"path\"]\n[[row.col]]\nmodules = [\"clock\"]\n",
    )
    .unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "<down><enter>");
    assert!(snapshot(&mut app, 80, 24).contains("row[0].col[0]"), "the column form");
    keys(&mut app, "<enter>");
    keys(&mut app, "<up><enter>");
    keys(&mut app, "abc<enter>");
    assert!(app.status().unwrap().contains("width"), "{:?}", app.status());
    assert_eq!(
        app.draft()
            .get(&["row"])
            .and_then(|r| r.get(0))
            .and_then(|r| r.get("col"))
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("width"))
            .and_then(toml::Value::as_str),
        Some("5"),
        "the bad value did not replace the old one"
    );
    keys(&mut app, "<esc>");
    // A builder edit that would nest boxes is refused with the parser's
    // message, and the draft is as it was.
    std::fs::write(&file, "[box.b]\n[[row]]\nbox = \"b\"\n[[row.col]]\nmodules = [\"path\"]\n[[row.col]]\nmodules = [\"clock\"]\n").unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "<down>b");
    keys(&mut app, "<down><enter>");
    assert!(app.status().unwrap().contains("nest"), "{:?}", app.status());
    assert!(app.draft().resolved().1.is_empty(), "{:?}", app.draft().resolved().1);
    // Unboxing the last member drops an orphaned [box.<name>], so the
    // saved file passes config check.
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    keys(&mut app, "b");
    keys(&mut app, "<up><enter>");
    keys(&mut app, "repo<enter>");
    assert!(app.draft().get(&["box", "repo"]).is_some());
    // The pick opens on the box the row is in; `none` is the first entry.
    keys(&mut app, "b");
    keys(&mut app, "<home><enter>");
    assert!(app.status().unwrap().contains("dropped"), "{:?}", app.status());
    assert!(app.draft().get(&["box"]).is_none());
    let problems = app.draft().resolved().1;
    assert!(problems.is_empty(), "{problems:?}");
    // A new box named in the row form gets its table, as b would give it.
    keys(&mut app, "<enter>");
    let shot = snapshot(&mut app, 100, 30);
    // A form line is `│  key   value  (default)`, or `│* key` when set.
    let is_key =
        |l: &str, key: &str| l.contains(&format!("│  {key} ")) || l.contains(&format!("│* {key} "));
    let field = shot.lines().position(|l| is_key(l, "box")).expect("a box field");
    // The first field is the line under the dialog's top border.
    let top = shot.lines().position(|l| l.contains("┌ row[0]")).expect("the row form");
    for _ in top.saturating_add(1)..field {
        keys(&mut app, "<down>");
    }
    keys(&mut app, "<enter><up><enter>");
    keys(&mut app, "side<enter>");
    assert!(app.status().unwrap().contains("set"), "{:?}", app.status());
    assert!(app.draft().get(&["box", "side"]).is_some(), "the table was created");
    let problems = app.draft().resolved().1;
    assert!(problems.is_empty(), "{problems:?}");
    keys(&mut app, "<esc>");
}

#[test]
fn a_file_with_only_a_preset_opens_with_its_rows_listed() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, "preset = \"compact\"\n").unwrap();
    let mut app = for_test("", Some(file), home);
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("row 1") && shot.contains("row 2"), "{shot}");
    assert!(!app.draft().is_dirty(), "listing the preset's rows is not an edit");
    keys(&mut app, "<right>");
    assert_eq!(app.selected(), Some("path"));
}

#[test]
fn row_list_clicks_land_on_the_drawn_chips() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    let shot = snapshot(&mut app, 80, 24);
    let (y, line) = shot.lines().enumerate().find(|(_, l)| l.starts_with("row 1")).unwrap();
    let y = u16::try_from(y).unwrap();
    // The last cell of `path`, the first of `clock`, and the label.
    let path_end = col(line, "path") + 3;
    click(&mut app, path_end, y);
    assert_eq!(app.selected(), Some("path"), "{line}");
    let clock_start = col(line, "clock");
    click(&mut app, clock_start, y);
    assert_eq!(app.selected(), Some("clock"), "{line}");
    click(&mut app, 1, y);
    assert_eq!(app.selected(), None);
    // An inner row's longer label shifts every chip; the ranges follow.
    // `C` leaves the cursor on the new second column; `]` moves `path` from
    // the first into it, `S` stacks that column.
    keys(&mut app, "C<up><right>]S");
    let shot = snapshot(&mut app, 80, 24);
    let (y, line) = shot.lines().enumerate().find(|(_, l)| l.contains("row 2.1")).unwrap();
    let y = u16::try_from(y).unwrap();
    let x = col(line, "path") + 2;
    click(&mut app, x, y);
    assert_eq!(app.selected(), Some("path"), "{line}");
}

#[test]
fn deleting_the_last_inner_row_or_column_frees_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    // `C` selects the new column, `S` stacks it, and the stack's one inner
    // row is deleted.
    keys(&mut app, "CS<down>x");
    assert!(app.status().unwrap().contains("deleted"), "{:?}", app.status());
    keys(&mut app, "<up>m");
    keys(&mut app, "clock<enter>");
    assert!(app.status().unwrap().contains("added clock"), "{:?}", app.status());
    keys(&mut app, "<up>x<up>x");
    let shot = snapshot(&mut app, 80, 24);
    assert!(!shot.contains("col 1"), "{shot}");
    keys(&mut app, "m");
    keys(&mut app, "model<enter>");
    assert!(app.status().unwrap().contains("added model"), "{:?}", app.status());
    assert!(app.draft().resolved().1.is_empty(), "{:?}", app.draft().resolved().1);
}

#[test]
fn an_empty_title_removes_the_key_and_a_refused_text_module_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    keys(&mut app, "t");
    keys(&mut app, "Repo<enter>");
    assert!(app.draft().get(&["row"]).is_some_and(|r| r.to_string().contains("Repo")));
    keys(&mut app, "t");
    keys(&mut app, "<bs><bs><bs><bs><enter>");
    assert!(!app.draft().get(&["row"]).is_some_and(|r| r.to_string().contains("title")));
    // A text module that cannot be placed (no row at all) is not created
    // and no editor opens.
    let mut empty = for_test("[[row]]\nmodules = [\"clock\"]\n", None, Path::new("/home/dev"));
    empty.open_builder();
    keys(&mut empty, "x");
    keys(&mut empty, "m<up><enter>");
    keys(&mut empty, "motd<enter>");
    assert!(empty.status().unwrap().contains("no row"), "{:?}", empty.status());
    assert!(empty.draft().get(&["modules", "text"]).is_none());
    assert!(!snapshot(&mut empty, 80, 24).contains("[modules.text.motd]"));
    // On a row of columns, `m` puts the module in the last column and says
    // so; the cursor follows it there.
    keys(&mut app, "C<up><up>m");
    keys(&mut app, "<up><enter>");
    keys(&mut app, "motd<enter>");
    assert!(app.status().unwrap().contains("to col 2"), "{:?}", app.status());
    assert!(snapshot(&mut app, 100, 30).contains("[modules.text.motd]"), "its editor opens");
    keys(&mut app, "<esc>");
    assert_eq!(app.selected(), Some("text.motd"));
    let (config, problems) = app.draft().resolved();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(config.rows[0].cols[1].left, vec!["text.motd".to_owned()]);
}

#[test]
fn a_click_on_a_scrolled_ticker_line_still_finds_its_module() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(
        &file,
        "icons = \"unicode\"\noverflow = \"ticker\"\n[[row]]\nmodules = [\"path\", \"model\", \"context\", \"limit5h\", \"limit7d\", \"session\", \"api\", \"cache\"]\nright = [\"clock\"]\n",
    )
    .unwrap();
    let mut app = for_test("", Some(file), home);
    let shot = snapshot(&mut app, 60, 20);
    assert!(shot.contains("1 line"), "{shot}");
    click(&mut app, 30, 1);
    assert!(app.selected().is_some(), "{shot}");
}

#[test]
fn new_text_modules_are_created_placed_and_dropped_with_their_table() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    keys(&mut app, "<right>m");
    keys(&mut app, "<up><enter>");
    keys(&mut app, "motd<enter>");
    assert!(snapshot(&mut app, 100, 30).contains("[modules.text.motd]"), "its editor opens");
    keys(&mut app, "<esc>");
    let draft = app.draft();
    assert_eq!(
        draft.get(&["modules", "text", "motd", "text"]).and_then(toml::Value::as_str),
        Some("motd")
    );
    assert!(draft.resolved().1.is_empty(), "{:?}", draft.resolved().1);
    // The same name twice is refused; a second placement keeps the table
    // when the first goes.
    keys(&mut app, "m<up><enter>");
    keys(&mut app, "motd<enter>");
    assert!(app.status().unwrap().contains("already exists"), "{:?}", app.status());
    keys(&mut app, "<down>m");
    keys(&mut app, "text.motd<enter>");
    assert!(app.status().unwrap().contains("added text.motd"), "{:?}", app.status());
    keys(&mut app, "x");
    assert!(!snapshot(&mut app, 80, 24).contains("placed nowhere"), "still placed once");
    assert!(app.draft().get(&["modules", "text", "motd"]).is_some());
    // Row 1's placement sits after `path`, where the cursor was.
    keys(&mut app, "<up><right><right>");
    assert_eq!(app.selected(), Some("text.motd"));
    keys(&mut app, "x");
    assert!(snapshot(&mut app, 80, 24).contains("placed nowhere"));
    keys(&mut app, "y");
    assert!(app.draft().get(&["modules", "text"]).is_none(), "the table went with it");
    // A bad name is refused with a reason.
    keys(&mut app, "m<up><enter>");
    keys(&mut app, "no good<enter>");
    assert!(app.status().unwrap().contains("letters, digits"), "{:?}", app.status());
}

#[test]
fn columns_stacks_titles_and_boxes_are_built_from_the_keys() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "C");
    assert!(app.status().unwrap().contains("column 2"), "{:?}", app.status());
    // The cursor sits on the new column, so `S` stacks it at once.
    keys(&mut app, "S");
    keys(&mut app, "<up>b");
    keys(&mut app, "<up><up><enter>");
    // A column takes no title (SPEC § 4.3): `t` on it is refused, and the
    // stack's inner row below it takes one.
    keys(&mut app, "<down>t");
    assert!(app.status().unwrap().contains("no title"), "{:?}", app.status());
    keys(&mut app, "<down>t");
    keys(&mut app, "Repo<enter>");
    let (config, problems) = app.draft().resolved();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(config.rows[0].cols.len(), 2);
    assert_eq!(config.rows[0].cols[1].rows.len(), 1, "the second column is a stack");
    let shot = check("columns", &mut app, 100, 30);
    assert!(shot.contains("col 1") && shot.contains("col 2") && shot.contains("row 2.1"), "{shot}");
    keys(&mut app, "s");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("[[row.col]]") && saved.contains("[[row.col.row]]"), "{saved}");
    assert!(saved.contains("title = \"Repo\""), "{saved}");
}

#[test]
fn undo_takes_an_edit_back_and_redo_puts_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(&file, TWO_ROWS).unwrap();
    let mut app = for_test("", Some(file), home);
    keys(&mut app, "u");
    assert_eq!(app.status(), Some("nothing to undo"));
    keys(&mut app, "<right>x");
    assert!(app.draft().is_dirty());
    assert_eq!(app.selected(), Some("branch"));
    keys(&mut app, "u");
    assert_eq!(app.status(), Some("undone: removed path"));
    assert!(!app.draft().is_dirty(), "back at the file is not an edit");
    assert_eq!(app.selected(), Some("path"), "the cursor comes back too");
    keys(&mut app, "U");
    assert_eq!(app.status(), Some("redone: removed path"));
    assert!(app.draft().is_dirty());
    keys(&mut app, "u");
    // A new edit ends the redo chain; two undos take the module and then
    // the row back.
    keys(&mut app, "<down>am");
    keys(&mut app, "clock<enter>");
    assert_eq!(app.draft().rows().len(), 3);
    keys(&mut app, "U");
    assert_eq!(app.status(), Some("nothing to redo"));
    keys(&mut app, "uu");
    assert_eq!(app.draft().rows().len(), 2);
    assert!(!app.draft().is_dirty());
    // Inside a form, ctrl-z undoes and the form shows the value put back.
    keys(&mut app, "1");
    keys(&mut app, "<down><down><down><down><down><down><down><down><enter>");
    assert_eq!(app.draft().get(&["align"]).and_then(toml::Value::as_bool), Some(true));
    app.input(Input::Key(Key::Ctrl('z')));
    assert!(app.draft().get(&["align"]).is_none());
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("top level") && !shot.contains("* align"), "{shot}");
    keys(&mut app, "<esc>");
    // Typing `u` into a picker's filter is not an undo.
    keys(&mut app, "<right>x");
    let removed = app.draft().table().clone();
    keys(&mut app, "mu<esc>");
    assert_eq!(app.draft().table(), &removed);
    // The hint bar is a row of buttons: a click on `u undo` undoes, and is
    // an undo (not an edit: the redo chain survives it).
    let shot = snapshot(&mut app, 80, 24);
    let bar = shot.lines().nth(23).unwrap();
    click(&mut app, col(bar, "u undo"), 23);
    assert!(app.status().unwrap().starts_with("undone: removed"), "{:?}", app.status());
    assert!(!app.draft().is_dirty());
    keys(&mut app, "U");
    assert!(app.status().unwrap().starts_with("redone: removed"), "{:?}", app.status());
    assert_eq!(app.draft().table(), &removed);
    keys(&mut app, "u");
    // A text module's editor open while ctrl-z takes the module back
    // closes with it.
    keys(&mut app, "m<up><enter>");
    keys(&mut app, "motd<enter>");
    assert!(app.form_keys().is_some_and(|k| k.contains(&"text".to_owned())));
    app.input(Input::Key(Key::Ctrl('z')));
    assert!(app.form_keys().is_none(), "the editor of a module that is gone closes");
    assert!(app.draft().get(&["modules", "text"]).is_none());
    // A preset adopted from the picker is a fresh start with nothing to
    // undo; the builder's own `p` is an edit.
    let mut fresh = for_test("", None, Path::new("/home/dev"));
    keys(&mut fresh, "<enter>e");
    keys(&mut fresh, "u");
    assert_eq!(fresh.status(), Some("nothing to undo"));
    keys(&mut fresh, "p");
    keys(&mut fresh, "compact<enter>y");
    assert_eq!(fresh.draft().rows().len(), 2);
    keys(&mut fresh, "u");
    assert_eq!(fresh.draft().rows().len(), 4, "back to the default preset's rows");
    // A save moves the baseline: undoing past it is dirty again, since the
    // file now differs.
    keys(&mut app, "<right>x");
    keys(&mut app, "s");
    assert!(!app.draft().is_dirty());
    keys(&mut app, "u");
    assert!(app.draft().is_dirty(), "the file lacks the module put back");
}

#[test]
fn values_that_break_another_key_say_so_and_boxes_never_orphan_their_tables() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    // `fill = false` under a `fill_pattern` is taken, and the status names
    // the key it silences (a walk of every preset's forms found the edit
    // accepted without a word, 2026-09-20).
    std::fs::write(&file, "[frame]\nfill_pattern = \"·─\"\n[[row]]\nmodules = [\"clock\"]\n")
        .unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "2<down><enter>");
    assert!(
        app.status().unwrap().starts_with("frame.fill set; ⚠ frame.fill_pattern"),
        "{:?}",
        app.status()
    );
    keys(&mut app, "<esc>");
    // A row inside a named box lists no title keys and no `blank`, and
    // unsetting its `box` drops the table nothing joins any more.
    std::fs::write(
        &file,
        "[box.repo]\ntitle = \"Repo\"\n[[row]]\nmodules = [\"clock\"]\nbox = \"repo\"\n",
    )
    .unwrap();
    let mut app = for_test("", Some(file), home);
    keys(&mut app, "<enter>");
    assert_eq!(app.form_keys().unwrap(), vec!["separator", "gap", "box"]);
    keys(&mut app, "<down><down>d");
    assert!(app.status().unwrap().contains("[box.repo] dropped"), "{:?}", app.status());
    assert!(app.draft().get(&["box"]).is_none());
    assert!(app.draft().resolved().1.is_empty(), "{:?}", app.draft().resolved().1);
    keys(&mut app, "<esc>");
    // A colour picked for a `[colors]` role is a literal the parser takes.
    keys(&mut app, "3<enter><enter>");
    assert!(app.status().unwrap().ends_with("colors.accent set"), "{:?}", app.status());
    assert!(app.draft().resolved().1.is_empty(), "{:?}", app.draft().resolved().1);
    keys(&mut app, "<esc>");
    // A separator picked from the suggestions keeps its spaces.
    keys(&mut app, "2<down><down><enter><enter>");
    let sep = app.draft().get(&["frame", "separator"]).and_then(toml::Value::as_str).unwrap();
    assert!(sep.trim() != sep, "{sep:?} lost its spaces");
    keys(&mut app, "<esc>");
}

#[test]
fn b_boxes_two_rows_and_the_preset_key_follows_its_own_rows() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let file = home.join("garnish.toml");
    std::fs::write(
        &file,
        "[[row]]\nmodules = [\"path\"]\ntitle = \"Repo\"\n[[row]]\nmodules = [\"model\"]\n[[row]]\nmodules = [\"clock\"]\n",
    )
    .unwrap();
    let mut app = for_test("", Some(file.clone()), home);
    keys(&mut app, "B");
    assert!(app.status().unwrap().contains("no row above"), "{:?}", app.status());
    keys(&mut app, "<down>B");
    // The name asked for starts from the title above, as a bare key.
    assert!(snapshot(&mut app, 80, 24).contains("repo▏"), "{}", snapshot(&mut app, 80, 24));
    keys(&mut app, "<enter>");
    assert!(app.status().unwrap().contains("new box repo"), "{:?}", app.status());
    keys(&mut app, "<down>B");
    assert!(app.status().unwrap().starts_with("joined box repo"), "{:?}", app.status());
    let (config, problems) = app.draft().resolved();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        config.boxes.get("repo").and_then(|b| b.title.as_ref()).map(|t| t.text.as_str()),
        Some("Repo")
    );
    assert!(config.rows.iter().all(|r| r.boxed.is_some()));
    // `preset` in the top form: rows that are still the old preset's follow
    // it; edited rows stay, and the status says which happened.
    std::fs::write(&file, "preset = \"compact\"\n").unwrap();
    let mut app = for_test("", Some(file), home);
    assert_eq!(app.draft().rows().len(), 2);
    keys(&mut app, "1<enter><down><enter>");
    assert!(
        app.status().unwrap().contains("rows replaced with the default preset's"),
        "{:?}",
        app.status()
    );
    assert_eq!(app.draft().rows().len(), 4);
    keys(&mut app, "<esc><right>x");
    keys(&mut app, "1<enter><down><enter>");
    assert!(app.status().unwrap().contains("rows below stay"), "{:?}", app.status());
    assert_eq!(app.draft().get(&["preset"]).and_then(toml::Value::as_str), Some("minimal"));
    assert_eq!(app.draft().rows().len(), 4);
}
