//! The preset picker (SPEC § 14): the built-in and gallery presets, the
//! highlighted one previewed live, `Enter` to write it, `e` to edit it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::install::Back;
use super::{App, Key, Level, Screen};
use crate::config::presets::TopPreset;
use crate::config::{Config, ConfigError};
use crate::install::Steps;
use crate::setup::draft::Draft;
use crate::setup::pick::{Confirm, Layer, Question};
use crate::setup::ui::{Chrome, cells, window};

/// The preset picker.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Picker {
    pub(super) items: Vec<PickItem>,
    cursor: usize,
    scroll: usize,
    /// The highlighted preset resolved, so a draw does not parse it again.
    shown: Option<(usize, Draft, Config, Vec<ConfigError>)>,
}

/// One preset of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PickItem {
    pub(super) name: String,
    pub(super) summary: String,
    columns: Option<usize>,
    needs: Option<String>,
}

impl Picker {
    /// The four built-in presets, then the gallery's.
    pub(super) fn new() -> Self {
        let mut items: Vec<PickItem> = TopPreset::ALL
            .iter()
            .map(|p| PickItem {
                name: p.name().to_owned(),
                summary: match p {
                    TopPreset::Default => "four lines, every module at its default".to_owned(),
                    TopPreset::Minimal => "one unframed line, the bare values".to_owned(),
                    TopPreset::Full => "four lines, everything each module knows".to_owned(),
                    TopPreset::Compact => "two lines".to_owned(),
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

impl App {
    pub(super) fn picker_key(&mut self, key: Key) {
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
            Key::Enter => self.picker_apply(false),
            _ => {}
        }
    }

    /// A click on line `line` of the list: highlights that preset, or
    /// applies it when it already is.
    pub(super) fn picker_click(&mut self, line: usize) {
        let Screen::Picker(picker) = &mut self.screen else { return };
        let at = line.saturating_add(picker.scroll);
        if at < picker.items.len() {
            if picker.cursor == at {
                self.picker_apply(false);
            } else {
                picker.cursor = at;
            }
        }
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
        self.forget_history();
        self.refresh();
        self.screen = Screen::Builder;
        self.say("editing the preset; s saves it to the config file".into(), Level::Info);
    }

    /// `Enter`: the highlighted preset becomes the config file, with the
    /// previous file kept as a backup; the install screen follows when the
    /// settings file has no status line yet. A file that appeared or
    /// changed since setup opened is replaced only once `force` says the
    /// question was answered.
    pub(super) fn picker_apply(&mut self, force: bool) {
        if !force && self.draft.changed_on_disk() {
            self.layers.push(Layer::Confirm(Confirm::new(
                Question::ApplyPreset,
                &[
                    "The config file changed on disk since setup opened.",
                    "Replace it with the preset (a backup is kept)?",
                ],
                "replace",
                "keep it",
            )));
            return;
        }
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
                self.forget_history();
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

    pub(super) fn draw_picker(&mut self, frame: &mut Frame<'_>, area: Rect) {
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
        info_lines.extend(crate::setup::ui::note_lines(&notes, area.width));
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
                let text = crate::setup::ui::clip(&format!(" {:<24} {}", p.name, p.summary), width);
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
}
