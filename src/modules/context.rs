//! `context`: the context-window bar with band colors and the auto-compaction marker.

use crate::ansi::{Segment, Style};
use crate::claude_settings::{self, DEFAULT_COMPACT_BUFFER};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::glyph;
use crate::num::percent_of;

use super::util::{BAR_STYLES, bar, percent, rounded, tokens};
use super::{Ctx, Module, Rendered, icon, seg};

/// `context`: smooth usage bar + percentage + compaction marker.
pub struct ContextModule;

impl Module for ContextModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "context",
            summary: "Context window usage bar with color bands and the auto-compaction marker.",
            doc: "A smooth bar spanning the full context window (`context_window.context_window_size`, 1M when absent). The filled part takes the color of the current band; a marker shows where Claude Code will auto-compact (`autoCompactWindow` / `CLAUDE_CODE_AUTO_COMPACT_WINDOW` minus the summary buffer). No token counter: the bar and the percentage are the story.",
            sources: &[
                "context_window.used_percentage",
                "context_window.context_window_size",
                "exceeds_200k_tokens",
                "~/.claude/settings.json autoCompactWindow/autoCompactEnabled",
                "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
                "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE",
            ],
            refresh: 0,
            opts: opts(),
            icons: vec![
                IconSpec {
                    key: "context",
                    doc: "Context icon.",
                    glyph: glyph("\u{f2db}", "⊞", "🧠", "ctx:"),
                },
                IconSpec {
                    key: "fill", doc: "Filled cell.", glyph: glyph("█", "█", "█", "#")
                },
                IconSpec {
                    key: "empty", doc: "Empty cell.", glyph: glyph("░", "░", "░", "-")
                },
                IconSpec {
                    key: "marker",
                    doc: "Compaction marker.",
                    glyph: glyph("▏", "▏", "▏", "|"),
                },
                IconSpec {
                    key: "compact",
                    doc: "Compaction label glyph.",
                    glyph: glyph("⤓", "⤓", "⤓", "compact@"),
                },
                IconSpec {
                    key: "exceeds",
                    doc: "Exceeds-200k indicator.",
                    glyph: glyph("‼", "‼", "‼", "!!"),
                },
                IconSpec {
                    key: "warn",
                    doc: "Warning badge.",
                    glyph: glyph("\u{f071}", "⚠", "⚠", "!"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent" },
                ColorSpec { key: "percent", doc: "Percentage text.", default: "text" },
                ColorSpec { key: "empty", doc: "Empty part of the bar.", default: "muted" },
                ColorSpec { key: "marker", doc: "Compaction marker.", default: "warn" },
                ColorSpec { key: "exceeds", doc: "Exceeds-200k indicator.", default: "danger" },
                ColorSpec { key: "window", doc: "Window size tag.", default: "muted" },
                ColorSpec { key: "warn", doc: "Warning badge.", default: "danger" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let window = ctx.payload.context_window_size();
        let used = ctx.payload.context_window.as_ref().and_then(|c| c.used_percentage);
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "context", "icon"));
        }
        let thresholds = cfg.nums("thresholds");
        let bands = cfg.color_list("band_colors", ctx.theme);
        let pct = used.map(crate::num::clamp_percent);
        let fill_color = ctx.theme.band(rounded(pct.unwrap_or(0.0)), &thresholds, &bands);

        let marker = compaction_percent(ctx, cfg, window);
        let width = cfg.size("width");
        if width > 0 {
            let marker_spec = marker.map(|m| (m, cfg.icon("marker"), cfg.color("marker")));
            segs.extend(bar(
                width,
                pct.unwrap_or(0.0),
                cfg.icon("fill"),
                cfg.icon("empty"),
                fill_color,
                cfg.color("empty"),
                marker_spec,
            ));
        }
        if cfg.bool("show_percent") {
            let text = pct.map_or_else(|| "–".to_owned(), percent);
            let sp = if segs.is_empty() { "" } else { " " };
            segs.push(Segment::styled(format!("{sp}{text}"), Style::fg(fill_color).bolded()));
        }
        if cfg.bool("show_compaction_percent")
            && let Some(m) = marker
        {
            segs.push(seg(cfg, format!(" {}{}", cfg.icon("compact"), percent(m)), "marker"));
        }
        if cfg.bool("show_window") {
            segs.push(seg(cfg, format!(" {}", tokens(window)), "window"));
        }
        if cfg.bool("exceeds_200k") && ctx.payload.exceeds_200k_tokens == Some(true) {
            segs.push(seg(cfg, format!(" {}", cfg.icon("exceeds")), "exceeds"));
        }
        let warn_at = cfg.float("warn_at");
        if warn_at > 0.0 && pct.is_some_and(|p| p >= warn_at) && !cfg.icon("warn").is_empty() {
            segs.push(seg(cfg, format!(" {}", cfg.icon("warn")), "warn"));
        }
        Rendered::fresh(segs)
    }
}

fn opts() -> Vec<OptSpec> {
    vec![
        OptSpec::new("width", Kind::Int, "Bar width in cells; 0 hides the bar.", Value::Int(20))
            .minimal(Value::Int(0))
            .full(Value::Int(30)),
        OptSpec::new(
            "bar",
            Kind::Enum(BAR_STYLES),
            "Bar glyphs: `blocks` (the icon set's `█`/`░`, fractional cells) or `line` (`━`/`─`, `=`/`-` in the ascii set; whole cells, so no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win.",
            Value::Str("blocks".into()),
        ),
        OptSpec::new("show_icon", Kind::Bool, "Show the context icon.", Value::Bool(true))
            .minimal(Value::Bool(false)),
        OptSpec::new(
            "show_percent",
            Kind::Bool,
            "Show the percentage after the bar.",
            Value::Bool(true),
        ),
        OptSpec::new(
            "thresholds",
            Kind::NumList,
            "Ascending percentages where the band color changes.",
            Value::NumList(vec![50.0, 75.0, 90.0]),
        ),
        OptSpec::new(
            "band_colors",
            Kind::ColorList,
            "One color per band (roles or literal colors).",
            Value::StrList(vec!["band1".into(), "band2".into(), "band3".into(), "band4".into()]),
        ),
        OptSpec::new(
            "compaction_marker",
            Kind::Bool,
            "Mark the auto-compaction threshold on the bar.",
            Value::Bool(true),
        ),
        OptSpec::new(
            "compact_buffer_tokens",
            Kind::Int,
            "Tokens Claude Code reserves below the window for the compaction summary.",
            Value::Int(i64::try_from(DEFAULT_COMPACT_BUFFER).unwrap_or(13_000)),
        ),
        OptSpec::new(
            "show_compaction_percent",
            Kind::Bool,
            "Also print the compaction threshold as a percentage.",
            Value::Bool(false),
        )
        .full(Value::Bool(true)),
        OptSpec::new(
            "show_window",
            Kind::Bool,
            "Show the window size tag (`1M`, `200k`).",
            Value::Bool(false),
        )
        .full(Value::Bool(true)),
        OptSpec::new(
            "exceeds_200k",
            Kind::Bool,
            "Show an indicator when the last response exceeded 200k tokens.",
            Value::Bool(false),
        )
        .full(Value::Bool(true)),
        OptSpec::new(
            "warn_at",
            Kind::Float,
            "Extra warning badge at or above this percentage; 0 disables.",
            Value::Float(0.0),
        ),
    ]
}

/// The compaction threshold as a percentage of the window, if enabled.
fn compaction_percent(ctx: &Ctx<'_>, cfg: &ModuleCfg, window: u64) -> Option<f64> {
    if !cfg.bool("compaction_marker") {
        return None;
    }
    // Project settings live under the directory Claude Code was launched in,
    // not under whatever subdirectory the session has moved to.
    let project = ctx.payload.project_dir().map(std::path::Path::new);
    let home = ctx.home.as_deref().map(std::path::Path::new);
    let ac = claude_settings::resolve(&ctx.settings_env, project, home);
    let threshold = ac.threshold(window, cfg.int("compact_buffer_tokens"))?;
    Some(percent_of(threshold, window))
}
