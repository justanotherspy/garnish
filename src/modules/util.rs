//! Shared rendering helpers: smooth bars, percent formatting.

use crate::ansi::{Color, Segment, Style};
use crate::icons::IconSet;
use crate::num::{floor_to_u64, u64_to_usize, usize_to_f64};

/// Eighth-block characters for sub-cell precision, from 1/8 to 7/8.
const PARTIALS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// Render a smooth horizontal bar.
///
/// `percent` is 0..=100; `width` is the number of cells; `fill`/`empty` are
/// the glyphs for filled and empty cells (with `█`/single-cell `empty` the
/// last filled cell uses eighth-blocks for sub-cell precision). `marker`
/// places a glyph at a percentage position; it always wins over the cell it
/// lands on, filled or not, so the compaction point stays visible as usage
/// approaches it. The filled part takes `fill_color`; the rest `empty_color`.
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
    // A bar wider than any row is a bad number, not a wide bar: cap it so a
    // config integer can never size the loop (`config::MAX_CELLS`).
    let width = width.min(crate::config::MAX_CELLS);
    // Glyphs that are not exactly one cell wide would break the width
    // accounting of the whole line; fall back to safe defaults.
    let fill = if crate::ansi::display_width(fill) == 1 { fill } else { "█" };
    let empty = if crate::ansi::display_width(empty) == 1 { empty } else { "░" };
    let marker = marker.map(|(p, g, c)| {
        (p, if g.is_empty() || crate::ansi::display_width(g) == 1 { g } else { "|" }, c)
    });
    let pct = crate::num::clamp_percent(percent);
    let cells_f = usize_to_f64(width) * pct / 100.0;
    let whole = u64_to_usize(floor_to_u64(cells_f)).min(width);
    let frac = cells_f - usize_to_f64(whole);
    let smooth = fill == "█" && empty.chars().count() == 1;
    let partial_idx = u64_to_usize(floor_to_u64(frac * 8.0));
    let partial = (smooth && whole < width && partial_idx > 0)
        .then(|| PARTIALS.get(partial_idx.saturating_sub(1)))
        .flatten();
    let marker_cell = marker.and_then(|(mpct, glyph, color)| {
        let cell = u64_to_usize(floor_to_u64(
            usize_to_f64(width) * crate::num::clamp_percent(mpct) / 100.0,
        ));
        (!glyph.is_empty()).then_some((cell.min(width.saturating_sub(1)), glyph, color))
    });

    let mut segs: Vec<Segment> = Vec::new();
    let mut push = |text: &str, style: Style| match segs.last_mut() {
        Some(last) if last.style == style => last.push_str(text),
        _ => segs.push(Segment::styled(text, style)),
    };
    let filled_style = Style::fg(fill_color);
    let empty_style = Style::fg(empty_color);
    for cell in 0..width {
        if let Some((mcell, glyph, color)) = marker_cell
            && cell == mcell
        {
            push(glyph, Style::fg(color).bolded());
        } else if cell < whole {
            push(fill, filled_style);
        } else if cell == whole
            && let Some(c) = partial
        {
            push(&c.to_string(), filled_style);
        } else {
            push(empty, empty_style);
        }
    }
    segs
}

/// The `bar` option's choices: block glyphs with fractional cells, or a line.
///
/// `line` draws `━`/`─` with whole cells only, so no hairline gaps appear in
/// fonts that draw `█` a shade narrower than a cell (SPEC § 4.1).
/// `ModuleCfg::resolve` applies the shorthand to the `fill`/`empty` icons; an
/// explicit override still wins.
pub const BAR_STYLES: &[&str] = &["blocks", "line"];

/// Format a percentage with no decimals, e.g. `42%`.
#[must_use]
pub fn percent(p: f64) -> String {
    format!("{}%", crate::num::round_to_u64(crate::num::clamp_percent(p)))
}

/// The percentage as the user sees it (rounded, 0..=100), so band colors
/// agree with the printed number at the boundaries (89.6 prints `90%` and is
/// colored as 90).
#[must_use]
pub fn rounded(p: f64) -> f64 {
    crate::num::u64_to_f64(crate::num::round_to_u64(crate::num::clamp_percent(p)))
}

/// Format a percentage allowing values above 100 (spend limits).
#[must_use]
pub fn percent_unclamped(p: f64) -> String {
    if p.is_nan() || p < 0.0 { "0%".into() } else { format!("{}%", crate::num::round_to_u64(p)) }
}

/// [`rounded`] for a percentage that may pass 100, so a band threshold above
/// 100 can be reached (the twin of [`percent_unclamped`], SPEC § 3.3).
#[must_use]
pub fn rounded_unclamped(p: f64) -> f64 {
    crate::num::u64_to_f64(crate::num::round_to_u64(p))
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
    // Capped at config time (`cost.decimals` has a schema `max`) and again
    // here: the formatter allocates one byte per place.
    let decimals = decimals.min(crate::config::MAX_DECIMALS);
    format!("${usd:.decimals$}")
}

/// Cut a name to at most `max` characters, ending in the icon set's ellipsis
/// ([`IconSet::ellipsis`]); `0` means no limit.
///
/// The `max_length` of `branch` and `session_name` (SPEC § 3.1, § 3.5). A
/// character here is a terminal cluster, the unit [`crate::ansi::truncate`]
/// cuts in, so a flag, a skin tone or a combining mark is never split in
/// half. The ellipsis is counted into the budget and is itself cut when
/// `max` is smaller than it, so the result is never wider than asked.
#[must_use]
pub fn cut_name(name: &str, max: usize, icons: IconSet) -> String {
    if max == 0 {
        return name.to_owned();
    }
    // A cluster is at least one char, so a name with no more chars than the
    // budget cannot need cutting and never reaches `clusters`, which
    // allocates a `String` per grapheme. That only covers the short names;
    // the long ones are bounded at the source instead (`git::MAX_REF_BYTES`
    // caps what `.git/HEAD` can make a branch name in a checkout garnish did
    // not create), because there is no cheap way to count clusters without
    // building them.
    if name.chars().take(max.saturating_add(1)).count() <= max {
        return name.to_owned();
    }
    let clusters = crate::ansi::clusters(name);
    if clusters.len() <= max {
        return name.to_owned();
    }
    let ellipsis: String = icons.ellipsis().chars().take(max).collect();
    let mut out: String =
        clusters.into_iter().take(max.saturating_sub(ellipsis.chars().count())).collect();
    out.push_str(&ellipsis);
    out
}

/// The first seven characters of a commit hash, as git abbreviates one.
#[must_use]
pub fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
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

    /// SPEC § 3.1, § 3.5: `max_length` counts terminal clusters, never
    /// splits one, ends in the icon set's own ellipsis, and the ellipsis is
    /// inside the budget — so an ascii-only line never grows a `…` and half
    /// a flag never reaches a row.
    #[test]
    fn cut_name_counts_clusters_and_follows_the_icon_set() {
        use IconSet::{Ascii, Unicode};
        assert_eq!(cut_name("feature/long-branch", 0, Unicode), "feature/long-branch");
        assert_eq!(cut_name("short", 5, Unicode), "short");
        assert_eq!(cut_name("abcdef", 5, Unicode), "abcd…");
        assert_eq!(cut_name("abcdef", 5, Ascii), "abc..");
        // The ellipsis itself is cut when the budget is smaller than it.
        assert_eq!(cut_name("abcdef", 1, Ascii), ".");
        assert_eq!(cut_name("abcdef", 1, Unicode), "…");
        assert_eq!(cut_name("abcdef", 2, Ascii), "..");
        // A flag is two code points and one cluster; `é` as e + U+0301 is
        // two code points and one cluster. Neither is ever split.
        assert_eq!(cut_name("🇺🇸🇫🇷🇩🇪", 2, Unicode), "🇺🇸…");
        assert_eq!(cut_name("e\u{301}e\u{301}e\u{301}", 3, Unicode), "e\u{301}e\u{301}e\u{301}");
        assert_eq!(cut_name("e\u{301}e\u{301}e\u{301}", 2, Unicode), "e\u{301}…");
        for icons in IconSet::ALL {
            for max in 1..8_usize {
                let cut = cut_name("🇺🇸abcdef", max, icons);
                assert!(
                    crate::ansi::clusters(&cut).len() <= max,
                    "{} max={max}: {cut:?}",
                    icons.name()
                );
            }
        }
    }

    #[test]
    fn short_sha_abbreviates_like_git() {
        assert_eq!(short_sha("0123456789abcdef"), "0123456");
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha(""), "");
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
    fn marker_always_wins_its_cell() {
        let c = Color::Default;
        assert_eq!(text(&bar(10, 30.0, "█", "░", c, c, Some((90.0, "▏", c)))), "███░░░░░░▏");
        // marker over the partial cell and over a full cell stays visible
        assert_eq!(text(&bar(10, 95.0, "█", "░", c, c, Some((90.0, "▏", c)))), "█████████▏");
        assert_eq!(text(&bar(10, 100.0, "█", "░", c, c, Some((90.0, "▏", c)))), "█████████▏");
        assert_eq!(
            text(&bar(20, 96.0, "█", "░", c, c, Some((98.7, "▏", c)))),
            "███████████████████▏"
        );
        // marker at 100% sits in the last cell
        assert_eq!(text(&bar(5, 0.0, "█", "░", c, c, Some((100.0, "|", c)))), "░░░░|");
        // tiny bars
        assert_eq!(text(&bar(1, 50.0, "█", "░", c, c, Some((90.0, "▏", c)))), "▏");
        assert_eq!(text(&bar(2, 50.0, "█", "░", c, c, Some((90.0, "▏", c)))), "█▏");
        assert_eq!(text(&bar(3, 50.0, "█", "░", c, c, None)), "█▌░");
        // an empty marker glyph draws nothing
        assert_eq!(text(&bar(4, 0.0, "█", "░", c, c, Some((50.0, "", c)))), "░░░░");
    }

    #[test]
    fn multi_cell_glyphs_fall_back_so_the_bar_stays_the_right_width() {
        let c = Color::Default;
        let out = text(&bar(6, 50.0, "🟩", "..", c, c, Some((90.0, "|>", c))));
        assert_eq!(crate::ansi::display_width(&out), 6, "{out}");
        assert_eq!(out, "███░░|");
    }

    #[test]
    fn bar_width_is_capped_below_the_parser() {
        // A resolved size that slipped past config validation (a future
        // caller) still cannot spin the loop: MAX_CELLS is the ceiling.
        let cells: usize = bar(usize::MAX, 50.0, "█", "░", Color::Default, Color::Default, None)
            .iter()
            .map(Segment::width)
            .sum();
        assert_eq!(cells, crate::config::MAX_CELLS);
        assert_eq!(bar(0, 50.0, "█", "░", Color::Default, Color::Default, None), Vec::new());
    }

    #[test]
    fn bar_shorthand_yields_line_glyphs_unless_an_icon_is_explicit() {
        use crate::config::schema::{ModuleCfg, Overrides, Preset, Value};
        use crate::icons::IconSet;
        use crate::modules::Module;
        use crate::theme::Theme;
        let schema = crate::modules::context::ContextModule.schema();
        let theme = Theme::default();
        let resolve = |ov: &Overrides| {
            ModuleCfg::resolve(&schema, Preset::Default, IconSet::Nerd, &theme, ov)
        };
        assert_eq!(icon_pair(&resolve(&Overrides::default())), ("█", "░"));
        let mut line = Overrides::default();
        line.opts.insert("bar".into(), Value::Str("line".into()));
        assert_eq!(icon_pair(&resolve(&line)), ("━", "─"));
        // A line bar has no fractional cell, so the hairline gap cannot appear.
        let c = Color::Default;
        assert_eq!(text(&bar(8, 55.0, "━", "─", c, c, None)), "━━━━────");
        line.icons.insert("fill".into(), "▰".into());
        assert_eq!(
            icon_pair(&resolve(&line)),
            ("▰", "─"),
            "explicit fill wins, empty follows the shorthand"
        );
    }

    /// The shorthand is resolved, not rendered: `spend` gets it through the
    /// shared limit schema, the ascii set stays ascii, and `config show`
    /// prints the resolved glyphs so its output parses back identically.
    #[test]
    fn bar_shorthand_covers_spend_ascii_and_the_show_round_trip() {
        use crate::modules::SCHEMAS;
        let text = "icons = \"ascii\"\n[modules.spend]\nbar = \"line\"\n[modules.context]\nbar = \"line\"\n";
        let (cfg, errs) = crate::config::parse(text, &SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(icon_pair(cfg.modules.get("spend").unwrap()), ("=", "-"));
        assert_eq!(icon_pair(cfg.modules.get("context").unwrap()), ("=", "-"));
        let (unicode, _) = crate::config::parse("[modules.spend]\nbar = \"line\"\n", &SCHEMAS);
        assert_eq!(icon_pair(unicode.modules.get("spend").unwrap()), ("━", "─"));
        let shown = crate::docs::config_toml(&unicode, false);
        assert!(shown.contains("fill = \"━\""), "{shown}");
        let (again, errs) = crate::config::parse(&shown, &SCHEMAS);
        assert_eq!(errs, Vec::new());
        // `show` pins the animation switch in effect (SPEC § 4.2).
        let mut expected = unicode.clone();
        expected.animate = Some(unicode.animate.unwrap_or(true));
        assert_eq!(again, expected);
    }

    fn icon_pair(cfg: &crate::config::schema::ModuleCfg) -> (&str, &str) {
        (cfg.icon("fill"), cfg.icon("empty"))
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(percent(41.6), "42%");
        assert_eq!(percent(140.0), "100%");
        assert_eq!(rounded(89.6), 90.0);
        assert_eq!(rounded(f64::NAN), 0.0);
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
