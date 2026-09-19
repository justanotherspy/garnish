//! `limit5h`, `limit7d`, `spend`, `cost`: subscription rate limits and API spend.

use crate::ansi::{Segment, Style};
use crate::config::schema::{
    ColorSpec, IconSpec, Kind, MeasureKind, ModuleCfg, ModuleSchema, OptSpec, Value,
};
use crate::icons::{Glyph, glyph};
use crate::num::{clamp_percent, round_to_u64, u64_to_f64};
use crate::payload::RateWindow;
use crate::time::WallClock;

use super::util::{BAR_STYLES, bar};
use super::{Ctx, Module, Rendered, detail, glyph_prefix, lead, seg};

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

    /// The window's length in seconds, for the pace arithmetic of SPEC
    /// § 3.3; `None` for `spend`, whose period the payload does not say.
    const fn length_secs(self) -> Option<u64> {
        match self {
            Self::FiveHour => Some(5 * 3600),
            Self::SevenDay => Some(7 * 86_400),
            Self::Spend => None,
        }
    }

    /// The window's name in the elapsed form (`2h46m/5h`).
    const fn label(self) -> &'static str {
        match self {
            Self::FiveHour => "5h",
            Self::SevenDay => "7d",
            Self::Spend => "",
        }
    }
}

/// The `reset` choices (SPEC § 3.3): how the time a window resets at shows.
pub const RESET_STYLES: &[&str] = &["countdown", "absolute", "both"];

/// The `reset` choices of the two windows of known length: `elapsed` is
/// the time into the window over its length (SPEC § 3.3).
pub const RESET_STYLES_WINDOWED: &[&str] = &["countdown", "absolute", "both", "elapsed"];

/// The `reset` option's doc per window, following [`Window::wall_clock`].
const fn reset_doc(window: Window) -> &'static str {
    match window {
        Window::SevenDay => {
            "How the reset shows: `countdown` (`⏱3d4h`), `absolute` the local wall-clock time with its weekday, since the reset is days away (`⏱Tue 14:30`), `both` (`3d4h (Tue 14:30)`), or `elapsed` the time into the window over its length (`⏱3d20h/7d`); `show_reset = false` hides every form."
        }
        Window::FiveHour => {
            "How the reset shows: `countdown` (`⏱2h13m`), `absolute` the local wall-clock time (`⏱14:30`, no weekday or date, since this window resets within the day, so the width stays steady), `both` (`2h13m (14:30)`), or `elapsed` the time into the window over its length (`⏱2h46m/5h`); `show_reset = false` hides every form."
        }
        Window::Spend => {
            "How the reset shows: `countdown` (`⏱27d8h`), `absolute` the local date the window resets on, since it is weeks away and a clock time alone would read as tonight (`⏱Mar 1`), or `both` (`27d8h (Mar 1)`); `show_reset = false` hides every form."
        }
    }
}

/// The pace arithmetic of SPEC § 3.3 for one window at one instant, from
/// `used_percentage` and `resets_at` alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    /// Seconds into the window, at most its length.
    pub elapsed_secs: u64,
    /// The share of the window elapsed, `0..=100`.
    pub elapsed_pct: f64,
    /// `used − elapsed`, in points: positive is ahead of pace.
    pub delta: f64,
    /// `used ÷ max(elapsed, 1)`, the band's ratio.
    pub ratio: f64,
    /// Seconds until the window reaches 100 % at the current rate, only
    /// when that lands before the reset.
    pub eta_secs: Option<u64>,
}

/// [`Pace`] of a window `length_secs` long that resets at `resets_at`, seen
/// at `now`; a reset already passed reads as the window fully elapsed, and
/// one farther off than the length as its start.
#[must_use]
pub fn pace(used: f64, resets_at: i64, now: i64, length_secs: u64) -> Pace {
    let remaining = u64::try_from(resets_at.saturating_sub(now)).unwrap_or(0).min(length_secs);
    let elapsed_secs = length_secs.saturating_sub(remaining);
    let used = clamp_percent(used);
    let elapsed_pct = if length_secs == 0 {
        0.0
    } else {
        u64_to_f64(elapsed_secs) / u64_to_f64(length_secs) * 100.0
    };
    let delta = used - elapsed_pct;
    let ratio = used / elapsed_pct.max(1.0);
    let eta_secs = (used > 0.0 && used < 100.0 && elapsed_secs > 0)
        .then(|| round_to_u64(u64_to_f64(elapsed_secs) * (100.0 - used) / used))
        .filter(|eta| *eta < remaining);
    Pace { elapsed_secs, elapsed_pct, delta, ratio, eta_secs }
}

/// The pace band of SPEC § 3.3, which `pace_colors` colours the percentage by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaceBand {
    /// Used at most as much of the window as has elapsed.
    Nominal,
    /// Up to half again as much.
    Caution,
    /// More than that, or over 80 % used whatever the ratio.
    Critical,
}

impl PaceBand {
    /// The module colour key the band takes.
    const fn color_key(self) -> &'static str {
        match self {
            Self::Nominal => "pace_nominal",
            Self::Caution => "pace_caution",
            Self::Critical => "pace_critical",
        }
    }
}

/// [`PaceBand`] for a window `used` percent spent at `ratio`; `None` under
/// 20 % used, where the ratio is noise and the thresholds bands stand.
#[must_use]
pub fn pace_band(used: f64, ratio: f64) -> Option<PaceBand> {
    if used < 20.0 {
        None
    } else if used > 80.0 || ratio > 1.5 {
        Some(PaceBand::Critical)
    } else if ratio > 1.0 {
        Some(PaceBand::Caution)
    } else {
        Some(PaceBand::Nominal)
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
        // The two windows of known length take the pace keys (SPEC § 3.3);
        // `spend` has no length and takes none of them.
        let windowed = self.0.length_secs().is_some();
        let mut opts = limit_opts(self.0, windowed);
        let mut icons = vec![
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
        ];
        let mut colors = vec![
            ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
            ColorSpec { key: "reset", doc: "Countdown.", default: "muted" },
            ColorSpec { key: "empty", doc: "Bar empty part.", default: "muted" },
        ];
        if windowed {
            let (pace_opts, pace_icons, pace_colors) = pace_keys();
            opts.extend(pace_opts);
            icons.extend(pace_icons);
            colors.extend(pace_colors);
        }
        ModuleSchema {
            id: self.id(),
            measure: Some(MeasureKind::Percent),
            summary,
            doc,
            sources,
            refresh: 0,
            opts,
            icons,
            colors,
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
        let shown = if self.0 == Window::Spend {
            ctx.percent_shown_unclamped(cfg, used)
        } else {
            ctx.percent_shown(cfg, used)
        };
        // SPEC § 3.3: the pace is computed once, only when a switch wants it,
        // and only for a window whose length is known.
        let wants_pace = cfg.bool("pace")
            || cfg.bool("pace_colors")
            || cfg.bool("eta")
            || cfg.bool("elapsed_marker")
            || cfg.str("reset") == "elapsed";
        // A window whose reset has passed has no pace: the payload keeps the
        // old `resets_at` until the next response, and against it every
        // switch would read the usage as 100 % elapsed (SPEC § 3.3).
        let now = ctx.now.as_second();
        let pace = self
            .0
            .length_secs()
            .filter(|_| wants_pace)
            .zip(w.resets_at.filter(|at| *at > now))
            .map(|(length, at)| pace(used, at, now, length));
        let color = match pace.and_then(|p| pace_band(shown, p.ratio)) {
            Some(band) if cfg.bool("pace_colors") => cfg.color(band.color_key()),
            _ => ctx.theme.band(shown, &thresholds, &bands),
        };
        let mut segs: Vec<Segment> = lead(cfg, "window");
        let bw = cfg.size("bar_width");
        if bw > 0 {
            let marker = pace
                .filter(|_| cfg.bool("elapsed_marker"))
                .map(|p| (p.elapsed_pct, cfg.icon("marker"), cfg.color("marker")));
            segs.extend(bar(
                bw,
                used,
                cfg.icon("fill"),
                cfg.icon("empty"),
                color,
                cfg.color("empty"),
                marker,
            ));
            segs.push(Segment::plain(" "));
        }
        let text = if self.0 == Window::Spend {
            ctx.percent_unclamped(cfg, used)
        } else {
            ctx.percent(cfg, used)
        };
        segs.push(Segment::styled(text, Style::fg(color).bolded()));
        if cfg.bool("pace")
            && let Some(p) = pace
        {
            segs.push(pace_segment(ctx, cfg, p.delta));
        }
        if cfg.bool("eta")
            && let Some(secs) = pace.and_then(|p| p.eta_secs)
        {
            let eta = format!(" {}{}", glyph_prefix(cfg, "eta"), ctx.duration(cfg, secs));
            segs.push(seg(cfg, eta, "eta"));
        }
        if cfg.bool("show_reset")
            && let Some(at) = w.resets_at
            && let Some((reset, extra)) = reset_text(ctx, cfg, at, self.0, pace)
        {
            let before = format!(" {}{reset}", glyph_prefix(cfg, "reset"));
            match extra {
                Some(inner) => segs.extend(detail(ctx, cfg, &before, &inner, "reset")),
                None => segs.push(seg(cfg, before, "reset")),
            }
        }
        Rendered::fresh(segs).measured(super::Measure::Percent(shown))
    }
}

/// The options every limit module carries; `windowed` widens `reset` with
/// the elapsed form (SPEC § 3.3).
fn limit_opts(window: Window, windowed: bool) -> Vec<OptSpec> {
    vec![
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
            Kind::Enum(if windowed { RESET_STYLES_WINDOWED } else { RESET_STYLES }),
            reset_doc(window),
            Value::Str("countdown".into()),
        ),
        OptSpec::new("bar_width", Kind::Int, "Mini bar width in cells; 0 hides it.", Value::Int(0))
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
        super::format_opt(super::NumberKind::Percent),
        OptSpec::new(
            "band_colors",
            Kind::ColorList,
            "One color per band.",
            Value::StrList(vec!["band1".into(), "band2".into(), "band3".into(), "band4".into()]),
        ),
    ]
}

/// The pace keys of the two windows of known length (SPEC § 3.3): the
/// options, the glyphs and the colours `limit5h` and `limit7d` carry and
/// `spend` does not.
fn pace_keys() -> (Vec<OptSpec>, Vec<IconSpec>, Vec<ColorSpec>) {
    let opts = vec![
        OptSpec::new(
            "pace",
            Kind::Bool,
            "Print the difference between the share used and the share of the window elapsed: `⇡14%` ahead of pace in `colors.ahead`, `⇣32%` behind in `colors.behind`, a zero difference bare.",
            Value::Bool(false),
        ),
        OptSpec::new(
            "pace_colors",
            Kind::Bool,
            "Colour the percentage by the pace band instead of `thresholds`: used ÷ elapsed at most 1 is nominal, at most 1.5 caution, above that critical; ignored under 20 % used, always critical above 80 %.",
            Value::Bool(false),
        ),
        OptSpec::new(
            "eta",
            Kind::Bool,
            "Print the time until the window reaches 100 % at the current rate (`⇥ 1h37m`), only when that lands before the reset.",
            Value::Bool(false),
        ),
        OptSpec::new(
            "elapsed_marker",
            Kind::Bool,
            "Draw the `marker` glyph on the mini bar at the share of the window elapsed, so usage and time read together; needs `bar_width`.",
            Value::Bool(false),
        ),
    ];
    let icons = vec![
        IconSpec {
            key: "ahead", doc: "Ahead-of-pace glyph.", glyph: glyph("⇡", "⇡", "🔼", "^")
        },
        IconSpec {
            key: "behind", doc: "Behind-pace glyph.", glyph: glyph("⇣", "⇣", "🔽", "v")
        },
        IconSpec { key: "eta", doc: "Eta glyph.", glyph: glyph("\u{f04e}", "⇥", "⏩", "eta") },
        IconSpec {
            key: "marker",
            doc: "Elapsed marker on the bar.",
            glyph: glyph("▏", "▏", "▏", "|"),
        },
    ];
    let colors = vec![
        ColorSpec { key: "ahead", doc: "Pace delta, ahead.", default: "hot" },
        ColorSpec { key: "behind", doc: "Pace delta, behind.", default: "ok" },
        ColorSpec { key: "eta", doc: "Eta.", default: "hot" },
        ColorSpec { key: "marker", doc: "Elapsed marker.", default: "muted" },
        ColorSpec { key: "pace_nominal", doc: "Percentage, nominal pace.", default: "ok" },
        ColorSpec { key: "pace_caution", doc: "Percentage, caution.", default: "warn" },
        ColorSpec { key: "pace_critical", doc: "Percentage, critical.", default: "danger" },
    ];
    (opts, icons, colors)
}

/// The pace delta after the percentage (SPEC § 3.3): `⇡14%` ahead in
/// `colors.ahead`, `⇣32%` behind in `colors.behind`, a zero delta bare in
/// the `behind` colour; the number follows the module's percent style.
fn pace_segment(ctx: &Ctx<'_>, cfg: &ModuleCfg, delta: f64) -> Segment {
    let text = ctx.percent(cfg, delta.abs());
    let zero = text.chars().all(|c| matches!(c, '0' | '.' | '%'));
    let (icon_key, color_key) =
        if delta > 0.0 && !zero { ("ahead", "ahead") } else { ("behind", "behind") };
    let arrow = if zero { "" } else { cfg.icon(icon_key) };
    seg(cfg, format!(" {arrow}{text}"), color_key)
}

/// The reset in the module's `reset` form (SPEC § 3.3): the countdown or
/// the absolute time in the tick's zone in this window's [`WallClock`]
/// shape, for `both` the countdown with that time as the parenthesised
/// detail, and for `elapsed` the time into the window over its length.
/// `None` once the instant has passed, whichever the form.
fn reset_text(
    ctx: &Ctx<'_>,
    cfg: &ModuleCfg,
    at: i64,
    window: Window,
    pace: Option<Pace>,
) -> Option<(String, Option<String>)> {
    let countdown = ctx.countdown(cfg, at);
    let clock = ctx.wall_clock(at, window.wall_clock());
    match cfg.str("reset") {
        "absolute" => clock.map(|t| (t, None)),
        "both" => countdown.zip(clock).map(|(c, t)| (c, Some(t))),
        "elapsed" => countdown
            .and(pace)
            .map(|p| (format!("{}/{}", ctx.duration(cfg, p.elapsed_secs), window.label()), None)),
        _ => countdown.map(|c| (c, None)),
    }
}

/// `cost`: estimated session cost in dollars.
pub struct CostModule;

impl Module for CostModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "cost",
            measure: Some(MeasureKind::Amount),
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
                super::format_opt(super::NumberKind::Cost),
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
        let decimals = cfg.size("decimals");
        let mut segs: Vec<Segment> = lead(cfg, "cost");
        segs.push(Segment::styled(
            ctx.dollars(cfg, usd, decimals),
            Style::fg(cfg.color("amount")).bolded(),
        ));
        if cfg.bool("show_lines") {
            let added = cost.total_lines_added.unwrap_or(0);
            let removed = cost.total_lines_removed.unwrap_or(0);
            segs.push(seg(cfg, format!(" {}{added}", cfg.icon("added")), "added"));
            segs.push(seg(cfg, format!(" {}{removed}", cfg.icon("removed")), "removed"));
        }
        // The amount as printed, so `zero` is what reads as zero (SPEC § 3).
        let shown = ctx.dollars_shown(cfg, usd, decimals);
        Rendered::fresh(segs).measured(super::Measure::Amount(shown))
    }
}

#[cfg(test)]
mod tests {
    use jiff::tz::{Offset, TimeZone};

    use super::{PaceBand, pace, pace_band};
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

    /// SPEC § 3.3: the pace arithmetic from `resets_at` alone, at the two
    /// instants the goldens pin (the five-hour window of the fixtures resets
    /// at 1738433620), and the band's edges.
    #[test]
    fn pace_reads_the_window_from_resets_at() {
        let at = 1_738_425_600;
        let p = pace(23.5, 1_738_433_620, at, 18_000);
        assert_eq!(p.elapsed_secs, 9980);
        assert!((p.elapsed_pct - 55.444).abs() < 0.01, "{p:?}");
        assert!((p.delta + 31.944).abs() < 0.01, "{p:?}");
        assert!((p.ratio - 0.4238).abs() < 0.001, "{p:?}");
        assert_eq!(p.eta_secs, None, "the window resets before it is spent");
        let early = pace(23.5, 1_738_433_620, at - 8_180, 18_000);
        assert_eq!(early.elapsed_secs, 1800);
        assert!((early.elapsed_pct - 10.0).abs() < 0.001, "{early:?}");
        assert!((early.delta - 13.5).abs() < 0.001, "{early:?}");
        assert!((early.ratio - 2.35).abs() < 0.001, "{early:?}");
        assert_eq!(early.eta_secs, Some(5860));
        // A reset already passed is a window fully elapsed; one farther off
        // than the length is its start.
        assert_eq!(pace(50.0, at - 1, at, 18_000).elapsed_secs, 18_000);
        assert_eq!(pace(50.0, at + 30_000, at, 18_000).elapsed_secs, 0);
        // At the window's start the ratio divides by one, never by nothing.
        let start = pace(30.0, at + 18_000, at, 18_000);
        assert!(start.ratio.is_finite() && (start.ratio - 30.0).abs() < 1e-9, "{start:?}");
        assert_eq!(pace(0.0, at + 9_000, at, 18_000).eta_secs, None, "nothing used: no rate");
        assert_eq!(pace(100.0, at + 9_000, at, 18_000).eta_secs, None, "already spent");
        // Spent exactly at the reset is not before it.
        assert_eq!(pace(50.0, at + 9_000, at, 18_000).eta_secs, None);
        assert_eq!(pace(50.0, at + 9_001, at, 18_000).eta_secs, Some(8_999));
        // The band: noise under 20 % used, critical over 80 %, else the ratio.
        assert_eq!(pace_band(19.9, 9.0), None);
        assert_eq!(pace_band(20.0, 1.0), Some(PaceBand::Nominal));
        assert_eq!(pace_band(30.0, 1.5), Some(PaceBand::Caution));
        assert_eq!(pace_band(30.0, 1.51), Some(PaceBand::Critical));
        assert_eq!(pace_band(80.1, 0.5), Some(PaceBand::Critical));
        assert_eq!(pace_band(80.0, 0.5), Some(PaceBand::Nominal));
    }

    /// SPEC § 3.3: pace, eta and the elapsed form on the two windows whose
    /// length is known, nothing on `spend`, and nothing past the reset.
    #[test]
    fn pace_eta_and_elapsed_render_on_the_two_windows_only() {
        let all = "[modules.limit5h]\npace = true\neta = true\n[modules.limit7d]\npace = true\neta = true\n";
        let at = 1_738_425_600;
        assert_eq!(
            render(all, at, TimeZone::UTC),
            "⏳ 24% ⇣32% ⏱ 2h13m  ≣ 41% ⇣14% ⏱ 3d4h  $ 112% ⏱ 27d8h"
        );
        let early = at - 8_180;
        assert_eq!(
            render(all, early, TimeZone::UTC),
            "⏳ 24% ⇡14% ⇥ 1h37m ⏱ 4h30m  ≣ 41% ⇣12% ⏱ 3d6h  $ 112% ⏱ 27d10h"
        );
        let elapsed =
            "[modules.limit5h]\nreset = \"elapsed\"\n[modules.limit7d]\nreset = \"elapsed\"\n";
        assert_eq!(
            render(elapsed, at, TimeZone::UTC),
            "⏳ 24% ⏱ 2h46m/5h  ≣ 41% ⏱ 3d20h/7d  $ 112% ⏱ 27d8h"
        );
        let past = 1_738_699_201;
        assert_eq!(render(elapsed, past, TimeZone::UTC), "⏳ 24%  ≣ 41%  $ 112% ⏱ 24d3h");
        // Every switch dies with the countdown: the payload keeps the old
        // `resets_at` until the next response, and against it the usage
        // would read as 100 % elapsed (`⇣77%`, the marker in the last cell).
        let bars = "[modules.limit5h]\nbar_width = 8\n[modules.limit7d]\nbar_width = 8\n";
        let every = "[modules.limit5h]\nbar_width = 8\npace = true\neta = true\nelapsed_marker = true\nreset = \"elapsed\"\n[modules.limit7d]\nbar_width = 8\npace = true\neta = true\nelapsed_marker = true\nreset = \"elapsed\"\n";
        assert_eq!(
            render(every, past, TimeZone::UTC),
            "⏳ █▉░░░░░░ 24%  ≣ ███▎░░░░ 41%  $ 112% ⏱ 24d3h"
        );
        assert_eq!(render(every, past, TimeZone::UTC), render(bars, past, TimeZone::UTC));
        assert_ne!(render(every, at, TimeZone::UTC), render(bars, at, TimeZone::UTC));
        // The precise style reaches the delta; a zero delta prints bare.
        let precise = "[format]\npercent = \"precise\"\n[modules.limit5h]\npace = true\n[modules.limit7d]\npace = true\n";
        assert!(render(precise, at, TimeZone::UTC).contains("23.5% ⇣31.9%"));
        // 23.5 % of the five-hour window elapsed with 23.5 % used: a zero
        // delta prints bare, in the `behind` colour, no arrow.
        let even = 1_738_433_620 - 18_000 + 4_230;
        let row = render("[modules.limit5h]\npace = true\n", even, TimeZone::UTC);
        assert!(row.starts_with("⏳ 24% 0% ⏱ 3h49m"), "{row}");
        // `spend` takes none of the keys.
        for text in ["[modules.spend]\npace = true\n", "[modules.spend]\nreset = \"elapsed\"\n"] {
            let (_, errs) = crate::config::parse(text, &crate::modules::SCHEMAS);
            assert_eq!(errs.len(), 1, "{text}: {errs:?}");
            assert!(errs[0].path.starts_with("modules.spend."), "{errs:?}");
        }
    }

    /// SPEC § 3.3 `elapsed_marker`: the cursor is drawn only when its own
    /// switch says so, even when another switch has computed the pace.
    #[test]
    fn the_elapsed_marker_needs_its_own_switch() {
        let at = 1_738_425_600;
        let unmarked = render("[modules.limit5h]\nbar_width = 8\npace = true\n", at, TimeZone::UTC);
        assert!(unmarked.starts_with("⏳ █▉░░░░░░ 24% ⇣32%"), "{unmarked}");
        let marked =
            render("[modules.limit5h]\nbar_width = 8\nelapsed_marker = true\n", at, TimeZone::UTC);
        assert!(marked.starts_with("⏳ █▉░░▏░░░ 24% ⏱"), "{marked}");
    }

    /// SPEC § 3.3: the pace arrow takes its own colour, `ahead` when ahead
    /// of the window and `behind` when behind, whatever band the
    /// percentage is in.
    #[test]
    fn pace_arrows_take_their_own_colours() {
        let arrow = |now: i64| {
            let text = "[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"limit5h\"]\n[modules.limit5h]\npace = true\n";
            let path =
                format!("{}/tests/fixtures/payloads/spend-limit.json", env!("CARGO_MANIFEST_DIR"));
            let payload =
                crate::payload::Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            let (config, errs) = crate::config::parse(text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = Clock { now: jiff::Timestamp::from_second(now).unwrap(), ..Clock::fixed() };
            let row = crate::render::render_lines_at(&payload, &config, Some(80), &clock);
            let segment = row
                .first()
                .and_then(|l| l.iter().find(|s| s.text().contains('⇡') || s.text().contains('⇣')))
                .cloned()
                .unwrap();
            let cfg = config.modules.get("limit5h").cloned().unwrap();
            (segment.text().to_owned(), segment.style.fg, cfg)
        };
        let at = 1_738_425_600;
        let (text, fg, cfg) = arrow(at - 8_180);
        assert!(text.contains("⇡14%") && fg == cfg.color("ahead"), "{text}");
        let (text, fg, cfg) = arrow(at);
        assert!(text.contains("⇣32%") && fg == cfg.color("behind"), "{text}");
    }

    /// SPEC § 3.3 `pace_colors`: the percentage takes the pace band's
    /// colour when there is one, else the thresholds band as always.
    #[test]
    fn pace_colors_take_the_band_over_the_thresholds() {
        let percent_color = |extra: &str, now: i64| {
            let text = format!(
                "[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"limit5h\"]\n[modules.limit5h]\n{extra}"
            );
            let path =
                format!("{}/tests/fixtures/payloads/spend-limit.json", env!("CARGO_MANIFEST_DIR"));
            let payload =
                crate::payload::Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = Clock { now: jiff::Timestamp::from_second(now).unwrap(), ..Clock::fixed() };
            let row = crate::render::render_lines_at(&payload, &config, Some(80), &clock);
            let fg = row
                .first()
                .and_then(|l| l.iter().find(|s| s.text().ends_with('%')))
                .map(|s| s.style.fg)
                .unwrap();
            (fg, config.theme)
        };
        let at = 1_738_425_600;
        // 10 % elapsed, 23.5 % used: critical. The nominal band's colour is
        // overridden, since `ok` and `band1` are one colour in the palette.
        let (fg, theme) = percent_color("pace_colors = true\n", at - 8_180);
        assert_eq!(fg, theme.role(Role::Danger));
        let nominal = "pace_colors = true\n[modules.limit5h.colors]\npace_nominal = \"accent\"\n";
        let (fg, theme) = percent_color(nominal, at);
        assert_eq!(fg, theme.role(Role::Accent));
        // Off, or `pace` alone: the thresholds band as always.
        let (fg, theme) = percent_color("", at - 8_180);
        assert_eq!(fg, theme.role(Role::Band1));
        let (fg, theme) =
            percent_color("pace = true\n[modules.limit5h.colors]\npace_nominal = \"accent\"\n", at);
        assert_eq!(fg, theme.role(Role::Band1));
        // Past the reset there is no pace, so the thresholds band stands.
        let (fg, theme) = percent_color(nominal, 1_738_699_201);
        assert_eq!(fg, theme.role(Role::Band1));
    }

    /// SPEC § 3, § 4: a band threshold and a `below:N` / `above:N` rule
    /// compare the number the row prints, whichever `percent` style: the
    /// fixture's 23.5 % is 24 under `whole` and 23.5 under `precise`. The
    /// bands and the measure used to take the whole-number rounding under
    /// either style, so `thresholds = [23.7]` coloured a row printing
    /// `23.5%` as over the threshold and `below:23.6` left it in place.
    #[test]
    fn bands_and_hide_rules_compare_the_printed_number() {
        let at = 1_738_425_600;
        let below = "[modules.limit5h]\nhide = [\"below:23.6\"]\n";
        assert!(render(below, at, TimeZone::UTC).starts_with("⏳ 24%"), "24 is not below 23.6");
        let precise = format!("[format]\npercent = \"precise\"\n{below}");
        assert!(render(&precise, at, TimeZone::UTC).starts_with("≣ 41.2%"), "23.5 is below 23.6");
        let band_of = |top: &str| {
            let text = format!(
                "{top}[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"limit5h\"]\n[modules.limit5h]\nthresholds = [23.7, 75, 90]\n"
            );
            let path =
                format!("{}/tests/fixtures/payloads/spend-limit.json", env!("CARGO_MANIFEST_DIR"));
            let payload =
                crate::payload::Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = Clock { now: jiff::Timestamp::from_second(at).unwrap(), ..Clock::fixed() };
            let row = crate::render::render_lines_at(&payload, &config, Some(80), &clock);
            let fg = row
                .first()
                .and_then(|l| l.iter().find(|s| s.text().ends_with('%')))
                .map(|s| s.style.fg)
                .unwrap();
            (fg, config.theme)
        };
        let (fg, theme) = band_of("");
        assert_eq!(fg, theme.role(Role::Band2), "24 is over 23.7");
        let (fg, theme) = band_of("[format]\npercent = \"precise\"\n");
        assert_eq!(fg, theme.role(Role::Band1), "23.5 is under 23.7");
    }

    /// SPEC § 3 `zero` on `cost` reads the amount as the row prints it:
    /// `$0.00` under two decimals is zero, `$0.004` under three is not,
    /// and `$0` under `cost = "whole"` is zero whatever the cents.
    #[test]
    fn cost_zero_follows_the_printed_amount() {
        let row = |usd: &str, extra: &str| {
            let payload = crate::payload::Payload::parse(&format!(
                "{{\"session_id\": \"s\", \"cost\": {{\"total_cost_usd\": {usd}}}}}"
            ))
            .unwrap();
            let text = format!(
                "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"cost\"]\n[modules.cost]\n{extra}"
            );
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()))
                .trim_end()
                .to_owned()
        };
        let zero = "hide = [\"zero\"]\n";
        assert_eq!(row("0.004", zero), "", "$0.00 is zero");
        assert!(row("0.004", &format!("{zero}decimals = 3\n")).ends_with("$0.004"), "not zero");
        assert!(row("0.006", zero).ends_with("$0.01"));
        assert_eq!(row("0.4", &format!("{zero}cost = \"whole\"\n")), "", "$0 is zero");
        assert!(row("0.6", &format!("{zero}cost = \"whole\"\n")).ends_with("$1"));
        assert_eq!(row("-0.0", zero), "", "a negative zero is zero");
        assert!(row("-0.0", "").ends_with("$0.00"), "and prints no sign");
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
