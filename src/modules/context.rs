//! `context`: the context-window bar with band colors and the auto-compaction marker.

use crate::ansi::{Segment, Style};
use crate::claude_settings::{self, DEFAULT_COMPACT_BUFFER};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::glyph;
use crate::num::percent_of;

use super::util::{BAR_STYLES, bar, percent, rounded, tokens};
use super::{Ctx, Module, Rendered, badge, lead, seg};

/// The `scale` choices (SPEC § 3.2): what 100 % of the bar and the
/// percentage means.
pub const SCALES: &[&str] = &["window", "usable"];

/// Below this share of the window the autocompact threshold is too small
/// to be a scale (a huge `compact_buffer_tokens`, a tiny percentage
/// override) and `usable` falls back to `window` (SPEC § 3.2).
const MIN_USABLE_PERCENT: f64 = 10.0;

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
        let mut segs: Vec<Segment> = lead(cfg, "context");
        let thresholds = cfg.nums("thresholds");
        let bands = cfg.color_list("band_colors", ctx.theme);
        // SPEC § 3.2 `scale = "usable"`: 100 % is the compaction point, so
        // the usage is measured against the threshold and the marker (and
        // its percentage) is implied rather than drawn; back to the window
        // when compaction is off or the threshold is too small to scale by.
        //
        // The threshold is read from the settings chain, which a tick touches
        // only when something needs it (CLAUDE.md § Cache and worker
        // invariants): with the marker off and the window scale, nothing
        // does, so it is not read at all.
        let usable_scale = cfg.str("scale") == "usable";
        let wants_threshold =
            usable_scale || cfg.bool("compaction_marker") || cfg.bool("show_compaction_percent");
        let threshold = wants_threshold.then(|| threshold_percent(ctx, cfg, window)).flatten();
        let usable =
            usable_scale.then_some(threshold).flatten().filter(|t| *t >= MIN_USABLE_PERCENT);
        let pct = used
            .map(crate::num::clamp_percent)
            .map(|u| usable.map_or(u, |scale| crate::num::clamp_percent(u * 100.0 / scale)));
        let fill_color = ctx.theme.band(rounded(pct.unwrap_or(0.0)), &thresholds, &bands);

        let marker =
            if usable.is_some() || !cfg.bool("compaction_marker") { None } else { threshold };
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
        // The label follows the threshold, not the marker: the two are
        // separate switches, and only the `usable` scale hides both (the
        // percentage would read a constant 100 %).
        if cfg.bool("show_compaction_percent")
            && usable.is_none()
            && let Some(m) = threshold
        {
            segs.push(seg(cfg, format!(" {}{}", cfg.icon("compact"), percent(m)), "marker"));
        }
        if cfg.bool("show_window") {
            segs.push(seg(cfg, format!(" {}", tokens(window)), "window"));
        }
        if cfg.bool("exceeds_200k") && ctx.payload.exceeds_200k_tokens == Some(true) {
            segs.extend(badge(cfg, "exceeds", "exceeds"));
        }
        let warn_at = cfg.float("warn_at");
        if warn_at > 0.0 && pct.is_some_and(|p| p >= warn_at) {
            segs.extend(badge(cfg, "warn", "warn"));
        }
        Rendered::fresh(segs)
    }
}

fn opts() -> Vec<OptSpec> {
    vec![
        OptSpec::new("width", Kind::Int, "Bar width in cells; 0 hides the bar.", Value::Int(20))
            .minimal(Value::Int(0))
            .full(Value::Int(30))
            .max(crate::config::MAX_CELLS),
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
            "scale",
            Kind::Enum(SCALES),
            "What 100 % means: `window` the whole context window; `usable` the auto-compaction threshold, so the bar and the percentage say how close compaction is (the marker and its percentage are then implied and not drawn; the window tag still names the real window). `usable` falls back to `window` when compaction is disabled or the threshold is under a tenth of the window.",
            Value::Str("window".into()),
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

/// The auto-compaction threshold (SPEC § 2.3) as a percentage of the
/// window, whatever `compaction_marker` says; `None` when compaction is
/// disabled. The marker and the `usable` scale both read it.
fn threshold_percent(ctx: &Ctx<'_>, cfg: &ModuleCfg, window: u64) -> Option<f64> {
    let ac = claude_settings::resolve(&ctx.settings_env, ctx.settings());
    let threshold = ac.threshold(window, cfg.int("compact_buffer_tokens"))?;
    Some(percent_of(threshold, window))
}

#[cfg(test)]
mod tests {
    use crate::ansi::strip_ansi;
    use crate::render::{Clock, render_plain_at};

    /// The context module alone, plain, with `used_percentage` on a 1M
    /// window and the given auto-compaction environment.
    fn render(used: f64, extra: &str, env: crate::claude_settings::Env) -> String {
        let payload = crate::payload::Payload::parse(&format!(
            "{{\"session_id\": \"s\", \"context_window\": {{\"context_window_size\": 1000000, \"used_percentage\": {used}}}}}"
        ))
        .unwrap();
        let text = format!(
            "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"context\"]\n[modules.context]\nwidth = 10\n{extra}"
        );
        let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        let clock = Clock { settings_env: env, ..Clock::fixed() };
        strip_ansi(&render_plain_at(&payload, &config, Some(80), &clock)).trim_end().to_owned()
    }

    /// SPEC § 3.2 `scale = "usable"`: the percentage and the bar are
    /// measured against the threshold (98.7 % of 1M by default), capped at
    /// 100, the marker and its label are not drawn, the window tag still
    /// names the window; `window` is today's render, byte for byte.
    #[test]
    fn usable_scale_measures_against_the_compaction_threshold() {
        let env = crate::claude_settings::Env::default();
        assert_eq!(render(50.0, "", env.clone()), "⊞ █████░░░░▏ 50%");
        assert_eq!(render(50.0, "scale = \"window\"\n", env.clone()), "⊞ █████░░░░▏ 50%");
        // 50 / 98.7 = 50.66 → 51 %; no marker cell.
        assert_eq!(render(50.0, "scale = \"usable\"\n", env.clone()), "⊞ █████░░░░░ 51%");
        // At the threshold exactly: 100 %; above it: still 100 %.
        assert_eq!(render(98.7, "scale = \"usable\"\n", env.clone()), "⊞ ██████████ 100%");
        assert_eq!(render(99.5, "scale = \"usable\"\n", env.clone()), "⊞ ██████████ 100%");
        assert_eq!(render(98.6, "scale = \"usable\"\n", env.clone()), "⊞ █████████▉ 100%");
        // The full preset: no `⤓` label under `usable`, the window tag stays.
        let full = "preset = \"full\"\nscale = \"usable\"\n";
        assert_eq!(render(80.0, full, env.clone()), "⊞ ████████░░ 81% 1.0M");
        let full_window = "preset = \"full\"\n";
        assert_eq!(render(80.0, full_window, env), "⊞ ████████░▏ 80% ⤓99% 1.0M");
        // A lower configured window moves the threshold: 500k − 13k = 48.7 %
        // of 1M, so 40 % used reads 82 %.
        let low =
            crate::claude_settings::Env { window: Some("500000".into()), ..Default::default() };
        assert_eq!(render(40.0, "scale = \"usable\"\n", low), "⊞ ████████▏░ 82%");
    }

    /// SPEC § 3.2: `usable` falls back to `window` when compaction is
    /// disabled and when the threshold is under a tenth of the window.
    #[test]
    fn usable_scale_falls_back_to_the_window() {
        let disabled =
            crate::claude_settings::Env { disable: Some("1".into()), ..Default::default() };
        assert_eq!(render(50.0, "scale = \"usable\"\n", disabled.clone()), "⊞ █████░░░░░ 50%");
        assert_eq!(render(50.0, "", disabled), "⊞ █████░░░░░ 50%", "no marker either way");
        // 50 000 − 13 000 = 3.7 % of the window: too small to scale by, so
        // the window scale and its marker (in the first cell) stay.
        let tiny =
            crate::claude_settings::Env { window: Some("50000".into()), ..Default::default() };
        assert_eq!(render(50.0, "scale = \"usable\"\n", tiny.clone()), "⊞ ▏████░░░░░ 50%");
        assert_eq!(render(50.0, "", tiny), "⊞ ▏████░░░░░ 50%");
        // Exactly a tenth (113 000 → 10.0 %) is enough.
        let tenth =
            crate::claude_settings::Env { window: Some("113000".into()), ..Default::default() };
        assert_eq!(render(5.0, "scale = \"usable\"\n", tenth), "⊞ █████░░░░░ 50%");
        // `compaction_marker` governs drawing alone: off, the scale still
        // measures against the threshold (SPEC § 3.2 keys the fallback on
        // § 2.3's enabled state).
        let env = crate::claude_settings::Env::default();
        let no_marker = "scale = \"usable\"\ncompaction_marker = false\n";
        assert_eq!(render(50.0, no_marker, env.clone()), "⊞ █████░░░░░ 51%");
        assert_eq!(render(50.0, "compaction_marker = false\n", env.clone()), "⊞ █████░░░░░ 50%");
        // Bands and `warn_at` follow the displayed percentage.
        let warn = "scale = \"usable\"\nwarn_at = 90\n";
        assert_eq!(render(89.0, warn, env.clone()), "⊞ █████████░ 90% ⚠");
        assert_eq!(render(88.0, warn, env), "⊞ ████████▉░ 89%");
    }

    /// SPEC § 3.2: `compaction_marker` draws the marker and
    /// `show_compaction_percent` prints the label; they are separate
    /// switches, so the label works with the marker off (it used to be
    /// gated on the marker and produce nothing). Only `usable` hides both,
    /// where the label would read a constant 100 %.
    #[test]
    fn the_compaction_label_and_the_marker_are_separate_switches() {
        let env = crate::claude_settings::Env::default();
        let label = "show_compaction_percent = true\n";
        assert_eq!(render(50.0, label, env.clone()), "⊞ █████░░░░▏ 50% ⤓99%");
        assert_eq!(
            render(50.0, &format!("{label}compaction_marker = false\n"), env.clone()),
            "⊞ █████░░░░░ 50% ⤓99%"
        );
        assert_eq!(render(50.0, "compaction_marker = false\n", env.clone()), "⊞ █████░░░░░ 50%");
        // `usable` hides both; compaction disabled leaves nothing to print.
        assert_eq!(render(50.0, &format!("{label}scale = \"usable\"\n"), env), "⊞ █████░░░░░ 51%");
        let disabled =
            crate::claude_settings::Env { disable: Some("1".into()), ..Default::default() };
        assert_eq!(render(50.0, label, disabled), "⊞ █████░░░░░ 50%");
    }
}
