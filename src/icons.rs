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

    /// The mark a cut ends in: `…`, or `..` where the set is ASCII only.
    ///
    /// One rule for every cut garnish makes — the line, a module's
    /// `max_width`, a text box, a name's `max_length` — so an ASCII-only
    /// status line never grows a non-ASCII glyph.
    #[must_use]
    pub const fn ellipsis(self) -> &'static str {
        match self {
            Self::Ascii => "..",
            Self::Nerd | Self::Unicode | Self::Emoji => "…",
        }
    }

    /// The glyphs an overdue and a failed value are marked with (SPEC § 3.6).
    #[must_use]
    pub const fn stale_glyphs(self) -> (&'static str, &'static str) {
        match self {
            Self::Ascii => ("~", "x"),
            Self::Nerd | Self::Unicode | Self::Emoji => ("⟳", "✗"),
        }
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

/// Alternatives worth trying for an icon, by module id and icon key (SPEC § 14).
///
/// The `setup` glyph picker lists them after the four sets, and each module
/// page lists them as *also try*. Every glyph here passes the width guard
/// the built-in sets pass (one cell, no variation selector).
#[must_use]
pub fn suggestions(module: &str, key: &str) -> &'static [&'static str] {
    match (module, key) {
        ("model", "model") => &["", "", "", "❖", "✦", "✧"],
        ("model", "fast") => &["⚡", "↯", "*"],
        ("effort", "effort") => &["", "", "⚙", "✱"],
        ("context", "context") => &["", "", "⊞", "⊟", "⊡"],
        ("context" | "limit5h" | "limit7d" | "spend", "fill") => &["█", "━", "▓", "#", "="],
        ("context" | "limit5h" | "limit7d" | "spend", "empty") => &["░", "─", "▒", ".", "-"],
        ("branch", "branch") => &["", "", "", "⎇", "⌥", "⑂"],
        ("branch", "dirty") => &["✱", "*", "+", "~"],
        ("path", "folder") => &["", "", "", "❒", "❏"],
        ("clock", "spinner") => &["⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "|/-\\", "▁▃▅▇█▇▅▃", "⣾⣽⣻⢿⡿⣟⣯⣷", "⠁⠂⠄⡀⢀⠠⠐⠈"],
        ("session", "session") => &["", "", "⏱", "⌛"],
        ("api", "api") => &["", "", "⇄", "⇵"],
        ("cache", "cache") => &["", "", "⛁", "⛃"],
        ("cost", "cost") => &["", "", "$", "¢"],
        ("limit5h" | "limit7d" | "spend", "window") => &["", "", "⏳", "≣", "⌛"],
        ("pr", "pr") => &["", "", "⇄", "⇋"],
        ("session_name", "name") => &["", "", "❯", "›"],
        ("agent", "agent") => &["", "", "", "⚙"],
        ("lines", "lines") => &["", "", "Δ", "∆"],
        ("vim", "vim") => &["", "", "V"],
        ("style", "style") => &["", "", "✎", "✏"],
        ("worktree", "worktree") => &["", "", "", "⌂"],
        ("sync", "ahead") => &["⇡", "⇈", "^"],
        ("sync", "behind") => &["⇣", "⇊", "v"],
        _ => &[],
    }
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

    /// Every cut and every stale mark stays inside the set it was asked for:
    /// the ascii set is 7-bit, the others are not.
    #[test]
    fn ascii_set_marks_are_ascii_and_the_others_are_not() {
        for set in IconSet::ALL {
            let (overdue, failed) = set.stale_glyphs();
            let marks = [set.ellipsis(), overdue, failed];
            assert_eq!(
                set == IconSet::Ascii,
                marks.iter().all(|m| m.is_ascii()),
                "{}: {marks:?}",
                set.name()
            );
            assert!(marks.iter().all(|m| !m.is_empty()));
        }
    }

    #[test]
    fn glyph_lookup() {
        let g = glyph("N", "U", "E", "A");
        assert_eq!(g.get(IconSet::Nerd), "N");
        assert_eq!(g.get(IconSet::Ascii), "A");
        assert_eq!(Glyph::same("x").get(IconSet::Emoji), "x");
    }
}
