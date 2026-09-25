//! The editors of `setup` (SPEC § 14): forms of fields.
//!
//! Every form is generated from the same tables the parser reads, so an
//! option added to a schema appears in `setup` the next build. Nothing here
//! is hand-coded per option.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use toml::Value;

use super::app::{Action, Key};
use super::draft::{Draft, RowAt, TITLE_KEYS};
use super::pick::{Choice, Choose, InputBox, Layer, Outcome, Target};
use super::ui::{Chrome, cells, centered, clip, hints, window};
use crate::config::schema::{COMMON_OPTS, Kind, ModuleSchema, Preset};
use crate::config::{self, Config};
use crate::frame::FrameStyle;
use crate::icons::IconSet;
use crate::theme::{PALETTES, Role};

/// Where a field's value lives in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// A key of the table at this path (`[]` is the top level).
    Table(Vec<String>),
    /// A key of a row, column or inner-row table.
    Row(RowAt),
}

/// One key of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The table.
    pub base: Base,
    /// The key in it.
    pub key: String,
}

impl Slot {
    /// A key of the table at `path`.
    #[must_use]
    pub fn table(path: &[&str], key: &str) -> Self {
        Self {
            base: Base::Table(path.iter().map(|s| (*s).to_owned()).collect()),
            key: key.to_owned(),
        }
    }

    /// A top-level key.
    #[must_use]
    pub fn top(key: &str) -> Self {
        Self::table(&[], key)
    }

    /// A key of the row table at `at`.
    #[must_use]
    pub fn row(at: RowAt, key: &str) -> Self {
        Self { base: Base::Row(at), key: key.to_owned() }
    }

    /// The dotted path `config check` names the key by.
    #[must_use]
    pub fn path(&self) -> String {
        match &self.base {
            Base::Table(p) if p.is_empty() => self.key.clone(),
            Base::Table(p) => format!("{}.{}", p.join("."), self.key),
            Base::Row(at) => format!("{}.{}", at.path(), self.key),
        }
    }

    /// The value the draft holds for this key, if set.
    #[must_use]
    pub fn get<'a>(&self, draft: &'a Draft) -> Option<&'a Value> {
        match &self.base {
            Base::Table(p) => {
                let mut path: Vec<&str> = p.iter().map(String::as_str).collect();
                path.push(&self.key);
                draft.get(&path)
            }
            Base::Row(at) => draft.row(*at)?.get(&self.key),
        }
    }

    /// Set the key in the draft.
    pub fn set(&self, draft: &mut Draft, value: Value) {
        match &self.base {
            Base::Table(p) => {
                let mut path: Vec<&str> = p.iter().map(String::as_str).collect();
                path.push(&self.key);
                draft.set(&path, value);
            }
            Base::Row(at) => {
                if let Some(t) = draft.row_mut(*at) {
                    t.insert(self.key.clone(), value);
                }
            }
        }
    }

    /// Remove the key from the draft.
    pub fn unset(&self, draft: &mut Draft) {
        match &self.base {
            Base::Table(p) => {
                let mut path: Vec<&str> = p.iter().map(String::as_str).collect();
                path.push(&self.key);
                draft.remove(&path);
            }
            Base::Row(at) => {
                if let Some(t) = draft.row_mut(*at) {
                    t.remove(&self.key);
                }
            }
        }
    }
}

/// How a field is edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotKind {
    /// A checkbox.
    Bool,
    /// Unset, `true` or `false` (the `animate` switch).
    Tri,
    /// One of a fixed list.
    Enum(Vec<String>),
    /// An integer between `min` and `max`.
    Int {
        /// The smallest value the parser takes.
        min: i64,
        /// The largest, when the option is bounded.
        max: Option<i64>,
    },
    /// A number.
    Float,
    /// Free text, with suggestions.
    Str,
    /// A theme role or a literal colour.
    Color,
    /// An icon glyph.
    Icon,
    /// A list of strings, typed comma-separated.
    StrList,
    /// A list of animation frames, typed as the TOML array it is written
    /// as: a frame's spaces are part of it, and it may hold a comma.
    Frames,
    /// A list of numbers, typed comma-separated.
    NumList,
    /// A list of colours, typed comma-separated.
    ColorList,
    /// A column's `"<n>fr" | "auto" | cells`.
    Width,
    /// A row's or column's `box`: none, `true`, or a name.
    BoxRef,
    /// A module's `preset`: unset (follow the top level) or one of three.
    Preset,
    /// Any TOML value, typed as the file would write it: the row of a key
    /// the form has no row of its own for.
    Literal,
}

impl SlotKind {
    /// The kind for a schema option's [`Kind`]: every kind has an editor.
    #[must_use]
    pub fn of(kind: Kind, max: Option<usize>) -> Self {
        match kind {
            Kind::Bool => Self::Bool,
            Kind::Int => Self::Int { min: 0, max: max.and_then(|m| i64::try_from(m).ok()) },
            Kind::Float => Self::Float,
            Kind::Str => Self::Str,
            Kind::Enum(vals) => Self::Enum(vals.iter().map(|v| (*v).to_owned()).collect()),
            Kind::StrList => Self::StrList,
            Kind::NumList => Self::NumList,
            Kind::ColorList => Self::ColorList,
        }
    }

    /// The TOML value for text typed for this kind, or why it is not one.
    ///
    /// # Errors
    /// Text that is not a value of the kind, in a line for the status bar.
    pub fn parse(&self, text: &str) -> Result<Option<Value>, String> {
        // A string keeps its spaces: ` │ ` and `  ` are separators, a pad
        // is a space, a blank glyph is one cell. Every other kind is read
        // trimmed.
        let raw = text;
        let text = text.trim();
        Ok(Some(match self {
            Self::Bool => Value::Boolean(matches!(text, "true" | "yes" | "on" | "1")),
            Self::Tri => match text {
                "" => return Ok(None),
                "true" | "yes" | "on" => Value::Boolean(true),
                _ => Value::Boolean(false),
            },
            Self::Str | Self::Icon => Value::String(raw.to_owned()),
            Self::Enum(_) | Self::Color => Value::String(text.to_owned()),
            Self::Preset => {
                if text.is_empty() {
                    return Ok(None);
                }
                Value::String(text.to_owned())
            }
            Self::Int { min, max } => {
                let n: i64 = text.parse().map_err(|_| format!("{text:?} is not an integer"))?;
                if n < *min {
                    return Err(format!("{n} is below the minimum of {min}"));
                }
                if let Some(m) = max
                    && n > *m
                {
                    return Err(format!("{n} is above the maximum of {m}"));
                }
                Value::Integer(n)
            }
            Self::Float => Value::Float(number(text)?),
            Self::Literal => {
                if text.is_empty() {
                    return Ok(None);
                }
                literal(text)?
            }
            Self::Frames => {
                if text.is_empty() {
                    return Ok(None);
                }
                let v = literal(text)?;
                if !v.as_array().is_some_and(|a| a.iter().all(Value::is_str)) {
                    return Err(format!(
                        "{text} is not a list of strings, like [\" │ \", \" ┃ \"]"
                    ));
                }
                v
            }
            Self::StrList | Self::ColorList => Value::Array(
                text.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| Value::String(s.to_owned()))
                    .collect(),
            ),
            Self::NumList => {
                let items: Result<Vec<Value>, String> = text
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| number(s).map(Value::Float))
                    .collect();
                Value::Array(items?)
            }
            Self::Width => {
                text.parse::<i64>().map_or_else(|_| Value::String(text.to_owned()), Value::Integer)
            }
            Self::BoxRef => match text {
                "" | "none" | "false" => return Ok(None),
                "true" => Value::Boolean(true),
                name => Value::String(name.to_owned()),
            },
        }))
    }
}

/// A finite number typed (`f64` itself also parses `nan` and `inf`).
fn number(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .ok_or_else(|| format!("{text:?} is not a number"))
}

/// A TOML value typed as the file would write it (`"12h"`, `[" │ "]`,
/// `true`), or why it is not one.
fn literal(text: &str) -> Result<Value, String> {
    toml::from_str::<toml::Table>(&format!("v = {text}"))
        .ok()
        .and_then(|mut t| t.remove("v"))
        .ok_or_else(|| format!("{text} is not a TOML value (a string is quoted: \"…\")"))
}

/// A list of strings as the TOML array literal the file would hold.
fn array_literal(items: &[Value]) -> String {
    let items: Vec<String> = items
        .iter()
        .map(|v| v.as_str().map_or_else(|| v.to_string(), crate::config::schema::toml_string))
        .collect();
    format!("[{}]", items.join(", "))
}

/// A TOML value as a form shows it: strings bare, lists in brackets.
#[must_use]
pub fn show(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => crate::config::schema::Value::Float(*f).to_toml(),
        Value::Boolean(b) => b.to_string(),
        Value::Array(items) => {
            format!("[{}]", items.iter().map(show).collect::<Vec<_>>().join(", "))
        }
        Value::Table(_) => "{…}".to_owned(),
        Value::Datetime(d) => d.to_string(),
    }
}

/// A value in a form's current column: [`show`], except that a string of
/// spaces alone (`ticker_gap`) is quoted, since blank and unset look the
/// same otherwise.
fn current_text(value: &Value) -> String {
    match value {
        Value::String(s) if !s.is_empty() && s.trim().is_empty() => format!("{s:?}"),
        other => show(other),
    }
}

/// One line of a form.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The key.
    pub key: String,
    /// Its doc string.
    pub doc: String,
    /// How it is edited.
    pub kind: SlotKind,
    /// Where it lives.
    pub slot: Slot,
    /// The file sets it (a dot on the line).
    pub set: bool,
    /// The value in effect, as shown.
    pub current: String,
    /// The value in effect, for stepping.
    pub value: Option<Value>,
    /// What applies when the key is unset.
    pub default: String,
    /// The entries a picker offers before `custom…`.
    pub choices: Vec<Choice>,
}

impl Field {
    fn new(key: &str, doc: &str, kind: SlotKind, slot: Slot) -> Self {
        Self {
            key: key.to_owned(),
            doc: doc.to_owned(),
            kind,
            slot,
            set: false,
            current: String::new(),
            value: None,
            default: String::new(),
            choices: Vec::new(),
        }
    }

    /// With the value in effect and the default it falls back to.
    fn valued(mut self, draft: &Draft, effective: Option<Value>, default: &str) -> Self {
        self.set = self.slot.get(draft).is_some();
        self.value = effective;
        self.current = self.value.as_ref().map_or_else(|| "(unset)".to_owned(), current_text);
        default.clone_into(&mut self.default);
        self
    }

    fn with_choices(mut self, choices: Vec<Choice>) -> Self {
        self.choices = choices;
        self
    }
}

/// What a form edits, so the app can rebuild it after each change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    /// `[modules.<id>]` (or `[modules.text.<name>]`).
    Module(String),
    /// The top-level keys.
    Top,
    /// `[frame]`.
    Frame,
    /// `[colors]`.
    Colors,
    /// A `[[row]]` or `[[row.col.row]]`.
    Row(RowAt),
    /// A `[[row.col]]`.
    Col(RowAt),
    /// `[box.<name>]`.
    Box(String),
}

/// A form: fields, a cursor, and what it edits.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    /// The heading.
    pub title: String,
    /// The fields, in order.
    pub fields: Vec<Field>,
    /// The selected field.
    pub cursor: usize,
    scroll: usize,
    /// What it edits.
    pub kind: FormKind,
}

impl Form {
    /// The form for `kind` over the current draft and its resolved config.
    #[must_use]
    pub fn build(kind: FormKind, draft: &Draft, config: &Config, hints: &Suggestions) -> Self {
        let (title, fields) = match &kind {
            FormKind::Module(id) => {
                (format!("[modules.{id}]"), module_fields(id, draft, config, hints))
            }
            FormKind::Top => ("top level".to_owned(), top_fields(draft, config, hints)),
            FormKind::Frame => ("[frame]".to_owned(), frame_fields(draft, config, hints)),
            FormKind::Colors => ("[colors]".to_owned(), color_fields(draft, config)),
            FormKind::Row(at) => (at.path(), row_fields(*at, draft, config, hints)),
            FormKind::Col(at) => (at.path(), col_fields(*at, draft, config)),
            FormKind::Box(name) => {
                (format!("[box.{name}]"), box_fields(name, draft, config, hints))
            }
        };
        Self { title, fields, cursor: 0, scroll: 0, kind }
    }

    /// Put the cursor on `key`, when the form has it.
    pub fn focus(&mut self, key: &str) {
        if let Some(i) = self.fields.iter().position(|f| f.key == key) {
            self.cursor = i;
        }
    }

    /// Handle a key.
    pub fn handle(&mut self, key: Key) -> Outcome {
        let n = self.fields.len();
        let Some(field) = self.fields.get(self.cursor).cloned() else {
            return if key == Key::Esc { Outcome::close() } else { Outcome::default() };
        };
        match key {
            Key::Esc => Outcome::close(),
            Key::Up | Key::BackTab => {
                self.cursor = self.cursor.checked_sub(1).unwrap_or_else(|| n.saturating_sub(1));
                Outcome::default()
            }
            Key::Down | Key::Tab => {
                self.cursor = self.cursor.saturating_add(1).checked_rem(n.max(1)).unwrap_or(0);
                Outcome::default()
            }
            Key::PageUp => {
                self.cursor = self.cursor.saturating_sub(10);
                Outcome::default()
            }
            Key::PageDown => {
                self.cursor = self.cursor.saturating_add(10).min(n.saturating_sub(1));
                Outcome::default()
            }
            Key::Home => {
                self.cursor = 0;
                Outcome::default()
            }
            Key::End => {
                self.cursor = n.saturating_sub(1);
                Outcome::default()
            }
            Key::Char('d') | Key::Delete | Key::Backspace => {
                Outcome { close: false, push: None, actions: vec![Action::Unset(field.slot)] }
            }
            Key::Left | Key::Right | Key::Char('-' | '+') => {
                let up = matches!(key, Key::Right | Key::Char('+'));
                Self::step(&field, up)
            }
            Key::Enter | Key::Char(' ') => Self::activate(&field),
            _ => Outcome::default(),
        }
    }

    /// `←`/`→`: the next value along for a stepped kind, else nothing.
    fn step(field: &Field, up: bool) -> Outcome {
        let set = |v: Value| Outcome {
            close: false,
            push: None,
            actions: vec![Action::Set(field.slot.clone(), v)],
        };
        match &field.kind {
            SlotKind::Bool => {
                set(Value::Boolean(!matches!(field.value, Some(Value::Boolean(true)))))
            }
            SlotKind::Tri => {
                let next = match (field.set, &field.value) {
                    (false, _) => Some(true),
                    (true, Some(Value::Boolean(true))) => Some(false),
                    _ => None,
                };
                next.map_or_else(
                    || Outcome {
                        close: false,
                        push: None,
                        actions: vec![Action::Unset(field.slot.clone())],
                    },
                    |b| set(Value::Boolean(b)),
                )
            }
            SlotKind::Int { min, max } => {
                let n = match field.value {
                    Some(Value::Integer(n)) => n,
                    _ => *min,
                };
                let next = if up { n.saturating_add(1) } else { n.saturating_sub(1) };
                set(Value::Integer(next.max(*min).min(max.unwrap_or(i64::MAX))))
            }
            SlotKind::Enum(vals) => {
                let at = vals
                    .iter()
                    .position(|v| Some(v.as_str()) == field.value.as_ref().and_then(Value::as_str));
                let len = vals.len().max(1);
                let next = match at {
                    Some(i) if up => i.saturating_add(1).checked_rem(len).unwrap_or(0),
                    Some(i) => i.checked_sub(1).unwrap_or_else(|| len.saturating_sub(1)),
                    None => 0,
                };
                vals.get(next).map_or_else(Outcome::default, |v| set(Value::String(v.clone())))
            }
            SlotKind::Preset => {
                let names: Vec<&str> = Preset::ALL.iter().map(|p| p.name()).collect();
                let at = field
                    .value
                    .as_ref()
                    .filter(|_| field.set)
                    .and_then(Value::as_str)
                    .and_then(|n| names.iter().position(|x| *x == n));
                let last = names.len().saturating_sub(1);
                let next = match (at, up) {
                    (None, true) => Some(0),
                    (None, false) => Some(last),
                    (Some(i), true) if i >= last => None,
                    (Some(0), false) => None,
                    (Some(i), true) => Some(i.saturating_add(1)),
                    (Some(i), false) => Some(i.saturating_sub(1)),
                };
                next.and_then(|i| names.get(i)).map_or_else(
                    || Outcome {
                        close: false,
                        push: None,
                        actions: vec![Action::Unset(field.slot.clone())],
                    },
                    |v| set(Value::String((*v).to_owned())),
                )
            }
            _ => Outcome::default(),
        }
    }

    /// `Enter`: toggle, cycle, or open the picker or input the kind wants.
    /// A picker opens on the value in effect and its `custom…` line starts
    /// from it, so a label is edited rather than retyped.
    fn activate(field: &Field) -> Outcome {
        let target = Target::Slot(field.slot.clone(), field.kind.clone());
        let typed = field.value.as_ref().map_or_else(String::new, |v| match (&field.kind, v) {
            (SlotKind::Literal, v) => v.to_string(),
            (SlotKind::Frames, Value::Array(items)) => array_literal(items),
            (_, Value::Array(items)) => items.iter().map(show).collect::<Vec<_>>().join(", "),
            (_, other) => show(other),
        });
        let open = |title: String, mut items: Vec<Choice>, custom: Option<&str>| {
            if let Some(what) = custom {
                items.push(Choice::custom(what));
            }
            let mut choose = Choose::new(&title, items, target.clone());
            choose.custom_title = format!("{}: custom value", field.key);
            choose.custom_start.clone_from(&typed);
            choose.select(&typed);
            Outcome { close: false, push: Some(Layer::Choose(choose)), actions: Vec::new() }
        };
        let input = |title: String, text: String| Outcome {
            close: false,
            push: Some(Layer::Input(InputBox::new(&title, &text, target.clone()))),
            actions: Vec::new(),
        };
        match &field.kind {
            SlotKind::Bool | SlotKind::Tri => Self::step(field, true),
            SlotKind::Enum(vals) => {
                open(field.key.clone(), vals.iter().map(|v| Choice::plain(v)).collect(), None)
            }
            SlotKind::Preset => {
                let mut items = vec![Choice::noted("", "follow the top-level preset")];
                items.extend(Preset::ALL.iter().map(|p| Choice::plain(p.name())));
                let mut out = open(field.key.clone(), items, None);
                // Unset follows the top level: the first entry, whatever the
                // module resolves to.
                if !field.set
                    && let Some(Layer::Choose(c)) = &mut out.push
                {
                    c.cursor = 0;
                }
                out
            }
            SlotKind::Int { .. }
            | SlotKind::Float
            | SlotKind::StrList
            | SlotKind::Frames
            | SlotKind::Literal
            | SlotKind::NumList
            | SlotKind::ColorList => input(field.key.clone(), typed),
            SlotKind::Str | SlotKind::Color | SlotKind::Icon => {
                open(field.key.clone(), field.choices.clone(), Some("custom"))
            }
            SlotKind::Width => {
                let items =
                    ["1fr", "2fr", "3fr", "auto"].iter().map(|v| Choice::plain(v)).collect();
                open("width".to_owned(), items, Some("a cell count"))
            }
            SlotKind::BoxRef => {
                open("box".to_owned(), field.choices.clone(), Some("a new box name"))
            }
        }
    }

    /// Draw the form as a dialog over `area`.
    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let width = usize::from(area.width).min(78);
        let height = usize::from(area.height).min(self.fields.len().saturating_add(8).max(12));
        let rect = centered(area, cells(width), cells(height));
        frame.render_widget(Clear, rect);
        let block =
            Block::bordered().title(Span::styled(format!(" {} ", self.title), Chrome::title()));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let doc_lines = 3_usize;
        let list_height =
            usize::from(inner.height).saturating_sub(doc_lines).saturating_sub(1).max(1);
        self.scroll = window(self.cursor, self.fields.len(), list_height, self.scroll);
        let key_w = self.fields.iter().map(|f| f.key.len()).max().unwrap_or(8).min(24);
        let inner_w = usize::from(inner.width);
        let value_w =
            inner_w.saturating_sub(key_w).saturating_sub(6).checked_div(2).unwrap_or(10).max(6);
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, f) in self.fields.iter().enumerate().skip(self.scroll).take(list_height) {
            // A plain mark: the geometric dots draw two cells in some
            // terminals (CLAUDE.md § Conventions).
            let mark = if f.set { Span::styled("* ", Chrome::set()) } else { Span::raw("  ") };
            let key = Span::raw(format!("{:<key_w$} ", clip(&f.key, key_w)));
            let value = Span::styled(
                format!("{:<value_w$} ", clip(&f.current, value_w)),
                if f.set { Chrome::set() } else { ratatui::style::Style::new() },
            );
            let default = Span::styled(
                clip(
                    &format!("({})", f.default),
                    inner_w.saturating_sub(key_w).saturating_sub(value_w).saturating_sub(4),
                ),
                Chrome::muted(),
            );
            let mut line = Line::from(vec![mark, key, value, default]);
            if i == self.cursor {
                line = line.style(Chrome::selected());
            }
            lines.push(line);
        }
        while lines.len() < list_height {
            lines.push(Line::from(""));
        }
        let doc = self.fields.get(self.cursor).map_or(String::new(), |f| f.doc.clone());
        frame.render_widget(Paragraph::new(lines), Rect { height: cells(list_height), ..inner });
        let doc_rect = Rect {
            y: inner.y.saturating_add(cells(list_height)),
            height: cells(doc_lines),
            ..inner
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(doc, Chrome::muted())))
                .wrap(Wrap { trim: true }),
            doc_rect,
        );
        let hint_rect = Rect { y: inner.bottom().saturating_sub(1), height: 1, ..inner };
        frame.render_widget(
            Paragraph::new(hints(&[
                ("↑↓", "field"),
                ("enter", "edit"),
                ("←→", "step"),
                ("d", "unset"),
                ("esc", "back"),
            ])),
            hint_rect,
        );
    }
}

/// The values the presets, the frame tables and the gallery already use for
/// a key (SPEC § 14): what a string picker offers before `custom…`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    by_key: Vec<(String, Vec<String>)>,
}

impl Suggestions {
    /// Gather from the frame styles and every gallery preset, once.
    #[must_use]
    pub fn gather() -> Self {
        let mut by_key: Vec<(String, Vec<String>)> = Vec::new();
        let mut add = |key: &str, value: &str| {
            let at = by_key.iter().position(|(k, _)| k == key).unwrap_or_else(|| {
                by_key.push((key.to_owned(), Vec::new()));
                by_key.len().saturating_sub(1)
            });
            if let Some((_, values)) = by_key.get_mut(at)
                && !values.iter().any(|v| v == value)
            {
                values.push(value.to_owned());
            }
        };
        for style in FrameStyle::ALL {
            let c = crate::frame::FrameChars::for_style(style);
            for (key, value) in [
                ("separator", &c.separator),
                ("pad", &c.pad),
                ("fill_char", &c.fill),
                ("first", &c.first),
                ("middle", &c.middle),
                ("last", &c.last),
                ("single", &c.single),
                ("right_first", &c.right_first),
                ("right_middle", &c.right_middle),
                ("right_last", &c.right_last),
                ("right_single", &c.right_single),
                ("top_left", &c.top_left),
                ("top_right", &c.top_right),
                ("bottom_left", &c.bottom_left),
                ("bottom_right", &c.bottom_right),
                ("side", &c.side),
            ] {
                if !value.is_empty() {
                    add(key, value);
                }
            }
        }
        for preset in crate::gallery::PRESETS.iter() {
            if let Ok(table) = toml::from_str::<toml::Table>(&crate::gallery::body(preset.source)) {
                scan(&table, &mut add);
            }
        }
        Self { by_key }
    }

    /// The distinct values seen for `key`, first seen first.
    #[must_use]
    pub fn for_key(&self, key: &str) -> Vec<String> {
        self.by_key.iter().find(|(k, _)| k == key).map_or_default(|(_, v)| v.clone())
    }

    fn choices(&self, key: &str) -> Vec<Choice> {
        self.for_key(key)
            .iter()
            .map(|v| Choice::noted(v, &format!("{} cells", crate::ansi::display_width(v))))
            .collect()
    }
}

/// Every string value in a table, by key, recursively.
fn scan(table: &toml::Table, add: &mut impl FnMut(&str, &str)) {
    for (key, value) in table {
        match value {
            Value::String(s) => add(key, s),
            Value::Table(t) => scan(t, add),
            Value::Array(items) => {
                for item in items {
                    if let Value::Table(t) = item {
                        scan(t, add);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The named terminal colours every colour picker ends with.
const NAMED_COLORS: [&str; 9] =
    ["default", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "gray"];

/// Two cells in the colour, or blank for the terminal's default.
fn swatch(c: crate::ansi::Color) -> Span<'static> {
    super::paint::color(c).map_or_else(
        || Span::raw("  "),
        |fg| Span::styled("██", ratatui::style::Style::new().fg(fg)),
    )
}

/// The colour picker's entries for a key that takes a role (a module's
/// `colors.*`, a title, a box): every role with a swatch, then the named
/// terminal colours.
fn color_choices(config: &Config) -> Vec<Choice> {
    let mut items: Vec<Choice> = Role::ALL
        .iter()
        .map(|role| {
            let c = config.theme.role(*role);
            Choice {
                label: Line::from(vec![
                    Span::raw(format!("{:<8} ", role.name())),
                    swatch(c),
                    Span::styled(format!("  {}", c.to_spec()), Chrome::muted()),
                ]),
                value: role.name().to_owned(),
                custom: false,
            }
        })
        .collect();
    items.extend(NAMED_COLORS.iter().map(|name| Choice::plain(name)));
    items
}

/// The colour picker's entries for a `[colors]` role, which takes a literal
/// only (a role defined by another role would have no ground): the theme's
/// own literals, each noted with the role it colours, then the named
/// terminal colours.
fn literal_color_choices(config: &Config) -> Vec<Choice> {
    let mut items: Vec<Choice> = Vec::new();
    for role in Role::ALL {
        let c = config.theme.role(role);
        let spec = c.to_spec();
        if items.iter().any(|i| i.value == spec) {
            continue;
        }
        items.push(Choice {
            label: Line::from(vec![
                Span::raw(format!("{spec:<8} ")),
                swatch(c),
                Span::styled(format!("  the theme's {}", role.name()), Chrome::muted()),
            ]),
            value: spec,
            custom: false,
        });
    }
    items.extend(NAMED_COLORS.iter().map(|name| Choice::plain(name)));
    items
}

/// What a module's `label` picker offers first: the module's own name, bare
/// and capitalised, before the labels the gallery uses.
fn label_choices(id: &str, hints: &Suggestions) -> Vec<Choice> {
    let name = id.strip_prefix(crate::modules::text::PREFIX).unwrap_or(id);
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    let capitalised: String =
        chars.next().map_or_default(|c| c.to_uppercase().chain(chars).collect());
    let mut items = vec![
        Choice::noted(name, "the module's name"),
        Choice::noted(&capitalised, "the module's name"),
    ];
    items.extend(
        hints.choices("label").into_iter().filter(|c| c.value != name && c.value != capitalised),
    );
    items
}

/// The glyph picker's entries for one icon: the four sets, the key's
/// suggested alternatives, each with the cell count `doctor` shows.
fn icon_choices(schema: &ModuleSchema, key: &str) -> Vec<Choice> {
    let Some(spec) = schema.icon(key) else { return Vec::new() };
    let mut items: Vec<Choice> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut push = |glyph: &str, note: &str| {
        if seen.iter().any(|g| g == glyph) {
            return;
        }
        seen.push(glyph.to_owned());
        let width = crate::ansi::display_width(glyph);
        let shown = if glyph.is_empty() { "(blank)".to_owned() } else { glyph.to_owned() };
        items.push(Choice {
            label: Line::from(vec![
                Span::raw(format!("{shown:<2}")),
                Span::styled(format!("|{width}  "), Chrome::muted()),
                Span::styled(note.to_owned(), Chrome::muted()),
            ]),
            value: glyph.to_owned(),
            custom: false,
        });
    };
    for set in IconSet::ALL {
        push(spec.glyph.get(set), set.name());
    }
    for glyph in crate::icons::suggestions(schema.id, key) {
        push(glyph, "also try");
    }
    items
}

/// The entries of a `box` picker: none, this row alone, every defined box.
fn box_choices(config: &Config) -> Vec<Choice> {
    let mut items =
        vec![Choice::noted("none", "no box"), Choice::noted("true", "a box of its own")];
    items.extend(config.boxes.keys().map(|name| Choice::noted(name, "[box] table")));
    items
}

fn module_fields(id: &str, draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let text = id.strip_prefix(crate::modules::text::PREFIX);
    let (schema, cfg, base): (&ModuleSchema, Option<&crate::config::schema::ModuleCfg>, Vec<&str>) =
        match text {
            // A text module is its table: with none (an undo took it back
            // under an open editor) there is nothing to edit.
            Some(name) => match config.texts.get(name) {
                Some(cfg) => {
                    (&crate::modules::text::SCHEMA, Some(cfg), vec!["modules", "text", name])
                }
                None => return Vec::new(),
            },
            None => match crate::modules::SCHEMAS.iter().find(|s| s.id == id) {
                Some(s) => (s, config.modules.get(id), vec!["modules", id]),
                None => return Vec::new(),
            },
        };
    let slot = |key: &str| Slot::table(&base, key);
    let mut fields: Vec<Field> = Vec::new();
    fields.push(
        Field::new("enabled", "Render this module at all.", SlotKind::Bool, slot("enabled"))
            .valued(draft, Some(Value::Boolean(cfg.is_none_or(|c| c.enabled))), "true"),
    );
    if text.is_none() {
        let preset = cfg.map_or(Preset::Default, |c| c.preset);
        let mut f = Field::new(
            "preset",
            "minimal | default | full; unset follows the top-level preset.",
            SlotKind::Preset,
            slot("preset"),
        )
        .valued(
            draft,
            Some(Value::String(preset.name().to_owned())),
            config.preset.module_preset().name(),
        );
        f.set = slot("preset").get(draft).is_some();
        fields.push(f);
        let min = i64::from(schema.refresh > 0);
        fields.push(
            Field::new(
                "refresh",
                "Seconds between background refreshes (0 = every tick, payload-only modules).",
                SlotKind::Int { min, max: None },
                slot("refresh"),
            )
            .valued(
                draft,
                Some(Value::Integer(
                    i64::try_from(cfg.map_or(schema.refresh, |c| c.refresh)).unwrap_or(0),
                )),
                &schema.refresh.to_string(),
            ),
        );
    }
    // The states come from the schema's measure (SPEC § 3), as the parser's
    // check and the reference row do.
    let states = schema.hide_states().join(", ");
    fields.push(
        Field::new(
            "hide",
            &format!(
                "States that hide the module, comma-separated: {states}. `empty` is what hide_when_empty hides; the two combine."
            ),
            SlotKind::StrList,
            slot("hide"),
        )
        .valued(
            draft,
            cfg.and_then(|c| c.common("hide"))
                .map(to_toml)
                .or_else(|| Some(Value::Array(Vec::new()))),
            "[]",
        ),
    );
    for opt in COMMON_OPTS.iter().filter(|o| text.is_none() || config::text_takes(o.key)) {
        let value = cfg.and_then(|c| c.common(opt.key)).map(to_toml);
        let kind = SlotKind::of(opt.kind, opt.max);
        let mut f = Field::new(opt.key, opt.doc, kind, slot(opt.key)).valued(
            draft,
            value.or_else(|| Some(to_toml(opt.default.clone()))),
            &opt.default.to_toml(),
        );
        if opt.key == "label" {
            f = f.with_choices(label_choices(id, hints));
        } else if opt.kind == Kind::Str {
            f = f.with_choices(hints.choices(opt.key));
        }
        fields.push(f);
    }
    let module_preset = cfg.map_or(Preset::Default, |c| c.preset);
    for opt in &schema.opts {
        let value = cfg.and_then(|c| c.value(opt.key)).cloned().map(to_toml);
        let kind = SlotKind::of(opt.kind, opt.max);
        let default = opt.for_preset(module_preset).to_toml();
        let mut f =
            Field::new(opt.key, opt.doc, kind, slot(opt.key)).valued(draft, value, &default);
        if opt.kind == Kind::Str {
            f = f.with_choices(hints.choices(opt.key));
        }
        fields.push(f);
    }
    fields.extend(module_glyph_fields(schema, cfg, &base, draft, config));
    module_extra_fields(fields, schema, text.is_some(), &base, draft, config)
}

/// A module's form past its schema's rows: a text module's `color`
/// shorthand and an icon's `<key>_frames` as proper rows, then every other
/// key of its tables as [`extra_fields`].
fn module_extra_fields(
    mut fields: Vec<Field>,
    schema: &ModuleSchema,
    text: bool,
    base: &[&str],
    draft: &Draft,
    config: &Config,
) -> Vec<Field> {
    let slot = |key: &str| Slot::table(base, key);
    let table = draft.get(base).and_then(Value::as_table);
    if text && let Some(color) = table.and_then(|t| t.get("color")) {
        fields.push(
            Field::new(
                "color",
                "Shorthand for colors.text; an explicit colors.text wins.",
                SlotKind::Color,
                slot("color"),
            )
            .valued(draft, Some(color.clone()), "colors.text")
            .with_choices(color_choices(config)),
        );
    }
    let sub_table = |key: &str| table.and_then(|t| t.get(key)).and_then(Value::as_table);
    let icons_base: Vec<&str> = base.iter().copied().chain(std::iter::once("icons")).collect();
    let colors_base: Vec<&str> = base.iter().copied().chain(std::iter::once("colors")).collect();
    let icon_slot = |k: &str| Slot::table(&icons_base, k);
    let color_slot = |k: &str| Slot::table(&colors_base, k);
    let icons = sub_table("icons");
    for (key, value) in icons.into_iter().flatten() {
        let Some(stem) = key.strip_suffix("_frames").filter(|s| schema.icon(s).is_some()) else {
            continue;
        };
        fields.push(
            Field::new(
                &format!("icons.{key}"),
                &format!(
                    "The frames icons.{stem} cycles through, one per tick while animation is on, as a TOML array."
                ),
                SlotKind::Frames,
                icon_slot(key),
            )
            .valued(draft, Some(value.clone()), "[]"),
        );
    }
    let extra = extra_fields(&fields, icons, &icon_slot, "icons.", &[], draft);
    fields.extend(extra);
    let colors = sub_table("colors");
    let extra = extra_fields(&fields, colors, &color_slot, "colors.", &[], draft);
    fields.extend(extra);
    let tables: Vec<&str> = [("icons", icons), ("colors", colors)]
        .iter()
        .filter(|(_, t)| t.is_some())
        .map(|(k, _)| *k)
        .collect();
    let extra = extra_fields(&fields, table, &slot, "", &tables, draft);
    fields.extend(extra);
    fields
}

/// What the row of a key a form has no row of its own for says.
const EXTRA_DOC: &str = "A key this form has no row of its own for; the status bar names the parser's problem with it, if any. Enter types a TOML value (a string quoted), d removes it.";

/// A row for every key of `table` the form's own `fields` miss, as `slot`
/// addresses it and keyed `<prefix><key>`, but the `skip` keys another
/// screen edits (SPEC § 14: a form lists the keys the parser takes plus
/// any the file sets, so `d` can unset one the parser reports).
fn extra_fields(
    fields: &[Field],
    table: Option<&toml::Table>,
    slot: &dyn Fn(&str) -> Slot,
    prefix: &str,
    skip: &[&str],
    draft: &Draft,
) -> Vec<Field> {
    table
        .into_iter()
        .flatten()
        .filter(|(key, _)| !skip.contains(&key.as_str()))
        .map(|(key, value)| (key, value, slot(key)))
        .filter(|(_, _, s)| !fields.iter().any(|f| f.slot == *s))
        .map(|(key, value, s)| {
            Field::new(&format!("{prefix}{key}"), EXTRA_DOC, SlotKind::Literal, s).valued(
                draft,
                Some(value.clone()),
                "none",
            )
        })
        .collect()
}

/// A module's `icons.*` and `colors.*` fields.
fn module_glyph_fields(
    schema: &ModuleSchema,
    cfg: Option<&crate::config::schema::ModuleCfg>,
    base: &[&str],
    draft: &Draft,
    config: &Config,
) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    let icons_base: Vec<&str> = base.iter().copied().chain(std::iter::once("icons")).collect();
    for icon in &schema.icons {
        let value = cfg.map(|c| Value::String(c.icon(icon.key).to_owned()));
        fields.push(
            Field::new(
                &format!("icons.{}", icon.key),
                icon.doc,
                SlotKind::Icon,
                Slot::table(&icons_base, icon.key),
            )
            .valued(draft, value, icon.glyph.get(config.icons))
            .with_choices(icon_choices(schema, icon.key)),
        );
    }
    let colors_base: Vec<&str> = base.iter().copied().chain(std::iter::once("colors")).collect();
    // A text module's `color` is its `colors.text` while that is unset.
    let is_text = schema.id == crate::modules::text::SCHEMA.id;
    let color_path: Vec<&str> = base.iter().copied().chain(std::iter::once("color")).collect();
    let shorthand = draft.get(&color_path).filter(|_| is_text);
    for color in &schema.colors {
        let slot = Slot::table(&colors_base, color.key);
        // As written, a role or a literal, not the resolved colour: picked
        // back, that would pin a role to today's theme.
        let value = slot
            .get(draft)
            .or_else(|| shorthand.filter(|_| color.key == "text"))
            .cloned()
            .unwrap_or_else(|| Value::String(color.default.to_owned()));
        fields.push(
            Field::new(&format!("colors.{}", color.key), color.doc, SlotKind::Color, slot)
                .valued(draft, Some(value), color.default)
                .with_choices(color_choices(config)),
        );
    }
    fields
}

/// A schema value as a TOML value.
fn to_toml(v: crate::config::schema::Value) -> Value {
    use crate::config::schema::Value as V;
    match v {
        V::Bool(b) => Value::Boolean(b),
        V::Int(i) => Value::Integer(i),
        V::Float(f) => Value::Float(f),
        V::Str(s) => Value::String(s),
        V::StrList(items) => Value::Array(items.into_iter().map(Value::String).collect()),
        V::NumList(items) => Value::Array(items.into_iter().map(Value::Float).collect()),
    }
}

/// An enum field's kind from a list of names.
fn names(items: &[&str]) -> SlotKind {
    SlotKind::Enum(items.iter().map(|v| (*v).to_owned()).collect())
}

/// A string value in effect.
fn string(v: &str) -> Value {
    Value::String(v.to_owned())
}

/// The top-level keys: what the line looks like, how it is laid out, then
/// the `[format]` table.
fn top_fields(draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let mut fields = look_fields(draft, config);
    fields.extend(layout_fields(draft, config, hints));
    fields.extend(format_fields(draft, config));
    // The rows are the builder's, and each table has a form of its own.
    let mut skip = vec!["row"];
    skip.extend(
        ["modules", "frame", "colors", "box", "format"]
            .into_iter()
            .filter(|k| draft.get(&[k]).is_some_and(Value::is_table)),
    );
    let extra = extra_fields(&fields, Some(draft.table()), &Slot::top, "", &skip, draft);
    fields.extend(extra);
    let format = draft.get(&["format"]).and_then(Value::as_table);
    let extra =
        extra_fields(&fields, format, &|k| Slot::table(&["format"], k), "format.", &[], draft);
    fields.extend(extra);
    fields
}

/// `[format]`: the number styles (SPEC § 4), each row keyed `format.<key>`.
fn format_fields(draft: &Draft, config: &Config) -> Vec<Field> {
    let s = |key: &str| Slot::table(&["format"], key);
    let f = &config.format;
    vec![
        Field::new(
            "format.tokens",
            "Token counts: compact (128k, 1.0M) | precise (128,400) | whole (128400).",
            names(&["compact", "precise", "whole"]),
            s("tokens"),
        )
        .valued(draft, Some(string(f.tokens.name())), "compact"),
        Field::new(
            "format.percent",
            "Percentages: whole (42%) | precise (42.3%).",
            names(&["whole", "precise"]),
            s("percent"),
        )
        .valued(draft, Some(string(f.percent.name())), "whole"),
        Field::new(
            "format.cost",
            "Money: precise ($1.23, cost.decimals places) | whole ($1).",
            names(&["precise", "whole"]),
            s("cost"),
        )
        .valued(draft, Some(string(f.cost.name())), "precise"),
        Field::new(
            "format.parens",
            "Parenthesised details (api's share, lines' net, a both reset): plain | dim (the muted role).",
            names(&["plain", "dim"]),
            s("parens"),
        )
        .valued(draft, Some(string(f.parens.name())), "plain"),
    ]
}

/// The top-level keys that pick the preset, the glyphs and the colours.
fn look_fields(draft: &Draft, config: &Config) -> Vec<Field> {
    let s = |key: &str| Slot::top(key);
    let presets: Vec<&str> = config::presets::TopPreset::ALL.iter().map(|p| p.name()).collect();
    let icon_sets: Vec<&str> = IconSet::ALL.iter().map(|i| i.name()).collect();
    let themes: Vec<&str> = PALETTES.iter().map(|p| p.name).collect();
    vec![
        Field::new(
            "preset",
            "Which rows exist and how much each module says: default | minimal | full | compact.",
            names(&presets),
            s("preset"),
        )
        .valued(draft, Some(string(config.preset.name())), "default"),
        Field::new(
            "icons",
            "The glyph set: nerd needs a Nerd Font; unicode, emoji and ascii do not.",
            names(&icon_sets),
            s("icons"),
        )
        .valued(draft, Some(string(config.icons.name())), "nerd"),
        Field::new(
            "theme",
            "The colour palette every role comes from.",
            names(&themes),
            s("theme"),
        )
        .valued(draft, Some(string(&config.theme_name)), "garnish"),
        Field::new(
            "color",
            "Colour output: auto | always | never | 256 | truecolor.",
            names(&["auto", "always", "never", "256", "truecolor"]),
            s("color"),
        )
        .valued(draft, Some(string(config.color.name())), "auto"),
        Field::new(
            "truncate",
            "Cut the left group when a line overflows the width.",
            SlotKind::Bool,
            s("truncate"),
        )
        .valued(draft, Some(Value::Boolean(config.truncate)), "true"),
        Field::new(
            "stale_style",
            "How an overdue cached value is shown: dim | hide | plain.",
            names(&["dim", "hide", "plain"]),
            s("stale_style"),
        )
        .valued(draft, Some(string(config.stale_style.name())), "dim"),
        Field::new(
            "stale_after",
            "TTL periods a cached value may be overdue before it is styled stale.",
            SlotKind::Int { min: 1, max: None },
            s("stale_after"),
        )
        .valued(draft, Some(Value::Integer(i64::from(config.stale_after))), "5"),
        Field::new(
            "padding",
            "Extra cells subtracted from the width: 2 × statusLine.padding.",
            SlotKind::Int { min: 0, max: Some(65_535) },
            s("padding"),
        )
        .valued(
            draft,
            Some(Value::Integer(i64::try_from(config.padding).unwrap_or(0))),
            "0",
        ),
    ]
}

/// The top-level keys that lay the rows out and move things.
fn layout_fields(draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let s = |key: &str| Slot::top(key);
    vec![
        Field::new(
            "align",
            "Pad every module column to the widest module in it, so separators line up.",
            SlotKind::Bool,
            s("align"),
        )
        .valued(draft, Some(Value::Boolean(config.align)), "false"),
        Field::new(
            "right_justify",
            "Where a padded right-group module's text sits: end (hugs the cap) | start.",
            names(&["end", "start"]),
            s("right_justify"),
        )
        .valued(draft, Some(string(config.right_justify.name())), "end"),
        Field::new(
            "hide_empty_rows",
            "Drop a row whose modules all rendered nothing (spacers stay).",
            SlotKind::Bool,
            s("hide_empty_rows"),
        )
        .valued(draft, Some(Value::Boolean(config.hide_empty_rows)), "true"),
        Field::new(
            "overflow",
            "A left group wider than its budget: truncate (cut) | ticker (scroll).",
            names(&["truncate", "ticker"]),
            s("overflow"),
        )
        .valued(draft, Some(string(config.overflow.name())), "truncate"),
        Field::new(
            "ticker_step",
            "Cells the ticker advances per tick (0.5 = every second tick).",
            SlotKind::Float,
            s("ticker_step"),
        )
        .valued(draft, Some(Value::Float(config.ticker_step)), "1"),
        Field::new(
            "ticker_gap",
            "Text between the end of a scrolled group and its start.",
            SlotKind::Str,
            s("ticker_gap"),
        )
        .valued(draft, Some(string(&config.ticker_gap)), "\"   \"")
        .with_choices(hints.choices("ticker_gap")),
        Field::new(
            "animate",
            "Every animation: unset follows Claude Code's reduced-motion setting.",
            SlotKind::Tri,
            s("animate"),
        )
        .valued(draft, config.animate.map(Value::Boolean), "unset"),
        Field::new(
            "durations",
            "How timers print: compact (9m) | fixed (9m00s); unset is fixed under a ticker.",
            names(&["compact", "fixed"]),
            s("durations"),
        )
        .valued(draft, Some(string(config.durations.name())), "compact"),
    ]
}

/// `[frame]`: the style and its glyphs, then the animation keys.
fn frame_fields(draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let mut fields = frame_glyph_fields(draft, config, hints);
    fields.extend(frame_motion_fields(draft, config, hints));
    let frame = draft.get(&["frame"]).and_then(Value::as_table);
    let extra = extra_fields(&fields, frame, &|k| Slot::table(&["frame"], k), "", &[], draft);
    fields.extend(extra);
    fields
}

fn frame_glyph_fields(draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let s = |key: &str| Slot::table(&["frame"], key);
    let styles: Vec<String> = FrameStyle::ALL.iter().map(|f| f.name().to_owned()).collect();
    let c = &config.frame.chars;
    let mut fields = vec![
        Field::new(
            "style",
            "none | rounded | square | double | heavy | powerline | custom.",
            SlotKind::Enum(styles),
            s("style"),
        )
        .valued(draft, Some(string(config.frame.style.name())), "rounded"),
        Field::new(
            "fill",
            "Rule to the full width and close with the right cap.",
            SlotKind::Bool,
            s("fill"),
        )
        .valued(draft, Some(Value::Boolean(config.frame.fill)), "true"),
    ];
    for (key, value, doc) in [
        ("separator", &c.separator, "Default separator between modules."),
        ("pad", &c.pad, "Text between a cap and the content."),
        ("fill_char", &c.fill, "The rule glyph, one cell."),
        ("first", &c.first, "Left cap of the first line (custom style)."),
        ("middle", &c.middle, "Left cap of middle lines."),
        ("last", &c.last, "Left cap of the last line."),
        ("single", &c.single, "Left cap of a lone line."),
        ("right_first", &c.right_first, "Right cap of the first line."),
        ("right_middle", &c.right_middle, "Right cap of middle lines."),
        ("right_last", &c.right_last, "Right cap of the last line."),
        ("right_single", &c.right_single, "Right cap of a lone line."),
        ("top_left", &c.top_left, "A box's top-left corner, one cell."),
        ("top_right", &c.top_right, "A box's top-right corner."),
        ("bottom_left", &c.bottom_left, "A box's bottom-left corner."),
        ("bottom_right", &c.bottom_right, "A box's bottom-right corner."),
        ("side", &c.side, "A box's side glyph."),
    ] {
        fields.push(
            Field::new(key, doc, SlotKind::Str, s(key))
                .valued(draft, Some(string(value)), "the style's")
                .with_choices(hints.choices(key)),
        );
    }
    // Right after `separator`: a role, a literal, or `inherit` (SPEC § 4.1).
    let mut choices = vec![Choice::noted("inherit", "the module before it")];
    choices.extend(Role::ALL.iter().map(|r| Choice::noted(r.name(), "role")));
    fields.insert(
        3,
        Field::new(
            "separator_color",
            "Every separator's colour: muted | inherit (the module before it) | a role or literal.",
            SlotKind::Str,
            s("separator_color"),
        )
        .valued(draft, Some(string(config.frame.separator_color.spec())), "muted")
        .with_choices(choices),
    );
    fields
}

fn frame_motion_fields(draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let s = |key: &str| Slot::table(&["frame"], key);
    vec![
        Field::new(
            "fill_pattern",
            "One-cell glyphs travelling along the rule instead of fill_char.",
            SlotKind::Str,
            s("fill_pattern"),
        )
        .valued(draft, Some(string(&config.frame.fill_pattern.concat())), "none")
        .with_choices(hints.choices("fill_pattern")),
        Field::new(
            "fill_step",
            "Cells the pattern shifts per tick.",
            SlotKind::Float,
            s("fill_step"),
        )
        .valued(draft, Some(Value::Float(config.frame.fill_step)), "1"),
        Field::new(
            "fill_direction",
            "left | right.",
            SlotKind::Enum(vec!["left".into(), "right".into()]),
            s("fill_direction"),
        )
        .valued(draft, Some(string(config.frame.fill_direction.name())), "right"),
        Field::new(
            "separator_frames",
            "Separator frames cycled one per tick, all the same width, as a TOML array.",
            SlotKind::Frames,
            s("separator_frames"),
        )
        .valued(
            draft,
            Some(Value::Array(
                config.frame.separator_frames.iter().map(|f| Value::String(f.clone())).collect(),
            )),
            "[]",
        ),
        Field::new(
            "separator_step",
            "Frames the separator advances per tick.",
            SlotKind::Float,
            s("separator_step"),
        )
        .valued(draft, Some(Value::Float(config.frame.separator_step)), "1"),
    ]
}

fn color_fields(draft: &Draft, config: &Config) -> Vec<Field> {
    let mut fields: Vec<Field> = Role::ALL
        .iter()
        .map(|role| {
            let palette =
                crate::theme::palette(&config.theme_name).map_or("default", |p| p.spec(*role));
            Field::new(
                role.name(),
                "A role every module colour defaults to; a name, 0-255 or #rrggbb.",
                SlotKind::Color,
                Slot::table(&["colors"], role.name()),
            )
            .valued(draft, Some(Value::String(config.theme.role(*role).to_spec())), palette)
            .with_choices(literal_color_choices(config))
        })
        .collect();
    let colors = draft.get(&["colors"]).and_then(Value::as_table);
    let extra = extra_fields(&fields, colors, &|k| Slot::table(&["colors"], k), "", &[], draft);
    fields.extend(extra);
    fields
}

/// A row's form lists the keys the parser would take for it: `blank` only
/// on a spacer or a row with columns, `gap` only on an outer row, the
/// title keys only outside a named box (the box carries the title), each
/// still listed while the file sets it, and any other key the table holds
/// after them, so `d` can unset one the parser reports.
fn row_fields(at: RowAt, draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    // No table at the path (an undo took the row back under its open
    // form): nothing to edit, and the app closes the form.
    let Some(table) = draft.row(at).cloned() else { return Vec::new() };
    let raw = |key: &str| table.get(key).cloned();
    let s = |key: &str| Slot::row(at, key);
    let ids = |key: &str| table.get(key).and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    let has_cols = table.contains_key("col");
    let spacer = !has_cols && !ids("modules") && !ids("right");
    let named_box = matches!(table.get("box"), Some(Value::String(_)));
    // What an unset key resolves to, so `←`/`→` and a picker start there.
    let resolved = resolved_row(config, at);
    let separator = resolved
        .and_then(|r| r.separator.clone())
        .unwrap_or_else(|| config.frame.chars.separator.clone());
    let gap = resolved.map_or(config::DEFAULT_GAP, |r| r.gap);
    let mut fields = vec![
        Field::new(
            "separator",
            "Joins this row's modules; the frame's when unset.",
            SlotKind::Str,
            s("separator"),
        )
        .valued(
            draft,
            raw("separator").or(Some(Value::String(separator))),
            &config.frame.chars.separator,
        )
        .with_choices(hints.choices("separator")),
    ];
    if at.col.is_none() || table.contains_key("gap") {
        fields.push(
            Field::new(
                "gap",
                "Empty cells between columns.",
                SlotKind::Int { min: 0, max: Some(16) },
                s("gap"),
            )
            .valued(draft, raw("gap").or_else(|| Some(count(gap))), "1"),
        );
    }
    if !named_box || TITLE_KEYS.iter().any(|k| table.contains_key(*k)) {
        let title = resolved.and_then(|r| r.title.as_ref());
        fields.extend(title_fields(&s, &raw, title, draft, hints, config));
    }
    fields.push(
        Field::new(
            "box",
            "The box this row joins: none, true (alone) or a [box.<name>].",
            SlotKind::BoxRef,
            s("box"),
        )
        .valued(draft, raw("box"), "none")
        .with_choices(box_choices(config)),
    );
    if spacer || has_cols || table.contains_key("blank") {
        let blank = resolved.is_some_and(|r| r.blank);
        fields.push(
            Field::new(
                "blank",
                "Keep a spacer (or a row of columns) on screen with one invisible cell when colour is off.",
                SlotKind::Bool,
                s("blank"),
            )
            .valued(draft, raw("blank").or(Some(Value::Boolean(blank))), "false"),
        );
    }
    // The groups and the lists of columns are the builder's.
    let skip = ["modules", "right", "col", "row"];
    let extra = extra_fields(&fields, Some(&table), &s, "", &skip, draft);
    fields.extend(extra);
    fields
}

/// The title keys of a row or a box, `resolved` the title in effect.
fn title_fields(
    s: &dyn Fn(&str) -> Slot,
    raw: &dyn Fn(&str) -> Option<Value>,
    resolved: Option<&config::TitleCfg>,
    draft: &Draft,
    hints: &Suggestions,
    config: &Config,
) -> Vec<Field> {
    let effect = resolved.cloned().unwrap_or_default();
    vec![
        Field::new("title", "Plain text set into the rule.", SlotKind::Str, s("title"))
            .valued(draft, raw("title"), "none")
            .with_choices(hints.choices("title")),
        Field::new(
            "title_justify",
            "left | center | right.",
            SlotKind::Enum(vec!["left".into(), "center".into(), "right".into()]),
            s("title_justify"),
        )
        .valued(
            draft,
            raw("title_justify").or_else(|| Some(string(effect.justify.name()))),
            "left",
        ),
        Field::new(
            "title_pad",
            "Spaces on each side of the title.",
            SlotKind::Int { min: 0, max: Some(64) },
            s("title_pad"),
        )
        .valued(draft, raw("title_pad").or_else(|| Some(count(effect.pad))), "1"),
        Field::new(
            "title_color",
            "A role or literal for the title; the frame colour when unset.",
            SlotKind::Color,
            s("title_color"),
        )
        .valued(draft, raw("title_color"), "frame")
        .with_choices(color_choices(config)),
    ]
}

/// A count as a TOML integer.
fn count(n: usize) -> Value {
    Value::Integer(i64::try_from(n).unwrap_or(i64::MAX))
}

/// The resolved row, or a stack's inner row, at `at` (a column is none).
fn resolved_row(config: &Config, at: RowAt) -> Option<&config::RowCfg> {
    let row = config.rows.get(at.row)?;
    match (at.col, at.inner) {
        (None, _) => Some(row),
        (Some(c), Some(i)) => row.cols.get(c)?.rows.get(i),
        (Some(_), None) => None,
    }
}

fn col_fields(at: RowAt, draft: &Draft, config: &Config) -> Vec<Field> {
    let Some(table) = draft.row(at).cloned() else { return Vec::new() };
    let raw = |key: &str| table.get(key).cloned();
    let s = |key: &str| Slot::row(at, key);
    // Unset, `justify` follows the column's place and `valign` is `top`.
    let col = at.col.and_then(|c| config.rows.get(at.row)?.cols.get(c));
    let justify = raw("justify").or_else(|| col.map(|c| string(c.justify.name())));
    let valign = raw("valign").or_else(|| col.map(|c| string(c.valign.name())));
    let mut fields = vec![
        Field::new("width", "\"<n>fr\" (a share of what is left) | \"auto\" (the content) | a cell count.", SlotKind::Width, s("width"))
            .valued(draft, raw("width"), "1fr"),
        Field::new("justify", "Where a lone modules group sits: left | center | right (default follows the position).", SlotKind::Enum(vec!["left".into(), "center".into(), "right".into()]), s("justify"))
            .valued(draft, justify, "by position"),
        Field::new("valign", "Where a short stack sits in a taller row: top | center | bottom.", SlotKind::Enum(vec!["top".into(), "center".into(), "bottom".into()]), s("valign"))
            .valued(draft, valign, "top"),
        Field::new("box", "Box the whole column: none, true or a [box.<name>].", SlotKind::BoxRef, s("box"))
            .valued(draft, raw("box"), "none")
            .with_choices(box_choices(config)),
    ];
    // The groups and the stack's rows are the builder's.
    let extra = extra_fields(&fields, Some(&table), &s, "", &["modules", "right", "row"], draft);
    fields.extend(extra);
    fields
}

fn box_fields(name: &str, draft: &Draft, config: &Config, hints: &Suggestions) -> Vec<Field> {
    let base = ["box", name];
    let Some(table) = draft.get(&base).and_then(Value::as_table).cloned() else {
        return Vec::new();
    };
    let raw = |key: &str| table.get(key).cloned();
    let s = |key: &str| Slot::table(&base, key);
    let resolved = config.boxes.get(name);
    let mut fields =
        title_fields(&s, &raw, resolved.and_then(|b| b.title.as_ref()), draft, hints, config);
    let styles: Vec<String> = FrameStyle::ALL
        .iter()
        .filter(|f| **f != FrameStyle::Powerline)
        .map(|f| f.name().to_owned())
        .collect();
    // Unset, the frame's style, rounded when that has no box shape (as the
    // layout draws it).
    let style = resolved.and_then(|b| b.style).unwrap_or(match config.frame.style {
        FrameStyle::None | FrameStyle::Powerline => FrameStyle::Rounded,
        other => other,
    });
    let fill = resolved.is_some_and(|b| b.fill);
    fields.push(
        Field::new(
            "style",
            "The box's glyphs; the frame's style when unset (rounded if that has no box shape).",
            SlotKind::Enum(styles),
            s("style"),
        )
        .valued(draft, raw("style").or_else(|| Some(string(style.name()))), "the frame's"),
    );
    fields.push(
        Field::new(
            "fill",
            "Draw the rule between a row's groups inside the box.",
            SlotKind::Bool,
            s("fill"),
        )
        .valued(draft, raw("fill").or(Some(Value::Boolean(fill))), "false"),
    );
    fields.push(
        Field::new(
            "color",
            "A role or literal for the box's glyphs; the frame colour when unset.",
            SlotKind::Color,
            s("color"),
        )
        .valued(draft, raw("color"), "frame")
        .with_choices(color_choices(config)),
    );
    let extra = extra_fields(&fields, Some(&table), &s, "", &[], draft);
    fields.extend(extra);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::SCHEMAS;

    fn built(text: &str, kind: &FormKind) -> (Form, Draft) {
        let draft = Draft::from_text(text);
        let (config, _) = draft.resolved();
        (Form::build(kind.clone(), &draft, &config, &Suggestions::default()), draft)
    }

    /// SPEC § 14: every `OptSpec` kind and every top-level key has a form
    /// row, so an option added to a schema appears in `setup` the next build.
    #[test]
    fn every_option_kind_and_top_level_key_has_a_field() {
        for kind in [
            Kind::Bool,
            Kind::Int,
            Kind::Float,
            Kind::Str,
            Kind::Enum(&["a"]),
            Kind::StrList,
            Kind::NumList,
            Kind::ColorList,
        ] {
            let _ = SlotKind::of(kind, None);
        }
        let (top, _) = built("", &FormKind::Top);
        let keys: Vec<&str> = top.fields.iter().map(|f| f.key.as_str()).collect();
        for key in config::TOP_KEYS {
            let table = matches!(
                key,
                "colors"
                    | "frame"
                    | "format"
                    | "row"
                    | "line"
                    | "box"
                    | "modules"
                    | "hide_empty_lines"
            );
            assert!(table || keys.contains(&key), "{key} has no field");
        }
        // The `[format]` table's four keys sit on the same screen.
        for key in ["format.tokens", "format.percent", "format.cost", "format.parens"] {
            assert!(keys.contains(&key), "{key} has no field");
        }
        for schema in SCHEMAS.iter() {
            let (form, _) = built("", &FormKind::Module(schema.id.to_owned()));
            let keys: Vec<&str> = form.fields.iter().map(|f| f.key.as_str()).collect();
            for opt in &schema.opts {
                assert!(keys.contains(&opt.key), "{}.{}", schema.id, opt.key);
            }
            for icon in &schema.icons {
                assert!(keys.contains(&format!("icons.{}", icon.key).as_str()), "{}", schema.id);
            }
            for color in &schema.colors {
                assert!(keys.contains(&format!("colors.{}", color.key).as_str()), "{}", schema.id);
            }
            assert!(
                keys.contains(&"preset") && keys.contains(&"max_width") && keys.contains(&"hide")
            );
            let hide = form.fields.iter().find(|f| f.key == "hide").unwrap();
            assert_eq!(hide.kind, SlotKind::StrList);
            for state in schema.hide_states() {
                assert!(hide.doc.contains(state), "{}: {}", schema.id, hide.doc);
            }
        }
        let (text, _) =
            built("[modules.text.motd]\ntext = \"hi\"\n", &FormKind::Module("text.motd".into()));
        let keys: Vec<&str> = text.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(
            keys.contains(&"text")
                && keys.contains(&"width")
                && keys.contains(&"hide")
                && !keys.contains(&"max_width")
                && !keys.contains(&"preset")
        );
        assert!(text.fields.iter().find(|f| f.key == "text").unwrap().set);
        let (frame, _) = built("", &FormKind::Frame);
        assert!(frame.fields.iter().any(|f| f.key == "fill_pattern"));
        let (colors, _) = built("", &FormKind::Colors);
        assert_eq!(colors.fields.len(), Role::ALL.len());
        let (row, _) =
            built("[[row]]\nmodules = [\"clock\"]\ntitle = \"T\"\n", &FormKind::Row(RowAt::row(0)));
        assert!(row.fields.iter().find(|f| f.key == "title").unwrap().set);
        let (col, _) = built(
            "[[row]]\n[[row.col]]\nwidth = 20\n",
            &FormKind::Col(RowAt { row: 0, col: Some(0), inner: None }),
        );
        assert_eq!(col.fields.iter().find(|f| f.key == "width").unwrap().current, "20");
        let (b, _) =
            built("[box.x]\ntitle = \"X\"\n[[row]]\nbox = \"x\"\n", &FormKind::Box("x".into()));
        assert_eq!(b.fields.iter().find(|f| f.key == "title").unwrap().current, "X");
    }

    #[test]
    fn fields_step_toggle_and_open_the_right_layer() {
        let (mut form, _) =
            built("[modules.context]\nwidth = 20\n", &FormKind::Module("context".into()));
        form.focus("width");
        let f = form.fields.get(form.cursor).unwrap().clone();
        assert!(f.set && f.current == "20");
        let out = form.handle(Key::Right);
        assert_eq!(out.actions, vec![Action::Set(f.slot.clone(), Value::Integer(21))]);
        let out = form.handle(Key::Char('d'));
        assert_eq!(out.actions, vec![Action::Unset(f.slot)]);
        assert!(
            matches!(form.handle(Key::Enter).push, Some(Layer::Input(_))),
            "an integer opens an input"
        );
        form.focus("enabled");
        let out = form.handle(Key::Enter);
        assert!(matches!(out.actions.first(), Some(Action::Set(_, Value::Boolean(false)))));
        form.focus("preset");
        assert!(matches!(form.handle(Key::Enter).push, Some(Layer::Choose(_))));
        let out = form.handle(Key::Right);
        assert!(
            matches!(out.actions.first(), Some(Action::Set(_, Value::String(s))) if s == "minimal")
        );
        form.focus("icons.fill");
        assert!(
            matches!(form.handle(Key::Enter).push, Some(Layer::Choose(c)) if c.items.iter().any(|i| i.custom))
        );
        form.focus("colors.icon");
        assert!(
            matches!(form.handle(Key::Enter).push, Some(Layer::Choose(c)) if c.items.iter().any(|i| i.value == "accent"))
        );
        assert!(form.handle(Key::Esc).close);
        // Typed text parses per kind; an integer outside its bounds is
        // refused with the bound named, never clamped in silence.
        assert_eq!(SlotKind::Int { min: 0, max: Some(5) }.parse("5"), Ok(Some(Value::Integer(5))));
        let over = SlotKind::Int { min: 0, max: Some(5) }.parse("9").unwrap_err();
        assert!(over.contains("maximum of 5"), "{over}");
        assert!(SlotKind::Int { min: 1, max: None }.parse("0").unwrap_err().contains("minimum"));
        assert!(SlotKind::Int { min: 0, max: None }.parse("x").is_err());
        assert_eq!(
            SlotKind::StrList.parse("a, b,").unwrap(),
            Some(Value::Array(vec![Value::String("a".into()), Value::String("b".into())]))
        );
        assert_eq!(SlotKind::Width.parse("24").unwrap(), Some(Value::Integer(24)));
        assert_eq!(SlotKind::Width.parse("2fr").unwrap(), Some(Value::String("2fr".into())));
        assert_eq!(SlotKind::BoxRef.parse("none").unwrap(), None);
        assert_eq!(SlotKind::BoxRef.parse("true").unwrap(), Some(Value::Boolean(true)));
        assert_eq!(SlotKind::Tri.parse("").unwrap(), None);
        assert!(SlotKind::NumList.parse("1, x").is_err());
        // frm-07: `f64` parses `nan` and `inf`, which no option means.
        for text in ["nan", "NaN", "inf", "-inf"] {
            assert!(SlotKind::Float.parse(text).is_err(), "{text}");
            assert!(SlotKind::NumList.parse(&format!("1, {text}")).is_err(), "{text}");
        }
        assert_eq!(SlotKind::Float.parse("2.5"), Ok(Some(Value::Float(2.5))));
        // The animate tri-state cycles unset → true → false → unset.
        let (mut top, _) = built("", &FormKind::Top);
        top.focus("animate");
        assert!(matches!(
            top.handle(Key::Enter).actions.first(),
            Some(Action::Set(_, Value::Boolean(true)))
        ));
        let (mut top, _) = built("animate = true\n", &FormKind::Top);
        top.focus("animate");
        assert!(matches!(
            top.handle(Key::Enter).actions.first(),
            Some(Action::Set(_, Value::Boolean(false)))
        ));
        let (mut top, _) = built("animate = false\n", &FormKind::Top);
        top.focus("animate");
        assert!(matches!(top.handle(Key::Enter).actions.first(), Some(Action::Unset(_))));
    }

    /// frm-02: separator frames keep their spaces (and may hold commas), so
    /// they are edited as the TOML array they are written as; an untouched
    /// `Enter` gives the same array back.
    #[test]
    fn frames_are_typed_as_a_toml_array_and_keep_their_spaces() {
        let (mut form, _) =
            built("[frame]\nseparator_frames = [\" │ \", \" , \"]\n", &FormKind::Frame);
        form.focus("separator_frames");
        let field = form.fields.get(form.cursor).unwrap().clone();
        let Some(Layer::Input(input)) = form.handle(Key::Enter).push else { panic!("an input") };
        assert_eq!(input.text, "[\" │ \", \" , \"]");
        let want = Value::Array(vec![Value::String(" │ ".into()), Value::String(" , ".into())]);
        assert_eq!(field.kind.parse(&input.text), Ok(Some(want)));
        assert_eq!(SlotKind::Frames.parse("[]"), Ok(Some(Value::Array(Vec::new()))));
        assert_eq!(SlotKind::Frames.parse("  "), Ok(None), "nothing typed unsets it");
        assert!(SlotKind::Frames.parse("[1, 2]").is_err());
        assert!(SlotKind::Frames.parse(" │ , ┃ ").is_err(), "not an array literal");
        // The comma form stays for the lists whose items have no spaces.
        let (mut m, _) =
            built("[modules.context]\nhide = [\"empty\"]\n", &FormKind::Module("context".into()));
        m.focus("hide");
        let Some(Layer::Input(input)) = m.handle(Key::Enter).push else { panic!("an input") };
        assert_eq!(input.text, "empty");
    }

    /// frm-03: every form lists the keys its table holds beyond its own
    /// rows (an unknown or misplaced key the parser reports, which `d` then
    /// removes), and gives the legal ones it had no row for a proper one.
    #[test]
    fn every_form_lists_the_keys_its_table_has_beyond_its_own() {
        let inner = RowAt { row: 0, col: Some(0), inner: Some(0) };
        let col = RowAt { row: 0, col: Some(0), inner: None };
        let module = |id: &str| FormKind::Module(id.to_owned());
        for (text, kind, key, path) in [
            (
                "[modules.clock]\nfromat = \"12h\"\n",
                module("clock"),
                "fromat",
                "modules.clock.fromat",
            ),
            (
                "[modules.clock.icons]\nnope = \"x\"\n",
                module("clock"),
                "icons.nope",
                "modules.clock.icons.nope",
            ),
            (
                "[modules.clock.colors]\nnope = \"red\"\n",
                module("clock"),
                "colors.nope",
                "modules.clock.colors.nope",
            ),
            (
                "[modules.text.m]\ntext = \"hi\"\nrefresh = 3\n",
                module("text.m"),
                "refresh",
                "modules.text.m.refresh",
            ),
            ("nope = 1\n", FormKind::Top, "nope", "nope"),
            ("[format]\nnope = 1\n", FormKind::Top, "format.nope", "format.nope"),
            ("[frame]\nnope = 1\n", FormKind::Frame, "nope", "frame.nope"),
            ("[colors]\nnope = \"red\"\n", FormKind::Colors, "nope", "colors.nope"),
            (
                "[[row]]\nmodules = [\"clock\"]\nnope = 1\n",
                FormKind::Row(RowAt::row(0)),
                "nope",
                "row[0].nope",
            ),
            (
                "[[row]]\n[[row.col]]\n[[row.col.row]]\nmodules = [\"clock\"]\ngap = 2\n",
                FormKind::Row(inner),
                "gap",
                "row[0].col[0].row[0].gap",
            ),
            (
                "[[row]]\n[[row.col]]\nmodules = [\"clock\"]\nnope = 1\n",
                FormKind::Col(col),
                "nope",
                "row[0].col[0].nope",
            ),
            (
                "[box.b]\nnope = 1\n[[row]]\nmodules = [\"clock\"]\nbox = \"b\"\n",
                FormKind::Box("b".into()),
                "nope",
                "box.b.nope",
            ),
        ] {
            let (form, mut draft) = built(text, &kind);
            let problems = draft.resolved().1;
            assert!(problems.iter().any(|p| p.path == path), "{text}: {problems:?}");
            let keys: Vec<&str> = form.fields.iter().map(|f| f.key.as_str()).collect();
            let field = form.fields.iter().find(|f| f.key == key);
            let field = field.unwrap_or_else(|| panic!("{text}: no {key} in {keys:?}"));
            assert!(field.set && field.slot.path() == path, "{text}: {field:?}");
            field.slot.unset(&mut draft);
            let problems = draft.resolved().1;
            assert!(!problems.iter().any(|p| p.path == path), "{text}: {problems:?}");
        }
        // A legal key with no row of its own gets a proper one: an icon's
        // frames (typed as an array) and a text module's `color` shorthand.
        let dots = crate::gallery::body(crate::gallery::find("animated-dots").unwrap().source);
        let (m, _) = built(&dots, &module("model"));
        let frames = m.fields.iter().find(|f| f.key == "icons.model_frames").unwrap();
        assert!(frames.set && frames.kind == SlotKind::Frames, "{frames:?}");
        let ticker = crate::gallery::body(crate::gallery::find("motd-ticker").unwrap().source);
        let draft = Draft::from_text(&ticker);
        let (config, _) = draft.resolved();
        let name = config.texts.keys().next().unwrap().clone();
        let (t, _) = built(&ticker, &module(&format!("text.{name}")));
        let color = t.fields.iter().find(|f| f.key == "color").unwrap();
        assert!(color.set && color.kind == SlotKind::Color, "{color:?}");
        // A generic row types its value as a TOML literal.
        let (mut form, _) = built("[modules.clock]\nfromat = \"12h\"\n", &module("clock"));
        form.focus("fromat");
        let Some(Layer::Input(input)) = form.handle(Key::Enter).push else { panic!("an input") };
        assert_eq!(input.text, "\"12h\"");
        assert_eq!(
            SlotKind::Literal.parse("[1, 2]"),
            Ok(Some(Value::Array(vec![1.into(), 2.into()])))
        );
        assert!(SlotKind::Literal.parse("12h").is_err(), "a string is quoted");
    }

    /// frm-06: an unset row, column or box key steps and opens from the
    /// value in effect, not from the minimum or the first entry.
    #[test]
    fn unset_layout_keys_step_from_the_value_in_effect() {
        let step = |form: &mut Form, key: &str, k: Key| {
            form.focus(key);
            match form.handle(k).actions.first() {
                Some(Action::Set(_, v)) => v.clone(),
                other => panic!("{key}: {other:?}"),
            }
        };
        let open = |form: &mut Form, key: &str| {
            form.focus(key);
            let Some(Layer::Choose(c)) = form.handle(Key::Enter).push else { panic!("a picker") };
            c.items.get(c.cursor).map(|i| i.value.clone())
        };
        let (mut b, _) = built(
            "[box.x]\n[[row]]\nbox = \"x\"\nmodules = [\"clock\"]\n",
            &FormKind::Box("x".into()),
        );
        assert_eq!(b.fields.iter().find(|f| f.key == "style").unwrap().current, "rounded");
        assert_eq!(step(&mut b, "style", Key::Right), Value::String("square".into()));
        assert_eq!(open(&mut b, "style").as_deref(), Some("rounded"));
        let (mut c, _) = built(
            "[[row]]\n[[row.col]]\n[[row.col]]\n[[row.col]]\nmodules = [\"clock\"]\n",
            &FormKind::Col(RowAt { row: 0, col: Some(2), inner: None }),
        );
        assert_eq!(open(&mut c, "justify").as_deref(), Some("right"));
        assert_eq!(open(&mut c, "valign").as_deref(), Some("top"));
        let (mut r, _) = built("[[row]]\nmodules = [\"clock\"]\n", &FormKind::Row(RowAt::row(0)));
        assert_eq!(step(&mut r, "gap", Key::Right), Value::Integer(2));
        assert_eq!(step(&mut r, "title_pad", Key::Right), Value::Integer(2));
        assert_eq!(open(&mut r, "title_justify").as_deref(), Some("left"));
        let gap = r.fields.iter().find(|f| f.key == "gap").unwrap();
        assert!(!gap.set && gap.current == "1", "{gap:?}");
    }

    /// frm-05: a module colour's row shows the role or literal in effect,
    /// as written, and its picker opens on it; a text module's `color`
    /// shorthand is what its `colors.text` row shows while that is unset.
    #[test]
    fn a_module_colour_shows_and_opens_on_the_value_as_written() {
        let module = |id: &str| FormKind::Module(id.to_owned());
        let open = |form: &mut Form, key: &str| {
            form.focus(key);
            let Some(Layer::Choose(c)) = form.handle(Key::Enter).push else { panic!("a picker") };
            c.items.get(c.cursor).map(|i| i.value.clone())
        };
        let (mut form, _) = built("", &module("context"));
        let marker = form.fields.iter().find(|f| f.key == "colors.marker").unwrap().clone();
        assert_eq!((marker.current.as_str(), marker.set), ("warn", false));
        assert_eq!(open(&mut form, "colors.marker").as_deref(), Some("warn"));
        let (mut form, _) =
            built("[modules.context.colors]\nmarker = \"ok\"\n", &module("context"));
        let marker = form.fields.iter().find(|f| f.key == "colors.marker").unwrap().clone();
        assert_eq!((marker.current.as_str(), marker.set), ("ok", true));
        assert_eq!(open(&mut form, "colors.marker").as_deref(), Some("ok"));
        let (t, _) =
            built("[modules.text.m]\ntext = \"hi\"\ncolor = \"accent2\"\n", &module("text.m"));
        let text = t.fields.iter().find(|f| f.key == "colors.text").unwrap();
        assert_eq!(text.current, "accent2");
    }

    #[test]
    fn suggestions_come_from_the_frames_and_the_gallery() {
        let s = Suggestions::gather();
        let seps = s.for_key("separator");
        assert!(seps.iter().any(|v| v == " │ ") && seps.iter().any(|v| v == "  "), "{seps:?}");
        assert!(!s.for_key("text").is_empty(), "text modules in the gallery");
        assert_eq!(s.for_key("nope"), Vec::<String>::new());
        let icons = icon_choices(SCHEMAS.iter().find(|s| s.id == "model").unwrap(), "model");
        assert!(icons.len() >= 4);
        assert_eq!(
            show(&Value::Array(vec![Value::String("a".into()), Value::Integer(2)])),
            "[a, 2]"
        );
    }

    /// What a walk of every preset's forms found (2026-09-20): a picked
    /// separator lost its spaces, the `[colors]` picker offered roles the
    /// parser refuses, `blank` and the title keys were offered where no
    /// value is legal, and `custom…` started empty.
    #[test]
    fn strings_keep_spaces_colours_take_literals_and_rows_list_only_legal_keys() {
        assert_eq!(SlotKind::Str.parse(" │ ").unwrap(), Some(Value::String(" │ ".into())));
        assert_eq!(SlotKind::Str.parse("  ").unwrap(), Some(Value::String("  ".into())));
        assert_eq!(SlotKind::Icon.parse(" ").unwrap(), Some(Value::String(" ".into())));
        assert_eq!(
            SlotKind::Enum(vec!["a".into()]).parse(" a ").unwrap(),
            Some(Value::String("a".into()))
        );
        // `[colors]` takes literals: every entry its picker offers is one.
        let (colors, _) = built("theme = \"nord\"\n", &FormKind::Colors);
        for f in &colors.fields {
            assert!(!f.choices.is_empty(), "{}", f.key);
            for c in &f.choices {
                assert!(
                    crate::ansi::Color::parse(&c.value).is_some(),
                    "{}: {:?} is not a literal colour",
                    f.key,
                    c.value
                );
            }
        }
        // A module colour still offers the roles, and its label picker
        // starts with the module's own name.
        let (m, _) = built("", &FormKind::Module("model".into()));
        assert!(
            m.fields
                .iter()
                .find(|f| f.key == "colors.icon")
                .unwrap()
                .choices
                .iter()
                .any(|c| c.value == "accent")
        );
        let label = |form: &Form| -> Vec<String> {
            form.fields
                .iter()
                .find(|f| f.key == "label")
                .unwrap()
                .choices
                .iter()
                .take(2)
                .map(|c| c.value.clone())
                .collect()
        };
        assert_eq!(label(&m), vec!["model", "Model"]);
        let (sn, _) = built("", &FormKind::Module("session_name".into()));
        assert_eq!(label(&sn), vec!["session_name", "Session name"]);
        let (t, _) =
            built("[modules.text.motd]\ntext = \"hi\"\n", &FormKind::Module("text.motd".into()));
        assert_eq!(label(&t), vec!["motd", "Motd"]);
        // A plain row has no `blank`; a spacer and a row of columns do; a
        // row inside a named box has no title keys. A key the file sets is
        // listed whatever the rule, so `d` can unset it.
        let keys = |text: &str| -> Vec<String> {
            built(text, &FormKind::Row(RowAt::row(0)))
                .0
                .fields
                .iter()
                .map(|f| f.key.clone())
                .collect()
        };
        let has = |keys: &[String], k: &str| keys.iter().any(|x| x == k);
        let plain = keys("[[row]]\nmodules = [\"clock\"]\n");
        assert!(!has(&plain, "blank") && has(&plain, "title"), "{plain:?}");
        assert!(has(&keys("[[row]]\nmodules = []\n"), "blank"));
        assert!(has(&keys("[[row]]\n[[row.col]]\nmodules = [\"clock\"]\n"), "blank"));
        assert!(has(&keys("[[row]]\nmodules = [\"clock\"]\nblank = true\n"), "blank"));
        let boxed = keys("[box.b]\n[[row]]\nmodules = [\"clock\"]\nbox = \"b\"\n");
        assert!(
            !has(&boxed, "title") && !has(&boxed, "title_pad") && has(&boxed, "box"),
            "{boxed:?}"
        );
        assert!(has(
            &keys("[box.b]\n[[row]]\nmodules = [\"clock\"]\nbox = \"b\"\ntitle = \"T\"\n"),
            "title"
        ));
        assert!(
            has(&keys("[[row]]\nmodules = [\"clock\"]\nbox = true\n"), "title"),
            "an unnamed box carries no title"
        );
        // A picker opens on the value in effect and its custom line starts
        // from it.
        let (mut form, _) =
            built("[modules.path]\nlabel = \"in\"\n", &FormKind::Module("path".into()));
        form.focus("label");
        let Some(Layer::Choose(c)) = form.handle(Key::Enter).push else { panic!("a picker") };
        assert_eq!(c.custom_start, "in");
        let (mut form, _) = built("", &FormKind::Top);
        form.focus("theme");
        let Some(Layer::Choose(c)) = form.handle(Key::Enter).push else { panic!("a picker") };
        assert_eq!(c.items.get(c.cursor).map(|i| i.value.as_str()), Some("garnish"));
        form.focus("preset");
        let Some(Layer::Choose(c)) = form.handle(Key::Enter).push else { panic!("a picker") };
        assert_eq!(c.items.get(c.cursor).map(|i| i.value.as_str()), Some("default"));
    }
}
