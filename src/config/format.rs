//! The `[format]` table (SPEC § 4, Number formats).
//!
//! One style per kind of number a module prints, each defaulting to the
//! rendering garnish has always had, and a per-module override of the same
//! name whose `inherit` follows the table, the way `durations` works.

use crate::modules::util;

/// How token counts print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenStyle {
    /// `12k`, `128k`, `1.0M`.
    #[default]
    Compact,
    /// `128,400`: every digit, thousands separated.
    Precise,
    /// `128400`.
    Whole,
}

/// How percentages print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PercentStyle {
    /// `42%`.
    #[default]
    Whole,
    /// `42.3%`, one decimal.
    Precise,
}

/// How money prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostStyle {
    /// `$1.23` (`cost.decimals` places; `$1.2k` from a thousand up).
    #[default]
    Precise,
    /// `$1`.
    Whole,
}

/// How a parenthesised detail is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParensStyle {
    /// In the colour of the value it follows, as one segment with it.
    #[default]
    Plain,
    /// In the muted role, the way a `label` is drawn.
    Dim,
}

/// The resolved `[format]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatCfg {
    /// `tokens`.
    pub tokens: TokenStyle,
    /// `percent`.
    pub percent: PercentStyle,
    /// `cost`.
    pub cost: CostStyle,
    /// `parens`.
    pub parens: ParensStyle,
}

impl TokenStyle {
    /// The config names, for messages.
    pub const CHOICES: &'static str = "compact, precise, whole";

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Precise => "precise",
            Self::Whole => "whole",
        }
    }

    /// The style a module option names, or `None` for `inherit` (and
    /// anything else, which the parser has already refused).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Compact, Self::Precise, Self::Whole].into_iter().find(|s| s.name() == name)
    }

    /// A token count in this style.
    #[must_use]
    pub fn format(self, n: u64) -> String {
        match self {
            Self::Compact => util::tokens(n),
            Self::Precise => thousands(n),
            Self::Whole => n.to_string(),
        }
    }
}

impl PercentStyle {
    /// The config names, for messages.
    pub const CHOICES: &'static str = "whole, precise";

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Precise => "precise",
        }
    }

    /// The style a module option names, or `None` for `inherit`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Whole, Self::Precise].into_iter().find(|s| s.name() == name)
    }

    /// The number this style prints for `p`, as a number: what a band
    /// threshold and a `below:N` / `above:N` rule compare, so they agree
    /// with the printed value at the boundaries whatever the style (SPEC
    /// § 3, § 4). `clamp` holds it to `0..=100`; off, it may pass 100
    /// (`spend`). NaN and anything at or below zero are 0 (a negative
    /// zero would print its sign).
    #[must_use]
    pub fn shown(self, p: f64, clamp: bool) -> f64 {
        let p = if p.is_nan() || p <= 0.0 {
            0.0
        } else if clamp {
            p.min(100.0)
        } else {
            p
        };
        match self {
            Self::Whole => crate::num::u64_to_f64(crate::num::round_to_u64(p)),
            // Rounded here rather than by the formatter, so the compared
            // number and the printed text are one rounding: the formatter
            // rounds a tie to even, `round` away from zero (12.25 → 12.3).
            Self::Precise => (p * 10.0).round() / 10.0,
        }
    }

    /// A percentage in this style: [`PercentStyle::shown`], printed.
    #[must_use]
    pub fn format(self, p: f64, clamp: bool) -> String {
        let shown = self.shown(p, clamp);
        match self {
            Self::Whole => format!("{}%", crate::num::round_to_u64(shown)),
            Self::Precise => format!("{shown:.1}%"),
        }
    }
}

impl CostStyle {
    /// The config names, for messages.
    pub const CHOICES: &'static str = "precise, whole";

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Precise => "precise",
            Self::Whole => "whole",
        }
    }

    /// The style a module option names, or `None` for `inherit`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Precise, Self::Whole].into_iter().find(|s| s.name() == name)
    }

    /// The amount this style prints for `usd`, as a number: what `zero` in
    /// a `hide` list reads (SPEC § 3), rounded to the places printed
    /// (`decimals` under `precise`, none under `whole`). NaN and anything
    /// at or below zero are 0 (a negative zero would print its sign).
    #[must_use]
    pub fn shown(self, usd: f64, decimals: usize) -> f64 {
        if usd.is_nan() || usd <= 0.0 {
            return 0.0;
        }
        let places = match self {
            Self::Precise => decimals.min(crate::config::MAX_DECIMALS),
            Self::Whole => 0,
        };
        let scale = 10_f64.powi(i32::try_from(places).unwrap_or(i32::MAX));
        (usd * scale).round() / scale
    }

    /// An amount in this style: [`CostStyle::shown`], printed. `decimals`
    /// is `cost.decimals`, which only `precise` reads; a thousand and up
    /// (after rounding) is `$1.2k` in both.
    #[must_use]
    pub fn format(self, usd: f64, decimals: usize) -> String {
        let shown = self.shown(usd, decimals);
        match self {
            Self::Precise => util::dollars(shown, decimals),
            Self::Whole => util::dollars(shown, 0),
        }
    }
}

impl ParensStyle {
    /// The config names, for messages.
    pub const CHOICES: &'static str = "plain, dim";

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Dim => "dim",
        }
    }
}

/// `128400` → `128,400`: a comma before every group of three digits but
/// the first.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len().saturating_add(digits.len() / 3));
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && digits.len().saturating_sub(i).checked_rem(3) == Some(0) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_print_each_kind_and_default_to_the_old_rendering() {
        assert_eq!(TokenStyle::Compact.format(128_400), "128k");
        assert_eq!(TokenStyle::Compact.format(1_000_000), "1.0M");
        assert_eq!(TokenStyle::Precise.format(128_400), "128,400");
        assert_eq!(TokenStyle::Precise.format(1_000_000), "1,000,000");
        assert_eq!(TokenStyle::Precise.format(999), "999");
        assert_eq!(TokenStyle::Precise.format(0), "0");
        assert_eq!(TokenStyle::Whole.format(128_400), "128400");
        assert_eq!(PercentStyle::Whole.format(42.34, true), "42%");
        assert_eq!(PercentStyle::Whole.format(112.4, false), "112%");
        assert_eq!(PercentStyle::Whole.format(112.4, true), "100%");
        assert_eq!(PercentStyle::Precise.format(42.34, true), "42.3%");
        assert_eq!(PercentStyle::Precise.format(112.44, false), "112.4%");
        assert_eq!(PercentStyle::Precise.format(112.44, true), "100.0%");
        assert_eq!(PercentStyle::Precise.format(-3.0, false), "0.0%");
        assert_eq!(PercentStyle::Precise.format(f64::NAN, true), "0.0%");
        assert_eq!(CostStyle::Precise.format(1.2345, 2), "$1.23");
        assert_eq!(CostStyle::Precise.format(1.2345, 0), "$1");
        assert_eq!(CostStyle::Whole.format(1.2345, 2), "$1");
        assert_eq!(CostStyle::Whole.format(0.6, 2), "$1");
        assert_eq!(CostStyle::Whole.format(1234.0, 2), "$1.2k");
        assert_eq!(CostStyle::Whole.format(-1.0, 2), "$0");
        assert_eq!(CostStyle::Whole.format(f64::NAN, 2), "$0");
        // The defaults are the old rendering, so a config without the
        // table renders byte for byte as before.
        assert_eq!(FormatCfg::default().tokens.format(12_345), util::tokens(12_345));
        assert_eq!(FormatCfg::default().percent.format(41.6, true), util::percent(41.6));
        assert_eq!(FormatCfg::default().cost.format(1.2345, 2), util::dollars(1.2345, 2));
        assert_eq!(FormatCfg::default().parens, ParensStyle::Plain);
    }

    #[test]
    fn names_round_trip_and_inherit_is_none() {
        for s in [TokenStyle::Compact, TokenStyle::Precise, TokenStyle::Whole] {
            assert_eq!(TokenStyle::parse(s.name()), Some(s));
        }
        for s in [PercentStyle::Whole, PercentStyle::Precise] {
            assert_eq!(PercentStyle::parse(s.name()), Some(s));
        }
        for s in [CostStyle::Precise, CostStyle::Whole] {
            assert_eq!(CostStyle::parse(s.name()), Some(s));
        }
        assert_eq!(TokenStyle::parse("inherit"), None);
        assert_eq!(PercentStyle::parse("compact"), None);
        assert_eq!(CostStyle::parse(""), None);
        assert_eq!(ParensStyle::Dim.name(), "dim");
        assert!(TokenStyle::CHOICES.contains("precise") && ParensStyle::CHOICES.contains("dim"));
    }

    /// SPEC § 3, § 4: what a band or a `hide` rule compares is the number
    /// printed, rounded as the style rounds it; a tie rounds the same way
    /// in both, a negative zero prints no sign, and the thousand mark
    /// follows the rounded amount.
    #[test]
    fn shown_is_the_printed_number() {
        assert_eq!(PercentStyle::Whole.shown(23.5, true), 24.0);
        assert_eq!(PercentStyle::Precise.shown(23.5, true), 23.5);
        assert_eq!(PercentStyle::Precise.shown(23.46, true), 23.5);
        assert_eq!(PercentStyle::Precise.shown(12.25, true), 12.3);
        assert_eq!(PercentStyle::Precise.format(12.25, true), "12.3%");
        assert_eq!(PercentStyle::Precise.shown(140.0, true), 100.0);
        assert_eq!(PercentStyle::Precise.shown(140.04, false), 140.0);
        assert_eq!(PercentStyle::Whole.shown(f64::NAN, false), 0.0);
        assert_eq!(PercentStyle::Precise.format(-0.0, true), "0.0%");
        assert_eq!(PercentStyle::Precise.format(-3.0, false), "0.0%");
        assert_eq!(PercentStyle::Whole.format(-0.0, true), "0%");
        assert_eq!(CostStyle::Precise.shown(0.004, 2), 0.0);
        assert_eq!(CostStyle::Precise.shown(0.004, 3), 0.004);
        assert_eq!(CostStyle::Precise.shown(0.006, 2), 0.01);
        assert_eq!(CostStyle::Whole.shown(0.4, 2), 0.0);
        assert_eq!(CostStyle::Whole.shown(0.6, 2), 1.0);
        assert_eq!(CostStyle::Whole.format(-0.0, 2), "$0");
        assert_eq!(CostStyle::Precise.format(-0.0, 2), "$0.00");
        assert_eq!(CostStyle::Whole.format(999.5, 2), "$1.0k");
        assert_eq!(CostStyle::Precise.format(999.999, 2), "$1.0k");
        assert_eq!(CostStyle::Precise.format(999.994, 2), "$999.99");
        assert_eq!(CostStyle::Whole.format(1000.0, 2), "$1.0k");
        assert_eq!(CostStyle::Whole.format(999.4, 2), "$999");
        assert_eq!(PercentStyle::Precise.format(f64::NAN, false), "0.0%");
    }
}
