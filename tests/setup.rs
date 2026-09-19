//! Snapshot tests of the `setup` screen (SPEC § 9, § 14): every screen
//! drawn into ratatui's `TestBackend` at two terminal sizes and compared
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
    "help--80x24",
    "home--80x24",
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
    keys(&mut app, "<esc>2");
    assert!(snapshot(&mut app, 80, 24).contains("[frame]"));
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
    click(&mut app, 7, 1);
    assert_eq!(app.selected(), Some("path"));
    assert!(app.status().unwrap().starts_with("selected path"), "{:?}", app.status());
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
    assert!(snapshot(&mut app, 80, 24).contains("changed on disk"));
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
    assert!(shot.contains(".claude/settings.json"), "{shot}");
    keys(&mut app, "<enter>");
    assert!(snapshot(&mut app, 80, 24).contains("statusLine block"), "asks once");
    keys(&mut app, "y");
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("\"command\": \"garnish\""), "{settings}");
    let shot = snapshot(&mut app, 80, 24);
    assert!(shot.contains("wrote") && shot.contains("skills"), "{shot}");
    keys(&mut app, "<esc>");
    assert!(snapshot(&mut app, 80, 24).contains("rows"), "back to the builder");
    // With the status line configured, applying a preset goes straight to
    // the builder.
    keys(&mut app, "p");
    keys(&mut app, "minimal-clean<enter>");
    assert!(app.status().unwrap().contains("minimal-clean"), "{:?}", app.status());
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
    assert!(app.status().unwrap().contains("column"), "{:?}", app.status());
    keys(&mut app, "<down><down>S");
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
