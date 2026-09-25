//! The builder screen (SPEC § 14): its keys, its clicks in the preview and
//! the row list, saving, and its drawing.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use toml::Value;

use super::install::Back;
use super::picker::Picker;
use super::{App, Key, Level};
use crate::layout::Elem;
use crate::setup::builder::{Builder, ItemKind};
use crate::setup::draft::RowAt;
use crate::setup::form::{FormKind, Slot, SlotKind};
use crate::setup::pick::{Choice, Choose, Confirm, InputBox, Layer, Question, Target};
use crate::setup::ui::Chrome;

impl App {
    /// The builder's keys that change the draft; `false` for any other key.
    fn builder_edit_key(&mut self, key: Key) -> bool {
        let out = match key {
            Key::Char('a') => self.edit(|b, d| b.insert(d, true)),
            Key::Char('i') => self.edit(|b, d| b.insert(d, false)),
            Key::Char('c') => self.edit(Builder::clone_line),
            Key::Char('J') => self.edit(|b, d| b.shift(d, true)),
            Key::Char('K') => self.edit(|b, d| b.shift(d, false)),
            Key::Char('r') => self.edit(Builder::switch_side),
            Key::Char('[') => self.edit(|b, d| b.switch_column(d, false)),
            Key::Char(']') => self.edit(|b, d| b.switch_column(d, true)),
            Key::Char('C') => self.edit(Builder::add_column),
            Key::Char('S') => self.edit(Builder::stack),
            Key::Char(' ') => self.edit(Builder::toggle_spacer),
            _ => return false,
        };
        self.report(out);
        true
    }

    pub(super) fn builder_key(&mut self, key: Key) {
        if self.builder_edit_key(key) {
            return;
        }
        match key {
            Key::Up | Key::Char('k') => self.builder.move_line(false),
            Key::Down | Key::Char('j') => self.builder.move_line(true),
            Key::Left | Key::Char('h') => self.builder.move_chip(false),
            Key::Right | Key::Char('l') => self.builder.move_chip(true),
            Key::Tab => self.builder.next_chip(true),
            Key::BackTab => self.builder.next_chip(false),
            Key::Enter => self.edit_selection(),
            Key::Char('m') => self.layers.push(Layer::Choose(self.module_picker())),
            Key::Char('x') | Key::Delete => self.delete_selection(),
            Key::Char('t') => self.ask_title(),
            Key::Char('b') => self.ask_box(),
            Key::Char('B') => self.box_with_above(),
            Key::Char('e') => self.edit_box(),
            Key::Char('1') => self.open_form(FormKind::Top),
            Key::Char('2') => self.open_form(FormKind::Frame),
            Key::Char('3') => self.open_form(FormKind::Colors),
            Key::Char('p') => {
                let items: Vec<Choice> = Picker::new()
                    .items
                    .iter()
                    .map(|i| Choice::noted(&i.name, &i.summary))
                    .collect();
                self.layers.push(Layer::Choose(Choose::new(
                    "Start from a preset",
                    items,
                    Target::Preset,
                )));
            }
            Key::Char('f') => self.preview.cycle(true),
            Key::Char('F') => self.preview.cycle(false),
            Key::Char('w') => self.ask_width(),
            Key::Char('s') | Key::Ctrl('s') => self.save(false),
            Key::Char('I') => self.open_install(Back::Builder),
            Key::Char('q') | Key::Esc => {
                if self.draft.is_dirty() {
                    self.layers.push(Layer::Confirm(Confirm::new(
                        Question::QuitUnsaved,
                        &["The draft has unsaved edits.", "Quit and lose them?"],
                        "quit",
                        "stay",
                    )));
                } else {
                    self.quit = true;
                }
            }
            _ => {}
        }
    }

    /// `t`: the title of the selected row, typed (a column has none).
    fn ask_title(&mut self) {
        let Some(at) = self.builder.item().map(|i| i.at) else { return };
        // SPEC § 4.3: a title sits in a row's rule; a column has none, its
        // rows and its box do.
        if at.col.is_some() && at.inner.is_none() {
            self.say("a column has no title; title its rows, or box it (b)".into(), Level::Warn);
            return;
        }
        let current = self
            .draft
            .row(at)
            .and_then(|t| t.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        self.layers.push(Layer::Input(InputBox::new(
            "Title (empty removes it)",
            &current,
            Target::Slot(Slot::row(at, "title"), SlotKind::Str),
        )));
    }

    /// `b`: the box picker for the selected line, open on the box it is in.
    fn ask_box(&mut self) {
        let Some(at) = self.builder.item().map(|i| i.at) else { return };
        let mut items =
            vec![Choice::noted("none", "no box"), Choice::noted("true", "a box of its own")];
        items.extend(self.config.boxes.keys().map(|n| Choice::noted(n, "[box] table")));
        items.push(Choice::custom("A new box named"));
        let mut choose = Choose::new("Box", items, Target::BoxFor(at));
        let current = self.draft.row(at).and_then(|t| t.get("box")).map_or_else(
            || "none".to_owned(),
            |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
        );
        choose.select(&current);
        self.layers.push(Layer::Choose(choose));
    }

    /// `e`: the form of the `[box.<name>]` the selected line is in, by its
    /// own `box` or, for a column's inner row, the column's, then the
    /// row's: the keyboard's way to a box (SPEC § 14).
    fn edit_box(&mut self) {
        let Some(at) = self.builder.item().map(|i| i.at) else {
            self.say("no rows yet; a adds one".into(), Level::Info);
            return;
        };
        let named = [at, RowAt { inner: None, ..at }, RowAt::row(at.row)]
            .into_iter()
            .find_map(|a| self.draft.row(a)?.get("box")?.as_str().map(str::to_owned));
        match named {
            Some(name) if self.draft.get(&["box", &name]).is_some_and(Value::is_table) => {
                self.open_form(FormKind::Box(name));
            }
            Some(name) => self.say(format!("no [box.{name}] table; b picks a box"), Level::Warn),
            None => self.say("this line is in no named box; b puts it in one".into(), Level::Info),
        }
    }

    /// `B`: the row joins the named box of the row above without asking;
    /// with no such box, a name is asked for and both rows go in it.
    fn box_with_above(&mut self) {
        let Some(item) = self.builder.item().cloned() else {
            self.say("no rows yet; a adds one".into(), Level::Info);
            return;
        };
        match self.builder.above(&self.draft) {
            None => self.say(
                if item.kind == ItemKind::Col {
                    "a column is boxed on its own: b".to_owned()
                } else {
                    "no row above this one; b boxes a row alone".to_owned()
                },
                Level::Warn,
            ),
            Some((_, Some(name))) => {
                let out = self.edit(|builder, draft| builder.box_with_above(draft, &name));
                self.report(out);
            }
            Some((above, None)) => {
                // A title either row carries names the box; `panel` otherwise.
                let title = |at: RowAt| {
                    self.draft
                        .row(at)
                        .and_then(|t| t.get("title"))
                        .and_then(Value::as_str)
                        .map(bare_key_of)
                        .filter(|s| !s.is_empty())
                };
                let start =
                    title(above).or_else(|| title(item.at)).unwrap_or_else(|| "panel".to_owned());
                self.layers.push(Layer::Input(InputBox::new(
                    "Name for the box holding this row and the one above",
                    &start,
                    Target::BoxWith(item.at),
                )));
            }
        }
    }

    /// The module picker (SPEC § 14): the 25 ids, the config's text modules,
    /// and a new text module.
    fn module_picker(&self) -> Choose {
        let mut items: Vec<Choice> =
            crate::modules::SCHEMAS.iter().map(|s| Choice::noted(s.id, s.summary)).collect();
        for name in self.config.texts.keys() {
            items.push(Choice::noted(
                &format!("{}{name}", crate::modules::text::PREFIX),
                "text module",
            ));
        }
        items.push(Choice::custom("New text module"));
        let mut choose = Choose::new("Add a module", items, Target::AddModule);
        "New text module: name (letters, digits, _ and -)".clone_into(&mut choose.custom_title);
        choose
    }

    /// `Enter` in the builder: the module's editor, or the row's or
    /// column's form.
    fn edit_selection(&mut self) {
        if let Some(id) = self.builder.selected_id().map(str::to_owned) {
            let known = crate::modules::SCHEMAS.iter().any(|s| s.id == id)
                || id
                    .strip_prefix(crate::modules::text::PREFIX)
                    .is_some_and(|name| self.config.texts.contains_key(name));
            if known {
                self.open_form(FormKind::Module(id));
            } else {
                self.say(format!("{id} is not a module id; x removes it"), Level::Warn);
            }
            return;
        }
        let Some(item) = self.builder.item().cloned() else {
            self.say("no rows yet; a adds one".into(), Level::Info);
            return;
        };
        let kind = if item.kind == ItemKind::Col {
            FormKind::Col(item.at)
        } else {
            FormKind::Row(item.at)
        };
        self.open_form(kind);
    }

    /// `x`: the module (asking about its text table when that was its
    /// last placement), or the line.
    fn delete_selection(&mut self) {
        let text = self
            .builder
            .selected_id()
            .and_then(|id| id.strip_prefix(crate::modules::text::PREFIX))
            .map(str::to_owned);
        let out = self.builder.delete(&mut self.draft);
        self.report(out);
        if let Some(name) = text {
            let id = format!("{}{name}", crate::modules::text::PREFIX);
            let placed =
                self.config.rows.iter().flat_map(crate::config::RowCfg::ids).any(|i| *i == id);
            if !placed && self.config.texts.contains_key(&name) {
                self.layers.push(Layer::Confirm(Confirm::new(
                    Question::DropText(name.clone()),
                    &[
                        &format!("text.{name} is placed nowhere now."),
                        "Drop its [modules.text] table too?",
                    ],
                    "drop it",
                    "keep it",
                )));
            }
        }
    }

    /// `s`: write the draft, after the change check (SPEC § 14); a draft
    /// the file already holds is not written again, which would only drop
    /// its comments and leave one more backup.
    pub(super) fn save(&mut self, force: bool) {
        let exists = self.draft.path().is_some_and(std::path::Path::exists);
        let unchanged = !self.draft.is_dirty() && !self.draft.changed_on_disk();
        // A file that does not parse is told why it is never written.
        if exists && unchanged && self.draft.unreadable().is_none() {
            self.say("nothing to save: the file holds the draft already".into(), Level::Info);
            return;
        }
        if !force && self.draft.changed_on_disk() {
            // Both answers act (one drops the file, the other the edits),
            // so the question opens on doing neither, which `Esc` is too.
            self.layers.push(Layer::Confirm(
                Confirm::new(
                    Question::OverwriteOrReload,
                    &[
                        "The file changed on disk since it was read.",
                        "y overwrites it with the draft; n reloads it (u undoes that).",
                    ],
                    "overwrite",
                    "reload",
                )
                .or_cancel("keep editing"),
            ));
            return;
        }
        let rewrote = self.draft.loses_comments();
        match self.draft.save() {
            Ok(backup) => {
                let path = self.draft.path().map_or_else(String::new, |p| self.shown(p));
                // What the save dropped comes first: a line holding two
                // paths is cut long before its end on an 80-column screen.
                let line = match backup {
                    Some(b) if rewrote => format!(
                        "saved; the old file's comments live on in its backup only, {}",
                        self.shown(&b)
                    ),
                    Some(b) => format!("saved {path} (backup: {})", self.shown(&b)),
                    None => format!("saved {path}"),
                };
                self.say(line, Level::Info);
            }
            Err(e) => self.say(e, Level::Error),
        }
    }

    /// A click in the row list: selects the line (and the chip under it);
    /// a click on what is already selected edits it.
    pub(super) fn click_list(&mut self, x: u16, y: u16) {
        let was = (self.builder.cursor, self.builder.chip);
        if self.builder.click(usize::from(x), usize::from(y), self.list_area)
            && was == (self.builder.cursor, self.builder.chip)
        {
            self.edit_selection();
        }
    }

    /// A click in the preview (SPEC § 14): a module selects it (again
    /// opens its editor), a rule or cap opens the frame form, a separator
    /// its field, a title or box edge the row's form.
    pub(super) fn click_preview(&mut self, x: usize, y: usize) {
        // The pane has a two-cell gutter for the row marker: a click there
        // is on the line's row, not on the cell after the gutter.
        let Some(x) = x.checked_sub(2) else {
            if let Some(row) = self.rendered.lines.get(y).map(|p| p.row) {
                self.builder.select_row(row);
            }
            return;
        };
        let Some(hit) = self.rendered.at(x, y) else { return };
        match hit.elem {
            Elem::Module(id) => {
                let already = self.builder.selected_id() == Some(id.as_str());
                if already {
                    self.open_form(FormKind::Module(id));
                } else if self.builder.select_module(&id, Some(hit.row)) {
                    self.say(
                        format!("selected {id}; enter or a second click edits it"),
                        Level::Info,
                    );
                } else {
                    self.say(format!("{id} is placed nowhere in the row list"), Level::Warn);
                }
            }
            Elem::Group(_) => self.builder.select_row(hit.row),
            Elem::Rule | Elem::Cap => self.open_form(FormKind::Frame),
            Elem::Separator => {
                self.open_form(FormKind::Frame);
                if let Some(Layer::Form(f)) = self.layers.last_mut() {
                    f.focus("separator");
                }
            }
            Elem::Title | Elem::BoxEdge | Elem::Gap | Elem::Pad => {
                self.builder.select_row(hit.row);
                let at = RowAt::row(hit.row);
                let row = self.config.rows.get(hit.row);
                let named = row
                    .and_then(|r| r.boxed.as_ref())
                    .and_then(crate::config::BoxRef::name)
                    .map(str::to_owned);
                // The map names the outer row only, so a title or a box
                // edge inside a row of columns may belong to a column or
                // an inner row; the list is the way to those.
                let inside = row.is_some_and(|r| !r.cols.is_empty());
                match (hit.elem, named) {
                    (Elem::BoxEdge, Some(name)) => self.open_form(FormKind::Box(name)),
                    (Elem::Title | Elem::BoxEdge, _) if inside => self.say(
                        "that may belong to a column or an inner row: select it in the list and press enter".into(),
                        Level::Info,
                    ),
                    (Elem::Title | Elem::BoxEdge, _) => self.open_form(FormKind::Row(at)),
                    _ => {}
                }
            }
        }
    }

    pub(super) fn draw_builder(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let config = self.config.clone();
        let selected_row = self.builder.item().map(|i| i.at.row);
        let selected_id = self.builder.selected_id().map(str::to_owned);
        let max = area.height.saturating_sub(6).checked_div(2).unwrap_or(1).max(2);
        let pane = self.draw_pane(frame, area, &config, selected_row, selected_id.as_deref(), max);
        let head_y = area.y.saturating_add(pane.height);
        let dirty = if self.draft.is_dirty() { " (unsaved)" } else { "" };
        let head = Line::from(vec![
            Span::styled("rows", Chrome::title()),
            Span::styled(
                format!(
                    "  {}{dirty}",
                    self.draft.path().map_or_else(|| "no file".to_owned(), |p| self.shown(p))
                ),
                Chrome::muted(),
            ),
        ]);
        frame.render_widget(Paragraph::new(head), Rect { y: head_y, height: 1, ..area });
        let list_rect = Rect {
            y: head_y.saturating_add(1),
            height: area.height.saturating_sub(pane.height).saturating_sub(3),
            ..area
        };
        let draft = self.draft.clone();
        self.builder.draw(frame, list_rect, &draft);
        self.list_area = list_rect;
        // Every hint is a button too, so the row keeps to what fits 80
        // columns; the rest of the keys are on the help page.
        self.draw_status(
            frame,
            area,
            &[
                ("enter", "edit"),
                ("m", "module"),
                ("a", "row"),
                ("C", "column"),
                ("b", "box"),
                ("u", "undo"),
                ("s", "save"),
                ("q", "quit"),
                ("?", "help"),
            ],
        );
    }
}

/// A title as a box name: lower case, runs of anything else as one `-`
/// (`"Repo & CI"` → `repo-ci`).
fn bare_key_of(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::is_bare_key;
    use crate::setup::draft::dropped_boxes;

    #[test]
    fn titles_become_box_names() {
        assert_eq!(bare_key_of("Repo & CI"), "repo-ci");
        assert_eq!(bare_key_of("  Usage  "), "usage");
        assert_eq!(bare_key_of("· · ·"), "", "nothing bare in it: the caller falls back");
        assert_eq!(bare_key_of("a_b-c"), "a_b-c");
        assert!(is_bare_key("side-panel_2") && !is_bare_key("") && !is_bare_key("a b"));
        assert_eq!(
            dropped_boxes(&["a".to_owned(), "b".to_owned()]),
            "[box.a], [box.b] dropped, nothing used it"
        );
    }
}
