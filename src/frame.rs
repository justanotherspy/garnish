//! Frames: the characters a row is drawn with.
//!
//! The caps that join rows into one block, the rule between the groups, the
//! glyphs of a box, and the two things that move with the clock (the
//! ticker's window and the rule's pattern). Putting them on a line is
//! [`crate::layout`].

/// The one cell that keeps a row on screen that Claude Code would trim
/// away: a `blank = true` spacer (SPEC § 4.1), and the first leading space
/// of a row painted without colour (SPEC § 2.1).
///
/// A braille blank: JavaScript's `trim` does not count it as whitespace,
/// and a font with the clock spinner's braille should draw it empty.
pub const BLANK_CELL: char = '\u{2800}';

/// Named frame styles, read from a config through
/// [`crate::config::Vocab`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameStyle {
    /// No frame characters at all.
    None,
    /// `╭─ ├─ ╰─` with `─` rules and `╮ ┤ ╯` caps.
    #[default]
    Rounded,
    /// `┌─ ├─ └─`.
    Square,
    /// `╔═ ╠═ ╚═`.
    Double,
    /// `┏━ ┣━ ┗━`.
    Heavy,
    /// Powerline separators, no vertical joins.
    Powerline,
    /// Characters from `[frame]` keys.
    Custom,
}

impl FrameStyle {
    /// All styles in documentation order.
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Rounded,
        Self::Square,
        Self::Double,
        Self::Heavy,
        Self::Powerline,
        Self::Custom,
    ];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rounded => "rounded",
            Self::Square => "square",
            Self::Double => "double",
            Self::Heavy => "heavy",
            Self::Powerline => "powerline",
            Self::Custom => "custom",
        }
    }
}

/// One glyph of a frame under the `[frame]` key that sets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedGlyph<'a> {
    /// The `[frame]` key.
    pub key: &'static str,
    /// The glyph in effect.
    pub value: &'a str,
    /// What it draws, as the `setup` form says it.
    pub doc: &'static str,
    /// The config refuses anything but one cell here, so an empty value is
    /// a style without the glyph, never one a file could write.
    pub one_cell: bool,
}

/// The characters of a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameChars {
    /// Prefix of the first line (when there are several).
    pub first: String,
    /// Prefix of middle lines.
    pub middle: String,
    /// Prefix of the last line.
    pub last: String,
    /// Prefix when there is exactly one line.
    pub single: String,
    /// Rule character repeated between the left and right groups.
    pub fill: String,
    /// Right cap of the first line.
    pub right_first: String,
    /// Right cap of middle lines.
    pub right_middle: String,
    /// Right cap of the last line.
    pub right_last: String,
    /// Right cap of a single line.
    pub right_single: String,
    /// Text between the prefix and the content, and content and the rule.
    pub pad: String,
    /// Default separator between modules.
    pub separator: String,
    /// Top-left corner of a box (SPEC § 4.3).
    pub top_left: String,
    /// Top-right corner of a box.
    pub top_right: String,
    /// Bottom-left corner of a box.
    pub bottom_left: String,
    /// Bottom-right corner of a box.
    pub bottom_right: String,
    /// The glyph at both ends of a line inside a box.
    pub side: String,
}

impl FrameChars {
    /// Built-in characters for a named style.
    ///
    /// The five box glyphs (SPEC § 4.3) are empty for the styles with no box
    /// shape: `none` draws an invisible box, and `powerline` is reported and
    /// drawn `rounded` before it reaches here.
    #[must_use]
    pub fn for_style(style: FrameStyle) -> Self {
        let s = |v: &str| v.to_owned();
        match style {
            FrameStyle::None | FrameStyle::Custom => Self {
                first: String::new(),
                middle: String::new(),
                last: String::new(),
                single: String::new(),
                fill: s(" "),
                right_first: String::new(),
                right_middle: String::new(),
                right_last: String::new(),
                right_single: String::new(),
                pad: String::new(),
                separator: s("  "),
                top_left: String::new(),
                top_right: String::new(),
                bottom_left: String::new(),
                bottom_right: String::new(),
                side: String::new(),
            },
            FrameStyle::Rounded => Self::boxed("╭─", "├─", "╰─", "──", "─", "─╮", "─┤", "─╯", "──")
                .with_box("╭", "╮", "╰", "╯", "│"),
            FrameStyle::Square => Self::boxed("┌─", "├─", "└─", "──", "─", "─┐", "─┤", "─┘", "──")
                .with_box("┌", "┐", "└", "┘", "│"),
            FrameStyle::Double => Self::boxed("╔═", "╠═", "╚═", "══", "═", "═╗", "═╣", "═╝", "══")
                .with_box("╔", "╗", "╚", "╝", "║"),
            FrameStyle::Heavy => Self::boxed("┏━", "┣━", "┗━", "━━", "━", "━┓", "━┫", "━┛", "━━")
                .with_box("┏", "┓", "┗", "┛", "┃"),
            FrameStyle::Powerline => Self {
                first: s("\u{e0b6}"),
                middle: s("\u{e0b6}"),
                last: s("\u{e0b6}"),
                single: s("\u{e0b6}"),
                fill: s(" "),
                right_first: s("\u{e0b4}"),
                right_middle: s("\u{e0b4}"),
                right_last: s("\u{e0b4}"),
                right_single: s("\u{e0b4}"),
                // The caps are half-circles; without a pad the text touches them.
                pad: s(" "),
                separator: s(" \u{e0b1} "),
                top_left: String::new(),
                top_right: String::new(),
                bottom_left: String::new(),
                bottom_right: String::new(),
                side: String::new(),
            },
        }
    }

    /// The five box glyphs of a built-in style.
    fn with_box(mut self, tl: &str, tr: &str, bl: &str, br: &str, side: &str) -> Self {
        self.top_left = tl.into();
        self.top_right = tr.into();
        self.bottom_left = bl.into();
        self.bottom_right = br.into();
        self.side = side.into();
        self
    }

    #[allow(clippy::too_many_arguments)] // nine literal glyphs; a struct literal would be noisier
    fn boxed(
        first: &str,
        middle: &str,
        last: &str,
        single: &str,
        fill: &str,
        rf: &str,
        rm: &str,
        rl: &str,
        rs: &str,
    ) -> Self {
        Self {
            first: first.into(),
            middle: middle.into(),
            last: last.into(),
            single: single.into(),
            fill: fill.into(),
            right_first: rf.into(),
            right_middle: rm.into(),
            right_last: rl.into(),
            right_single: rs.into(),
            pad: " ".into(),
            separator: " │ ".into(),
            top_left: String::new(),
            top_right: String::new(),
            bottom_left: String::new(),
            bottom_right: String::new(),
            side: String::new(),
        }
    }

    /// Every glyph under its `[frame]` key, in the order the `setup` form
    /// lists them: the one table the form, its suggestions and
    /// `config show` read.
    #[must_use]
    pub fn named(&self) -> [NamedGlyph<'_>; 16] {
        const fn g<'a>(
            key: &'static str,
            value: &'a str,
            doc: &'static str,
            one_cell: bool,
        ) -> NamedGlyph<'a> {
            NamedGlyph { key, value, doc, one_cell }
        }
        [
            g("separator", &self.separator, "Default separator between modules.", false),
            g("pad", &self.pad, "Text between a cap and the content.", false),
            g("fill_char", &self.fill, "The rule glyph, one cell.", true),
            g("first", &self.first, "Left cap of the first line (custom style).", false),
            g("middle", &self.middle, "Left cap of middle lines.", false),
            g("last", &self.last, "Left cap of the last line.", false),
            g("single", &self.single, "Left cap of a lone line.", false),
            g("right_first", &self.right_first, "Right cap of the first line.", false),
            g("right_middle", &self.right_middle, "Right cap of middle lines.", false),
            g("right_last", &self.right_last, "Right cap of the last line.", false),
            g("right_single", &self.right_single, "Right cap of a lone line.", false),
            g("top_left", &self.top_left, "A box's top-left corner, one cell.", true),
            g("top_right", &self.top_right, "A box's top-right corner.", true),
            g("bottom_left", &self.bottom_left, "A box's bottom-left corner.", true),
            g("bottom_right", &self.bottom_right, "A box's bottom-right corner.", true),
            g("side", &self.side, "A box's side glyph.", true),
        ]
    }

    /// Prefix and right cap for line `index` of `count`.
    #[must_use]
    pub fn ends(&self, index: usize, count: usize) -> (&str, &str) {
        if count <= 1 {
            (&self.single, &self.right_single)
        } else if index == 0 {
            (&self.first, &self.right_first)
        } else if index.saturating_add(1) >= count {
            (&self.last, &self.right_last)
        } else {
            (&self.middle, &self.right_middle)
        }
    }
}

/// The line ticker (SPEC § 4.1): an over-budget left group scrolls instead of being cut.
///
/// The offset is a pure function of the tick's clock ([`crate::time::frame`]),
/// so a cancelled tick loses nothing and `GARNISH_NOW` pins the window. A
/// layout carries a ticker only while animations run; with them off the
/// group is truncated like any other (SPEC § 4.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Ticker {
    /// Cells the window advances per tick (0.5 = every second tick).
    pub step: f64,
    /// Text between the end of the group and its wrapped-around start.
    pub gap: String,
    /// The tick's clock.
    pub now: jiff::Timestamp,
}

/// An animated rule (SPEC § 4.2): one-cell glyphs repeated across the rule,
/// starting at `offset` so the pattern appears to travel one step per tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The pattern, one cell per entry (validated by the config).
    pub cells: Vec<String>,
    /// Index of the pattern cell drawn in the rule's first cell.
    pub offset: usize,
}

impl Rule {
    /// The rule text for `width` cells starting `start` cells into the
    /// line's rule.
    ///
    /// A line's rule cells are numbered together, gaps between columns
    /// included, so the pattern travels across a column boundary instead of
    /// restarting at each (SPEC § 4.3).
    #[must_use]
    pub fn paint_at(&self, start: usize, width: usize) -> String {
        let n = self.cells.len();
        (0..width)
            .filter_map(|i| {
                let at = i.saturating_add(start).saturating_add(self.offset).checked_rem(n)?;
                self.cells.get(at).map(String::as_str)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::display_width;

    /// Every built-in style with a box shape carries all five glyphs, one
    /// cell each, and the two without carry none (SPEC § 4.3).
    #[test]
    fn box_glyphs_exist_for_every_style_that_has_a_shape() {
        for style in FrameStyle::ALL {
            let c = FrameChars::for_style(style);
            let glyphs = [&c.top_left, &c.top_right, &c.bottom_left, &c.bottom_right, &c.side];
            let shaped = matches!(
                style,
                FrameStyle::Rounded | FrameStyle::Square | FrameStyle::Double | FrameStyle::Heavy
            );
            for glyph in glyphs {
                if shaped {
                    assert_eq!(display_width(glyph), 1, "{}: {glyph:?}", style.name());
                } else {
                    assert!(glyph.is_empty(), "{}: {glyph:?}", style.name());
                }
            }
        }
    }
}
