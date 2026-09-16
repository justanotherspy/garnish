//! `text.<name>`: user-defined static text in a box (SPEC § 3.7).
//!
//! The one module kind a config may define any number of, under
//! `[modules.text.<name>]`, placed on a line as `text.<name>`. A text module
//! never runs a command, reads a file or touches the cache, so it costs
//! nothing on the tick; its only moving part is the clock-driven scroller it
//! shares with the line ticker.

use std::sync::LazyLock;

use crate::ansi::{Segment, display_width, scroll, truncate};
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
        summary: "Static text in a box of fixed width; define any number as `[modules.text.<name>]`.",
        doc: "A fixed string in a box, placed on a line as `text.<name>`. `width = 0` makes the box as wide as the text; otherwise the box is `width` cells with `pad` blank cells on each side, `justify` places shorter text in it, and `overflow` decides what happens to longer text: `clip` cuts it with an ellipsis, `scroll` slides a window over it and restarts after the end has passed, `scroll-wrap` is a ticker that flows continuously with `gap` between the end and the start. Scrolling is a pure function of the clock (`floor(now × step) mod period`), so nothing is stored between ticks and `GARNISH_ANIMATE=0` freezes it. The text is plain: escape sequences and control characters are stripped. Text modules have no `preset` and no `refresh`.",
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
                "Cells scrolled per tick (> 0; 0.5 = every second tick).",
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
            "clip" => truncate(&styled, box_w, ctx.icons.ellipsis()),
            "scroll-wrap" => {
                let gap = cfg.str("gap");
                let period = text_w.saturating_add(display_width(gap));
                scroll(&styled, box_w, ctx.frame(step, period), gap, true)
            }
            _ => scroll(&styled, box_w, ctx.frame(step, text_w), "", false),
        }
    };
    // `url` (SPEC § 3.7) links the box, padding cells excluded; the config
    // already checked it against the painter's rule, which applies again.
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
        let centred = module("text = \"hi\"\nwidth = 6\njustify = \"center\"\n");
        assert_eq!(crate::ansi::Painter::PLAIN.paint(&centred), "  hi  ");
        assert!(linked(&centred), "{centred:?}");
        assert!(centred.len() >= 3, "fill, text, fill: {centred:?}");
    }
}
