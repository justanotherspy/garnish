//! ANSI styling, OSC 8 hyperlinks, plain-text sanitising, display width,
//! terminal clusters, and the width-aware cut and scroller.
//!
//! Rendering produces [`Segment`]s (text + style), whose text is reduced to
//! plain text on the way in ([`plain_text`]). Styles are resolved to escape
//! sequences only at the very end, by [`Painter`], so tests can assert on
//! plain text and the color mode can be switched without touching modules.
//! [`truncate`] and [`scroll`] work in terminal clusters, never splitting a
//! glyph, and [`scroll_period`] is the one period their callers count with.

use std::borrow::Cow;
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

    /// Write the SGR parameters for this color as a foreground onto `out`
    /// (nothing for the default colour or under [`ColorMode::Never`]).
    fn write_fg(self, mode: ColorMode, out: &mut String) {
        let _ = match (self, mode) {
            (Self::Default, _) | (_, ColorMode::Never) => Ok(()),
            (Self::Ansi(n), _) if n < 8 => write!(out, "{}", 30_u8.saturating_add(n)),
            (Self::Ansi(n), _) => write!(out, "{}", 90_u8.saturating_add(n.saturating_sub(8))),
            (Self::Indexed(n), _) => write!(out, "38;5;{n}"),
            (Self::Rgb(r, g, b), ColorMode::TrueColor) => write!(out, "38;2;{r};{g};{b}"),
            (Self::Rgb(r, g, b), ColorMode::Ansi256) => {
                write!(out, "38;5;{}", rgb_to_256(r, g, b))
            }
        };
    }
}

/// The six channel values of the 256-colour palette's 6×6×6 cube.
///
/// xterm spaces them unevenly — `0` then `55 + 40 i` — so dividing the range
/// into six equal parts picks the wrong level for most of 96..130 and
/// 176..214. Every built-in palette is written in `#rrggbb`, so under
/// `color = "256"` that error moved whole themes: `#6c7086` (muted) came out
/// as `rgb(135,135,175)`, a light blue-grey, instead of `rgb(95,95,135)`.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Approximate an RGB color with the 256-color palette: the nearest level of
/// the 6×6×6 cube per channel, and where that answer is a gray, the nearer
/// of it and the 24-step grayscale ramp (232..=255, `8 + 10 i`).
///
/// The ramp is only a candidate where the cube has nothing but a gray to
/// offer: a colour the cube keeps a hue for (`#6c7086` → 60) stays in the
/// cube, while a dark near-gray (every built-in `frame` role) is no longer
/// lightened to the cube's 95.
fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let level_of = |channel: u8| -> u8 {
        let nearest = CUBE_LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, level)| u16::from(channel).abs_diff(u16::from(**level)))
            .map_or(0, |(index, _)| index);
        u8::try_from(nearest).unwrap_or(5)
    };
    let (red, green, blue) = (level_of(r), level_of(g), level_of(b));
    let cube = 16_u8
        .saturating_add(red.saturating_mul(36))
        .saturating_add(green.saturating_mul(6))
        .saturating_add(blue);
    if red != green || green != blue {
        return cube;
    }
    let gray = CUBE_LEVELS.get(usize::from(red)).copied().unwrap_or(0);
    let ramp = (0..24_u8)
        .map(|step| (distance([r, g, b], 8_u8.saturating_add(step.saturating_mul(10))), step))
        .min();
    match ramp {
        Some((nearest, step)) if nearest < distance([r, g, b], gray) => 232_u8.saturating_add(step),
        _ => cube,
    }
}

/// Squared distance from a colour to the gray of one level.
fn distance(rgb: [u8; 3], level: u8) -> u32 {
    rgb.into_iter()
        .map(|c| u32::from(c.abs_diff(level)))
        .fold(0, |sum, d| sum.saturating_add(d.saturating_mul(d)))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Foreground color.
    pub fg: Color,
    /// Bold.
    pub bold: bool,
    /// Dim / faint.
    pub dim: bool,
    /// Underline.
    pub underline: bool,
}

impl Style {
    /// Plain text.
    pub const PLAIN: Self = Self { fg: Color::Default, bold: false, dim: false, underline: false };

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

    /// Write this style's SGR sequence onto `out`, and say whether there was
    /// one (so the painter knows to reset after the text). Written in
    /// place: every painted segment of every tick comes through here.
    fn write_sgr(self, mode: ColorMode, out: &mut String) -> bool {
        if mode == ColorMode::Never {
            return false;
        }
        let mut open = false;
        let mut param = |out: &mut String| {
            out.push_str(if open { ";" } else { "\x1b[" });
            open = true;
        };
        for (on, code) in [(self.bold, "1"), (self.dim, "2"), (self.underline, "4")] {
            if on {
                param(out);
                out.push_str(code);
            }
        }
        if self.fg != Color::Default {
            param(out);
            self.fg.write_fg(mode, out);
        }
        if open {
            out.push('m');
        }
        open
    }
}

/// A run of text with one style and an optional hyperlink.
///
/// The text is private so the plain-text invariant of SPEC § 5 is held by
/// the type, not by convention: every way to set it ([`Segment::plain`],
/// [`Segment::styled`], [`Segment::with_text`], [`Segment::push_str`]) runs
/// it through [`plain_text`], and [`Segment::text`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Segment {
    /// The text, without escape sequences.
    text: String,
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

    /// The same style and link with other text, sanitised like
    /// [`Segment::plain`].
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = clean(text.into());
        self
    }

    /// The text: plain, without escape sequences or control characters.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Append text, sanitised like [`Segment::plain`] (no allocation when
    /// it is already plain: the bar builder appends a glyph per cell).
    pub fn push_str(&mut self, text: &str) {
        self.text.push_str(&plain_cow(text));
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
/// variation selectors), anything joined by U+200D ZERO WIDTH JOINER, a
/// skin-tone modifier with the emoji it modifies, and the two regional
/// indicators of a flag.
///
/// Cutting inside any of those changes the glyph rather than shortening it:
/// half a flag is a lone letter and a dropped skin tone is a different
/// person, so `truncate` and the fish path's initial both work in these
/// units, not in `char`s.
///
/// The clusters are slices of `s`, produced as they are asked for, so a cut
/// that keeps the first few clusters of a long string reads no further.
pub(crate) fn clusters(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = s;
    std::iter::from_fn(move || {
        let mut chars = rest.char_indices();
        let (_, first) = chars.next()?;
        let mut end = first.len_utf8();
        let mut joined = first == '\u{200d}';
        // A lone regional indicator waits for the one that completes its flag.
        let mut lone_regional = is_regional(first);
        for (i, c) in chars {
            if !(joined
                || char_width(c) == 0
                || is_emoji_modifier(c)
                || (lone_regional && is_regional(c)))
            {
                break;
            }
            end = i.saturating_add(c.len_utf8());
            joined = c == '\u{200d}';
            lone_regional = false;
        }
        let (cluster, tail) = rest.split_at_checked(end)?;
        rest = tail;
        Some(cluster)
    })
}

/// A skin-tone modifier, which belongs to the emoji before it.
const fn is_emoji_modifier(c: char) -> bool {
    matches!(c, '\u{1f3fb}'..='\u{1f3ff}')
}

/// A regional indicator letter; a flag is exactly two of them.
const fn is_regional(c: char) -> bool {
    matches!(c, '\u{1f1e6}'..='\u{1f1ff}')
}

/// Sum of segment widths.
#[must_use]
pub fn segments_width(segments: &[Segment]) -> usize {
    segments.iter().map(Segment::width).sum()
}

/// Sum of segment widths counted terminal cluster by cluster, the unit
/// [`truncate`] and [`scroll`] advance in.
///
/// It exceeds [`segments_width`] by a cell for each ligature pair
/// `unicode-width` measures as one cell (Arabic `لا`), which the cut and the
/// scroller split in two; whoever places an offset or a cut against them
/// counts this way too.
#[must_use]
pub fn cluster_width(segments: &[Segment]) -> usize {
    segments.iter().flat_map(|seg| clusters(&seg.text)).map(display_width).sum()
}

/// The cells after which a [`scroll`] of `segments` repeats.
///
/// The text's [`cluster_width`], plus the gap's with `wrap`. A caller
/// reduces its clock to an offset modulo this (`time::frame`), so the
/// offset, the window and anything mapped onto the window share one period.
#[must_use]
pub fn scroll_period(segments: &[Segment], gap: &str, wrap: bool) -> usize {
    let text = cluster_width(segments);
    if wrap {
        text.saturating_add(clusters(&plain_cow(gap)).map(display_width).sum())
    } else {
        text
    }
}

/// The cells of text [`truncate`] keeps when it cuts to `max_width`: what
/// the ellipsis, itself cut to fit, leaves.
#[must_use]
pub fn kept_width(max_width: usize, ellipsis: &str) -> usize {
    max_width.saturating_sub(display_width(fit(ellipsis, max_width)))
}

/// Truncate segments to at most `max_width` cells, appending `ellipsis` when
/// anything was cut. Never splits a character. Returns the new segments.
#[must_use]
pub fn truncate(segments: &[Segment], max_width: usize, ellipsis: &str) -> Vec<Segment> {
    if segments_width(segments) <= max_width {
        return segments.to_vec();
    }
    let budget = kept_width(max_width, ellipsis);
    // A box narrower than the ellipsis still gets a mark: as much of the
    // ellipsis as fits (`..` becomes `.` in a one-cell ascii box).
    let ellipsis = fit(ellipsis, max_width);
    let mut out: Vec<Segment> = Vec::new();
    let mut used = 0_usize;
    'outer: for seg in segments {
        let mut kept = String::new();
        for cluster in clusters(&seg.text) {
            let w = display_width(cluster);
            if used.saturating_add(w) > budget {
                if !kept.is_empty() {
                    out.push(Segment { text: kept, style: seg.style, link: seg.link.clone() });
                }
                break 'outer;
            }
            used = used.saturating_add(w);
            kept.push_str(cluster);
        }
        out.push(Segment { text: kept, style: seg.style, link: seg.link.clone() });
    }
    if display_width(ellipsis) > 0 {
        // The style of whatever survived the cut, or of the text that would
        // have been there: with a budget narrower than the first cluster
        // nothing survives, and an unstyled `…` would make a module flip to
        // the terminal default at exactly its narrowest setting.
        let style = out.last().or_else(|| segments.first()).map_or(Style::PLAIN, |s| s.style);
        out.push(Segment::styled(ellipsis, style));
    }
    out
}

/// The longest prefix of `s` that is at most `width` cells.
pub(crate) fn fit(s: &str, width: usize) -> &str {
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
    text: &'a str,
    width: usize,
    style: Style,
    link: Option<&'a str>,
}

fn cells(segments: &[Segment]) -> Vec<Cell<'_>> {
    segments
        .iter()
        .flat_map(|seg| {
            clusters(&seg.text).map(move |text| Cell {
                width: display_width(text),
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
/// clusters; the gap and padding are plain. The period is
/// [`scroll_period`], which a caller computes its offset with.
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
    let mut emitted = 0_usize;
    // Without wrap one pass; with it, as many as the window spans (each
    // advances a whole period, which is not zero).
    'outer: loop {
        for cell in &sequence {
            let stop = start.saturating_add(cell.width);
            if start >= end {
                break 'outer;
            }
            if stop > offset {
                let visible_from = start.max(offset);
                let visible_to = stop.min(end);
                if visible_from == start && visible_to == stop {
                    push(cell.text, cell.style, cell.link);
                    emitted = emitted.saturating_add(cell.width);
                } else {
                    let cut = visible_to.saturating_sub(visible_from);
                    push(&" ".repeat(cut), Style::PLAIN, None);
                    emitted = emitted.saturating_add(cut);
                }
            }
            start = stop;
        }
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
    /// Fold SGR 2 (faint) into every segment, the way Claude Code draws
    /// every status line row on screen (SPEC § 2.1). `preview` sets it so
    /// that it shows the intensity the screen will have; the tick never
    /// does, the harness adds it. Nothing under [`ColorMode::Never`].
    pub dim: bool,
}

impl Painter {
    /// Painter that emits nothing but text.
    pub const PLAIN: Self = Self { mode: ColorMode::Never, links: false, dim: false };

    /// The style a segment is drawn with under this painter: `dim` folded in,
    /// no colour under [`ColorMode::Never`], and an RGB colour quantised to
    /// the 256-colour cube under [`ColorMode::Ansi256`].
    ///
    /// [`Painter::paint`] and the `setup` pane's spans both go through here,
    /// which is what makes the pane show the colours the status line prints
    /// (SPEC § 14).
    #[must_use]
    pub fn painted_style(&self, style: Style) -> Style {
        let style = if self.dim { style.dimmed() } else { style };
        let fg = match (style.fg, self.mode) {
            (_, ColorMode::Never) => Color::Default,
            (Color::Rgb(r, g, b), ColorMode::Ansi256) => Color::Indexed(rgb_to_256(r, g, b)),
            (fg, ColorMode::Ansi256 | ColorMode::TrueColor) => fg,
        };
        Style { fg, ..style }
    }

    /// Whether this painter emits a link for `url`: links on, and an OSC 8
    /// target it is willing to emit ([`safe_link`]).
    #[must_use]
    pub fn links_to(&self, url: &str) -> bool {
        self.links && safe_link(url)
    }

    /// Render segments to a single line (no trailing newline).
    #[must_use]
    pub fn paint(&self, segments: &[Segment]) -> String {
        let mut out = String::new();
        for seg in segments {
            if seg.text.is_empty() {
                continue;
            }
            let link = seg.link.as_deref().filter(|u| self.links_to(u));
            if let Some(url) = link {
                let _ = write!(out, "\x1b]8;;{url}\x1b\\");
            }
            let styled = self.painted_style(seg.style).write_sgr(self.mode, &mut out);
            out.push_str(&seg.text);
            if styled {
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
    plain_cow(s).into_owned()
}

/// [`plain_text`], borrowing `s` when it is already plain, which on a warm
/// tick is every string. Whatever measures or cuts text from outside (a
/// payload string, a ref name) before it becomes a [`Segment`] measures
/// this, the text the row will show, never the raw string.
pub(crate) fn plain_cow(s: &str) -> Cow<'_, str> {
    if is_plain(s) { Cow::Borrowed(s) } else { Cow::Owned(strip(s)) }
}

/// [`plain_text`] without an allocation when the text is already plain.
fn clean(s: String) -> String {
    if is_plain(&s) { s } else { strip(&s) }
}

fn is_plain(s: &str) -> bool {
    !s.chars().any(|c| c.is_control() || is_format_char(c))
}

fn strip(s: &str) -> String {
    strip_ansi(s).chars().filter(|&c| !c.is_control() && !is_format_char(c)).collect()
}

/// Unicode `Cf` characters that change layout or reading order without
/// occupying a cell: zero-width space/non-joiner, the bidi marks and
/// embeddings/isolates, word joiner and friends, the byte order mark.
pub(crate) const fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{200b}' | '\u{200c}' | '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{180e}' | '\u{feff}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// Remove every escape sequence from a string: CSI, OSC, the string
/// sequences DCS/SOS/PM/APC with their payloads, nF sequences (`ESC ( B`)
/// with their final byte, and any other `ESC x` pair.
///
/// The first half of [`plain_text`], so it runs on the tick for every
/// string that is not already plain; control characters are the second
/// half's business.
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
            Some('P' | 'X' | '^' | '_') => {
                // DCS, SOS, PM, APC (sixel, kitty graphics, …): a string
                // sequence terminated by ST (ESC \) alone, never by BEL.
                let mut prev_esc = false;
                for n in chars.by_ref() {
                    if prev_esc && n == '\\' {
                        break;
                    }
                    prev_esc = n == '\x1b';
                }
            }
            Some(' '..='/') => {
                // nF (a charset designation, as `tput sgr0` emits): more
                // intermediate bytes, then one final byte.
                for n in chars.by_ref() {
                    if !(' '..='/').contains(&n) {
                        break;
                    }
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

    /// The period a caller reduces its clock by is the one the scroller
    /// repeats on, ligatures included: `لا` is one cell to `unicode-width`
    /// and two clusters here, and every window of the cycle is distinct.
    #[test]
    fn scroll_period_is_what_the_scroller_repeats_on() {
        let segs = [Segment::plain("abلاcd")];
        assert_eq!((segments_width(&segs), cluster_width(&segs)), (5, 6));
        let period = scroll_period(&segs, "  ", true);
        assert_eq!(period, 8);
        assert_eq!(scroll_period(&segs, "  ", false), 6);
        let window = |k: usize| plain(&scroll(&segs, 3, k, "  ", true));
        let cycle: Vec<String> = (0..period).map(window).collect();
        for (k, w) in cycle.iter().enumerate() {
            assert!(!cycle[..k].contains(w), "offset {k} repeats early: {cycle:?}");
            assert_eq!(window(k + period), *w);
        }
        assert_eq!(kept_width(5, "…"), 4);
        assert_eq!(kept_width(1, ".."), 0);
        assert_eq!(kept_width(3, ".."), 1);
        assert_eq!(kept_width(0, "…"), 0);
    }

    /// A cut inside a flag or a skin tone changes the glyph rather than
    /// shortening it, so those are clusters like a combining mark is.
    #[test]
    fn clusters_keep_flags_skin_tones_marks_and_zwj_sequences_whole() {
        fn c(s: &str) -> Vec<&str> {
            clusters(s).collect()
        }
        assert_eq!(c("ab"), ["a", "b"]);
        assert_eq!(c("e\u{301}x"), ["e\u{301}", "x"]);
        assert_eq!(c("👨\u{200d}💻x"), ["👨\u{200d}💻", "x"]);
        assert_eq!(c("☁\u{fe0f}x"), ["☁\u{fe0f}", "x"]);
        // A flag is two regional indicators; two flags are two clusters,
        // and a lone indicator stands alone.
        assert_eq!(c("🇺🇸ab"), ["🇺🇸", "a", "b"]);
        assert_eq!(c("🇺🇸🇬🇧"), ["🇺🇸", "🇬🇧"]);
        assert_eq!(c("🇺x"), ["🇺", "x"]);
        // A skin tone belongs to the emoji before it.
        assert_eq!(c("👍🏽ab"), ["👍🏽", "a", "b"]);
        // A leading zero-width character stands alone; one after a ZWJ is
        // joined; three regional indicators are a flag and a lone letter.
        assert_eq!(c("\u{301}ab"), ["\u{301}", "a", "b"]);
        assert_eq!(c("\u{200d}ab"), ["\u{200d}a", "b"]);
        assert_eq!(c("🇺🇸🇬"), ["🇺🇸", "🇬"]);
        assert_eq!(c("🇺\u{301}🇸"), ["🇺\u{301}", "🇸"]);
        assert_eq!(c(""), Vec::<&str>::new());
        // Cutting therefore keeps the glyph or drops it whole.
        let seg = |s: &str| vec![Segment::plain(s)];
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg("🇺🇸ab"), 3, "…")), "🇺🇸…");
        assert_eq!(Painter::PLAIN.paint(&truncate(&seg("👍🏽ab"), 3, "…")), "👍🏽…");
    }

    /// The lazy slices are the clusters the eager splitter used to build as
    /// one `String` each: every string of up to three pieces from an
    /// alphabet of the joining cases splits the same way under both.
    #[test]
    fn lazy_clusters_equal_the_eager_splitter() {
        fn eager(s: &str) -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            let mut joined = false;
            for c in s.chars() {
                let lone_ri = out.last().is_some_and(|last| {
                    let mut chars = last.chars();
                    chars.next().is_some_and(is_regional) && chars.next().is_none()
                });
                let attach = joined
                    || (char_width(c) == 0 && !out.is_empty())
                    || is_emoji_modifier(c)
                    || (lone_ri && is_regional(c));
                match out.last_mut() {
                    Some(last) if attach => last.push(c),
                    _ => out.push(c.to_string()),
                }
                joined = c == '\u{200d}';
            }
            out
        }
        let pieces =
            ["a", "🇺", "🇸", "\u{200d}", "\u{301}", "\u{fe0f}", "🏽", "👍", "日", "\u{1b}", "😀"];
        for a in pieces {
            for b in pieces {
                for c in pieces {
                    let s = format!("{a}{b}{c}");
                    assert_eq!(clusters(&s).collect::<Vec<_>>(), eager(&s), "{s:?}");
                }
            }
        }
    }

    #[test]
    fn plain_text_strips_escapes_and_controls_but_keeps_text() {
        assert_eq!(plain_text("ship it"), "ship it");
        assert_eq!(plain_text("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(plain_text("\x1b]8;;https://x\x1b\\link\x1b]8;;\x1b\\"), "link");
        assert_eq!(plain_text("a\tb\nc\u{7}d"), "abcd");
        assert_eq!(plain_text("\x1b"), "", "a bare ESC never reaches the row");
        // String sequences (DCS/SOS/PM/APC) lose their payload too: a sixel
        // or kitty graphics blob is not text, and only ST ends them.
        assert_eq!(plain_text("a\x1bPq#0;2;0;0;0~~\x07still\x1b\\b"), "ab");
        assert_eq!(plain_text("a\x1b_Gf=100;AAAA\x1b\\b\x1bXsos\x1b\\c\x1b^pm\x1b\\d"), "abcd");
        // An nF sequence keeps nothing of itself: `tput sgr0` on xterm is
        // `ESC ( B ESC [ m`, which used to leave its `B` behind.
        assert_eq!(plain_text("bold\x1b(B\x1b[m text"), "bold text");
        assert_eq!(plain_text("a\x1b#8b\x1b % Gc"), "abc");
        // Two-byte sequences (`ESC 7`, `ESC =`) lose both bytes, as before.
        assert_eq!(plain_text("a\x1b7b\x1b=c"), "abc");
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
        let painter = Painter { mode: ColorMode::TrueColor, links: true, dim: false };
        let s = painter.paint(std::slice::from_ref(&seg));
        assert_eq!(s, "\x1b]8;;https://x\x1b\\\x1b[1;38;2;1;2;3mPR\x1b[0m\x1b]8;;\x1b\\");
        assert_eq!(strip_ansi(&s), "PR");
        let p256 = Painter { mode: ColorMode::Ansi256, links: false, dim: false };
        assert_eq!(p256.paint(&[seg]), "\x1b[1;38;5;16mPR\x1b[0m");
        assert_eq!(Painter::PLAIN.paint(&[Segment::styled("x", Style::PLAIN.dimmed())]), "x");
    }

    /// The SGR bytes written straight into the row are the ones the
    /// parameter list used to be joined into, for every attribute and colour
    /// combination under every mode.
    #[test]
    fn sgr_is_written_in_place_byte_for_byte() {
        fn joined(style: Style, mode: ColorMode) -> String {
            if mode == ColorMode::Never {
                return String::new();
            }
            let mut params: Vec<String> = Vec::new();
            for (on, code) in [(style.bold, "1"), (style.dim, "2"), (style.underline, "4")] {
                if on {
                    params.push(code.into());
                }
            }
            match (style.fg, mode) {
                (Color::Default, _) => {}
                (Color::Ansi(n), _) if n < 8 => params.push(format!("{}", 30 + n)),
                (Color::Ansi(n), _) => params.push(format!("{}", 90 + n - 8)),
                (Color::Indexed(n), _) => params.push(format!("38;5;{n}")),
                (Color::Rgb(r, g, b), ColorMode::TrueColor) => {
                    params.push(format!("38;2;{r};{g};{b}"));
                }
                (Color::Rgb(r, g, b), _) => params.push(format!("38;5;{}", rgb_to_256(r, g, b))),
            }
            if params.is_empty() { String::new() } else { format!("\x1b[{}m", params.join(";")) }
        }
        let colors = [
            Color::Default,
            Color::Ansi(1),
            Color::Ansi(12),
            Color::Indexed(208),
            Color::Rgb(1, 2, 3),
            Color::Rgb(0x44, 0x47, 0x5a),
        ];
        for mode in [ColorMode::Never, ColorMode::Ansi256, ColorMode::TrueColor] {
            for fg in colors {
                for bits in 0..8_u8 {
                    let style = Style {
                        fg,
                        bold: bits & 1 != 0,
                        dim: bits & 2 != 0,
                        underline: bits & 4 != 0,
                    };
                    let mut out = String::from("x");
                    let styled = style.write_sgr(mode, &mut out);
                    let want = joined(style, mode);
                    assert_eq!(out, format!("x{want}"), "{style:?} {mode:?}");
                    assert_eq!(styled, !want.is_empty());
                }
            }
        }
    }

    /// SPEC § 2.1: `preview` paints every segment faint, plain runs
    /// included, so it shows what the screen shows; a segment garnish
    /// already dims is dimmed once; the tick's painter adds nothing; colour
    /// off stays plain.
    #[test]
    fn painter_dim_folds_faint_into_every_segment() {
        let screen = Painter { mode: ColorMode::Ansi256, links: false, dim: true };
        let row = [
            Segment::plain("a b"),
            Segment::styled("x", Style::fg(Color::Rgb(1, 2, 3)).bolded()),
            Segment::styled("y", Style::PLAIN.dimmed()),
        ];
        assert_eq!(screen.paint(&row), "\x1b[2ma b\x1b[0m\x1b[1;2;38;5;16mx\x1b[0m\x1b[2my\x1b[0m");
        let tick = Painter { dim: false, ..screen };
        assert_eq!(tick.paint(&row), "a b\x1b[1;38;5;16mx\x1b[0m\x1b[2my\x1b[0m");
        assert_eq!(Painter { dim: true, ..Painter::PLAIN }.paint(&row), "a bxy");
    }

    #[test]
    fn painter_drops_unsafe_links() {
        let painter = Painter { mode: ColorMode::Never, links: true, dim: false };
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
        // A cut that keeps nothing still paints the mark in the module's
        // colour: `max_width = 2` on a two-cell icon used to leave an
        // unstyled `…` where `max_width = 3` was coloured.
        let styled = [Segment::styled("🌿 ", Style::fg(Color::Ansi(2))), Segment::plain("main")];
        let cut = truncate(&styled, 2, "…");
        assert_eq!(Painter::PLAIN.paint(&cut), "…");
        assert_eq!(cut.last().map(|s| s.style.fg), Some(Color::Ansi(2)));
        assert_eq!(truncate(&styled, 3, "…").last().map(|s| s.style.fg), Some(Color::Ansi(2)));
    }

    #[test]
    fn rgb_cube_mapping() {
        assert_eq!(rgb_to_256(0, 0, 0), 16);
        assert_eq!(rgb_to_256(255, 255, 255), 231);
        assert_eq!(rgb_to_256(255, 0, 0), 196);
        // Every level maps to itself, and the midpoints go to the nearer
        // one: the corners alone pass under an even split of the range,
        // which is what used to move `color = "256"` themes off their
        // palette (`#6c7086` → 103, a light blue-grey, instead of 60).
        for (i, level) in CUBE_LEVELS.into_iter().enumerate() {
            let index = u8::try_from(16 + i * 36 + i * 6 + i).unwrap();
            assert_eq!(rgb_to_256(level, level, level), index, "level {level}");
        }
        // The `garnish` palette's muted role, which an even split sent to
        // 103 (`rgb(135,135,175)`): the cube keeps its blue.
        assert_eq!(rgb_to_256(0x6c, 0x70, 0x86), 60);
        // A colour the cube can only answer with a gray takes the nearer of
        // that gray and the 24-step ramp: every built-in `frame` role is a
        // dark near-gray the cube lightened to 95 (59), where the ramp has
        // 78 (239) or, for the `garnish` frame, 98 (241).
        assert_eq!(rgb_to_256(0x58, 0x5b, 0x70), 241);
        assert_eq!(rgb_to_256(0x44, 0x47, 0x5a), 239, "dracula");
        assert_eq!(rgb_to_256(0x3b, 0x42, 0x61), 239, "tokyonight");
        assert_eq!(rgb_to_256(0x45, 0x47, 0x5a), 239, "catppuccin");
        assert_eq!(rgb_to_256(0x80, 0x80, 0x80), 244);
        assert_eq!(rgb_to_256(3, 3, 3), 16, "black is nearer than the ramp's 8");
        assert_eq!(rgb_to_256(250, 250, 250), 231, "white is nearer than the ramp's 238");
        // 115 is the midpoint of 95 and 135; 116 rounds up.
        assert_eq!(rgb_to_256(115, 0, 0), 16 + 36);
        assert_eq!(rgb_to_256(116, 0, 0), 16 + 2 * 36);
    }

    /// Every byte lands on its nearest cube level, so no channel is ever
    /// moved further than half the gap it sits in: 48 across the wide
    /// 0..95 step, 20 across the even 40-wide ones. The old even split of
    /// the range was off by up to 47 even inside a 40-wide step (128 went
    /// to 175) and by 69 across the wide first one (26 went to 95, where
    /// 0 is nearest, a gap of 95 between the two answers).
    #[test]
    fn rgb_cube_is_the_nearest_level_for_every_byte() {
        for c in 0..=u8::MAX {
            let index = usize::from(rgb_to_256(c, 0, 0).saturating_sub(16)) / 36;
            let level = CUBE_LEVELS.get(index).copied().unwrap_or(0);
            let best = CUBE_LEVELS
                .into_iter()
                .min_by_key(|l| u16::from(c).abs_diff(u16::from(*l)))
                .unwrap_or(0);
            assert_eq!(level, best, "{c}");
            let error = u16::from(c).abs_diff(u16::from(level));
            assert!(error <= if c < 95 { 48 } else { 20 }, "{c} → {level}");
        }
    }
}
