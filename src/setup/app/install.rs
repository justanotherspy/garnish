//! The install screen (SPEC § 14): `install --dry-run` on screen, applied
//! after one question, through the same code as `garnish install`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{App, Key, Screen};
use crate::install::{ConfigStep, Refusal, Steps};
use crate::setup::pick::{Confirm, Layer, Question};
use crate::setup::ui::Chrome;

/// The install screen: the plan, and what applying it did.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct InstallScreen {
    steps: Result<Steps, Refusal>,
    /// What applying the plan wrote, one line each, or why it could not.
    applied: Option<Result<Vec<String>, Refusal>>,
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

    /// Apply the plan, made again first: the screen may have been open for
    /// a while, and a plan merged from the text read when it opened would
    /// drop what another program wrote since (`/voice` in Claude Code
    /// writes `voice.enabled` to the user file) and back up nothing if the
    /// file was created meanwhile. The screen then shows the plan applied.
    pub(super) fn install_apply(&mut self) {
        let Screen::Install(screen) = &mut self.screen else { return };
        let steps = Steps::plan(&self.options);
        screen.applied = steps.as_ref().ok().map(Steps::apply);
        screen.steps = steps;
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
                // The command may name a config path under the home, which
                // is shown as `~` like every path here.
                let command = self.home.as_deref().map_or_else(
                    || steps.command.clone(),
                    |home| {
                        let prefix = format!("{}/", home.display());
                        steps.command.replace(&prefix, "~/")
                    },
                );
                lines.push(Line::from(format!(
                    "statusLine      {{ \"type\": \"command\", \"command\": {command:?}, \"refreshInterval\": {}{} }}",
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
                    ConfigStep::Skipped
                    | ConfigStep::Unresolved(_)
                    | ConfigStep::Checkout(_)
                    | ConfigStep::Elsewhere { .. } => {}
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
                        for l in applied {
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
