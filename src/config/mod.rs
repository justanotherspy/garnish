//! Configuration: loading the TOML file, validating it against the module
//! schemas, applying presets, and producing a fully resolved [`Config`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ansi::{Color, ColorMode};
use crate::frame::{FrameChars, FrameStyle};
use crate::icons::IconSet;
use crate::theme::{PALETTES, Role, Theme, palette};
use crate::time::DurationStyle;

pub mod format;
pub mod presets;
pub mod schema;

use format::{CostStyle, FormatCfg, ParensStyle, PercentStyle, TokenStyle};
use presets::TopPreset;
use schema::{
    COMMON_OPTS, HideRule, Kind, ModuleCfg, ModuleSchema, OptSpec, Overrides, Preset, Value,
    common_keys,
};

/// Environment variable naming the config file.
pub const CONFIG_ENV: &str = "GARNISH_CONFIG";

/// A validation problem with a TOML path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// Dotted TOML path (`modules.context.width`), or `""` for the whole file.
    pub path: String,
    /// Message.
    pub message: String,
    /// 1-based line in the file, when known.
    pub line: Option<usize>,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.line, self.path.is_empty()) {
            (Some(l), false) => write!(f, "line {l}: {}: {}", self.path, self.message),
            (Some(l), true) => write!(f, "line {l}: {}", self.message),
            (None, false) => write!(f, "{}: {}", self.path, self.message),
            (None, true) => write!(f, "{}", self.message),
        }
    }
}

/// One `[[row]]`: the addressable unit of the config, one or more terminal
/// lines tall (SPEC § 4.3). `[[line]]` is its permanent alias.
///
/// A resolved row always has at least one column: a row written with
/// `modules`/`right` and no `[[row.col]]` is normalised to one `1fr` column
/// carrying them, so every consumer sees the same shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowCfg {
    /// The columns, left to right; never empty after resolution.
    pub cols: Vec<ColCfg>,
    /// The file wrote `[[row.col]]` tables, so `config show` writes them
    /// back instead of the plain one-column form.
    pub explicit_cols: bool,
    /// Empty cells between columns (SPEC § 4.3); nothing on a one-column row.
    pub gap: usize,
    /// Separator override for this row.
    pub separator: Option<String>,
    /// Text set into the row's rule, or into the anonymous box of a
    /// `box = true` row (SPEC § 4.3).
    pub title: Option<TitleCfg>,
    /// The box this row joins: adjacent rows naming the same box form one.
    pub boxed: Option<BoxRef>,
    /// Every column is empty: an intentional blank row that
    /// `hide_empty_rows` never drops (SPEC § 4.1).
    pub spacer: bool,
    /// `blank = true` on a spacer: when the row would be whitespace only
    /// (no visible frame) it carries one invisible cell so Claude Code keeps
    /// it (SPEC § 4.1). Off by default, so the harness's own rule stands.
    pub blank: bool,
}

impl RowCfg {
    /// A one-column row holding `left` and `right`, the shape every
    /// `[[row]]` written without columns resolves to.
    #[must_use]
    pub fn plain(left: Vec<String>, right: Vec<String>) -> Self {
        Self {
            cols: vec![ColCfg { left, right, ..ColCfg::default() }],
            explicit_cols: false,
            gap: DEFAULT_GAP,
            separator: None,
            title: None,
            boxed: None,
            spacer: false,
            blank: false,
        }
    }

    /// The row's only column, when it has exactly one (the plain form).
    #[must_use]
    pub const fn single(&self) -> Option<&ColCfg> {
        match self.cols.as_slice() {
            [col] => Some(col),
            _ => None,
        }
    }

    /// Every module id placed anywhere in the row, columns and stacks alike.
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.cols.iter().flat_map(ColCfg::ids)
    }
}

/// One `[[row.col]]`: a column of a row (SPEC § 4.3).
///
/// A column holds either its own modules (`left`/`right`) or a stack of
/// inner rows, never both.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColCfg {
    /// The column's share of the row's width.
    pub width: Width,
    /// Left-aligned module ids.
    pub left: Vec<String>,
    /// Right-aligned module ids.
    pub right: Vec<String>,
    /// Where a lone `modules` group sits; defaulted by the column's position
    /// when the file does not say (SPEC § 4.3).
    pub justify: Justify,
    /// The file wrote `justify`, so `config show` writes it back rather than
    /// pinning what the position decided.
    pub justify_set: bool,
    /// Where a short stack sits in a taller row.
    pub valign: VAlign,
    /// The box drawn around the whole column, the outer row's full height.
    pub boxed: Option<BoxRef>,
    /// `[[row.col.row]]`: the column is a stack of rows instead of modules.
    pub rows: Vec<RowCfg>,
}

impl ColCfg {
    /// Every module id in the column, its stack included.
    pub fn ids(&self) -> Box<dyn Iterator<Item = &String> + '_> {
        if self.rows.is_empty() {
            Box::new(self.left.iter().chain(&self.right))
        } else {
            Box::new(self.rows.iter().flat_map(RowCfg::ids))
        }
    }

    /// Nothing is placed in this column (SPEC § 4.3: it still keeps its
    /// share of the width).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty() && self.rows.is_empty()
    }
}

/// A column's share of its row's width (SPEC § 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    /// `"<n>fr"`: a share of the width left over once the others are placed.
    Fr(u32),
    /// `"auto"`: exactly the column's own content.
    Auto,
    /// A fixed number of cells.
    Cells(usize),
}

impl Default for Width {
    fn default() -> Self {
        Self::Fr(1)
    }
}

impl Width {
    /// The value as `config show` writes it: a string for `fr` and `auto`,
    /// an integer for a cell count.
    #[must_use]
    pub fn to_toml(self) -> String {
        match self {
            Self::Fr(n) => format!("\"{n}fr\""),
            Self::Auto => "\"auto\"".to_owned(),
            Self::Cells(n) => n.to_string(),
        }
    }
}

/// Where a lone `modules` group sits in its column, and where a title sits
/// in its rule (SPEC § 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Justify {
    /// Against the column's left edge; a title right after the left cap.
    #[default]
    Left,
    /// Centred in the column, or in the widest empty gap of a title's line.
    Center,
    /// Against the column's right edge; a title right before the right cap.
    Right,
}

impl Justify {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// Where a stack shorter than its row sits (SPEC § 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VAlign {
    /// Padding lines below the content.
    #[default]
    Top,
    /// Padding lines split above and below.
    Center,
    /// Padding lines above the content.
    Bottom,
}

impl VAlign {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Center => "center",
            Self::Bottom => "bottom",
        }
    }
}

/// A title on a row's rule or on a box (SPEC § 4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleCfg {
    /// Plain text, reduced and capped like every config string.
    pub text: String,
    /// Where it sits in the rule.
    pub justify: Justify,
    /// Spaces on each side of the text.
    pub pad: usize,
    /// Role or literal for the text; the frame colour when unset.
    pub color: Option<Color>,
}

impl Default for TitleCfg {
    fn default() -> Self {
        Self { text: String::new(), justify: Justify::Left, pad: DEFAULT_TITLE_PAD, color: None }
    }
}

/// The box a row or column joins (SPEC § 4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxRef {
    /// `box = "<name>"`: the `[box.<name>]` table, shared with the adjacent
    /// rows that name it.
    Named(String),
    /// `box = true`: this row or column alone, with no `[box]` table.
    Anon,
}

impl BoxRef {
    /// The `[box.<name>]` this reference names, if any.
    #[must_use]
    pub const fn name(&self) -> Option<&str> {
        match self {
            Self::Named(n) => Some(n.as_str()),
            Self::Anon => None,
        }
    }
}

/// One `[box.<name>]` (SPEC § 4.3): the frame drawn around a run of rows or
/// around a column.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoxCfg {
    /// The box's title, set into its top rule.
    pub title: Option<TitleCfg>,
    /// Box style; the `[frame]` style when unset (`rounded` when that style
    /// has no box shape).
    pub style: Option<FrameStyle>,
    /// Draw the rule between a row's groups inside the box; off by default,
    /// so a box's interior is clean.
    pub fill: bool,
    /// Role or literal for the box's glyphs; the frame colour when unset.
    pub color: Option<Color>,
}

/// Stale-value styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StaleStyle {
    /// Dim the value and append a refresh glyph.
    #[default]
    Dim,
    /// Hide stale values entirely.
    Hide,
    /// Show stale values unchanged.
    Plain,
}

impl StaleStyle {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dim => "dim",
            Self::Hide => "hide",
            Self::Plain => "plain",
        }
    }
}

/// Where a padded right-group module's text sits (`right_justify`, SPEC § 4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RightJustify {
    /// Pad on the left: the text hugs the right cap.
    #[default]
    End,
    /// Pad on the right: the text follows the separator, the gap sits before the cap.
    Start,
}

impl RightJustify {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::End => "end",
            Self::Start => "start",
        }
    }
}

/// What happens to a left group wider than its budget (`overflow`, SPEC § 4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Overflow {
    /// Cut it with the ellipsis.
    #[default]
    Truncate,
    /// Scroll it: a window that advances `ticker_step` cells per tick and
    /// wraps around with `ticker_gap` between the end and the start.
    Ticker,
}

impl Overflow {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Truncate => "truncate",
            Self::Ticker => "ticker",
        }
    }
}

/// Color emission choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorChoice {
    /// Truecolor unless `NO_COLOR` is set.
    #[default]
    Auto,
    /// Always truecolor.
    Always,
    /// Never.
    Never,
    /// 256-color palette.
    #[serde(rename = "256")]
    Ansi256,
    /// 24-bit color.
    TrueColor,
}

impl ColorChoice {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
            Self::Ansi256 => "256",
            Self::TrueColor => "truecolor",
        }
    }

    /// Resolve to a concrete mode given the environment.
    #[must_use]
    pub const fn mode(self, no_color_env: bool) -> ColorMode {
        match self {
            Self::Auto => {
                if no_color_env {
                    ColorMode::Never
                } else {
                    ColorMode::TrueColor
                }
            }
            Self::Always | Self::TrueColor => ColorMode::TrueColor,
            Self::Never => ColorMode::Never,
            Self::Ansi256 => ColorMode::Ansi256,
        }
    }
}

/// Which way an animated rule pattern travels (`[frame] fill_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillDirection {
    /// Toward the left cap.
    Left,
    /// Toward the right cap.
    #[default]
    Right,
}

impl FillDirection {
    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// `[frame] separator_color` (SPEC § 4.1): one colour for every separator,
/// or the colour of the module before each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeparatorColor {
    /// A role or a literal, as written, and what it resolves to.
    Fixed {
        /// The value as the file wrote it (`muted` when unset), for `config show`.
        spec: String,
        /// The resolved colour.
        color: Color,
    },
    /// The first coloured, undimmed segment of the module before the
    /// separator (an icon or a value, never a label or an align pad).
    Inherit,
}

impl SeparatorColor {
    /// The value as written in a config.
    #[must_use]
    pub fn spec(&self) -> &str {
        match self {
            Self::Fixed { spec, .. } => spec,
            Self::Inherit => "inherit",
        }
    }
}

/// Frame configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameCfg {
    /// Style.
    pub style: FrameStyle,
    /// Characters in effect.
    pub chars: FrameChars,
    /// Fill the rule to the full width.
    pub fill: bool,
    /// The colour of every separator (SPEC § 4.1).
    pub separator_color: SeparatorColor,
    /// One-cell glyphs repeated across the rule instead of `fill_char`;
    /// empty means the static rule (SPEC § 4.2).
    pub fill_pattern: Vec<String>,
    /// Cells the pattern shifts per tick.
    pub fill_step: f64,
    /// Which way the pattern travels.
    pub fill_direction: FillDirection,
    /// Separator frames cycled one per tick (all the same width); empty
    /// means the static `separator`.
    pub separator_frames: Vec<String>,
    /// Frames the separator advances per tick.
    pub separator_step: f64,
}

/// Cells Claude Code's status line box loses to the harness's own footer
/// padding (2 on each side, `COLUMNS − 4`; SPEC § 2.1, verified in 2.1.261).
/// A row wider than the box is cut with `…` by the harness.
pub const HARNESS_PADDING: usize = 4;

/// Narrowest width garnish will render to, whatever `COLUMNS` says.
pub const MIN_WIDTH: usize = 10;

/// Widest width garnish will render to, whatever `COLUMNS` says.
///
/// No terminal has this many cells (an 8K display at a 4-pixel glyph is
/// under 2000), so anything above is a bad number, not a wide screen, and
/// must not size the buffers of a tick.
pub const MAX_WIDTH: usize = 4096;

/// Largest cell count a config may ask for in one module (`width`, `pad`, a bar).
///
/// More than a whole row cannot be shown and would only size an allocation.
/// The schema of every such option carries it as its `max`
/// ([`schema::OptSpec::max`]), so it is reported at config time and shown in
/// the reference; the renderers clamp again.
pub const MAX_CELLS: usize = 1024;

/// Longest string a config may put on a row (`text`, `gap`, `ticker_gap`),
/// in characters: a status line, not a document. The schema `max` of the
/// module options; `ticker_gap` is checked by hand.
pub const MAX_TEXT_CHARS: usize = 4096;

/// Columns on one row, and inner rows in one column (SPEC § 4.3). Each
/// bounds a loop over the tick's rendered segments, so both are checked at
/// config time and the extras dropped.
pub const MAX_COLS: usize = 16;

/// Cells between two columns, and the default when nothing says otherwise:
/// adjacent columns never touch without tuning (SPEC § 4.3).
pub const MAX_GAP: usize = 16;
/// `gap` when a row does not set it.
pub const DEFAULT_GAP: usize = 1;

/// Spaces on each side of a title, and the default (SPEC § 4.3).
pub const MAX_TITLE_PAD: usize = 64;
/// `title_pad` when a title does not set it.
pub const DEFAULT_TITLE_PAD: usize = 1;

/// Largest `n` in a `"<n>fr"` width: the shares are divided, never summed
/// into a cell count, but a bound keeps the arithmetic small (SPEC § 4.3).
pub const MAX_FR: u32 = 64;

/// Most decimal places a money amount prints with (`cost.decimals`): the
/// formatter allocates that many digits, so it is bounded like a width.
pub const MAX_DECIMALS: usize = 8;

/// The fully resolved configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Top-level preset.
    pub preset: TopPreset,
    /// Icon set.
    pub icons: IconSet,
    /// Theme name.
    pub theme_name: String,
    /// Resolved theme.
    pub theme: Theme,
    /// Color choice.
    pub color: ColorChoice,
    /// Truncate overflowing lines.
    pub truncate: bool,
    /// Stale styling.
    pub stale_style: StaleStyle,
    /// TTL periods a cached value may be overdue before it is styled stale
    /// (≥ 1; SPEC § 3.6).
    pub stale_after: u32,
    /// Extra cells subtracted from the width on top of [`HARNESS_PADDING`]
    /// (`2 × statusLine.padding` when that setting is non-zero).
    pub padding: usize,
    /// Pad module columns to the widest module in each across lines so the
    /// separators line up (SPEC § 4).
    pub align: bool,
    /// Which side of a padded right-group module the text sits on.
    pub right_justify: RightJustify,
    /// Drop a row whose modules all rendered nothing (spacers are kept).
    pub hide_empty_rows: bool,
    /// Truncate or scroll a left group wider than its budget.
    pub overflow: Overflow,
    /// Cells the ticker advances per tick (`> 0`; 0.5 = every second tick).
    pub ticker_step: f64,
    /// Text between the end of a scrolled group and its wrapped-around start.
    pub ticker_gap: String,
    /// Master switch for every animation, when the file sets it: `false`
    /// freezes them at frame 0 and cuts a ticker line with the ellipsis
    /// (SPEC § 4.2). `None` leaves the decision to Claude Code's
    /// `prefersReducedMotion` setting, then the default (`true`);
    /// `GARNISH_ANIMATE=0` freezes a session whatever the file says.
    pub animate: Option<bool>,
    /// How elapsed times and countdowns print.
    pub durations: DurationStyle,
    /// How numbers print (`[format]`, SPEC § 4).
    pub format: FormatCfg,
    /// Frame.
    pub frame: FrameCfg,
    /// Rows, in order (SPEC § 4.3).
    pub rows: Vec<RowCfg>,
    /// The `[box.<name>]` tables a row or column may join, keyed by name.
    pub boxes: BTreeMap<String, BoxCfg>,
    /// Resolved module configs, keyed by id, for every registered module.
    pub modules: BTreeMap<&'static str, ModuleCfg>,
    /// The user-defined text modules (`[modules.text.<name>]`), keyed by name
    /// and placed on a line as `text.<name>` (SPEC § 3.7).
    pub texts: BTreeMap<String, ModuleCfg>,
}

/// Command-line overrides applied on top of the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// Top-level preset.
    pub preset: Option<TopPreset>,
    /// Icon set.
    pub icons: Option<IconSet>,
    /// Theme name.
    pub theme: Option<String>,
    /// Color choice.
    pub color: Option<ColorChoice>,
}

/// The result of loading a config file.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The config in effect: the file with the built-in default standing in
    /// for each bad key, or the defaults when the file does not parse.
    pub config: Config,
    /// The file that was read, if any.
    pub path: Option<PathBuf>,
    /// Validation problems. The built-in default stands in for each bad key;
    /// only a file that does not parse as TOML is replaced wholesale (SPEC § 5).
    pub errors: Vec<ConfigError>,
}

/// The file as written, before presets and defaults are applied.
///
/// Every field is optional so a bad value can be reported and defaulted on
/// its own (SPEC § 5): the file is read as a plain TOML table and each known
/// key is converted separately, instead of through one `serde` model that
/// would reject the whole file on the first bad key.
#[derive(Debug, Default)]
struct RawConfig {
    preset: Option<TopPreset>,
    icons: Option<IconSet>,
    theme: Option<String>,
    color: Option<ColorChoice>,
    truncate: Option<bool>,
    stale_style: Option<StaleStyle>,
    stale_after: Option<u32>,
    padding: Option<u16>,
    align: Option<bool>,
    right_justify: Option<RightJustify>,
    hide_empty_rows: Option<bool>,
    overflow: Option<Overflow>,
    ticker_step: Option<f64>,
    ticker_gap: Option<String>,
    animate: Option<bool>,
    durations: Option<DurationStyle>,
    format: Option<RawFormat>,
    colors: BTreeMap<String, String>,
    frame: Option<RawFrame>,
    /// The rows as written, under whichever of the two array names the file
    /// used ([`RawConfig::rows_key`]).
    row: Vec<RawRow>,
    /// The file wrote the rows as `[[line]]`: every error under a row points
    /// at the name the user typed.
    rows_alias: bool,
    /// `[box.<name>]` tables, kept raw until the theme exists to resolve
    /// their colours (as `[modules.text.<name>]` are).
    boxes: BTreeMap<String, toml::Table>,
    modules: BTreeMap<String, toml::Table>,
}

/// Every key the top level of the file accepts, in the order the "expected
/// one of" message names them (`line` and `hide_empty_lines` are the
/// aliases of SPEC § 4.3).
pub const TOP_KEYS: [&str; 24] = [
    "preset",
    "icons",
    "theme",
    "color",
    "truncate",
    "stale_style",
    "stale_after",
    "padding",
    "align",
    "right_justify",
    "hide_empty_rows",
    "hide_empty_lines",
    "overflow",
    "ticker_step",
    "ticker_gap",
    "animate",
    "durations",
    "format",
    "colors",
    "frame",
    "row",
    "line",
    "box",
    "modules",
];

const COLOR_CHOICES: &str = "auto, always, never, 256, truecolor";
const STALE_STYLES: &str = "dim, hide, plain";
const DURATION_STYLES: &str = "compact, fixed";
const RIGHT_JUSTIFIES: &str = "end, start";
const OVERFLOWS: &str = "truncate, ticker";
/// Default `ticker_gap`: three blanks between the end of a scrolled group and its start.
pub const DEFAULT_TICKER_GAP: &str = "   ";

impl RawConfig {
    /// The array name the file used, so an error points at what was typed.
    const fn rows_key(&self) -> &'static str {
        if self.rows_alias { "line" } else { "row" }
    }

    // The table is taken by value so every field moves into place: cloning
    // each value cost a fifth of the parse on the full annotated file.
    fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut raw = Self::default();
        let presets = TopPreset::ALL.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ");
        let icon_sets = IconSet::ALL.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
        // The two array names are reconciled after the loop: TOML gives no
        // order between two arrays of tables, so a file carries one or the
        // other (SPEC § 4.3).
        let (mut rows, mut alias): (Option<Vec<RawRow>>, Option<Vec<RawRow>>) = (None, None);
        for (key, value) in table {
            match key.as_str() {
                "preset" => raw.preset = enum_field(&key, value, &presets, errors),
                "icons" => raw.icons = enum_field(&key, value, &icon_sets, errors),
                "theme" => raw.theme = field(&key, value, errors),
                "color" => raw.color = enum_field(&key, value, COLOR_CHOICES, errors),
                "truncate" => raw.truncate = field(&key, value, errors),
                "stale_style" => raw.stale_style = enum_field(&key, value, STALE_STYLES, errors),
                "stale_after" => raw.stale_after = field(&key, value, errors),
                "padding" => raw.padding = field(&key, value, errors),
                "align" => raw.align = field(&key, value, errors),
                "right_justify" => {
                    raw.right_justify = enum_field(&key, value, RIGHT_JUSTIFIES, errors);
                }
                // `hide_empty_lines` is the permanent alias of
                // `hide_empty_rows` (SPEC § 4.3); the new name wins when a
                // file carries both.
                "hide_empty_rows" => raw.hide_empty_rows = field(&key, value, errors),
                "hide_empty_lines" => {
                    let alias = field(&key, value, errors);
                    raw.hide_empty_rows = raw.hide_empty_rows.or(alias);
                }
                "overflow" => raw.overflow = enum_field(&key, value, OVERFLOWS, errors),
                "ticker_step" => raw.ticker_step = field(&key, value, errors),
                "ticker_gap" => raw.ticker_gap = field(&key, value, errors),
                "animate" => raw.animate = field(&key, value, errors),
                "durations" => raw.durations = enum_field(&key, value, DURATION_STYLES, errors),
                "format" => match value {
                    toml::Value::Table(t) => raw.format = Some(RawFormat::from_table(t, errors)),
                    _ => errors.push(problem("format", "expected a [format] table")),
                },
                "colors" => match value {
                    toml::Value::Table(t) => raw.colors = string_table("colors", t, errors),
                    _ => errors.push(problem("colors", "expected a table of role = color")),
                },
                "frame" => match value {
                    toml::Value::Table(t) => raw.frame = Some(RawFrame::from_table(t, errors)),
                    _ => errors.push(problem("frame", "expected a [frame] table")),
                },
                // `[[row]]` and its permanent alias `[[line]]` (SPEC § 4.3).
                "row" => rows = Some(row_array("row", value, errors)),
                "line" => alias = Some(row_array("line", value, errors)),
                // `[box.<name>]`, named by a bare key like a text module.
                "box" => match value {
                    toml::Value::Table(t) => {
                        for (name, b) in t {
                            let path = format!("box.{name}");
                            match b {
                                toml::Value::Table(bt) => {
                                    raw.boxes.insert(name, bt);
                                }
                                _ => errors.push(problem(&path, "expected a [box.<name>] table")),
                            }
                        }
                    }
                    _ => errors.push(problem("box", "expected [box.<name>] tables")),
                },
                "modules" => match value {
                    toml::Value::Table(t) => {
                        for (id, module) in t {
                            match module {
                                toml::Value::Table(m) => {
                                    raw.modules.insert(id, m);
                                }
                                _ => errors.push(problem(
                                    &format!("modules.{id}"),
                                    "expected a [modules.<id>] table",
                                )),
                            }
                        }
                    }
                    _ => errors.push(problem("modules", "expected [modules.<id>] tables")),
                },
                other => errors.push(problem(
                    other,
                    &format!("unknown key; expected one of {}", TOP_KEYS.join(", ")),
                )),
            }
        }
        match (rows, alias) {
            (Some(rows), None) => raw.row = rows,
            (None, Some(alias)) => {
                raw.row = alias;
                raw.rows_alias = true;
            }
            (Some(rows), Some(_)) => {
                errors.push(problem(
                    "line",
                    "a file uses either [[row]] or its alias [[line]], not both; \
                     the [[line]] entries are ignored",
                ));
                raw.row = rows;
            }
            (None, None) => {}
        }
        raw
    }
}

/// One `[[row]]` (or `[[line]]`) array, item by item.
///
/// A non-table keeps its place as a `bad_list` placeholder, so every later
/// row keeps the index it has in the file: dropping it would renumber the
/// survivors and send the user to a `row[n]` that is not theirs. The flag
/// stops it reading as a spacer, and it renders nothing.
fn row_array(name: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Vec<RawRow> {
    let toml::Value::Array(items) = value else {
        errors.push(problem(name, &format!("expected [[{name}]] tables")));
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let path = format!("{name}[{i}]");
            if let toml::Value::Table(t) = item {
                RawRow::from_table(&path, t, errors)
            } else {
                errors.push(problem(&path, &format!("expected a [[{name}]] table")));
                RawRow { bad_list: true, ..RawRow::default() }
            }
        })
        .collect()
}

#[derive(Debug, Default)]
struct RawFrame {
    style: Option<FrameStyle>,
    fill: Option<bool>,
    first: Option<String>,
    middle: Option<String>,
    last: Option<String>,
    single: Option<String>,
    fill_char: Option<String>,
    right_first: Option<String>,
    right_middle: Option<String>,
    right_last: Option<String>,
    right_single: Option<String>,
    pad: Option<String>,
    separator: Option<String>,
    separator_color: Option<String>,
    fill_pattern: Option<String>,
    fill_step: Option<f64>,
    fill_direction: Option<FillDirection>,
    separator_frames: Option<Vec<String>>,
    separator_step: Option<f64>,
    top_left: Option<String>,
    top_right: Option<String>,
    bottom_left: Option<String>,
    bottom_right: Option<String>,
    side: Option<String>,
}

const FRAME_KEYS: [&str; 24] = [
    "style",
    "fill",
    "first",
    "middle",
    "last",
    "single",
    "fill_char",
    "right_first",
    "right_middle",
    "right_last",
    "right_single",
    "pad",
    "separator",
    "separator_color",
    "fill_pattern",
    "fill_step",
    "fill_direction",
    "separator_frames",
    "separator_step",
    "top_left",
    "top_right",
    "bottom_left",
    "bottom_right",
    "side",
];
const FILL_DIRECTIONS: &str = "left, right";

impl RawFrame {
    fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut f = Self::default();
        let styles = FrameStyle::ALL.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
        for (key, value) in table {
            let path = format!("frame.{key}");
            let text_slot = match key.as_str() {
                "first" => Some(&mut f.first),
                "middle" => Some(&mut f.middle),
                "last" => Some(&mut f.last),
                "single" => Some(&mut f.single),
                "fill_char" => Some(&mut f.fill_char),
                "right_first" => Some(&mut f.right_first),
                "right_middle" => Some(&mut f.right_middle),
                "right_last" => Some(&mut f.right_last),
                "right_single" => Some(&mut f.right_single),
                "pad" => Some(&mut f.pad),
                "separator" => Some(&mut f.separator),
                "fill_pattern" => Some(&mut f.fill_pattern),
                "top_left" => Some(&mut f.top_left),
                "top_right" => Some(&mut f.top_right),
                "bottom_left" => Some(&mut f.bottom_left),
                "bottom_right" => Some(&mut f.bottom_right),
                "side" => Some(&mut f.side),
                _ => None,
            };
            if let Some(slot) = text_slot {
                // Frame glyphs enter the width arithmetic directly, so they
                // are reduced to plain text here, before any cell is counted.
                *slot = field::<String>(&path, value, errors).map(|s| crate::ansi::plain_text(&s));
                continue;
            }
            match key.as_str() {
                "style" => f.style = enum_field(&path, value, &styles, errors),
                "fill" => f.fill = field(&path, value, errors),
                // A colour spec, resolved against the theme in `resolve_frame`.
                "separator_color" => f.separator_color = field(&path, value, errors),
                "fill_step" => f.fill_step = field(&path, value, errors),
                "fill_direction" => {
                    f.fill_direction = enum_field(&path, value, FILL_DIRECTIONS, errors);
                }
                "separator_frames" => f.separator_frames = field(&path, value, errors),
                "separator_step" => f.separator_step = field(&path, value, errors),
                _ => errors.push(problem(
                    &path,
                    &format!("unknown key; expected one of {}", FRAME_KEYS.join(", ")),
                )),
            }
        }
        f
    }
}

/// The `[format]` table as written (SPEC § 4, Number formats), each key
/// reported and defaulted on its own like the rest of the file.
#[derive(Debug, Default)]
struct RawFormat {
    tokens: Option<TokenStyle>,
    percent: Option<PercentStyle>,
    cost: Option<CostStyle>,
    parens: Option<ParensStyle>,
}

const FORMAT_KEYS: [&str; 4] = ["tokens", "percent", "cost", "parens"];

impl RawFormat {
    fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut f = Self::default();
        for (key, value) in table {
            let path = format!("format.{key}");
            match key.as_str() {
                "tokens" => f.tokens = enum_field(&path, value, TokenStyle::CHOICES, errors),
                "percent" => f.percent = enum_field(&path, value, PercentStyle::CHOICES, errors),
                "cost" => f.cost = enum_field(&path, value, CostStyle::CHOICES, errors),
                "parens" => f.parens = enum_field(&path, value, ParensStyle::CHOICES, errors),
                _ => errors.push(problem(
                    &path,
                    &format!("unknown key; expected one of {}", FORMAT_KEYS.join(", ")),
                )),
            }
        }
        f
    }

    fn resolve(&self) -> FormatCfg {
        FormatCfg {
            tokens: self.tokens.unwrap_or_default(),
            percent: self.percent.unwrap_or_default(),
            cost: self.cost.unwrap_or_default(),
            parens: self.parens.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
struct RawRow {
    modules: Vec<String>,
    right: Vec<String>,
    separator: Option<String>,
    /// `modules` or `right` was not a list at all (reported), so the empty
    /// list that stands in must not read as an intentional spacer.
    bad_list: bool,
    blank: bool,
    gap: Option<usize>,
    title: Option<String>,
    title_justify: Option<Justify>,
    title_pad: Option<usize>,
    title_color: Option<String>,
    boxed: Option<BoxRef>,
    /// `[[row.col]]` tables, in order; `None` when the row wrote none, which
    /// is what tells `config show` to write the plain form back.
    cols: Option<Vec<RawCol>>,
}

const ROW_KEYS: &str =
    "modules, right, separator, blank, gap, title, title_justify, title_pad, title_color, box, col";
/// An inner row (`[[row.col.row]]`) is one line of a stack: it takes no
/// columns of its own and no `gap`, so the tree is two levels deep and never
/// deeper (SPEC § 4.3).
const INNER_ROW_KEYS: &str =
    "modules, right, separator, blank, title, title_justify, title_pad, title_color, box";
const COL_KEYS: &str = "width, modules, right, justify, valign, box, row";
const JUSTIFIES: &str = "left, center, right";
const VALIGNS: &str = "top, center, bottom";

impl RawRow {
    /// One `[[row]]` or `[[row.col.row]]` table. An inner row may not carry
    /// `col` or `gap`: both are reported and ignored, never recursed into.
    fn from_table_at(
        path: &str,
        table: toml::Table,
        inner: bool,
        errors: &mut Vec<ConfigError>,
    ) -> Self {
        let mut row = Self::default();
        for (key, value) in table {
            let path = format!("{path}.{key}");
            match key.as_str() {
                "modules" | "right" => {
                    // Not a list, or a list with non-string items: either way
                    // the empty result is a reported mistake, not a spacer.
                    let given = value.as_array().map_or(usize::MAX, Vec::len);
                    let ids = id_list(&path, value, errors);
                    row.bad_list |= ids.len() != given;
                    if key == "modules" {
                        row.modules = ids;
                    } else {
                        row.right = ids;
                    }
                }
                "separator" => {
                    row.separator =
                        field::<String>(&path, value, errors).map(|s| crate::ansi::plain_text(&s));
                }
                "blank" => row.blank = field::<bool>(&path, value, errors).unwrap_or(false),
                "title" => row.title = text_field(&path, value, errors),
                "title_justify" => {
                    row.title_justify = enum_field(&path, value, JUSTIFIES, errors);
                }
                "title_pad" => row.title_pad = bounded_count(&path, value, MAX_TITLE_PAD, errors),
                "title_color" => row.title_color = field(&path, value, errors),
                "box" => row.boxed = box_ref(&path, value, errors),
                "gap" if !inner => row.gap = bounded_count(&path, value, MAX_GAP, errors),
                "col" if !inner => row.cols = Some(col_array(&path, value, errors)),
                _ => {
                    let keys = if inner { INNER_ROW_KEYS } else { ROW_KEYS };
                    let message = format!("unknown key; expected one of {keys}");
                    errors.push(problem(&path, &message));
                }
            }
        }
        row
    }

    fn from_table(path: &str, table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        Self::from_table_at(path, table, false, errors)
    }
}

#[derive(Debug, Default)]
struct RawCol {
    width: Option<Width>,
    modules: Vec<String>,
    right: Vec<String>,
    bad_list: bool,
    justify: Option<Justify>,
    valign: Option<VAlign>,
    boxed: Option<BoxRef>,
    rows: Vec<RawRow>,
}

impl RawCol {
    fn from_table(path: &str, table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut col = Self::default();
        for (key, value) in table {
            let path = format!("{path}.{key}");
            match key.as_str() {
                "width" => col.width = width_field(&path, value, errors),
                "modules" | "right" => {
                    let given = value.as_array().map_or(usize::MAX, Vec::len);
                    let ids = id_list(&path, value, errors);
                    col.bad_list |= ids.len() != given;
                    if key == "modules" {
                        col.modules = ids;
                    } else {
                        col.right = ids;
                    }
                }
                "justify" => col.justify = enum_field(&path, value, JUSTIFIES, errors),
                "valign" => col.valign = enum_field(&path, value, VALIGNS, errors),
                "box" => col.boxed = box_ref(&path, value, errors),
                "row" => {
                    let toml::Value::Array(items) = value else {
                        errors.push(problem(&path, "expected [[row.col.row]] tables"));
                        continue;
                    };
                    for (k, item) in items.into_iter().enumerate() {
                        let path = format!("{path}[{k}]");
                        if col.rows.len() >= MAX_COLS {
                            errors.push(problem(
                                &path,
                                &format!("at most {MAX_COLS} rows in one column; ignored"),
                            ));
                            break;
                        }
                        let inner = if let toml::Value::Table(t) = item {
                            RawRow::from_table_at(&path, t, true, errors)
                        } else {
                            errors.push(problem(&path, "expected a [[row.col.row]] table"));
                            RawRow { bad_list: true, ..RawRow::default() }
                        };
                        col.rows.push(inner);
                    }
                }
                _ => {
                    let message = format!("unknown key; expected one of {COL_KEYS}");
                    errors.push(problem(&path, &message));
                }
            }
        }
        col
    }
}

/// The `[[row.col]]` array of one row, bounded at [`MAX_COLS`].
fn col_array(path: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Vec<RawCol> {
    let toml::Value::Array(items) = value else {
        errors.push(problem(path, "expected [[row.col]] tables"));
        return Vec::new();
    };
    let mut cols = Vec::new();
    for (j, item) in items.into_iter().enumerate() {
        let path = format!("{path}[{j}]");
        if cols.len() >= MAX_COLS {
            errors.push(problem(&path, &format!("at most {MAX_COLS} columns on a row; ignored")));
            break;
        }
        let col = if let toml::Value::Table(t) = item {
            RawCol::from_table(&path, t, errors)
        } else {
            errors.push(problem(&path, "expected a [[row.col]] table"));
            RawCol { bad_list: true, ..RawCol::default() }
        };
        cols.push(col);
    }
    cols
}

/// `box = "<name>"` or `box = true`; `false` is "no box", as leaving the key
/// out is (SPEC § 4.3).
fn box_ref(path: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Option<BoxRef> {
    match value {
        toml::Value::String(name) if is_bare_key(&name) => Some(BoxRef::Named(name)),
        toml::Value::String(name) => {
            errors.push(problem(
                path,
                &format!(
                    "box name {name:?} must be letters, digits, _ or - so [box.{name}] reads the same"
                ),
            ));
            None
        }
        toml::Value::Boolean(true) => Some(BoxRef::Anon),
        toml::Value::Boolean(false) => None,
        _ => {
            errors.push(problem(path, "expected a [box.<name>] name or true"));
            None
        }
    }
}

/// `width = "<n>fr" | "auto" | <cells>` (SPEC § 4.3). The three forms are
/// named in the message, because a quoted number is the easy mistake.
fn width_field(path: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Option<Width> {
    let bad = |errors: &mut Vec<ConfigError>| {
        errors.push(problem(
            path,
            &format!(
                "expected \"<n>fr\" (1–{MAX_FR}), \"auto\", or a cell count 0–{MAX_CELLS} as an integer"
            ),
        ));
        None
    };
    match value {
        toml::Value::Integer(n) => match usize::try_from(n) {
            Ok(cells) if cells <= MAX_CELLS => Some(Width::Cells(cells)),
            _ => bad(errors),
        },
        toml::Value::String(s) if s == "auto" => Some(Width::Auto),
        toml::Value::String(s) => match s.strip_suffix("fr").map(str::parse::<u32>) {
            Some(Ok(n)) if (1..=MAX_FR).contains(&n) => Some(Width::Fr(n)),
            _ => bad(errors),
        },
        _ => bad(errors),
    }
}

/// A non-negative count with a ceiling: above it the key is reported and
/// left unset, so its default applies (the pattern of [`schema::OptSpec`]'s
/// `max`, for the layout keys the schemas do not own).
fn bounded_count(
    path: &str,
    value: toml::Value,
    max: usize,
    errors: &mut Vec<ConfigError>,
) -> Option<usize> {
    let n = field::<usize>(path, value, errors)?;
    if n > max {
        errors.push(problem(path, &format!("must be at most {max}")));
        return None;
    }
    Some(n)
}

/// A config string that reaches a row: reduced to plain text and capped, as
/// every other row string is (SPEC § 5).
fn text_field(path: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Option<String> {
    let text = field::<String>(path, value, errors)?;
    if text.chars().count() > MAX_TEXT_CHARS {
        errors.push(problem(path, &format!("must be at most {MAX_TEXT_CHARS} characters")));
        return None;
    }
    Some(crate::ansi::plain_text(&text))
}

/// Convert one TOML value to its typed field, reporting a bad one under
/// `path` and leaving the field unset so its default applies.
fn field<T: serde::de::DeserializeOwned>(
    path: &str,
    value: toml::Value,
    errors: &mut Vec<ConfigError>,
) -> Option<T> {
    match value.try_into::<T>() {
        Ok(v) => Some(v),
        Err(e) => {
            // serde names Rust types; say what a person can type instead.
            let message = e
                .message()
                .replace("expected u16", "expected an integer 0–65535")
                .replace("expected u32", "expected a non-negative integer")
                .replace("expected u64", "expected a non-negative integer")
                .replace("expected f64", "expected a number");
            errors.push(problem(path, &message));
            None
        }
    }
}

/// [`field`] for a key with a fixed vocabulary, naming the choices in the
/// message: `try_into` alone says "invalid type: unit variant" for a
/// non-string and reads a table's keys as if they were the value.
fn enum_field<T: serde::de::DeserializeOwned>(
    path: &str,
    value: toml::Value,
    options: &str,
    errors: &mut Vec<ConfigError>,
) -> Option<T> {
    let Some(text) = value.as_str().map(str::to_owned) else {
        errors.push(problem(path, &format!("expected a string, one of {options}")));
        return None;
    };
    value.try_into::<T>().ok().or_else(|| {
        errors.push(problem(path, &format!("unknown value {text:?}; expected one of {options}")));
        None
    })
}

/// A `[[line]]` module list kept item by item: a non-string item is reported
/// under its index and skipped, the rest of the line stays (one typo must not
/// blank a whole row, SPEC § 5).
fn id_list(path: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Vec<String> {
    let toml::Value::Array(items) = value else {
        errors.push(problem(path, "expected a list of module ids"));
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(j, item)| {
            if let toml::Value::String(s) = item {
                Some(s)
            } else {
                errors.push(problem(&format!("{path}[{j}]"), "expected a module id string"));
                None
            }
        })
        .collect()
}

/// A table of string values, skipping (and reporting) entries of another type.
fn string_table(
    path: &str,
    table: toml::Table,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (k, v) in table {
        match v {
            toml::Value::String(s) => {
                out.insert(k, s);
            }
            _ => errors.push(problem(&format!("{path}.{k}"), "expected a string")),
        }
    }
    out
}

fn problem(path: &str, message: &str) -> ConfigError {
    ConfigError { path: path.to_owned(), message: message.to_owned(), line: None }
}

/// An environment variable holding a path, or `None` when it is unset *or
/// empty*.
///
/// An empty value is the shell's idiom for "unset" (`FOO= cmd`), and the two
/// mean the same thing here: `GARNISH_CONFIG=` once named the empty path,
/// which put `⚠ config: cannot read` on every tick, and `XDG_CONFIG_HOME=`
/// once made the candidate the *relative* `garnish/garnish.toml`, so a
/// checkout holding that file became the user's config for every session
/// started in it. [`crate::claude_settings::home_dir`] is the same rule for
/// `HOME`.
pub(crate) fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The XDG base for garnish's own files: `XDG_CONFIG_HOME`, else `~/.config`.
fn config_home() -> Option<PathBuf> {
    env_path("XDG_CONFIG_HOME")
        .or_else(|| crate::claude_settings::home_dir().map(|h| h.join(".config")))
}

/// Locate the config file: explicit path > `GARNISH_CONFIG` > XDG > `~/.garnish.toml`.
#[must_use]
pub fn locate(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Some(p) = env_path(CONFIG_ENV) {
        return Some(p);
    }
    let xdg = config_home().map(|d| d.join("garnish").join("garnish.toml"));
    if let Some(p) = xdg.filter(|p| p.is_file()) {
        return Some(p);
    }
    crate::claude_settings::home_dir().map(|h| h.join(".garnish.toml")).filter(|p| p.is_file())
}

/// The default location a new config should be written to.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    // Without a home there is no default: guessing `.` would write into
    // whatever directory garnish happens to run from (a repository, say).
    Some(config_home()?.join("garnish").join("garnish.toml"))
}

/// Load and resolve the configuration. Never fails: a bad key is reported
/// and defaulted on its own; only an unreadable or non-TOML file yields the
/// built-in defaults wholesale, plus the error.
#[must_use]
pub fn load(explicit: Option<&Path>, schemas: &[ModuleSchema]) -> Loaded {
    load_with(explicit, schemas, &Overlay::default())
}

/// [`load`] with command-line overrides.
#[must_use]
pub fn load_with(explicit: Option<&Path>, schemas: &[ModuleSchema], overlay: &Overlay) -> Loaded {
    let path = locate(explicit);
    let Some(p) = path.clone() else {
        let (config, errors) = parse_with("", schemas, overlay);
        return Loaded { config, path: None, errors };
    };
    match std::fs::read_to_string(&p) {
        Ok(text) => {
            let (config, errors) = parse_with(&text, schemas, overlay);
            Loaded { config, path, errors }
        }
        Err(e) => {
            // The defaults, but still under the command-line overlay: a
            // `--color never` render of an unreadable config must stay plain.
            let (config, mut errors) = parse_with("", schemas, overlay);
            errors.push(ConfigError {
                path: String::new(),
                message: format!("cannot read: {e}"),
                line: None,
            });
            Loaded { config, path, errors }
        }
    }
}

/// Parse and resolve TOML text.
///
/// Every valid key takes effect; each invalid one is reported and its
/// built-in default used instead. Only text that is not TOML yields the
/// defaults wholesale, with the line of the syntax error (SPEC § 5).
#[must_use]
pub fn parse(text: &str, schemas: &[ModuleSchema]) -> (Config, Vec<ConfigError>) {
    parse_with(text, schemas, &Overlay::default())
}

/// [`parse`] with command-line overrides.
#[must_use]
pub fn parse_with(
    text: &str,
    schemas: &[ModuleSchema],
    overlay: &Overlay,
) -> (Config, Vec<ConfigError>) {
    let mut errors = Vec::new();
    let table = match toml::from_str::<toml::Table>(text) {
        Ok(table) => table,
        Err(e) => {
            // The whole file falls back to the defaults, under the same
            // command-line overrides as a good file would be: `preview
            // --color never` of a broken config must still be plain.
            let line = e.span().map(|s| line_of(text, s.start));
            errors.push(ConfigError { path: String::new(), message: e.message().to_owned(), line });
            toml::Table::new()
        }
    };
    let (config, more) = resolve_table(table, schemas, overlay);
    errors.extend(more);
    (config, errors)
}

/// [`parse`] of a file already read as a TOML table: what `setup` renders
/// its draft through on every edit (SPEC § 14), so an edit never round-trips
/// through text.
#[must_use]
pub fn parse_table(table: toml::Table, schemas: &[ModuleSchema]) -> (Config, Vec<ConfigError>) {
    resolve_table(table, schemas, &Overlay::default())
}

fn resolve_table(
    table: toml::Table,
    schemas: &[ModuleSchema],
    overlay: &Overlay,
) -> (Config, Vec<ConfigError>) {
    let mut errors = Vec::new();
    let mut raw = RawConfig::from_table(table, &mut errors);
    if overlay.preset.is_some() {
        // The preset's rows replace the file's, so their problems are moot.
        let key = format!("{}[", raw.rows_key());
        raw.preset = overlay.preset;
        raw.row.clear();
        errors.retain(|e| !e.path.starts_with(&key));
    }
    raw.icons = overlay.icons.or(raw.icons);
    raw.theme = overlay.theme.clone().or(raw.theme);
    raw.color = overlay.color.or(raw.color);
    let config = resolve(&raw, schemas, &mut errors);
    (config, errors)
}

fn line_of(text: &str, byte: usize) -> usize {
    text.bytes().take(byte).filter(|&b| b == b'\n').count().saturating_add(1)
}

/// The TOML syntax error of `text` as `line N: message`, when it has one.
///
/// A syntax error is the one problem that makes a file unreadable rather
/// than fixable per key (SPEC § 5), and so the one a writing command must
/// refuse to paper over.
#[must_use]
pub fn syntax_error(text: &str) -> Option<String> {
    toml::from_str::<toml::Table>(text).err().map(|e| {
        e.span().map_or_else(
            || e.message().to_owned(),
            |s| format!("line {}: {}", line_of(text, s.start), e.message()),
        )
    })
}

impl Config {
    /// Built-in defaults.
    #[must_use]
    pub fn defaults(schemas: &[ModuleSchema]) -> Self {
        let mut errors = Vec::new();
        resolve(&RawConfig::default(), schemas, &mut errors)
    }

    /// Effective width for rendering: `COLUMNS` (or `GARNISH_COLUMNS`,
    /// `--width`, then 120) minus [`HARNESS_PADDING`] minus `padding`,
    /// never below [`MIN_WIDTH`] nor above [`MAX_WIDTH`].
    #[must_use]
    pub fn width(&self, columns: Option<usize>) -> usize {
        columns
            .unwrap_or(120)
            .min(MAX_WIDTH.saturating_add(HARNESS_PADDING))
            .saturating_sub(HARNESS_PADDING)
            .saturating_sub(self.padding)
            .max(MIN_WIDTH)
    }

    /// Separator for a row.
    #[must_use]
    pub fn separator<'a>(&'a self, row: &'a RowCfg) -> &'a str {
        row.separator.as_deref().unwrap_or(&self.frame.chars.separator)
    }

    /// Separator for a row at animation frame `frame`: the row's own
    /// override wins, then `separator_frames[frame]`, then the static
    /// separator (SPEC § 4.2).
    #[must_use]
    pub fn separator_at<'a>(&'a self, row: &'a RowCfg, frame: usize) -> &'a str {
        row.separator
            .as_deref()
            .or_else(|| self.frame.separator_frames.get(frame).map(String::as_str))
            .unwrap_or(&self.frame.chars.separator)
    }
}

/// `[colors]` role overrides; unknown roles and bad colors are reported and skipped.
fn resolve_colors(
    raw: &BTreeMap<String, String>,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<Role, Color> {
    let mut overrides: BTreeMap<Role, Color> = BTreeMap::new();
    for (k, v) in raw {
        match (Role::parse(k), Color::parse(v)) {
            (Some(role), Some(color)) => {
                overrides.insert(role, color);
            }
            (None, _) => errors.push(ConfigError {
                path: format!("colors.{k}"),
                message: format!(
                    "unknown color role; expected one of {}",
                    Role::ALL.iter().map(|r| r.name()).collect::<Vec<_>>().join(", ")
                ),
                line: None,
            }),
            (_, None) => errors.push(ConfigError {
                path: format!("colors.{k}"),
                message: format!("invalid color {v:?}; use a name, a 0-255 index, or #rrggbb"),
                line: None,
            }),
        }
    }
    overrides
}

/// The `[[row]]` tables as configured (SPEC § 4.1, § 4.3): normalised to the
/// resolved shape (every row at least one column), with the layout rules
/// checked against the tree the file wrote. `key` is the array name the file
/// used, so an error points at what was typed.
fn resolve_rows(
    key: &str,
    raw: &[RawRow],
    defined: &BTreeMap<String, BoxCfg>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> Vec<RowCfg> {
    let mut rows: Vec<RowCfg> = raw
        .iter()
        .enumerate()
        .map(|(i, r)| resolve_row(&format!("{key}[{i}]"), r, false, defined, theme, errors))
        .collect();
    check_box_runs(key, &mut rows, errors);
    rows
}

/// One `[[row]]` or `[[row.col.row]]`.
fn resolve_row(
    path: &str,
    raw: &RawRow,
    inner: bool,
    defined: &BTreeMap<String, BoxCfg>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> RowCfg {
    let explicit_cols = raw.cols.is_some();
    // Columns win over the row's own groups: the row would otherwise have
    // two places for its modules and no rule for which is drawn first.
    if explicit_cols && !(raw.modules.is_empty() && raw.right.is_empty()) {
        errors.push(problem(
            path,
            "a row with [[row.col]] tables takes no `modules` or `right` of its own; \
             the columns win",
        ));
    }
    let mut cols: Vec<ColCfg> = raw.cols.as_ref().map_or_else(
        || {
            vec![ColCfg {
                left: raw.modules.clone(),
                right: raw.right.clone(),
                ..ColCfg::default()
            }]
        },
        |cols| {
            cols.iter()
                .enumerate()
                .map(|(j, c)| resolve_col(&format!("{path}.col[{j}]"), c, defined, theme, errors))
                .collect()
        },
    );
    if cols.is_empty() {
        cols.push(ColCfg::default());
    }
    // A lone column reads left, the first left, the last right, the rest
    // centre, so a three-column row needs no `justify` at all (SPEC § 4.3).
    let last = cols.len().saturating_sub(1);
    for (j, col) in cols.iter_mut().enumerate() {
        if !col.justify_set {
            col.justify = match j {
                0 => Justify::Left,
                _ if j == last => Justify::Right,
                _ => Justify::Center,
            };
        }
    }
    let boxed = check_box_ref(path, raw.boxed.clone(), defined, errors);
    let title = resolve_title(
        path,
        raw.title.as_deref(),
        raw.title_justify,
        raw.title_pad,
        raw.title_color.as_deref(),
        theme,
        errors,
    );
    // Boxes never nest, in either direction (SPEC § 4.3): a column inside a
    // boxed row keeps its place but loses its own box.
    if boxed.is_some() {
        for (j, col) in cols.iter_mut().enumerate() {
            if col.boxed.take().is_some() {
                errors.push(problem(
                    &format!("{path}.col[{j}].box"),
                    "boxes never nest: this column is already inside its row's box",
                ));
            }
        }
    }
    // A row inside a named box gets no title of its own: the box has one.
    let title = match (&boxed, title) {
        (Some(BoxRef::Named(name)), Some(_)) => {
            errors.push(problem(
                &format!("{path}.title"),
                &format!("a row inside box {name:?} takes no title; [box.{name}] carries it"),
            ));
            None
        }
        (_, title) => title,
    };
    // Only a row written empty is a spacer; a mistyped `modules` is an error
    // and an empty row, which `hide_empty_rows` then drops like any other.
    let bad_list = raw.bad_list || raw.cols.iter().flatten().any(|c| c.bad_list);
    let spacer = cols.iter().all(ColCfg::is_empty) && !bad_list;
    // A spacer asks for the cell because it *is* empty; a row with columns
    // asks for it because it can be several lines tall and its padding
    // lines are whitespace (SPEC § 4.3). A plain row of modules can be
    // neither, so `blank` on one is a mistake.
    let can_blank = spacer || explicit_cols;
    if raw.blank && !can_blank {
        errors.push(problem(
            &format!("{path}.blank"),
            "only a spacer (modules = [] with no right) or a row with columns can be marked blank",
        ));
    }
    let gap = raw.gap.unwrap_or(DEFAULT_GAP);
    RowCfg {
        cols,
        explicit_cols,
        gap: if inner { DEFAULT_GAP } else { gap },
        separator: raw.separator.clone(),
        title,
        boxed,
        spacer,
        blank: raw.blank && can_blank,
    }
}

/// One `[[row.col]]`.
fn resolve_col(
    path: &str,
    raw: &RawCol,
    defined: &BTreeMap<String, BoxCfg>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> ColCfg {
    let stacked = !raw.rows.is_empty();
    if stacked && !(raw.modules.is_empty() && raw.right.is_empty()) {
        errors.push(problem(
            path,
            "a column with [[row.col.row]] tables takes no `modules` or `right`; the stack wins",
        ));
    }
    let boxed = check_box_ref(path, raw.boxed.clone(), defined, errors);
    let rows: Vec<RowCfg> = raw
        .rows
        .iter()
        .enumerate()
        .map(|(k, r)| resolve_row(&format!("{path}.row[{k}]"), r, true, defined, theme, errors))
        .collect();
    // Boxes never nest, in either direction: a boxed column's rows may not
    // box themselves, and the box a row carries is the one that is dropped.
    let rows = if boxed.is_some() {
        rows.into_iter()
            .enumerate()
            .map(|(k, mut r)| {
                if r.boxed.take().is_some() {
                    errors.push(problem(
                        &format!("{path}.row[{k}].box"),
                        "boxes never nest: this row is already inside its column's box",
                    ));
                }
                r
            })
            .collect()
    } else {
        rows
    };
    ColCfg {
        width: raw.width.unwrap_or_default(),
        left: if stacked { Vec::new() } else { raw.modules.clone() },
        right: if stacked { Vec::new() } else { raw.right.clone() },
        justify: raw.justify.unwrap_or_default(),
        justify_set: raw.justify.is_some(),
        valign: raw.valign.unwrap_or_default(),
        boxed,
        rows,
    }
}

/// A `box` key that names no `[box.<name>]` is reported and ignored: the
/// alternative is a box drawn with defaults nobody asked for.
fn check_box_ref(
    path: &str,
    boxed: Option<BoxRef>,
    defined: &BTreeMap<String, BoxCfg>,
    errors: &mut Vec<ConfigError>,
) -> Option<BoxRef> {
    match boxed {
        Some(BoxRef::Named(name)) if !defined.contains_key(&name) => {
            errors.push(problem(
                &format!("{path}.box"),
                &format!("no [box.{name}] table; define it or use box = true"),
            ));
            None
        }
        other => other,
    }
}

/// A named box is one run of adjacent rows (SPEC § 4.3): a name that comes
/// back after another box, or after a bare row, is reported and the second
/// run unboxed, since two boxes cannot share a name.
fn check_box_runs(key: &str, rows: &mut [RowCfg], errors: &mut Vec<ConfigError>) {
    check_box_run(key, rows, &mut Vec::new(), errors);
}

/// One list of rows. A stack is a run of its own, so a name inside one is
/// never adjacent to a name outside it, but `seen` spans the whole tree: a
/// box is one run in the config, not one run per list.
fn check_box_run(
    key: &str,
    rows: &mut [RowCfg],
    seen: &mut Vec<String>,
    errors: &mut Vec<ConfigError>,
) {
    let name_of = |boxed: Option<&BoxRef>| boxed.and_then(BoxRef::name).map(str::to_owned);
    let reused = |name: &str, what: &str| {
        format!(
            "box {name:?} is already drawn around earlier rows; \
             a box is one run of adjacent rows, so this {what} is not boxed"
        )
    };
    let mut previous: Option<String> = None;
    for (i, row) in rows.iter_mut().enumerate() {
        let path = format!("{key}[{i}]");
        if let Some(name) = name_of(row.boxed.as_ref())
            && previous.as_ref() != Some(&name)
        {
            if seen.contains(&name) {
                errors.push(problem(&format!("{path}.box"), &reused(&name, "row")));
                row.boxed = None;
            } else {
                seen.push(name);
            }
        }
        previous = name_of(row.boxed.as_ref());
        for (j, col) in row.cols.iter_mut().enumerate() {
            let path = format!("{path}.col[{j}]");
            if let Some(name) = name_of(col.boxed.as_ref()) {
                if seen.contains(&name) {
                    errors.push(problem(&format!("{path}.box"), &reused(&name, "column")));
                    col.boxed = None;
                } else {
                    seen.push(name);
                }
            }
            if !col.rows.is_empty() {
                check_box_run(&format!("{path}.row"), &mut col.rows, seen, errors);
            }
        }
    }
}

/// The four `title*` keys as one value (SPEC § 4.3).
fn resolve_title(
    path: &str,
    text: Option<&str>,
    justify: Option<Justify>,
    pad: Option<usize>,
    color: Option<&str>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> Option<TitleCfg> {
    let color = color.and_then(|spec| {
        theme.resolve(spec).or_else(|| {
            errors.push(problem(
                &format!("{path}.title_color"),
                "expected a role name, a color name, 0-255, or #rrggbb",
            ));
            None
        })
    });
    let text = text?;
    Some(TitleCfg {
        text: text.to_owned(),
        justify: justify.unwrap_or_default(),
        pad: pad.unwrap_or(DEFAULT_TITLE_PAD),
        color,
    })
}

/// Whether `name` is joined anywhere in this row: by the row itself, by one
/// of its columns, or by a row of one of its stacks (an inner row may name a
/// box, and drawing one there is what `dashboard-panels` does).
fn joins_box(row: &RowCfg, name: &str) -> bool {
    let joins = |b: &Option<BoxRef>| b.as_ref().and_then(BoxRef::name) == Some(name);
    joins(&row.boxed)
        || row.cols.iter().any(|c| joins(&c.boxed) || c.rows.iter().any(|r| joins_box(r, name)))
}

/// The `[box.<name>]` tables (SPEC § 4.3), each validated under its own path.
fn resolve_boxes(
    raw: &BTreeMap<String, toml::Table>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<String, BoxCfg> {
    let styles = FrameStyle::ALL.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
    let mut out = BTreeMap::new();
    for (name, table) in raw {
        let base = format!("box.{name}");
        if !is_bare_key(name) {
            errors.push(problem(
                &base,
                "a box name is letters, digits, _ and - only, so `box = \"<name>\"` \
                 reads the same on a row",
            ));
            continue;
        }
        let mut cfg = BoxCfg::default();
        let (mut title, mut justify, mut pad, mut color) = (None, None, None, None);
        for (key, value) in table.clone() {
            let path = format!("{base}.{key}");
            match key.as_str() {
                "title" => title = text_field(&path, value, errors),
                "title_justify" => justify = enum_field(&path, value, JUSTIFIES, errors),
                "title_pad" => pad = bounded_count(&path, value, MAX_TITLE_PAD, errors),
                "title_color" => color = field::<String>(&path, value, errors),
                // Powerline has caps, not a box shape; the box is drawn
                // rounded rather than silently losing its sides.
                "style" => {
                    cfg.style = enum_field(&path, value, &styles, errors);
                    if cfg.style == Some(FrameStyle::Powerline) {
                        errors.push(problem(
                            &path,
                            "powerline has no box shape; this box is drawn rounded",
                        ));
                        cfg.style = Some(FrameStyle::Rounded);
                    }
                }
                "fill" => cfg.fill = field(&path, value, errors).unwrap_or(false),
                "color" => {
                    cfg.color = field::<String>(&path, value, errors).and_then(|spec| {
                        theme.resolve(&spec).or_else(|| {
                            errors.push(problem(
                                &path,
                                "expected a role name, a color name, 0-255, or #rrggbb",
                            ));
                            None
                        })
                    });
                }
                _ => errors.push(problem(
                    &path,
                    "unknown key; expected one of title, title_justify, title_pad, \
                     title_color, style, fill, color",
                )),
            }
        }
        cfg.title =
            resolve_title(&base, title.as_deref(), justify, pad, color.as_deref(), theme, errors);
        out.insert(name.clone(), cfg);
    }
    out
}

fn resolve(raw: &RawConfig, schemas: &[ModuleSchema], errors: &mut Vec<ConfigError>) -> Config {
    let preset = raw.preset.unwrap_or_default();
    let icons = raw.icons.unwrap_or_default();

    let requested = raw.theme.clone().unwrap_or_else(|| "garnish".to_owned());
    let pal = palette(&requested).unwrap_or_else(|| {
        errors.push(ConfigError {
            path: "theme".into(),
            message: format!(
                "unknown theme {requested:?}; expected one of {}",
                PALETTES.iter().map(|p| p.name).collect::<Vec<_>>().join(", ")
            ),
            line: None,
        });
        &PALETTES[0]
    });
    // The name of the palette in effect, so `config show` round-trips.
    let theme_name = pal.name.to_owned();
    let overrides = resolve_colors(&raw.colors, errors);
    let theme = Theme::from_palette(pal, &overrides);

    let frame = resolve_frame(raw.frame.as_ref(), preset, &theme, errors);
    let boxes = resolve_boxes(&raw.boxes, &theme, errors);
    let mut rows: Vec<RowCfg> = if raw.row.is_empty() {
        preset.rows()
    } else {
        resolve_rows(raw.rows_key(), &raw.row, &boxes, &theme, errors)
    };
    // A box nothing joins draws nothing: said once, here, rather than left
    // for the user to wonder about on screen.
    for name in boxes.keys() {
        if !rows.iter().any(|r| joins_box(r, name)) {
            errors.push(problem(
                &format!("box.{name}"),
                "no row or column joins this box; add box = \"<name>\" to one",
            ));
        }
    }
    let mut modules: BTreeMap<&'static str, ModuleCfg> = BTreeMap::new();
    for schema in schemas {
        let base = format!("modules.{}", schema.id);
        let table = raw.modules.get(schema.id);
        let ov =
            table.map_or_else(Overrides::default, |t| parse_overrides(schema, &base, t, errors));
        let module_preset = ov.preset.unwrap_or_else(|| preset.module_preset());
        modules.insert(schema.id, ModuleCfg::resolve(schema, module_preset, icons, &theme, &ov));
    }
    let texts = resolve_texts(raw.modules.get("text"), icons, &theme, errors);
    for id in raw.modules.keys() {
        if id != "text" && !schemas.iter().any(|s| s.id == id) {
            errors.push(ConfigError {
                path: format!("modules.{id}"),
                message: format!(
                    "unknown module; expected one of {}, or text.<name>",
                    schemas.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
                ),
                line: None,
            });
        }
    }
    // Preset rows are valid by construction; only explicit rows need checking.
    if !raw.row.is_empty() {
        check_row_ids(raw.rows_key(), &mut rows, schemas, &texts, errors);
    }

    let stale_after = resolve_stale_after(raw.stale_after, errors);

    Config {
        preset,
        icons,
        theme_name,
        theme,
        color: raw.color.unwrap_or_default(),
        truncate: raw.truncate.unwrap_or(true),
        stale_style: raw.stale_style.unwrap_or_default(),
        stale_after,
        padding: usize::from(raw.padding.unwrap_or(0)),
        align: raw.align.unwrap_or(false),
        right_justify: raw.right_justify.unwrap_or_default(),
        hide_empty_rows: raw.hide_empty_rows.unwrap_or(true),
        overflow: raw.overflow.unwrap_or_default(),
        ticker_step: resolve_step("ticker_step", raw.ticker_step, errors),
        // Plain text only: an escape sequence in the gap would be cut by the
        // window; and no longer than a row's worth of text.
        ticker_gap: crate::ansi::plain_text(match raw.ticker_gap.as_deref() {
            Some(gap) if gap.chars().count() > MAX_TEXT_CHARS => {
                errors.push(problem(
                    "ticker_gap",
                    &format!("must be at most {MAX_TEXT_CHARS} characters"),
                ));
                DEFAULT_TICKER_GAP
            }
            Some(gap) => gap,
            None => DEFAULT_TICKER_GAP,
        }),
        animate: raw.animate,
        // A ticker's period follows the scrolled group's width, so timers
        // that change width would make the window jump (SPEC § 4.1): the
        // smooth style is the default there, compact an explicit opt-in.
        durations: raw.durations.unwrap_or(match raw.overflow {
            Some(Overflow::Ticker) => DurationStyle::Fixed,
            _ => DurationStyle::Compact,
        }),
        format: raw.format.as_ref().map_or_else(FormatCfg::default, RawFormat::resolve),
        frame,
        rows,
        boxes,
        modules,
        texts,
    }
}

/// Every id on a `[[row]]` is a registered module or a defined `text.<name>`;
/// an unknown one is reported and removed, so the resolved config (and
/// `config show`) carries only ids that render.
fn check_row_ids(
    key: &str,
    rows: &mut [RowCfg],
    schemas: &[ModuleSchema],
    texts: &BTreeMap<String, ModuleCfg>,
    errors: &mut Vec<ConfigError>,
) {
    for (i, row) in rows.iter_mut().enumerate() {
        check_row_ids_at(&format!("{key}[{i}]"), row, schemas, texts, errors);
    }
}

/// [`check_row_ids`] for one row, at the path the file wrote it under: a
/// normalised one-column row keeps the plain `row[i].modules[j]` path, an
/// explicit column adds `.col[j]`, and a stack recurses one level more
/// (SPEC § 4.3 allows no deeper nesting).
fn check_row_ids_at(
    path: &str,
    row: &mut RowCfg,
    schemas: &[ModuleSchema],
    texts: &BTreeMap<String, ModuleCfg>,
    errors: &mut Vec<ConfigError>,
) {
    let explicit = row.explicit_cols;
    for (j, col) in row.cols.iter_mut().enumerate() {
        let base = if explicit { format!("{path}.col[{j}]") } else { path.to_owned() };
        for (k, inner) in col.rows.iter_mut().enumerate() {
            check_row_ids_at(&format!("{base}.row[{k}]"), inner, schemas, texts, errors);
        }
        for (field, ids) in [("modules", &mut col.left), ("right", &mut col.right)] {
            let mut j = 0_usize;
            ids.retain(|id| {
                let path = format!("{base}.{field}[{j}]");
                j = j.saturating_add(1);
                let (known, message) = id.strip_prefix(crate::modules::text::PREFIX).map_or_else(
                    || {
                        (
                            schemas.iter().any(|s| s.id == id),
                            format!(
                                "unknown module {id:?}; expected one of {}, or text.<name>",
                                schemas.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
                            ),
                        )
                    },
                    |name| {
                        (
                            texts.contains_key(name),
                            format!("unknown text module {name:?}; define [modules.text.{name}]"),
                        )
                    },
                );
                if !known {
                    errors.push(problem(&path, &message));
                }
                known
            });
        }
    }
}

/// The common keys a `[modules.text.<name>]` table may not carry (SPEC
/// § 3.7), each with why: reported and removed before the shared parser
/// sees the table, and left out of the "expected one of" list for a text
/// module so the message never recommends one of them.
const TEXT_REJECTED_KEYS: [(&str, &str); 4] = [
    ("refresh", "text modules render every tick; remove this key"),
    ("preset", "text modules have no presets; remove this key"),
    ("icons", "text modules have no icons; remove this table"),
    ("max_width", "a text module's box is sized by `width`; remove this key"),
];

/// Whether a `[modules.text.<name>]` table takes the common key: every one
/// but the [`TEXT_REJECTED_KEYS`] (SPEC § 3.7).
#[must_use]
pub(crate) fn text_takes(key: &str) -> bool {
    !TEXT_REJECTED_KEYS.iter().any(|(rejected, _)| *rejected == key)
}

/// A bare TOML key: what a text module or a box may be called, so
/// `text.<name>` and `box = "<name>"` are unambiguous on a line and
/// `config show` can write `[modules.text.<name>]` and `[box.<name>]` back
/// verbatim.
#[must_use]
pub(crate) fn is_bare_key(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The `[modules.text.<name>]` tables (SPEC § 3.7): each is validated against
/// the text schema under its own path. [`TEXT_REJECTED_KEYS`] do not apply to
/// text modules, `step` must lie in [`STEP_RANGE`], `color` is the shorthand
/// for `colors.text` (an explicit `colors.text` wins), and `text` and `gap`
/// are reduced to plain text so a scrolled window can never cut an escape
/// sequence.
fn resolve_texts(
    family: Option<&toml::Table>,
    icons: IconSet,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<String, ModuleCfg> {
    let schema = &*crate::modules::text::SCHEMA;
    let mut texts = BTreeMap::new();
    for (name, value) in family.into_iter().flatten() {
        let base = format!("modules.text.{name}");
        if !is_bare_key(name) {
            errors.push(problem(
                &base,
                "a text module name is letters, digits, `_` and `-` only (it becomes the id text.<name>)",
            ));
            continue;
        }
        let Some(table) = value.as_table() else {
            errors.push(problem(&base, "expected a [modules.text.<name>] table"));
            continue;
        };
        let mut table = table.clone();
        let color = table.remove("color");
        for (key, why) in TEXT_REJECTED_KEYS {
            if table.remove(key).is_some() {
                errors.push(problem(&format!("{base}.{key}"), why));
            }
        }
        let mut ov = parse_overrides(schema, &base, &table, errors);
        if let Some(color) = color {
            match color.as_str() {
                Some(s) if Role::parse(s).is_some() || Color::parse(s).is_some() => {
                    ov.colors.entry("text".to_owned()).or_insert_with(|| s.to_owned());
                }
                _ => errors.push(problem(
                    &format!("{base}.color"),
                    "expected a role name, a color name, 0-255, or #rrggbb",
                )),
            }
        }
        if let Some(Value::Float(step)) = ov.opts.get("step")
            && !STEP_RANGE.contains(step)
        {
            errors.push(problem(&format!("{base}.step"), STEP_MESSAGE));
            ov.opts.remove("step");
        }
        for key in ["text", "gap"] {
            if let Some(Value::Str(s)) = ov.opts.get_mut(key) {
                *s = crate::ansi::plain_text(s);
            }
        }
        // `url` (SPEC § 3.7) must meet the painter's rule (§ 5), or the
        // link would vanish on screen with `config check` saying nothing.
        if let Some(Value::Str(url)) = ov.opts.get("url")
            && !url.is_empty()
            && !crate::ansi::safe_link(url)
        {
            errors.push(problem(
                &format!("{base}.url"),
                "must be an http:// or https:// URL of printable ASCII (percent-encode anything else)",
            ));
            ov.opts.remove("url");
        }
        texts.insert(name.clone(), ModuleCfg::resolve(schema, Preset::Default, icons, theme, &ov));
    }
    texts
}

/// The steps an animation may take per tick.
///
/// Below the range the frame never changes in a lifetime; above it
/// `now × step` saturates and freezes (`1e308`), so both ends are rejected
/// rather than silently still. Public so the generated reference prints the
/// bound the parser enforces rather than a looser "> 0" of its own.
pub const STEP_RANGE: std::ops::RangeInclusive<f64> = 0.001..=1000.0;

/// [`STEP_RANGE`] as the reference and the `*_step` error message spell it.
pub const STEP_BOUNDS: &str = "0.001–1000";
const STEP_MESSAGE: &str =
    "must be a number between 0.001 and 1000: cells per tick (0.5 = every second tick)";

/// A `*_step` key: cells (or frames) an animation advances per tick, within
/// [`STEP_RANGE`] (0.5 = every second tick); anything else is reported and
/// replaced by 1.
fn resolve_step(path: &str, raw: Option<f64>, errors: &mut Vec<ConfigError>) -> f64 {
    match raw {
        None => 1.0,
        Some(step) if STEP_RANGE.contains(&step) => step,
        Some(_) => {
            errors.push(problem(path, STEP_MESSAGE));
            1.0
        }
    }
}

/// `stale_after`: TTL periods before an overdue value is styled stale.
/// Zero is reported and clamped to one so rendering never divides the TTL away.
fn resolve_stale_after(raw: Option<u32>, errors: &mut Vec<ConfigError>) -> u32 {
    let stale_after = raw.unwrap_or(5);
    if stale_after == 0 {
        errors.push(ConfigError {
            path: "stale_after".into(),
            message: "must be at least 1 (TTL periods before a value is styled stale)".into(),
            line: None,
        });
    }
    stale_after.max(1)
}

fn resolve_frame(
    raw: Option<&RawFrame>,
    preset: TopPreset,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> FrameCfg {
    let fallback = if preset.framed() { FrameStyle::Rounded } else { FrameStyle::None };
    let style = raw.and_then(|f| f.style).unwrap_or(fallback);
    let mut chars = FrameChars::for_style(style);
    if let Some(f) = raw {
        let set = |dst: &mut String, src: &Option<String>| {
            if let Some(v) = src {
                dst.clone_from(v);
            }
        };
        set(&mut chars.first, &f.first);
        set(&mut chars.middle, &f.middle);
        set(&mut chars.last, &f.last);
        set(&mut chars.single, &f.single);
        // The fill glyph is repeated across the rule, so it has to be one
        // cell; anything else is reported and the style's own glyph stays.
        match &f.fill_char {
            Some(c) if crate::ansi::display_width(c) == 1 => chars.fill.clone_from(c),
            Some(_) => errors.push(problem("frame.fill_char", "must be exactly one cell wide")),
            None => {}
        }
        set(&mut chars.right_first, &f.right_first);
        set(&mut chars.right_middle, &f.right_middle);
        set(&mut chars.right_last, &f.right_last);
        set(&mut chars.right_single, &f.right_single);
        set(&mut chars.pad, &f.pad);
        set(&mut chars.separator, &f.separator);
        // The five box glyphs (SPEC § 4.3) are drawn one per cell at the
        // ends of a box's lines, so each is one cell or the style's own
        // glyph stays.
        for (key, given, dst) in [
            ("top_left", &f.top_left, &mut chars.top_left),
            ("top_right", &f.top_right, &mut chars.top_right),
            ("bottom_left", &f.bottom_left, &mut chars.bottom_left),
            ("bottom_right", &f.bottom_right, &mut chars.bottom_right),
            ("side", &f.side, &mut chars.side),
        ] {
            match given {
                Some(c) if crate::ansi::display_width(c) == 1 => dst.clone_from(c),
                Some(_) => {
                    let path = format!("frame.{key}");
                    errors.push(problem(&path, "must be exactly one cell wide"));
                }
                None => {}
            }
        }
    }
    // Filling is on for every style: with `none` the rule is spaces, which is
    // what right-aligns the `right` group on an unframed line.
    let fill = raw.and_then(|f| f.fill).unwrap_or(true);
    // SPEC § 4.2: a pattern is one-cell glyphs (each lands in exactly one
    // rule cell); separator frames all share one width so columns never
    // jitter. Anything else is reported and the static fallback stays.
    let fill_pattern: Vec<String> = raw
        .and_then(|f| f.fill_pattern.as_deref())
        .map(crate::ansi::plain_text)
        .filter(|p| !p.is_empty())
        .map_or_else(Vec::new, |p| {
            let cells: Vec<String> = p.chars().map(|c| c.to_string()).collect();
            if cells.iter().all(|c| crate::ansi::display_width(c) == 1) {
                cells
            } else {
                errors.push(problem(
                    "frame.fill_pattern",
                    "every glyph in the pattern must be one cell wide",
                ));
                Vec::new()
            }
        });
    let fill_pattern = if !fill && !fill_pattern.is_empty() {
        errors.push(problem(
            "frame.fill_pattern",
            "has no effect with fill = false (there is no rule to paint)",
        ));
        Vec::new()
    } else {
        fill_pattern
    };
    let separator_frames: Vec<String> =
        raw.and_then(|f| f.separator_frames.as_deref()).map_or_else(Vec::new, |frames| {
            equal_width_frames(frames.iter().map(String::as_str), "the columns", true)
                .inspect_err(|msg| errors.push(problem("frame.separator_frames", msg)))
                .unwrap_or_default()
        });
    let separator_color =
        resolve_separator_color(raw.and_then(|f| f.separator_color.as_deref()), theme, errors);
    FrameCfg {
        style,
        chars,
        fill,
        separator_color,
        fill_pattern,
        fill_step: resolve_step("frame.fill_step", raw.and_then(|f| f.fill_step), errors),
        fill_direction: raw.and_then(|f| f.fill_direction).unwrap_or_default(),
        separator_frames,
        separator_step: resolve_step(
            "frame.separator_step",
            raw.and_then(|f| f.separator_step),
            errors,
        ),
    }
}

/// SPEC § 4.1 `separator_color`: `inherit`, or a role or literal resolved
/// now, the muted role standing in for a bad value as it does when unset.
fn resolve_separator_color(
    spec: Option<&str>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> SeparatorColor {
    let muted =
        || SeparatorColor::Fixed { spec: "muted".to_owned(), color: theme.role(Role::Muted) };
    match spec {
        None => muted(),
        Some("inherit") => SeparatorColor::Inherit,
        Some(spec) => theme.resolve(spec).map_or_else(
            || {
                errors.push(problem(
                    "frame.separator_color",
                    &format!(
                        "invalid color {spec:?}; use inherit, a role name, a color name, 0-255, or #rrggbb"
                    ),
                ));
                muted()
            },
            |color| SeparatorColor::Fixed { spec: spec.to_owned(), color },
        ),
    }
}

fn parse_overrides(
    schema: &ModuleSchema,
    base: &str,
    table: &toml::Table,
    errors: &mut Vec<ConfigError>,
) -> Overrides {
    let mut ov = Overrides::default();
    let mut err = |key: &str, msg: String| {
        errors.push(ConfigError { path: format!("{base}.{key}"), message: msg, line: None });
    };
    for (key, value) in table {
        match key.as_str() {
            "enabled" => match value.as_bool() {
                Some(b) => ov.enabled = Some(b),
                None => err(key, "expected true or false".into()),
            },
            "preset" => match value.as_str().and_then(Preset::parse) {
                Some(p) => ov.preset = Some(p),
                None => err(key, "expected \"minimal\", \"default\" or \"full\"".into()),
            },
            "refresh" => match value.as_integer().and_then(|i| u64::try_from(i).ok()) {
                Some(0) if schema.refresh > 0 => err(
                    key,
                    "this module is refreshed by a background worker; use at least 1 second".into(),
                ),
                Some(n) => ov.refresh = Some(n),
                None => err(key, "expected a non-negative integer (seconds)".into()),
            },
            // Checked against the schema's measure (SPEC § 3), as `refresh`
            // is against `schema.refresh`, so hand-parsed like it.
            "hide" => match hide_rules(schema, value) {
                Ok(rules) => ov.hide = Some(rules),
                Err(msg) => err(key, msg),
            },
            "icons" => match value.as_table() {
                Some(t) => parse_icons(schema, t, &mut ov, &mut err),
                None => err(key, "expected a table of icon overrides".into()),
            },
            "colors" => match value.as_table() {
                Some(t) => parse_colors(schema, t, &mut ov, &mut err),
                None => err(key, "expected a table of color overrides".into()),
            },
            // The common options and the module's own go through the same
            // coercion and cap (`COMMON_OPTS` first: a schema never redeclares
            // a common key).
            other => {
                match COMMON_OPTS.iter().find(|o| o.key == other).or_else(|| schema.opt(other)) {
                    Some(spec) => match coerce(spec.kind, value).and_then(|v| bounded(spec, v)) {
                        Ok(v) if !set_common(&mut ov, other, v.clone()) => {
                            ov.opts.insert(other.to_owned(), v);
                        }
                        Ok(_) => {}
                        Err(msg) => err(other, msg),
                    },
                    None => err(other, unknown_option_message(schema)),
                }
            }
        }
    }
    ov
}

/// Store a coerced [`COMMON_OPTS`] value on the overrides; `false` when the
/// key is not a common option. The row strings are reduced to plain text
/// here like `text` and `gap` (SPEC § 5).
fn set_common(ov: &mut Overrides, key: &str, value: Value) -> bool {
    match (key, value) {
        ("label", Value::Str(s)) => ov.label = Some(crate::ansi::plain_text(&s)),
        ("prefix", Value::Str(s)) => ov.prefix = Some(crate::ansi::plain_text(&s)),
        ("suffix", Value::Str(s)) => ov.suffix = Some(crate::ansi::plain_text(&s)),
        ("hide_when_empty", Value::Bool(b)) => ov.hide_when_empty = Some(b),
        // `coerce` rejects a negative integer, so the conversion cannot fail.
        ("max_width", Value::Int(n)) => ov.max_width = Some(u64::try_from(n).unwrap_or(0)),
        _ => return false,
    }
    true
}

/// A module's `hide` list (SPEC § 3): every entry a state the schema's
/// measure allows, or the reason the key is refused whole (the per-key
/// fallback of § 5: the default, no hiding, stands in for the list).
fn hide_rules(schema: &ModuleSchema, value: &toml::Value) -> Result<Vec<HideRule>, String> {
    let accepts = || format!("this module accepts {}", schema.hide_states().join(", "));
    let items = value.as_array().ok_or_else(|| "expected a list of strings".to_owned())?;
    items
        .iter()
        .map(|item| {
            let text = item.as_str().ok_or_else(|| "expected a list of strings".to_owned())?;
            let rule = HideRule::parse(text).map_err(|e| format!("{e}; {}", accepts()))?;
            if rule.applies_to(schema.measure) {
                Ok(rule)
            } else {
                Err(format!("{text:?} does not apply here; {}", accepts()))
            }
        })
        .collect()
}

/// The size limit a module option must respect, from its schema
/// ([`OptSpec::max`]): cell counts at most [`MAX_CELLS`], row text at most
/// [`MAX_TEXT_CHARS`] characters, and so on. A row is a fixed, small thing;
/// a number beyond the cap is a mistake, and honouring it would size an
/// allocation or a loop on every tick.
fn bounded(spec: &OptSpec, value: Value) -> Result<Value, String> {
    spec.over_max(&value).map_or(Ok(value), Err)
}

fn unknown_option_message(schema: &ModuleSchema) -> String {
    let is_text = schema.id == crate::modules::text::SCHEMA.id;
    format!(
        "unknown option; expected one of {}",
        common_keys()
            .filter(|k| !is_text || text_takes(k))
            .chain(std::iter::once("colors"))
            .chain(schema.opts.iter().map(|o| o.key))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// An animation frame list (SPEC § 4.2): plain text, at least one frame, and
/// every frame the same width, or whatever it decorates jitters as the
/// frames cycle. `what` names that thing in the message.
///
/// One rule for the frame's `separator_frames` and every icon table's
/// `<key>_frames`: every frame is the same width, or the thing they draw
/// jitters from tick to tick.
///
/// `allow_empty` is what the two keys disagree on, and it is a
/// compatibility rule rather than a design one: `[]` is the line every
/// `garnish config init` has ever written for `separator_frames` (it means
/// "no animation, keep the static separator"), so rejecting it would put a
/// `⚠ config:` row on every tick of every config in the wild. An icon's
/// `<key>_frames = []` has always been reported and nothing generates it.
fn equal_width_frames<'a>(
    frames: impl IntoIterator<Item = &'a str>,
    what: &str,
    allow_empty: bool,
) -> Result<Vec<String>, String> {
    let frames: Vec<String> = frames.into_iter().map(crate::ansi::plain_text).collect();
    if frames.is_empty() && allow_empty {
        return Ok(frames);
    }
    let Some(width) = frames.first().map(|f| crate::ansi::display_width(f)) else {
        return Err("expected at least one frame".to_owned());
    };
    if frames.iter().all(|f| crate::ansi::display_width(f) == width) {
        Ok(frames)
    } else {
        Err(format!("every frame must have the same width, or {what} would jitter"))
    }
}

fn parse_icons(
    schema: &ModuleSchema,
    table: &toml::Table,
    ov: &mut Overrides,
    err: &mut impl FnMut(&str, String),
) {
    for (ik, iv) in table {
        // `<key>_frames`: equal-width frames cycled one per tick (SPEC § 4.2).
        if let Some(base) = ik.strip_suffix("_frames")
            && let Some(spec) = schema.icon(base)
        {
            let given: Option<Vec<&str>> =
                iv.as_array().and_then(|items| items.iter().map(toml::Value::as_str).collect());
            match given {
                Some(frames) => match equal_width_frames(frames, "the row", false) {
                    // A frame becomes the glyph for that tick, so the bar's
                    // one-cell rule applies to every frame too. Checking only
                    // the static arm below left the same defect reachable
                    // through `fill_frames`, equal widths and all.
                    Ok(frames)
                        if spec.one_cell()
                            && frames.iter().any(|f| {
                                !(f.is_empty() && spec.may_be_blank())
                                    && crate::ansi::display_width(f) != 1
                            }) =>
                    {
                        err(&format!("icons.{ik}"), "must be exactly one cell wide".into());
                    }
                    Ok(frames) => {
                        ov.icon_frames.insert(base.to_owned(), frames);
                    }
                    Err(msg) => err(&format!("icons.{ik}"), msg),
                },
                None => err(&format!("icons.{ik}"), "expected a list of strings".into()),
            }
            continue;
        }
        match (schema.icon(ik), iv.as_str()) {
            (Some(spec), Some(s)) => {
                let glyph = crate::ansi::plain_text(s);
                // A bar glyph is repeated cell by cell, so anything but one
                // cell breaks the row's arithmetic; `bar` would swap in a
                // safe glyph and the override would vanish in silence. Same
                // rule, same message as `frame.fill_char`. Blanking the
                // marker is not that case: it is how the marker is turned
                // off, and `bar` honours it (`IconSpec::may_be_blank`).
                let blanked = glyph.is_empty() && spec.may_be_blank();
                if spec.one_cell() && !blanked && crate::ansi::display_width(&glyph) != 1 {
                    err(&format!("icons.{ik}"), "must be exactly one cell wide".into());
                } else {
                    ov.icons.insert(ik.clone(), glyph);
                }
            }
            (None, _) => err(
                &format!("icons.{ik}"),
                format!(
                    "unknown icon; expected one of {}",
                    schema.icons.iter().map(|i| i.key).collect::<Vec<_>>().join(", ")
                ),
            ),
            (_, None) => err(&format!("icons.{ik}"), "expected a string".into()),
        }
    }
}

fn parse_colors(
    schema: &ModuleSchema,
    table: &toml::Table,
    ov: &mut Overrides,
    err: &mut impl FnMut(&str, String),
) {
    for (ck, cv) in table {
        match (schema.color(ck), cv.as_str()) {
            (Some(_), Some(s)) if Role::parse(s).is_some() || Color::parse(s).is_some() => {
                ov.colors.insert(ck.clone(), s.to_owned());
            }
            (Some(_), Some(s)) => err(
                &format!("colors.{ck}"),
                format!("invalid color {s:?}; use a role name, a color name, 0-255, or #rrggbb"),
            ),
            (None, _) => err(
                &format!("colors.{ck}"),
                format!(
                    "unknown color; expected one of {}",
                    schema.colors.iter().map(|c| c.key).collect::<Vec<_>>().join(", ")
                ),
            ),
            (_, None) => err(&format!("colors.{ck}"), "expected a string".into()),
        }
    }
}

fn coerce(kind: Kind, value: &toml::Value) -> Result<Value, String> {
    match kind {
        Kind::Bool => {
            value.as_bool().map(Value::Bool).ok_or_else(|| "expected true or false".into())
        }
        Kind::Int => value
            .as_integer()
            .filter(|i| *i >= 0)
            .map(Value::Int)
            .ok_or_else(|| "expected a non-negative integer".into()),
        // TOML takes `nan` and `inf`; no option means either.
        Kind::Float => value
            .as_float()
            .or_else(|| {
                value.as_integer().map(|i| crate::num::u64_to_f64(u64::try_from(i).unwrap_or(0)))
            })
            .filter(|f| f.is_finite())
            .map(Value::Float)
            .ok_or_else(|| "expected a number".into()),
        Kind::Str => value
            .as_str()
            .map(|s| Value::Str(s.to_owned()))
            .ok_or_else(|| "expected a string".into()),
        Kind::Enum(allowed) => value
            .as_str()
            .filter(|s| allowed.contains(s))
            .map(|s| Value::Str(s.to_owned()))
            .ok_or_else(|| {
                format!(
                    "expected one of {}",
                    allowed.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(", ")
                )
            }),
        Kind::StrList | Kind::ColorList => {
            let items = value.as_array().ok_or_else(|| "expected a list of strings".to_owned())?;
            let strs: Option<Vec<String>> =
                items.iter().map(|i| i.as_str().map(str::to_owned)).collect();
            let strs = strs.ok_or_else(|| "expected a list of strings".to_owned())?;
            let bad_color = (kind == Kind::ColorList)
                .then(|| {
                    strs.iter().find(|s| Role::parse(s).is_none() && Color::parse(s).is_none())
                })
                .flatten();
            if let Some(bad) = bad_color {
                return Err(format!("invalid color {bad:?}"));
            }
            Ok(Value::StrList(strs))
        }
        Kind::NumList => {
            let items = value.as_array().ok_or_else(|| "expected a list of numbers".to_owned())?;
            let nums: Option<Vec<f64>> = items
                .iter()
                .map(|i| {
                    i.as_float()
                        .or_else(|| {
                            i.as_integer()
                                .map(|n| crate::num::u64_to_f64(u64::try_from(n).unwrap_or(0)))
                        })
                        .filter(|f| f.is_finite())
                })
                .collect();
            nums.map(Value::NumList).ok_or_else(|| "expected a list of numbers".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ColorSpec, IconSpec, OptSpec};
    use crate::icons::glyph;

    fn schemas() -> Vec<ModuleSchema> {
        vec![
            ModuleSchema {
                id: "path",
                measure: None,
                summary: "",
                doc: "",
                sources: &[],
                refresh: 0,
                opts: vec![OptSpec::new("depth", Kind::Int, "", Value::Int(2))],
                icons: vec![IconSpec { key: "folder", doc: "", glyph: glyph("N", "U", "E", "A") }],
                colors: vec![ColorSpec { key: "dir", doc: "", default: "accent" }],
            },
            ModuleSchema {
                id: "clock",
                measure: None,
                summary: "",
                doc: "",
                sources: &[],
                refresh: 0,
                opts: vec![OptSpec::new(
                    "format",
                    Kind::Enum(&["24h", "12h"]),
                    "",
                    Value::Str("24h".into()),
                )],
                icons: vec![],
                colors: vec![],
            },
        ]
    }

    #[test]
    fn empty_text_is_defaults() {
        let (c, errs) = parse("", &schemas());
        assert_eq!(errs, Vec::new());
        assert_eq!(c.preset, TopPreset::Default);
        assert_eq!(c.rows.len(), 4);
        assert_eq!(c.frame.style, FrameStyle::Rounded);
        assert!(c.frame.fill);
        assert_eq!(c.modules.get("path").map(|m| m.int("depth")), Some(2));
        assert_eq!(c.width(Some(100)), 96);
    }

    #[test]
    fn width_subtracts_the_harness_frame_then_padding() {
        let schemas = schemas();
        let c = Config::defaults(&schemas);
        assert_eq!(c.width(None), 116, "default COLUMNS is 120");
        assert_eq!(c.width(Some(80)), 76);
        assert_eq!(c.width(Some(12)), MIN_WIDTH, "never below the floor");
        assert_eq!(c.width(Some(0)), MIN_WIDTH);
        let (c, _) = parse("padding = 2", &schemas);
        assert_eq!(c.width(Some(80)), 74, "statusLine.padding = 1 costs two more cells");
    }

    #[test]
    fn align_and_durations_default_off_and_parse() {
        let schemas = schemas();
        let (c, errs) = parse("", &schemas);
        assert_eq!(errs, Vec::new());
        assert!(!c.align);
        assert_eq!(c.durations, DurationStyle::Compact);
        let (c, errs) = parse("align = true\ndurations = \"fixed\"", &schemas);
        assert_eq!(errs, Vec::new());
        assert!(c.align);
        assert_eq!(c.durations, DurationStyle::Fixed);
        let (c, errs) = parse("align = true\ndurations = \"loose\"", &schemas);
        assert_eq!(errs.len(), 1, "unknown duration style must be reported: {errs:?}");
        assert_eq!(errs[0].path, "durations");
        assert_eq!(c.durations, DurationStyle::Compact, "and fall back to the default");
        assert!(c.align, "while the valid key next to it stays in effect");
        assert_eq!(c.right_justify, RightJustify::End, "end is the default");
        let (c, errs) = parse("right_justify = \"start\"", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.right_justify, RightJustify::Start);
        let (c, errs) = parse("right_justify = \"middle\"", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.ends_with("expected one of end, start"), "{}", errs[0].message);
        assert_eq!(c.right_justify, RightJustify::End);
        assert!(c.hide_empty_rows, "empty rows are hidden by default");
        let (c, errs) = parse(
            "hide_empty_rows = false\n[[row]]\nmodules = []\n[[row]]\nright = [\"clock\"]\n[[row]]\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert_eq!(errs, Vec::new());
        assert!(!c.hide_empty_rows);
        let spacers: Vec<bool> = c.rows.iter().map(|l| l.spacer).collect();
        // A mistyped list is an error and an empty row, never a spacer
        // (whole-stack review: it rendered as a permanent blank rule).
        let (bad, errs) = parse(
            "[[line]]\nmodules = \"clock\"\n[[line]]\n[[line]]\nmodules = [1, 2]\n",
            &schemas,
        );
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert_eq!(errs[0].path, "line[0].modules", "the error names the array the file used");
        assert!(!bad.rows[0].spacer && bad.rows[0].cols[0].left.is_empty());
        assert!(bad.rows[1].spacer, "a [[line]] with no keys is a spacer");
        assert!(!bad.rows[2].spacer, "a list of non-ids is a mistake, not a spacer");
        assert_eq!(
            spacers,
            vec![true, false, false],
            "only `modules = []` with no `right` is a spacer"
        );
        assert!(Config::defaults(&schemas).rows.iter().all(|l| !l.spacer && !l.blank));
        // `blank = true` is an opt-in for spacers only (SPEC § 4.1): on a
        // line with modules it is reported and ignored; a wrong type too.
        let (c, errs) = parse(
            "[[line]]\nmodules = []\nblank = true\n[[line]]\nmodules = [\"path\"]\nblank = true\n[[line]]\nmodules = []\nblank = \"yes\"\n[[line]]\nmodules = []\n",
            &schemas,
        );
        assert_eq!(errs.len(), 2, "{errs:?}");
        let misuse = errs.iter().find(|e| e.path == "line[1].blank").expect("misuse reported");
        assert!(misuse.message.contains("only a spacer"), "{}", misuse.message);
        assert!(errs.iter().any(|e| e.path == "line[2].blank"), "wrong type reported: {errs:?}");
        let blanks: Vec<bool> = c.rows.iter().map(|l| l.blank).collect();
        assert_eq!(blanks, vec![true, false, false, false]);
        assert!(c.rows.iter().all(|r| r.blank || r.spacer || !r.cols[0].left.is_empty()));
        // A row with columns can be several lines tall, and its padding
        // lines are whitespace, so it may ask for the cell too (SPEC § 4.3).
        let (tall, errs) = parse(
            "[[row]]\nblank = true\n[[row.col]]\n[[row.col.row]]\nmodules = [\"path\"]\n[[row.col.row]]\nmodules = []\nblank = true\n",
            &schemas,
        );
        assert_eq!(errs, Vec::new());
        assert!(tall.rows[0].blank, "a row with columns may be blank");
        assert!(tall.rows[0].cols[0].rows[1].blank, "and so may an inner spacer");
    }

    /// A row without `[[row.col]]` resolves to exactly one `1fr` column
    /// carrying its groups, so every consumer sees one shape (SPEC § 4.3).
    #[test]
    fn a_plain_row_normalises_to_one_column_and_columns_default_by_position() {
        let schemas = schemas();
        let (c, errs) = parse("[[row]]\nmodules = [\"path\"]\nright = [\"clock\"]\n", &schemas);
        assert_eq!(errs, Vec::new());
        let row = &c.rows[0];
        assert!(!row.explicit_cols, "the plain form is written back as itself");
        assert_eq!(row.cols.len(), 1);
        assert_eq!(row.cols[0].width, Width::Fr(1));
        assert_eq!(row.cols[0].left, ["path"]);
        assert_eq!(row.cols[0].right, ["clock"]);
        assert_eq!(row.cols[0].justify, Justify::Left, "a lone column reads left");
        assert_eq!(row.gap, DEFAULT_GAP);

        // First left, last right, the middle centred: a three-column row
        // reads left / centre / right without saying so.
        let three = "[[row]]\n[[row.col]]\nmodules = [\"path\"]\n[[row.col]]\nmodules = [\"clock\"]\n[[row.col]]\nmodules = [\"clock\"]\n";
        let (c, errs) = parse(three, &schemas);
        assert_eq!(errs, Vec::new());
        let justify: Vec<Justify> = c.rows[0].cols.iter().map(|col| col.justify).collect();
        assert_eq!(justify, [Justify::Left, Justify::Center, Justify::Right]);
        assert!(c.rows[0].explicit_cols);
        assert!(c.rows[0].cols.iter().all(|col| !col.justify_set));
        let (c, errs) =
            parse("[[row]]\n[[row.col]]\njustify = \"center\"\nmodules = [\"path\"]\n", &schemas);
        assert_eq!(errs, Vec::new());
        assert!(c.rows[0].cols[0].justify_set, "an explicit justify is kept as written");
        assert_eq!(c.rows[0].cols[0].justify, Justify::Center);
    }

    /// The three `width` forms, and the one message that names all three:
    /// a quoted number is the easy mistake (SPEC § 4.3).
    #[test]
    fn column_widths_take_fr_auto_or_cells_and_nothing_else() {
        let schemas = schemas();
        let (c, errs) = parse(
            "[[row]]\n[[row.col]]\nwidth = \"2fr\"\n[[row.col]]\nwidth = \"auto\"\n[[row.col]]\nwidth = 24\n",
            &schemas,
        );
        assert_eq!(errs, Vec::new());
        let widths: Vec<Width> = c.rows[0].cols.iter().map(|col| col.width).collect();
        assert_eq!(widths, [Width::Fr(2), Width::Auto, Width::Cells(24)]);
        for bad in ["\"24\"", "\"0fr\"", "\"65fr\"", "\"fr\"", "1025", "-1", "true", "\"wide\""] {
            let text = format!("[[row]]\n[[row.col]]\nwidth = {bad}\n");
            let (c, errs) = parse(&text, &schemas);
            assert_eq!(errs.len(), 1, "{bad}: {errs:?}");
            assert_eq!(errs[0].path, "row[0].col[0].width");
            assert!(errs[0].message.contains("fr"), "{}", errs[0].message);
            assert_eq!(c.rows[0].cols[0].width, Width::Fr(1), "{bad}: the default stands in");
        }
    }

    /// Every layout rule SPEC § 4.3 says `config check` reports, each with
    /// its TOML path and the rest of the file left in effect.
    #[test]
    fn the_layout_rules_are_reported_with_their_paths() {
        let schemas = schemas();
        let cases: [(&str, &str, &str); 11] = [
            (
                "a row cannot hold both",
                "[[row]]\nmodules = [\"path\"]\n[[row.col]]\nmodules = [\"clock\"]\n",
                "row[0]",
            ),
            (
                "a column cannot hold both",
                "[[row]]\n[[row.col]]\nmodules = [\"path\"]\n[[row.col.row]]\nmodules = [\"clock\"]\n",
                "row[0].col[0]",
            ),
            ("gap above the cap", "[[row]]\ngap = 17\nmodules = [\"path\"]\n", "row[0].gap"),
            (
                "title_pad above the cap",
                "[[row]]\ntitle = \"x\"\ntitle_pad = 65\nmodules = [\"path\"]\n",
                "row[0].title_pad",
            ),
            (
                "justify outside its words",
                "[[row]]\n[[row.col]]\njustify = \"middle\"\n",
                "row[0].col[0].justify",
            ),
            (
                "valign outside its words",
                "[[row]]\n[[row.col]]\nvalign = \"middle\"\n",
                "row[0].col[0].valign",
            ),
            (
                "a box nobody defined",
                "[[row]]\nbox = \"repo\"\nmodules = [\"path\"]\n",
                "row[0].box",
            ),
            (
                "a box nobody joins",
                "[box.repo]\ntitle = \"Repository\"\n[[row]]\nmodules = [\"path\"]\n",
                "box.repo",
            ),
            (
                "a title inside a named box",
                "[box.repo]\n[[row]]\nbox = \"repo\"\ntitle = \"x\"\nmodules = [\"path\"]\n",
                "row[0].title",
            ),
            (
                "a box reused for a second run",
                "[box.repo]\n[[row]]\nbox = \"repo\"\nmodules = [\"path\"]\n[[row]]\nmodules = [\"clock\"]\n[[row]]\nbox = \"repo\"\nmodules = [\"clock\"]\n",
                "row[2].box",
            ),
            (
                "an inner row taking columns",
                "[[row]]\n[[row.col]]\n[[row.col.row]]\ngap = 2\nmodules = [\"path\"]\n",
                "row[0].col[0].row[0].gap",
            ),
        ];
        for (what, text, path) in cases {
            let (_, errs) = parse(text, &schemas);
            assert_eq!(errs.len(), 1, "{what}: {errs:?}");
            assert_eq!(errs[0].path, path, "{what}: {errs:?}");
        }

        // Boxes never nest, in either direction, and the inner one goes.
        let (c, errs) = parse(
            "[box.a]\n[[row]]\n[[row.col]]\nbox = \"a\"\n[[row.col.row]]\nbox = true\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[0].col[0].row[0].box");
        assert_eq!(c.rows[0].cols[0].boxed, Some(BoxRef::Named("a".into())));
        assert_eq!(c.rows[0].cols[0].rows[0].boxed, None, "the inner box is dropped");
        let (c, errs) = parse(
            "[box.a]\n[[row]]\nbox = \"a\"\n[[row.col]]\nbox = true\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[0].col[0].box");
        assert_eq!(c.rows[0].boxed, Some(BoxRef::Named("a".into())));
        assert_eq!(c.rows[0].cols[0].boxed, None, "a column inside a boxed row loses its box");

        // A second run of the same name is unboxed, the first keeps its box.
        let (c, _) = parse(
            "[box.repo]\n[[row]]\nbox = \"repo\"\nmodules = [\"path\"]\n[[row]]\nmodules = [\"clock\"]\n[[row]]\nbox = \"repo\"\nmodules = [\"clock\"]\n",
            &schemas,
        );
        assert_eq!(c.rows[0].boxed, Some(BoxRef::Named("repo".into())));
        assert_eq!(c.rows[2].boxed, None);

        // The columns win, and the row's own groups are dropped, not merged.
        let (c, _) =
            parse("[[row]]\nmodules = [\"path\"]\n[[row.col]]\nmodules = [\"clock\"]\n", &schemas);
        assert_eq!(c.rows[0].cols.len(), 1);
        assert_eq!(c.rows[0].cols[0].left, ["clock"]);
    }

    /// A stack is two levels deep and no deeper, and both caps hold.
    #[test]
    fn stacks_and_columns_are_bounded() {
        let schemas = schemas();
        let mut text = String::from("[[row]]\n");
        for _ in 0..(MAX_COLS + 2) {
            text.push_str("[[row.col]]\nmodules = [\"path\"]\n");
        }
        let (c, errs) = parse(&text, &schemas);
        assert_eq!(errs.len(), 1, "one message, not one per extra column: {errs:?}");
        assert_eq!(c.rows[0].cols.len(), MAX_COLS);
        let mut text = String::from("[[row]]\n[[row.col]]\n");
        for _ in 0..=MAX_COLS {
            text.push_str("[[row.col.row]]\nmodules = [\"path\"]\n");
        }
        let (c, errs) = parse(&text, &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(c.rows[0].cols[0].rows.len(), MAX_COLS);
        // An inner row is one line: it takes no columns of its own.
        let (_, errs) = parse(
            "[[row]]\n[[row.col]]\n[[row.col.row]]\n[[row.col.row.col]]\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert!(
            errs.iter().any(|e| e.path.ends_with("row[0].col")),
            "an inner row takes no columns: {errs:?}"
        );
    }

    /// `[box.<name>]` resolves its title and colours like any other table,
    /// and keeps its own `fill` default (SPEC § 4.3).
    #[test]
    fn boxes_resolve_their_titles_styles_and_colours() {
        let schemas = schemas();
        let (c, errs) = parse(
            "[box.repo]\ntitle = \"Repository\"\ntitle_justify = \"center\"\ntitle_pad = 2\ntitle_color = \"accent\"\nstyle = \"double\"\ncolor = \"#ff0000\"\n[[row]]\nbox = \"repo\"\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert_eq!(errs, Vec::new());
        let b = &c.boxes["repo"];
        let title = b.title.as_ref().expect("the box has a title");
        assert_eq!(title.text, "Repository");
        assert_eq!(title.justify, Justify::Center);
        assert_eq!(title.pad, 2);
        assert_eq!(title.color, Some(c.theme.role(Role::Accent)), "a role name resolves");
        assert_eq!(b.style, Some(FrameStyle::Double));
        assert!(!b.fill, "a box interior is clean unless it asks for the rule");
        assert_eq!(b.color, Color::parse("#ff0000"));

        // Powerline has caps, not a box shape: reported and drawn rounded.
        let (c, errs) = parse(
            "[box.a]\nstyle = \"powerline\"\n[[row]]\nbox = \"a\"\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "box.a.style");
        assert_eq!(c.boxes["a"].style, Some(FrameStyle::Rounded));
    }

    /// SPEC § 4.3: a box is one run of adjacent rows, wherever those rows
    /// are. A run inside a stack is a run like any other, and a name that
    /// comes back anywhere else in the tree is reported and left unboxed.
    #[test]
    fn a_box_is_one_run_of_adjacent_rows_anywhere_in_the_tree() {
        let schemas = schemas();
        let inner = |body: &str| format!("[box.a]\ntitle = \"A\"\n[[row]]\n[[row.col]]\n{body}");
        let stacked = |boxed: &str, module: &str| {
            format!("[[row.col.row]]\n{boxed}modules = [\"{module}\"]\n")
        };

        // Two adjacent inner rows naming the same box are one run.
        let body =
            format!("{}{}", stacked("box = \"a\"\n", "path"), stacked("box = \"a\"\n", "clock"));
        let (c, errs) = parse(&inner(&body), &schemas);
        assert_eq!(errs, Vec::new());
        let rows = &c.rows[0].cols[0].rows;
        assert!(rows.iter().all(|r| r.boxed.is_some()), "both rows keep the box");

        // A bare row between them makes the second a new run of a name
        // already drawn, which is reported and unboxed.
        let body = format!(
            "{}{}{}",
            stacked("box = \"a\"\n", "path"),
            stacked("", "path"),
            stacked("box = \"a\"\n", "clock")
        );
        let (c, errs) = parse(&inner(&body), &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[0].col[0].row[2].box");
        let rows = &c.rows[0].cols[0].rows;
        assert!(rows[0].boxed.is_some() && rows[2].boxed.is_none());

        // The run spans the whole tree: a name drawn at the top level and
        // again inside a stack is two boxes with one name.
        let text = format!(
            "[box.a]\ntitle = \"A\"\n[[row]]\nbox = \"a\"\nmodules = [\"path\"]\n[[row]]\n[[row.col]]\n{}",
            stacked("box = \"a\"\n", "clock")
        );
        let (c, errs) = parse(&text, &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[1].col[0].row[0].box");
        assert!(c.rows[0].boxed.is_some(), "the first run keeps it");
        assert!(c.rows[1].cols[0].rows[0].boxed.is_none());

        // A column's own box takes the name too, so a row cannot take it back.
        let text = "[box.a]\ntitle = \"A\"\n[[row]]\n[[row.col]]\nbox = \"a\"\n\
                    [[row.col.row]]\nmodules = [\"path\"]\n[[row]]\nbox = \"a\"\nmodules = [\"clock\"]\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[1].box");
        assert!(c.rows[0].cols[0].boxed.is_some() && c.rows[1].boxed.is_none());
    }

    /// `[[line]]` and `hide_empty_lines` are permanent aliases (SPEC § 4.3):
    /// every config written before rows existed must resolve to exactly the
    /// same thing, and a file that carries both array names is reported
    /// rather than silently ordered.
    #[test]
    fn the_line_aliases_resolve_to_the_same_config_and_both_arrays_are_reported() {
        let schemas = schemas();
        let body = |key: &str, hide: &str| {
            format!(
                "{hide} = false\n[[{key}]]\nmodules = [\"path\"]\nright = [\"clock\"]\nseparator = \" | \"\n[[{key}]]\nmodules = []\nblank = true\n"
            )
        };
        let (rows, row_errs) = parse(&body("row", "hide_empty_rows"), &schemas);
        let (lines, line_errs) = parse(&body("line", "hide_empty_lines"), &schemas);
        assert_eq!(row_errs, Vec::new());
        assert_eq!(line_errs, Vec::new());
        assert_eq!(rows.rows, lines.rows, "the alias resolves to the same rows");
        assert!(!rows.hide_empty_rows && !lines.hide_empty_rows);

        // Two arrays of tables have no order between them, so a file uses one
        // name: the alias is dropped and said so, never interleaved.
        let (both, errs) =
            parse("[[row]]\nmodules = [\"path\"]\n[[line]]\nmodules = [\"clock\"]\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "line");
        assert!(errs[0].message.contains("not both"), "{}", errs[0].message);
        assert_eq!(both.rows.len(), 1);
        assert_eq!(both.rows[0].cols[0].left, ["path"], "the [[row]] entries win");

        // The new name wins whichever order the file writes the two switches
        // in, and an error under a row points at the array the file used.
        let (c, errs) = parse("hide_empty_lines = true\nhide_empty_rows = false\n", &schemas);
        assert_eq!(errs, Vec::new());
        assert!(!c.hide_empty_rows);
        let (_, errs) = parse("[[row]]\nmodules = [\"nope\"]\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[0].modules[0]");
    }

    #[test]
    fn stale_after_defaults_to_five_and_rejects_zero() {
        let (cfg, errors) = parse("", &schemas());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(cfg.stale_after, 5);
        let (cfg, errors) = parse("stale_after = 2\n", &schemas());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(cfg.stale_after, 2);
        let (cfg, errors) = parse("stale_after = 0\n", &schemas());
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors.first().map(|e| e.path.as_str()), Some("stale_after"));
        assert!(cfg.stale_after >= 1, "never zero, whatever the fallback");
    }

    #[test]
    fn full_config_resolves() {
        let text = r##"
preset = "minimal"
icons = "ascii"
theme = "nord"
color = "256"
padding = 2
[colors]
accent = "#010203"
[frame]
style = "double"
fill = false
separator = " | "
[[line]]
modules = ["path"]
right = ["clock"]
separator = "  "
[modules.path]
preset = "full"
depth = 3
refresh = 9
label = "in"
[modules.path.icons]
folder = ">"
[modules.path.colors]
dir = "danger"
[modules.clock]
format = "12h"
"##;
        let (c, errs) = parse(text, &schemas());
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(c.icons, IconSet::Ascii);
        assert_eq!(c.theme.role(Role::Accent), Color::Rgb(1, 2, 3));
        assert_eq!(c.color, ColorChoice::Ansi256);
        assert_eq!(c.frame.style, FrameStyle::Double);
        assert!(!c.frame.fill);
        assert_eq!(c.frame.chars.separator, " | ");
        assert_eq!(c.rows.len(), 1);
        assert_eq!(c.separator(&c.rows[0]), "  ");
        let path = c.modules.get("path").unwrap();
        assert_eq!(path.preset, Preset::Full);
        assert_eq!(path.int("depth"), 3);
        assert_eq!(path.refresh, 9);
        assert_eq!(path.label, "in");
        assert_eq!(path.icon("folder"), ">");
        assert_eq!(path.color("dir"), c.theme.role(Role::Danger));
        assert_eq!(c.modules.get("clock").unwrap().str("format"), "12h");
        assert_eq!(c.width(Some(100)), 94);
    }

    /// Every `*_step` key takes the same range, the reference prints the
    /// bound the parser enforces, and both ends are rejected. The reference
    /// used to say "must be > 0", so `ticker_step = 0.0001` was documented
    /// as valid and reported as a mistake on every tick.
    #[test]
    fn every_step_key_shares_one_range_and_the_reference_prints_it() {
        assert_eq!(STEP_BOUNDS, format!("{}–{}", STEP_RANGE.start(), STEP_RANGE.end()));
        assert!(STEP_MESSAGE.contains(&STEP_RANGE.start().to_string()));
        assert!(STEP_MESSAGE.contains(&STEP_RANGE.end().to_string()));
        let reference = crate::docs::config_page();
        for key in ["ticker_step", "fill_step", "separator_step"] {
            let row = reference
                .lines()
                .find(|l| l.starts_with(&format!("| `{key}` ")))
                .unwrap_or_else(|| panic!("{key} has no row in the reference"));
            assert!(row.contains(STEP_BOUNDS), "{row}");
        }
        assert!(
            crate::modules::text::SCHEMA.opt("step").unwrap().doc.contains(STEP_BOUNDS),
            "text.step"
        );
        let below = format!("ticker_step = {}", STEP_RANGE.start() / 10.0);
        let above = format!("fill_step = {}", STEP_RANGE.end() * 10.0);
        for (text, path) in
            [(below, "ticker_step"), (format!("[frame]\n{above}"), "frame.fill_step")]
        {
            let (cfg, errs) = parse(&text, &schemas());
            assert_eq!(errs.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), [path], "{text}");
            assert!((cfg.ticker_step - 1.0).abs() < f64::EPSILON);
            assert!((cfg.frame.fill_step - 1.0).abs() < f64::EPSILON);
        }
        // Both ends themselves are in range.
        let ends = format!("ticker_step = {}\n[frame]\nfill_step = {}", 0.001, 1000);
        assert_eq!(parse(&ends, &schemas()).1, Vec::new());
    }

    /// SPEC § 5: every bad key is reported under its TOML path and falls back
    /// to its default on its own; everything valid around it stays in effect
    /// (walkthrough bug 8: one bad colour used to discard the whole file).
    #[test]
    fn errors_have_paths_and_only_the_bad_keys_fall_back() {
        let text = r#"
theme = "solarized"
durations = "loose"
padding = 70000
mystery = 1
[colors]
accent = "bogus"
nope = "red"
[frame]
style = "heavy"
separator = " ┃ "
bogus_key = 1
[[line]]
modules = ["path", "ghost"]
right = 3
[modules.path]
depth = -1
wat = 1
[modules.path.icons]
nope = "x"
[modules.clock]
format = "13h"
[modules.ghost]
x = 1
"#;
        let (c, errs) = parse(text, &schemas());
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        for expected in [
            "theme",
            "durations",
            "padding",
            "mystery",
            "colors.accent",
            "colors.nope",
            "frame.bogus_key",
            "line[0].modules[1]",
            "line[0].right",
            "modules.path.depth",
            "modules.path.wat",
            "modules.path.icons.nope",
            "modules.clock.format",
            "modules.ghost",
        ] {
            assert!(paths.contains(&expected), "{expected} missing from {paths:?}");
        }
        assert!(errs.iter().all(|e| e.line.is_none()), "value errors carry a path, not a line");
        // The valid keys are in effect…
        assert_eq!(c.frame.style, FrameStyle::Heavy);
        assert_eq!(c.frame.chars.separator, " ┃ ");
        assert_eq!(c.rows.len(), 1);
        assert_eq!(c.rows[0].cols[0].left, vec!["path"], "the unknown id is reported and removed");
        assert_eq!(c.modules.get("clock").map(|m| m.str("format")), Some("24h"));
        // …and each bad one fell back to its own default.
        let defaults = Config::defaults(&schemas());
        assert_eq!(c.theme, defaults.theme, "unknown theme and bad colour → default palette");
        assert_eq!(c.durations, DurationStyle::Compact);
        assert_eq!(c.padding, 0);
        assert_eq!(c.rows[0].cols[0].right, Vec::<String>::new(), "bad right list → no group");
        assert_eq!(c.modules.get("path").map(|m| m.int("depth")), Some(2));
        let (_, errs) = parse("[modules.path]\nrefresh = 0\n", &schemas());
        assert!(errs.is_empty(), "payload-only modules may run every tick: {errs:?}");
        let mut cached = schemas();
        cached[0].refresh = 5;
        let (_, errs) = parse("[modules.path]\nrefresh = 0\n", &cached);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "modules.path.refresh");
    }

    #[test]
    fn a_bad_list_item_or_table_shape_reports_itself_and_keeps_the_rest() {
        let (c, errs) = parse("[[line]]\nmodules = [\"clock\", 3]\nright = \"x\"\n", &schemas());
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["line[0].modules[1]", "line[0].right"]);
        assert_eq!(c.rows[0].cols[0].left, vec!["clock"], "the good item stays");
        for (text, path) in [
            ("line = \"x\"", "line"),
            ("modules = 1", "modules"),
            ("colors = 1", "colors"),
            ("frame = \"x\"", "frame"),
            ("[[frame]]\nstyle = \"heavy\"", "frame"),
            ("[modules]\nclock = 1", "modules.clock"),
        ] {
            let (c, errs) = parse(text, &schemas());
            assert_eq!(errs.len(), 1, "{text}: {errs:?}");
            assert_eq!(errs[0].path, path, "{text}");
            assert_eq!(c.rows.len(), 4, "{text}: the default lines stand in");
        }
        // An inline `line = [...]`, which nothing else in the suite used.
        // A non-table item keeps its place as a placeholder rather than being
        // dropped: dropping it renumbered every later line, so the error
        // named a `line[n]` that was not the one in the user's file.
        let (c, errs) = parse("line = [1, { modules = [\"clock\", 3] }]", &schemas());
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["line[0]", "line[1].modules[1]"], "the second line is line[1]");
        // The placeholder holds the index and nothing else, so the default
        // `hide_empty_lines` drops it at render and the row it stands for
        // does not become a blank line.
        assert_eq!(c.rows.len(), 2);
        assert!(c.rows[0].cols[0].is_empty() && !c.rows[0].spacer);
        assert_eq!(c.rows[1].cols[0].left, vec!["clock"], "the good item stays");
    }

    #[test]
    fn enum_keys_name_their_choices() {
        let text = "color = 1\npreset = { a = 1 }\ndurations = \"loose\"\n[frame]\nstyle = 7\n";
        let (_, errs) = parse(text, &schemas());
        let msgs: Vec<String> = errs.iter().map(|e| format!("{}: {}", e.path, e.message)).collect();
        for expected in [
            "color: expected a string, one of auto, always, never, 256, truecolor",
            "preset: expected a string, one of default, minimal, full, compact",
            "durations: unknown value \"loose\"; expected one of compact, fixed",
            "frame.style: expected a string, one of none, rounded, square, double, heavy, powerline, custom",
        ] {
            assert!(msgs.iter().any(|m| m == expected), "{expected}\n{msgs:?}");
        }
    }

    /// Every key the walk lists as valid is accepted (guards the key tables
    /// against drifting from the match arms).
    #[test]
    fn every_listed_key_is_accepted() {
        let text = "preset = \"compact\"\nicons = \"ascii\"\ntheme = \"nord\"\ncolor = \"never\"\ntruncate = false\nstale_style = \"hide\"\nstale_after = 3\npadding = 2\nalign = true\nright_justify = \"start\"\nhide_empty_lines = false\noverflow = \"ticker\"\nticker_step = 0.5\nticker_gap = \" ~ \"\nanimate = false\ndurations = \"fixed\"\n[colors]\naccent = \"red\"\n[frame]\nstyle = \"custom\"\nfill = true\nfirst = \"a\"\nmiddle = \"b\"\nlast = \"c\"\nsingle = \"d\"\nfill_char = \"-\"\nright_first = \"e\"\nright_middle = \"f\"\nright_last = \"g\"\nright_single = \"h\"\npad = \" \"\nseparator = \" | \"\nfill_pattern = \"-=\"\nfill_step = 2\nfill_direction = \"left\"\nseparator_frames = [\" | \", \" : \"]\nseparator_step = 0.5\n[[line]]\nmodules = [\"path\"]\nright = [\"clock\"]\nseparator = \"  \"\n[modules.path]\ndepth = 1\n";
        let (_, errs) = parse(text, &schemas());
        assert_eq!(errs, Vec::new());
    }

    /// SPEC § 4.2: whether the file sets `animate` is knowable, since an
    /// unset key defers to Claude Code's `prefersReducedMotion` setting.
    #[test]
    fn animate_records_whether_the_file_set_it() {
        assert_eq!(parse("", &schemas()).0.animate, None);
        assert_eq!(parse("animate = false", &schemas()).0.animate, Some(false));
        assert_eq!(parse("animate = true", &schemas()).0.animate, Some(true));
        let (c, errs) = parse("animate = \"yes\"", &schemas());
        assert_eq!(c.animate, None, "a bad value is reported and left unset");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs.first().map(|e| e.path.as_str()), Some("animate"));
    }

    /// SPEC § 3.7: `[modules.text.<name>]` tables are validated against the
    /// text schema under their own path; `text.<name>` is a valid line id
    /// only with a table; `refresh`, `preset` and a non-positive `step` are
    /// rejected; a non-table entry is reported.
    #[test]
    fn text_modules_are_defined_by_the_config() {
        let schemas = schemas();
        let text = "[[line]]\nmodules = [\"path\", \"text.motd\"]\nright = [\"text.tag\"]\n[modules.text.motd]\ntext = \"hello\"\nwidth = 12\noverflow = \"scroll-wrap\"\nstep = 0.5\n[modules.text.tag]\ntext = \"v0.2\"\ncolor = \"muted\"\njustify = \"right\"\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.texts.len(), 2);
        let motd = c.texts.get("motd").unwrap();
        assert_eq!(motd.str("text"), "hello");
        assert_eq!(motd.size("width"), 12);
        assert_eq!(motd.str("overflow"), "scroll-wrap");
        assert!((motd.float("step") - 0.5).abs() < f64::EPSILON);
        let tag = c.texts.get("tag").unwrap();
        assert_eq!(tag.str("justify"), "right");
        assert_eq!(tag.color("text"), c.theme.role(Role::Muted), "`color` shorthand");
        assert!(!c.modules.contains_key("text"), "the family is not a registry module");

        let bad = "[[line]]\nmodules = [\"text.ghost\"]\n[modules.text.motd]\ntext = \"x\"\nrefresh = 5\npreset = \"bogus\"\nstep = 0\njustify = \"middle\"\ncolor = \"bogus\"\nwat = 1\n[modules.text.motd.icons]\nfoo = \"x\"\n[modules.text.\"sp ace\"]\ntext = \"y\"\n[modules.text]\nplain = 3\n";
        let (c, errs) = parse(bad, &schemas);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        for expected in [
            "line[0].modules[0]",
            "modules.text.motd.refresh",
            "modules.text.motd.preset",
            "modules.text.motd.step",
            "modules.text.motd.justify",
            "modules.text.motd.color",
            "modules.text.motd.wat",
            "modules.text.motd.icons",
            "modules.text.sp ace",
            "modules.text.plain",
        ] {
            assert!(paths.contains(&expected), "{expected} missing from {paths:?}");
        }
        // One error per bad key, not one from the generic walk plus one from the family.
        assert_eq!(paths.iter().filter(|p| **p == "modules.text.motd.preset").count(), 1);
        assert_eq!(paths.iter().filter(|p| **p == "modules.text.motd.refresh").count(), 1);
        assert!(!c.texts.contains_key("sp ace"), "a non-bare name is rejected");
        assert!(errs.iter().any(|e| e.message.contains("define [modules.text.ghost]")), "{errs:?}");
        let motd = c.texts.get("motd").unwrap();
        assert!((motd.float("step") - 1.0).abs() < f64::EPSILON, "bad step → default");
        assert_eq!(motd.str("justify"), "left", "bad justify → default");
        assert_eq!(motd.refresh, 0);
        assert_eq!(c.rows[0].cols[0].left, Vec::<String>::new(), "the unknown id is removed");

        // An explicit colors.text wins over the shorthand; text and gap are plain.
        let text = "[modules.text.x]\ntext = \"\\u001b[31mred\\u001b[0m\\tnote\"\ngap = \" \\u001b[5m·\\u001b[0m \"\ncolor = \"muted\"\n[modules.text.x.colors]\ntext = \"red\"\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        let x = c.texts.get("x").unwrap();
        assert_eq!(x.color("text"), Color::Ansi(1), "explicit colors.text wins");
        assert_eq!(x.str("text"), "rednote");
        assert_eq!(x.str("gap"), " · ");

        // `url` (SPEC § 3.7) must meet the painter's rule, else it is
        // reported and dropped; an empty one is no link at all.
        let text = "[modules.text.a]\ntext = \"a\"\nurl = \"https://example.com/x?y=1\"\n[modules.text.b]\ntext = \"b\"\nurl = \"ftp://example.com\"\n[modules.text.c]\ntext = \"c\"\nurl = \"https://ex ample.com\"\n[modules.text.d]\ntext = \"d\"\nurl = \"\"\n";
        let (c, errs) = parse(text, &schemas);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["modules.text.b.url", "modules.text.c.url"], "{errs:?}");
        assert!(errs[0].message.contains("http:// or https://"), "{errs:?}");
        assert_eq!(c.texts.get("a").unwrap().str("url"), "https://example.com/x?y=1");
        assert_eq!(c.texts.get("b").unwrap().str("url"), "", "dropped");
        assert_eq!(c.texts.get("c").unwrap().str("url"), "", "dropped");
        assert_eq!(c.texts.get("d").unwrap().str("url"), "");
    }

    /// SPEC § 4.2: a rule pattern is one-cell glyphs, separator frames share
    /// one width; bad values are reported and the static frame stays.
    /// SPEC § 4.2 Animated glyphs: `<key>_frames` on any icon key, equal
    /// widths enforced, plain text, unknown base keys and bad shapes reported.
    #[test]
    fn icon_frames_parse_and_validate() {
        let schemas = schemas();
        let text =
            "[modules.path.icons]\nfolder_frames = [\"a\", \"\\u001b[1mb\\u001b[0m\", \"c\"]\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.modules.get("path").unwrap().icon_frames("folder"), ["a", "b", "c"]);
        for (bad, path) in [
            (
                "[modules.path.icons]\nfolder_frames = [\"a\", \"🌿\"]\n",
                "modules.path.icons.folder_frames",
            ),
            ("[modules.path.icons]\nfolder_frames = []\n", "modules.path.icons.folder_frames"),
            ("[modules.path.icons]\nfolder_frames = \"abc\"\n", "modules.path.icons.folder_frames"),
            ("[modules.path.icons]\nghost_frames = [\"a\"]\n", "modules.path.icons.ghost_frames"),
        ] {
            let (c, errs) = parse(bad, &schemas);
            assert_eq!(errs.len(), 1, "{bad}: {errs:?}");
            assert_eq!(errs[0].path, path, "{bad}");
            assert!(c.modules.get("path").unwrap().icon_frames("folder").is_empty(), "{bad}");
        }
    }

    /// A `custom` frame carries the five box glyphs too (SPEC § 4.3), each
    /// one cell like the caps, and a built-in style brings its own.
    #[test]
    fn a_custom_frame_takes_the_five_box_glyphs() {
        let schemas = schemas();
        let keys = ["top_left", "top_right", "bottom_left", "bottom_right", "side"];
        let mut body = String::new();
        for key in keys {
            body.push_str(key);
            body.push_str(" = \"+\"\n");
        }
        let (c, errs) = parse(&format!("[frame]\nstyle = \"custom\"\n{body}"), &schemas);
        assert_eq!(errs, Vec::new());
        let ch = &c.frame.chars;
        assert_eq!(
            [&ch.top_left, &ch.top_right, &ch.bottom_left, &ch.bottom_right, &ch.side],
            [&"+".to_owned(); 5]
        );
        // A glyph wider than one cell would push every box line out of line.
        for key in keys {
            let (c, errs) =
                parse(&format!("[frame]\nstyle = \"custom\"\n{key} = \"ab\"\n"), &schemas);
            assert_eq!(errs.len(), 1, "{key}: {errs:?}");
            assert_eq!(errs[0].path, format!("frame.{key}"));
            assert!(errs[0].message.contains("one cell"), "{}", errs[0].message);
            assert!(c.frame.chars.side.is_empty(), "{key}: the style's own glyph stays");
        }
        // The built-in shapes come with their own; `none` has none, which is
        // what makes its box invisible.
        let (c, _) = parse("[frame]\nstyle = \"double\"\n", &schemas);
        assert_eq!(c.frame.chars.side, "║");
        let (c, _) = parse("[frame]\nstyle = \"none\"\n", &schemas);
        assert_eq!(c.frame.chars.side, "", "an invisible box has no side glyph");
    }

    #[test]
    fn frame_animation_keys_parse_and_validate() {
        let schemas = schemas();
        let (c, errs) = parse("", &schemas);
        assert_eq!(errs, Vec::new());
        assert!(c.frame.fill_pattern.is_empty() && c.frame.separator_frames.is_empty());
        assert_eq!(c.frame.fill_direction, FillDirection::Right);
        let text = "[frame]\nfill_pattern = \"·  \"\nfill_step = 0.5\nfill_direction = \"left\"\nseparator_frames = [\" │ \", \" ┃ \", \" ╎ \"]\nseparator_step = 2\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.fill_pattern, vec!["·", " ", " "]);
        assert!((c.frame.fill_step - 0.5).abs() < f64::EPSILON);
        assert_eq!(c.frame.fill_direction, FillDirection::Left);
        assert_eq!(c.frame.separator_frames, vec![" │ ", " ┃ ", " ╎ "]);
        assert_eq!(c.separator_at(&c.rows[0], 1), " ┃ ");
        assert_eq!(c.separator_at(&c.rows[0], 7), " │ ", "out of range → static");
        let line = RowCfg { separator: Some("--".into()), ..c.rows[0].clone() };
        assert_eq!(c.separator_at(&line, 1), "--", "a per-line separator wins");
        // A two-cell glyph in the pattern, frames of unequal width, a bad
        // direction and a zero step: each reported under its path, each
        // falling back to the static frame.
        let bad = "[frame]\nfill_pattern = \"·🌿\"\nseparator_frames = [\" │ \", \"│\"]\nfill_direction = \"up\"\nfill_step = 0\n";
        let (c, errs) = parse(bad, &schemas);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "frame.fill_direction",
                "frame.fill_pattern",
                "frame.separator_frames",
                "frame.fill_step"
            ],
            "{errs:?}"
        );
        assert!(c.frame.fill_pattern.is_empty() && c.frame.separator_frames.is_empty());
        assert!((c.frame.fill_step - 1.0).abs() < f64::EPSILON);
        // Escapes in a frame never reach the row.
        let (c, errs) =
            parse("[frame]\nseparator_frames = [\"\\u001b[1m|\\u001b[0m\", \":\"]\n", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_frames, vec!["|", ":"]);
        // A pattern without a rule to paint is a dead key: said, not ignored.
        let (c, errs) = parse("[frame]\nfill = false\nfill_pattern = \"·  \"\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "frame.fill_pattern");
        assert_eq!(c.frame.fill_pattern, Vec::<String>::new());
    }

    /// `separator_frames = []` is the line every `garnish config init` has
    /// written, and it means "no animation". Rejecting it would put a
    /// `⚠ config:` row on every tick of every config already on disk, so the
    /// empty list is legal there and reported for an icon, which nothing
    /// generates.
    #[test]
    fn an_empty_frame_list_is_legal_for_the_separator_and_not_for_an_icon() {
        // The real schema set: `config show` writes every module, so a round
        // trip through a reduced one would fail on the modules it omits.
        let all = &crate::modules::SCHEMAS;
        let (c, errs) = parse("[frame]\nseparator_frames = []\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_frames, Vec::<String>::new());
        // What `config init` writes still parses clean after a round trip.
        let shown = crate::docs::config_toml(&c, false);
        assert!(shown.contains("separator_frames = []"), "{shown}");
        assert_eq!(parse(&shown, all).1, Vec::new());

        let (_, errs) = parse("[modules.model.icons]\nmodel_frames = []\n", all);
        let problems: Vec<(&str, &str)> =
            errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
        assert_eq!(problems, [("modules.model.icons.model_frames", "expected at least one frame")]);
    }

    #[test]
    fn a_ticker_defaults_durations_to_fixed_unless_set() {
        // SPEC § 4.1: a timer changing width inside the scrolled group makes
        // the window jump, so the smooth style is the default under a ticker
        // and compact an explicit opt-in; every timer module can pin itself.
        let schemas = schemas();
        let (c, errs) = parse("overflow = \"ticker\"", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.durations, DurationStyle::Fixed);
        let (c, errs) = parse("overflow = \"ticker\"\ndurations = \"compact\"", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.durations, DurationStyle::Compact, "an explicit value wins");
        let (c, errs) = parse("overflow = \"truncate\"", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.durations, DurationStyle::Compact, "no ticker, no switch");
        let (c, errs) = parse("overflow = \"ticker\"\ndurations = \"loose\"", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(c.durations, DurationStyle::Fixed, "a bad value falls back to the implied one");
        let all = &crate::modules::SCHEMAS;
        let (c, errs) = parse(
            "overflow = \"ticker\"\n[modules.api]\ndurations = \"compact\"\n[modules.sync]\ndurations = \"fixed\"\n[modules.clock]\ndurations = \"fixed\"",
            all,
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs.first().is_some_and(|e| e.path == "modules.clock.durations"), "{errs:?}");
        assert_eq!(c.modules.get("api").map(|m| m.str("durations")), Some("compact"));
        assert_eq!(c.modules.get("sync").map(|m| m.str("durations")), Some("fixed"));
        assert_eq!(c.modules.get("session").map(|m| m.str("durations")), Some("inherit"));
        let (_, errs) = parse("[modules.session]\ndurations = \"loose\"", all);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs.first().is_some_and(|e| e.message.contains("\"inherit\", \"compact\", \"fixed\"")),
            "{errs:?}"
        );
    }

    #[test]
    fn ticker_keys_parse_and_a_bad_step_falls_back() {
        let schemas = schemas();
        let (c, errs) = parse("", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.overflow, Overflow::Truncate);
        assert!((c.ticker_step - 1.0).abs() < f64::EPSILON);
        assert_eq!(c.ticker_gap, DEFAULT_TICKER_GAP);
        let (c, errs) =
            parse("overflow = \"ticker\"\nticker_step = 2\nticker_gap = \" · \"", &schemas);
        assert_eq!(errs, Vec::new(), "an integer step is a number too");
        assert_eq!(c.overflow, Overflow::Ticker);
        assert!((c.ticker_step - 2.0).abs() < f64::EPSILON);
        assert_eq!(c.ticker_gap, " · ");
        let (c, errs) = parse("ticker_gap = \"\\u001b[31m G \\u001b[0m\\n\"", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.ticker_gap, " G ", "escapes and controls never reach the row");
        // Zero, negative, non-numeric, and the two silent freezes: a step so
        // small nothing moves in a lifetime, and one so large `now × step`
        // saturates to a constant frame (whole-stack review).
        for bad in [
            "ticker_step = 0",
            "ticker_step = -0.5",
            "ticker_step = \"fast\"",
            "ticker_step = 1e-300",
            "ticker_step = 1e308",
            "ticker_step = 1001",
        ] {
            let (c, errs) = parse(bad, &schemas);
            assert_eq!(errs.len(), 1, "{bad}: {errs:?}");
            assert_eq!(errs[0].path, "ticker_step", "{bad}");
            if !bad.contains('"') {
                assert!(errs[0].message.contains("between 0.001 and 1000"), "{bad}: {errs:?}");
            }
            assert!((c.ticker_step - 1.0).abs() < f64::EPSILON, "{bad}: back to 1");
        }
        for ok in ["ticker_step = 0.001", "ticker_step = 1000"] {
            let (_, errs) = parse(ok, &schemas);
            assert_eq!(errs, Vec::new(), "{ok}");
        }
        let (c, errs) = parse(
            &format!("ticker_gap = \"{}\"", "x".repeat(crate::config::MAX_TEXT_CHARS + 1)),
            &schemas,
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("at most 4096 characters"), "{errs:?}");
        assert_eq!(c.ticker_gap, DEFAULT_TICKER_GAP);
        let (_, errs) = parse("overflow = \"marquee\"", &schemas);
        assert!(errs[0].message.ends_with("expected one of truncate, ticker"), "{errs:?}");
    }

    #[test]
    fn preset_overlay_drops_line_errors_with_the_lines() {
        let overlay = Overlay { preset: Some(TopPreset::Minimal), ..Default::default() };
        let (c, errs) = parse_with("[[line]]\nmodules = [3]\n", &schemas(), &overlay);
        assert_eq!(errs, Vec::new(), "the overlay replaces the lines, so their problems are moot");
        assert_eq!(c.rows.len(), 1);
    }

    #[test]
    fn every_config_string_that_reaches_a_row_is_plain_text() {
        // Whole-stack review: escapes in a label, prefix, suffix, icon, frame
        // glyph or per-line separator inflated the width arithmetic (the
        // printable bytes of `ESC[31m` counted as cells) and a cut could
        // land inside the sequence. All of them are reduced at parse time.
        let text = concat!(
            "[frame]\nstyle = \"custom\"\nfirst = \"\\u001b]0;title\\u0007<\"\npad = \"\\u001b[1m \"\n",
            "separator = \" \\u001b[2m|\\u001b[0m \"\nright_last = \">\\u001b[H\"\n",
            "[[line]]\nmodules = [\"model\"]\nseparator = \"\\u001bP dcs \\u001b\\\\+\"\n",
            "[modules.model]\nlabel = \"\\u001b[31mM\\u001b[0m\"\nprefix = \"\\u001b]52;c;aGk=\\u0007(\"\n",
            "suffix = \")\\u200e\\u202e\"\n[modules.model.icons]\nmodel = \"\\u001b[32m*\\u001b[0m\"\n",
        );
        let (c, errs) = parse(text, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.chars.first, "<");
        assert_eq!(c.frame.chars.pad, " ");
        assert_eq!(c.frame.chars.separator, " | ");
        assert_eq!(c.frame.chars.right_last, ">");
        assert_eq!(c.rows[0].separator.as_deref(), Some("+"), "a DCS loses its payload too");
        let model = c.modules.get("model").unwrap();
        assert_eq!(model.label, "M");
        assert_eq!(model.prefix, "(");
        assert_eq!(model.suffix, ")", "bidi marks are dropped too");
        assert_eq!(model.icon("model"), "*");
        // Nothing in the resolved config carries a control character.
        let shown = crate::docs::config_toml(&c, false);
        assert!(!shown.contains('\u{1b}') && !shown.contains('\u{7}'), "{shown}");
    }

    #[test]
    fn sizes_and_text_lengths_are_bounded_at_config_time() {
        // A cell count or a string a row can never show would only size an
        // allocation or a loop on every tick; it is reported and defaulted.
        let big = "[modules.context]\nwidth = 99999999999\n[modules.text.a]\ntext = \"hi\"\nwidth = 1025\npad = 4000000000\n[modules.limit5h]\nbar_width = 1025\n";
        let (c, errs) = parse(big, &crate::modules::SCHEMAS);
        // The modules come in schema order and a table's keys in the order
        // the file wrote them (the table keeps its order, so `setup` can
        // write it back as read).
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "modules.context.width",
                "modules.limit5h.bar_width",
                "modules.text.a.width",
                "modules.text.a.pad"
            ],
            "{errs:?}"
        );
        assert!(errs.iter().all(|e| e.message == "must be at most 1024"), "{errs:?}");
        assert_eq!(c.modules.get("context").unwrap().size("width"), 20, "default stands in");
        let a = c.texts.get("a").unwrap();
        assert_eq!((a.size("width"), a.size("pad")), (0, 0));
        // `decimals` sizes the money formatter's buffer: capped the same way.
        let (c, errs) = parse("[modules.cost]\ndecimals = 4000000000\n", &crate::modules::SCHEMAS);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(
            (errs[0].path.as_str(), errs[0].message.as_str()),
            ("modules.cost.decimals", "must be at most 8")
        );
        assert_eq!(c.modules.get("cost").unwrap().size("decimals"), 2);
        // Every cap lives in the schema, so the reference can print it.
        let capped: std::collections::BTreeSet<(&str, &str, usize)> = crate::modules::SCHEMAS
            .iter()
            .chain(std::iter::once(&*crate::modules::text::SCHEMA))
            .flat_map(|s| s.opts.iter().filter_map(|o| Some((s.id, o.key, o.max?))))
            .collect();
        assert_eq!(
            capped,
            [
                ("context", "width", MAX_CELLS),
                ("cost", "decimals", MAX_DECIMALS),
                ("limit5h", "bar_width", MAX_CELLS),
                ("limit7d", "bar_width", MAX_CELLS),
                ("spend", "bar_width", MAX_CELLS),
                ("text", "gap", MAX_TEXT_CHARS),
                ("text", "pad", MAX_CELLS),
                ("text", "text", MAX_TEXT_CHARS),
                ("text", "url", MAX_TEXT_CHARS),
                ("text", "width", MAX_CELLS),
            ]
            .into_iter()
            .collect()
        );
        let (ok, errs) = parse("[modules.context]\nwidth = 1024\n", &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(ok.modules.get("context").unwrap().size("width"), 1024);
        let long = format!(
            "[modules.text.a]\ntext = \"{}\"\ngap = \"{}\"\n",
            "x".repeat(MAX_TEXT_CHARS + 1),
            "y".repeat(MAX_TEXT_CHARS)
        );
        let (c, errs) = parse(&long, &crate::modules::SCHEMAS);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "modules.text.a.text");
        assert!(errs[0].message.contains("at most 4096 characters"), "{errs:?}");
        assert_eq!(c.texts.get("a").unwrap().str("gap").chars().count(), MAX_TEXT_CHARS);
        // The common row strings are bounded the same way.
        let long = format!(
            "[modules.model]\nlabel = \"{}\"\nprefix = \"{}\"\nsuffix = \"ok\"\n",
            "x".repeat(MAX_TEXT_CHARS + 1),
            "y".repeat(MAX_TEXT_CHARS)
        );
        let (c, errs) = parse(&long, &crate::modules::SCHEMAS);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["modules.model.label"], "{errs:?}");
        let model = c.modules.get("model").unwrap();
        assert_eq!((model.label.as_str(), model.prefix.chars().count()), ("", MAX_TEXT_CHARS));
        // The common options carry their caps as specs, like any option.
        let common: Vec<(&str, Option<usize>)> =
            COMMON_OPTS.iter().map(|o| (o.key, o.max)).collect();
        assert_eq!(
            common,
            [
                ("label", Some(MAX_TEXT_CHARS)),
                ("prefix", Some(MAX_TEXT_CHARS)),
                ("suffix", Some(MAX_TEXT_CHARS)),
                ("hide_when_empty", None),
                ("max_width", Some(MAX_CELLS)),
            ]
        );
    }

    /// `parse_overrides` looks a key up in `COMMON_OPTS` before the module's
    /// own schema, so a schema that redeclared a common key would be silently
    /// shadowed: its option would parse against the common spec and
    /// `cfg.str("label")` would read the schema default for ever. The
    /// comment there states the invariant; this is what enforces it. The
    /// other half of the same trap is a `COMMON_OPTS` entry with no
    /// `set_common` arm, which would be accepted and dropped into `ov.opts`.
    #[test]
    fn no_schema_redeclares_a_common_key_and_every_common_key_is_stored() {
        // Every key `parse_overrides` handles before the schema, not just
        // the `COMMON_OPTS` five: `enabled`, `preset`, `refresh`, `icons`
        // and `colors` are matched by name too, so a schema option with one
        // of those keys would be parsed by the hand-written arm and its
        // `cfg.bool(..)`/`cfg.int(..)` reader would see the default for ever.
        let common: Vec<&str> = common_keys().chain(std::iter::once("colors")).collect();
        let schemas =
            crate::modules::SCHEMAS.iter().chain(std::iter::once(&*crate::modules::text::SCHEMA));
        for schema in schemas {
            for opt in &schema.opts {
                assert!(
                    !common.contains(&opt.key),
                    "module `{}` redeclares the common key `{}`",
                    schema.id,
                    opt.key
                );
            }
        }
        for opt in &COMMON_OPTS {
            let mut ov = Overrides::default();
            assert!(
                set_common(&mut ov, opt.key, opt.default.clone()),
                "`{}` is a common option with no `set_common` arm: it would be dropped",
                opt.key
            );
        }

        // The whole chain, end to end, so the tripwire covers all four edits
        // a sixth common option needs rather than only the `set_common` arm:
        // every key is written into a config, and the value that comes back
        // out of the resolved `ModuleCfg` must be the one that went in. A
        // missing `Overrides` field, `resolve` arm or `common` arm all show
        // up here as the default coming back.
        let not_the_default = |opt: &OptSpec| match &opt.default {
            Value::Bool(b) => Value::Bool(!b),
            Value::Int(n) => Value::Int(n.saturating_add(1)),
            Value::Float(f) => Value::Float(f + 0.5),
            Value::Str(s) => Value::Str(format!("{s}x")),
            other => other.clone(),
        };
        let rows = COMMON_OPTS.iter().map(|opt| {
            let value = match not_the_default(opt) {
                Value::Bool(b) => b.to_string(),
                Value::Int(n) => n.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Str(s) => crate::config::schema::toml_string(&s),
                other => panic!("no literal for {other:?}"),
            };
            format!("{} = {value}\n", opt.key)
        });
        let text: String = std::iter::once("[modules.clock]\n".to_owned()).chain(rows).collect();
        let (cfg, errs) = parse(&text, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new(), "{text}");
        let clock = cfg.modules.get("clock").expect("the clock module");
        for opt in &COMMON_OPTS {
            assert_eq!(
                clock.common(opt.key),
                Some(not_the_default(opt)),
                "`{}` did not survive the config → `ModuleCfg` chain ({text})",
                opt.key
            );
        }
    }

    /// SPEC § 4.1 `separator_color`: a role or a literal resolved at config
    /// time, `inherit`, the muted role when unset or bad, and `config show`
    /// writes the value as it was written.
    #[test]
    fn separator_color_resolves_inherits_or_falls_back_to_muted() {
        let all = &crate::modules::SCHEMAS;
        let (c, errs) = parse("", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(
            c.frame.separator_color,
            SeparatorColor::Fixed { spec: "muted".into(), color: c.theme.role(Role::Muted) }
        );
        let (c, errs) = parse("[frame]\nseparator_color = \"inherit\"\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_color, SeparatorColor::Inherit);
        let (c, errs) = parse("[frame]\nseparator_color = \"accent\"\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(
            c.frame.separator_color,
            SeparatorColor::Fixed { spec: "accent".into(), color: c.theme.role(Role::Accent) }
        );
        let (c, errs) = parse("[frame]\nseparator_color = \"#ff8800\"\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_color.spec(), "#ff8800");
        let (c, errs) = parse("[frame]\nseparator_color = \"nope\"\n", all);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "frame.separator_color");
        assert!(errs[0].message.contains("inherit"), "{}", errs[0].message);
        assert_eq!(c.frame.separator_color.spec(), "muted");
        let (_, errs) = parse("[frame]\nseparator_color = 3\n", all);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "frame.separator_color");
        // `config show` writes the value as written and it parses back.
        for spec in ["inherit", "accent", "#ff8800", "muted"] {
            let (c, _) = parse(&format!("[frame]\nseparator_color = \"{spec}\"\n"), all);
            let shown = crate::docs::config_toml(&c, false);
            assert!(shown.contains(&format!("separator_color = \"{spec}\"")), "{shown}");
            let (again, errs) = parse(&shown, all);
            assert_eq!(errs, Vec::new());
            assert_eq!(again.frame, c.frame);
        }
    }

    /// SPEC § 4 `[format]`: each key is reported and defaulted on its own,
    /// the table's absence is the old rendering, and the per-module
    /// override keys exist only on the modules that print that kind.
    #[test]
    fn format_table_parses_per_key_and_the_overrides_sit_on_the_right_modules() {
        let all = &crate::modules::SCHEMAS;
        let (c, errs) = parse("", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.format, FormatCfg::default());
        let (c, errs) = parse(
            "[format]\ntokens = \"precise\"\npercent = \"precise\"\ncost = \"whole\"\nparens = \"dim\"\n",
            all,
        );
        assert_eq!(errs, Vec::new());
        assert_eq!(
            c.format,
            FormatCfg {
                tokens: TokenStyle::Precise,
                percent: PercentStyle::Precise,
                cost: CostStyle::Whole,
                parens: ParensStyle::Dim,
            }
        );
        // One bad key falls back alone; an unknown key is named with the
        // four expected; a non-table is refused whole.
        let (c, errs) = parse("[format]\ntokens = \"loose\"\ncost = \"whole\"\nstyle = 1\n", all);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["format.tokens", "format.style"], "{errs:?}");
        assert!(errs.iter().any(|e| e.message.contains("compact, precise, whole")), "{errs:?}");
        assert!(errs.iter().any(|e| e.message.contains("tokens, percent, cost, parens")));
        assert_eq!(c.format.tokens, TokenStyle::Compact);
        assert_eq!(c.format.cost, CostStyle::Whole);
        let (c, errs) = parse("format = \"precise\"\n", all);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "format");
        assert_eq!(c.format, FormatCfg::default());
        // The overrides: `tokens` on the modules that print tokens, `percent`
        // on those that print a percentage, `cost` on `cost`, nowhere else.
        let carriers = |key: &str| -> Vec<&str> {
            all.iter().filter(|s| s.opt(key).is_some()).map(|s| s.id).collect()
        };
        assert_eq!(carriers("tokens"), ["context", "cache"]);
        assert_eq!(carriers("percent"), ["context", "limit5h", "limit7d", "spend", "api", "cache"]);
        assert_eq!(carriers("cost"), ["cost"]);
        let (c, errs) = parse(
            "[modules.context]\ntokens = \"whole\"\npercent = \"precise\"\n[modules.cost]\ncost = \"whole\"\n[modules.model]\ntokens = \"whole\"\n",
            all,
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "modules.model.tokens");
        assert_eq!(c.modules.get("context").map(|m| m.str("tokens")), Some("whole"));
        assert_eq!(c.modules.get("cache").map(|m| m.str("tokens")), Some("inherit"));
        assert_eq!(c.modules.get("cost").map(|m| m.str("cost")), Some("whole"));
        // `config show` writes the table and it parses back.
        let shown = crate::docs::config_toml(&c, false);
        assert!(shown.contains("[format]\ntokens = \"compact\""), "{shown}");
        let (again, errs) = parse(&shown, all);
        assert_eq!(errs, Vec::new());
        assert_eq!(again.format, c.format);
    }

    /// SPEC § 3 `hide`: every state is checked against the schema's measure,
    /// a bad list is refused whole (the default stands in), the list and
    /// `hide_when_empty` are a union, and `config show` writes it back.
    #[test]
    fn hide_lists_follow_the_schemas_measure() {
        let (c, errs) = parse(
            "[modules.cost]\nhide = [\"zero\", \"empty\"]\n[modules.context]\nhide = [\"below:10\", \"above:90.5\"]\n[modules.text.note]\ntext = \"x\"\nhide = [\"empty\"]\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        let cost = c.modules.get("cost").unwrap();
        assert_eq!(cost.hide, vec![HideRule::Zero, HideRule::Empty]);
        assert_eq!(
            c.modules.get("context").unwrap().hide,
            vec![HideRule::Below(10.0), HideRule::Above(90.5)]
        );
        assert_eq!(c.texts.get("note").unwrap().hide, vec![HideRule::Empty]);
        assert!(cost.hides_empty());
        // The union: `empty` in the list hides an empty render whatever
        // `hide_when_empty` says, and the flag alone still works.
        let (c, errs) = parse(
            "[modules.pr]\nhide_when_empty = false\nhide = [\"empty\"]\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        assert!(c.modules.get("pr").unwrap().hides_empty());
        let (c, _) = parse("[modules.pr]\nhide_when_empty = false\n", &crate::modules::SCHEMAS);
        assert!(!c.modules.get("pr").unwrap().hides_empty());
        // Refused lists, each naming what the module accepts; the default stands.
        let refused = [
            (
                "[modules.context]\nhide = [\"zero\"]\n",
                "modules.context",
                "empty, below:N, above:N",
            ),
            ("[modules.cost]\nhide = [\"below:5\"]\n", "modules.cost", "accepts empty, zero"),
            ("[modules.model]\nhide = [\"zero\"]\n", "modules.model", "accepts empty"),
            (
                "[modules.text.a]\ntext = \"x\"\nhide = [\"zero\"]\n",
                "modules.text.a",
                "accepts empty",
            ),
            (
                "[modules.cost]\nhide = [\"empty\", \"nope\"]\n",
                "modules.cost",
                "unknown hide state",
            ),
            ("[modules.context]\nhide = [\"below:2000\"]\n", "modules.context", "0 to 1000"),
            ("[modules.context]\nhide = [\"below:x\"]\n", "modules.context", "needs a number"),
            ("[modules.cost]\nhide = \"zero\"\n", "modules.cost", "expected a list"),
            ("[modules.cost]\nhide = [1]\n", "modules.cost", "expected a list"),
        ];
        for (text, base, message) in refused {
            let (c, errs) = parse(text, &crate::modules::SCHEMAS);
            assert_eq!(errs.len(), 1, "{text}: {errs:?}");
            assert_eq!(errs[0].path, format!("{base}.hide"), "{text}");
            assert!(errs[0].message.contains(message), "{text}: {}", errs[0].message);
            // A state the parser does not know is refused naming what the
            // module accepts, as one the measure disallows is.
            if message == "unknown hide state" {
                assert!(
                    errs[0].message.ends_with("; this module accepts empty, zero"),
                    "{text}: {}",
                    errs[0].message
                );
            }
            let id = base.trim_start_matches("modules.");
            let hide =
                c.modules.get(id).map(|m| m.hide.clone()).or_else(|| {
                    c.texts.get(id.trim_start_matches("text.")).map(|m| m.hide.clone())
                });
            assert_eq!(hide, Some(Vec::new()), "{text}");
        }
        // `config show` writes the list, and it parses back to the same rules.
        let (c, _) = parse("[modules.context]\nhide = [\"below:10\"]\n", &crate::modules::SCHEMAS);
        let shown = crate::docs::config_toml(&c, false);
        assert!(shown.contains("hide = [\"below:10\"]"), "{shown}");
        let (again, errs) = parse(&shown, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(again.modules.get("context").unwrap().hide, vec![HideRule::Below(10.0)]);
        // Every module table round-trips (`show` pins `animate`, so the whole
        // config is compared in the shorthand test, not here).
        assert_eq!(again.modules, c.modules);
        assert_eq!(again.texts, c.texts);
    }

    /// SPEC § 4.1: a bar glyph is repeated cell by cell, so an override
    /// that is not one cell is reported and the schema glyph stays — the
    /// same rule `frame.fill_char` has always had. It used to be swallowed:
    /// `config check` said `ok`, `config show` echoed the glyph, and the
    /// tick drew `█` because `util::bar` substituted one.
    #[test]
    fn a_bar_glyph_override_must_be_one_cell() {
        for (id, key) in [("context", "fill"), ("context", "marker"), ("limit5h", "empty")] {
            let text = format!("[modules.{id}.icons]\n{key} = \"🟩\"\n");
            let (cfg, errs) = parse(&text, &crate::modules::SCHEMAS);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(
                problems,
                [(&*format!("modules.{id}.icons.{key}"), "must be exactly one cell wide")],
                "{text}"
            );
            let module = cfg.modules.get(id).unwrap();
            assert_eq!(crate::ansi::display_width(module.icon(key)), 1, "{text}");
        }
        // A one-cell override is still taken, and a key that is not a bar
        // cell may be any width.
        let (cfg, errs) = parse(
            "[modules.context.icons]\nfill = \"▓\"\ncontext = \"ctx:\"\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        let context = cfg.modules.get("context").unwrap();
        assert_eq!((context.icon("fill"), context.icon("context")), ("▓", "ctx:"));

        // An animated bar glyph is the same rule: a frame is the glyph for
        // its tick, so equal widths are not enough.
        let (_, errs) = parse(
            "[modules.context.icons]\nfill_frames = [\"🟩\", \"🟥\"]\n",
            &crate::modules::SCHEMAS,
        );
        let problems: Vec<(&str, &str)> =
            errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
        assert_eq!(
            problems,
            [("modules.context.icons.fill_frames", "must be exactly one cell wide")]
        );
        let (cfg, errs) = parse(
            "[modules.context.icons]\nfill_frames = [\"▓\", \"▒\"]\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new(), "one-cell frames are still taken");
        assert!(cfg.modules.contains_key("context"));

        // Blanking the marker is how it is turned off, and `util::bar`
        // honours it, so the width rule must not refuse it. The cells
        // themselves cannot be blanked: a zero-width cell has no width to
        // repeat.
        let (cfg, errs) =
            parse("[modules.context.icons]\nmarker = \"\"\n", &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(cfg.modules.get("context").map(|m| m.icon("marker")), Some(""));
        for key in ["fill", "empty"] {
            let text = format!("[modules.context.icons]\n{key} = \"\"\n");
            let (_, errs) = parse(&text, &crate::modules::SCHEMAS);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(
                problems,
                [(&*format!("modules.context.icons.{key}"), "must be exactly one cell wide")],
                "{text}"
            );
        }
    }

    /// SPEC § 3.7: the keys a text module refuses are one table, so the
    /// refusal and the "expected one of" list can never drift apart.
    #[test]
    fn every_rejected_text_key_is_refused_with_its_own_message() {
        for (key, why) in TEXT_REJECTED_KEYS {
            let value = if key == "icons" { "{ folder = \"x\" }" } else { "1" };
            let text = format!("[modules.text.a]\ntext = \"hi\"\n{key} = {value}\n");
            let (c, errs) = parse(&text, &crate::modules::SCHEMAS);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(problems, [(&*format!("modules.text.a.{key}"), why)], "{key}");
            assert_eq!(c.texts.get("a").map(|t| t.str("text")), Some("hi"), "{key}");
            assert!(!text_takes(key), "{key}");
        }
        // Every common key `text_takes` (what the docs page and the setup
        // form list) parses there without a word.
        for opt in COMMON_OPTS.iter().filter(|o| text_takes(o.key)) {
            let text = format!("[modules.text.a]\n{} = {}\n", opt.key, opt.default.to_toml());
            assert_eq!(parse(&text, &crate::modules::SCHEMAS).1, Vec::new(), "{}", opt.key);
        }
    }

    /// A number option takes finite numbers only: TOML's `nan` and `inf`
    /// are reported at the key (frm-07), and the default stands in.
    #[test]
    fn a_number_option_refuses_nan_and_inf() {
        for (text, path) in [
            ("[modules.context]\nwarn_at = nan\n", "modules.context.warn_at"),
            ("[modules.context]\nwarn_at = -inf\n", "modules.context.warn_at"),
            ("[modules.context]\nthresholds = [50.0, nan]\n", "modules.context.thresholds"),
        ] {
            let (c, errs) = parse(text, &crate::modules::SCHEMAS);
            let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
            assert_eq!(paths, [path], "{text}");
            let ctx = c.modules.get("context").unwrap();
            assert!(ctx.value("warn_at").is_none_or(|v| *v == Value::Float(0.0)), "{text}");
        }
    }

    /// SPEC § 3: `max_width` is a common option bounded like a cell count;
    /// a text module has `width` instead and is told so (SPEC § 3.7).
    #[test]
    fn max_width_is_a_common_option_except_on_text_modules() {
        let text = "[modules.branch]\nmax_width = 12\n[modules.model]\nmax_width = 2000\n[modules.pr]\nmax_width = -1\n[modules.text.a]\ntext = \"hi\"\nmax_width = 3\n";
        let (c, errs) = parse(text, &crate::modules::SCHEMAS);
        let problems: Vec<(&str, &str)> =
            errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
        assert_eq!(
            problems,
            [
                // Modules are checked in registry order (`pr` before `model`).
                ("modules.pr.max_width", "expected a non-negative integer"),
                ("modules.model.max_width", "must be at most 1024"),
                (
                    "modules.text.a.max_width",
                    "a text module's box is sized by `width`; remove this key"
                ),
            ]
        );
        assert_eq!(c.modules.get("branch").unwrap().max_width, 12);
        assert_eq!(c.modules.get("model").unwrap().max_width, 0, "the default stands in");
        assert_eq!(c.modules.get("pr").unwrap().max_width, 0);
        assert_eq!(c.texts.get("a").unwrap().max_width, 0);
        // An unknown key names it among the common keys, except on a text
        // module, where the message never recommends a key it would reject.
        let (_, errs) = parse("[modules.model]\nmax_widht = 1\n", &crate::modules::SCHEMAS);
        assert!(errs[0].message.contains("max_width"), "{errs:?}");
        let (_, errs) =
            parse("[modules.text.a]\ntext = \"x\"\nmax_widht = 1\n", &crate::modules::SCHEMAS);
        assert_eq!(errs.len(), 1, "{errs:?}");
        for rejected in ["max_width", "preset", "refresh", "icons"] {
            assert!(!errs[0].message.contains(rejected), "{rejected}: {}", errs[0].message);
        }
        assert!(errs[0].message.contains("hide_when_empty, colors, text, width"), "{errs:?}");
        // The common strings are still reduced to plain text on the way in.
        let (c, errs) = parse(
            "[modules.model]\nlabel = \"a\\u001b[31mb\"\nhide_when_empty = false\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        let model = c.modules.get("model").unwrap();
        assert_eq!(model.label, "ab");
        assert!(!model.hide_when_empty);
        assert_eq!(model.common("hide_when_empty"), Some(Value::Bool(false)));
        assert_eq!(model.common("max_width"), Some(Value::Int(0)));
        assert_eq!(model.common("show_id"), None);
    }

    #[test]
    fn the_resolved_config_carries_only_what_is_in_effect() {
        // `config show` writes the resolved config; an unknown theme name or
        // an unknown line id echoed back made its output fail `config check`
        // (whole-stack review).
        let text = "theme = \"solarized\"\n[[line]]\nmodules = [\"text.motd\", \"clock\", \"nope\"]\nright = [\"path\"]\n";
        let (c, errs) = parse(text, &crate::modules::SCHEMAS);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["theme", "line[0].modules[0]", "line[0].modules[2]"], "{errs:?}");
        assert_eq!(c.theme_name, "garnish", "the palette in effect, not the typo");
        assert_eq!(c.rows[0].cols[0].left, ["clock"]);
        assert_eq!(c.rows[0].cols[0].right, ["path"]);
        let shown = crate::docs::config_toml(&c, false);
        let (again, errs) = parse(&shown, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new(), "{shown}");
        assert_eq!(crate::docs::config_toml(&again, false), shown);
    }

    /// `config show` is a fixed point for every layout form too (SPEC § 4.3):
    /// what it writes parses clean, resolves to the same config, and writes
    /// itself back byte for byte.
    #[test]
    fn config_show_round_trips_columns_stacks_titles_and_boxes() {
        let all = &crate::modules::SCHEMAS;
        let text = "\
[box.repo]
title = \"Repository\"
title_justify = \"center\"
title_pad = 2
style = \"double\"
color = \"accent\"

[[row]]
title = \"Session\"
title_color = \"warn\"
modules = [\"path\", \"branch\"]
right = [\"clock\"]

[[row]]
gap = 3
[[row.col]]
width = \"2fr\"
box = \"repo\"
[[row.col.row]]
modules = [\"path\"]
[[row.col.row]]
modules = [\"branch\"]
separator = \" - \"
[[row.col]]
width = \"auto\"
justify = \"center\"
valign = \"bottom\"
modules = [\"model\"]
[[row.col]]
width = 24
box = true
modules = [\"cost\"]

[[row]]
modules = []
blank = true
";
        let (c, errs) = parse(text, all);
        assert_eq!(errs, Vec::new(), "{errs:?}");
        assert_eq!(c.rows.len(), 3);
        assert_eq!(c.rows[1].gap, 3);
        assert_eq!(c.rows[1].cols.len(), 3);
        assert_eq!(c.rows[1].cols[0].rows.len(), 2, "the first column is a stack");
        assert_eq!(c.rows[1].cols[2].boxed, Some(BoxRef::Anon));
        assert!(c.rows[2].spacer && c.rows[2].blank);

        let shown = crate::docs::config_toml(&c, false);
        let (again, errs) = parse(&shown, all);
        assert_eq!(errs, Vec::new(), "{shown}");
        assert_eq!(again.rows, c.rows, "the layout survives the round trip");
        assert_eq!(again.boxes, c.boxes);
        assert_eq!(crate::docs::config_toml(&again, false), shown, "a fixed point");
        // The plain form stays plain: `config show` writes no column table
        // for a row that has none.
        let first = shown.split("[[row]]").nth(1).unwrap_or_default();
        assert!(!first.contains("[[row.col]]"), "{shown}");
    }

    #[test]
    fn syntax_errors_carry_a_line_and_fall_back_wholesale() {
        let (c, errs) = parse("preset = \"minimal\"\n[frame\nstyle = 1", &schemas());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, Some(2));
        assert!(errs[0].to_string().starts_with("line 2: "));
        assert_eq!(c, Config::defaults(&schemas()), "not TOML: nothing can be trusted");
        // ... but the command line still is: `preview --color never --icons
        // ascii` of a broken file renders plain ascii (whole-stack review).
        let overlay = Overlay {
            color: Some(ColorChoice::Never),
            icons: Some(IconSet::Ascii),
            preset: Some(TopPreset::Compact),
            theme: Some("nord".into()),
        };
        let (c, errs) = parse_with("[frame\nstyle = 1", &schemas(), &overlay);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].line, Some(1));
        assert_eq!(c.color, ColorChoice::Never);
        assert_eq!(c.icons, IconSet::Ascii);
        assert_eq!(c.preset, TopPreset::Compact);
        assert_eq!(c.theme_name, "nord");
        let (c, errs) = parse("unknown_top = 1\npreset = \"minimal\"", &schemas());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "unknown_top");
        assert!(errs[0].message.contains("unknown key"), "{}", errs[0].message);
        assert_eq!(c.preset, TopPreset::Minimal, "the valid key next to it still counts");
    }

    #[test]
    fn minimal_preset_is_unframed_and_compact_has_two_lines() {
        let (c, _) = parse("preset = \"minimal\"", &schemas());
        assert_eq!(c.frame.style, FrameStyle::None);
        assert!(c.frame.fill);
        assert_eq!(c.rows.len(), 1);
        // A fill glyph that is not one cell is reported (SPEC § 5) and the
        // style's own glyph stays, instead of a silent blank rule.
        let (wide, errs) = parse("[frame]\nfill_char = \"ab\"", &schemas());
        assert_eq!(wide.frame.chars.fill, "─");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "frame.fill_char");
        let (empty, errs) = parse("[frame]\nfill_char = \"\"", &schemas());
        assert_eq!(empty.frame.chars.fill, "─");
        assert_eq!(errs.len(), 1, "{errs:?}");
        let (esc, errs) = parse("[frame]\nfill_char = \"\\u001b[31m-\"", &schemas());
        assert_eq!(esc.frame.chars.fill, "-", "the escape is stripped before the width check");
        assert_eq!(errs, Vec::new());
        let (c, _) = parse("preset = \"compact\"", &schemas());
        assert_eq!(c.rows.len(), 2);
        assert_eq!(c.modules.get("path").unwrap().preset, Preset::Default);
        let (c, _) = parse("preset = \"full\"", &schemas());
        assert_eq!(c.modules.get("path").unwrap().preset, Preset::Full);
    }

    #[test]
    fn color_choice_modes() {
        assert_eq!(ColorChoice::Auto.mode(false), ColorMode::TrueColor);
        assert_eq!(ColorChoice::Auto.mode(true), ColorMode::Never);
        assert_eq!(ColorChoice::Never.mode(false), ColorMode::Never);
        assert_eq!(ColorChoice::Ansi256.mode(false), ColorMode::Ansi256);
    }
}
