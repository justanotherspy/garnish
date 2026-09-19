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

    /// A percentage in this style; `clamp` holds it to `0..=100`, and off
    /// it may pass 100 (`spend`), never falling below 0.
    #[must_use]
    pub fn format(self, p: f64, clamp: bool) -> String {
        match (self, clamp) {
            (Self::Whole, true) => util::percent(p),
            (Self::Whole, false) => util::percent_unclamped(p),
            (Self::Precise, true) => format!("{:.1}%", crate::num::clamp_percent(p)),
            (Self::Precise, false) => {
                format!("{:.1}%", if p.is_nan() || p < 0.0 { 0.0 } else { p })
            }
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

    /// An amount in this style; `decimals` is `cost.decimals`, which only
    /// `precise` reads. A thousand and up is `$1.2k` in both.
    #[must_use]
    pub fn format(self, usd: f64, decimals: usize) -> String {
        match self {
            Self::Precise => util::dollars(usd, decimals),
            Self::Whole if usd.is_nan() || usd < 0.0 => "$0".to_owned(),
            Self::Whole if usd >= 1000.0 => util::dollars(usd, 0),
            Self::Whole => format!("${usd:.0}"),
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
}
