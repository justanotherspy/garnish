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

/// Where a list's cursor goes on `key` among `len` entries: `↑`/`↓` wrap,
/// the page keys go ten at a time and stop at the ends, `Home`/`End` jump;
/// `None` for any other key.
#[must_use]
pub fn move_cursor(cursor: usize, len: usize, key: crate::setup::Key) -> Option<usize> {
    use crate::setup::Key;
    let last = len.saturating_sub(1);
    Some(match key {
        Key::Up => cursor.checked_sub(1).unwrap_or(last),
        Key::Down => cursor.saturating_add(1).checked_rem(len.max(1)).unwrap_or(0),
        Key::PageUp => cursor.saturating_sub(10),
        Key::PageDown => cursor.saturating_add(10).min(last),
        Key::Home => 0,
        Key::End => last,
        _ => return None,
    })
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

/// The cells each hint of [`hints`] occupies, as `(key, start, end)` from
/// the line's first cell: what a click on the hint bar is measured against.
#[must_use]
pub fn hint_cells<'a>(pairs: &[(&'a str, &str)]) -> Vec<(&'a str, usize, usize)> {
    let mut x = 0_usize;
    let mut out = Vec::new();
    for (i, (key, what)) in pairs.iter().enumerate() {
        if i > 0 {
            x = x.saturating_add(2);
        }
        let start = x;
        x = x
            .saturating_add(crate::ansi::display_width(key))
            .saturating_add(1)
            .saturating_add(crate::ansi::display_width(what));
        out.push((*key, start, x));
    }
    out
}

/// `text` cut to `width` cells with an ellipsis, for a label that must fit
/// its column.
#[must_use]
pub fn clip(text: &str, width: usize) -> String {
    let segs = [crate::ansi::Segment::plain(text)];
    crate::ansi::Painter::PLAIN.paint(&crate::ansi::truncate(&segs, width, "…"))
}

/// Spans cut to `width` cells with an ellipsis as [`clip`] cuts their
/// text, each keeping its style (a selected chip past the cut still shows
/// selected); spans that fit come back as they are.
#[must_use]
pub fn clip_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    if crate::ansi::display_width(&text) <= width {
        return spans;
    }
    let clipped = clip(&text, width);
    let mut kept = clipped.chars().peekable();
    let mut out: Vec<Span<'static>> = Vec::new();
    'spans: for span in spans {
        let mut part = String::new();
        for c in span.content.chars() {
            if kept.peek() != Some(&c) {
                if !part.is_empty() {
                    out.push(Span::styled(part, span.style));
                }
                break 'spans;
            }
            part.push(c);
            kept.next();
        }
        out.push(Span::styled(part, span.style));
    }
    // What the cut put in place of the rest: the ellipsis, and a pad for a
    // wide glyph it could not fit.
    let rest: String = kept.collect();
    if !rest.is_empty() {
        out.push(Span::raw(rest));
    }
    out
}

/// Warnings as lines of their own, never clipped away.
///
/// On one line when they all fit in `width`, else one per line (each cut
/// to the width). A warning appended to a long title vanished past the
/// right edge on the narrow terminals it was written for.
#[must_use]
pub fn note_lines(notes: &[String], width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let joined = notes.join("  ");
    if notes.is_empty() {
        Vec::new()
    } else if crate::ansi::display_width(&joined) <= width {
        vec![Line::from(Span::styled(joined, Chrome::warn()))]
    } else {
        notes.iter().map(|n| Line::from(Span::styled(clip(n, width), Chrome::warn()))).collect()
    }
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
        // The hint cells tile the line the same way `hints` draws it.
        let pairs = [("enter", "edit"), ("u", "undo")];
        assert_eq!(hint_cells(&pairs), vec![("enter", 0, 10), ("u", 12, 18)]);
        assert_eq!(hints(&pairs).width(), 18);
    }

    #[test]
    fn a_list_cursor_wraps_pages_and_jumps() {
        use crate::setup::Key;
        assert_eq!(move_cursor(0, 5, Key::Up), Some(4), "wraps");
        assert_eq!(move_cursor(4, 5, Key::Down), Some(0));
        assert_eq!(move_cursor(3, 25, Key::PageDown), Some(13));
        assert_eq!(move_cursor(20, 25, Key::PageDown), Some(24), "stops at the end");
        assert_eq!(move_cursor(3, 25, Key::PageUp), Some(0));
        assert_eq!((move_cursor(3, 5, Key::Home), move_cursor(3, 5, Key::End)), (Some(0), Some(4)));
        assert_eq!(move_cursor(0, 0, Key::Down), Some(0));
        assert_eq!(move_cursor(1, 5, Key::Enter), None);
    }
}
