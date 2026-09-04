//! Shared rendering helpers: smooth bars, percent formatting.

use crate::ansi::{Color, Segment, Style};
use crate::num::{floor_to_u64, u64_to_usize, usize_to_f64};

/// Eighth-block characters for sub-cell precision, from 1/8 to 7/8.
const PARTIALS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// Render a smooth horizontal bar.
///
/// `percent` is 0..=100; `width` is the number of cells; `fill`/`empty` are
/// the glyphs for filled and empty cells (`fill` may be multi-char in ASCII
/// sets, in which case no partial blocks are used). `marker` places a glyph at
/// a percentage position (drawn over the empty part only). The filled part
/// takes `fill_color`; the rest `empty_color`.
#[must_use]
pub fn bar(
    width: usize,
    percent: f64,
    fill: &str,
    empty: &str,
    fill_color: Color,
    empty_color: Color,
    marker: Option<(f64, &str, Color)>,
) -> Vec<Segment> {
    if width == 0 {
        return Vec::new();
    }
    let pct = crate::num::clamp_percent(percent);
    let cells_f = usize_to_f64(width) * pct / 100.0;
    let whole = u64_to_usize(floor_to_u64(cells_f)).min(width);
    let frac = cells_f - usize_to_f64(whole);
    let smooth = fill == "█" && empty.chars().count() == 1;
    let partial_idx = u64_to_usize(floor_to_u64(frac * 8.0));

    let mut segs: Vec<Segment> = Vec::new();
    let mut filled_text = fill.repeat(whole);
    let mut used = whole;
    let partial = (smooth && whole < width && partial_idx > 0)
        .then(|| PARTIALS.get(partial_idx.saturating_sub(1)))
        .flatten();
    if let Some(c) = partial {
        filled_text.push(*c);
        used = used.saturating_add(1);
    }
    if !filled_text.is_empty() {
        segs.push(Segment::styled(filled_text, Style::fg(fill_color)));
    }
    let remaining = width.saturating_sub(used);
    if remaining > 0 {
        let marker_cell = marker.and_then(|(mpct, glyph, color)| {
            let cell = u64_to_usize(floor_to_u64(
                usize_to_f64(width) * crate::num::clamp_percent(mpct) / 100.0,
            ));
            let cell = cell.min(width.saturating_sub(1));
            (cell >= used && !glyph.is_empty()).then_some((cell, glyph, color))
        });
        match marker_cell {
            Some((cell, glyph, color)) => {
                let before = cell.saturating_sub(used);
                if before > 0 {
                    segs.push(Segment::styled(empty.repeat(before), Style::fg(empty_color)));
                }
                segs.push(Segment::styled(glyph, Style::fg(color).bolded()));
                let after = remaining.saturating_sub(before).saturating_sub(1);
                if after > 0 {
                    segs.push(Segment::styled(empty.repeat(after), Style::fg(empty_color)));
                }
            }
            None => segs.push(Segment::styled(empty.repeat(remaining), Style::fg(empty_color))),
        }
    }
    segs
}

/// Format a percentage with no decimals, e.g. `42%`.
#[must_use]
pub fn percent(p: f64) -> String {
    format!("{}%", crate::num::round_to_u64(crate::num::clamp_percent(p)))
}

/// Format a percentage allowing values above 100 (spend limits).
#[must_use]
pub fn percent_unclamped(p: f64) -> String {
    if p.is_nan() || p < 0.0 { "0%".into() } else { format!("{}%", crate::num::round_to_u64(p)) }
}

/// Format dollars: `$0.42`, `$12.35`, `$1.2k`.
#[must_use]
pub fn dollars(usd: f64, decimals: usize) -> String {
    if usd.is_nan() || usd < 0.0 {
        return "$0.00".into();
    }
    if usd >= 1000.0 {
        return format!("${:.1}k", usd / 1000.0);
    }
    format!("${usd:.decimals$}")
}

/// Format a token count compactly: `12k`, `1.0M`, `200k`.
#[must_use]
pub fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", crate::num::u64_to_f64(n) / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::Painter;

    fn text(segs: &[Segment]) -> String {
        Painter::PLAIN.paint(segs)
    }

    #[test]
    fn smooth_bar_uses_partial_blocks() {
        let c = Color::Default;
        assert_eq!(text(&bar(10, 0.0, "█", "░", c, c, None)), "░░░░░░░░░░");
        assert_eq!(text(&bar(10, 100.0, "█", "░", c, c, None)), "██████████");
        assert_eq!(text(&bar(10, 50.0, "█", "░", c, c, None)), "█████░░░░░");
        assert_eq!(text(&bar(10, 55.0, "█", "░", c, c, None)), "█████▌░░░░");
        assert_eq!(text(&bar(10, 42.0, "█", "░", c, c, None)), "████▏░░░░░");
        assert_eq!(text(&bar(4, 120.0, "█", "░", c, c, None)), "████");
        assert_eq!(bar(0, 50.0, "█", "░", c, c, None), Vec::new());
    }

    #[test]
    fn ascii_bar_has_no_partials() {
        let c = Color::Default;
        assert_eq!(text(&bar(10, 55.0, "#", "-", c, c, None)), "#####-----");
    }

    #[test]
    fn marker_lands_in_the_empty_part_only() {
        let c = Color::Default;
        assert_eq!(text(&bar(10, 30.0, "█", "░", c, c, Some((90.0, "▏", c)))), "███░░░░░░▏");
        // marker inside the filled part is hidden
        assert_eq!(text(&bar(10, 95.0, "█", "░", c, c, Some((90.0, "▏", c)))), "█████████▌");
        // marker at 100% sits in the last cell
        assert_eq!(text(&bar(5, 0.0, "█", "░", c, c, Some((100.0, "|", c)))), "░░░░|");
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(percent(41.6), "42%");
        assert_eq!(percent(140.0), "100%");
        assert_eq!(percent_unclamped(112.4), "112%");
        assert_eq!(dollars(1.2345, 2), "$1.23");
        assert_eq!(dollars(0.0, 2), "$0.00");
        assert_eq!(dollars(1234.0, 2), "$1.2k");
        assert_eq!(dollars(-1.0, 2), "$0.00");
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(12_345), "12k");
        assert_eq!(tokens(1_000_000), "1.0M");
    }
}
