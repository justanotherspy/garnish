//! Icon sets. Every glyph a module uses is declared in its schema with one
//! value per set; users pick a set globally and may override any glyph.

/// The four built-in icon sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
/// page lists them as *also try*. Every glyph here passes the guard the
/// built-in sets pass: one or two cells, one for the bar glyphs that are
/// repeated cell by cell (`fill`, `empty`, `marker`), a spinner entry a
/// string of one-cell frames, and no variation selector.
#[must_use]
pub fn suggestions(module: &str, key: &str) -> &'static [&'static str] {
    SUGGESTIONS
        .iter()
        .find(|(modules, k, _)| *k == key && modules.contains(&module))
        .map_or(&[], |(_, _, glyphs)| glyphs)
}

/// The table [`suggestions`] reads: the modules an entry is for, the icon
/// key, and the glyphs. A table rather than a `match`, so a test can walk
/// every entry and find one naming a module or key that does not exist.
///
/// Nerd Font glyphs are written as escapes so an editor cannot drop them,
/// and only from the BMP private-use area (U+E000–U+F8FF: Font Awesome,
/// Octicons, Devicons, Seti), the one range every Nerd Font version draws.
const SUGGESTIONS: &[(&[&str], &str, &[&str])] = &[
    (&["model"], "model", &["\u{F0D0}", "\u{F135}", "\u{F477}", "❖", "✦", "✧"]),
    (&["model"], "fast", &["⚡", "↯", "*"]),
    (&["effort"], "effort", &["\u{F012}", "\u{F080}", "⚙", "✱"]),
    (&["context"], "context", &["\u{F1C0}", "\u{F00A}", "⊞", "⊟", "⊡"]),
    (&["context", "limit5h", "limit7d", "spend"], "fill", &["█", "━", "▓", "#", "="]),
    (&["context", "limit5h", "limit7d", "spend"], "empty", &["░", "─", "▒", ".", "-"]),
    (&["branch"], "branch", &["\u{F126}", "\u{F418}", "\u{E702}", "⎇", "⌥", "⑂"]),
    (&["branch"], "dirty", &["✱", "*", "+", "~"]),
    (&["path"], "folder", &["\u{F07C}", "\u{F413}", "\u{F015}", "❒", "❏"]),
    (&["clock"], "spinner", &["⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "|/-\\", "▁▃▅▇█▇▅▃", "⣾⣽⣻⢿⡿⣟⣯⣷", "⠁⠂⠄⡀⢀⠠⠐⠈"]),
    (&["session"], "session", &["\u{F252}", "\u{F1DA}", "⏱", "⌛"]),
    (&["api"], "api", &["\u{F1E6}", "\u{F0C2}", "⇄", "⇵"]),
    (&["cache"], "cache", &["\u{F0A0}", "\u{F187}", "⛁", "⛃"]),
    (&["cost"], "cost", &["\u{F0D6}", "\u{F09D}", "$", "¢"]),
    (&["limit5h", "limit7d", "spend"], "window", &["\u{F250}", "\u{F133}", "⏳", "≣", "⌛"]),
    (&["pr"], "pr", &["\u{F419}", "\u{F126}", "⇄", "⇋"]),
    (&["session_name"], "name", &["\u{F02E}", "\u{F292}", "❯", "›"]),
    (&["agent"], "agent", &["\u{F007}", "\u{F17B}", "\u{F477}", "⚙"]),
    (&["lines"], "lines", &["\u{F457}", "\u{F0CB}", "Δ", "∆"]),
    (&["vim"], "vim", &["\u{E7C5}", "\u{F120}", "V"]),
    (&["style"], "style", &["\u{F040}", "\u{F031}", "✎", "✏"]),
    (&["worktree"], "worktree", &["\u{F0E8}", "\u{F1BB}", "\u{F402}", "⌂"]),
    (&["sync"], "ahead", &["⇡", "⇈", "^"]),
    (&["sync"], "behind", &["⇣", "⇊", "v"]),
    (&["version"], "version", &["\u{F02C}", "\u{F02B}", "\u{F1B2}", "⊛", "#", "v"]),
    (&["sandbox"], "sandbox", &["\u{F023}", "\u{F132}", "\u{F1B2}", "⊡", "⊠", "[]"]),
    (&["voice"], "voice", &["\u{F130}", "\u{F028}", "\u{F2CE}", "∿", "~"]),
    (&["account"], "account", &["\u{F007}", "\u{F2BD}", "\u{F2C0}", "@", "&"]),
];

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

    /// sch-18: every entry of the table names a module and an icon key that
    /// exist, once, with glyphs to offer; the width guard over every
    /// module's keys is `modules`' own test, which can only reach an entry
    /// through a key that exists.
    #[test]
    fn every_suggestion_names_a_module_and_key_that_exist() {
        use crate::modules::SCHEMAS;
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for (modules, key, glyphs) in SUGGESTIONS {
            assert!(!glyphs.is_empty(), "{modules:?}.{key}");
            for module in *modules {
                let schema = SCHEMAS.iter().find(|s| s.id == *module);
                let schema = schema.unwrap_or_else(|| panic!("{module}: no such module"));
                assert!(schema.icons.iter().any(|i| i.key == *key), "{module}.{key}: no such icon");
                assert!(!seen.contains(&(module, key)), "{module}.{key} twice");
                seen.push((module, key));
                assert_eq!(suggestions(module, key), *glyphs);
            }
        }
    }

    /// spec-21: every module that leads with an icon (it has `show_icon`)
    /// offers alternatives for it, the four Phase 23 badges included.
    #[test]
    fn every_leading_icon_has_suggestions() {
        for schema in crate::modules::SCHEMAS.iter() {
            if schema.opt("show_icon").is_none() {
                continue;
            }
            let lead = schema.icons.first().unwrap();
            assert!(!suggestions(schema.id, lead.key).is_empty(), "{}.{}", schema.id, lead.key);
        }
    }

    #[test]
    fn glyph_lookup() {
        let g = glyph("N", "U", "E", "A");
        assert_eq!(g.get(IconSet::Nerd), "N");
        assert_eq!(g.get(IconSet::Ascii), "A");
    }
}
