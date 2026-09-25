//! The `setup` screen's state (SPEC § 14).
//!
//! The home menu, the preset picker, the builder and the install screen,
//! the overlays stacked on them, and the one draft they all edit. Every
//! input is a value, so the whole screen runs without a terminal in the
//! snapshot tests. This file holds the state, the input path and the
//! pieces every screen draws (the preview pane, the status and hint bar);
//! each screen's keys and drawing, the undo history and the actions an
//! overlay asks for live in the modules below.

mod actions;
mod builder_screen;
mod history;
mod home;
mod install;
mod picker;

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use toml::Value;

use self::history::History;
use self::install::InstallScreen;
use self::picker::Picker;
use super::builder::Builder;
use super::draft::Draft;
use super::form::{Form, FormKind, Slot, Suggestions};
use super::pick::{Help, InputBox, Layer, Outcome, Question, Target};
use super::preview::{Preview, Rendered};
use super::ui::{Chrome, cells, hints};
use crate::ansi::{ColorMode, Painter};
use crate::config::{Config, ConfigError};
use crate::install::Options;
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

/// A preview pane as drawn: the rows it took, the rows of its rendered
/// lines (under its title and notes), and those lines' placement map.
struct Pane {
    rect: Rect,
    lines: Rect,
    rendered: Rendered,
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
            no_color,
        };
        if has_file {
            // A file that only names a preset has no `[[row]]` to list.
            app.draft.materialise_rows_as_read();
        }
        app.builder.rebuild(&app.draft);
        if let Some((note, level)) = app.opening_note().filter(|_| has_file) {
            app.say(note, level);
        }
        app
    }

    /// What the status bar says about a file just opened or reloaded: that
    /// it does not parse, its first problem, or that it has comments a save
    /// drops (said before anything is lost, and short enough for 80
    /// columns).
    fn opening_note(&self) -> Option<(String, Level)> {
        if let Some(problem) = self.draft.unreadable() {
            return Some((
                format!(
                    "the file does not parse ({problem}); opened on the defaults, and s will not overwrite it"
                ),
                Level::Error,
            ));
        }
        if !self.problems.is_empty() {
            return Some((
                format!(
                    "{}; a save keeps the key as written (d in its form unsets it)",
                    self.first_problem()
                ),
                Level::Warn,
            ));
        }
        self.draft.loses_comments().then(|| {
            (
                "this file has comments: s writes it without them (its backup keeps them)"
                    .to_owned(),
                Level::Info,
            )
        })
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
        crate::modules::repo::tildify_path(path, self.home.as_deref())
    }

    /// Open the builder straight away (tests: a screen over a text draft,
    /// which starts on the home menu).
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
        let new =
            new_problem(&self.problems, &problems).map(|p| format!("{}: {}", p.path, p.message));
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
    /// click on a hint of the bottom bar is that hint's key. On a terminal
    /// too small to lay the screen out, only the ways out act: nothing a
    /// key or a click would change is on screen.
    pub fn input(&mut self, input: Input) {
        let too_small = self.size.0 < MIN_SIZE.0 || self.size.1 < MIN_SIZE.1;
        match input {
            Input::Tick => self.preview.tick(),
            Input::Resize(w, h) => self.size = (w, h),
            Input::Key(Key::CtrlC) => self.quit = true,
            Input::Key(key) if too_small && !matches!(key, Key::Char('q') | Key::Esc) => {}
            Input::Mouse { .. } if too_small => {}
            Input::Key(key) => {
                if self.is_undo_key(key) {
                    self.undo_key(key);
                    return;
                }
                self.bracketed(|app| app.key(key));
            }
            Input::Mouse { x, y, kind } => {
                if kind == Mouse::Click
                    && self.layers.is_empty()
                    && let Some(key) = self.hint_at(x, y)
                {
                    self.input(Input::Key(key));
                    return;
                }
                self.bracketed(|app| app.mouse(x, y, kind));
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

    fn open_form(&mut self, kind: FormKind) {
        let form = Form::build(kind, &self.draft, &self.config, &self.suggestions);
        self.layers.push(Layer::Form(form));
    }

    /// `w`: ask for the terminal width to preview at.
    fn ask_width(&mut self) {
        let current = self.preview.columns.map_or_else(String::new, |c| c.to_string());
        self.layers.push(Layer::Input(InputBox::new(
            "Preview at a terminal width (columns; empty = the real one)",
            &current,
            Target::Columns,
        )));
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
                    // One line for the three box keys: the page fits 24 rows.
                    ("b / B / e", "box the line / box it with the row above / edit its [box]"),
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

    fn mouse(&mut self, x: u16, y: u16, kind: Mouse) {
        if !self.layers.is_empty() {
            if let (Mouse::Wheel(d), Some(layer)) = (kind, self.layers.last_mut()) {
                let key = wheel_key(d);
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
        let line = usize::from(y.saturating_sub(list.y));
        match (kind, &self.screen) {
            (Mouse::Wheel(d), Screen::Picker(_)) => self.picker_key(wheel_key(d)),
            (Mouse::Wheel(d), Screen::Builder) => self.builder.move_line(d > 0),
            (Mouse::Wheel(d), Screen::Home) => self.home_key(wheel_key(d)),
            (Mouse::Click, Screen::Builder) if inside(pane) => {
                let cx = usize::from(x.saturating_sub(pane.x));
                let cy = usize::from(y.saturating_sub(pane.y));
                self.click_preview(cx, cy);
            }
            (Mouse::Click, Screen::Builder) if inside(list) => self.click_list(x, y),
            (Mouse::Click, Screen::Picker(_)) if inside(list) => self.picker_click(line),
            (Mouse::Click, Screen::Home) if inside(list) => self.home_click(line),
            _ => {}
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
            // Nothing of the last full draw is on screen to be clicked.
            self.hint_hits.clear();
            self.pane_area = Rect::default();
            self.list_area = Rect::default();
            return;
        }
        match self.screen {
            Screen::Home => self.draw_home(frame, area),
            Screen::Picker(_) => self.draw_picker(frame, area),
            Screen::Builder => self.draw_builder(frame, area),
            Screen::Install(_) => self.draw_install(frame, area),
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
    /// marker beside those of the selected row. It borrows the screen, so
    /// `config` may be one of its own; [`App::keep_pane`] stores what the
    /// clicks after it need.
    fn draw_pane(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        config: &Config,
        selected_row: Option<usize>,
        selected_id: Option<&str>,
        max_height: u16,
    ) -> Pane {
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
                let line = match (&piece.elem, selected_id) {
                    (Elem::Module(id), Some(selected)) if id == selected => {
                        super::paint::line(&painter, &piece.segs, Some(selection))
                    }
                    // A cut or scrolled run of modules: the selected one's
                    // own cells only (SPEC § 14).
                    (Elem::Group(map), Some(selected)) => {
                        let cells: Vec<std::ops::Range<usize>> = map
                            .iter()
                            .filter(|(o, _)| o == selected)
                            .map(|(_, r)| r.clone())
                            .collect();
                        super::paint::marked(&painter, &piece.segs, &cells, selection)
                    }
                    _ => super::paint::line(&painter, &piece.segs, None),
                };
                spans.extend(line.spans);
            }
            lines.push(Line::from(spans));
        }
        let height = cells(lines.len()).min(max_height).max(header.saturating_add(1));
        let rect = Rect { height, ..area };
        frame.render_widget(Paragraph::new(lines), rect);
        let lines = Rect {
            y: rect.y.saturating_add(header),
            height: rect.height.saturating_sub(header),
            ..rect
        };
        Pane { rect, lines, rendered }
    }

    /// Keep what a drawn pane's clicks are measured against; the rows it
    /// took.
    fn keep_pane(&mut self, pane: Pane) -> Rect {
        self.pane_area = pane.lines;
        self.rendered = pane.rendered;
        pane.rect
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
}

/// The first problem of `after` that `before` did not have. Problems are
/// compared by message and by path with every index taken out, count for
/// count: an edit that renumbers rows or modules moves an old problem to
/// another path without making it new, while a second copy of one (a
/// broken row cloned) is new.
fn new_problem<'a>(before: &[ConfigError], after: &'a [ConfigError]) -> Option<&'a ConfigError> {
    let key = |p: &ConfigError| (without_indices(&p.path), p.message.clone());
    let mut old: Vec<(String, String)> = before.iter().map(key).collect();
    after.iter().find(|p| {
        let k = key(p);
        old.iter().position(|o| *o == k).map(|i| old.swap_remove(i)).is_none()
    })
}

/// A problem's path with its `[<n>]` indices taken out
/// (`row[1].modules[0]` → `row.modules`).
fn without_indices(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some((head, tail)) = rest.split_once('[') {
        out.push_str(head);
        match tail.split_once(']') {
            Some((index, after))
                if !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()) =>
            {
                rest = after;
            }
            _ => {
                out.push('[');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The key a step of the wheel stands for in a list.
const fn wheel_key(delta: i8) -> Key {
    if delta < 0 { Key::Up } else { Key::Down }
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
    fn hint_labels_map_to_keys() {
        assert_eq!(hint_key("u"), Some(Key::Char('u')));
        assert_eq!(hint_key("?"), Some(Key::Char('?')));
        assert_eq!(hint_key("enter"), Some(Key::Enter));
        assert_eq!(hint_key("esc"), Some(Key::Esc));
        assert_eq!(hint_key("ctrl-u"), Some(Key::Ctrl('u')));
        for several in ["1 2 3", "x del", "↑↓", "↑↓←→", ""] {
            assert_eq!(hint_key(several), None, "{several:?}");
        }
        assert_eq!((wheel_key(-1), wheel_key(1)), (Key::Up, Key::Down));
    }

    /// app-23: a module selected inside a scrolled group is shown in
    /// inverse video over its own cells, not over the whole group.
    #[test]
    fn a_selected_module_in_a_scrolled_group_is_marked_on_its_own_cells() {
        let text = "icons = \"unicode\"\noverflow = \"ticker\"\n[[row]]\nmodules = [\"path\", \"model\", \"context\", \"limit5h\", \"limit7d\", \"session\", \"api\", \"cache\"]\nright = [\"clock\"]\n";
        let mut app = crate::setup::for_test(text, None, Path::new("/home/dev"));
        app.open_builder();
        let draw = |app: &mut App| {
            let Ok(mut terminal) =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 20));
            app.input(Input::Resize(60, 20));
            let Ok(_) = terminal.draw(|f| app.draw(f));
            terminal.backend().buffer().clone()
        };
        let _ = draw(&mut app);
        let line = app.rendered.lines.first().unwrap().line.clone();
        let (id, group) = line
            .pieces
            .iter()
            .find_map(|p| match &p.elem {
                Elem::Group(map) if !map.is_empty() => Some((map[0].0.clone(), map.clone())),
                _ => None,
            })
            .expect("a scrolled group");
        assert!(group.iter().any(|(o, _)| *o != id), "more than one module in the group");
        assert!(app.builder.select_module(&id, None));
        let buffer = draw(&mut app);
        let owned: Vec<std::ops::Range<usize>> = app.rendered.lines[0]
            .modules
            .iter()
            .filter(|(o, _)| *o == id)
            .map(|(_, r)| r.clone())
            .collect();
        let y = app.pane_area.y;
        let mut marked = 0;
        for x in 2..60_u16 {
            let cell = buffer.cell((x, y)).unwrap();
            let reversed = cell.modifier.contains(Modifier::REVERSED);
            let mine = owned.iter().any(|r| r.contains(&usize::from(x - 2)));
            assert_eq!(reversed, mine, "cell {x} of {id}: {owned:?}");
            marked += usize::from(reversed);
        }
        assert!(marked > 0);
    }

    /// app-28: a box picked for a line goes on that line or nowhere, as
    /// `B`'s typed name does.
    #[test]
    fn a_box_pick_for_another_line_is_refused() {
        let text = "[[row]]\nmodules = [\"path\"]\n[[row]]\nmodules = [\"clock\"]\n";
        let mut app = crate::setup::for_test(text, None, Path::new("/home/dev"));
        app.open_builder();
        let other = super::super::draft::RowAt::row(1);
        app.apply(Action::Picked(Target::BoxFor(other), "true".into()));
        assert_eq!(app.status(), Some("the selection moved; press b again"));
        assert!(app.draft().rows().iter().all(|r| r.get("box").is_none()));
        let here = super::super::draft::RowAt::row(0);
        app.apply(Action::Picked(Target::BoxFor(here), "true".into()));
        assert_eq!(app.draft().rows()[0].get("box"), Some(&Value::Boolean(true)));
    }

    /// app-01: a problem renumbered by an edit is the same problem; one
    /// more copy of it is new.
    #[test]
    fn a_renumbered_problem_is_not_new_and_a_copied_one_is() {
        assert_eq!(without_indices("row[1].col[0].modules[12]"), "row.col.modules");
        assert_eq!(without_indices("modules.text.a[b].x[]"), "modules.text.a[b].x[]");
        assert_eq!(without_indices("frame.fill"), "frame.fill");
        let p = |path: &str, message: &str| ConfigError {
            path: path.into(),
            message: message.into(),
            line: None,
        };
        let before = [p("row[0].modules[1]", "unknown module"), p("row[1].separator", "a string")];
        let moved = [p("row[2].separator", "a string"), p("row[0].modules[0]", "unknown module")];
        assert_eq!(new_problem(&before, &moved), None);
        let copied = [p("row[0].separator", "a string"), p("row[1].separator", "a string")];
        assert_eq!(new_problem(&before[1..], &copied), Some(&copied[1]));
        let other = [p("row[1].separator", "another message")];
        assert_eq!(new_problem(&before[1..], &other), Some(&other[0]));
    }
}
