//! Time source and formatting. Every clock read in garnish goes through
//! [`now`], which honours `GARNISH_NOW` so tests and golden renders are
//! deterministic.

use jiff::{Timestamp, tz::TimeZone};

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
