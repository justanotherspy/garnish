//! `text.<name>`: user-defined static text in a box (SPEC § 3.7).
//!
//! The one module kind a config may define any number of, under
//! `[modules.text.<name>]`, placed on a line as `text.<name>`. A text module
//! never runs a command, reads a file or touches the cache, so it costs
//! nothing on the tick; its only moving part is the clock-driven scroller it
//! shares with the line ticker.

use std::sync::LazyLock;

use crate::ansi::{Segment, display_width, scroll, scroll_period, truncate};
use crate::config::schema::{ColorSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::config::{MAX_CELLS, MAX_TEXT_CHARS};

use super::{Ctx, Rendered, seg};

/// The family's id prefix: a line places `text.<name>`.
pub const PREFIX: &str = "text.";

/// `justify` choices: where text narrower than the box sits.
pub const JUSTIFIES: &[&str] = &["left", "right", "center"];

/// `overflow` choices for text wider than the box.
pub const OVERFLOWS: &[&str] = &["clip", "scroll", "scroll-wrap"];

/// The schema every `[modules.text.<name>]` table is validated against and
/// the reference page is generated from.
pub static SCHEMA: LazyLock<ModuleSchema> = LazyLock::new(schema);

fn schema() -> ModuleSchema {
    ModuleSchema {
        id: "text",
        measure: None,
        summary: "Static text in a box of fixed width; define any number as `[modules.text.<name>]`.",
        doc: "A fixed string in a box, placed on a line as `text.<name>`. `width = 0` makes the box as wide as the text; otherwise the box is `width` cells with `pad` blank cells on each side, `justify` places shorter text in it, and `overflow` decides what happens to longer text: `clip` cuts it with an ellipsis, `scroll` slides a window over it and restarts after the end has passed, `scroll-wrap` is a ticker that flows continuously with `gap` between the end and the start. Scrolling is a pure function of the clock (`floor(now × step) mod period`), so nothing is stored between ticks and `GARNISH_ANIMATE=0` freezes it. The text is plain: escape sequences and control characters are stripped. With an empty `text` the module has nothing to show, so `hide_when_empty` (on by default) hides it rather than drawing a dim `–`. Text modules have no `preset` and no `refresh`, and `width` sizes the box where other modules take `max_width`.",
        sources: &["the config file"],
        refresh: 0,
        opts: vec![
            OptSpec::new(
                "text",
                Kind::Str,
                "The text. ANSI/OSC sequences and control characters are stripped.",
                Value::Str(String::new()),
            )
            .max(MAX_TEXT_CHARS),
            OptSpec::new(
                "width",
                Kind::Int,
                "Box width in cells; 0 = the text's own width.",
                Value::Int(0),
            )
            .max(MAX_CELLS),
            OptSpec::new(
                "pad",
                Kind::Int,
                "Blank cells added on each side of the box.",
                Value::Int(0),
            )
            .max(MAX_CELLS),
            OptSpec::new(
                "justify",
                Kind::Enum(JUSTIFIES),
                "Where text narrower than the box sits.",
                Value::Str("left".into()),
            ),
            OptSpec::new(
                "overflow",
                Kind::Enum(OVERFLOWS),
                "Text wider than the box: `clip` cuts with an ellipsis, `scroll` slides a window and restarts after the end, `scroll-wrap` flows continuously with `gap` between end and start.",
                Value::Str("scroll".into()),
            ),
            OptSpec::new(
                "step",
                Kind::Float,
                "Cells scrolled per tick (0.001–1000; 0.5 = every second tick).",
                Value::Float(1.0),
            ),
            OptSpec::new(
                "gap",
                Kind::Str,
                "`scroll-wrap` only: text between the end and the start.",
                Value::Str("   ".into()),
            )
            .max(MAX_TEXT_CHARS),
            OptSpec::new(
                "url",
                Kind::Str,
                "Wrap the box in a clickable OSC 8 link to this `http(s)://` URL (printable ASCII only; anything else is reported and dropped).",
                Value::Str(String::new()),
            )
            .max(MAX_TEXT_CHARS),
        ],
        icons: Vec::new(),
        colors: vec![ColorSpec { key: "text", doc: "The text.", default: "accent" }],
    }
}

/// Render one text module for a tick.
///
/// The result is always `pad + box + pad` cells wide (with `width = 0`, the
/// box is the text), which is what makes a text module a fixed-width slot
/// next to aligned columns. `text` and `gap` are already plain text: the
/// config reduced them with [`crate::ansi::plain_text`].
#[must_use]
pub fn render(ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
    let text = cfg.str("text").to_owned();
    if text.is_empty() {
        return Rendered::empty();
    }
    let text_w = display_width(&text);
    let styled = vec![seg(cfg, text, "text")];
    // Sizes are capped at config time (`MAX_CELLS`) and again here: a row is
    // never wider than the box, so a bigger box would only size the padding.
    let box_w = match cfg.size("width").min(ctx.width) {
        0 => text_w,
        w => w,
    };
    let body: Vec<Segment> = if text_w <= box_w {
        let fill = box_w.saturating_sub(text_w);
        let (before, after) = match cfg.str("justify") {
            "right" => (fill, 0),
            "center" => {
                let half = fill.checked_div(2).unwrap_or(0);
                (half, fill.saturating_sub(half))
            }
            _ => (0, fill),
        };
        let mut out: Vec<Segment> = Vec::new();
        if before > 0 {
            out.push(Segment::plain(" ".repeat(before)));
        }
        out.extend(styled);
        if after > 0 {
            out.push(Segment::plain(" ".repeat(after)));
        }
        out
    } else {
        let step = cfg.float("step");
        match cfg.str("overflow") {
            "clip" => {
                let mut cut = truncate(&styled, box_w, ctx.icons.ellipsis());
                // A cut can land short of the box: the next cluster may be
                // two cells wide with one cell left, so `truncate` stops
                // early. `scroll` always fills its window, and a text
                // module is a fixed-width slot next to aligned columns, so
                // the shortfall is padded rather than left to shift them.
                let short = box_w.saturating_sub(crate::ansi::segments_width(&cut));
                if short > 0 {
                    cut.push(Segment::plain(" ".repeat(short)));
                }
                cut
            }
            "scroll-wrap" => {
                let gap = cfg.str("gap");
                let period = scroll_period(&styled, gap, true);
                scroll(&styled, box_w, ctx.frame(step, period), gap, true)
            }
            _ => {
                let period = scroll_period(&styled, "", false);
                scroll(&styled, box_w, ctx.frame(step, period), "", false)
            }
        }
    };
    // `url` (SPEC § 3.7) links the whole box and nothing outside it: the
    // `justify` fill, a scrolled window's cells and the clip shortfall are
    // all inside, only the `pad` cells added below are not. The config
    // already checked the URL against the painter's rule, which applies again.
    let url = cfg.str("url");
    let body: Vec<Segment> =
        if url.is_empty() { body } else { body.into_iter().map(|s| s.with_link(url)).collect() };
    let pad = cfg.size("pad").min(ctx.width);
    if pad == 0 {
        return Rendered::fresh(body);
    }
    let blank = Segment::plain(" ".repeat(pad));
    let mut out = vec![blank.clone()];
    out.extend(body);
    out.push(blank);
    Rendered::fresh(out)
}

#[cfg(test)]
mod tests {
    use crate::render::{Clock, render_lines_at};

    /// SPEC § 3.7 `url`: every segment of the finished box carries the
    /// link (a scrolled window's cut cells and padding, a clipped box's
    /// ellipsis, the `justify` fill), the `pad` cells around it never.
    #[test]
    fn url_links_the_whole_box_but_not_the_pads() {
        let payload = crate::payload::Payload::parse("{\"session_id\": \"s\"}").unwrap();
        let module = |table: &str| {
            let text = format!(
                "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\nurl = \"https://x.example/a\"\n{table}"
            );
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            // Offset 1 into a two-cell script cuts the first cluster.
            let clock = Clock {
                now: jiff::Timestamp::from_second(1_738_425_601).unwrap(),
                animate: true,
                ..Clock::fixed()
            };
            render_lines_at(&payload, &config, Some(80), &clock).into_iter().next().unwrap()
        };
        let linked = |segs: &[crate::ansi::Segment]| {
            segs.iter().all(|s| s.link.as_deref() == Some("https://x.example/a"))
        };
        let scrolled = module("text = \"日本語テキスト\"\nwidth = 5\npad = 1\n");
        assert_eq!(scrolled.first().unwrap().text(), " ");
        assert_eq!(scrolled.last().unwrap().text(), " ");
        assert!(
            scrolled.first().unwrap().link.is_none() && scrolled.last().unwrap().link.is_none()
        );
        assert!(linked(&scrolled[1..scrolled.len() - 1]), "{scrolled:?}");
        assert_eq!(crate::ansi::segments_width(&scrolled), 7);
        let clipped = module("text = \"clip me\"\nwidth = 4\noverflow = \"clip\"\n");
        assert_eq!(crate::ansi::Painter::PLAIN.paint(&clipped), "cli…");
        assert!(linked(&clipped), "{clipped:?}");
        // A cut that lands short (the next cluster is two cells wide with one
        // cell left) is padded to the box, and that cell is inside the box
        // like the `justify` fill, so it carries the link too.
        let short = module("text = \"日本語\"\nwidth = 4\noverflow = \"clip\"\n");
        assert_eq!(crate::ansi::Painter::PLAIN.paint(&short), "日… ");
        assert_eq!(crate::ansi::segments_width(&short), 4);
        assert!(linked(&short), "{short:?}");
        let centred = module("text = \"hi\"\nwidth = 6\njustify = \"center\"\n");
        assert_eq!(crate::ansi::Painter::PLAIN.paint(&centred), "  hi  ");
        assert!(linked(&centred), "{centred:?}");
        assert!(centred.len() >= 3, "fill, text, fill: {centred:?}");
    }

    /// SPEC § 4.2: a scrolling text cycles with the scroller's own period,
    /// counted cluster by cluster. `لا` is one cell to `unicode-width` but
    /// two clusters to the scroller, and the offset used to wrap a cell
    /// early, never showing the last window of the cycle.
    #[test]
    fn a_scrolling_text_over_a_ligature_cycles_with_the_scroller() {
        let payload = crate::payload::Payload::parse("{\"session_id\": \"s\"}").unwrap();
        let text = "[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\ntext = \"abلاcd\"\nwidth = 3\noverflow = \"scroll-wrap\"\ngap = \"  \"\n";
        let (config, errs) = crate::config::parse(text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        let at = |secs: i64| {
            let clock = Clock {
                now: jiff::Timestamp::from_second(secs).unwrap(),
                animate: true,
                ..Clock::fixed()
            };
            let row = render_lines_at(&payload, &config, Some(80), &clock);
            crate::ansi::Painter::PLAIN.paint(row.first().unwrap())
        };
        // Six clusters and a two-cell gap: eight windows, then the first.
        let windows: Vec<String> = (0..8).map(at).collect();
        for (i, w) in windows.iter().enumerate() {
            assert!(!windows[..i].contains(w), "window {i} repeats early: {windows:?}");
        }
        assert_eq!(at(8), windows[0]);
    }

    /// SPEC § 9: the `text.<name>` family is the one module set outside the
    /// schema matrix, its content being wholly user-supplied, so `text`,
    /// `width`, `justify`, `overflow` and `url` are swept here instead, over
    /// text a user could really paste. `pad` is pinned at 1 (it is what
    /// makes the link boundary visible) and `step` and `gap` stay at their
    /// defaults, so the two scrolling modes are sampled at one clock phase.
    ///
    /// The invariants are the matrix's, and each is asserted: the box is
    /// exactly the width it was asked for, no escape or control byte reaches
    /// a row, every cluster on the row is one of the input's (so none was
    /// split), a `url` the painter accepts is on the row and one it refuses
    /// does not parse at all.
    #[test]
    fn every_text_option_holds_the_shared_invariants() {
        use crate::icons::IconSet;
        let payload = crate::payload::Payload::parse("{\"session_id\": \"s\"}").unwrap();
        let hostile = [
            ("plain", "hello"),
            ("escape", "a\u{1b}[31mred\u{1b}[0m"),
            ("control", "a\u{7}b\u{0}c"),
            ("bidi", "a\u{202e}gnp.txt"),
            ("wide", "日本語テキスト"),
            ("flag", "🇺🇸🇫🇷ab"),
            ("combining", "e\u{301}e\u{301}e\u{301}"),
            ("newline", "one\ntwo"),
        ];
        // Format characters too, not only controls: the `bidi` fixture is
        // there for U+202E, which is `Cf` and passes `is_control`, so a
        // predicate of controls alone let the one row that exists to catch a
        // reversed name assert nothing but its width.
        let is_escape = |c: char| c.is_control() || crate::ansi::is_format_char(c);
        // A URL the painter would refuse never reaches a row because the
        // *config* refuses it first, which is the stronger rule and is
        // asserted once here rather than swept.
        let (_, errs) = crate::config::parse(
            "[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\ntext = \"x\"\nurl = \"git@github.com:o/r.git\"\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(
            errs.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            vec!["modules.text.a.url"],
            "a URL the painter refuses must not parse"
        );

        // `url` varies with the text rather than adding a fifth loop: with a
        // single value the link assertion below was vacuous, because nothing
        // in the sweep set `url` and `seg.link` was `None` in all 1152 cases.
        let urls = ["", "https://x.example/a"];
        for (i, (name, text)) in hostile.into_iter().enumerate() {
            let url = urls.get(i % urls.len()).copied().unwrap_or_default();
            let linkable = !url.is_empty();
            for icons in IconSet::ALL {
                for overflow in ["clip", "scroll", "scroll-wrap"] {
                    for justify in ["left", "center", "right"] {
                        for width in [0_usize, 1, 4, 9] {
                            let cfg = format!(
                                "icons = \"{}\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"text.a\"]\n[modules.text.a]\ntext = {}\nurl = {}\nwidth = {width}\npad = 1\noverflow = \"{overflow}\"\njustify = \"{justify}\"\n",
                                icons.name(),
                                crate::config::schema::toml_string(text),
                                crate::config::schema::toml_string(url),
                            );
                            let (config, errs) =
                                crate::config::parse(&cfg, &crate::modules::SCHEMAS);
                            assert!(errs.is_empty(), "{name}: {errs:?}");
                            let mut clock = Clock::fixed();
                            clock.animate = true;
                            let label =
                                format!("{name}/{}/{overflow}/{justify}/{width}", icons.name());
                            let row = render_lines_at(&payload, &config, Some(80), &clock)
                                .into_iter()
                                .next()
                                .unwrap_or_default();
                            for seg in &row {
                                assert!(
                                    !seg.text().chars().any(is_escape),
                                    "{label}: {:?}",
                                    seg.text()
                                );
                                assert!(
                                    seg.link.as_deref().is_none_or(crate::ansi::safe_link),
                                    "{label}: {:?}",
                                    seg.link
                                );
                            }
                            // A URL the painter refuses never reaches a row;
                            // one it accepts always does, so neither half of
                            // the rule can pass by nothing happening.
                            let linked = row.iter().any(|s| s.link.as_deref() == Some(url));
                            assert_eq!(linked, linkable, "{label}: url {url:?}");
                            // No cluster is ever split: every cluster on the
                            // row is one of the input's, a pad space, or the
                            // set's own cut mark. A half-emoji would be
                            // neither, and so would a lone combining mark.
                            // Against the *reduced* text: the config strips
                            // escapes and control bytes on the way in, so the
                            // raw literal is not what the module rendered.
                            let plain = crate::ansi::plain_text(text);
                            let source: Vec<&str> = crate::ansi::clusters(&plain).collect();
                            let mark: Vec<&str> = crate::ansi::clusters(icons.ellipsis()).collect();
                            for seg in &row {
                                for c in crate::ansi::clusters(seg.text()) {
                                    assert!(
                                        c == " " || source.contains(&c) || mark.contains(&c),
                                        "{label}: {c:?} is not a cluster of the input"
                                    );
                                }
                            }
                            // `width = 0` sizes the box to the text; any
                            // other width is exactly that, plus the pads.
                            let cells = crate::ansi::segments_width(&row);
                            if width > 0 {
                                assert_eq!(cells, width + 2, "{label}: {cells} cells");
                            }
                        }
                    }
                }
            }
        }
    }
}
