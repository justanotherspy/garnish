//! Top-level presets: which lines exist and which module preset they imply.

use super::LineCfg;
use super::schema::Preset;

/// Top-level presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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

    /// The lines this preset defines.
    #[must_use]
    pub fn lines(self) -> Vec<LineCfg> {
        let line = |left: &[&str], right: &[&str]| LineCfg {
            left: left.iter().map(|s| (*s).to_owned()).collect(),
            right: right.iter().map(|s| (*s).to_owned()).collect(),
            separator: None,
            spacer: false,
        };
        match self {
            Self::Default | Self::Full => vec![
                line(&["path", "branch", "sync", "worktree", "pr"], &["session_name", "agent"]),
                line(&["model", "effort", "context", "style"], &["vim"]),
                line(&["limit5h", "limit7d", "spend", "cost"], &["lines"]),
                line(&["session", "api", "cache"], &["clock"]),
            ],
            Self::Minimal => {
                vec![line(&["path", "branch", "context", "limit5h", "cost"], &["clock"])]
            }
            Self::Compact => vec![
                line(&["path", "branch", "sync", "pr"], &["clock"]),
                line(&["model", "effort", "context", "limit5h", "cost"], &["cache"]),
            ],
        }
    }
}
