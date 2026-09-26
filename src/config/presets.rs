//! Top-level presets: which rows exist and which module preset they imply.

use super::RowCfg;
use super::schema::Preset;

/// Top-level presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TopPreset {
    /// Four lines, module `default` presets.
    #[default]
    Default,
    /// One unframed line, module `minimal` presets.
    Minimal,
    /// Four lines, module `full` presets.
    Full,
    /// Two lines, module `default` presets.
    Compact,
}

impl TopPreset {
    /// All presets in documentation order.
    pub const ALL: [Self; 4] = [Self::Default, Self::Minimal, Self::Full, Self::Compact];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Minimal => "minimal",
            Self::Full => "full",
            Self::Compact => "compact",
        }
    }

    /// Parse a config name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }

    /// What the preset draws, in a few words (the `setup` preset picker).
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Default => "four lines, every module at its default",
            Self::Minimal => "one unframed line, the bare values",
            Self::Full => "four lines, everything each module knows",
            Self::Compact => "two lines",
        }
    }

    /// The module preset this top-level preset implies.
    #[must_use]
    pub const fn module_preset(self) -> Preset {
        match self {
            Self::Minimal => Preset::Minimal,
            Self::Full => Preset::Full,
            Self::Default | Self::Compact => Preset::Default,
        }
    }

    /// Whether the frame is drawn by default.
    #[must_use]
    pub const fn framed(self) -> bool {
        !matches!(self, Self::Minimal)
    }

    /// The rows this preset defines.
    #[must_use]
    pub fn rows(self) -> Vec<RowCfg> {
        let row = |left: &[&str], right: &[&str]| {
            RowCfg::plain(
                left.iter().map(|s| (*s).to_owned()).collect(),
                right.iter().map(|s| (*s).to_owned()).collect(),
            )
        };
        match self {
            Self::Default | Self::Full => vec![
                row(&["path", "branch", "sync", "worktree", "pr"], &["session_name", "agent"]),
                row(&["model", "effort", "context", "style"], &["vim"]),
                row(&["limit5h", "limit7d", "spend", "cost"], &["lines"]),
                row(&["session", "api", "cache"], &["clock"]),
            ],
            Self::Minimal => {
                vec![row(&["path", "branch", "context", "limit5h", "cost"], &["clock"])]
            }
            Self::Compact => vec![
                row(&["path", "branch", "sync", "pr"], &["clock"]),
                row(&["model", "effort", "context", "limit5h", "cost"], &["cache"]),
            ],
        }
    }
}
