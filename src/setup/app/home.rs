//! The home menu (SPEC § 14): where `setup` opens when there is no config
//! file yet.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::install::Back;
use super::picker::Picker;
use super::{App, Key, Screen};
use crate::setup::ui::{Chrome, cells};

/// The menu, in order; `1` to `4` pick an entry by its place.
const HOME_ITEMS: [&str; 4] =
    ["Pick a preset", "Build a custom layout", "Install into Claude Code", "Quit"];

/// The entry a digit key names (`1` the first), when there is one.
fn home_digit(c: char) -> Option<usize> {
    let i = usize::try_from(c.to_digit(10)?).ok()?.checked_sub(1)?;
    (i < HOME_ITEMS.len()).then_some(i)
}

impl App {
    pub(super) fn home_key(&mut self, key: Key) {
        if let Key::Char(c) = key
            && let Some(i) = home_digit(c)
        {
            self.home_cursor = i;
            self.home_enter();
            return;
        }
        match key {
            Key::Up | Key::Char('k') => {
                self.home_cursor =
                    self.home_cursor.checked_sub(1).unwrap_or(HOME_ITEMS.len().saturating_sub(1));
            }
            Key::Down | Key::Char('j') | Key::Tab => {
                self.home_cursor =
                    self.home_cursor.saturating_add(1).checked_rem(HOME_ITEMS.len()).unwrap_or(0);
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

    /// A click on line `line` of the menu.
    pub(super) fn home_click(&mut self, line: usize) {
        if line < HOME_ITEMS.len() {
            self.home_cursor = line;
            self.home_enter();
        }
    }

    pub(super) fn draw_home(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let config = self.config.clone();
        let pane = self.draw_pane(
            frame,
            area,
            &config,
            None,
            None,
            area.height.checked_div(2).unwrap_or(1),
        );
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
        for (i, item) in HOME_ITEMS.iter().enumerate() {
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
        self.list_area = Rect { y: list_y, height: cells(HOME_ITEMS.len()), ..area };
        self.draw_status(
            frame,
            area,
            &[("↑↓", "choose"), ("enter", "open"), ("q", "quit"), ("?", "help")],
        );
    }
}
