//! The `setup` screen's state (SPEC § 14).
//!
//! The home menu, the preset picker, the builder and the install screen,
//! the overlays stacked on them, and the one draft they all edit. Every
//! input is a value, so the whole screen runs without a terminal in the
//! snapshot tests.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use toml::Value;

use super::builder::Builder;
use super::draft::{Draft, RowAt};
use super::form::{Form, FormKind, Slot, SlotKind, Suggestions};
use super::pick::{Choice, Choose, Confirm, Help, InputBox, Layer, Outcome, Question, Target};
use super::preview::{Preview, Rendered};
use super::ui::{Chrome, cells, hints, window};
use crate::ansi::{ColorMode, Painter};
use crate::config::{Config, ConfigError};
use crate::install::{Applied, Options, Refusal, Steps};
use crate::layout::Elem;

/// A key press, as the screen sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// `Ctrl` with a character (other than `c`).
    Ctrl(char),
    /// `Ctrl+C`: leave at once.
    CtrlC,
    /// Enter.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
}

/// A mouse action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    /// A left click.
    Click,
    /// The wheel, negative up.
    Wheel(i8),
}

/// One input to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A key.
    Key(Key),
    /// A mouse action at a cell.
    Mouse {
        /// Column.
        x: u16,
        /// Row.
        y: u16,
        /// What happened.
        kind: Mouse,
    },
    /// The terminal is `width × height` now.
    Resize(u16, u16),
    /// A quarter second passed: the clock moves on.
    Tick,
}

/// What an overlay asks the app to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Set a key.
    Set(Slot, Value),
    /// Remove a key.
    Unset(Slot),
    /// A list entry was picked for a target.
    Picked(Target, String),
    /// Text was typed for a target.
    Typed(Target, String),
    /// A question was answered.
    Answered(Question, bool),
}

/// Which screen is up.
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Home,
    Picker(Box<Picker>),
    Builder,
    Install(Box<InstallScreen>),
}

/// The preset picker.
#[derive(Debug, Clone, PartialEq)]
struct Picker {
    items: Vec<PickItem>,
    cursor: usize,
    scroll: usize,
    /// The highlighted preset resolved, so a draw does not parse it again.
    shown: Option<(usize, Draft, Config, Vec<ConfigError>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PickItem {
    name: String,
    summary: String,
    columns: Option<usize>,
    needs: Option<String>,
}

impl Picker {
    fn new() -> Self {
        let mut items: Vec<PickItem> = crate::config::presets::TopPreset::ALL
            .iter()
            .map(|p| PickItem {
                name: p.name().to_owned(),
                summary: match p {
                    crate::config::presets::TopPreset::Default => {
                        "four lines, every module at its default".to_owned()
                    }
                    crate::config::presets::TopPreset::Minimal => {
                        "one unframed line, the bare values".to_owned()
                    }
                    crate::config::presets::TopPreset::Full => {
                        "four lines, everything each module knows".to_owned()
                    }
                    crate::config::presets::TopPreset::Compact => "two lines".to_owned(),
                },
                columns: None,
                needs: None,
            })
            .collect();
        items.extend(crate::gallery::PRESETS.iter().map(|p| PickItem {
            name: p.name.to_owned(),
            summary: p.summary.clone(),
            columns: Some(p.columns),
            needs: p.needs.clone(),
        }));
        Self { items, cursor: 0, scroll: 0, shown: None }
    }

    fn item(&self) -> Option<&PickItem> {
        self.items.get(self.cursor)
    }

    /// The highlighted preset's draft and config, resolved once per move.
    fn shown(&mut self) -> Option<(&Draft, &Config, &[ConfigError])> {
        if self.shown.as_ref().is_none_or(|(i, ..)| *i != self.cursor) {
            let draft = Draft::from_preset(&self.item()?.name)?;
            let (config, problems) = draft.resolved();
            self.shown = Some((self.cursor, draft, config, problems));
        }
        self.shown.as_ref().map(|(_, d, c, p)| (d, c, p.as_slice()))
    }
}

/// The install screen: the plan, and what applying it did.
#[derive(Debug, Clone, PartialEq)]
struct InstallScreen {
    steps: Result<Steps, Refusal>,
    applied: Option<Result<Applied, Refusal>>,
    /// Where `Esc` goes back to.
    back: Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Back {
    Home,
    Builder,
}

/// How urgent a status line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Info,
    Warn,
    Error,
}

/// The narrowest and shortest terminal the screen lays out for.
pub const MIN_SIZE: (u16, u16) = (60, 12);

/// How many edits `u` can take back.
const HISTORY_LIMIT: usize = 100;

/// The draft before an edit, with the list cursor of the time and the
/// status line the edit produced (what `u` says it undid).
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    table: toml::Table,
    cursor: usize,
    chip: Option<usize>,
    what: String,
}

/// The undo and redo stacks (SPEC § 14): every input that changed the
/// draft's table pushes the table it replaced.
#[derive(Debug, Clone, Default, PartialEq)]
struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// The screen.
#[derive(Debug, Clone)]
pub struct App {
    draft: Draft,
    config: Config,
    problems: Vec<ConfigError>,
    preview: Preview,
    screen: Screen,
    builder: Builder,
    layers: Vec<Layer>,
    status: Option<(String, Level)>,
    size: (u16, u16),
    home_cursor: usize,
    rendered: Rendered,
    pane_area: Rect,
    list_area: Rect,
    /// The cells of each key hint on the bottom row, so a click on one is
    /// that key (the hint bar is the screen's buttons).
    hint_hits: Vec<(u16, u16, Key)>,
    hint_row: u16,
    history: History,
    suggestions: Suggestions,
    options: Options,
    home: Option<PathBuf>,
    quit: bool,
    comments_note_shown: bool,
    no_color: bool,
}

impl App {
    /// A screen over `draft`, previewing on `preview`, installing with
    /// `options`; paths under `home` are shown as `~/…`. It opens in the
    /// builder when the draft has a file behind it, on the home menu
    /// otherwise (SPEC § 14).
    #[must_use]
    pub fn new(
        draft: Draft,
        preview: Preview,
        options: Options,
        home: Option<PathBuf>,
        no_color: bool,
    ) -> Self {
        let has_file = draft.path().is_some_and(std::path::Path::exists);
        let (config, problems) = draft.resolved();
        let mut app = Self {
            draft,
            config,
            problems,
            preview,
            screen: if has_file { Screen::Builder } else { Screen::Home },
            builder: Builder::default(),
            layers: Vec::new(),
            status: None,
            size: (80, 24),
            home_cursor: 0,
            rendered: Rendered::default(),
            pane_area: Rect::default(),
            list_area: Rect::default(),
            hint_hits: Vec::new(),
            hint_row: 0,
            history: History::default(),
            suggestions: Suggestions::gather(),
            options,
            home,
            quit: false,
            comments_note_shown: false,
            no_color,
        };
        if has_file {
            // A file that only names a preset has no `[[row]]` to list.
            app.draft.materialise_rows_as_read();
        }
        app.builder.rebuild(&app.draft);
        if let Some(problem) = app.draft.unreadable() {
            app.say(
                format!("the file does not parse ({problem}); opened on the defaults, and s will not overwrite it"),
                Level::Error,
            );
        } else if has_file && !app.problems.is_empty() {
            app.say(
                format!(
                    "{}; a save keeps the key as written (d in its form unsets it)",
                    app.first_problem()
                ),
                Level::Warn,
            );
        }
        app
    }

    /// Whether the screen asked to quit.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.quit
    }

    /// The draft as it stands (tests).
    #[must_use]
    pub const fn draft(&self) -> &Draft {
        &self.draft
    }

    /// The module the builder's cursor is on, when it is on one (tests).
    #[must_use]
    pub fn selected(&self) -> Option<&str> {
        self.builder.selected_id()
    }

    /// The status line, when one is up (tests).
    #[must_use]
    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().map(|(s, _)| s.as_str())
    }

    /// The keys of the open form's fields, when a form is the top layer
    /// (tests).
    #[must_use]
    pub fn form_keys(&self) -> Option<Vec<String>> {
        match self.layers.last()? {
            Layer::Form(f) => Some(f.fields.iter().map(|f| f.key.clone()).collect()),
            _ => None,
        }
    }

    /// A path as the screen shows it: under the home directory as `~/…`.
    fn shown(&self, path: &Path) -> String {
        self.home
            .as_deref()
            .and_then(|home| path.strip_prefix(home).ok())
            .map_or_else(|| path.display().to_string(), |rest| format!("~/{}", rest.display()))
    }

    /// Open the builder straight away (the picker's `e`, tests).
    pub fn open_builder(&mut self) {
        self.screen = Screen::Builder;
    }

    fn say(&mut self, text: String, level: Level) {
        self.status = Some((text, level));
    }

    fn first_problem(&self) -> String {
        let extra = self.problems.len().saturating_sub(1);
        self.problems.first().map_or_else(String::new, |p| {
            if extra > 0 { format!("{p} (+{extra} more)") } else { p.to_string() }
        })
    }

    /// Re-resolve the draft after an edit and rebuild what depends on it,
    /// naming the first problem the edit introduced, if any.
    fn refresh(&mut self) -> Option<String> {
        let (config, problems) = self.draft.resolved();
        let new = problems
            .iter()
            .find(|p| !self.problems.contains(p))
            .map(|p| format!("{}: {}", p.path, p.message));
        self.config = config;
        self.problems = problems;
        self.builder.rebuild(&self.draft);
        self.refresh_form();
        new
    }

    /// Rebuild an open form so its values show the edit just made; a form
    /// whose subject went (a text module's editor open while `Ctrl+Z` took
    /// the module back) closes with the layers over it.
    fn refresh_form(&mut self) {
        let Some(Layer::Form(form)) = self.layers.first_mut() else { return };
        let mut rebuilt =
            Form::build(form.kind.clone(), &self.draft, &self.config, &self.suggestions);
        if rebuilt.fields.is_empty() {
            self.layers.clear();
            return;
        }
        rebuilt.cursor = form.cursor.min(rebuilt.fields.len().saturating_sub(1));
        *form = rebuilt;
    }

    /// Feed one input in. A key or a click in the builder that changes the
    /// draft's table leaves the table it replaced on the undo stack; a
    /// click on a hint of the bottom bar is that hint's key.
    pub fn input(&mut self, input: Input) {
        match input {
            Input::Tick => self.preview.tick(),
            Input::Resize(w, h) => self.size = (w, h),
            Input::Key(Key::CtrlC) => self.quit = true,
            Input::Key(key) => {
                if self.is_undo_key(key) {
                    self.undo_key(key);
                    return;
                }
                let before = (self.screen == Screen::Builder).then(|| self.snapshot());
                self.key(key);
                if let Some(before) = before {
                    self.remember(before);
                }
            }
            Input::Mouse { x, y, kind } => {
                if kind == Mouse::Click
                    && self.layers.is_empty()
                    && let Some(key) = self.hint_at(x, y)
                {
                    self.input(Input::Key(key));
                    return;
                }
                let before = (self.screen == Screen::Builder).then(|| self.snapshot());
                self.mouse(x, y, kind);
                if let Some(before) = before {
                    self.remember(before);
                }
            }
        }
    }

    /// The key of the hint under a cell of the bottom bar, when one is.
    fn hint_at(&self, x: u16, y: u16) -> Option<Key> {
        (y == self.hint_row)
            .then(|| self.hint_hits.iter().find(|(start, end, _)| x >= *start && x < *end))
            .flatten()
            .map(|(_, _, key)| *key)
    }

    /// The draft and the list cursor as they stand, for the history.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            table: self.draft.table().clone(),
            cursor: self.builder.cursor,
            chip: self.builder.chip,
            what: String::new(),
        }
    }

    /// Keep `before` when the input just handled changed the table; a new
    /// edit ends the redo chain.
    fn remember(&mut self, mut before: Snapshot) {
        if self.draft.table() == &before.table {
            return;
        }
        before.what = self.status.as_ref().map_or_default(|(s, _)| s.clone());
        self.history.redo.clear();
        self.history.undo.push(before);
        if self.history.undo.len() > HISTORY_LIMIT {
            self.history.undo.remove(0);
        }
    }

    /// `u`/`U` in the builder, `Ctrl+Z`/`Ctrl+R` there or in a form: the
    /// undo keys, which never count as edits themselves. Typing into a
    /// picker's filter or an input line keeps every letter.
    fn is_undo_key(&self, key: Key) -> bool {
        let over_form = matches!(self.layers.last(), None | Some(Layer::Form(_)));
        let builder = self.screen == Screen::Builder;
        match key {
            Key::Ctrl('z' | 'r') => builder && over_form,
            Key::Char('u' | 'U') => builder && self.layers.is_empty(),
            _ => false,
        }
    }

    fn undo_key(&mut self, key: Key) {
        let redo = matches!(key, Key::Char('U') | Key::Ctrl('r'));
        let mut now = self.snapshot();
        let (from, to) = if redo {
            (&mut self.history.redo, &mut self.history.undo)
        } else {
            (&mut self.history.undo, &mut self.history.redo)
        };
        let Some(target) = from.pop() else {
            self.say(
                if redo { "nothing to redo".to_owned() } else { "nothing to undo".to_owned() },
                Level::Info,
            );
            return;
        };
        now.what.clone_from(&target.what);
        to.push(now);
        self.draft.replace_table(target.table);
        self.builder.cursor = target.cursor;
        self.builder.chip = target.chip;
        // A row's, a column's or a box's form is about a place in the file,
        // which the table put back may have moved or removed; it closes
        // rather than edit another table under the same path. A module's
        // or a top-level form is about a name and is rebuilt in place.
        let positional = matches!(
            self.layers.first(),
            Some(Layer::Form(f)) if matches!(f.kind, FormKind::Row(_) | FormKind::Col(_) | FormKind::Box(_))
        );
        if positional {
            self.layers.clear();
        }
        self.refresh();
        let what = if target.what.is_empty() { "the last edit".to_owned() } else { target.what };
        self.say(format!("{}: {what}", if redo { "redone" } else { "undone" }), Level::Info);
    }

    fn key(&mut self, key: Key) {
        if let Some(layer) = self.layers.last_mut() {
            let outcome = match layer {
                Layer::Form(f) => f.handle(key),
                Layer::Choose(c) => c.handle(key),
                Layer::Input(i) => i.handle(key),
                Layer::Confirm(c) => c.handle(key),
                Layer::Help(h) => h.handle(key),
            };
            self.outcome(outcome);
            return;
        }
        if key == Key::Char('?') {
            self.layers.push(Layer::Help(self.help()));
            return;
        }
        match &self.screen {
            Screen::Home => self.home_key(key),
            Screen::Picker(_) => self.picker_key(key),
            Screen::Builder => self.builder_key(key),
            Screen::Install(_) => self.install_key(key),
        }
    }

    fn outcome(&mut self, outcome: Outcome) {
        if outcome.close {
            self.layers.pop();
        }
        for action in outcome.actions {
            self.apply(action);
        }
        if let Some(layer) = outcome.push {
            self.layers.push(layer);
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Set(slot, value) => {
                if let Err(e) = self.try_set(&slot, value) {
                    self.say(e, Level::Error);
                }
            }
            Action::Unset(slot) => {
                if slot.get(&self.draft).is_none() {
                    self.say(format!("{} is not set", slot.path()), Level::Info);
                    return;
                }
                slot.unset(&mut self.draft);
                // The last member leaving a box takes an unused
                // `[box.<name>]` with it, as the builder's `b` does.
                let dropped = if slot.key == "box" { self.prune_orphan_boxes() } else { None };
                let problem = self.refresh();
                let path = slot.path();
                match (dropped, problem) {
                    (_, Some(problem)) => {
                        self.say(format!("{path} unset; ⚠ {problem}"), Level::Warn);
                    }
                    (Some(dropped), None) => {
                        self.say(format!("{path} unset; {dropped}"), Level::Info);
                    }
                    (None, None) => self.say(format!("{path} unset"), Level::Info),
                }
            }
            // Only the picker's "new text module" entry types into this
            // target: the typed text is the new module's name.
            Action::Typed(Target::AddModule, name) => self.new_text(&name),
            Action::Picked(target, value) | Action::Typed(target, value) => {
                self.chosen(target, &value);
            }
            Action::Answered(question, yes) => self.answered(question, yes),
        }
    }

    /// Set a key, unless the parser would report it: the value is tried on
    /// a copy first and refused with the parser's own message (SPEC § 14:
    /// validated as `config check` would). A value the parser takes but
    /// that leaves another key reported (`fill = false` under a
    /// `fill_pattern`) is set and the status bar names that key.
    fn try_set(&mut self, slot: &Slot, value: Value) -> Result<(), String> {
        let mut trial = self.draft.clone();
        slot.set(&mut trial, value.clone());
        let (_, problems) = trial.resolved();
        let path = slot.path();
        // A problem at the key's own path is about this value, whether or
        // not the file already had one there.
        if let Some(p) = problems.iter().find(|p| p.path == path) {
            return Err(format!("{}: {}", p.path, p.message));
        }
        let swapped = self.swap_preset_rows(slot, &value);
        slot.set(&mut self.draft, value);
        let dropped = if slot.key == "box" { self.prune_orphan_boxes() } else { None };
        let note = [swapped, dropped].into_iter().flatten().fold(String::new(), |mut s, n| {
            s.push_str("; ");
            s.push_str(&n);
            s
        });
        match self.refresh() {
            Some(problem) => self.say(format!("{path} set{note}; ⚠ {problem}"), Level::Warn),
            None => self.say(format!("{path} set{note}"), Level::Info),
        }
        Ok(())
    }

    /// The rows follow a change of the top-level `preset` when they are
    /// still exactly the rows the old preset wrote out (the builder writes
    /// a preset's rows into the file so they can be edited, which would
    /// otherwise pin them). Returns what happened, for the status bar.
    fn swap_preset_rows(&mut self, slot: &Slot, value: &Value) -> Option<String> {
        if slot.path() != "preset" {
            return None;
        }
        let name = value.as_str()?;
        if name == self.config.preset.name() {
            return None;
        }
        let old = Draft::from_preset(self.config.preset.name())?;
        if old.rows() != self.draft.rows() {
            return Some("the rows below stay (p replaces them with a preset's)".to_owned());
        }
        let mut fresh = Draft::from_preset(name)?;
        fresh.materialise_rows();
        let rows = fresh.get(&["row"])?.clone();
        self.draft.set(&["row"], rows);
        Some(format!("rows replaced with the {name} preset's"))
    }

    /// Drop every `[box.<name>]` nothing joins any more; says which, when
    /// any.
    fn prune_orphan_boxes(&mut self) -> Option<String> {
        let orphans = self.draft.prune_orphan_boxes();
        (!orphans.is_empty()).then(|| dropped_boxes(&orphans))
    }

    fn chosen(&mut self, target: Target, value: &str) {
        match target {
            Target::Slot(slot, kind) => {
                // A title emptied is a title removed, as its prompt says; a
                // box name nobody defined yet gets its `[box.<name>]`, as
                // the builder's `b` gives one, so the form's own entry is
                // not refused for the table it could not have made.
                let title = slot.path().rsplit_once('.').is_some_and(|(_, key)| key == "title");
                if title && value.trim().is_empty() {
                    self.apply(Action::Unset(slot));
                    return;
                }
                let new_box = kind == SlotKind::BoxRef
                    && !matches!(value, "" | "none" | "false" | "true")
                    && !self.config.boxes.contains_key(value);
                if new_box {
                    if !is_bare_key(value) {
                        self.say(
                            "a box name is letters, digits, _ and - only".into(),
                            Level::Error,
                        );
                        return;
                    }
                    self.draft.set(&["box", value, "title"], Value::String(value.to_owned()));
                }
                match kind.parse(value) {
                    Ok(Some(v)) => self.apply(Action::Set(slot, v)),
                    Ok(None) => self.apply(Action::Unset(slot)),
                    Err(e) => self.say(e, Level::Error),
                }
                // A table made for a value the parser then refused (a box
                // that would nest) must not stay behind as an orphan.
                if new_box && self.prune_orphan_boxes().is_some() {
                    self.refresh();
                }
            }
            Target::AddModule => {
                let out = self.builder.add_module(&mut self.draft, value);
                self.report(out);
            }
            Target::Preset => {
                if self.draft.is_dirty() {
                    let ask = format!("Replace them with the {value} preset?");
                    self.layers.push(Layer::Confirm(Confirm::new(
                        Question::ReplaceDraft(value.to_owned()),
                        &["The draft has unsaved edits.", ask.as_str()],
                        "replace",
                        "keep",
                    )));
                } else {
                    self.load_preset(value);
                }
            }
            Target::Columns => match value.trim().parse::<usize>() {
                Ok(0) | Err(_) if value.trim().is_empty() || value.trim() == "0" => {
                    self.preview.columns = None;
                    self.say("previewing at the terminal's own width".into(), Level::Info);
                }
                Ok(n) => {
                    self.preview.columns = Some(n.clamp(10, 4100));
                    self.say(format!("previewing at {} columns", n.clamp(10, 4100)), Level::Info);
                }
                Err(_) => self.say(format!("{value:?} is not a width"), Level::Error),
            },
            Target::BoxFor(_) => {
                let name = if value == "true" {
                    ""
                } else if value == "none" {
                    "-"
                } else {
                    value
                };
                if name == "-" {
                    let out = self.edit(|builder, draft| {
                        let at = builder.item().map(|i| i.at).ok_or("nothing selected")?;
                        let table = draft.row_mut(at).ok_or("no such row")?;
                        table.remove("box");
                        // The last member leaving takes an unused
                        // `[box.<name>]` with it, which the parser would
                        // otherwise report on every tick.
                        let orphans = draft.prune_orphan_boxes();
                        Ok(if orphans.is_empty() {
                            "unboxed".to_owned()
                        } else {
                            format!("unboxed; {}", dropped_boxes(&orphans))
                        })
                    });
                    self.report(out);
                } else if !name.is_empty() && !is_bare_key(name) {
                    self.say("a box name is letters, digits, _ and - only".into(), Level::Error);
                } else {
                    let out = self.edit(|builder, draft| builder.set_box(draft, name));
                    self.report(out);
                }
            }
            Target::BoxWith(at) => {
                let name = value.trim();
                if !is_bare_key(name) {
                    self.say("a box name is letters, digits, _ and - only".into(), Level::Error);
                    return;
                }
                // The name was asked for this row; the selection cannot move
                // under an input line, but the box goes nowhere else.
                if self.builder.item().map(|i| i.at) != Some(at) {
                    self.say("the selection moved; press B again".into(), Level::Warn);
                    return;
                }
                let out = self.edit(|builder, draft| builder.box_with_above(draft, name));
                self.report(out);
            }
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
                if item.kind == super::builder::ItemKind::Col {
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

    fn report(&mut self, out: Result<String, String>) {
        match out {
            Ok(s) => match self.refresh() {
                Some(problem) => self.say(format!("{s}; ⚠ {problem}"), Level::Warn),
                None => self.say(s, Level::Info),
            },
            Err(e) => self.say(e, Level::Warn),
        }
    }

    /// A builder edit tried on the draft and kept only when the parser
    /// reports nothing new about the result (SPEC § 14: every edit is
    /// validated as `config check` would); otherwise the draft and the
    /// list are put back and the parser's message is the error.
    fn edit(
        &mut self,
        f: impl FnOnce(&mut Builder, &mut Draft) -> Result<String, String>,
    ) -> Result<String, String> {
        let before = (self.draft.clone(), self.builder.clone());
        let result = f(&mut self.builder, &mut self.draft).and_then(|out| {
            let (_, problems) = self.draft.resolved();
            problems
                .iter()
                .find(|p| !self.problems.contains(p))
                .map_or(Ok(out), |p| Err(format!("{}: {}", p.path, p.message)))
        });
        // A refused edit leaves nothing behind, whichever step refused it.
        if result.is_err() {
            (self.draft, self.builder) = before;
        }
        result
    }

    /// Replace the draft with a preset: the built-in name with its rows
    /// written out, or the gallery file. Never over a file that does not
    /// parse (SPEC § 5), and always as an edit, since the file differs.
    fn load_preset(&mut self, name: &str) {
        if let Some(problem) = self.draft.unreadable() {
            self.say(
                format!(
                    "the config file does not parse ({problem}) and is never overwritten; fix or move it first"
                ),
                Level::Error,
            );
            return;
        }
        let Some(mut preset) = Draft::from_preset(name) else {
            self.say(format!("no preset named {name}"), Level::Error);
            return;
        };
        preset.materialise_rows();
        // The table alone is adopted: the draft keeps the file it belongs
        // to and stays dirty exactly while it differs from that file.
        self.draft.replace_table(preset.table().clone());
        self.refresh();
        self.say(format!("preset {name} loaded; s saves it"), Level::Info);
    }

    /// `[modules.text.<name>]` with the schema's defaults (and the name as
    /// its text, so it shows), placed at the cursor, then its editor; a
    /// cursor that cannot take a module leaves nothing behind.
    fn new_text(&mut self, name: &str) {
        let name = name.trim();
        if !is_bare_key(name) {
            self.say("a text module name is letters, digits, _ and - only".into(), Level::Error);
            return;
        }
        let id = format!("{}{name}", crate::modules::text::PREFIX);
        if self.config.texts.contains_key(name) {
            self.say(format!("{id} already exists"), Level::Warn);
            return;
        }
        let out = self.edit(|builder, draft| {
            draft.set(&["modules", "text", name, "text"], Value::String(name.to_owned()));
            builder.add_module(draft, &id)
        });
        let placed = out.is_ok();
        self.report(out);
        if placed {
            self.open_form(FormKind::Module(id));
        }
    }

    fn answered(&mut self, question: Question, yes: bool) {
        match question {
            Question::QuitUnsaved => {
                if yes {
                    self.quit = true;
                }
            }
            Question::OverwriteOrReload => {
                if yes {
                    self.save(true);
                } else {
                    self.draft.reload();
                    self.refresh();
                    self.say("reloaded from disk; the edits were dropped".into(), Level::Info);
                }
            }
            Question::DropText(name) => {
                if yes {
                    self.draft.remove(&["modules", "text", &name]);
                    self.refresh();
                    self.say(format!("dropped [modules.text.{name}]"), Level::Info);
                }
            }
            Question::ReplaceDraft(name) => {
                if yes {
                    self.load_preset(&name);
                }
            }
            Question::Install => {
                if yes {
                    self.install_apply();
                }
            }
        }
    }

    fn open_form(&mut self, kind: FormKind) {
        let form = Form::build(kind, &self.draft, &self.config, &self.suggestions);
        self.layers.push(Layer::Form(form));
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

    fn help(&self) -> Help {
        let (title, keys): (&str, Vec<(&str, &str)>) = match &self.screen {
            Screen::Home => ("Home", vec![("↑↓ enter", "choose"), ("1-4", "jump"), ("q", "quit")]),
            Screen::Picker(_) => (
                "Preset picker",
                vec![
                    ("↑↓", "highlight a preset (previewed above)"),
                    ("enter", "write it to the config file"),
                    ("e", "open it in the builder"),
                    ("f / F", "next / previous sample payload"),
                    ("w", "preview at another terminal width"),
                    ("esc", "back"),
                ],
            ),
            Screen::Builder => (
                "Builder",
                vec![
                    ("↑↓", "select a row, column or inner row"),
                    ("←→ tab", "select a module"),
                    ("enter", "edit the selection"),
                    ("m", "add a module after the selection"),
                    ("x del", "remove the module or line"),
                    ("a / i / c", "add a row after / before, clone the line"),
                    ("J / K", "move the line or module down / up"),
                    ("r", "move the module to the other side"),
                    ("[ / ]", "move the module a column left / right (past the edge: a new one)"),
                    ("C", "add a column after the selection"),
                    ("S", "stack the column (or add an inner row)"),
                    ("t", "title the row"),
                    ("b / B", "box the row or column / box it with the row above"),
                    ("space", "make the row a spacer"),
                    ("u / U", "undo / redo the last edit (also ctrl-z / ctrl-r)"),
                    ("1 2 3", "top-level keys / frame / colours"),
                    ("p", "start from a preset"),
                    ("f / F", "next / previous sample payload"),
                    ("w", "preview at another terminal width"),
                    ("s", "save"),
                    ("I", "install into Claude Code"),
                    ("q / esc", "quit (esc inside a form or picker closes it)"),
                ],
            ),
            Screen::Install(_) => ("Install", vec![("enter", "apply"), ("esc", "back")]),
        };
        Help {
            title: title.to_owned(),
            keys: keys.into_iter().map(|(k, w)| (k.to_owned(), w.to_owned())).collect(),
        }
    }

    fn home_key(&mut self, key: Key) {
        match key {
            Key::Up | Key::Char('k') => {
                self.home_cursor = self.home_cursor.checked_sub(1).unwrap_or(3);
            }
            Key::Down | Key::Char('j') | Key::Tab => {
                self.home_cursor = self.home_cursor.saturating_add(1).checked_rem(4).unwrap_or(0);
            }
            Key::Char(c @ '1'..='4') => {
                self.home_cursor =
                    usize::from(u8::try_from(c).unwrap_or(b'1').saturating_sub(b'1'));
                self.home_enter();
            }
            Key::Enter => self.home_enter(),
            Key::Esc | Key::Char('q') => self.quit = true,
            _ => {}
        }
    }

    fn home_enter(&mut self) {
        match self.home_cursor {
            0 => self.screen = Screen::Picker(Box::new(Picker::new())),
            1 => {
                self.draft.materialise_rows();
                self.refresh();
                self.screen = Screen::Builder;
            }
            2 => self.open_install(Back::Home),
            _ => self.quit = true,
        }
    }

    fn picker_key(&mut self, key: Key) {
        let Screen::Picker(picker) = &mut self.screen else { return };
        let n = picker.items.len();
        match key {
            Key::Up | Key::Char('k') => {
                picker.cursor = picker.cursor.checked_sub(1).unwrap_or_else(|| n.saturating_sub(1));
            }
            Key::Down | Key::Char('j') | Key::Tab => {
                picker.cursor = picker.cursor.saturating_add(1).checked_rem(n.max(1)).unwrap_or(0);
            }
            Key::PageUp => picker.cursor = picker.cursor.saturating_sub(5),
            Key::PageDown => {
                picker.cursor = picker.cursor.saturating_add(5).min(n.saturating_sub(1));
            }
            Key::Home => picker.cursor = 0,
            Key::End => picker.cursor = n.saturating_sub(1),
            Key::Esc | Key::Char('q') => self.screen = Screen::Home,
            Key::Char('f') => self.preview.cycle(true),
            Key::Char('F') => self.preview.cycle(false),
            Key::Char('w') => self.ask_width(),
            Key::Char('e') => self.picker_edit(),
            Key::Enter => self.picker_apply(),
            _ => {}
        }
    }

    fn ask_width(&mut self) {
        let current = self.preview.columns.map_or_else(String::new, |c| c.to_string());
        self.layers.push(Layer::Input(InputBox::new(
            "Preview at a terminal width (columns; empty = the real one)",
            &current,
            Target::Columns,
        )));
    }

    /// `e`: the highlighted preset opens in the builder, unsaved.
    fn picker_edit(&mut self) {
        let Screen::Picker(picker) = &mut self.screen else { return };
        let Some((preset, ..)) = picker.shown() else { return };
        let mut preset = preset.clone();
        // No unparsable-file guard here: the picker opens only when no file
        // exists (`App::new` opens an existing one in the builder), and
        // `Draft::save` refuses such a file anyway.
        preset.materialise_rows();
        self.draft.replace_table(preset.table().clone());
        // A preset adopted from the picker is a fresh start, not an edit.
        self.history = History::default();
        self.refresh();
        self.screen = Screen::Builder;
        self.say("editing the preset; s saves it to the config file".into(), Level::Info);
    }

    /// `Enter`: the highlighted preset becomes the config file, with the
    /// previous file kept as a backup; the install screen follows when the
    /// settings file has no status line yet.
    fn picker_apply(&mut self) {
        let Screen::Picker(picker) = &mut self.screen else { return };
        let Some((preset, ..)) = picker.shown() else { return };
        let mut preset = preset.clone();
        let unreadable = self.draft.unreadable().map(str::to_owned);
        if let Some(problem) = unreadable {
            self.say(format!("the config file does not parse ({problem}) and is never overwritten; fix or move it first"), Level::Error);
            return;
        }
        preset.materialise_rows();
        let mut draft = self.draft.clone();
        draft.replace_table(preset.table().clone());
        match draft.save() {
            Ok(backup) => {
                let path = draft.path().map_or_else(String::new, |p| self.shown(p));
                let note =
                    backup.map_or_else(String::new, |b| format!(" (backup: {})", self.shown(&b)));
                self.draft = draft;
                self.history = History::default();
                self.refresh();
                self.say(format!("wrote {path}{note}"), Level::Info);
                let installed = Steps::plan(&self.options).is_ok_and(|s| s.statusline_configured());
                if installed {
                    self.screen = Screen::Builder;
                } else {
                    self.open_install(Back::Builder);
                }
            }
            Err(e) => self.say(e, Level::Error),
        }
    }

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

    fn builder_key(&mut self, key: Key) {
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
            Key::Char('t') => {
                if let Some(at) = self.builder.item().map(|i| i.at) {
                    // SPEC § 4.3: a title sits in a row's rule; a column has
                    // none, its rows and its box do.
                    if at.col.is_some() && at.inner.is_none() {
                        self.say(
                            "a column has no title; title its rows, or box it (b)".into(),
                            Level::Warn,
                        );
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
            }
            Key::Char('b') => {
                if let Some(at) = self.builder.item().map(|i| i.at) {
                    let mut items = vec![
                        Choice::noted("none", "no box"),
                        Choice::noted("true", "a box of its own"),
                    ];
                    items.extend(self.config.boxes.keys().map(|n| Choice::noted(n, "[box] table")));
                    items.push(Choice::custom("A new box named"));
                    let mut choose = Choose::new("Box", items, Target::BoxFor(at));
                    // The pick opens on the box the row is in.
                    let current = self.draft.row(at).and_then(|t| t.get("box")).map_or_else(
                        || "none".to_owned(),
                        |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                    );
                    choose.select(&current);
                    self.layers.push(Layer::Choose(choose));
                }
            }
            Key::Char('B') => self.box_with_above(),
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
        let kind = if item.kind == super::builder::ItemKind::Col {
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

    /// `s`: write the draft, after the change check (SPEC § 14).
    fn save(&mut self, force: bool) {
        if !force && self.draft.changed_on_disk() {
            self.layers.push(Layer::Confirm(Confirm::new(
                Question::OverwriteOrReload,
                &[
                    "The file changed on disk since it was read.",
                    "Overwrite it with the draft, or reload it?",
                ],
                "overwrite",
                "reload",
            )));
            return;
        }
        match self.draft.save() {
            Ok(backup) => {
                let path = self.draft.path().map_or_else(String::new, |p| self.shown(p));
                let mut note =
                    backup.map_or_else(String::new, |b| format!(" (backup: {})", self.shown(&b)));
                if !self.comments_note_shown {
                    self.comments_note_shown = true;
                    if !note.is_empty() {
                        note.push_str(
                            "; a hand-written file's comments live on in the backup only",
                        );
                    }
                }
                self.say(format!("saved {path}{note}"), Level::Info);
            }
            Err(e) => self.say(e, Level::Error),
        }
    }

    fn open_install(&mut self, back: Back) {
        let steps = Steps::plan(&self.options);
        self.screen = Screen::Install(Box::new(InstallScreen { steps, applied: None, back }));
    }

    fn install_key(&mut self, key: Key) {
        let Screen::Install(screen) = &self.screen else { return };
        match key {
            Key::Enter if screen.applied.is_none() && screen.steps.is_ok() => {
                self.layers.push(Layer::Confirm(Confirm::new(
                    Question::Install,
                    &["Write the statusLine block into settings.json (a backup is kept)?"],
                    "install",
                    "not now",
                )));
            }
            Key::Esc | Key::Char('q') | Key::Enter => {
                self.screen = match screen.back {
                    Back::Home => Screen::Home,
                    Back::Builder => Screen::Builder,
                };
            }
            _ => {}
        }
    }

    fn install_apply(&mut self) {
        let Screen::Install(screen) = &mut self.screen else { return };
        if let Ok(steps) = &screen.steps {
            screen.applied = Some(steps.apply());
        }
    }

    fn mouse(&mut self, x: u16, y: u16, kind: Mouse) {
        if !self.layers.is_empty() {
            if let (Mouse::Wheel(d), Some(layer)) = (kind, self.layers.last_mut()) {
                let key = if d < 0 { Key::Up } else { Key::Down };
                let outcome = match layer {
                    Layer::Form(f) => f.handle(key),
                    Layer::Choose(c) => c.handle(key),
                    _ => Outcome::default(),
                };
                self.outcome(outcome);
            }
            return;
        }
        let pane = self.pane_area;
        let list = self.list_area;
        let inside = |r: Rect| x >= r.x && x < r.right() && y >= r.y && y < r.bottom();
        match (kind, &self.screen) {
            (Mouse::Wheel(d), Screen::Picker(_)) => {
                self.picker_key(if d < 0 { Key::Up } else { Key::Down });
            }
            (Mouse::Wheel(d), Screen::Builder) => self.builder.move_line(d > 0),
            (Mouse::Wheel(d), Screen::Home) => {
                self.home_key(if d < 0 { Key::Up } else { Key::Down });
            }
            (Mouse::Click, Screen::Builder) if inside(pane) => {
                let cx = usize::from(x.saturating_sub(pane.x));
                let cy = usize::from(y.saturating_sub(pane.y));
                self.click_preview(cx, cy);
            }
            (Mouse::Click, Screen::Builder) if inside(list) => {
                let was = (self.builder.cursor, self.builder.chip);
                if self.builder.click(usize::from(x), usize::from(y), list)
                    && was == (self.builder.cursor, self.builder.chip)
                {
                    self.edit_selection();
                }
            }
            (Mouse::Click, Screen::Picker(_)) if inside(list) => {
                let line = usize::from(y.saturating_sub(list.y));
                let Screen::Picker(picker) = &mut self.screen else { return };
                let at = line.saturating_add(picker.scroll);
                if at < picker.items.len() {
                    if picker.cursor == at {
                        self.picker_apply();
                    } else {
                        picker.cursor = at;
                    }
                }
            }
            (Mouse::Click, Screen::Home) if inside(list) => {
                let line = usize::from(y.saturating_sub(list.y));
                if line < 4 {
                    self.home_cursor = line;
                    self.home_enter();
                }
            }
            _ => {}
        }
    }

    /// A click in the preview (SPEC § 14): a module selects it (again
    /// opens its editor), a rule or cap opens the frame form, a separator
    /// its field, a title or box edge the row's form.
    fn click_preview(&mut self, x: usize, y: usize) {
        // The pane has a two-cell gutter for the row marker.
        let Some(hit) = self.rendered.at(x.saturating_sub(2), y) else { return };
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

    /// Draw the screen.
    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        self.size = (area.width, area.height);
        if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
            let text = format!(
                "garnish setup needs at least {}×{} cells; this terminal is {}×{}",
                MIN_SIZE.0, MIN_SIZE.1, area.width, area.height
            );
            frame.render_widget(Paragraph::new(text), area);
            return;
        }
        match self.screen.clone() {
            Screen::Home => self.draw_home(frame, area),
            Screen::Picker(_) => self.draw_picker(frame, area),
            Screen::Builder => self.draw_builder(frame, area),
            Screen::Install(screen) => self.draw_install(frame, area, &screen),
        }
        for layer in &mut self.layers {
            match layer {
                Layer::Form(f) => f.draw(frame, area),
                Layer::Choose(c) => c.draw(frame, area),
                Layer::Input(i) => i.draw(frame, area),
                Layer::Confirm(c) => c.draw(frame, area),
                Layer::Help(h) => h.draw(frame, area),
            }
        }
    }

    const fn painter(&self, config: &Config) -> Painter {
        let mode = config.color.mode(self.no_color);
        Painter { mode, links: false, dim: true }
    }

    /// The preview pane: a title line, then the rendered lines with a
    /// marker beside those of the selected row. Returns the area used.
    fn draw_pane(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        config: &Config,
        selected_row: Option<usize>,
        selected_id: Option<&str>,
        max_height: u16,
    ) -> Rect {
        let rendered = self.preview.render(config, usize::from(self.size.0));
        let painter = self.painter(config);
        let count = rendered.lines.len();
        let budget = usize::from(self.size.1).checked_div(2).unwrap_or(0).saturating_sub(5);
        // The numbers first and the fixture's summary last, since the line
        // is cut at the right edge; the warnings get lines of their own.
        let title = vec![
            Span::styled("preview", Chrome::title()),
            Span::styled(
                format!(
                    "  {} cols, box {}  ·  {} line{}  ·  {}: {}",
                    rendered.columns,
                    rendered.width,
                    count,
                    if count == 1 { "" } else { "s" },
                    self.preview.fixture_name(),
                    self.preview.fixture_summary(),
                ),
                Chrome::muted(),
            ),
        ];
        let mut notes: Vec<String> = Vec::new();
        if count > budget {
            notes.push(format!(
                "⚠ fullscreen keeps {budget} of the {count} lines whole at this height"
            ));
        }
        if painter.mode == ColorMode::Never {
            notes.push("⚠ colours off: edits are saved, not previewed".to_owned());
        }
        let mut lines: Vec<Line<'static>> = vec![Line::from(title)];
        lines.extend(super::ui::note_lines(&notes, area.width));
        let header = cells(lines.len());
        let selection = Style::new().add_modifier(Modifier::REVERSED);
        for placed in &rendered.lines {
            let marker = if Some(placed.row) == selected_row { "▶ " } else { "  " };
            let mut spans: Vec<Span<'static>> = vec![Span::styled(marker, Chrome::key())];
            for piece in &placed.line.pieces {
                let owned = selected_id
                    .is_some_and(|id| piece.elem.owners(0).iter().any(|(o, _)| o == id))
                    || matches!(&piece.elem, Elem::Group(map) if selected_id.is_some_and(|id| map.iter().any(|(o, _)| o == id)));
                let extra = owned.then_some(selection);
                spans.extend(super::paint::line(&painter, &piece.segs, extra).spans);
            }
            lines.push(Line::from(spans));
        }
        let height = cells(lines.len()).min(max_height).max(header.saturating_add(1));
        let rect = Rect { height, ..area };
        frame.render_widget(Paragraph::new(lines), rect);
        self.pane_area = Rect {
            y: rect.y.saturating_add(header),
            height: rect.height.saturating_sub(header),
            ..rect
        };
        self.rendered = rendered;
        rect
    }

    /// The status line and, under it, the key hints, each hint recorded
    /// with its cells so a click on it presses the key.
    fn draw_status(&mut self, frame: &mut Frame<'_>, area: Rect, keys: &[(&str, &str)]) {
        let status = self.status.clone().or_else(|| {
            (!self.problems.is_empty())
                .then(|| (format!("⚠ {}", self.first_problem()), Level::Warn))
        });
        let line = status.map_or_else(
            || Line::from(""),
            |(text, level)| {
                let style = match level {
                    Level::Info => Chrome::muted(),
                    Level::Warn => Chrome::warn(),
                    Level::Error => Chrome::error(),
                };
                Line::from(Span::styled(super::ui::clip(&text, usize::from(area.width)), style))
            },
        );
        let y = area.bottom().saturating_sub(2);
        frame.render_widget(Paragraph::new(line), Rect { y, height: 1, ..area });
        self.hint_row = y.saturating_add(1);
        frame.render_widget(
            Paragraph::new(hints(keys)),
            Rect { y: self.hint_row, height: 1, ..area },
        );
        self.hint_hits = super::ui::hint_cells(keys)
            .into_iter()
            .filter_map(|(label, start, end)| {
                let key = hint_key(label)?;
                let start = area.x.saturating_add(cells(start));
                let end = area.x.saturating_add(cells(end)).min(area.right());
                Some((start, end, key))
            })
            .collect();
    }

    fn draw_home(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let config = self.config.clone();
        let pane = self.draw_pane(
            frame,
            area,
            &config,
            None,
            None,
            area.height.checked_div(2).unwrap_or(1),
        );
        let items = ["Pick a preset", "Build a custom layout", "Install into Claude Code", "Quit"];
        let mut lines: Vec<Line<'static>> = vec![
            Line::from(""),
            Line::from(Span::styled("garnish setup", Chrome::title())),
            Line::from(Span::styled(
                self.draft.path().map_or_else(
                    || "no config file yet".to_owned(),
                    |p| {
                        format!(
                            "config: {}{}",
                            self.shown(p),
                            if p.exists() { "" } else { " (not written yet)" }
                        )
                    },
                ),
                Chrome::muted(),
            )),
            Line::from(""),
        ];
        let list_y = area.y.saturating_add(pane.height).saturating_add(cells(lines.len()));
        for (i, item) in items.iter().enumerate() {
            let style = if i == self.home_cursor { Chrome::selected() } else { Style::new() };
            lines.push(Line::from(Span::styled(
                format!(" {}  {item} ", i.saturating_add(1)),
                style,
            )));
        }
        let rect = Rect {
            y: area.y.saturating_add(pane.height),
            height: area.height.saturating_sub(pane.height).saturating_sub(2),
            ..area
        };
        frame.render_widget(Paragraph::new(lines), rect);
        self.list_area = Rect { y: list_y, height: 4, ..area };
        self.draw_status(
            frame,
            area,
            &[("↑↓", "choose"), ("enter", "open"), ("q", "quit"), ("?", "help")],
        );
    }

    fn draw_picker(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Screen::Picker(picker) = &mut self.screen else { return };
        let Some((_, config, problems)) =
            picker.shown().map(|(d, c, p)| (d.clone(), c.clone(), p.to_vec()))
        else {
            return;
        };
        let item = picker.item().cloned();
        let (cursor, len) = (picker.cursor, picker.items.len());
        let pane = self.draw_pane(
            frame,
            area,
            &config,
            None,
            None,
            area.height.checked_div(2).unwrap_or(1),
        );
        // The facts first and the summary last on the info line, which is
        // cut at the right edge; the warnings get lines of their own.
        let mut info: Vec<Span<'static>> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        if let Some(item) = &item {
            info.push(Span::styled(item.name.clone(), Chrome::title()));
            if let Some(needs) = &item.needs {
                info.push(Span::styled(format!("  needs {needs}"), Chrome::muted()));
            }
            if let Some(columns) = item.columns {
                info.push(Span::styled(
                    format!("  designed for {columns} columns"),
                    Chrome::muted(),
                ));
                if usize::from(self.size.0) < columns {
                    notes.push(format!(
                        "⚠ this terminal is {} wide, so the cut shows as it would on screen",
                        self.size.0
                    ));
                }
            }
            info.push(Span::styled(format!("  {}", item.summary), Chrome::muted()));
            if !problems.is_empty() {
                notes.push(format!("⚠ {} problem(s)", problems.len()));
            }
        }
        let mut info_lines = vec![Line::from(info)];
        info_lines.extend(super::ui::note_lines(&notes, area.width));
        let info_rect =
            Rect { y: area.y.saturating_add(pane.height), height: cells(info_lines.len()), ..area };
        frame.render_widget(Paragraph::new(info_lines), info_rect);
        let list_rect = Rect {
            y: info_rect.y.saturating_add(info_rect.height),
            height: area
                .height
                .saturating_sub(pane.height)
                .saturating_sub(info_rect.height)
                .saturating_sub(2),
            ..area
        };
        let Screen::Picker(picker) = &mut self.screen else { return };
        picker.scroll = window(cursor, len, usize::from(list_rect.height), picker.scroll);
        let width = usize::from(list_rect.width);
        let lines: Vec<Line<'static>> = picker
            .items
            .iter()
            .enumerate()
            .skip(picker.scroll)
            .take(usize::from(list_rect.height))
            .map(|(i, p)| {
                let text = super::ui::clip(&format!(" {:<24} {}", p.name, p.summary), width);
                let style = if i == cursor { Chrome::selected() } else { Style::new() };
                Line::from(Span::styled(text, style))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), list_rect);
        self.list_area = list_rect;
        self.draw_status(
            frame,
            area,
            &[
                ("↑↓", "preset"),
                ("enter", "use it"),
                ("e", "edit it"),
                ("f", "payload"),
                ("w", "width"),
                ("esc", "back"),
                ("?", "help"),
            ],
        );
    }

    fn draw_builder(&mut self, frame: &mut Frame<'_>, area: Rect) {
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

    fn draw_install(&mut self, frame: &mut Frame<'_>, area: Rect, screen: &InstallScreen) {
        let mut lines: Vec<Line<'static>> = vec![
            Line::from(Span::styled("Install into Claude Code", Chrome::title())),
            Line::from(""),
        ];
        match &screen.steps {
            Err(e) => lines.push(Line::from(Span::styled(e.to_string(), Chrome::error()))),
            Ok(steps) => {
                lines.push(Line::from(format!(
                    "settings file   {}",
                    self.shown(&steps.plan.settings)
                )));
                lines.push(Line::from(format!(
                    "statusLine      {{ \"type\": \"command\", \"command\": {:?}, \"refreshInterval\": {}{} }}",
                    steps.plan.command,
                    steps.plan.refresh_interval,
                    steps.plan.padding.map_or_else(String::new, |p| format!(", \"padding\": {p}"))
                )));
                lines.push(Line::from(if steps.settings_up_to_date() {
                    "                already up to date".to_owned()
                } else if steps.existing.is_some() {
                    "                the file is kept as a .bak-<epoch> backup next to it"
                        .to_owned()
                } else {
                    "                the file will be created".to_owned()
                }));
                match &steps.config {
                    crate::install::ConfigStep::Skipped => {}
                    crate::install::ConfigStep::Exists { path, .. } => {
                        lines.push(Line::from(format!(
                            "config          {} (kept)",
                            self.shown(path)
                        )));
                    }
                    crate::install::ConfigStep::Write { path, .. } => {
                        lines.push(Line::from(format!(
                            "config          {} (the annotated defaults)",
                            self.shown(path)
                        )));
                    }
                }
                if let Some(dir) = &steps.skills {
                    lines.push(Line::from(format!(
                        "skills          {} skill(s) to {}",
                        crate::skills::SKILLS.len(),
                        self.shown(dir)
                    )));
                }
                for note in steps.notes() {
                    lines.push(Line::from(Span::styled(note, Chrome::warn())));
                }
                lines.push(Line::from(""));
                match &screen.applied {
                    None => lines.push(Line::from(Span::styled(
                        "enter applies this, after one confirmation; esc goes back",
                        Chrome::muted(),
                    ))),
                    Some(Ok(applied)) => {
                        for l in &applied.lines {
                            lines.push(Line::from(Span::styled(l.clone(), Chrome::set())));
                        }
                        lines.push(Line::from(Span::styled(
                            "done; enter or esc goes back",
                            Chrome::muted(),
                        )));
                    }
                    Some(Err(e)) => {
                        lines.push(Line::from(Span::styled(e.to_string(), Chrome::error())));
                    }
                }
            }
        }
        frame.render_widget(
            Paragraph::new(lines),
            Rect { height: area.height.saturating_sub(2), ..area },
        );
        self.draw_status(frame, area, &[("enter", "apply"), ("esc", "back"), ("?", "help")]);
    }
}

/// The status line for `[box.<name>]` tables dropped as orphans.
fn dropped_boxes(names: &[String]) -> String {
    format!("[box.{}] dropped, nothing used it", names.join("], [box."))
}

/// A bare TOML key: what a box or text module may be called.
fn is_bare_key(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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

/// The key a hint's label stands for, when a click on it can press one: a
/// single character, `enter`, `esc` or `ctrl-<c>`; a label naming several
/// keys (`↑↓`, `1 2 3`) presses none.
fn hint_key(label: &str) -> Option<Key> {
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(Key::Char(c)),
        _ => match label {
            "enter" => Some(Key::Enter),
            "esc" => Some(Key::Esc),
            _ => label.strip_prefix("ctrl-").and_then(|c| c.chars().next()).map(Key::Ctrl),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_labels_and_titles_map_to_keys_and_box_names() {
        assert_eq!(hint_key("u"), Some(Key::Char('u')));
        assert_eq!(hint_key("?"), Some(Key::Char('?')));
        assert_eq!(hint_key("enter"), Some(Key::Enter));
        assert_eq!(hint_key("esc"), Some(Key::Esc));
        assert_eq!(hint_key("ctrl-u"), Some(Key::Ctrl('u')));
        for several in ["1 2 3", "x del", "↑↓", "↑↓←→", ""] {
            assert_eq!(hint_key(several), None, "{several:?}");
        }
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
