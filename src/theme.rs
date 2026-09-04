//! Color themes.
//!
//! A theme maps semantic *roles* to colors; modules ask for roles (or literal
//! colors) so the whole status line can be re-skinned with one `theme = "…"`
//! line. Any role can be overridden under `[colors]`.

use std::collections::BTreeMap;

use crate::ansi::Color;

/// Semantic color roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// Primary highlight (icons, names).
    Accent,
    /// Secondary highlight.
    Accent2,
    /// De-emphasised text.
    Muted,
    /// Ordinary text.
    Text,
    /// Good / low usage.
    Ok,
    /// Caution / medium usage.
    Warn,
    /// High usage.
    Hot,
    /// Critical / errors.
    Danger,
    /// Frame lines.
    Frame,
    /// Bar band 1 (lowest).
    Band1,
    /// Bar band 2.
    Band2,
    /// Bar band 3.
    Band3,
    /// Bar band 4 (highest).
    Band4,
}

impl Role {
    /// Every role, in documentation order.
    pub const ALL: [Self; 13] = [
        Self::Accent,
        Self::Accent2,
        Self::Muted,
        Self::Text,
        Self::Ok,
        Self::Warn,
        Self::Hot,
        Self::Danger,
        Self::Frame,
        Self::Band1,
        Self::Band2,
        Self::Band3,
        Self::Band4,
    ];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Accent2 => "accent2",
            Self::Muted => "muted",
            Self::Text => "text",
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Hot => "hot",
            Self::Danger => "danger",
            Self::Frame => "frame",
            Self::Band1 => "band1",
            Self::Band2 => "band2",
            Self::Band3 => "band3",
            Self::Band4 => "band4",
        }
    }

    /// Parse a config name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == s)
    }
}

/// A named palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Config name.
    pub name: &'static str,
    /// One-line description for the docs.
    pub doc: &'static str,
    colors: [&'static str; 13],
}

impl Palette {
    const fn new(name: &'static str, doc: &'static str, colors: [&'static str; 13]) -> Self {
        Self { name, doc, colors }
    }

    /// Color spec for a role (as written in the palette).
    #[must_use]
    pub fn spec(&self, role: Role) -> &'static str {
        let idx = Role::ALL.iter().position(|r| *r == role).unwrap_or(0);
        self.colors.get(idx).copied().unwrap_or("default")
    }
}

/// The built-in palettes. Order: accent, accent2, muted, text, ok, warn, hot,
/// danger, frame, band1..band4.
pub const PALETTES: [Palette; 6] = [
    Palette::new(
        "garnish",
        "The house palette: fresh greens with warm accents.",
        [
            "#7dd3a0", "#89b4fa", "#6c7086", "#cdd6f4", "#a6e3a1", "#f9e2af", "#fab387", "#f38ba8",
            "#585b70", "#a6e3a1", "#f9e2af", "#fab387", "#f38ba8",
        ],
    ),
    Palette::new(
        "catppuccin-mocha",
        "Catppuccin Mocha.",
        [
            "#cba6f7", "#89b4fa", "#6c7086", "#cdd6f4", "#a6e3a1", "#f9e2af", "#fab387", "#f38ba8",
            "#45475a", "#94e2d5", "#a6e3a1", "#f9e2af", "#f38ba8",
        ],
    ),
    Palette::new(
        "nord",
        "Nord.",
        [
            "#88c0d0", "#81a1c1", "#4c566a", "#d8dee9", "#a3be8c", "#ebcb8b", "#d08770", "#bf616a",
            "#4c566a", "#a3be8c", "#ebcb8b", "#d08770", "#bf616a",
        ],
    ),
    Palette::new(
        "dracula",
        "Dracula.",
        [
            "#bd93f9", "#8be9fd", "#6272a4", "#f8f8f2", "#50fa7b", "#f1fa8c", "#ffb86c", "#ff5555",
            "#44475a", "#50fa7b", "#f1fa8c", "#ffb86c", "#ff5555",
        ],
    ),
    Palette::new(
        "tokyonight",
        "Tokyo Night.",
        [
            "#7aa2f7", "#bb9af7", "#565f89", "#c0caf5", "#9ece6a", "#e0af68", "#ff9e64", "#f7768e",
            "#3b4261", "#9ece6a", "#e0af68", "#ff9e64", "#f7768e",
        ],
    ),
    Palette::new(
        "mono",
        "No color at all; relies on dim and bold only.",
        [
            "default", "default", "gray", "default", "default", "default", "default", "default",
            "gray", "default", "default", "default", "default",
        ],
    ),
];

/// Look up a palette by name.
#[must_use]
pub fn palette(name: &str) -> Option<&'static Palette> {
    PALETTES.iter().find(|p| p.name == name)
}

/// A resolved theme: every role has a color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    colors: BTreeMap<Role, Color>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_palette(&PALETTES[0], &BTreeMap::new())
    }
}

impl Theme {
    /// Build from a palette plus role overrides (already validated color strings).
    #[must_use]
    pub fn from_palette(palette: &Palette, overrides: &BTreeMap<Role, Color>) -> Self {
        let colors = Role::ALL
            .into_iter()
            .map(|role| {
                let color = overrides
                    .get(&role)
                    .copied()
                    .or_else(|| Color::parse(palette.spec(role)))
                    .unwrap_or_default();
                (role, color)
            })
            .collect();
        Self { colors }
    }

    /// Color for a role.
    #[must_use]
    pub fn role(&self, role: Role) -> Color {
        self.colors.get(&role).copied().unwrap_or_default()
    }

    /// Resolve a color spec that is either a role name or a literal color.
    #[must_use]
    pub fn resolve(&self, spec: &str) -> Option<Color> {
        Role::parse(spec).map_or_else(|| Color::parse(spec), |r| Some(self.role(r)))
    }

    /// Band color for a percentage against ascending thresholds.
    ///
    /// `thresholds = [50, 75, 90]` gives band1 below 50, band2 below 75,
    /// band3 below 90, band4 at or above 90.
    #[must_use]
    pub fn band(&self, percent: f64, thresholds: &[f64], bands: &[Color]) -> Color {
        let idx = thresholds.iter().take_while(|&&t| percent >= t).count();
        let role_band = [Role::Band1, Role::Band2, Role::Band3, Role::Band4]
            .get(idx.min(3))
            .copied()
            .unwrap_or(Role::Band4);
        bands.get(idx).or_else(|| bands.last()).copied().unwrap_or_else(|| self.role(role_band))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_parses_every_role() {
        for p in &PALETTES {
            for role in Role::ALL {
                assert!(Color::parse(p.spec(role)).is_some(), "{} / {}", p.name, role.name());
            }
        }
        assert!(palette("nord").is_some());
        assert!(palette("nope").is_none());
    }

    #[test]
    fn overrides_win_and_roles_resolve() {
        let mut o = BTreeMap::new();
        o.insert(Role::Accent, Color::Ansi(1));
        let t = Theme::from_palette(&PALETTES[0], &o);
        assert_eq!(t.role(Role::Accent), Color::Ansi(1));
        assert_eq!(t.resolve("accent"), Some(Color::Ansi(1)));
        assert_eq!(t.resolve("#010203"), Some(Color::Rgb(1, 2, 3)));
        assert_eq!(t.resolve("bogus"), None);
    }

    #[test]
    fn bands_follow_thresholds() {
        let t = Theme::default();
        let bands = [Color::Ansi(2), Color::Ansi(3), Color::Ansi(5), Color::Ansi(1)];
        let th = [50.0, 75.0, 90.0];
        assert_eq!(t.band(0.0, &th, &bands), Color::Ansi(2));
        assert_eq!(t.band(49.9, &th, &bands), Color::Ansi(2));
        assert_eq!(t.band(50.0, &th, &bands), Color::Ansi(3));
        assert_eq!(t.band(89.9, &th, &bands), Color::Ansi(5));
        assert_eq!(t.band(90.0, &th, &bands), Color::Ansi(1));
        assert_eq!(t.band(120.0, &th, &bands), Color::Ansi(1));
        // fewer bands than thresholds: last band repeats
        assert_eq!(t.band(95.0, &th, &bands[..2]), Color::Ansi(3));
        // no bands at all: theme roles
        assert_eq!(t.band(95.0, &th, &[]), t.role(Role::Band4));
    }
}
