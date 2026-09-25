//! What the screen does with the [`Action`]s an overlay asks for: set or
//! unset a key (tried on a copy first), a pick or a typed value for a
//! target, a question answered; and the builder edit every such change to
//! the rows goes through.

use toml::Value;

use super::{Action, App, Level, new_problem};
use crate::config::presets::TopPreset;
use crate::config::{self, is_bare_key};
use crate::setup::builder::Builder;
use crate::setup::draft::{Draft, dropped_boxes};
use crate::setup::form::{FormKind, Slot, SlotKind};
use crate::setup::pick::{Confirm, Layer, Question, Target};

/// Why a box name was refused (SPEC § 4.3): the parser's own rule.
const BOX_NAME_RULE: &str = "a box name is letters, digits, _ and - only";

/// What else an edit did, as `; <note>` for each, for its status line.
fn notes<const N: usize>(notes: [Option<String>; N]) -> String {
    notes.into_iter().flatten().fold(String::new(), |mut s, n| {
        s.push_str("; ");
        s.push_str(&n);
        s
    })
}

impl App {
    pub(super) fn apply(&mut self, action: Action) {
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
                // An unset `preset` is the default one, and the rows follow
                // it as they follow a preset set.
                let default = Value::String(TopPreset::Default.name().to_owned());
                let swapped = self.swap_preset_rows(&slot, &default);
                slot.unset(&mut self.draft);
                // The last member leaving a box takes an unused
                // `[box.<name>]` with it, as the builder's `b` does.
                let dropped = if slot.key == "box" { self.prune_orphan_boxes() } else { None };
                let note = notes([swapped, dropped]);
                let path = slot.path();
                match self.refresh() {
                    Some(problem) => {
                        self.say(format!("{path} unset{note}; ⚠ {problem}"), Level::Warn);
                    }
                    None => self.say(format!("{path} unset{note}"), Level::Info),
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
        let note = notes([swapped, dropped]);
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
                        self.say(BOX_NAME_RULE.into(), Level::Error);
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
            Target::Columns => {
                let typed = value.trim();
                match if typed.is_empty() { Ok(0) } else { typed.parse::<usize>() } {
                    Ok(0) => {
                        self.preview.columns = None;
                        self.say("previewing at the terminal's own width".into(), Level::Info);
                    }
                    Ok(n) => {
                        // The terminal around the widest box garnish renders.
                        let widest = config::MAX_WIDTH.saturating_add(config::HARNESS_PADDING);
                        let n = n.clamp(config::MIN_WIDTH, widest);
                        self.preview.columns = Some(n);
                        self.say(format!("previewing at {n} columns"), Level::Info);
                    }
                    Err(_) => self.say(format!("{value:?} is not a width"), Level::Error),
                }
            }
            // A pick or a typed name reads as the form's `box` field reads
            // it: `none`, `false` and nothing unbox, `true` is a box of its
            // own. It was asked for one line, as `B`'s name is, and goes on
            // that line or nowhere.
            Target::BoxFor(at) => {
                if self.builder.item().map(|i| i.at) != Some(at) {
                    self.say("the selection moved; press b again".into(), Level::Warn);
                    return;
                }
                match SlotKind::BoxRef.parse(value) {
                    Ok(Some(Value::String(name))) if !is_bare_key(&name) => {
                        self.say(BOX_NAME_RULE.into(), Level::Error);
                    }
                    Ok(v) => {
                        let out = self.edit(|builder, draft| builder.set_box(draft, v));
                        self.report(out);
                    }
                    Err(e) => self.say(e, Level::Error),
                }
            }
            Target::BoxWith(at) => {
                let name = value.trim();
                if !is_bare_key(name) {
                    self.say(BOX_NAME_RULE.into(), Level::Error);
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

    /// Show what a builder edit did: its line, and the first problem it
    /// left, if any; or why it did nothing.
    pub(super) fn report(&mut self, out: Result<String, String>) {
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
    /// validated as `config check` would; a problem the file already had
    /// is not new because the edit renumbered it); otherwise the draft and
    /// the list are put back and the parser's message is the error.
    pub(super) fn edit(
        &mut self,
        f: impl FnOnce(&mut Builder, &mut Draft) -> Result<String, String>,
    ) -> Result<String, String> {
        let before = (self.draft.clone(), self.builder.clone());
        let result = f(&mut self.builder, &mut self.draft).and_then(|out| {
            let (_, problems) = self.draft.resolved();
            new_problem(&self.problems, &problems)
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
                    let (line, level) = self.opening_note().map_or_else(
                        || {
                            (
                                "reloaded from disk; the edits were dropped (u takes them back)"
                                    .to_owned(),
                                Level::Info,
                            )
                        },
                        |(note, level)| {
                            (format!("reloaded (u takes the edits back): {note}"), level)
                        },
                    );
                    self.say(line, level);
                }
            }
            Question::DropText(names) => {
                if yes {
                    for name in &names {
                        self.draft.remove(&["modules", "text", name]);
                    }
                    self.refresh();
                    let tables: Vec<String> =
                        names.iter().map(|n| format!("[modules.text.{n}]")).collect();
                    self.say(format!("dropped {}", tables.join(", ")), Level::Info);
                }
            }
            Question::ReplaceDraft(name) => {
                if yes {
                    self.load_preset(&name);
                }
            }
            Question::ApplyPreset => {
                if yes {
                    self.picker_apply(true);
                }
            }
            Question::Install => {
                if yes {
                    self.install_apply();
                }
            }
        }
    }
}
