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
use crate::icons::IconSet;

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
            ),
            OptSpec::new(
                "width",
                Kind::Int,
                "Box width in cells; 0 = the text's own width.",
                Value::Int(0),
            ),
            OptSpec::new(
                "pad",
                Kind::Int,
                "Blank cells added on each side of the box.",
                Value::Int(0),
            ),
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
            ),
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
    let box_w = match cfg.size("width") {
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
                let ellipsis = if ctx.icons == IconSet::Ascii { ".." } else { "…" };
                truncate(&styled, box_w, ellipsis)
            }
            "scroll-wrap" => {
                let gap = cfg.str("gap");
                let period = text_w.saturating_add(display_width(gap));
                scroll(&styled, box_w, ctx.frame(step, period), gap, true)
            }
            _ => scroll(&styled, box_w, ctx.frame(step, text_w), "", false),
        }
    };
    let pad = cfg.size("pad");
    if pad == 0 {
        return Rendered::fresh(body);
    }
    let blank = Segment::plain(" ".repeat(pad));
    let mut out = vec![blank.clone()];
    out.extend(body);
    out.push(blank);
    Rendered::fresh(out)
}
