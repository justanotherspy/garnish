//! Numeric helpers that keep the strict lints happy: no `as`, no unchecked
//! arithmetic, saturating conversions everywhere.

/// Clamp a float into `0..=100` and drop NaN to zero.
#[must_use]
pub const fn clamp_percent(value: f64) -> f64 {
    if value.is_nan() { 0.0 } else { value.clamp(0.0, 100.0) }
}

/// The largest amount a formatter prints.
///
/// A percentage allowed past 100 (`spend`) and a cost stop here, so an
/// absurd payload number (`1e300`, or an infinity from arithmetic) reads as
/// a large value rather than a 300-digit one. No real session comes near it.
pub const MAX_SHOWN: f64 = 99_999.0;

/// An amount as the formatters print it.
///
/// NaN and anything at or below zero are `0` (a negative zero would print
/// its sign), anything above [`MAX_SHOWN`] is [`MAX_SHOWN`]. The one place
/// both rules live, so the printed number and the one a band or a `hide`
/// rule compares agree.
#[must_use]
pub const fn shown_amount(value: f64) -> f64 {
    if value.is_nan() || value <= 0.0 { 0.0 } else { value.min(MAX_SHOWN) }
}

/// Round a non-negative float to the nearest integer, saturating at `u64::MAX`.
/// Negative and NaN inputs give zero.
#[must_use]
pub fn round_to_u64(value: f64) -> u64 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    let rounded = value.round();
    // `u64::MAX as f64` is exact enough for a saturation bound; compare in f64.
    if rounded >= 18_446_744_073_709_551_615.0 {
        return u64::MAX;
    }
    // Build the integer digit by digit to avoid `as`: the value is finite,
    // non-negative and below 2^64, so parsing its integer formatting is exact.
    format!("{rounded:.0}").parse().unwrap_or(u64::MAX)
}

/// Floor a non-negative float to an integer, saturating; negatives give zero.
#[must_use]
pub fn floor_to_u64(value: f64) -> u64 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    round_to_u64(value.floor())
}

/// Convert a `u64` to `f64` (lossy above 2^53, which never matters for token counts).
#[must_use]
pub fn u64_to_f64(value: u64) -> f64 {
    // u32 halves convert exactly; recombine in f64.
    let hi = u32::try_from(value >> 32).unwrap_or(u32::MAX);
    let lo = u32::try_from(value & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    f64::from(hi).mul_add(4_294_967_296.0, f64::from(lo))
}

/// Convert an `i64` to `f64` with its sign (lossy above 2^53 in magnitude,
/// as [`u64_to_f64`] is).
#[must_use]
pub fn i64_to_f64(value: i64) -> f64 {
    let magnitude = u64_to_f64(value.unsigned_abs());
    if value < 0 { -magnitude } else { magnitude }
}

/// `usize` → `f64` via `u64`.
#[must_use]
pub fn usize_to_f64(value: usize) -> f64 {
    u64_to_f64(u64::try_from(value).unwrap_or(u64::MAX))
}

/// `u64` → `usize`, saturating on 32-bit targets.
#[must_use]
pub fn u64_to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Percentage (0..=100) of `part` over `whole`; zero when `whole` is zero.
#[must_use]
pub fn percent_of(part: u64, whole: u64) -> f64 {
    if whole == 0 { 0.0 } else { clamp_percent(u64_to_f64(part) / u64_to_f64(whole) * 100.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_and_saturation() {
        assert_eq!(round_to_u64(0.4), 0);
        assert_eq!(round_to_u64(0.5), 1);
        assert_eq!(round_to_u64(41.6), 42);
        assert_eq!(round_to_u64(-3.0), 0);
        assert_eq!(round_to_u64(f64::NAN), 0);
        assert_eq!(round_to_u64(f64::INFINITY), u64::MAX);
        assert_eq!(round_to_u64(1e30), u64::MAX);
        assert_eq!(floor_to_u64(41.9), 41);
    }

    #[test]
    fn float_conversions_roundtrip() {
        assert_eq!(u64_to_f64(0), 0.0);
        assert_eq!(u64_to_f64(1_000_000), 1_000_000.0);
        assert_eq!(u64_to_f64(u64::from(u32::MAX) + 1), 4_294_967_296.0);
        assert_eq!(usize_to_f64(7), 7.0);
        assert_eq!(u64_to_usize(9), 9);
        assert_eq!(i64_to_f64(-10), -10.0);
        assert_eq!(i64_to_f64(42), 42.0);
        assert_eq!(i64_to_f64(i64::MIN), -9_223_372_036_854_775_808.0);
    }

    #[test]
    fn shown_amounts_are_finite_non_negative_and_bounded() {
        assert_eq!(shown_amount(42.5), 42.5);
        assert_eq!(shown_amount(MAX_SHOWN), MAX_SHOWN);
        for absurd in [1e300, f64::MAX, f64::INFINITY] {
            assert_eq!(shown_amount(absurd), MAX_SHOWN, "{absurd}");
        }
        for nothing in [0.0, -0.0, -1.0, f64::NEG_INFINITY, f64::NAN] {
            assert!(shown_amount(nothing) == 0.0 && shown_amount(nothing).is_sign_positive());
        }
    }

    #[test]
    fn percent_math() {
        assert_eq!(percent_of(1, 0), 0.0);
        assert_eq!(percent_of(50, 200), 25.0);
        assert_eq!(percent_of(300, 200), 100.0);
        assert_eq!(clamp_percent(f64::NAN), 0.0);
        assert_eq!(clamp_percent(-1.0), 0.0);
    }
}
