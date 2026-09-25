//! Time source and formatting. Every clock read in garnish goes through
//! [`now`], which honours `GARNISH_NOW` so tests and golden renders are
//! deterministic.

use std::path::Path;

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
            WARNED.call_once(|| {
                crate::debug::stderr_line(&format!(
                    "garnish: ignoring unparseable {NOW_ENV}={v:?}"
                ));
            });
            Timestamp::now()
        }),
        Err(_) => Timestamp::now(),
    }
}

/// The local time zone: `TZ` when it names one ([`zone`]), else
/// `/etc/localtime` read as `TZif`, else UTC.
///
/// Called once per tick, so it reads one file where it can: jiff's own
/// lookup builds its database first, a walk of the whole zoneinfo tree,
/// and every tick is a new process. That database is consulted only for a
/// `TZ` naming a zone no zoneinfo directory has a file for. A `TZ` that
/// names nothing is reported on stderr once and the system zone is used.
#[must_use]
pub fn local_zone() -> TimeZone {
    std::env::var("TZ")
        .ok()
        .filter(|v| !v.is_empty())
        .and_then(|value| {
            let tz = zone(&value);
            if tz.is_none() {
                static WARNED: std::sync::Once = std::sync::Once::new();
                WARNED.call_once(|| {
                    crate::debug::stderr_line(&format!(
                        "garnish: TZ={value:?} names no time zone; using the system zone"
                    ));
                });
            }
            tz
        })
        .or_else(|| read_tzif(Path::new("/etc/localtime"), "Local"))
        .unwrap_or(TimeZone::UTC)
}

/// Where a zone name's file is looked for after `TZDIR`: where tzdata
/// installs on Linux, the BSDs and macOS (jiff's own search list).
const ZONEINFO_DIRS: [&str; 3] =
    ["/usr/share/zoneinfo", "/usr/share/lib/zoneinfo", "/etc/zoneinfo"];

/// The most of a zone file read; a real one is a few KiB.
const MAX_TZIF_BYTES: u64 = 256 * 1024;

/// The zone a `TZ`-style value names: `TZ` itself and the `clock` module's
/// `tz` option (SPEC § 3.4).
///
/// A POSIX rule (`JST-9`, `EST5EDT,M3.2.0,M11.1.0`) is that rule. Anything
/// else, or anything after a leading `:`, is an absolute path to a `TZif`
/// file or a zone name, read from its file under `TZDIR` or the standard
/// zoneinfo directories, and only when no file has it from jiff's database
/// (which matches names case-insensitively). `None` when nothing matches.
#[must_use]
pub fn zone(value: &str) -> Option<TimeZone> {
    zone_in(value, crate::config::env_path("TZDIR").as_deref())
}

/// [`zone`] with `TZDIR` given.
fn zone_in(value: &str, tzdir: Option<&Path>) -> Option<TimeZone> {
    let (name, rule) = value.strip_prefix(':').map_or((value, true), |name| (name, false));
    if rule && let Ok(tz) = TimeZone::posix(name) {
        return Some(tz);
    }
    if Path::new(name).is_absolute() {
        return read_tzif(Path::new(name), name);
    }
    // A name never climbs out of the directory it is looked up in, and a
    // relative one is never read against the working directory.
    if name.is_empty() || name.split('/').any(|part| part == "..") {
        return None;
    }
    tzdir
        .into_iter()
        .chain(ZONEINFO_DIRS.iter().map(Path::new))
        .find_map(|dir| read_tzif(&dir.join(name), name))
        .or_else(|| TimeZone::get(name).ok())
}

/// The `TZif` file at `path`, a regular file of at most [`MAX_TZIF_BYTES`].
fn read_tzif(path: &Path, name: &str) -> Option<TimeZone> {
    let bytes = crate::claude_settings::read_regular(path, MAX_TZIF_BYTES).ok().flatten()?;
    TimeZone::tzif(name, &bytes).ok()
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

/// Environment variable that freezes every animation at frame 0 when set to
/// `0` (or another off word of the boolean hook rule, SPEC § 9).
pub const ANIMATE_ENV: &str = "GARNISH_ANIMATE";

/// Whether animations run for this process (`GARNISH_ANIMATE=0` freezes them).
#[must_use]
pub fn animate_from_env() -> bool {
    crate::claude_settings::env_flag(ANIMATE_ENV) != Some(false)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DurationStyle {
    /// [`compact_duration`]: at most two units, a zero second unit dropped.
    #[default]
    Compact,
    /// [`fixed_duration`]: two units always, the small one two digits wide.
    Fixed,
}

impl DurationStyle {
    /// Both styles, in the order the reference lists them.
    pub const ALL: [Self; 2] = [Self::Compact, Self::Fixed];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Fixed => "fixed",
        }
    }

    /// The style a config name stands for; `None` for `inherit` (a module's
    /// own `durations` deferring to the top-level key) and anything else.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
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

/// How an absolute reset time reads (SPEC § 3.3), one shape per window:
/// the further off a reset is, the coarser the form that identifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallClock {
    /// `14:30`, for a window that resets within the day (`limit5h`).
    Time,
    /// `Tue 14:30`, for a window days out (`limit7d`).
    Weekday,
    /// `Mar 1`, for a window weeks out (`spend`), where a clock time alone
    /// reads as tonight.
    Date,
}

/// The wall-clock time of an instant in a zone, in one of the [`WallClock`]
/// forms (SPEC § 3.3: the absolute form of a reset time, in the zone the
/// `clock` module uses so the two agree).
#[must_use]
pub fn wall_clock(at: Timestamp, tz: &TimeZone, form: WallClock) -> String {
    let format = match form {
        WallClock::Time => "%H:%M",
        WallClock::Weekday => "%a %H:%M",
        WallClock::Date => "%b %-d",
    };
    at.to_zoned(tz.clone()).strftime(format).to_string()
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
    fn wall_clock_prints_the_zone_in_each_windows_form() {
        use WallClock::{Date, Time, Weekday};
        use jiff::tz::{Offset, TimeZone};
        let at = |secs: i64| Timestamp::from_second(secs).unwrap();
        // 2025-02-01T18:13:40Z is a Saturday; 2025-02-04T20:00:00Z a Tuesday.
        assert_eq!(wall_clock(at(1_738_433_620), &TimeZone::UTC, Time), "18:13");
        assert_eq!(wall_clock(at(1_738_433_620), &TimeZone::UTC, Weekday), "Sat 18:13");
        assert_eq!(wall_clock(at(1_738_699_200), &TimeZone::UTC, Weekday), "Tue 20:00");
        let plus_two = TimeZone::fixed(Offset::constant(2));
        assert_eq!(wall_clock(at(1_738_433_620), &plus_two, Time), "20:13");
        let minus_five = TimeZone::fixed(Offset::constant(-5));
        assert_eq!(wall_clock(at(1_738_699_200), &minus_five, Weekday), "Tue 15:00");
        // A zone shift across midnight moves the weekday with it.
        let plus_five = TimeZone::fixed(Offset::constant(5));
        assert_eq!(wall_clock(at(1_738_699_200), &plus_five, Weekday), "Wed 01:00");
        // The date form: no padding, so a single-digit day does not carry a
        // zero or a gap into a row whose width is measured in cells.
        assert_eq!(wall_clock(at(1_740_787_200), &TimeZone::UTC, Date), "Mar 1");
        assert_eq!(wall_clock(at(1_738_433_620), &TimeZone::UTC, Date), "Feb 1");
        assert_eq!(wall_clock(at(1_739_000_000), &TimeZone::UTC, Date), "Feb 8");
        assert_eq!(wall_clock(at(1_740_000_000), &TimeZone::UTC, Date), "Feb 19");
        // The date follows the zone across midnight like the weekday does.
        assert_eq!(wall_clock(at(1_740_787_200), &minus_five, Date), "Feb 28");
    }

    /// A `TZif` file of one fixed offset, the smallest shape the format has
    /// (version 1: no transitions, one type), so the resolver's tests need
    /// no zoneinfo on the machine.
    fn fixed_tzif(offset_secs: i32, abbreviation: &str) -> Vec<u8> {
        let mut out = b"TZif".to_vec();
        out.extend([0_u8; 16]);
        let chars = u32::try_from(abbreviation.len().saturating_add(1)).unwrap();
        for count in [0_u32, 0, 0, 0, 1, chars] {
            out.extend(count.to_be_bytes());
        }
        out.extend(offset_secs.to_be_bytes());
        out.extend([0, 0]);
        out.extend(abbreviation.as_bytes());
        out.push(0);
        out
    }

    fn offset_at(tz: &TimeZone, secs: i64) -> i32 {
        tz.to_offset(Timestamp::from_second(secs).unwrap()).seconds()
    }

    /// 2025-07-01T00:00:00Z: summer in the north, so a DST rule is in force.
    const JULY: i64 = 1_751_328_000;

    /// SPEC § 3.4: `TZ` is a POSIX rule, a zone name read from its file, or
    /// a path; `:` marks a name or path; a name never climbs out of the
    /// zoneinfo directory and never resolves against the working directory.
    #[test]
    fn a_tz_value_resolves_as_a_rule_a_name_or_a_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Test")).unwrap();
        std::fs::create_dir_all(dir.path().join("sub/Dir")).unwrap();
        let file = dir.path().join("Test/Zone");
        std::fs::write(&file, fixed_tzif(7_200, "TST")).unwrap();
        let tzdir = Some(dir.path());
        let offset = |value: &str| zone_in(value, tzdir).map(|tz| offset_at(&tz, JULY));
        // POSIX rules, which no zoneinfo file names.
        assert_eq!(offset("JST-9"), Some(9 * 3_600));
        assert_eq!(offset("EST5EDT,M3.2.0,M11.1.0"), Some(-4 * 3_600));
        assert_eq!(offset("<+0330>-3:30"), Some(12_600));
        // A name, read from `TZDIR`, with or without the `:`.
        assert_eq!(offset("Test/Zone"), Some(7_200));
        assert_eq!(offset(":Test/Zone"), Some(7_200));
        // A path, with or without the `:`.
        let path = file.display().to_string();
        assert_eq!(offset(&path), Some(7_200));
        assert_eq!(offset(&format!(":{path}")), Some(7_200));
        // Nothing: an unknown name, a name that climbs, a directory, an
        // empty name, a `:` rule (the colon makes it a name), a relative
        // name that is only a file in the working directory.
        let sub = Some(dir.path().join("sub"));
        assert_eq!(zone_in("../Test/Zone", sub.as_deref()).map(|_| ()), None);
        for value in ["Not/AZone", "Dir", "sub/Dir", ":", ":JST-9", "Cargo.toml", "src/time.rs"] {
            assert_eq!(offset(value), None, "{value:?}");
        }
    }

    /// The system's own zone files are read the same way, and agree with
    /// jiff's database lookup, where the machine has them.
    #[test]
    fn a_zone_name_reads_the_system_zoneinfo_file() {
        if !Path::new("/usr/share/zoneinfo/Europe/Berlin").is_file() {
            return;
        }
        let read = zone_in("Europe/Berlin", None).unwrap();
        let db = TimeZone::get("Europe/Berlin").unwrap();
        assert_eq!(offset_at(&read, JULY), 7_200);
        assert_eq!(offset_at(&read, JULY), offset_at(&db, JULY));
        assert_eq!(read.iana_name(), Some("Europe/Berlin"));
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
