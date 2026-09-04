//! Icon sets. Every glyph a module uses is declared in its schema with one
//! value per set; users pick a set globally and may override any glyph.

use serde::Deserialize;

/// The four built-in icon sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSet {
    /// Nerd Font private-use glyphs (needs a patched font).
    #[default]
    Nerd,
    /// Plain Unicode symbols, no emoji.
    Unicode,
    /// Emoji.
    Emoji,
    /// 7-bit ASCII only.
    Ascii,
}

impl IconSet {
    /// All sets, in documentation order.
    pub const ALL: [Self; 4] = [Self::Nerd, Self::Unicode, Self::Emoji, Self::Ascii];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nerd => "nerd",
            Self::Unicode => "unicode",
            Self::Emoji => "emoji",
            Self::Ascii => "ascii",
        }
    }

    /// Parse a config name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|set| set.name() == s)
    }
}

/// One glyph with a value per icon set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    /// Nerd Font value.
    pub nerd: &'static str,
    /// Unicode value.
    pub unicode: &'static str,
    /// Emoji value.
    pub emoji: &'static str,
    /// ASCII value.
    pub ascii: &'static str,
}

impl Glyph {
    /// Same text in every set.
    #[must_use]
    pub const fn same(s: &'static str) -> Self {
        Self { nerd: s, unicode: s, emoji: s, ascii: s }
    }

    /// Value for a set.
    #[must_use]
    pub const fn get(self, set: IconSet) -> &'static str {
        match set {
            IconSet::Nerd => self.nerd,
            IconSet::Unicode => self.unicode,
            IconSet::Emoji => self.emoji,
            IconSet::Ascii => self.ascii,
        }
    }
}

/// Build a glyph from four literals.
#[must_use]
pub const fn glyph(
    nerd: &'static str,
    unicode: &'static str,
    emoji: &'static str,
    ascii: &'static str,
) -> Glyph {
    Glyph { nerd, unicode, emoji, ascii }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip() {
        for set in IconSet::ALL {
            assert_eq!(IconSet::parse(set.name()), Some(set));
        }
        assert_eq!(IconSet::parse("comic"), None);
    }

    #[test]
    fn glyph_lookup() {
        let g = glyph("N", "U", "E", "A");
        assert_eq!(g.get(IconSet::Nerd), "N");
        assert_eq!(g.get(IconSet::Ascii), "A");
        assert_eq!(Glyph::same("x").get(IconSet::Emoji), "x");
    }
}
