//! The overlay layers of `setup` (SPEC § 14).
//!
//! A list to choose from with a filter line, a one-line text input, a
//! yes/no question and the help page. Each turns keys into [`Action`]s for
//! the app to apply; none touches the draft itself.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::app::{Action, Key};
use super::ui::{Chrome, cells, centered, clip, hints, window};

/// What a chosen value is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Set a config key to the value.
    Slot(super::form::Slot, super::form::SlotKind),
    /// Add the module id at the builder's cursor.
    AddModule,
    /// Replace the draft with the named preset (the builder's `p`).
    Preset,
    /// Put the row, column or inner row at the path in the named box.
    BoxFor(super::draft::RowAt),
    /// Put the row at the path and the row above it in a new box of the
    /// typed name (the builder's `B`).
    BoxWith(super::draft::RowAt),
    /// Preview at the typed terminal width.
    Columns,
}

/// One entry of a [`Choose`] list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// What the list shows.
    pub label: Line<'static>,
    /// The text the filter matches and the value that is picked.
    pub value: String,
    /// Picking it opens an input for a typed value instead.
    pub custom: bool,
}

impl Choice {
    /// A plain entry whose label is its value.
    #[must_use]
    pub fn plain(value: &str) -> Self {
        Self { label: Line::from(value.to_owned()), value: value.to_owned(), custom: false }
    }

    /// An entry shown as `value  note`.
    #[must_use]
    pub fn noted(value: &str, note: &str) -> Self {
        Self {
            label: Line::from(vec![
                Span::raw(value.to_owned()),
                Span::raw("  "),
                Span::styled(note.to_owned(), Chrome::muted()),
            ]),
            value: value.to_owned(),
            custom: false,
        }
    }

    /// The `custom…` entry that opens an input line.
    #[must_use]
    pub fn custom(what: &str) -> Self {
        Self {
            label: Line::from(Span::styled(format!("{what}…"), Chrome::title())),
            value: String::new(),
            custom: true,
        }
    }
}

/// A list to pick one entry from, narrowed by what is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choose {
    /// The heading.
    pub title: String,
    /// Every entry, in the order given.
    pub items: Vec<Choice>,
    /// What is typed into the filter.
    pub filter: String,
    /// The cursor, as an index into the *matching* entries.
    pub cursor: usize,
    scroll: usize,
    /// What the pick is for.
    pub target: Target,
    /// The input line's title when `custom` is picked.
    pub custom_title: String,
    /// What the input line starts with when `custom` is picked: the value
    /// in effect, so a label is edited rather than retyped.
    pub custom_start: String,
}

impl Choose {
    /// A list for `target`, headed `title`.
    #[must_use]
    pub fn new(title: &str, items: Vec<Choice>, target: Target) -> Self {
        Self {
            title: title.to_owned(),
            items,
            filter: String::new(),
            cursor: 0,
            scroll: 0,
            target,
            custom_title: title.to_owned(),
            custom_start: String::new(),
        }
    }

    /// Put the cursor on the entry whose value is `value`, when the list
    /// has it, so the pick opens on what is in effect.
    pub fn select(&mut self, value: &str) {
        if let Some(i) = self.items.iter().position(|c| !c.custom && c.value == value) {
            self.cursor = i;
        }
    }

    /// The entries that match the filter, best first.
    #[must_use]
    pub fn matching(&self) -> Vec<&Choice> {
        if self.filter.is_empty() {
            return self.items.iter().collect();
        }
        let ranked = super::fuzzy::rank(&self.filter, self.items.iter().map(|c| c.value.as_str()));
        let mut out: Vec<&Choice> = ranked
            .iter()
            .filter_map(|v| self.items.iter().find(|c| c.value == *v && !c.custom))
            .collect();
        out.extend(self.items.iter().filter(|c| c.custom));
        out
    }

    /// Handle a key: the outcome says what to do.
    pub fn handle(&mut self, key: Key) -> Outcome {
        let n = self.matching().len();
        match key {
            Key::Esc => Outcome::close(),
            Key::Up => {
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
            Key::Backspace => {
                self.filter.pop();
                self.cursor = 0;
                Outcome::default()
            }
            Key::Char(c) if !c.is_control() => {
                self.filter.push(c);
                self.cursor = 0;
                Outcome::default()
            }
            Key::Enter => {
                let Some(choice) = self.matching().get(self.cursor).copied().cloned() else {
                    return Outcome::default();
                };
                if choice.custom {
                    return Outcome {
                        close: true,
                        push: Some(Layer::Input(InputBox::new(
                            &self.custom_title,
                            &self.custom_start,
                            self.target.clone(),
                        ))),
                        actions: Vec::new(),
                    };
                }
                Outcome {
                    close: true,
                    push: None,
                    actions: vec![Action::Picked(self.target.clone(), choice.value)],
                }
            }
            _ => Outcome::default(),
        }
    }

    /// Draw the list as a dialog over `area`.
    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let matching: Vec<Choice> = self.matching().into_iter().cloned().collect();
        let width = matching
            .iter()
            .map(|c| c.label.width())
            .max()
            .unwrap_or(20)
            .max(self.title.len().saturating_add(4))
            .saturating_add(6)
            .clamp(30, 70);
        let height = matching.len().saturating_add(5).clamp(7, usize::from(area.height));
        let rect = centered(area, cells(width), cells(height));
        frame.render_widget(Clear, rect);
        let block =
            Block::bordered().title(Span::styled(format!(" {} ", self.title), Chrome::title()));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let list_height = usize::from(inner.height.saturating_sub(2));
        self.scroll = window(self.cursor, matching.len(), list_height, self.scroll);
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, c) in matching.iter().enumerate().skip(self.scroll).take(list_height) {
            let mut line = c.label.clone();
            if i == self.cursor {
                line = line.style(Chrome::selected());
            }
            lines.push(line);
        }
        while lines.len() < list_height {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(vec![
            Span::styled("type to filter: ", Chrome::muted()),
            Span::raw(self.filter.clone()),
            Span::styled("▏", Chrome::muted()),
        ]));
        lines.push(hints(&[("↑↓", "move"), ("enter", "pick"), ("esc", "back")]));
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// A one-line text input with a cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputBox {
    /// The heading.
    pub title: String,
    /// The text so far.
    pub text: String,
    /// The cursor, as a character index into `text` (at the end to start,
    /// so typing appends to the value the box opened with).
    pub cursor: usize,
    /// What the text is for.
    pub target: Target,
}

impl InputBox {
    /// An input for `target`, starting with `text`.
    #[must_use]
    pub fn new(title: &str, text: &str, target: Target) -> Self {
        Self {
            title: title.to_owned(),
            text: text.to_owned(),
            cursor: text.chars().count(),
            target,
        }
    }

    /// The byte offset of the cursor.
    fn at(&self) -> usize {
        self.text.char_indices().nth(self.cursor).map_or(self.text.len(), |(b, _)| b)
    }

    /// Handle a key.
    pub fn handle(&mut self, key: Key) -> Outcome {
        let len = self.text.chars().count();
        match key {
            Key::Esc => Outcome::close(),
            Key::Enter => Outcome {
                close: true,
                push: None,
                actions: vec![Action::Typed(self.target.clone(), self.text.clone())],
            },
            Key::Backspace => {
                if let Some(prev) = self.cursor.checked_sub(1) {
                    self.cursor = prev;
                    let at = self.at();
                    self.text.remove(at);
                }
                Outcome::default()
            }
            Key::Delete => {
                if self.cursor < len {
                    let at = self.at();
                    self.text.remove(at);
                }
                Outcome::default()
            }
            Key::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                Outcome::default()
            }
            Key::Right => {
                self.cursor = self.cursor.saturating_add(1).min(len);
                Outcome::default()
            }
            Key::Home => {
                self.cursor = 0;
                Outcome::default()
            }
            Key::End => {
                self.cursor = len;
                Outcome::default()
            }
            Key::Ctrl('u') => {
                self.text.clear();
                self.cursor = 0;
                Outcome::default()
            }
            Key::Char(c) if !c.is_control() => {
                let at = self.at();
                self.text.insert(at, c);
                self.cursor = self.cursor.saturating_add(1);
                Outcome::default()
            }
            _ => Outcome::default(),
        }
    }

    /// Draw the input as a small dialog over `area`, the cursor drawn as a
    /// thin bar where the next character goes.
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        let width = self
            .text
            .chars()
            .count()
            .max(self.title.chars().count())
            .saturating_add(8)
            .clamp(34, 70);
        let rect = centered(area, cells(width), 5);
        frame.render_widget(Clear, rect);
        let block =
            Block::bordered().title(Span::styled(format!(" {} ", self.title), Chrome::title()));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let room = usize::from(inner.width).saturating_sub(2);
        let at = self.at();
        let (before, after) = self.text.split_at_checked(at).unwrap_or((&self.text, ""));
        // The text before the cursor keeps its tail (that is where typing
        // lands); what follows takes the rest of the room.
        let before = tail(before, room);
        let after = clip(after, room.saturating_sub(crate::ansi::display_width(&before)));
        let lines = vec![
            Line::from(vec![
                Span::raw(before),
                Span::styled("▏", Chrome::muted()),
                Span::raw(after),
            ]),
            Line::from(""),
            hints(&[("enter", "apply"), ("esc", "cancel"), ("←→", "move"), ("ctrl-u", "clear")]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// The last `width` cells of `text`, with an ellipsis in front when it was
/// cut.
fn tail(text: &str, width: usize) -> String {
    if crate::ansi::display_width(text) <= width {
        return text.to_owned();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1_usize;
    for c in text.chars().rev() {
        let w = crate::ansi::display_width(&c.to_string());
        if used.saturating_add(w) > width {
            break;
        }
        used = used.saturating_add(w);
        kept.push(c);
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

/// What a yes/no question is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Quit with unsaved edits.
    QuitUnsaved,
    /// The file changed on disk since it was read: `true` overwrites,
    /// `false` reloads.
    OverwriteOrReload,
    /// Drop the `[modules.text.<name>]` table whose last placement went.
    DropText(String),
    /// Replace a draft with unsaved edits by the named preset.
    ReplaceDraft(String),
    /// Write the picker's preset over a config file that appeared or
    /// changed since `setup` opened.
    ApplyPreset,
    /// Apply the install plan.
    Install,
}

/// A yes/no question, with a third answer that does nothing when both of
/// the others act.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    /// The question, one or two lines.
    pub text: Vec<String>,
    /// The two answers, `yes` first.
    pub answers: (String, String),
    /// The answer that closes the question and does nothing, when it has
    /// one: then it is the default, and what `Esc` means.
    pub cancel: Option<String>,
    /// The highlighted answer: 0 is `yes`, 1 `no`, 2 the cancel.
    pub focus: usize,
    /// What a `yes` (or `no`) does.
    pub question: Question,
}

impl Confirm {
    /// A question with `yes`/`no` answers, opening on `no`.
    #[must_use]
    pub fn new(question: Question, text: &[&str], yes: &str, no: &str) -> Self {
        Self {
            text: text.iter().map(|s| (*s).to_owned()).collect(),
            answers: (yes.to_owned(), no.to_owned()),
            cancel: None,
            focus: 1,
            question,
        }
    }

    /// With a third answer that does nothing, the one it opens on: for a
    /// question whose `yes` and `no` both act, so that neither `Esc` nor a
    /// reflexive `Enter` does either.
    #[must_use]
    pub fn or_cancel(mut self, label: &str) -> Self {
        self.cancel = Some(label.to_owned());
        self.focus = 2;
        self
    }

    const fn answer_count(&self) -> usize {
        if self.cancel.is_some() { 3 } else { 2 }
    }

    /// Handle a key.
    pub fn handle(&mut self, key: Key) -> Outcome {
        let answer = |yes: bool| Outcome {
            close: true,
            push: None,
            actions: vec![Action::Answered(self.question.clone(), yes)],
        };
        let n = self.answer_count();
        match key {
            Key::Esc if self.cancel.is_some() => Outcome::close(),
            Key::Esc | Key::Char('n') => answer(false),
            Key::Char('y') => answer(true),
            Key::Left | Key::Up | Key::BackTab => {
                self.focus = self.focus.checked_sub(1).unwrap_or_else(|| n.saturating_sub(1));
                Outcome::default()
            }
            Key::Right | Key::Down | Key::Tab => {
                self.focus = self.focus.saturating_add(1).checked_rem(n).unwrap_or(0);
                Outcome::default()
            }
            Key::Enter => match self.focus {
                0 => answer(true),
                1 => answer(false),
                _ => Outcome::close(),
            },
            _ => Outcome::default(),
        }
    }

    /// Draw the question as a dialog over `area`.
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        let pick = |on: bool, text: &str| {
            Span::styled(format!(" {text} "), if on { Chrome::selected() } else { Style::new() })
        };
        let mut answers = vec![
            pick(self.focus == 0, &self.answers.0),
            Span::raw("   "),
            pick(self.focus == 1, &self.answers.1),
        ];
        if let Some(cancel) = &self.cancel {
            answers.push(Span::raw("   "));
            answers.push(pick(self.focus == 2, cancel));
        }
        answers.push(Span::raw("     "));
        let keys = if self.cancel.is_some() { "y / n / esc" } else { "y / n / enter" };
        answers.push(Span::styled(keys, Chrome::muted()));
        let answers = Line::from(answers);
        let text_width = self.text.iter().map(|t| crate::ansi::display_width(t)).max().unwrap_or(0);
        let width =
            text_width.saturating_add(6).max(answers.width().saturating_add(2)).clamp(36, 72);
        let height = self.text.len().saturating_add(4);
        let rect = centered(area, cells(width), cells(height));
        frame.render_widget(Clear, rect);
        let block = Block::bordered();
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let mut lines: Vec<Line<'static>> =
            self.text.iter().map(|t| Line::from(t.clone())).collect();
        lines.push(Line::from(""));
        lines.push(answers);
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// The help page: every key of the screen it was opened from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    /// The heading.
    pub title: String,
    /// `key`, `meaning` pairs.
    pub keys: Vec<(String, String)>,
}

impl Help {
    /// Handle a key: any key closes the page.
    #[must_use]
    pub const fn handle(&self, _key: Key) -> Outcome {
        Outcome::close()
    }

    /// Draw the page over `area`.
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        // The keys sit in a twelve-cell column, so the box is that plus the
        // widest meaning, and the closing hint is dropped before a key is,
        // on a terminal too short for the whole page.
        let key_w = 12_usize;
        let width = self
            .keys
            .iter()
            .map(|(k, w)| k.chars().count().max(key_w).saturating_add(w.chars().count()))
            .max()
            .unwrap_or(20)
            .saturating_add(4)
            .clamp(40, 100);
        let hint = usize::from(area.height) >= self.keys.len().saturating_add(4);
        let height = self.keys.len().saturating_add(if hint { 4 } else { 2 });
        let rect = centered(area, cells(width), cells(height));
        frame.render_widget(Clear, rect);
        let block =
            Block::bordered().title(Span::styled(format!(" {} ", self.title), Chrome::title()));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let mut lines: Vec<Line<'static>> = self
            .keys
            .iter()
            .map(|(k, w)| {
                Line::from(vec![
                    Span::styled(format!("{k:<key_w$}"), Chrome::key()),
                    Span::raw(w.clone()),
                ])
            })
            .collect();
        if hint {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("any key closes this page", Chrome::muted())));
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// An overlay over a screen (SPEC § 14: `Esc` closes the innermost one).
#[derive(Debug, Clone, PartialEq)]
pub enum Layer {
    /// A form of fields.
    Form(super::form::Form),
    /// A list to pick from.
    Choose(Choose),
    /// A line of text.
    Input(InputBox),
    /// A yes/no question.
    Confirm(Confirm),
    /// The help page.
    Help(Help),
}

/// What a layer asks the app to do after a key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// Close this layer.
    pub close: bool,
    /// Open this layer on top.
    pub push: Option<Layer>,
    /// Apply these, in order.
    pub actions: Vec<Action>,
}

impl Outcome {
    /// Close the layer and nothing else.
    #[must_use]
    pub const fn close() -> Self {
        Self { close: true, push: None, actions: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_filters_wraps_and_picks_or_opens_the_custom_input() {
        let items =
            vec![Choice::plain("sync"), Choice::plain("session_name"), Choice::custom("custom")];
        let mut c = Choose::new("Module", items, Target::AddModule);
        assert_eq!(c.matching().len(), 3);
        c.handle(Key::Up);
        assert_eq!(c.cursor, 2, "wraps");
        c.handle(Key::Down);
        assert_eq!(c.cursor, 0);
        for ch in "sn".chars() {
            c.handle(Key::Char(ch));
        }
        let names: Vec<&str> = c.matching().iter().map(|c| c.value.as_str()).collect();
        assert_eq!(
            names,
            vec!["session_name", "sync", ""],
            "the initialism outranks the subsequence; the custom entry stays last"
        );
        let out = c.handle(Key::Enter);
        assert!(out.close);
        assert_eq!(out.actions, vec![Action::Picked(Target::AddModule, "session_name".into())]);
        c.handle(Key::Backspace);
        c.handle(Key::Backspace);
        c.handle(Key::End);
        let out = c.handle(Key::Enter);
        assert!(matches!(out.push, Some(Layer::Input(_))), "custom opens an input");
        assert!(c.handle(Key::Esc).close);
    }

    #[test]
    fn inputs_and_questions_report_what_was_typed_or_answered() {
        let mut i = InputBox::new("Title", "ab", Target::Columns);
        i.handle(Key::Char('c'));
        i.handle(Key::Backspace);
        let out = i.handle(Key::Enter);
        assert_eq!(out.actions, vec![Action::Typed(Target::Columns, "ab".into())]);
        // The cursor moves: an insertion in the middle, a delete under it,
        // a backspace before it, and home/end.
        i.handle(Key::Left);
        i.handle(Key::Char('x'));
        assert_eq!((i.text.as_str(), i.cursor), ("axb", 2));
        i.handle(Key::Delete);
        assert_eq!(i.text, "ax");
        i.handle(Key::Home);
        i.handle(Key::Backspace);
        assert_eq!((i.text.as_str(), i.cursor), ("ax", 0), "nothing before the start");
        i.handle(Key::Delete);
        assert_eq!(i.text, "x");
        i.handle(Key::End);
        i.handle(Key::Char('é'));
        i.handle(Key::Left);
        i.handle(Key::Left);
        i.handle(Key::Char('-'));
        assert_eq!(i.text, "-xé", "the cursor counts characters, not bytes");
        i.handle(Key::Ctrl('u'));
        assert_eq!((i.text.as_str(), i.cursor), ("", 0));
        assert_eq!(tail("hello world", 6), "…world");
        assert_eq!(tail("hi", 6), "hi");
        let mut q = Confirm::new(Question::QuitUnsaved, &["Quit?"], "quit", "stay");
        assert_eq!(
            q.handle(Key::Char('y')).actions,
            vec![Action::Answered(Question::QuitUnsaved, true)]
        );
        q.handle(Key::Tab);
        assert_eq!(
            q.handle(Key::Enter).actions,
            vec![Action::Answered(Question::QuitUnsaved, true)]
        );
        assert_eq!(
            q.handle(Key::Esc).actions,
            vec![Action::Answered(Question::QuitUnsaved, false)]
        );
        // app-06: a question whose two answers both act opens on the third,
        // which does nothing, as `Esc` does; each act needs its own key.
        let mut c = Confirm::new(Question::OverwriteOrReload, &["Changed?"], "overwrite", "reload")
            .or_cancel("keep editing");
        assert_eq!(c.handle(Key::Enter), Outcome::close());
        assert_eq!(c.handle(Key::Esc), Outcome::close());
        c.handle(Key::Left);
        assert_eq!(
            c.handle(Key::Enter).actions,
            vec![Action::Answered(Question::OverwriteOrReload, false)]
        );
        c.handle(Key::Right);
        assert_eq!(c.focus, 2);
        c.handle(Key::Right);
        assert_eq!(c.focus, 0, "wraps");
        assert_eq!(
            c.handle(Key::Char('y')).actions,
            vec![Action::Answered(Question::OverwriteOrReload, true)]
        );
        let h = Help { title: "Keys".into(), keys: vec![("q".into(), "quit".into())] };
        assert!(h.handle(Key::Char('x')).close);
    }
}
