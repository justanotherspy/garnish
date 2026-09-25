//! The install screen (SPEC § 14): `install --dry-run` on screen, applied
//! after one question, through the same code as `garnish install`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{App, Key, Screen};
use crate::install::{Applied, ConfigStep, Refusal, Steps};
use crate::setup::pick::{Confirm, Layer, Question};
use crate::setup::ui::Chrome;

/// The install screen: the plan, and what applying it did.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct InstallScreen {
    steps: Result<Steps, Refusal>,
    applied: Option<Result<Applied, Refusal>>,
    /// Where `Esc` goes back to.
    back: Back,
}

/// The screen the install screen was opened from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Back {
    Home,
    Builder,
}

impl App {
    pub(super) fn open_install(&mut self, back: Back) {
        let steps = Steps::plan(&self.options);
        self.screen = Screen::Install(Box::new(InstallScreen { steps, applied: None, back }));
    }

    pub(super) fn install_key(&mut self, key: Key) {
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

    pub(super) fn install_apply(&mut self) {
        let Screen::Install(screen) = &mut self.screen else { return };
        if let Ok(steps) = &screen.steps {
            screen.applied = Some(steps.apply());
        }
    }

    pub(super) fn draw_install(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Screen::Install(screen) = &self.screen else { return };
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
                    ConfigStep::Skipped => {}
                    ConfigStep::Exists { path, .. } => {
                        lines.push(Line::from(format!(
                            "config          {} (kept)",
                            self.shown(path)
                        )));
                    }
                    ConfigStep::Write { path, .. } => {
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
