//! The draft `setup` edits: the config file as the TOML table it was written
//! as (SPEC § 14).
//!
//! Every edit is a key set or removed in that table, the preview renders
//! the table resolved through the ordinary parser, and a save writes the
//! table back in the order it was read, so a saved file holds exactly the
//! keys the person set and nothing a `config show` would spell out for
//! them. Comments do not survive a save; the backup keeps them.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use toml::{Table, Value};

use crate::config::{self, Config, ConfigError};
use crate::modules::SCHEMAS;

/// The size and mtime of the file when it was last read or written, for
/// the change check a save makes (SPEC § 14: a best-effort compare).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl Stamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self { modified: meta.modified().ok(), len: meta.len() })
    }
}

/// Where a row table sits in the config tree: a `[[row]]`, one of its
/// `[[row.col]]` tables, or a `[[row.col.row]]` inside that column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowAt {
    /// Index into `[[row]]`.
    pub row: usize,
    /// Index into that row's `[[row.col]]`, when addressing a column or a
    /// row inside one.
    pub col: Option<usize>,
    /// Index into that column's `[[row.col.row]]`.
    pub inner: Option<usize>,
}

impl RowAt {
    /// A top-level row.
    #[must_use]
    pub const fn row(row: usize) -> Self {
        Self { row, col: None, inner: None }
    }

    /// The TOML path of this table, as `config check` would name it.
    #[must_use]
    pub fn path(&self) -> String {
        use std::fmt::Write as _;
        let mut s = format!("row[{}]", self.row);
        if let Some(c) = self.col {
            let _ = write!(s, ".col[{c}]");
        }
        if let Some(i) = self.inner {
            let _ = write!(s, ".row[{i}]");
        }
        s
    }
}

/// The config file as a table, with what a save needs to know about the
/// file it goes to.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    table: Table,
    /// The table as the file holds it (as read, or as last saved): the draft
    /// is dirty while it differs, so an edit undone is not an edit.
    saved: Table,
    path: Option<PathBuf>,
    stamp: Option<Stamp>,
    unreadable: Option<String>,
    /// The file holds comments, which a save cannot write back: the backup
    /// alone keeps them.
    comments: bool,
}

impl Draft {
    /// Open the config at `path` (or an empty draft with no file). A file
    /// with a TOML syntax error opens on the built-in defaults and is
    /// never overwritten; [`Draft::unreadable`] says why.
    #[must_use]
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut draft = Self::from_text("");
        let Some(p) = path else { return draft };
        match std::fs::read_to_string(&p) {
            Ok(text) => {
                if let Some(problem) = config::syntax_error(&text) {
                    draft.unreadable = Some(problem);
                } else {
                    draft = Self::from_text(&text);
                    draft.comments = has_comment(&text);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => draft.unreadable = Some(format!("cannot read: {e}")),
        }
        draft.stamp = Stamp::of(&p);
        draft.path = Some(p);
        draft
    }

    /// A draft from config text (a preset's file, a test's fixture), with
    /// no file behind it. Text that does not parse gives an empty draft.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut table = toml::from_str::<Table>(text).unwrap_or_default();
        // `[[line]]` and `hide_empty_lines` are read under their permanent
        // aliases and written back under the names `config show` writes
        // (SPEC § 4.3).
        if !table.contains_key("row")
            && let Some(rows) = table.remove("line")
        {
            table.insert("row".to_owned(), rows);
        }
        if !table.contains_key("hide_empty_rows")
            && let Some(v) = table.remove("hide_empty_lines")
        {
            table.insert("hide_empty_rows".to_owned(), v);
        }
        Self {
            saved: table.clone(),
            table,
            path: None,
            stamp: None,
            unreadable: None,
            comments: false,
        }
    }

    /// A draft for a preset: a built-in name gives `preset = "<name>"` with
    /// its rows written out (so they can be edited), a gallery name gives
    /// the preset's file; `None` for an unknown name.
    #[must_use]
    pub fn from_preset(name: &str) -> Option<Self> {
        if let Some(top) = config::presets::TopPreset::parse(name) {
            let mut draft = Self::from_text(&format!("preset = {}\n", toml_str(top.name())));
            draft.materialise_rows();
            draft.saved = draft.table.clone();
            return Some(draft);
        }
        crate::gallery::find(name).map(|p| Self::from_text(&crate::gallery::body(p.source)))
    }

    /// The file this draft is saved to, when it has one.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the table differs from the file's (as read, or as last saved).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.table != self.saved
    }

    /// Swap the draft's content for `table` (a preset adopted, an edit
    /// undone), keeping the file it belongs to and what that file holds, so
    /// the draft stays dirty exactly while it differs from the file.
    pub fn replace_table(&mut self, table: Table) {
        self.table = table;
    }

    /// The preset's rows written out on opening, so the list has lines to
    /// edit, without counting as an edit: a file that only names a preset
    /// gains its rows on its first save.
    pub fn materialise_rows_as_read(&mut self) {
        let was_dirty = self.is_dirty();
        self.materialise_rows();
        if !was_dirty {
            self.saved = self.table.clone();
        }
    }

    /// Whether the next save drops comments the file holds (the backup
    /// alone then keeps them): from opening such a file until its first
    /// save.
    #[must_use]
    pub const fn loses_comments(&self) -> bool {
        self.comments
    }

    /// Why the file on disk cannot be saved over: its TOML syntax error, or
    /// a read error. `None` for a readable (or absent) file.
    #[must_use]
    pub fn unreadable(&self) -> Option<&str> {
        self.unreadable.as_deref()
    }

    /// The table as it stands.
    #[must_use]
    pub const fn table(&self) -> &Table {
        &self.table
    }

    /// The draft resolved through the ordinary parser: what the tick would
    /// render, with the problems `config check` would report.
    #[must_use]
    pub fn resolved(&self) -> (Config, Vec<ConfigError>) {
        config::parse_table(self.table.clone(), &SCHEMAS)
    }

    /// The draft as the file it is saved as.
    #[must_use]
    pub fn text(&self) -> String {
        toml::to_string_pretty(&self.table).unwrap_or_default()
    }

    /// The value at a dotted path of table keys (`["modules", "context",
    /// "width"]`).
    #[must_use]
    pub fn get(&self, path: &[&str]) -> Option<&Value> {
        let (last, parents) = path.split_last()?;
        let mut table = &self.table;
        for key in parents {
            table = table.get(*key)?.as_table()?;
        }
        table.get(*last)
    }

    /// Set the value at a path, creating the tables on the way.
    pub fn set(&mut self, path: &[&str], value: Value) {
        let Some((last, parents)) = path.split_last() else { return };
        let mut table = &mut self.table;
        for key in parents {
            let slot = table.entry((*key).to_owned()).or_insert_with(|| Value::Table(Table::new()));
            if !slot.is_table() {
                *slot = Value::Table(Table::new());
            }
            let Some(next) = slot.as_table_mut() else { return };
            table = next;
        }
        table.insert((*last).to_owned(), value);
    }

    /// Remove the key at a path, and every table it leaves empty above it
    /// (an empty `[modules.context]` says nothing and would only clutter
    /// the file).
    pub fn remove(&mut self, path: &[&str]) {
        remove_at(&mut self.table, path);
    }

    /// Whether a key is set in the file (as opposed to resolved from a
    /// preset or a default): what a chip's dot and a form's marker show.
    #[must_use]
    pub fn is_set(&self, path: &[&str]) -> bool {
        self.get(path).is_some()
    }

    /// Whether `[modules.<id>]` carries any override.
    #[must_use]
    pub fn module_has_overrides(&self, id: &str) -> bool {
        let path: Vec<&str> = id
            .strip_prefix(crate::modules::text::PREFIX)
            .map_or_else(|| vec!["modules", id], |name| vec!["modules", "text", name]);
        self.get(&path).and_then(Value::as_table).is_some_and(|t| !t.is_empty())
    }

    /// The `[[row]]` array, written out from the preset first when the file
    /// has none, so the rows can be edited at all. `None` only when the key
    /// holds something that is not an array, which the parser reports.
    pub fn rows_mut(&mut self) -> Option<&mut Vec<Value>> {
        self.materialise_rows();
        let slot = self.table.entry("row".to_owned()).or_insert_with(|| Value::Array(Vec::new()));
        if !slot.is_array() {
            *slot = Value::Array(Vec::new());
        }
        slot.as_array_mut()
    }

    /// The `[[row]]` tables as read, empty when the preset's rows are in
    /// effect.
    #[must_use]
    pub fn rows(&self) -> &[Value] {
        self.table.get("row").and_then(Value::as_array).map_or(&[], Vec::as_slice)
    }

    /// Write the preset's rows into the table when it has none (SPEC § 4:
    /// a file without `[[row]]` takes them from the preset).
    pub fn materialise_rows(&mut self) {
        if self.table.get("row").and_then(Value::as_array).is_some_and(|r| !r.is_empty()) {
            return;
        }
        let (config, _) = self.resolved();
        let rows: Vec<Value> = config
            .rows
            .iter()
            .map(|row| {
                let mut t = Table::new();
                let col = row.single().cloned().unwrap_or_default();
                t.insert("modules".to_owned(), string_list(&col.left));
                if !col.right.is_empty() {
                    t.insert("right".to_owned(), string_list(&col.right));
                }
                Value::Table(t)
            })
            .collect();
        self.table.insert("row".to_owned(), Value::Array(rows));
    }

    /// The table at `at`, when it exists.
    #[must_use]
    pub fn row(&self, at: RowAt) -> Option<&Table> {
        let row = self.rows().get(at.row)?.as_table()?;
        let Some(c) = at.col else { return Some(row) };
        let col = row.get("col")?.as_array()?.get(c)?.as_table()?;
        let Some(i) = at.inner else { return Some(col) };
        col.get("row")?.as_array()?.get(i)?.as_table()
    }

    /// The table at `at`, to edit; marks the draft dirty.
    pub fn row_mut(&mut self, at: RowAt) -> Option<&mut Table> {
        let row = self.rows_mut()?.get_mut(at.row)?.as_table_mut()?;
        let Some(c) = at.col else { return Some(row) };
        let col = row.get_mut("col")?.as_array_mut()?.get_mut(c)?.as_table_mut()?;
        let Some(i) = at.inner else { return Some(col) };
        col.get_mut("row")?.as_array_mut()?.get_mut(i)?.as_table_mut()
    }

    /// The array a row table sits in (`[[row]]`, a row's `[[row.col]]`, a
    /// column's `[[row.col.row]]`), to add, move or delete entries there.
    pub fn siblings_mut(&mut self, at: RowAt) -> Option<&mut Vec<Value>> {
        match (at.col, at.inner) {
            (None, _) => self.rows_mut(),
            (Some(_), None) => {
                let row = self.rows_mut()?.get_mut(at.row)?.as_table_mut()?;
                row.get_mut("col")?.as_array_mut()
            }
            (Some(c), Some(_)) => {
                let row = self.rows_mut()?.get_mut(at.row)?.as_table_mut()?;
                let col = row.get_mut("col")?.as_array_mut()?.get_mut(c)?.as_table_mut()?;
                col.get_mut("row")?.as_array_mut()
            }
        }
    }

    /// Whether the file changed on disk since it was read or last saved: a
    /// file absent at open and present now counts as changed.
    #[must_use]
    pub fn changed_on_disk(&self) -> bool {
        self.path.as_deref().is_some_and(|p| Stamp::of(p) != self.stamp)
    }

    /// Write the draft to its file with `install`'s backup (SPEC § 5),
    /// returning the backup's path when one was kept.
    ///
    /// # Errors
    /// A draft with no file, a file that does not parse (never rewritten),
    /// or the OS error of the write, each as one line.
    pub fn save(&mut self) -> Result<Option<PathBuf>, String> {
        let Some(path) = self.path.clone() else {
            return Err("no file to save to; pass --config <FILE>".to_owned());
        };
        if let Some(problem) = &self.unreadable {
            return Err(format!(
                "{}: {problem}; a file that does not parse is never rewritten, fix or move it first",
                path.display()
            ));
        }
        let existed = path.exists();
        let text = toml::to_string_pretty(&self.table)
            .map_err(|e| format!("the draft cannot be written as TOML: {e}"))?;
        let backup = crate::install::replace_file(&path, &text, existed)?;
        self.stamp = Stamp::of(&path);
        self.saved = self.table.clone();
        self.comments = false;
        Ok(backup)
    }

    /// Re-read the file, dropping the draft's edits (the "reload" answer of
    /// the change check), and write a preset's rows out as opening does.
    pub fn reload(&mut self) {
        *self = Self::open(self.path.clone());
        self.materialise_rows_as_read();
    }

    /// Whether any row, column or inner row names the box.
    #[must_use]
    pub fn box_in_use(&self, name: &str) -> bool {
        let names = |t: &Table| t.get("box").and_then(Value::as_str) == Some(name);
        let tables = |t: &Table, key: &str| -> Vec<Table> {
            t.get(key)
                .and_then(Value::as_array)
                .map_or_default(|a| a.iter().filter_map(Value::as_table).cloned().collect())
        };
        self.rows().iter().filter_map(Value::as_table).any(|row| {
            names(row)
                || tables(row, "col")
                    .iter()
                    .any(|col| names(col) || tables(col, "row").iter().any(names))
        })
    }

    /// Drop every `[box.<name>]` nothing joins any more, which the parser
    /// would otherwise report on every tick; the names dropped, in file
    /// order.
    pub fn prune_orphan_boxes(&mut self) -> Vec<String> {
        let names: Vec<String> = self
            .get(&["box"])
            .and_then(Value::as_table)
            .map_or_default(|t| t.keys().cloned().collect());
        let orphans: Vec<String> = names.into_iter().filter(|n| !self.box_in_use(n)).collect();
        for name in &orphans {
            self.remove(&["box", name]);
        }
        orphans
    }
}

/// Whether TOML text (text that parses) holds a comment: a `#` outside
/// every kind of string.
fn has_comment(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let mut i = 0_usize;
    while let Some(c) = at(i) {
        if c == '#' {
            return true;
        }
        if c != '"' && c != '\'' {
            i = i.saturating_add(1);
            continue;
        }
        let triple = at(i.saturating_add(1)) == Some(c) && at(i.saturating_add(2)) == Some(c);
        let quote = if triple { 3 } else { 1 };
        i = i.saturating_add(quote);
        loop {
            match at(i) {
                None => return false,
                // Only a basic string escapes; `\"` is not its end.
                Some('\\') if c == '"' => i = i.saturating_add(2),
                Some(q)
                    if q == c
                        && (!triple
                            || (at(i.saturating_add(1)) == Some(c)
                                && at(i.saturating_add(2)) == Some(c))) =>
                {
                    i = i.saturating_add(quote);
                    // A multi-line string may end in one or two quotes of
                    // its own right before its closing three.
                    while triple && at(i) == Some(c) {
                        i = i.saturating_add(1);
                    }
                    break;
                }
                Some(_) => i = i.saturating_add(1),
            }
        }
    }
    false
}

/// The status line for the `[box.<name>]` tables
/// [`Draft::prune_orphan_boxes`] dropped.
#[must_use]
pub fn dropped_boxes(names: &[String]) -> String {
    format!("[box.{}] dropped, nothing used it", names.join("], [box."))
}

/// Remove the key at `path` under `table`, pruning every table the removal
/// leaves empty on the way back up but one whose being there is what it
/// says (a `[box.<name>]`, a `[modules.text.<name>]`); true when something
/// was removed.
fn remove_at(table: &mut Table, path: &[&str]) -> bool {
    remove_below(table, path, 0)
}

/// [`remove_at`] for the part of `path` from `depth` on, `table` being the
/// table at the part before it.
fn remove_below(table: &mut Table, path: &[&str], depth: usize) -> bool {
    match path.get(depth..) {
        None | Some([]) => false,
        Some([last]) => table.remove(*last).is_some(),
        Some([first, ..]) => {
            let Some(child) = table.get_mut(*first).and_then(Value::as_table_mut) else {
                return false;
            };
            let removed = remove_below(child, path, depth.saturating_add(1));
            let defining = path.get(..=depth).is_some_and(defined_by_being_there);
            if removed && child.is_empty() && !defining {
                table.remove(*first);
            }
            removed
        }
    }
}

/// Whether the table at `path` defines something by existing at all: an
/// empty `[box.<name>]` is still a box rows can join, an empty
/// `[modules.text.<name>]` still a module rows place (SPEC § 3.7, § 4.3).
fn defined_by_being_there(path: &[&str]) -> bool {
    matches!(path, ["box", _] | ["modules", "text", _])
}

/// A TOML array of strings.
#[must_use]
pub fn string_list(items: &[String]) -> Value {
    Value::Array(items.iter().map(|s| Value::String(s.clone())).collect())
}

/// A TOML basic string literal, for building small config texts.
fn toml_str(s: &str) -> String {
    crate::config::schema::toml_string(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_round_trips_the_file_in_its_own_order_and_names() {
        let text = "theme = \"nord\"\npreset = \"compact\"\n[[line]]\nmodules = [\"path\"]\nright = [\"clock\"]\n[modules.context]\nwidth = 30\n";
        let mut d = Draft::from_text(text);
        assert!(!d.is_dirty());
        assert_eq!(d.rows().len(), 1, "[[line]] is read as [[row]]");
        assert!(d.get(&["line"]).is_none());
        let out = d.text();
        assert!(out.find("theme").unwrap() < out.find("preset").unwrap(), "order kept: {out}");
        assert!(out.contains("[[row]]") && !out.contains("[[line]]"), "{out}");
        let (config, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(config.modules.get("context").unwrap().int("width"), 30);
        assert_eq!(config.rows.len(), 1);
        // set, get, is_set, remove with pruning
        d.set(&["modules", "clock", "preset"], Value::String("full".into()));
        assert!(d.is_dirty() && d.is_set(&["modules", "clock", "preset"]));
        assert!(d.module_has_overrides("clock") && !d.module_has_overrides("model"));
        d.remove(&["modules", "clock", "preset"]);
        assert!(d.get(&["modules", "clock"]).is_none(), "an emptied table is pruned");
        assert!(d.get(&["modules", "context", "width"]).is_some(), "siblings stay");
        d.remove(&["modules", "context", "width"]);
        assert!(d.get(&["modules"]).is_none(), "pruned all the way up");
        // A nested edit on a row table.
        let row = d.row_mut(RowAt::row(0)).unwrap();
        row.insert("title".into(), Value::String("Repo".into()));
        let (config, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(config.rows[0].title.as_ref().map(|t| t.text.as_str()), Some("Repo"));
        assert_eq!(RowAt { row: 1, col: Some(2), inner: Some(0) }.path(), "row[1].col[2].row[0]");
    }

    #[test]
    fn a_preset_draft_writes_its_rows_out_and_a_gallery_one_is_its_file() {
        let d = Draft::from_preset("compact").unwrap();
        assert_eq!(d.rows().len(), 2);
        assert_eq!(d.get(&["preset"]).and_then(Value::as_str), Some("compact"));
        assert!(!d.is_dirty(), "a fresh preset has nothing to save yet");
        let (config, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(config.rows[1].single().unwrap().right, vec!["cache".to_owned()]);
        let g = Draft::from_preset("minimal-clean").unwrap();
        assert!(g.get(&["name"]).is_none(), "the tooling header is not a key");
        assert_eq!(g.resolved().1, Vec::new());
        assert!(Draft::from_preset("nope").is_none());
        // Materialising a file with no rows keeps `preset` driving the modules.
        let mut e = Draft::from_text("preset = \"full\"\n");
        let none: &[Value] = &[];
        assert_eq!(e.rows(), none);
        assert_eq!(e.rows_mut().map(|rows| rows.len()), Some(4));
        assert_eq!(e.rows().len(), 4);
        assert!(e.is_dirty());
        let (config, _) = e.resolved();
        assert_eq!(config.modules.get("context").unwrap().int("width"), 30);
        // Dirty is a comparison with the file, not a flag: an edit put back
        // by hand (or by undo) leaves nothing to save, and a table swapped
        // in keeps the file it is compared with.
        let mut f = Draft::from_text("theme = \"nord\"\n");
        let original = f.table().clone();
        f.set(&["theme"], Value::String("mono".into()));
        assert!(f.is_dirty());
        f.set(&["theme"], Value::String("nord".into()));
        assert!(!f.is_dirty(), "the same value as the file is not an edit");
        f.replace_table(Table::new());
        assert!(f.is_dirty());
        f.replace_table(original);
        assert!(!f.is_dirty());
        // A box is in use from a row, a column or an inner row alike; only
        // the one nothing names is pruned.
        let mut g = Draft::from_text(
            "[box.r]\n[box.c]\n[box.i]\n[box.orphan]\n[[row]]\nmodules = [\"path\"]\nbox = \"r\"\n[[row]]\n[[row.col]]\nbox = \"c\"\nmodules = [\"model\"]\n[[row.col]]\n[[row.col.row]]\nbox = \"i\"\nmodules = [\"clock\"]\n",
        );
        for name in ["r", "c", "i"] {
            assert!(g.box_in_use(name), "{name}");
        }
        assert!(!g.box_in_use("orphan"));
        assert_eq!(g.prune_orphan_boxes(), vec!["orphan".to_owned()]);
        assert!(g.get(&["box", "orphan"]).is_none() && g.get(&["box", "i"]).is_some());
        assert_eq!(g.prune_orphan_boxes(), Vec::<String>::new());
    }

    /// app-04: a comment is a `#` outside every kind of string, which is
    /// what a save loses.
    #[test]
    fn comments_are_found_outside_strings_only() {
        for (text, want) in [
            ("# a comment\ntheme = \"nord\"\n", true),
            ("theme = \"nord\" # after a value\n", true),
            ("sep = \" # \"\n", false),
            ("sep = ' # '\n", false),
            ("\"key#1\" = 1\n", false),
            ("t = \"\"\"a # b\"\"\"\n", false),
            ("t = '''a # b'''\n", false),
            ("t = \"\"\"a\"\"\"\" # c\n", true),
            ("t = \"a\\\"#\"\n", false),
            ("e = ''\nf = \"\" # c\n", true),
            ("theme = \"nord\"\n", false),
        ] {
            assert_eq!(has_comment(text), want, "{text}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garnish.toml");
        std::fs::write(&path, "# mine\ntheme = \"nord\"\n").unwrap();
        let mut d = Draft::open(Some(path));
        assert!(d.loses_comments());
        d.save().unwrap();
        assert!(!d.loses_comments(), "the file has none left to lose");
    }

    /// app-03, frm-01: a `[box.<name>]` or a `[modules.text.<name>]` is
    /// defined by being there, so removing its last key leaves it (empty);
    /// removing the table itself still prunes the parents it empties.
    #[test]
    fn a_box_or_text_table_outlives_its_last_key() {
        for (text, table, key) in [
            (
                "[box.b]\ntitle = \"B\"\n[[row]]\nmodules = [\"clock\"]\nbox = \"b\"\n",
                ["box", "b"].as_slice(),
                "title",
            ),
            (
                "[modules.text.m]\ntext = \"x\"\n[[row]]\nmodules = [\"text.m\"]\n",
                ["modules", "text", "m"].as_slice(),
                "text",
            ),
        ] {
            let mut d = Draft::from_text(text);
            let mut path = table.to_vec();
            path.push(key);
            d.remove(&path);
            assert_eq!(d.get(table), Some(&Value::Table(Table::new())), "{text}");
            assert_eq!(d.resolved().1, Vec::new(), "{text}");
            let again = Draft::from_text(&d.text());
            assert_eq!(again.get(table), Some(&Value::Table(Table::new())), "{}", d.text());
            d.remove(table);
            assert!(d.get(table.get(..1).unwrap()).is_none(), "{text}: the parents go");
        }
        // A subtable of a text module is pruned as before.
        let mut d = Draft::from_text("[modules.text.m.colors]\ntext = \"accent\"\n");
        d.remove(&["modules", "text", "m", "colors", "text"]);
        assert_eq!(d.get(&["modules", "text", "m"]), Some(&Value::Table(Table::new())));
    }

    #[test]
    fn a_draft_saves_with_a_backup_and_refuses_an_unparsable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garnish.toml");
        let mut d = Draft::open(Some(path.clone()));
        assert!(d.path().is_some() && d.unreadable().is_none() && !d.changed_on_disk());
        d.set(&["theme"], Value::String("nord".into()));
        assert_eq!(d.save().unwrap(), None, "a new file needs no backup");
        assert!(!d.is_dirty() && !d.changed_on_disk());
        assert!(std::fs::read_to_string(&path).unwrap().contains("theme = \"nord\""));
        d.set(&["icons"], Value::String("ascii".into()));
        let backup = d.save().unwrap().unwrap();
        assert!(backup.file_name().unwrap().to_string_lossy().contains(".bak-"));
        // Someone else writes the file: the change check sees it.
        std::fs::write(&path, "theme = \"mono\"\n# hand-edited\n").unwrap();
        assert!(d.changed_on_disk());
        d.reload();
        assert_eq!(d.get(&["theme"]).and_then(Value::as_str), Some("mono"));
        assert!(!d.changed_on_disk());
        // A syntax error: opened on the defaults, never overwritten.
        std::fs::write(&path, "theme = \n").unwrap();
        let mut bad = Draft::open(Some(path.clone()));
        assert!(bad.unreadable().is_some());
        let none: &[Value] = &[];
        assert_eq!(bad.rows(), none);
        let err = bad.save().unwrap_err();
        assert!(err.contains("never rewritten"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theme = \n");
        // No file at all: the refusal names the flag.
        let mut none = Draft::from_text("");
        assert!(none.save().unwrap_err().contains("--config"));
    }
}
