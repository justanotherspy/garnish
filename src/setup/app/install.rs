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
                // Paths under the home read as `~/…`, as the rest of the
                // screen shows them, where the shell would expand one.
                let home = self.home.as_deref().map(|h| format!("{}/", h.display()));
                let command = home
                    .as_deref()
                    .map_or_else(|| steps.command.clone(), |h| tilde_paths(&steps.command, h));
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
                    | ConfigStep::Unresolved { .. }
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
                    let note =
                        home.as_deref().map_or_else(|| note.clone(), |h| tilde_paths(&note, h));
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

/// `text` with every path that starts with `home` (a directory ending in
/// `/`) shown as `~/…`: only where a path starts, at the start of the text
/// or after a blank, never inside a longer path, and never inside quotes,
/// where a `~` pasted into a shell names no home (verification of
/// 2026-09-26: the pasted advice made a `~` directory, and the install
/// screen's command line put a `~` inside quotes and inside a longer path).
fn tilde_paths(text: &str, home: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut starts_path = true;
    let mut quote = None;
    while !rest.is_empty() {
        if starts_path && let Some(after) = rest.strip_prefix(home) {
            out.push_str("~/");
            rest = after;
            starts_path = false;
            continue;
        }
        let mut chars = rest.chars();
        let Some(c) = chars.next() else { break };
        out.push(c);
        rest = chars.as_str();
        quote = match (quote, c) {
            (None, '\'' | '"') => Some(c),
            (Some(q), c) if c == q => None,
            (quote, _) => quote,
        };
        starts_path = quote.is_none() && c == ' ';
    }
    out
}

#[cfg(test)]
mod tests {
    use super::tilde_paths;

    /// Verification of 2026-09-26: the home is shortened only where a path
    /// starts, so a path that merely contains it keeps its name, and never
    /// in quoted advice meant to be pasted.
    #[test]
    fn a_note_shows_only_paths_under_the_home_with_a_tilde() {
        let home = "/home/u/";
        let note = "note: reads /home/u/a.toml, not /mnt/snap/home/u/g.toml; `garnish --config '/home/u/b' install`";
        assert_eq!(
            tilde_paths(note, home),
            "note: reads ~/a.toml, not /mnt/snap/home/u/g.toml; `garnish --config '/home/u/b' install`"
        );
        assert_eq!(tilde_paths("/home/u/x", home), "~/x");
        assert_eq!(tilde_paths("/home/user/x", home), "/home/user/x");
        // A quoted path keeps its home whatever blanks it holds; the words
        // after the quotes close are paths again.
        for quoted in ["'/home/u/My /home/u/g.toml'", "\"/home/u/My /home/u/g.toml\""] {
            let command = format!("garnish --config {quoted} /home/u/x");
            let shown = format!("garnish --config {quoted} ~/x");
            assert_eq!(tilde_paths(&command, home), shown);
        }
        assert_eq!(
            tilde_paths("garnish --config=/home/u/g.toml", home),
            "garnish --config=/home/u/g.toml"
        );
        assert_eq!(
            tilde_paths("garnish --config /mnt/snap/home/u/g.toml", home),
            "garnish --config /mnt/snap/home/u/g.toml"
        );
    }
}
