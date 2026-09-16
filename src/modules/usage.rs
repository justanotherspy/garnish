//! `limit5h`, `limit7d`, `spend`, `cost`: subscription rate limits and API spend.

use crate::ansi::{Segment, Style};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::{Glyph, glyph};
use crate::payload::RateWindow;
use crate::time::WallClock;

use super::util::{
    BAR_STYLES, bar, dollars, percent, percent_unclamped, rounded, rounded_unclamped,
};
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

impl Window {
    /// The shape this window's absolute reset reads in (SPEC § 3.3): the
    /// further off the reset, the coarser the form that identifies it. Only
    /// the five-hour window resets within the day, and it is the one whose
    /// text a ticker line wants steady.
    const fn wall_clock(self) -> WallClock {
        match self {
            Self::FiveHour => WallClock::Time,
            Self::SevenDay => WallClock::Weekday,
            Self::Spend => WallClock::Date,
        }
    }
}

/// The `reset` choices (SPEC § 3.3): how the time a window resets at shows.
pub const RESET_STYLES: &[&str] = &["countdown", "absolute", "both"];

/// The `reset` option's doc per window, following [`Window::wall_clock`].
const fn reset_doc(window: Window) -> &'static str {
    match window {
        Window::SevenDay => {
            "How the reset shows: `countdown` (`⏱3d4h`), `absolute` the local wall-clock time with its weekday, since the reset is days away (`⏱Tue 14:30`), or `both` (`3d4h (Tue 14:30)`); `show_reset = false` hides every form."
        }
        Window::FiveHour => {
            "How the reset shows: `countdown` (`⏱2h13m`), `absolute` the local wall-clock time (`⏱14:30`, no weekday or date, since this window resets within the day, so the width stays steady), or `both` (`2h13m (14:30)`); `show_reset = false` hides every form."
        }
        Window::Spend => {
            "How the reset shows: `countdown` (`⏱27d8h`), `absolute` the local date the window resets on, since it is weeks away and a clock time alone would read as tonight (`⏱Mar 1`), or `both` (`27d8h (Mar 1)`); `show_reset = false` hides every form."
        }
    }
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
                glyph("\u{f252}", "⏳", "⏳", "5h"),
            ),
            Window::SevenDay => (
                "Seven-day rate limit usage and time until reset.",
                "Percentage of the rolling seven-day window consumed and a countdown to `resets_at`. Only present for Claude.ai Pro/Max subscriptions; hidden otherwise.",
                &["rate_limits.seven_day.used_percentage", "rate_limits.seven_day.resets_at"],
                glyph("\u{f073}", "≣", "📅", "7d"),
            ),
            Window::Spend => (
                "Spend-limit usage behind a Claude apps gateway.",
                "Percentage of the applicable spend limit consumed (can exceed 100%) and a countdown to the period reset. Hidden unless a gateway reports it.",
                &["rate_limits.spend_limit.used_percentage", "rate_limits.spend_limit.resets_at"],
                glyph("\u{f0d6}", "$", "💳", "spend"),
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
                    "Show when the window resets, in the form `reset` picks.",
                    Value::Bool(true),
                )
                .minimal(Value::Bool(false)),
                OptSpec::new(
                    "reset",
                    Kind::Enum(RESET_STYLES),
                    reset_doc(self.0),
                    Value::Str("countdown".into()),
                ),
                OptSpec::new(
                    "bar_width",
                    Kind::Int,
                    "Mini bar width in cells; 0 hides it.",
                    Value::Int(0),
                )
                .full(Value::Int(8))
                .max(crate::config::MAX_CELLS),
                OptSpec::new(
                    "bar",
                    Kind::Enum(BAR_STYLES),
                    "Bar glyphs: `blocks` (the icon set's `█`/`░`, fractional cells) or `line` (`━`/`─`, `=`/`-` in the ascii set; whole cells, so no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win.",
                    Value::Str("blocks".into()),
                ),
                OptSpec::new(
                    "thresholds",
                    Kind::NumList,
                    "Ascending percentages where the color changes.",
                    Value::NumList(vec![50.0, 75.0, 90.0]),
                ),
                super::durations_opt(),
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
                    glyph: glyph("\u{f017}", "⏱", "⏰", "reset"),
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
        // The band follows the number the row prints, which for `spend` may
        // pass 100 (SPEC § 3.3). Clamping it there capped the band at the
        // one holding 100, so a threshold above 100 could never be reached.
        let shown = if self.0 == Window::Spend { rounded_unclamped(used) } else { rounded(used) };
        let color = ctx.theme.band(shown, &thresholds, &bands);
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
            && let Some(at) = w.resets_at
            && let Some(reset) = reset_text(ctx, cfg, at, self.0.wall_clock())
        {
            let g = cfg.icon("reset");
            let glyph_txt = if g.is_empty() { String::new() } else { format!("{g} ") };
            segs.push(seg(cfg, format!(" {glyph_txt}{reset}"), "reset"));
        }
        Rendered::fresh(segs)
    }
}

/// The reset in the module's `reset` form (SPEC § 3.3): the countdown, the
/// absolute time in the tick's zone in this window's [`WallClock`] shape,
/// or the countdown followed by that time in parentheses. `None` once the
/// instant has passed, whichever the form.
fn reset_text(ctx: &Ctx<'_>, cfg: &ModuleCfg, at: i64, form: WallClock) -> Option<String> {
    let countdown = ctx.countdown(cfg, at);
    let clock = ctx.wall_clock(at, form);
    match cfg.str("reset") {
        "absolute" => clock,
        "both" => countdown.zip(clock).map(|(c, t)| format!("{c} ({t})")),
        _ => countdown,
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
                OptSpec::new("decimals", Kind::Int, "Decimal places.", Value::Int(2))
                    .max(crate::config::MAX_DECIMALS),
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

#[cfg(test)]
mod tests {
    use jiff::tz::{Offset, TimeZone};

    use crate::ansi::strip_ansi;
    use crate::render::{Clock, render_plain_at};
    use crate::theme::Role;

    /// The three limit modules on one unframed line from the `spend-limit`
    /// fixture, at an instant and in a zone.
    fn render(extra: &str, now: i64, tz: TimeZone) -> String {
        let path =
            format!("{}/tests/fixtures/payloads/spend-limit.json", env!("CARGO_MANIFEST_DIR"));
        let payload =
            crate::payload::Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let text = format!(
            "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"limit5h\", \"limit7d\", \"spend\"]\n{extra}"
        );
        let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        let clock = Clock { now: jiff::Timestamp::from_second(now).unwrap(), tz, ..Clock::fixed() };
        strip_ansi(&render_plain_at(&payload, &config, Some(120), &clock)).trim_end().to_owned()
    }

    /// SPEC § 3.3 `reset`: `absolute` prints the wall-clock time in the
    /// tick's zone, with the weekday on `limit7d` only; `both` puts it in
    /// parentheses after the countdown; `countdown` is the default. The
    /// instants come from the fixture: 18:13:40 Sat (5h), Tue 20:00 (7d)
    /// and Sat 1 March 00:00 (spend), all UTC; the spend window is over
    /// its limit, which `spend` prints unclamped.
    #[test]
    fn reset_forms_follow_the_zone_and_each_windows_shape() {
        let all = |form: &str| {
            format!(
                "[modules.limit5h]\nreset = \"{form}\"\n[modules.limit7d]\nreset = \"{form}\"\n[modules.spend]\nreset = \"{form}\"\n"
            )
        };
        let at = 1_738_425_600;
        assert_eq!(render("", at, TimeZone::UTC), "⏳ 24% ⏱ 2h13m  ≣ 41% ⏱ 3d4h  $ 112% ⏱ 27d8h");
        assert_eq!(render(&all("countdown"), at, TimeZone::UTC), render("", at, TimeZone::UTC));
        assert_eq!(
            render(&all("absolute"), at, TimeZone::UTC),
            "⏳ 24% ⏱ 18:13  ≣ 41% ⏱ Tue 20:00  $ 112% ⏱ Mar 1"
        );
        assert_eq!(
            render(&all("both"), at, TimeZone::UTC),
            "⏳ 24% ⏱ 2h13m (18:13)  ≣ 41% ⏱ 3d4h (Tue 20:00)  $ 112% ⏱ 27d8h (Mar 1)"
        );
        // Another instant: the countdown moves, the absolute time does not.
        let later = at + 3_600;
        assert_eq!(
            render(&all("both"), later, TimeZone::UTC),
            "⏳ 24% ⏱ 1h13m (18:13)  ≣ 41% ⏱ 3d3h (Tue 20:00)  $ 112% ⏱ 27d7h (Mar 1)"
        );
        // Another zone: the times shift with it, the weekday and date too.
        let plus_five = TimeZone::fixed(Offset::constant(5));
        assert_eq!(
            render(&all("absolute"), at, plus_five),
            "⏳ 24% ⏱ 23:13  ≣ 41% ⏱ Wed 01:00  $ 112% ⏱ Mar 1"
        );
        // Past the instant nothing shows in any form; `show_reset = false`
        // hides every form; the module's own `durations` shapes `both`.
        let past = 1_738_699_201;
        assert_eq!(render(&all("absolute"), past, TimeZone::UTC), "⏳ 24%  ≣ 41%  $ 112% ⏱ Mar 1");
        assert_eq!(
            render(&all("both"), past, TimeZone::UTC),
            "⏳ 24%  ≣ 41%  $ 112% ⏱ 24d3h (Mar 1)"
        );
        let hidden = "[modules.limit5h]\nreset = \"absolute\"\nshow_reset = false\n[modules.limit7d]\nreset = \"both\"\nshow_reset = false\n[modules.spend]\nshow_reset = false\n";
        assert_eq!(render(hidden, at, TimeZone::UTC), "⏳ 24%  ≣ 41%  $ 112%");
        let fixed = "[modules.limit7d]\nreset = \"both\"\ndurations = \"fixed\"\n";
        assert!(render(fixed, at, TimeZone::UTC).contains("⏱ 3d04h (Tue 20:00)"));
    }

    /// SPEC § 3.3: `spend` prints a percentage that may pass 100, so its
    /// band follows that number too. It used to be clamped to 100 before
    /// the band was chosen, so a threshold above 100 could never be reached
    /// and the fixture's 112 % was coloured as the middle band.
    #[test]
    fn the_spend_band_follows_the_unclamped_percentage() {
        let band = |thresholds: &str| {
            let text = format!(
                "[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"spend\"]\n[modules.spend]\nthresholds = {thresholds}\nband_colors = [\"ok\", \"warn\", \"hot\", \"danger\"]\n"
            );
            let path =
                format!("{}/tests/fixtures/payloads/spend-limit.json", env!("CARGO_MANIFEST_DIR"));
            let payload =
                crate::payload::Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let row = crate::render::render_lines_at(&payload, &config, Some(80), &Clock::fixed());
            let percent = row
                .first()
                .and_then(|l| l.iter().find(|s| s.text().ends_with('%')))
                .cloned()
                .unwrap_or_default();
            assert_eq!(percent.text(), "112%");
            let theme = config.theme;
            [Role::Ok, Role::Warn, Role::Hot, Role::Danger]
                .into_iter()
                .position(|r| theme.role(r) == percent.style.fg)
                .unwrap_or(usize::MAX)
        };
        // 112 % is past every threshold of the default set.
        assert_eq!(band("[50, 75, 90]"), 3);
        // With thresholds above 100 it lands where the printed number says.
        assert_eq!(band("[50, 100, 150]"), 2);
        assert_eq!(band("[50, 111, 150]"), 2);
        assert_eq!(band("[50, 113, 150]"), 1);
    }
}
