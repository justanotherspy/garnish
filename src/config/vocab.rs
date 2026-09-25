//! Fixed vocabularies: the config values that are one word of a short list
//! (a style, a side, a preset, an icon set).
//!
//! Each such enum keeps its words in one place, its own `ALL` array and
//! `name`, and [`Vocab`] is how everything else reads them: the parser and
//! its "expected one of" message, the generated reference, the `setup`
//! pickers and the command line. A word added to an enum is therefore in
//! all of them, and the tripwire below checks that they agree.

use super::format::{CostStyle, ParensStyle, PercentStyle, TokenStyle};
use super::presets::TopPreset;
use super::schema::Preset;
use super::{ColorChoice, FillDirection, Justify, Overflow, RightJustify, StaleStyle, VAlign};
use crate::frame::FrameStyle;
use crate::icons::IconSet;
use crate::theme::Role;
use crate::time::DurationStyle;

/// A config enum whose value is one word of a fixed list.
pub trait Vocab: Copy + PartialEq + 'static {
    /// Every variant, in the order messages, the reference and pickers list
    /// them.
    const ALL: &'static [Self];

    /// The word a config file writes for this variant.
    fn name(self) -> &'static str;

    /// The variant a word stands for.
    #[must_use]
    fn parse(word: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.name() == word)
    }

    /// Every word, in order.
    #[must_use]
    fn names() -> Vec<&'static str> {
        Self::ALL.iter().map(|v| v.name()).collect()
    }

    /// The words as a message names them: `a, b, c`.
    #[must_use]
    fn choices() -> String {
        Self::names().join(", ")
    }
}

/// [`Vocab`] from a type's own `ALL` array and `const fn name`: inside the
/// impl, `<$t>::ALL` and `<$t>::name` are the inherent items (an inherent
/// item wins over a trait's of the same name), which stay so that `const`
/// contexts and every existing caller keep them.
macro_rules! vocab {
    ($($t:ty),+ $(,)?) => {$(
        impl Vocab for $t {
            const ALL: &'static [Self] = &<$t>::ALL;

            fn name(self) -> &'static str {
                <$t>::name(self)
            }
        }
    )+};
}

vocab!(
    TopPreset,
    IconSet,
    ColorChoice,
    StaleStyle,
    RightJustify,
    Overflow,
    DurationStyle,
    TokenStyle,
    PercentStyle,
    CostStyle,
    ParensStyle,
    FrameStyle,
    FillDirection,
    Justify,
    VAlign,
    Preset,
    Role,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::SCHEMAS;

    /// Every variant has its own word, the word parses back to it, and
    /// nothing else parses.
    fn words_round_trip<T: Vocab + std::fmt::Debug>() {
        let names = T::names();
        assert_ne!(names.len(), 0);
        for (i, v) in T::ALL.iter().enumerate() {
            assert!(!v.name().is_empty(), "{v:?}");
            assert_eq!(T::parse(v.name()), Some(*v), "{v:?}");
            assert!(!names.get(..i).unwrap().contains(&v.name()), "{} twice", v.name());
        }
        assert_eq!(T::parse("inherit"), None);
        assert_eq!(T::parse(""), None);
        assert_eq!(T::choices(), names.join(", "));
    }

    /// The config key that takes `T` accepts each word, and a word that is
    /// not one is refused naming exactly `T`'s list, under the key's path.
    fn the_parser_takes_the_words<T: Vocab>(text: &dyn Fn(&str) -> String, path: &str) {
        for word in T::names() {
            let (_, errs) = crate::config::parse(&text(word), &SCHEMAS);
            assert_eq!(errs, Vec::new(), "{path} = {word:?}");
        }
        let (_, errs) = crate::config::parse(&text("zzz"), &SCHEMAS);
        let problem = errs.iter().find(|e| e.path == path).unwrap_or_else(|| panic!("{path}"));
        assert!(
            problem.message.ends_with(&format!("expected one of {}", T::choices())),
            "{path}: {}",
            problem.message
        );
    }

    /// cfg-14: one list per vocabulary, and the parser, its message, the
    /// per-module `inherit` options, the reference and the command line all
    /// read it.
    #[test]
    fn every_vocabulary_agrees_with_its_parser_and_its_copies() {
        words_round_trip::<TopPreset>();
        words_round_trip::<IconSet>();
        words_round_trip::<ColorChoice>();
        words_round_trip::<StaleStyle>();
        words_round_trip::<RightJustify>();
        words_round_trip::<Overflow>();
        words_round_trip::<DurationStyle>();
        words_round_trip::<TokenStyle>();
        words_round_trip::<PercentStyle>();
        words_round_trip::<CostStyle>();
        words_round_trip::<ParensStyle>();
        words_round_trip::<FrameStyle>();
        words_round_trip::<FillDirection>();
        words_round_trip::<Justify>();
        words_round_trip::<VAlign>();
        words_round_trip::<Preset>();
        words_round_trip::<Role>();

        let top = |key: &'static str| move |w: &str| format!("{key} = \"{w}\"\n");
        let table =
            |t: &'static str, key: &'static str| move |w: &str| format!("[{t}]\n{key} = \"{w}\"\n");
        the_parser_takes_the_words::<TopPreset>(&top("preset"), "preset");
        the_parser_takes_the_words::<IconSet>(&top("icons"), "icons");
        the_parser_takes_the_words::<ColorChoice>(&top("color"), "color");
        the_parser_takes_the_words::<StaleStyle>(&top("stale_style"), "stale_style");
        the_parser_takes_the_words::<RightJustify>(&top("right_justify"), "right_justify");
        the_parser_takes_the_words::<Overflow>(&top("overflow"), "overflow");
        the_parser_takes_the_words::<DurationStyle>(&top("durations"), "durations");
        the_parser_takes_the_words::<TokenStyle>(&table("format", "tokens"), "format.tokens");
        the_parser_takes_the_words::<PercentStyle>(&table("format", "percent"), "format.percent");
        the_parser_takes_the_words::<CostStyle>(&table("format", "cost"), "format.cost");
        the_parser_takes_the_words::<ParensStyle>(&table("format", "parens"), "format.parens");
        the_parser_takes_the_words::<FrameStyle>(&table("frame", "style"), "frame.style");
        the_parser_takes_the_words::<FillDirection>(
            &table("frame", "fill_direction"),
            "frame.fill_direction",
        );
        let col =
            |key: &'static str| move |w: &str| format!("[[row]]\n[[row.col]]\n{key} = \"{w}\"\n");
        the_parser_takes_the_words::<Justify>(&col("justify"), "row[0].col[0].justify");
        the_parser_takes_the_words::<VAlign>(&col("valign"), "row[0].col[0].valign");
        the_parser_takes_the_words::<Justify>(
            &|w: &str| {
                format!("[[row]]\ntitle = \"T\"\ntitle_justify = \"{w}\"\nmodules = [\"clock\"]\n")
            },
            "row[0].title_justify",
        );

        // A module's `preset` is hand-parsed, and still names the words.
        the_parser_takes_the_words::<Preset>(
            &|w: &str| format!("[modules.clock]\npreset = \"{w}\"\n"),
            "modules.clock.preset",
        );

        // The per-module `inherit` options are `inherit` and the same words.
        let inherit =
            |names: Vec<&'static str>| std::iter::once("inherit").chain(names).collect::<Vec<_>>();
        assert_eq!(crate::modules::DURATION_CHOICES, inherit(DurationStyle::names()));
        assert_eq!(crate::modules::TOKEN_CHOICES, inherit(TokenStyle::names()));
        assert_eq!(crate::modules::PERCENT_CHOICES, inherit(PercentStyle::names()));
        assert_eq!(crate::modules::COST_CHOICES, inherit(CostStyle::names()));

        // The reference names every word of the lists it documents.
        let page = crate::docs::config_page();
        for word in TopPreset::names()
            .into_iter()
            .chain(IconSet::names())
            .chain(ColorChoice::names())
            .chain(StaleStyle::names())
            .chain(FrameStyle::names())
            .chain(FillDirection::names())
        {
            assert!(page.contains(&format!("`{word}`")), "config.md lacks {word}");
        }
    }
}
