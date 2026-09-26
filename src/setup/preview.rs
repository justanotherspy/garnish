//! The preview pane's model (SPEC § 14): which embedded payload is shown,
//! at what width, on which clock, and the placement map of the last render.

use std::ops::Range;

use crate::config::Config;
use crate::fixtures::FIXTURES;
use crate::layout::{Elem, Line};
use crate::payload::Payload;
use crate::render::{Clock, render_tree_at};

/// One rendered terminal line with what sits in each of its cells.
#[derive(Debug, Clone)]
pub struct Placed {
    /// The index of the `[[row]]` the line belongs to.
    pub row: usize,
    /// The line, as the painter draws it.
    pub line: Line,
    /// Each piece's kind and cells.
    pub spans: Vec<(Elem, Range<usize>)>,
    /// Each module's id and cells (a module may own several runs).
    pub modules: Vec<(String, Range<usize>)>,
}

/// A rendered preview: the lines and the width they were laid out to.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    /// The lines, top to bottom.
    pub lines: Vec<Placed>,
    /// The width of Claude Code's box the lines fill (`columns − 4 − padding`).
    pub width: usize,
    /// The terminal width the box was derived from.
    pub columns: usize,
}

impl Rendered {
    /// What sits at cell `x` of line `y`: the module id when a module owns
    /// the cell, else the piece's kind.
    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> Option<Hit> {
        let placed = self.lines.get(y)?;
        if let Some((id, _)) = placed.modules.iter().find(|(_, r)| r.contains(&x)) {
            return Some(Hit { row: placed.row, elem: Elem::Module(id.clone()) });
        }
        let (elem, _) = placed.spans.iter().find(|(_, r)| r.contains(&x))?;
        Some(Hit { row: placed.row, elem: elem.clone() })
    }
}

/// What a click in the preview landed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The `[[row]]` the line belongs to.
    pub row: usize,
    /// The piece under the cell.
    pub elem: Elem,
}

/// The pane's inputs.
#[derive(Debug, Clone)]
pub struct Preview {
    payloads: Vec<Payload>,
    /// Index into [`FIXTURES`] of the payload on show.
    pub fixture: usize,
    /// A terminal width other than the real one (`w`), when set.
    pub columns: Option<usize>,
    /// The clock the pane renders on.
    pub clock: Clock,
    /// The clock never advances (tests and goldens).
    pub pinned: bool,
}

impl Preview {
    /// A pane on `clock`, showing the first fixture.
    #[must_use]
    pub fn new(clock: Clock, pinned: bool) -> Self {
        let payloads =
            FIXTURES.iter().map(|f| Payload::parse(f.text).unwrap_or_default()).collect();
        Self { payloads, fixture: 0, columns: None, clock, pinned }
    }

    /// The live pane: the process clock and no cache or workers, like
    /// `garnish preview`, but also no git discovery and no settings, since
    /// the bundled fixtures name no real directory (SPEC § 14).
    #[must_use]
    pub fn live() -> Self {
        let clock = Clock {
            git: false,
            settings: false,
            managed: None,
            workers: false,
            ..Clock::from_env()
        };
        Self::new(clock, false)
    }

    /// Advance the clock to now, unless pinned; the animations move with it.
    pub fn tick(&mut self) {
        if !self.pinned {
            self.clock.now = crate::time::now();
        }
    }

    /// The fixture on show.
    #[must_use]
    pub fn fixture_name(&self) -> &'static str {
        FIXTURES.get(self.fixture).map_or("", |f| f.name)
    }

    /// What the fixture shows.
    #[must_use]
    pub fn fixture_summary(&self) -> &'static str {
        FIXTURES.get(self.fixture).map_or("", |f| f.summary)
    }

    /// Show the next (or previous) fixture, wrapping around.
    pub fn cycle(&mut self, forward: bool) {
        let n = FIXTURES.len().max(1);
        self.fixture = if forward {
            self.fixture.saturating_add(1).checked_rem(n).unwrap_or(0)
        } else {
            self.fixture.checked_sub(1).unwrap_or_else(|| n.saturating_sub(1))
        };
    }

    /// Show the fixture called `name`, when there is one (tests; the screen
    /// steps with `f` and `F`).
    #[cfg(test)]
    pub fn show(&mut self, name: &str) {
        if let Some(i) = FIXTURES.iter().position(|f| f.name == name) {
            self.fixture = i;
        }
    }

    /// Render `config` for the terminal width in effect: the `w` override,
    /// else `terminal_columns`.
    #[must_use]
    pub fn render(&self, config: &Config, terminal_columns: usize) -> Rendered {
        let columns = self.columns.unwrap_or(terminal_columns);
        let payload = self.payloads.get(self.fixture).cloned().unwrap_or_default();
        let width = config.width(Some(columns));
        let lines = render_tree_at(&payload, config, Some(columns), &self.clock)
            .into_iter()
            .flat_map(|(row, lines)| {
                lines.into_iter().map(move |line| Placed {
                    row,
                    spans: line.spans(),
                    modules: line.modules(),
                    line,
                })
            })
            .collect();
        Rendered { lines, width, columns }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::SCHEMAS;

    /// frm-11: the live pane's clock is the process's, and it reads no
    /// git, no settings (nor the managed file) and spawns no worker; the
    /// snapshot tests never build one, so this is its only pin.
    #[test]
    fn the_live_pane_touches_nothing_outside_the_fixture() {
        let p = Preview::live();
        assert!(!p.clock.git && !p.clock.settings && !p.clock.workers, "{:?}", p.clock);
        assert!(p.clock.managed.is_none() && p.clock.settings_keys.is_none(), "{:?}", p.clock);
        assert!(p.clock.cache.is_none(), "{:?}", p.clock);
        assert!(!p.pinned, "the animations move");
    }

    #[test]
    fn the_pane_renders_the_fixture_at_the_width_and_maps_cells_to_modules() {
        let mut p = Preview::new(Clock::fixed(), true);
        assert_eq!(p.fixture_name(), "subscription-full");
        let (config, _) = crate::config::parse("icons = \"unicode\"\n", &SCHEMAS);
        let r = p.render(&config, 100);
        assert_eq!(r.width, 96);
        assert_eq!(r.lines.len(), 4);
        assert!(r.lines.iter().all(|l| l.line.width() == 96));
        // The first row's first module is `path`, right after the cap and pad.
        let hit = r.at(4, 0).unwrap();
        assert_eq!(hit, Hit { row: 0, elem: Elem::Module("path".into()) });
        assert_eq!(r.at(0, 0).unwrap().elem, Elem::Cap);
        assert_eq!(r.at(95, 3).unwrap().row, 3);
        assert!(r.at(0, 4).is_none());
        // `w` overrides the terminal width; fixtures cycle both ways.
        p.columns = Some(60);
        assert_eq!(p.render(&config, 100).width, 56);
        p.cycle(false);
        assert_eq!(p.fixture, FIXTURES.len() - 1);
        p.cycle(true);
        assert_eq!(p.fixture, 0);
        p.show("api-key");
        assert_eq!(p.fixture_name(), "api-key");
        assert!(p.fixture_summary().contains("API key"));
        let before = p.clock.now;
        p.tick();
        assert_eq!(p.clock.now, before, "a pinned clock never moves");
    }
}
