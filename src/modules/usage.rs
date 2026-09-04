//! `limit5h`, `limit7d`, `spend`, `cost`: subscription rate limits and API spend.

use crate::ansi::{Segment, Style};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::{Glyph, glyph};
use crate::payload::RateWindow;

use super::util::{bar, dollars, percent, percent_unclamped, rounded};
use super::{Ctx, Module, Rendered, icon, seg};

/// Which rate-limit window a limit module shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// `rate_limits.five_hour`.
    FiveHour,
    /// `rate_limits.seven_day`.
    SevenDay,
    /// `rate_limits.spend_limit`.
    Spend,
}

/// A rate-limit window module.
pub struct LimitModule(pub Window);

impl LimitModule {
    fn window<'a>(&self, ctx: &'a Ctx<'_>) -> Option<&'a RateWindow> {
        let rl = ctx.payload.rate_limits.as_ref()?;
        match self.0 {
            Window::FiveHour => rl.five_hour.as_ref(),
            Window::SevenDay => rl.seven_day.as_ref(),
            Window::Spend => rl.spend_limit.as_ref(),
        }
    }

    const fn id(&self) -> &'static str {
        match self.0 {
            Window::FiveHour => "limit5h",
            Window::SevenDay => "limit7d",
            Window::Spend => "spend",
        }
    }
}

impl Module for LimitModule {
    fn schema(&self) -> ModuleSchema {
        let (summary, doc, sources, icon_glyph): (&str, &str, &[&str], Glyph) = match self.0 {
            Window::FiveHour => (
                "Five-hour rate limit usage and time until reset.",
                "Percentage of the rolling five-hour window consumed and a countdown to `resets_at`. Only present for Claude.ai Pro/Max subscriptions; hidden otherwise.",
                &["rate_limits.five_hour.used_percentage", "rate_limits.five_hour.resets_at"],
                glyph("\u{f252}", "⧗", "⏳", "5h"),
            ),
            Window::SevenDay => (
                "Seven-day rate limit usage and time until reset.",
                "Percentage of the rolling seven-day window consumed and a countdown to `resets_at`. Only present for Claude.ai Pro/Max subscriptions; hidden otherwise.",
                &["rate_limits.seven_day.used_percentage", "rate_limits.seven_day.resets_at"],
                glyph("\u{f073}", "▦", "📅", "7d"),
            ),
            Window::Spend => (
                "Spend-limit usage behind a Claude apps gateway.",
                "Percentage of the applicable spend limit consumed (can exceed 100%) and a countdown to the period reset. Hidden unless a gateway reports it.",
                &["rate_limits.spend_limit.used_percentage", "rate_limits.spend_limit.resets_at"],
                glyph("\u{f0d6}", "¤", "💳", "spend"),
            ),
        };
        ModuleSchema {
            id: self.id(),
            summary,
            doc,
            sources,
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the window icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_reset",
                    Kind::Bool,
                    "Show the countdown to the reset.",
                    Value::Bool(true),
                )
                .minimal(Value::Bool(false)),
                OptSpec::new(
                    "bar_width",
                    Kind::Int,
                    "Mini bar width in cells; 0 hides it.",
                    Value::Int(0),
                )
                .full(Value::Int(8)),
                OptSpec::new(
                    "thresholds",
                    Kind::NumList,
                    "Ascending percentages where the color changes.",
                    Value::NumList(vec![50.0, 75.0, 90.0]),
                ),
                OptSpec::new(
                    "band_colors",
                    Kind::ColorList,
                    "One color per band.",
                    Value::StrList(vec![
                        "band1".into(),
                        "band2".into(),
                        "band3".into(),
                        "band4".into(),
                    ]),
                ),
            ],
            icons: vec![
                IconSpec { key: "window", doc: "Window icon.", glyph: icon_glyph },
                IconSpec {
                    key: "reset",
                    doc: "Countdown glyph.",
                    glyph: glyph("\u{f017}", "⏱", "⏱\u{fe0f}", "reset"),
                },
                IconSpec {
                    key: "fill", doc: "Bar filled cell.", glyph: glyph("█", "█", "█", "#")
                },
                IconSpec {
                    key: "empty", doc: "Bar empty cell.", glyph: glyph("░", "░", "░", "-")
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "reset", doc: "Countdown.", default: "muted" },
                ColorSpec { key: "empty", doc: "Bar empty part.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(w) = self.window(ctx) else { return Rendered::empty() };
        let Some(used) = w.used_percentage else { return Rendered::empty() };
        let thresholds = cfg.nums("thresholds");
        let bands = cfg.color_list("band_colors", ctx.theme);
        let color = ctx.theme.band(rounded(used), &thresholds, &bands);
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "window", "icon"));
        }
        let bw = cfg.size("bar_width");
        if bw > 0 {
            segs.extend(bar(
                bw,
                used,
                cfg.icon("fill"),
                cfg.icon("empty"),
                color,
                cfg.color("empty"),
                None,
            ));
            segs.push(Segment::plain(" "));
        }
        let text = if self.0 == Window::Spend { percent_unclamped(used) } else { percent(used) };
        segs.push(Segment::styled(text, Style::fg(color).bolded()));
        if cfg.bool("show_reset")
            && let Some(cd) = w.resets_at.and_then(crate::time::countdown)
        {
            let g = cfg.icon("reset");
            let glyph_txt = if g.is_empty() { String::new() } else { format!("{g} ") };
            segs.push(seg(cfg, format!(" {glyph_txt}{cd}"), "reset"));
        }
        Rendered::fresh(segs)
    }
}

/// `cost`: estimated session cost in dollars.
pub struct CostModule;

impl Module for CostModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "cost",
            summary: "Estimated session cost in USD.",
            doc: "Shows `cost.total_cost_usd`. By default it is hidden for subscription sessions (those report `rate_limits`), so one usage line serves both auth modes; set `only_without_rate_limits = false` to always show it.",
            sources: &[
                "cost.total_cost_usd",
                "cost.total_lines_added",
                "cost.total_lines_removed",
                "rate_limits",
            ],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the cost icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new("decimals", Kind::Int, "Decimal places.", Value::Int(2)),
                OptSpec::new(
                    "only_without_rate_limits",
                    Kind::Bool,
                    "Hide when the harness reports subscription rate limits.",
                    Value::Bool(true),
                ),
                OptSpec::new(
                    "show_lines",
                    Kind::Bool,
                    "Append lines added/removed.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![
                IconSpec {
                    key: "cost", doc: "Cost icon.", glyph: glyph("\u{f155}", "", "💵", "")
                },
                IconSpec {
                    key: "added",
                    doc: "Lines-added glyph.",
                    glyph: glyph("+", "+", "+", "+"),
                },
                IconSpec {
                    key: "removed",
                    doc: "Lines-removed glyph.",
                    glyph: glyph("−", "−", "−", "-"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "ok" },
                ColorSpec { key: "amount", doc: "Amount.", default: "text" },
                ColorSpec { key: "added", doc: "Lines added.", default: "ok" },
                ColorSpec { key: "removed", doc: "Lines removed.", default: "danger" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        if cfg.bool("only_without_rate_limits") && ctx.payload.is_subscription() {
            return Rendered::empty();
        }
        let Some(cost) = ctx.payload.cost.as_ref() else { return Rendered::empty() };
        let usd = cost.total_cost_usd.unwrap_or(0.0);
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "cost", "icon"));
        }
        segs.push(Segment::styled(
            dollars(usd, cfg.size("decimals")),
            Style::fg(cfg.color("amount")).bolded(),
        ));
        if cfg.bool("show_lines") {
            let added = cost.total_lines_added.unwrap_or(0);
            let removed = cost.total_lines_removed.unwrap_or(0);
            segs.push(seg(cfg, format!(" {}{added}", cfg.icon("added")), "added"));
            segs.push(seg(cfg, format!(" {}{removed}", cfg.icon("removed")), "removed"));
        }
        Rendered::fresh(segs)
    }
}
