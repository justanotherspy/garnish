//! The second painter target (SPEC § 14): garnish segments as ratatui spans.
//!
//! ratatui interprets no escape bytes, so the pane cannot be fed what the
//! tick prints. Instead every segment goes through the same
//! [`Painter::painted_style`] the tick's bytes come from and lands in a
//! span with that style, which is what makes the pane show the status line
//! colour for colour.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::ansi::{self, Painter, Segment};

/// The ratatui colour of a garnish colour: the terminal's 16 as indexed
/// entries of the 256 palette, which is where the terminal keeps them.
#[must_use]
pub const fn color(c: ansi::Color) -> Option<Color> {
    match c {
        ansi::Color::Default => None,
        ansi::Color::Ansi(n) | ansi::Color::Indexed(n) => Some(Color::Indexed(n)),
        ansi::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

/// The ratatui style a segment's style paints as under `painter`. With
/// colour off the painter prints no escape at all, so nothing is left but
/// the harness's own faint.
#[must_use]
pub fn style(painter: &Painter, s: ansi::Style) -> Style {
    if painter.mode == ansi::ColorMode::Never {
        return if painter.dim { Style::new().add_modifier(Modifier::DIM) } else { Style::new() };
    }
    let s = painter.painted_style(s);
    let mut out = Style::new();
    if let Some(fg) = color(s.fg) {
        out = out.fg(fg);
    }
    if s.bold {
        out = out.add_modifier(Modifier::BOLD);
    }
    if s.dim {
        out = out.add_modifier(Modifier::DIM);
    }
    if s.italic {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if s.underline {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    out
}

/// A row of segments as one ratatui line, `extra` patched over every span
/// (the selection's inverse video).
#[must_use]
pub fn line(painter: &Painter, segments: &[Segment], extra: Option<Style>) -> Line<'static> {
    let spans: Vec<Span<'static>> = segments
        .iter()
        .filter(|s| !s.text().is_empty())
        .map(|s| {
            let mut st = style(painter, s.style);
            if let Some(e) = extra {
                st = st.patch(e);
            }
            Span::styled(s.text().to_owned(), st)
        })
        .collect();
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::{ColorMode, Style as GStyle, strip_ansi};

    /// SPEC § 14: the span text and styles agree with what `Painter::paint`
    /// prints, under every colour mode and with the harness's faint folded
    /// in.
    #[test]
    fn spans_agree_with_the_painted_bytes() {
        let segs = vec![
            Segment::styled("a", GStyle::fg(ansi::Color::Rgb(255, 0, 0)).bolded()),
            Segment::plain(" "),
            Segment::styled("b", GStyle::fg(ansi::Color::Ansi(4)).underline_if(true)),
            Segment::styled("", GStyle::PLAIN),
            Segment::styled("c", GStyle::fg(ansi::Color::Indexed(208)).dimmed()),
        ];
        for mode in [ColorMode::TrueColor, ColorMode::Ansi256, ColorMode::Never] {
            for dim in [false, true] {
                let painter = Painter { mode, links: false, dim };
                let bytes = painter.paint(&segs);
                let spans = line(&painter, &segs, None).spans;
                let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
                assert_eq!(text, strip_ansi(&bytes), "{mode:?} dim={dim}");
                assert_eq!(spans.len(), 4, "empty segments are dropped");
                let first = spans[0].style;
                match mode {
                    ColorMode::TrueColor => assert_eq!(first.fg, Some(Color::Rgb(255, 0, 0))),
                    ColorMode::Ansi256 => {
                        assert_eq!(first.fg, Some(Color::Indexed(196)));
                        assert!(bytes.contains("38;5;196"), "{bytes:?}");
                    }
                    ColorMode::Never => {
                        assert_eq!(first.fg, None);
                        assert_eq!(bytes, "a bc");
                    }
                }
                // Colour off prints no escape at all (app-16): the pane shows
                // no weight or underline either, only the harness's faint.
                let styled = mode != ColorMode::Never;
                assert_eq!(first.add_modifier.contains(Modifier::BOLD), styled, "{mode:?}");
                assert_eq!(first.add_modifier.contains(Modifier::DIM), dim, "{mode:?}");
                let underlined = spans[2].style.add_modifier.contains(Modifier::UNDERLINED);
                assert_eq!(underlined, styled, "{mode:?}");
                let dimmed = spans[3].style.add_modifier.contains(Modifier::DIM);
                assert_eq!(dimmed, styled || dim, "dim segment, {mode:?}");
                if mode == ColorMode::Never {
                    for span in &spans {
                        let want = if dim { Modifier::DIM } else { Modifier::empty() };
                        assert_eq!(span.style.add_modifier, want, "{span:?}");
                    }
                }
                if mode != ColorMode::Never {
                    assert_eq!(spans[2].style.fg, Some(Color::Indexed(4)));
                    assert_eq!(spans[3].style.fg, Some(Color::Indexed(208)));
                    assert_eq!(bytes.contains("\x1b[1;2;"), dim, "bold+dim: {bytes:?}");
                }
            }
        }
        let selected =
            line(&Painter::PLAIN, &segs, Some(Style::new().add_modifier(Modifier::REVERSED)));
        assert!(selected.spans.iter().all(|s| s.style.add_modifier.contains(Modifier::REVERSED)));
    }
}
