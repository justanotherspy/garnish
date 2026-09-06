//! Time source and formatting. Every clock read in garnish goes through
//! [`now`], which honours `GARNISH_NOW` so tests and golden renders are
//! deterministic.

use jiff::{Timestamp, tz::TimeZone};
use serde::Deserialize;

/// Environment variable that freezes the clock (epoch seconds or RFC 3339).
pub const NOW_ENV: &str = "GARNISH_NOW";

/// Current instant, or the frozen instant from `GARNISH_NOW`.
///
/// An unparseable `GARNISH_NOW` is reported on stderr once and ignored, so a
/// typo in a test harness cannot silently drift golden renders.
#[must_use]
pub fn now() -> Timestamp {
    match std::env::var(NOW_ENV) {
        Ok(v) if v.trim().is_empty() => Timestamp::now(),
        Ok(v) => parse_now(&v).unwrap_or_else(|| {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| eprintln!("garnish: ignoring unparseable {NOW_ENV}={v:?}"));
            Timestamp::now()
        }),
        Err(_) => Timestamp::now(),
    }
}

/// The local time zone, resolved without scanning the system zoneinfo
/// database: `TZ` when set, else `/etc/localtime` read as `TZif`, else UTC.
///
/// Reading one file is an order of magnitude cheaper than jiff's system
/// zone discovery and is called once per tick.
#[must_use]
pub fn local_zone() -> TimeZone {
    if let Ok(name) = std::env::var("TZ")
        && !name.is_empty()
    {
        if let Ok(tz) = TimeZone::get(&name) {
            return tz;
        }
        if let Ok(bytes) = std::fs::read(&name)
            && let Ok(tz) = TimeZone::tzif("TZ", &bytes)
        {
            return tz;
        }
    }
    std::fs::read("/etc/localtime")
        .ok()
        .and_then(|bytes| TimeZone::tzif("Local", &bytes).ok())
        .unwrap_or(TimeZone::UTC)
}

/// Parse a `GARNISH_NOW` value: integer epoch seconds or an RFC 3339 string.
#[must_use]
pub fn parse_now(value: &str) -> Option<Timestamp> {
    let v = value.trim();
    v.parse::<i64>()
        .ok()
        .and_then(|secs| Timestamp::from_second(secs).ok())
        .or_else(|| v.parse::<Timestamp>().ok())
}

/// Environment variable that freezes every animation at frame 0 when set to `0`.
pub const ANIMATE_ENV: &str = "GARNISH_ANIMATE";

/// Whether animations run for this process (`GARNISH_ANIMATE=0` freezes them).
#[must_use]
pub fn animate_from_env() -> bool {
    !std::env::var(ANIMATE_ENV).is_ok_and(|v| v.trim() == "0")
}

/// The one stateless animation rule (SPEC § 4.2): the frame index or scroll
/// offset at `now` is `floor(now_secs × step) mod period`.
///
/// Every animation in garnish (spinner frames, scrolling text, the ticker,
/// a patterned rule) derives from this, so no state is kept between ticks, a
/// cancelled tick loses nothing, every session on the machine animates in
/// step, and `GARNISH_NOW` freezes everything for goldens. `step` below 1
/// slows an animation (0.5 = every second tick). A zero period, or a step
/// that is not a positive finite number, gives frame 0.
#[must_use]
pub fn frame(now: Timestamp, step: f64, period: usize) -> usize {
    if period == 0 || !step.is_finite() || step <= 0.0 {
        return 0;
    }
    let secs = u64::try_from(now.as_second()).unwrap_or(0);
    let ticks = crate::num::floor_to_u64(crate::num::u64_to_f64(secs) * step);
    let period = u64::try_from(period).unwrap_or(u64::MAX);
    crate::num::u64_to_usize(ticks.checked_rem(period).unwrap_or(0))
}

/// Epoch seconds of [`now`].
#[must_use]
pub fn now_secs() -> i64 {
    now().as_second()
}

/// Epoch milliseconds of [`now`] (saturating on absurd values).
#[must_use]
pub fn now_millis() -> i64 {
    now().as_millisecond()
}

/// Compact duration such as `1h12m`, `8m20s`, `3d4h`, `47s`.
///
/// Two units at most; the second unit is dropped when it is zero.
#[must_use]
pub fn compact_duration(total_secs: u64) -> String {
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let mins = (total_secs % 3_600) / 60;
    let secs = total_secs % 60;
    let pair = |big: u64, big_unit: &str, small: u64, small_unit: &str| {
        if small == 0 {
            format!("{big}{big_unit}")
        } else {
            format!("{big}{big_unit}{small}{small_unit}")
        }
    };
    if days > 0 {
        pair(days, "d", hours, "h")
    } else if hours > 0 {
        pair(hours, "h", mins, "m")
    } else if mins > 0 {
        pair(mins, "m", secs, "s")
    } else {
        format!("{secs}s")
    }
}

/// Fixed-width duration such as `0m47s`, `9m00s`, `1h05m`, `3d04h`.
///
/// Always two units, the small one zero-padded to two digits, so a ticking
/// value only changes width when the large unit gains a digit or the unit
/// pair changes (`59m59s` → `1h00m`).
#[must_use]
pub fn fixed_duration(total_secs: u64) -> String {
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let mins = (total_secs % 3_600) / 60;
    let secs = total_secs % 60;
    if days > 0 {
        format!("{days}d{hours:02}h")
    } else if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else {
        format!("{mins}m{secs:02}s")
    }
}

/// How elapsed times and countdowns print (top-level `durations` key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DurationStyle {
    /// [`compact_duration`]: at most two units, a zero second unit dropped.
    #[default]
    Compact,
    /// [`fixed_duration`]: two units always, the small one two digits wide.
    Fixed,
}

impl DurationStyle {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Fixed => "fixed",
        }
    }

    /// Format a duration in this style.
    #[must_use]
    pub fn format(self, total_secs: u64) -> String {
        match self {
            Self::Compact => compact_duration(total_secs),
            Self::Fixed => fixed_duration(total_secs),
        }
    }

    /// Countdown from an explicit instant to an epoch-seconds instant in
    /// this style, or `None` once passed (renders pass the tick's clock so
    /// a pinned clock pins the countdown too).
    #[must_use]
    pub fn countdown_at(self, until_epoch_secs: i64, now_epoch_secs: i64) -> Option<String> {
        let remaining = until_epoch_secs.checked_sub(now_epoch_secs)?;
        u64::try_from(remaining).ok().filter(|&r| r > 0).map(|r| self.format(r))
    }
}

/// Compact countdown from an explicit instant; see
/// [`DurationStyle::countdown_at`]. Renders go through
/// `Ctx::countdown`, which picks the module's style, so this is test-only.
#[cfg(test)]
fn countdown_at(until_epoch_secs: i64, now_epoch_secs: i64) -> Option<String> {
    DurationStyle::Compact.countdown_at(until_epoch_secs, now_epoch_secs)
}

/// Seconds elapsed since an epoch-seconds instant (zero when in the future).
#[must_use]
pub fn elapsed_since(epoch_secs: i64) -> u64 {
    now_secs().checked_sub(epoch_secs).and_then(|d| u64::try_from(d).ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_durations() {
        assert_eq!(compact_duration(0), "0s");
        assert_eq!(compact_duration(47), "47s");
        assert_eq!(compact_duration(60), "1m");
        assert_eq!(compact_duration(500), "8m20s");
        assert_eq!(compact_duration(4_320), "1h12m");
        assert_eq!(compact_duration(7_200), "2h");
        assert_eq!(compact_duration(273_600), "3d4h");
        assert_eq!(compact_duration(86_400), "1d");
    }

    #[test]
    fn fixed_durations_keep_two_units_and_two_digits() {
        assert_eq!(fixed_duration(0), "0m00s");
        assert_eq!(fixed_duration(47), "0m47s");
        assert_eq!(fixed_duration(59), "0m59s");
        assert_eq!(fixed_duration(60), "1m00s");
        assert_eq!(fixed_duration(500), "8m20s");
        assert_eq!(fixed_duration(3_599), "59m59s");
        assert_eq!(fixed_duration(3_600), "1h00m");
        assert_eq!(fixed_duration(4_320), "1h12m");
        assert_eq!(fixed_duration(7_200), "2h00m");
        assert_eq!(fixed_duration(86_399), "23h59m");
        assert_eq!(fixed_duration(86_400), "1d00h");
        assert_eq!(fixed_duration(273_600), "3d04h");
        assert_eq!(DurationStyle::Fixed.format(60), "1m00s");
        assert_eq!(DurationStyle::Compact.format(60), "1m");
        assert_eq!(DurationStyle::Fixed.countdown_at(1_060, 1_000), Some("1m00s".into()));
        assert_eq!(DurationStyle::Fixed.countdown_at(1_000, 1_000), None);
        assert_eq!(DurationStyle::Fixed.name(), "fixed");
    }

    #[test]
    fn frame_is_floor_of_seconds_times_step_mod_period() {
        let at = |secs: i64| Timestamp::from_second(secs).unwrap();
        // The docs clock: 1738425600 is a multiple of 10, so a ten-frame
        // spinner shows frame 0 there (the goldens rely on it).
        assert_eq!(frame(at(1_738_425_600), 1.0, 10), 0);
        assert_eq!(frame(at(1_738_425_601), 1.0, 10), 1);
        assert_eq!(frame(at(1_738_425_609), 1.0, 10), 9);
        assert_eq!(frame(at(1_738_425_610), 1.0, 10), 0);
        // step 0.5: every second tick; step 2: two frames per second.
        assert_eq!(frame(at(1_738_425_601), 0.5, 10), 0);
        assert_eq!(frame(at(1_738_425_602), 0.5, 10), 1);
        assert_eq!(frame(at(1_738_425_603), 0.5, 10), 1);
        assert_eq!(frame(at(1_738_425_601), 2.0, 10), 2);
        // a period that does not divide the clock still cycles
        assert_eq!(frame(at(1_738_425_600), 1.0, 7), 1_738_425_600 % 7);
        assert_eq!(frame(at(1_738_425_600), 1.0, 1), 0);
        // degenerate inputs never panic and give frame 0
        assert_eq!(frame(at(1_738_425_600), 1.0, 0), 0);
        assert_eq!(frame(at(1_738_425_600), 0.0, 10), 0);
        assert_eq!(frame(at(1_738_425_600), -1.0, 10), 0);
        assert_eq!(frame(at(1_738_425_600), f64::NAN, 10), 0);
        assert_eq!(frame(at(1_738_425_600), f64::INFINITY, 10), 0);
        assert_eq!(frame(at(-5), 1.0, 10), 0, "before the epoch counts as 0");
        // year 9999 with a huge step: the tick count saturates and still reduces
        assert!(frame(at(253_402_207_200), f64::MAX, 3) < 3);
    }

    #[test]
    fn parse_now_accepts_epoch_and_rfc3339() {
        assert_eq!(parse_now("1738425600").unwrap().as_second(), 1_738_425_600);
        assert_eq!(parse_now(" 2025-02-01T16:00:00Z ").unwrap().as_second(), 1_738_425_600);
        assert!(parse_now("yesterday").is_none());
    }

    #[test]
    fn countdown_and_elapsed_are_relative_to_frozen_now() {
        // Compute against a fixed reference without touching the process env.
        let base = parse_now("1738425600").unwrap().as_second();
        assert_eq!(countdown_at(base + 8_020, base), Some("2h13m".into()));
        assert_eq!(countdown_at(base + 1, base), Some("1s".into()));
        assert_eq!(countdown_at(base, base), None);
        assert_eq!(countdown_at(base - 1, base), None);
        assert_eq!(countdown_at(i64::MAX, i64::MIN), None, "overflow is not a countdown");
    }
}
