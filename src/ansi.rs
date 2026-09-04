//! ANSI styling, OSC 8 hyperlinks, display width and width-aware truncation.
//!
//! Rendering produces [`Segment`]s (text + style). Styles are resolved to
//! escape sequences only at the very end, by [`Painter`], so tests can assert
//! on plain text and the color mode can be switched without touching modules.

use std::fmt::Write as _;
use unicode_width::UnicodeWidthChar;

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
    /// Unstyled text.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), style: Style::PLAIN, link: None }
    }

    /// Styled text.
    #[must_use]
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style, link: None }
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
/// Private-use glyphs (Nerd Font icons) count as one cell.
#[must_use]
pub fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Width of one character in cells.
#[must_use]
pub fn char_width(c: char) -> usize {
    let cp = u32::from(c);
    // Private Use Areas: Nerd Font glyphs render single-width in practice.
    if (0xE000..=0xF8FF).contains(&cp) || (0xF_0000..=0x10_FFFD).contains(&cp) {
        return 1;
    }
    // Variation selector 16 requests emoji presentation → the base glyph is wide.
    if cp == 0xFE0F {
        return 0;
    }
    UnicodeWidthChar::width(c).unwrap_or(0)
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
    let ell_width = display_width(ellipsis);
    let budget = max_width.saturating_sub(ell_width);
    let mut out: Vec<Segment> = Vec::new();
    let mut used = 0_usize;
    'outer: for seg in segments {
        let mut kept = String::new();
        for ch in seg.text.chars() {
            let w = char_width(ch);
            if used.saturating_add(w) > budget {
                if !kept.is_empty() {
                    out.push(Segment { text: kept, style: seg.style, link: seg.link.clone() });
                }
                break 'outer;
            }
            used = used.saturating_add(w);
            kept.push(ch);
        }
        out.push(Segment { text: kept, style: seg.style, link: seg.link.clone() });
    }
    if ell_width > 0 && max_width >= ell_width {
        let style = out.last().map_or(Style::PLAIN, |s| s.style);
        out.push(Segment::styled(ellipsis, style));
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
            let link = seg.link.as_deref().filter(|_| self.links);
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
    }

    #[test]
    fn widths_count_nerd_glyphs_as_one_and_emoji_as_two() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("\u{e725}"), 1);
        assert_eq!(display_width("⏱"), 1);
        assert_eq!(display_width("🌿"), 2);
        assert_eq!(display_width("█░▏"), 3);
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
    fn rgb_cube_mapping() {
        assert_eq!(rgb_to_256(0, 0, 0), 16);
        assert_eq!(rgb_to_256(255, 255, 255), 231);
        assert_eq!(rgb_to_256(255, 0, 0), 196);
    }
}
