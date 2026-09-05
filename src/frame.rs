//! Frames: the characters that join lines together, and the assembly of one
//! output line from a left group, a right group and the frame rule.

use itertools::Itertools;
use serde::Deserialize;

use crate::ansi::{Segment, Style, display_width, scroll, segments_width, truncate};
use crate::theme::{Role, Theme};

/// Named frame styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
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

    /// Parse a config name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == s)
    }
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
}

impl FrameChars {
    /// Built-in characters for a named style.
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
            },
            FrameStyle::Rounded => Self::boxed("╭─", "├─", "╰─", "──", "─", "─╮", "─┤", "─╯", "──"),
            FrameStyle::Square => Self::boxed("┌─", "├─", "└─", "──", "─", "─┐", "─┤", "─┘", "──"),
            FrameStyle::Double => Self::boxed("╔═", "╠═", "╚═", "══", "═", "═╗", "═╣", "═╝", "══"),
            FrameStyle::Heavy => Self::boxed("┏━", "┣━", "┗━", "━━", "━", "━┓", "━┫", "━┛", "━━"),
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
            },
        }
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
        }
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
/// so a cancelled tick loses nothing and `GARNISH_NOW` pins the window.
#[derive(Debug, Clone, PartialEq)]
pub struct Ticker {
    /// Cells the window advances per tick (0.5 = every second tick).
    pub step: f64,
    /// Text between the end of the group and its wrapped-around start.
    pub gap: String,
    /// The tick's clock.
    pub now: jiff::Timestamp,
    /// Whether animations run; off pins the window at offset 0.
    pub animate: bool,
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
    /// The rule text for `width` cells.
    #[must_use]
    pub fn paint(&self, width: usize) -> String {
        let n = self.cells.len();
        (0..width)
            .filter_map(|i| {
                let at = i.saturating_add(self.offset).checked_rem(n)?;
                self.cells.get(at).map(String::as_str)
            })
            .collect()
    }
}

/// Layout parameters for one render.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Frame characters.
    pub chars: FrameChars,
    /// Fill the rule to the full width and close with a right cap.
    pub fill: bool,
    /// Available width in cells.
    pub width: usize,
    /// Truncate the left group when the line overflows.
    pub truncate: bool,
    /// Ellipsis used when truncating.
    pub ellipsis: String,
    /// Scroll an overflowing left group instead of truncating it.
    pub ticker: Option<Ticker>,
    /// Paint the rule from a moving pattern instead of `fill_char`.
    pub rule: Option<Rule>,
}

/// Compose one line.
///
/// `left` and `right` are already-joined module segments; `separator` is the
/// line's own separator (per-line override or the frame default), used to
/// join the two groups when the rule is not filled.
///
/// Rules: prefix + pad + left + pad + rule + pad + right + pad + cap. On
/// overflow the rule collapses to a single fill cell, then the left group is
/// truncated, or scrolled when the layout has a [`Ticker`]; the right group
/// is never truncated. With `truncate = false` the whole row is handed to
/// the harness as is, ticker or not.
#[must_use]
pub fn compose_line(
    layout: &Layout,
    theme: &Theme,
    index: usize,
    count: usize,
    left: &[Segment],
    right: &[Segment],
    separator: &str,
) -> Vec<Segment> {
    let frame_style = Style::fg(theme.role(Role::Frame));
    let (prefix, cap) = layout.chars.ends(index, count);
    let pad = layout.chars.pad.as_str();
    let pad_w = display_width(pad);
    let prefix_w = if prefix.is_empty() { 0 } else { display_width(prefix).saturating_add(pad_w) };
    // The cap is padded from the right group only; with no right group the rule runs into it.
    let cap_w = if layout.fill && !cap.is_empty() {
        display_width(cap).saturating_add(if right.is_empty() { 0 } else { pad_w })
    } else {
        0
    };
    let right_w = segments_width(right);
    // With a rule the right group is preceded by a pad; without one, by the separator (join_w).
    let right_block_w =
        if right.is_empty() || !layout.fill { right_w } else { right_w.saturating_add(pad_w) };
    let fill_w = display_width(&layout.chars.fill).max(1);

    // Cells the left group may occupy before it gets truncated.
    let join_w = if layout.fill {
        fill_w.saturating_add(if left.is_empty() { 0 } else { pad_w })
    } else if right.is_empty() {
        0
    } else {
        display_width(separator)
    };
    let left_budget = layout
        .width
        .saturating_sub(prefix_w)
        .saturating_sub(right_block_w)
        .saturating_sub(cap_w)
        .saturating_sub(join_w);
    let left_segs: Vec<Segment> = if layout.truncate && segments_width(left) > left_budget {
        layout.ticker.as_ref().map_or_else(
            || truncate(left, left_budget, &layout.ellipsis),
            |ticker| {
                let period = segments_width(left).saturating_add(display_width(&ticker.gap));
                let offset = if ticker.animate {
                    crate::time::frame(ticker.now, ticker.step, period)
                } else {
                    0
                };
                scroll(left, left_budget, offset, &ticker.gap, true)
            },
        )
    } else {
        left.to_vec()
    };
    let left_w = segments_width(&left_segs);
    let left_pad_w = if left_segs.is_empty() { 0 } else { pad_w };

    let mut out: Vec<Segment> = Vec::new();
    if !prefix.is_empty() {
        out.push(Segment::styled(prefix, frame_style));
        out.push(Segment::plain(pad));
    }
    out.extend(left_segs);

    if layout.fill {
        let rule_cells = layout
            .width
            .saturating_sub(prefix_w)
            .saturating_sub(left_w)
            .saturating_sub(left_pad_w)
            .saturating_sub(right_block_w)
            .saturating_sub(cap_w);
        if left_pad_w > 0 {
            out.push(Segment::plain(pad));
        }
        // The rule's width never changes with the pattern, only which glyph
        // lands in each cell (SPEC § 4.2).
        match &layout.rule {
            Some(rule) if !rule.cells.is_empty() && rule_cells > 0 => {
                out.push(Segment::styled(rule.paint(rule_cells), frame_style));
            }
            _ => {
                let reps = rule_cells.checked_div(fill_w).unwrap_or(0);
                if reps > 0 {
                    out.push(Segment::styled(layout.chars.fill.repeat(reps), frame_style));
                }
            }
        }
        if !right.is_empty() {
            out.push(Segment::plain(pad));
            out.extend(right.iter().cloned());
        }
        if cap_w > 0 {
            if !right.is_empty() {
                out.push(Segment::plain(pad));
            }
            out.push(Segment::styled(cap, frame_style));
        }
    } else if !right.is_empty() {
        if !left.is_empty() {
            out.push(Segment::styled(separator, Style::fg(theme.role(Role::Muted))));
        }
        out.extend(right.iter().cloned());
    }
    // The right group is never truncated by design, but a terminal narrower
    // than the right group alone must still get a line that fits.
    if layout.truncate && segments_width(&out) > layout.width {
        return truncate(&out, layout.width, &layout.ellipsis);
    }
    out
}

/// Join module renders with a separator, skipping empty ones.
#[must_use]
pub fn join_modules(parts: &[Vec<Segment>], separator: &str, theme: &Theme) -> Vec<Segment> {
    let sep = vec![Segment::styled(separator, Style::fg(theme.role(Role::Muted)))];
    let non_empty = parts.iter().filter(|p| !p.is_empty());
    if separator.is_empty() {
        non_empty.flatten().cloned().collect()
    } else {
        Itertools::intersperse(non_empty, &sep).flatten().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::Painter;

    fn layout(style: FrameStyle, fill: bool, width: usize) -> Layout {
        Layout {
            chars: FrameChars::for_style(style),
            fill,
            width,
            truncate: true,
            ellipsis: "…".into(),
            ticker: None,
            rule: None,
        }
    }

    fn text(segs: &[Segment]) -> String {
        Painter::PLAIN.paint(segs)
    }

    #[test]
    fn rounded_frame_fills_to_width_with_right_group() {
        let theme = Theme::default();
        let l = layout(FrameStyle::Rounded, true, 30);
        let left = [Segment::plain("left")];
        let right = [Segment::plain("R")];
        let sep = l.chars.separator.clone();
        let line = compose_line(&l, &theme, 0, 2, &left, &right, &sep);
        let s = text(&line);
        assert_eq!(s, format!("╭─ left {} R ─╮", "─".repeat(17)));
        assert_eq!(display_width(&s), 30);
        let last = text(&compose_line(&l, &theme, 1, 2, &left, &[], &sep));
        assert_eq!(last, format!("╰─ left {}╯", "─".repeat(21)));
        assert_eq!(display_width(&last), 30);
        let single = text(&compose_line(&l, &theme, 0, 1, &left, &right, &sep));
        assert!(single.starts_with("── left") && single.ends_with("R ──"));
        assert_eq!(display_width(&single), 30);
    }

    #[test]
    fn overflow_truncates_left_never_right() {
        let theme = Theme::default();
        let l = layout(FrameStyle::Rounded, true, 20);
        let left = [Segment::plain("a very long left group")];
        let right = [Segment::plain("RIGHT")];
        let s = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(display_width(&s), 20);
        assert!(s.ends_with("RIGHT ──"), "{s}");
        assert!(s.contains('…'));
    }

    #[test]
    fn no_fill_uses_the_lines_separator_and_prefix_only() {
        let theme = Theme::default();
        let l = layout(FrameStyle::Square, false, 80);
        let (left, right) = ([Segment::plain("L")], [Segment::plain("R")]);
        let s = text(&compose_line(&l, &theme, 1, 3, &left, &right, &l.chars.separator));
        assert_eq!(s, "├─ L │ R");
        // A per-line separator joins the groups (walkthrough bug 6: the frame
        // default used to be taken regardless).
        let s = text(&compose_line(&l, &theme, 1, 3, &left, &right, " · "));
        assert_eq!(s, "├─ L · R");
        let none = layout(FrameStyle::None, false, 80);
        let s = text(&compose_line(&none, &theme, 0, 1, &left, &right, &none.chars.separator));
        assert_eq!(s, "L  R");
        let s = text(&compose_line(&none, &theme, 0, 1, &left, &[], &none.chars.separator));
        assert_eq!(s, "L");
    }

    /// SPEC § 4.1 Ticker: an over-budget left group scrolls one step per
    /// tick and wraps around after the gap; the right group and the frame
    /// are untouched, the row keeps its width, `animate = false` pins the
    /// window, and `truncate = false` hands the row over whole.
    #[test]
    fn ticker_scrolls_the_left_group_and_leaves_the_right_alone() {
        let theme = Theme::default();
        let at = |secs: i64| jiff::Timestamp::from_second(secs).unwrap();
        let mut l = layout(FrameStyle::Rounded, true, 24);
        let left = [Segment::plain("abcdefghijklmnop")]; // 16 cells
        let right = [Segment::plain("R")];
        // Budget: 24 − "╭─ " (3) − " R ─╮" (5) − rule + pad (2) = 14 cells.
        let cut = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(cut, "── abcdefghijklm… ─ R ──");
        let ticker = |secs: i64, animate: bool| Ticker {
            step: 1.0,
            gap: " · ".into(),
            now: at(secs),
            animate,
        };
        // period = 16 + 3 = 19; 1738425600 % 19 = 4 → the window starts at "e"
        // and, 14 cells later, shows the first two cells of the gap.
        l.ticker = Some(ticker(1_738_425_600, true));
        let s0 = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(s0, "── efghijklmnop · ─ R ──");
        assert_eq!(display_width(&s0), 24);
        l.ticker = Some(ticker(1_738_425_601, true));
        let s1 = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(s1, "── fghijklmnop ·  ─ R ──", "one cell further: the whole gap shows");
        l.ticker = Some(ticker(1_738_425_611, true));
        let wrapped = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(wrapped, "── p · abcdefghij ─ R ──", "offset 15: end, gap, start");
        // Frozen: offset 0 whatever the clock says.
        l.ticker = Some(ticker(1_738_425_601, false));
        let frozen = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(frozen, "── abcdefghijklmn ─ R ──");
        // A group that fits is never scrolled.
        let short = [Segment::plain("abc")];
        l.ticker = Some(ticker(1_738_425_601, true));
        assert_eq!(
            text(&compose_line(&l, &theme, 0, 1, &short, &right, " │ ")),
            format!("── abc {} R ──", "─".repeat(12))
        );
        // truncate = false: the whole row, ticker or not.
        l.truncate = false;
        let whole = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert!(whole.contains("abcdefghijklmnop"), "{whole}");
    }

    /// SPEC § 4.2 Animated rule: the pattern fills the rule cell by cell from
    /// `offset`, the rule keeps its width, and the caps and groups are as
    /// with a plain fill.
    #[test]
    fn patterned_rule_keeps_its_width_and_shifts_with_the_offset() {
        let theme = Theme::default();
        let mut l = layout(FrameStyle::Rounded, true, 20);
        let (left, right) = ([Segment::plain("L")], [Segment::plain("R")]);
        let plain = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        // "── L " (5) + rule (10) + " R ──" (5)
        assert_eq!(plain, format!("── L {} R ──", "─".repeat(10)));
        let cells =
            |offset| Some(Rule { cells: vec!["·".into(), " ".into(), " ".into()], offset });
        l.rule = cells(0);
        let s0 = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
        assert_eq!(s0, "── L ·  ·  ·  · R ──");
        assert_eq!(display_width(&s0), 20);
        let expected = |offset: usize| {
            let pattern = ["·", " ", " "];
            let rule: String = (0..10).map(|i| pattern[(i + offset) % 3]).collect();
            format!("── L {rule} R ──")
        };
        for offset in 1..=4 {
            l.rule = cells(offset);
            let s = text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ "));
            assert_eq!(s, expected(offset), "offset {offset}");
            assert_eq!(display_width(&s), 20);
        }
        assert_eq!(Rule { cells: vec!["ab".into()], offset: 5 }.paint(3), "ababab", "offset wraps");
        assert_eq!(Rule { cells: Vec::new(), offset: 0 }.paint(3), "", "no pattern, no rule text");
        // An empty pattern falls back to the fill character.
        l.rule = Some(Rule { cells: Vec::new(), offset: 0 });
        assert_eq!(text(&compose_line(&l, &theme, 0, 1, &left, &right, " │ ")), plain);
    }

    #[test]
    fn powerline_caps_are_padded() {
        let theme = Theme::default();
        let l = layout(FrameStyle::Powerline, true, 20);
        let s = text(&compose_line(
            &l,
            &theme,
            0,
            1,
            &[Segment::plain("L")],
            &[Segment::plain("R")],
            " ",
        ));
        assert_eq!(s, "\u{e0b6} L              R \u{e0b4}");
        assert_eq!(display_width(&s), 20);
    }

    #[test]
    fn none_style_with_fill_pads_to_width() {
        let theme = Theme::default();
        let l = layout(FrameStyle::None, true, 12);
        let s = text(&compose_line(
            &l,
            &theme,
            0,
            1,
            &[Segment::plain("ab")],
            &[Segment::plain("cd")],
            "  ",
        ));
        assert_eq!(s, "ab        cd");
        assert_eq!(display_width(&s), 12);
    }

    #[test]
    fn join_skips_empty_modules() {
        let theme = Theme::default();
        let parts = vec![vec![Segment::plain("a")], vec![], vec![Segment::plain("b")]];
        assert_eq!(text(&join_modules(&parts, " · ", &theme)), "a · b");
        assert_eq!(text(&join_modules(&[], " · ", &theme)), "");
    }

    #[test]
    fn style_names_roundtrip() {
        for s in FrameStyle::ALL {
            assert_eq!(FrameStyle::parse(s.name()), Some(s));
        }
    }
}
