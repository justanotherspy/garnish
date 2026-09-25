//! The builder's undo and redo history (SPEC § 14): every input that
//! changed the draft's table keeps the table it replaced, so an edit path
//! is undoable by construction.

use super::{App, Key, Level, Screen};
use crate::setup::form::FormKind;
use crate::setup::pick::Layer;

/// How many edits `u` can take back.
const HISTORY_LIMIT: usize = 100;

/// The draft before an edit, with the list cursor of the time and the
/// status line the edit produced (what `u` says it undid).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Snapshot {
    table: toml::Table,
    cursor: usize,
    chip: Option<usize>,
    what: String,
}

/// The undo and redo stacks: every input that changed the draft's table
/// pushes the table it replaced.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl App {
    /// Run one input's handling, keeping the table it replaced on the undo
    /// stack when it was a builder input that changed the draft.
    pub(super) fn bracketed(&mut self, handle: impl FnOnce(&mut Self)) {
        let before = (self.screen == Screen::Builder).then(|| self.snapshot());
        handle(self);
        if let Some(before) = before {
            self.remember(before);
        }
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
    pub(super) fn is_undo_key(&self, key: Key) -> bool {
        let over_form = matches!(self.layers.last(), None | Some(Layer::Form(_)));
        let builder = self.screen == Screen::Builder;
        match key {
            Key::Ctrl('z' | 'r') => builder && over_form,
            Key::Char('u' | 'U') => builder && self.layers.is_empty(),
            _ => false,
        }
    }

    pub(super) fn undo_key(&mut self, key: Key) {
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

    /// A fresh start (a preset adopted from the picker): nothing to undo.
    pub(super) fn forget_history(&mut self) {
        self.history = History::default();
    }
}
