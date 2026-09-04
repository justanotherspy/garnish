//! Time source and formatting. Every clock read in garnish goes through
//! [`now`], which honours `GARNISH_NOW` so tests and golden renders are
//! deterministic.

use jiff::{Timestamp, Zoned, tz::TimeZone};

/// Environment variable that freezes the clock (epoch seconds or RFC 3339).
pub const NOW_ENV: &str = "GARNISH_NOW";

/// Current instant, or the frozen instant from `GARNISH_NOW`.
#[must_use]
pub fn now() -> Timestamp {
    std::env::var(NOW_ENV).ok().and_then(|v| parse_now(&v)).unwrap_or_else(Timestamp::now)
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

/// Local wall-clock time for [`now`], using the system zone or `tz` when given.
///
/// Falls back to UTC when the zone cannot be resolved.
#[must_use]
pub fn local(tz: Option<&str>) -> Zoned {
    let zone = tz
        .and_then(|name| TimeZone::get(name).ok())
        .unwrap_or_else(|| TimeZone::try_system().unwrap_or(TimeZone::UTC));
    now().to_zoned(zone)
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

/// Countdown from [`now`] to an epoch-seconds instant, or `None` once passed.
#[must_use]
pub fn countdown(until_epoch_secs: i64) -> Option<String> {
    let remaining = until_epoch_secs.checked_sub(now_secs())?;
    u64::try_from(remaining).ok().filter(|&r| r > 0).map(compact_duration)
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
    fn parse_now_accepts_epoch_and_rfc3339() {
        assert_eq!(parse_now("1738425600").unwrap().as_second(), 1_738_425_600);
        assert_eq!(parse_now(" 2025-02-01T16:00:00Z ").unwrap().as_second(), 1_738_425_600);
        assert!(parse_now("yesterday").is_none());
    }

    #[test]
    fn countdown_and_elapsed_are_relative_to_frozen_now() {
        // Compute against a fixed reference without touching the process env.
        let base = parse_now("1738425600").unwrap();
        let later = base.as_second() + 8_020;
        let diff = u64::try_from(later - base.as_second()).unwrap();
        assert_eq!(compact_duration(diff), "2h13m");
    }
}
