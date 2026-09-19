//! Small drawing helpers shared by the `setup` screens: rectangles, key
//! hints, scrolling windows, the chrome's own styles.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// A `usize` as a `u16` cell count, saturating: a terminal is never that big.
#[must_use]
pub fn cells(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// A rectangle `width × height` centred in `area`, clamped to it.
#[must_use]
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x.saturating_add(area.width.saturating_sub(w).checked_div(2).unwrap_or(0));
    let y = area.y.saturating_add(area.height.saturating_sub(h).checked_div(2).unwrap_or(0));
    Rect::new(x, y, w, h)
}

/// The first visible index of a list `len` long shown `height` rows high
/// with `cursor` kept in view, given the window's previous `start`.
#[must_use]
pub fn window(cursor: usize, len: usize, height: usize, start: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let max_start = len.saturating_sub(height);
    let mut start = start.min(max_start);
    if cursor < start {
        start = cursor;
    } else if cursor >= start.saturating_add(height) {
        start = cursor.saturating_sub(height).saturating_add(1);
    }
    start.min(max_start)
}

/// The chrome's styles: the screen's own text, never the config's colours.
pub struct Chrome;

impl Chrome {
    /// A heading.
    #[must_use]
    pub const fn title() -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    /// Advice and doc strings.
    #[must_use]
    pub const fn muted() -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    /// The selected item of a list.
    #[must_use]
    pub const fn selected() -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }

    /// A key in a hint line.
    #[must_use]
    pub const fn key() -> Style {
        Style::new().add_modifier(Modifier::BOLD).fg(Color::Indexed(14))
    }

    /// A warning.
    #[must_use]
    pub const fn warn() -> Style {
        Style::new().fg(Color::Indexed(11))
    }

    /// An error.
    #[must_use]
    pub const fn error() -> Style {
        Style::new().fg(Color::Indexed(9))
    }

    /// A value a file sets (as opposed to a resolved default).
    #[must_use]
    pub const fn set() -> Style {
        Style::new().fg(Color::Indexed(10))
    }
}

/// A line of `key  meaning` pairs, the keys bold.
#[must_use]
pub fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, what)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled((*key).to_owned(), Chrome::key()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*what).to_owned(), Chrome::muted()));
    }
    Line::from(spans)
}

/// `text` cut to `width` cells with an ellipsis, for a label that must fit
/// its column.
#[must_use]
pub fn clip(text: &str, width: usize) -> String {
    let segs = [crate::ansi::Segment::plain(text)];
    crate::ansi::Painter::PLAIN.paint(&crate::ansi::truncate(&segs, width, "…"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_keep_the_cursor_visible_and_rects_stay_inside() {
        assert_eq!(window(0, 3, 10, 0), 0);
        assert_eq!(window(12, 20, 5, 0), 8);
        assert_eq!(window(2, 20, 5, 8), 2);
        assert_eq!(window(19, 20, 5, 0), 15);
        assert_eq!(window(5, 20, 0, 3), 0);
        let r = centered(Rect::new(0, 0, 80, 24), 40, 10);
        assert_eq!((r.x, r.y, r.width, r.height), (20, 7, 40, 10));
        let r = centered(Rect::new(0, 0, 20, 5), 40, 10);
        assert_eq!((r.width, r.height), (20, 5));
        assert_eq!(cells(70_000), u16::MAX);
        assert_eq!(clip("hello world", 6), "hello…");
        assert_eq!(hints(&[("q", "quit")]).spans.len(), 3);
    }
}
