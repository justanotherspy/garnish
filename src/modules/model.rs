//! `model`, `effort`, `style`: what is answering, how hard it thinks, and
//! which output style is active.

use crate::ansi::{Segment, Style};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::glyph;

use super::{Ctx, IconShown, Module, Rendered, badge, close_up, lead, seg, show_icon_opt};

/// `model`: display name, fast-mode and thinking glyphs, optionally the model id.
pub struct ModelModule;

impl Module for ModelModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "model",
            measure: None,
            summary: "Model name, with fast-mode and thinking indicators.",
            doc: "Shows `model.display_name`. A bolt appears when fast mode is on; the `full` preset adds the raw model id and a thinking glyph when extended thinking is enabled.",
            sources: &["model.display_name", "model.id", "fast_mode", "thinking.enabled"],
            refresh: 0,
            opts: vec![
                show_icon_opt("Show the model icon.", IconShown::ExceptMinimal),
                OptSpec::new("show_id", Kind::Bool, "Append the raw model id.", Value::Bool(false))
                    .full(Value::Bool(true)),
                OptSpec::new(
                    "show_fast",
                    Kind::Bool,
                    "Show a bolt when fast mode is on.",
                    Value::Bool(true),
                ),
                OptSpec::new(
                    "show_thinking",
                    Kind::Bool,
                    "Show a glyph when extended thinking is enabled.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![
                IconSpec {
                    key: "model",
                    doc: "Model icon.",
                    // nf-cod-hubot: in the BMP private-use area, so v2 Nerd
                    // Fonts render it; nf-md-robot (U+F06A9) is v3-only.
                    glyph: glyph("\u{eb08}", "❖", "🤖", ""),
                },
                IconSpec {
                    key: "fast",
                    doc: "Fast mode.",
                    glyph: glyph("\u{f0e7}", "⚡", "⚡", "!"),
                },
                IconSpec {
                    key: "thinking",
                    doc: "Extended thinking.",
                    glyph: glyph("\u{f0eb}", "⋯", "💭", "~"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent" },
                ColorSpec { key: "name", doc: "Model name.", default: "text" },
                ColorSpec { key: "id", doc: "Model id.", default: "muted" },
                ColorSpec { key: "fast", doc: "Fast-mode bolt.", default: "warn" },
                ColorSpec { key: "thinking", doc: "Thinking glyph.", default: "accent2" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(model) = ctx.payload.model.as_ref() else { return Rendered::empty() };
        let name = model.display_name.as_deref().or(model.id.as_deref()).unwrap_or("");
        if name.is_empty() {
            return Rendered::empty();
        }
        let mut segs: Vec<Segment> = lead(cfg, "model");
        segs.push(Segment::styled(name, Style::fg(cfg.color("name")).bolded()));
        if cfg.bool("show_fast") && ctx.payload.fast_mode == Some(true) {
            segs.extend(badge(cfg, "fast", "fast"));
        }
        if cfg.bool("show_thinking")
            && ctx.payload.thinking.as_ref().and_then(|t| t.enabled) == Some(true)
        {
            segs.extend(badge(cfg, "thinking", "thinking"));
        }
        if cfg.bool("show_id")
            && let Some(id) = model.id.as_deref().filter(|id| *id != name)
        {
            segs.push(seg(cfg, format!(" {id}"), "id"));
        }
        Rendered::fresh(segs)
    }
}

/// `effort`: reasoning effort as a glyph scale and/or word.
pub struct EffortModule;

const LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

impl Module for EffortModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "effort",
            measure: None,
            summary: "Reasoning effort level as a five-step scale and/or word.",
            doc: "Shows `effort.level` (`low`, `medium`, `high`, `xhigh`, `max`). Hidden when the model does not support effort. The scale lights one step per level.",
            sources: &["effort.level"],
            refresh: 0,
            opts: vec![
                OptSpec::new(
                    "style",
                    Kind::Enum(&["scale", "word", "both"]),
                    "How to show the level.",
                    Value::Str("scale".into()),
                )
                .minimal(Value::Str("word".into()))
                .full(Value::Str("both".into())),
                show_icon_opt("Show the effort icon.", IconShown::ExceptMinimal),
            ],
            icons: vec![
                IconSpec {
                    key: "effort",
                    doc: "Effort icon.",
                    glyph: glyph("\u{f0e4}", "⚙", "🎯", ""),
                },
                IconSpec {
                    key: "scale",
                    doc: "Five glyphs, one per level, lowest first.",
                    glyph: glyph("▁▃▅▇█", "▁▃▅▇█", "▁▃▅▇█", ".:=+#"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "active", doc: "Lit scale steps.", default: "accent2" },
                ColorSpec { key: "inactive", doc: "Unlit scale steps.", default: "muted" },
                ColorSpec { key: "word", doc: "Level word.", default: "text" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(level) = ctx.payload.effort.as_ref().and_then(|e| e.level.as_deref()) else {
            return Rendered::empty();
        };
        let steps = LEVELS.iter().position(|l| *l == level).map_or(0, |i| i.saturating_add(1));
        let style = cfg.str("style");
        let mut segs: Vec<Segment> = lead(cfg, "effort");
        let first = segs.len();
        if style != "word" {
            let scale: Vec<char> = cfg.icon("scale").chars().collect();
            let lit: String = scale.iter().take(steps).collect();
            let unlit: String = scale.iter().skip(steps).collect();
            if !lit.is_empty() {
                segs.push(Segment::styled(lit, Style::fg(cfg.color("active")).bolded()));
            }
            if !unlit.is_empty() {
                segs.push(Segment::styled(unlit, Style::fg(cfg.color("inactive")).dimmed()));
            }
        }
        if style != "scale" {
            segs.push(seg(cfg, format!(" {level}"), "word"));
        }
        close_up(&mut segs, first);
        Rendered::fresh(segs)
    }
}

/// `style`: the output style name.
pub struct StyleModule;

impl Module for StyleModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "style",
            measure: None,
            summary: "Output style name.",
            doc: "Shows `output_style.name`. By default the `default` style is hidden; the `full` preset always shows it.",
            sources: &["output_style.name"],
            refresh: 0,
            opts: vec![
                OptSpec::new(
                    "hide_default",
                    Kind::Bool,
                    "Hide when the style is `default`.",
                    Value::Bool(true),
                )
                .full(Value::Bool(false)),
                show_icon_opt("Show the style icon.", IconShown::ExceptMinimal),
            ],
            icons: vec![IconSpec {
                key: "style",
                doc: "Style icon.",
                glyph: glyph("\u{f1fc}", "✎", "🎨", "style:"),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "name", doc: "Style name.", default: "text" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(name) = ctx.payload.output_style.as_ref().and_then(|s| s.name.as_deref()) else {
            return Rendered::empty();
        };
        if name.is_empty() || (cfg.bool("hide_default") && name == "default") {
            return Rendered::empty();
        }
        let mut segs: Vec<Segment> = lead(cfg, "style");
        segs.push(seg(cfg, name, "name"));
        Rendered::fresh(segs)
    }
}

#[cfg(test)]
mod tests {
    use crate::ansi::strip_ansi;
    use crate::render::{Clock, render_plain_at};

    /// `effort` alone on an unframed line at `high`, plain.
    fn effort_row(extra: &str) -> String {
        let payload = crate::payload::Payload::parse(
            "{\"session_id\": \"s\", \"effort\": {\"level\": \"high\"}}",
        )
        .unwrap();
        let text = format!(
            "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"effort\"]\n[modules.effort]\n{extra}"
        );
        let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()))
            .trim_end()
            .to_owned()
    }

    /// The word under `style = "both"` is spaced from the scale, not from
    /// nothing: a blank `scale` left `⚙  high`, and ` high` without the icon.
    #[test]
    fn the_effort_word_is_spaced_once() {
        assert_eq!(effort_row("style = \"both\"\n"), "⚙ ▁▃▅▇█ high");
        assert_eq!(effort_row("style = \"word\"\n"), "⚙ high");
        let blank = "style = \"both\"\n[modules.effort.icons]\nscale = \"\"\n";
        assert_eq!(effort_row(blank), "⚙ high");
        let bare = "style = \"both\"\nshow_icon = false\n[modules.effort.icons]\nscale = \"\"\n";
        assert_eq!(effort_row(bare), "high");
    }
}
