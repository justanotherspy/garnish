//! `session`, `api`, `cache`, `clock`: time spent, time waiting, prompt cache
//! health, and the wall clock with a spinner.

use jiff::tz::TimeZone;

use crate::ansi::{Segment, Style};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::glyph;
use crate::num::{percent_of, u64_to_usize};

use super::util::{percent, tokens};
use super::{Ctx, Module, Rendered, icon, seg};

/// `session`: wall-clock session duration.
pub struct SessionModule;

impl Module for SessionModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "session",
            summary: "Session duration.",
            doc: "Wall-clock time since the session started (`cost.total_duration_ms`; resets on `/clear`). The `full` preset adds the start time.",
            sources: &["cost.total_duration_ms"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_start",
                    Kind::Bool,
                    "Append the start time (HH:MM).",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![IconSpec {
                key: "session",
                doc: "Session icon.",
                glyph: glyph("\u{f017}", "⏱", "⌚", "t:"),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "value", doc: "Duration.", default: "text" },
                ColorSpec { key: "start", doc: "Start time.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(ms) = ctx.payload.cost.as_ref().and_then(|c| c.total_duration_ms) else {
            return Rendered::empty();
        };
        let elapsed = ms / 1000;
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "session", "icon"));
        }
        segs.push(seg(cfg, ctx.duration(elapsed), "value"));
        if cfg.bool("show_start")
            && let Ok(started) = ctx
                .now
                .checked_sub(jiff::SignedDuration::from_secs(i64::try_from(elapsed).unwrap_or(0)))
        {
            let zoned = started.to_zoned(ctx.tz.clone());
            segs.push(seg(cfg, format!(" since {}", zoned.strftime("%H:%M")), "start"));
        }
        Rendered::fresh(segs)
    }
}

/// `api`: time spent waiting on the API.
pub struct ApiModule;

impl Module for ApiModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "api",
            summary: "Time spent waiting for API responses.",
            doc: "`cost.total_api_duration_ms`, a subset of the session duration. The `full` preset adds its share of the session.",
            sources: &["cost.total_api_duration_ms", "cost.total_duration_ms"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_share",
                    Kind::Bool,
                    "Append the share of the session.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![IconSpec {
                key: "api",
                doc: "API icon.",
                glyph: glyph("\u{f0ec}", "⇄", "📡", "api:"),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "value", doc: "Duration.", default: "text" },
                ColorSpec { key: "share", doc: "Share of session.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(cost) = ctx.payload.cost.as_ref() else { return Rendered::empty() };
        let Some(api_ms) = cost.total_api_duration_ms else { return Rendered::empty() };
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "api", "icon"));
        }
        segs.push(seg(cfg, ctx.duration(api_ms / 1000), "value"));
        if cfg.bool("show_share")
            && let Some(total) = cost.total_duration_ms.filter(|t| *t > 0)
        {
            segs.push(seg(cfg, format!(" ({})", percent(percent_of(api_ms, total))), "share"));
        }
        Rendered::fresh(segs)
    }
}

/// `cache`: prompt cache hit ratio, TTL and warmth.
pub struct CacheModule;

impl Module for CacheModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "cache",
            summary: "Prompt cache hit ratio, TTL and warmth.",
            doc: "Hit ratio from `prompt_cache.hit_ratio` (falls back to the last request's cache-read share), the cache lifetime badge (`5m` or `1h`), and a live countdown until the cached prefix goes cold. Shows `–` before the first API response.",
            sources: &["prompt_cache.*", "context_window.current_usage"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new("show_ttl", Kind::Bool, "Show the TTL badge.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_countdown",
                    Kind::Bool,
                    "Show the warm countdown / cold state.",
                    Value::Bool(true),
                )
                .minimal(Value::Bool(false)),
                OptSpec::new("show_misses", Kind::Bool, "Show the miss count.", Value::Bool(false))
                    .full(Value::Bool(true)),
                OptSpec::new(
                    "show_writes",
                    Kind::Bool,
                    "Show tokens written to the cache.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![
                IconSpec {
                    key: "cache",
                    doc: "Cache icon.",
                    glyph: glyph("\u{f1c0}", "⛁", "💾", "cache:"),
                },
                IconSpec {
                    key: "warm",
                    doc: "Warm glyph.",
                    glyph: glyph("\u{f06d}", "✦", "🔥", "warm"),
                },
                IconSpec {
                    key: "cold",
                    doc: "Cold glyph.",
                    glyph: glyph("\u{f2dc}", "✧", "🧊", "cold"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent" },
                ColorSpec { key: "percent", doc: "Hit ratio.", default: "text" },
                ColorSpec { key: "ttl", doc: "TTL badge.", default: "muted" },
                ColorSpec { key: "warm", doc: "Warm countdown.", default: "ok" },
                ColorSpec { key: "cold", doc: "Cold state.", default: "danger" },
                ColorSpec { key: "detail", doc: "Misses and writes.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "cache", "icon"));
        }
        let pc = ctx.payload.prompt_cache.as_ref();
        let ratio = pc.and_then(|p| p.hit_ratio).or_else(|| {
            let u = ctx.payload.context_window.as_ref()?.current_usage.as_ref()?;
            let read = u.cache_read_input_tokens?;
            let total = read
                .saturating_add(u.input_tokens.unwrap_or(0))
                .saturating_add(u.cache_creation_input_tokens.unwrap_or(0));
            (total > 0).then(|| crate::num::u64_to_f64(read) / crate::num::u64_to_f64(total))
        });
        let text = ratio.map_or_else(|| "–".to_owned(), |r| percent(r * 100.0));
        segs.push(Segment::styled(text, Style::fg(cfg.color("percent")).bolded()));
        let Some(pc) = pc else { return Rendered::fresh(segs) };
        if cfg.bool("show_ttl")
            && let Some(ttl) = pc.ttl.as_deref()
        {
            segs.push(seg(cfg, format!(" {ttl}"), "ttl"));
        }
        if cfg.bool("show_countdown") {
            let warm = pc.warm.unwrap_or(false);
            let cd = pc.expires_at.and_then(|t| ctx.countdown(t));
            match (warm, cd) {
                (true, Some(cd)) => {
                    segs.push(seg(cfg, format!(" {} {cd}", cfg.icon("warm")), "warm"));
                }
                (true, None) => segs.push(seg(cfg, format!(" {}", cfg.icon("warm")), "warm")),
                (false, _) if pc.caching_observed == Some(true) => {
                    segs.push(seg(cfg, format!(" {}", cfg.icon("cold")), "cold"));
                }
                _ => {}
            }
        }
        if cfg.bool("show_misses")
            && let Some(m) = pc.misses
        {
            segs.push(seg(cfg, format!(" {m} miss{}", if m == 1 { "" } else { "es" }), "detail"));
        }
        if cfg.bool("show_writes")
            && let Some(w) = pc.cache_write_tokens
        {
            segs.push(seg(cfg, format!(" {}w", tokens(w)), "detail"));
        }
        Rendered::fresh(segs)
    }
}

/// `clock`: local time with a spinner that advances every tick.
pub struct ClockModule;

impl Module for ClockModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "clock",
            summary: "Local wall-clock time with a spinner.",
            doc: "The local time (system zone, or `tz`), preceded by a spinner whose frame is derived from the current second so it advances on every one-second tick without keeping state.",
            sources: &["wall clock"],
            refresh: 0,
            opts: vec![
                OptSpec::new(
                    "format",
                    Kind::Enum(&["24h", "12h"]),
                    "Hour format.",
                    Value::Str("24h".into()),
                ),
                OptSpec::new("seconds", Kind::Bool, "Show seconds.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new("spinner", Kind::Bool, "Show the spinner.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new("date", Kind::Bool, "Show the date.", Value::Bool(false))
                    .full(Value::Bool(true)),
                OptSpec::new("utc_offset", Kind::Bool, "Show the UTC offset.", Value::Bool(false))
                    .full(Value::Bool(true)),
                OptSpec::new(
                    "tz",
                    Kind::Str,
                    "IANA time zone; empty means the system zone.",
                    Value::Str(String::new()),
                ),
            ],
            icons: vec![IconSpec {
                key: "spinner",
                doc: "Spinner frames, one character each.",
                glyph: glyph("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "🕐🕑🕒🕓🕔🕕🕖🕗🕘🕙🕚🕛", "|/-\\"),
            }],
            colors: vec![
                ColorSpec { key: "spinner", doc: "Spinner.", default: "accent" },
                ColorSpec { key: "time", doc: "Time.", default: "text" },
                ColorSpec { key: "date", doc: "Date and offset.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let tz = cfg.str("tz");
        let zone = (!tz.is_empty())
            .then(|| TimeZone::get(tz).ok())
            .flatten()
            .unwrap_or_else(|| ctx.tz.clone());
        let zoned = ctx.now.to_zoned(zone);
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("spinner") {
            let frames: Vec<char> = cfg.icon("spinner").chars().collect();
            if !frames.is_empty() {
                let len = i64::try_from(frames.len()).unwrap_or(1).max(1);
                let idx =
                    u64_to_usize(u64::try_from(ctx.now.as_second().rem_euclid(len)).unwrap_or(0));
                if let Some(f) = frames.get(idx) {
                    segs.push(seg(cfg, format!("{f} "), "spinner"));
                }
            }
        }
        let fmt = match (cfg.str("format") == "12h", cfg.bool("seconds")) {
            (true, true) => "%I:%M:%S %p",
            (true, false) => "%I:%M %p",
            (false, true) => "%H:%M:%S",
            (false, false) => "%H:%M",
        };
        segs.push(Segment::styled(
            zoned.strftime(fmt).to_string(),
            Style::fg(cfg.color("time")).bolded(),
        ));
        if cfg.bool("date") {
            segs.push(seg(cfg, format!(" {}", zoned.strftime("%a %d %b")), "date"));
        }
        if cfg.bool("utc_offset") {
            segs.push(seg(cfg, format!(" {}", zoned.strftime("%:z")), "date"));
        }
        Rendered::fresh(segs)
    }
}
