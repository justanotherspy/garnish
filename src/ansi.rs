//! ANSI styling, OSC 8 hyperlinks, display width and width-aware truncation.
//!
//! Rendering produces [`Segment`]s (text + style). Styles are resolved to
//! escape sequences only at the very end, by [`Painter`], so tests can assert
//! on plain text and the color mode can be switched without touching modules.

use std::fmt::Write as _;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A color, in any of the forms the config accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// No color (terminal default).
    #[default]
    Default,
    /// One of the 16 named ANSI colors (0..=15).
    Ansi(u8),
    /// 256-color palette index.
    Indexed(u8),
    /// 24-bit RGB.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Parse `"red"`, `"bright-blue"`, `"208"`, or `"#rrggbb"`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            let mut chars = hex.chars();
            let mut byte = || {
                let hi = chars.next()?.to_digit(16)?;
                let lo = chars.next()?.to_digit(16)?;
                u8::try_from(hi.checked_mul(16)?.checked_add(lo)?).ok()
            };
            let (r, g, b) = (byte()?, byte()?, byte()?);
            return if chars.next().is_none() { Some(Self::Rgb(r, g, b)) } else { None };
        }
        if let Ok(n) = s.parse::<u8>() {
            return Some(Self::Indexed(n));
        }
        let named = match s.to_ascii_lowercase().as_str() {
            "default" | "none" => return Some(Self::Default),
            "black" => 0,
            "red" => 1,
            "green" => 2,
            "yellow" => 3,
            "blue" => 4,
            "magenta" => 5,
            "cyan" => 6,
            "white" => 7,
            "bright-black" | "gray" | "grey" => 8,
            "bright-red" => 9,
            "bright-green" => 10,
            "bright-yellow" => 11,
            "bright-blue" => 12,
            "bright-magenta" => 13,
            "bright-cyan" => 14,
            "bright-white" => 15,
            _ => return None,
        };
        Some(Self::Ansi(named))
    }

    /// The config spelling of this color: `default`, a name, an index, or `#rrggbb`.
    #[must_use]
    pub fn to_spec(self) -> String {
        const NAMES: [&str; 16] = [
            "black",
            "red",
            "green",
            "yellow",
            "blue",
            "magenta",
            "cyan",
            "white",
            "bright-black",
            "bright-red",
            "bright-green",
            "bright-yellow",
            "bright-blue",
            "bright-magenta",
            "bright-cyan",
            "bright-white",
        ];
        match self {
            Self::Default => "default".to_owned(),
            Self::Ansi(n) => {
                NAMES.get(usize::from(n)).map_or_else(|| n.to_string(), |s| (*s).to_owned())
            }
            Self::Indexed(n) => n.to_string(),
            Self::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }

    /// SGR parameters for this color as a foreground.
    fn fg_params(self, mode: ColorMode) -> Option<String> {
        match (self, mode) {
            (Self::Default, _) | (_, ColorMode::Never) => None,
            (Self::Ansi(n), _) => Some(if n < 8 {
                format!("{}", 30_u8.saturating_add(n))
            } else {
                format!("{}", 90_u8.saturating_add(n.saturating_sub(8)))
            }),
            (Self::Indexed(n), _) => Some(format!("38;5;{n}")),
            (Self::Rgb(r, g, b), ColorMode::TrueColor) => Some(format!("38;2;{r};{g};{b}")),
            (Self::Rgb(r, g, b), ColorMode::Ansi256) => {
                Some(format!("38;5;{}", rgb_to_256(r, g, b)))
            }
        }
    }
}

/// Approximate an RGB color with the 6×6×6 cube of the 256-color palette.
fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let q = |c: u8| -> u8 {
        // 0..=255 → 0..=5
        u8::try_from((u16::from(c).saturating_mul(5).saturating_add(127)) / 255).unwrap_or(5)
    };
    16_u8
        .saturating_add(q(r).saturating_mul(36))
        .saturating_add(q(g).saturating_mul(6))
        .saturating_add(q(b))
}

/// How colors are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// No escape sequences at all.
    Never,
    /// 256-color palette (RGB is approximated).
    Ansi256,
    /// 24-bit color.
    #[default]
    TrueColor,
}

/// Text attributes.
// Four independent flags; a bitset would only obscure the config mapping.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Foreground color.
    pub fg: Color,
    /// Bold.
    pub bold: bool,
    /// Dim / faint.
    pub dim: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
}

impl Style {
    /// Plain text.
    pub const PLAIN: Self =
        Self { fg: Color::Default, bold: false, dim: false, italic: false, underline: false };

    /// Style with only a foreground color.
    #[must_use]
    pub const fn fg(color: Color) -> Self {
        Self { fg: color, ..Self::PLAIN }
    }

    /// Copy with `dim` set.
    #[must_use]
    pub const fn dimmed(self) -> Self {
        Self { dim: true, ..self }
    }

    /// Copy with `bold` set.
    #[must_use]
    pub const fn bolded(self) -> Self {
        Self { bold: true, ..self }
    }

    /// Copy with `underline` set when `on`.
    #[must_use]
    pub const fn underline_if(self, on: bool) -> Self {
        Self { underline: on, ..self }
    }

    fn sgr(self, mode: ColorMode) -> String {
        if mode == ColorMode::Never {
            return String::new();
        }
        let mut params: Vec<String> = Vec::new();
        if self.bold {
            params.push("1".into());
        }
        if self.dim {
            params.push("2".into());
        }
        if self.italic {
            params.push("3".into());
        }
        if self.underline {
            params.push("4".into());
        }
        if let Some(fg) = self.fg.fg_params(mode) {
            params.push(fg);
        }
        if params.is_empty() { String::new() } else { format!("\x1b[{}m", params.join(";")) }
    }
}

/// A run of text with one style and an optional hyperlink.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Segment {
    /// The text, without escape sequences.
    pub text: String,
    /// Style applied to the whole run.
    pub style: Style,
    /// OSC 8 target, if any.
    pub link: Option<String>,
}

impl Segment {
    /// Unstyled text. The text is reduced to plain text ([`plain_text`]): a
    /// segment is the one way onto a row, so nothing a payload, a git
    /// command or a config contributes can carry an escape sequence, a
    /// control character or a bidi override past this point.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: clean(text.into()), style: Style::PLAIN, link: None }
    }

    /// Styled text, sanitised like [`Segment::plain`].
    #[must_use]
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self { text: clean(text.into()), style, link: None }
    }

    /// Attach a hyperlink.
    #[must_use]
    pub fn with_link(mut self, url: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self
    }

    /// Display width of the text.
    #[must_use]
    pub fn width(&self) -> usize {
        display_width(&self.text)
    }
}

/// Display width of a string in terminal cells (no escape sequences expected).
///
/// Uses `unicode-width`'s string algorithm, which understands emoji
/// presentation sequences (VS16), ZWJ sequences and combining marks; private
/// use glyphs (Nerd Font icons) count as one cell.
#[must_use]
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Width of one character in cells (use [`display_width`] for text).
#[must_use]
pub fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

/// Split text into terminal clusters that must not be separated: a base
/// character plus any following zero-width characters (combining marks,
/// variation selectors), and anything joined by U+200D ZERO WIDTH JOINER.
fn clusters(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut joined = false;
    for c in s.chars() {
        let attach = joined || (char_width(c) == 0 && !out.is_empty());
        match out.last_mut() {
            Some(last) if attach => last.push(c),
            _ => out.push(c.to_string()),
        }
        joined = c == '\u{200d}';
    }
    out
}

/// Sum of segment widths.
#[must_use]
pub fn segments_width(segments: &[Segment]) -> usize {
    segments.iter().map(Segment::width).sum()
}

/// Truncate segments to at most `max_width` cells, appending `ellipsis` when
/// anything was cut. Never splits a character. Returns the new segments.
#[must_use]
pub fn truncate(segments: &[Segment], max_width: usize, ellipsis: &str) -> Vec<Segment> {
    if segments_width(segments) <= max_width {
        return segments.to_vec();
    }
    // A box narrower than the ellipsis still gets a mark: as much of the
    // ellipsis as fits (`..` becomes `.` in a one-cell ascii box).
    let ellipsis = fit(ellipsis, max_width);
    let ell_width = display_width(ellipsis);
    let budget = max_width.saturating_sub(ell_width);
    let mut out: Vec<Segment> = Vec::new();
    let mut used = 0_usize;
    'outer: for seg in segments {
        let mut kept = String::new();
        for cluster in clusters(&seg.text) {
            let w = display_width(&cluster);
            if used.saturating_add(w) > budget {
                if !kept.is_empty() {
                    out.push(Segment { text: kept, style: seg.style, link: seg.link.clone() });
                }
                break 'outer;
            }
            used = used.saturating_add(w);
            kept.push_str(&cluster);
        }
        out.push(Segment { text: kept, style: seg.style, link: seg.link.clone() });
    }
    if ell_width > 0 && max_width >= ell_width {
        let style = out.last().map_or(Style::PLAIN, |s| s.style);
        out.push(Segment::styled(ellipsis, style));
    }
    out
}

/// The longest prefix of `s` that is at most `width` cells.
fn fit(s: &str, width: usize) -> &str {
    let mut used = 0_usize;
    let mut end = 0_usize;
    for (i, c) in s.char_indices() {
        used = used.saturating_add(char_width(c));
        if used > width {
            break;
        }
        end = i.saturating_add(c.len_utf8());
    }
    s.get(..end).unwrap_or("")
}

/// One terminal cluster of a segment, with the style it came from.
struct Cell<'a> {
    text: String,
    width: usize,
    style: Style,
    link: Option<&'a str>,
}

fn cells(segments: &[Segment]) -> Vec<Cell<'_>> {
    segments
        .iter()
        .flat_map(|seg| {
            clusters(&seg.text).into_iter().map(move |text| Cell {
                width: display_width(&text),
                text,
                style: seg.style,
                link: seg.link.as_deref(),
            })
        })
        .collect()
}

/// A `width`-cell window onto `segments` starting `offset` cells in: the
/// scroller behind text modules and the line ticker (SPEC § 3.7, § 4.1).
///
/// With `wrap`, the text is followed by `gap` and then itself again, so the
/// window flows continuously and the offset is taken modulo the width of
/// text plus gap. Without `wrap`, the window slides over the text once; the
/// offset is taken modulo the text width, so once the end has scrolled past
/// the view restarts at the beginning, and cells past the end are blank.
/// Text no wider than the window is returned as is, padded on the right.
/// The result is always exactly `width` cells: a wide cluster cut by either
/// edge becomes spaces for its visible part. Styles and links follow their
/// clusters; the gap and padding are plain. The period is the sum of the
/// cluster widths, which for ligature scripts (Arabic `لا`, Lisu tone pairs)
/// can exceed [`display_width`] of the whole string by a cell; callers that
/// compute the period with `display_width` then see the window jump a cell
/// at the wrap, the same limitation [`truncate`] has.
#[must_use]
pub fn scroll(
    segments: &[Segment],
    width: usize,
    offset: usize,
    gap: &str,
    wrap: bool,
) -> Vec<Segment> {
    if width == 0 {
        return Vec::new();
    }
    let text_w = segments_width(segments);
    if !wrap && text_w <= width {
        let mut out = segments.to_vec();
        let pad = width.saturating_sub(text_w);
        if pad > 0 {
            out.push(Segment::plain(" ".repeat(pad)));
        }
        return out;
    }
    let gap_segment = [Segment::plain(gap)];
    let mut sequence = cells(segments);
    if wrap {
        sequence.extend(cells(&gap_segment));
    }
    let period: usize = sequence.iter().map(|c| c.width).sum();
    if period == 0 {
        return vec![Segment::plain(" ".repeat(width))];
    }
    let offset = offset.checked_rem(period).unwrap_or(0);
    let end = offset.saturating_add(width);

    let mut out: Vec<Segment> = Vec::new();
    let mut push = |text: &str, style: Style, link: Option<&str>| match out.last_mut() {
        Some(last) if last.style == style && last.link.as_deref() == link => {
            last.text.push_str(text);
        }
        _ => out.push(Segment { text: text.to_owned(), style, link: link.map(str::to_owned) }),
    };
    let mut start = 0_usize;
    let mut rounds = 0_usize;
    let mut emitted = 0_usize;
    // At most two passes over the sequence are ever needed: the window is
    // narrower than one period plus itself.
    'outer: while rounds < 2 || (wrap && start < end) {
        for cell in &sequence {
            let stop = start.saturating_add(cell.width);
            if start >= end {
                break 'outer;
            }
            if stop > offset {
                let visible_from = start.max(offset);
                let visible_to = stop.min(end);
                if visible_from == start && visible_to == stop {
                    push(&cell.text, cell.style, cell.link);
                    emitted = emitted.saturating_add(cell.width);
                } else {
                    let cut = visible_to.saturating_sub(visible_from);
                    push(&" ".repeat(cut), Style::PLAIN, None);
                    emitted = emitted.saturating_add(cut);
                }
            }
            start = stop;
        }
        rounds = rounds.saturating_add(1);
        if !wrap {
            break;
        }
    }
    if emitted < width {
        push(&" ".repeat(width.saturating_sub(emitted)), Style::PLAIN, None);
    }
    out
}

/// Turns segments into a string with escape sequences.
#[derive(Debug, Clone, Copy)]
pub struct Painter {
    /// Color mode.
    pub mode: ColorMode,
    /// Emit OSC 8 hyperlinks.
    pub links: bool,
}

impl Painter {
    /// Painter that emits nothing but text.
    pub const PLAIN: Self = Self { mode: ColorMode::Never, links: false };

    /// Render segments to a single line (no trailing newline).
    #[must_use]
    pub fn paint(&self, segments: &[Segment]) -> String {
        let mut out = String::new();
        for seg in segments {
            if seg.text.is_empty() {
                continue;
            }
            let sgr = seg.style.sgr(self.mode);
            let link = seg.link.as_deref().filter(|u| self.links && safe_link(u));
            if let Some(url) = link {
                let _ = write!(out, "\x1b]8;;{url}\x1b\\");
            }
            out.push_str(&sgr);
            out.push_str(&seg.text);
            if !sgr.is_empty() {
                out.push_str("\x1b[0m");
            }
            if link.is_some() {
                out.push_str("\x1b]8;;\x1b\\");
            }
        }
        out
    }
}

/// An OSC 8 target garnish is willing to emit: `http(s)://` and printable
/// ASCII only, so a URL can never close the sequence early (`ESC \`, BEL)
/// or name a scheme a terminal might act on.
#[must_use]
pub fn safe_link(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// Plain text only: escape sequences, control characters and invisible
/// format characters removed.
///
/// Every string that reaches a row goes through here, at config time for
/// the config's own strings and in [`Segment::plain`]/[`Segment::styled`]
/// for everything else, so a cut window can never split an escape sequence
/// and leak colour or a bare ESC into the row, a newline can never add a
/// row, and a bidi override can never make `main` read as something else
/// (SPEC § 3.7). Zero-width joiner and the emoji variation selector stay:
/// they are part of how glyphs are spelled.
#[must_use]
pub fn plain_text(s: &str) -> String {
    clean(s.to_owned())
}

/// [`plain_text`] without an allocation when the text is already plain,
/// which on a warm tick is every string.
fn clean(s: String) -> String {
    if s.chars().any(|c| c == '\x1b' || c.is_control() || is_format_char(c)) {
        strip_ansi(&s).chars().filter(|&c| !c.is_control() && !is_format_char(c)).collect()
    } else {
        s
    }
}

/// Unicode `Cf` characters that change layout or reading order without
/// occupying a cell: zero-width space/non-joiner, the bidi marks and
/// embeddings/isolates, word joiner and friends, the byte order mark.
const fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{200b}' | '\u{200c}' | '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{180e}' | '\u{feff}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// Remove ANSI CSI and OSC sequences from a string (used by docs and tests).
#[must_use]
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // CSI: consume until a final byte in 0x40..=0x7E
                for n in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&n) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: consume until BEL or ESC \
                let mut prev_esc = false;
                for n in chars.by_ref() {
                    if n == '\u{7}' || (prev_esc && n == '\\') {
                        break;
                    }
                    prev_esc = n == '\x1b';
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colors() {
        assert_eq!(Color::parse("red"), Some(Color::Ansi(1)));
        assert_eq!(Color::parse("bright-blue"), Some(Color::Ansi(12)));
        assert_eq!(Color::parse("208"), Some(Color::Indexed(208)));
        assert_eq!(Color::parse("#ff8800"), Some(Color::Rgb(255, 136, 0)));
        assert_eq!(Color::parse("#ff880"), None);
        assert_eq!(Color::parse("chartreuse"), None);
        assert_eq!(Color::parse("none"), Some(Color::Default));
        for spec in ["default", "red", "bright-blue", "208", "#ff8800"] {
            let c = Color::parse(spec).unwrap();
            assert_eq!(c.to_spec(), spec);
            assert_eq!(Color::parse(&c.to_spec()), Some(c));
        }
    }

    #[test]
    fn widths_count_nerd_glyphs_as_one_and_emoji_as_two() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("\u{e725}"), 1);
        assert_eq!(display_width("\u{f06a9}"), 1);
        assert_eq!(display_width("⏱"), 1);
        assert_eq!(display_width("⏱\u{fe0f}"), 2);
        assert_eq!(display_width("🌿"), 2);
        assert_eq!(display_width("👨\u{200d}💻"), 2);
        assert_eq!(display_width("e\u{301}"), 1);
        assert_eq!(display_width("日本"), 4);
        assert_eq!(display_width("█░▏"), 3);
    }

    #[test]
    fn truncation_never_splits_a_cluster() {
        let segs = vec![Segment::plain("a👨\u{200d}💻b⏱\u{fe0f}c")];
        // widths: a=1, family=2, b=1, timer=2, c=1 → total 7
        assert_eq!(segments_width(&segs), 7);
        assert_eq!(Painter::PLAIN.paint(&truncate(&segs, 3, "…")), "a…");
        assert_eq!(Painter::PLAIN.paint(&truncate(&segs, 4, "…")), "a👨\u{200d}💻…");
        assert_eq!(Painter::PLAIN.paint(&truncate(&segs, 6, "…")), "a👨\u{200d}💻b…");
        assert_eq!(segments_width(&truncate(&segs, 6, "…")), 5);
        let cjk = vec![Segment::plain("日本語")];
        assert_eq!(Painter::PLAIN.paint(&truncate(&cjk, 4, "…")), "日…");
    }

    #[test]
    fn truncation_keeps_style_and_adds_ellipsis() {
        let segs =
            vec![Segment::plain("hello "), Segment::styled("world", Style::fg(Color::Ansi(1)))];
        let t = truncate(&segs, 8, "…");
        assert_eq!(Painter::PLAIN.paint(&t), "hello w…");
        assert_eq!(segments_width(&t), 8);
        assert_eq!(t.last().unwrap().style.fg, Color::Ansi(1));
        // no-op when it fits
        assert_eq!(truncate(&segs, 11, "…"), segs);
        // width zero yields nothing
        assert_eq!(Painter::PLAIN.paint(&truncate(&segs, 0, "…")), "");
    }

    fn plain(segs: &[Segment]) -> String {
        Painter::PLAIN.paint(segs)
    }

    #[test]
    fn scroll_windows_are_exactly_the_requested_width() {
        // Mixed one- and two-cell clusters, several styles, a link.
        let segs = vec![
            Segment::styled("ab", Style::fg(Color::Ansi(1))),
            Segment::plain("🌿"),
            Segment::styled("cd", Style::fg(Color::Ansi(2))).with_link("https://x"),
            Segment::plain("e⏱\u{fe0f}f"),
        ];
        let text_w = segments_width(&segs);
        assert_eq!(text_w, 10);
        for wrap in [false, true] {
            for width in 1..=14 {
                for offset in 0..30 {
                    let out = scroll(&segs, width, offset, " · ", wrap);
                    assert_eq!(
                        segments_width(&out),
                        width,
                        "wrap={wrap} width={width} offset={offset}: {:?}",
                        plain(&out)
                    );
                }
            }
        }
        assert_eq!(scroll(&segs, 0, 3, "", true), Vec::new());
        assert_eq!(plain(&scroll(&[], 4, 7, "", true)), "    ");
    }

    #[test]
    fn scroll_slides_restarts_wraps_and_keeps_styles() {
        let segs = vec![
            Segment::styled("abc", Style::fg(Color::Ansi(1))),
            Segment::plain("🌿"),
            Segment::styled("d", Style::fg(Color::Ansi(2))).with_link("https://x"),
        ];
        // width 6, text 6: fits without wrap → unchanged (padded when wider).
        assert_eq!(scroll(&segs, 6, 3, "", false), segs);
        assert_eq!(plain(&scroll(&segs, 8, 3, "", false)), "abc🌿d  ");
        // Sliding window without wrap: cells past the end are blank, and the
        // offset restarts after the whole text (period 6) has gone by.
        assert_eq!(plain(&scroll(&segs, 4, 0, "", false)), "abc ");
        assert_eq!(plain(&scroll(&segs, 4, 1, "", false)), "bc🌿");
        assert_eq!(plain(&scroll(&segs, 4, 2, "", false)), "c🌿d");
        assert_eq!(plain(&scroll(&segs, 4, 3, "", false)), "🌿d ");
        assert_eq!(plain(&scroll(&segs, 4, 4, "", false)), " d  ", "the leaf is cut: blank");
        assert_eq!(plain(&scroll(&segs, 4, 5, "", false)), "d   ");
        assert_eq!(plain(&scroll(&segs, 4, 6, "", false)), "abc ", "restart");
        // The ticker: text, gap, text again, flowing round (period 6 + 3).
        assert_eq!(plain(&scroll(&segs, 4, 0, " · ", true)), "abc ");
        assert_eq!(plain(&scroll(&segs, 4, 5, " · ", true)), "d · ");
        assert_eq!(plain(&scroll(&segs, 4, 7, " · ", true)), "· ab");
        assert_eq!(plain(&scroll(&segs, 4, 9, " · ", true)), "abc ", "one period later");
        assert_eq!(plain(&scroll(&segs, 12, 0, " · ", true)), "abc🌿d · abc");
        // Styles and the link travel with their clusters; the gap is plain.
        let out = scroll(&segs, 4, 2, " · ", true);
        assert_eq!(out[0].style.fg, Color::Ansi(1), "{out:?}");
        assert_eq!(out[1].text, "🌿");
        assert_eq!(out[2].link.as_deref(), Some("https://x"));
        let out = scroll(&segs, 4, 5, " · ", true);
        assert_eq!(out[1].style, Style::PLAIN, "gap is plain: {out:?}");
    }

    #[test]
    fn plain_text_strips_escapes_and_controls_but_keeps_text() {
        assert_eq!(plain_text("ship it"), "ship it");
        assert_eq!(plain_text("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(plain_text("\x1b]8;;https://x\x1b\\link\x1b]8;;\x1b\\"), "link");
        assert_eq!(plain_text("a\tb\nc\u{7}d"), "abcd");
        assert_eq!(plain_text("\x1b"), "", "a bare ESC never reaches the row");
        assert_eq!(
            plain_text("🌿 e\u{301} 👨\u{200d}💻 ☁\u{fe0f}"),
            "🌿 e\u{301} 👨\u{200d}💻 ☁\u{fe0f}",
            "marks, ZWJ and VS16 stay"
        );
        // Bidi overrides, zero-width spaces and the BOM are dropped: a branch
        // called `niam\u{202e}` must not read as `main`.
        assert_eq!(plain_text("\u{feff}ni\u{200b}am\u{202e} \u{2066}x\u{2069}"), "niam x");
        assert_eq!(plain_text("\u{200e}\u{200f}\u{061c}\u{2060}\u{180e}"), "");
    }

    #[test]
    fn segments_are_plain_text_by_construction() {
        // Every string reaches a row through these constructors, so payload
        // and git strings cannot inject escapes, controls or a second row.
        assert_eq!(Segment::plain("\x1b[31mred\x1b[0m\nrow2").text, "redrow2");
        assert_eq!(Segment::styled("a\u{7}\x1b]0;title\x1b\\b", Style::PLAIN).text, "ab");
        assert_eq!(Segment::plain("\x1b]52;c;aGVsbG8=\x07x").text, "x", "OSC 52 clipboard");
        assert_eq!(Segment::plain("plain 🌿").text, "plain 🌿");
        // A cut can no longer land inside an escape sequence: there is none.
        let cut = truncate(&[Segment::plain("abc\x1b[31mdefghij")], 5, "…");
        assert_eq!(Painter::PLAIN.paint(&cut), "abcd…");
    }

    #[test]
    fn painter_emits_sgr_and_osc8() {
        let seg =
            Segment::styled("PR", Style::fg(Color::Rgb(1, 2, 3)).bolded()).with_link("https://x");
        let painter = Painter { mode: ColorMode::TrueColor, links: true };
        let s = painter.paint(std::slice::from_ref(&seg));
        assert_eq!(s, "\x1b]8;;https://x\x1b\\\x1b[1;38;2;1;2;3mPR\x1b[0m\x1b]8;;\x1b\\");
        assert_eq!(strip_ansi(&s), "PR");
        let p256 = Painter { mode: ColorMode::Ansi256, links: false };
        assert_eq!(p256.paint(&[seg]), "\x1b[1;38;5;16mPR\x1b[0m");
        assert_eq!(Painter::PLAIN.paint(&[Segment::styled("x", Style::PLAIN.dimmed())]), "x");
    }

    #[test]
    fn painter_drops_unsafe_links() {
        let painter = Painter { mode: ColorMode::Never, links: true };
        let paint = |url: &str| painter.paint(&[Segment::plain("#42").with_link(url)]);
        assert_eq!(
            paint("https://github.com/o/r/pull/42"),
            "\x1b]8;;https://github.com/o/r/pull/42\x1b\\#42\x1b]8;;\x1b\\"
        );
        assert!(paint("http://gitlab.local/o/r/-/merge_requests/7").contains("]8;;http://"));
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://x\x1b\\\x1b[31mINJECT",
            "https://x\u{7}y",
            "https://ex ample.com",
            "https://ünïcode.example",
            "ftp://x",
            "",
        ] {
            assert_eq!(paint(bad), "#42", "{bad:?} must not become a link");
            assert!(!safe_link(bad), "{bad:?}");
        }
    }

    #[test]
    fn truncation_marks_a_box_narrower_than_the_ellipsis() {
        let seg = [Segment::plain("abcdef")];
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg, 1, "..")), ".");
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg, 2, "..")), "..");
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg, 3, "..")), "a..");
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg, 1, "…")), "…");
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg, 0, "…")), "");
        assert_eq!(fit("..", 1), ".");
        assert_eq!(fit("🌿x", 1), "");
        assert_eq!(fit("🌿x", 2), "🌿");
    }

    #[test]
    fn rgb_cube_mapping() {
        assert_eq!(rgb_to_256(0, 0, 0), 16);
        assert_eq!(rgb_to_256(255, 255, 255), 231);
        assert_eq!(rgb_to_256(255, 0, 0), 196);
    }
}
