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

pub mod presets;
pub mod schema;

use presets::TopPreset;
use schema::{COMMON_KEYS, Kind, ModuleCfg, ModuleSchema, Overrides, Preset, Value};

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

/// One `[[line]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineCfg {
    /// Left-aligned module ids.
    pub left: Vec<String>,
    /// Right-aligned module ids.
    pub right: Vec<String>,
    /// Separator override for this line.
    pub separator: Option<String>,
    /// Configured with `modules = []` and no `right`: an intentional blank
    /// row that `hide_empty_lines` never drops (SPEC § 4.1).
    pub spacer: bool,
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

/// Frame configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameCfg {
    /// Style.
    pub style: FrameStyle,
    /// Characters in effect.
    pub chars: FrameChars,
    /// Fill the rule to the full width.
    pub fill: bool,
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
/// Reported at config time, clamped again at render time.
pub const MAX_CELLS: usize = 1024;

/// Longest string a config may put on a row (`text`, `gap`, `ticker_gap`),
/// in characters: a status line, not a document. Reported at config time.
pub const MAX_TEXT_CHARS: usize = 4096;

/// The fully resolved configuration.
// Four independent switches that mirror config keys one to one; a bitset
// would only obscure the mapping.
#[allow(clippy::struct_excessive_bools)]
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
    /// Drop a line whose modules all rendered nothing (spacers are kept).
    pub hide_empty_lines: bool,
    /// Truncate or scroll a left group wider than its budget.
    pub overflow: Overflow,
    /// Cells the ticker advances per tick (`> 0`; 0.5 = every second tick).
    pub ticker_step: f64,
    /// Text between the end of a scrolled group and its wrapped-around start.
    pub ticker_gap: String,
    /// Master switch for every animation; `false` freezes them at frame 0
    /// (SPEC § 4.2). `GARNISH_ANIMATE=0` does the same for a session.
    pub animate: bool,
    /// How elapsed times and countdowns print.
    pub durations: DurationStyle,
    /// Frame.
    pub frame: FrameCfg,
    /// Lines.
    pub lines: Vec<LineCfg>,
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
    hide_empty_lines: Option<bool>,
    overflow: Option<Overflow>,
    ticker_step: Option<f64>,
    ticker_gap: Option<String>,
    animate: Option<bool>,
    durations: Option<DurationStyle>,
    colors: BTreeMap<String, String>,
    frame: Option<RawFrame>,
    line: Vec<RawLine>,
    modules: BTreeMap<String, toml::Table>,
}

const TOP_KEYS: [&str; 20] = [
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
    "hide_empty_lines",
    "overflow",
    "ticker_step",
    "ticker_gap",
    "animate",
    "durations",
    "colors",
    "frame",
    "line",
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
    // The table is taken by value so every field moves into place: cloning
    // each value cost a fifth of the parse on the full annotated file.
    fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut raw = Self::default();
        let presets = TopPreset::ALL.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ");
        let icon_sets = IconSet::ALL.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
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
                "hide_empty_lines" => raw.hide_empty_lines = field(&key, value, errors),
                "overflow" => raw.overflow = enum_field(&key, value, OVERFLOWS, errors),
                "ticker_step" => raw.ticker_step = field(&key, value, errors),
                "ticker_gap" => raw.ticker_gap = field(&key, value, errors),
                "animate" => raw.animate = field(&key, value, errors),
                "durations" => raw.durations = enum_field(&key, value, DURATION_STYLES, errors),
                "colors" => match value {
                    toml::Value::Table(t) => raw.colors = string_table("colors", t, errors),
                    _ => errors.push(problem("colors", "expected a table of role = color")),
                },
                "frame" => match value {
                    toml::Value::Table(t) => raw.frame = Some(RawFrame::from_table(t, errors)),
                    _ => errors.push(problem("frame", "expected a [frame] table")),
                },
                "line" => match value {
                    toml::Value::Array(items) => {
                        for (i, item) in items.into_iter().enumerate() {
                            let path = format!("line[{i}]");
                            match item {
                                toml::Value::Table(t) => {
                                    raw.line.push(RawLine::from_table(&path, t, errors));
                                }
                                _ => errors.push(problem(&path, "expected a [[line]] table")),
                            }
                        }
                    }
                    _ => errors.push(problem("line", "expected [[line]] tables")),
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
        raw
    }
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
    fill_pattern: Option<String>,
    fill_step: Option<f64>,
    fill_direction: Option<FillDirection>,
    separator_frames: Option<Vec<String>>,
    separator_step: Option<f64>,
}

const FRAME_KEYS: [&str; 18] = [
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
    "fill_pattern",
    "fill_step",
    "fill_direction",
    "separator_frames",
    "separator_step",
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

#[derive(Debug, Default)]
struct RawLine {
    modules: Vec<String>,
    right: Vec<String>,
    separator: Option<String>,
    /// `modules` or `right` was not a list at all (reported), so the empty
    /// list that stands in must not read as an intentional spacer.
    bad_list: bool,
}

impl RawLine {
    fn from_table(path: &str, table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut line = Self::default();
        for (key, value) in table {
            let path = format!("{path}.{key}");
            match key.as_str() {
                "modules" | "right" => {
                    line.bad_list |= !value.is_array();
                    let ids = id_list(&path, value, errors);
                    if key == "modules" {
                        line.modules = ids;
                    } else {
                        line.right = ids;
                    }
                }
                "separator" => {
                    line.separator =
                        field::<String>(&path, value, errors).map(|s| crate::ansi::plain_text(&s));
                }
                _ => errors
                    .push(problem(&path, "unknown key; expected one of modules, right, separator")),
            }
        }
        line
    }
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

/// Locate the config file: explicit path > `GARNISH_CONFIG` > XDG > `~/.garnish.toml`.
#[must_use]
pub fn locate(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Some(p) = std::env::var_os(CONFIG_ENV) {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".config")))
        .map(|d| d.join("garnish").join("garnish.toml"));
    if let Some(p) = xdg.filter(|p| p.is_file()) {
        return Some(p);
    }
    home.map(|h| h.join(".garnish.toml")).filter(|p| p.is_file())
}

/// The default location a new config should be written to.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    // Without a home there is no default: guessing `.` would write into
    // whatever directory garnish happens to run from (a repository, say).
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("garnish").join("garnish.toml"))
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
    let mut raw = match toml::from_str::<toml::Table>(text) {
        Ok(table) => RawConfig::from_table(table, &mut errors),
        Err(e) => {
            // The whole file falls back to the defaults, under the same
            // command-line overrides as a good file would be: `preview
            // --color never` of a broken config must still be plain.
            let line = e.span().map(|s| line_of(text, s.start));
            errors.push(ConfigError { path: String::new(), message: e.message().to_owned(), line });
            RawConfig::default()
        }
    };
    if overlay.preset.is_some() {
        // The preset's lines replace the file's, so their problems are moot.
        raw.preset = overlay.preset;
        raw.line.clear();
        errors.retain(|e| !e.path.starts_with("line["));
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

    /// Separator for a line.
    #[must_use]
    pub fn separator<'a>(&'a self, line: &'a LineCfg) -> &'a str {
        line.separator.as_deref().unwrap_or(&self.frame.chars.separator)
    }

    /// Separator for a line at animation frame `frame`: the line's own
    /// override wins, then `separator_frames[frame]`, then the static
    /// separator (SPEC § 4.2).
    #[must_use]
    pub fn separator_at<'a>(&'a self, line: &'a LineCfg, frame: usize) -> &'a str {
        line.separator
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

    let frame = resolve_frame(raw.frame.as_ref(), preset, errors);
    let mut lines: Vec<LineCfg> = if raw.line.is_empty() {
        preset.lines()
    } else {
        raw.line
            .iter()
            .map(|l| LineCfg {
                left: l.modules.clone(),
                right: l.right.clone(),
                separator: l.separator.clone(),
                // Only a line written empty is a spacer; a mistyped
                // `modules` is an error and an empty row, which
                // `hide_empty_lines` then drops like any other.
                spacer: l.modules.is_empty() && l.right.is_empty() && !l.bad_list,
            })
            .collect()
    };
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
    // Preset lines are valid by construction; only explicit lines need checking.
    if !raw.line.is_empty() {
        check_line_ids(&mut lines, schemas, &texts, errors);
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
        hide_empty_lines: raw.hide_empty_lines.unwrap_or(true),
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
        animate: raw.animate.unwrap_or(true),
        durations: raw.durations.unwrap_or_default(),
        frame,
        lines,
        modules,
        texts,
    }
}

/// Every id on a `[[line]]` is a registered module or a defined `text.<name>`;
/// an unknown one is reported and removed, so the resolved config (and
/// `config show`) carries only ids that render.
fn check_line_ids(
    lines: &mut [LineCfg],
    schemas: &[ModuleSchema],
    texts: &BTreeMap<String, ModuleCfg>,
    errors: &mut Vec<ConfigError>,
) {
    for (i, line) in lines.iter_mut().enumerate() {
        for (field, ids) in [("modules", &mut line.left), ("right", &mut line.right)] {
            let mut j = 0_usize;
            ids.retain(|id| {
                let path = format!("line[{i}].{field}[{j}]");
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

/// A text module name is a bare TOML key, so `text.<name>` is unambiguous on
/// a line and `config show` can write `[modules.text.<name>]` back verbatim.
fn is_bare_key(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The `[modules.text.<name>]` tables (SPEC § 3.7): each is validated against
/// the text schema under its own path. `refresh`, `preset` and `icons` do not
/// apply to text modules, `step` must be positive, `color` is the shorthand
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
        for (key, why) in [
            ("refresh", "text modules render every tick; remove this key"),
            ("preset", "text modules have no presets; remove this key"),
            ("icons", "text modules have no icons; remove this table"),
        ] {
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
        texts.insert(name.clone(), ModuleCfg::resolve(schema, Preset::Default, icons, theme, &ov));
    }
    texts
}

/// The steps an animation may take per tick. Below the range the frame
/// never changes in a lifetime; above it `now × step` saturates and freezes
/// (`1e308`), so both ends are rejected rather than silently still.
const STEP_RANGE: std::ops::RangeInclusive<f64> = 0.001..=1000.0;
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
            let frames: Vec<String> = frames.iter().map(|s| crate::ansi::plain_text(s)).collect();
            let width = frames.first().map_or(0, |f| crate::ansi::display_width(f));
            if frames.iter().all(|f| crate::ansi::display_width(f) == width) {
                frames
            } else {
                errors.push(problem(
                    "frame.separator_frames",
                    "every frame must have the same width, or the columns would jitter",
                ));
                Vec::new()
            }
        });
    FrameCfg {
        style,
        chars,
        fill,
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
            "hide_when_empty" => match value.as_bool() {
                Some(b) => ov.hide_when_empty = Some(b),
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
            "label" | "prefix" | "suffix" => match value.as_str() {
                Some(s) => {
                    let s = crate::ansi::plain_text(s);
                    match key.as_str() {
                        "label" => ov.label = Some(s),
                        "prefix" => ov.prefix = Some(s),
                        _ => ov.suffix = Some(s),
                    }
                }
                None => err(key, "expected a string".into()),
            },
            "icons" => match value.as_table() {
                Some(t) => parse_icons(schema, t, &mut ov, &mut err),
                None => err(key, "expected a table of icon overrides".into()),
            },
            "colors" => match value.as_table() {
                Some(t) => parse_colors(schema, t, &mut ov, &mut err),
                None => err(key, "expected a table of color overrides".into()),
            },
            other => match schema.opt(other) {
                Some(spec) => match coerce(spec.kind, value).and_then(|v| bounded(other, v)) {
                    Ok(v) => {
                        ov.opts.insert(other.to_owned(), v);
                    }
                    Err(msg) => err(other, msg),
                },
                None => err(other, unknown_option_message(schema)),
            },
        }
    }
    ov
}

/// The size limits a module option must respect: cell counts (`width`,
/// `pad`) at most [`MAX_CELLS`], row text (`text`, `gap`) at most
/// [`MAX_TEXT_CHARS`] characters. A row is a fixed, small thing; a number
/// beyond these is a mistake, and honouring it would size an allocation or a
/// loop on every tick.
fn bounded(key: &str, value: Value) -> Result<Value, String> {
    match (key, &value) {
        ("width" | "pad" | "bar_width", Value::Int(n))
            if usize::try_from(*n).is_ok_and(|n| n > MAX_CELLS) =>
        {
            Err(format!("must be at most {MAX_CELLS} cells"))
        }
        ("text" | "gap", Value::Str(s)) if s.chars().count() > MAX_TEXT_CHARS => {
            Err(format!("must be at most {MAX_TEXT_CHARS} characters"))
        }
        _ => Ok(value),
    }
}

fn unknown_option_message(schema: &ModuleSchema) -> String {
    format!(
        "unknown option; expected one of {}",
        COMMON_KEYS
            .iter()
            .copied()
            .chain(std::iter::once("colors"))
            .chain(schema.opts.iter().map(|o| o.key))
            .collect::<Vec<_>>()
            .join(", ")
    )
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
            && schema.icon(base).is_some()
        {
            let frames: Option<Vec<String>> = iv.as_array().and_then(|items| {
                items.iter().map(|f| f.as_str().map(crate::ansi::plain_text)).collect()
            });
            match frames {
                Some(frames) if frames.is_empty() => {
                    err(&format!("icons.{ik}"), "expected at least one frame".into());
                }
                Some(frames) => {
                    let width = frames.first().map_or(0, |f| crate::ansi::display_width(f));
                    if frames.iter().all(|f| crate::ansi::display_width(f) == width) {
                        ov.icon_frames.insert(base.to_owned(), frames);
                    } else {
                        err(
                            &format!("icons.{ik}"),
                            "every frame must have the same width, or the row would jitter".into(),
                        );
                    }
                }
                None => err(&format!("icons.{ik}"), "expected a list of strings".into()),
            }
            continue;
        }
        match (schema.icon(ik), iv.as_str()) {
            (Some(_), Some(s)) => {
                ov.icons.insert(ik.clone(), crate::ansi::plain_text(s));
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
        Kind::Float => value
            .as_float()
            .or_else(|| {
                value.as_integer().map(|i| crate::num::u64_to_f64(u64::try_from(i).unwrap_or(0)))
            })
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
                    i.as_float().or_else(|| {
                        i.as_integer()
                            .map(|n| crate::num::u64_to_f64(u64::try_from(n).unwrap_or(0)))
                    })
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
        assert_eq!(c.lines.len(), 4);
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
        assert!(c.hide_empty_lines, "empty lines are hidden by default");
        let (c, errs) = parse(
            "hide_empty_lines = false\n[[line]]\nmodules = []\n[[line]]\nright = [\"clock\"]\n[[line]]\nmodules = [\"path\"]\n",
            &schemas,
        );
        assert_eq!(errs, Vec::new());
        assert!(!c.hide_empty_lines);
        let spacers: Vec<bool> = c.lines.iter().map(|l| l.spacer).collect();
        // A mistyped list is an error and an empty row, never a spacer
        // (whole-stack review: it rendered as a permanent blank rule).
        let (bad, errs) = parse("[[line]]\nmodules = \"clock\"\n[[line]]\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "line[0].modules");
        assert!(!bad.lines[0].spacer && bad.lines[0].left.is_empty());
        assert!(bad.lines[1].spacer, "a [[line]] with no keys is a spacer");
        assert_eq!(
            spacers,
            vec![true, false, false],
            "only `modules = []` with no `right` is a spacer"
        );
        assert!(Config::defaults(&schemas).lines.iter().all(|l| !l.spacer));
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
        assert_eq!(c.lines.len(), 1);
        assert_eq!(c.separator(&c.lines[0]), "  ");
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
        assert_eq!(c.lines.len(), 1);
        assert_eq!(c.lines[0].left, vec!["path"], "the unknown id is reported and removed");
        assert_eq!(c.modules.get("clock").map(|m| m.str("format")), Some("24h"));
        // …and each bad one fell back to its own default.
        let defaults = Config::defaults(&schemas());
        assert_eq!(c.theme, defaults.theme, "unknown theme and bad colour → default palette");
        assert_eq!(c.durations, DurationStyle::Compact);
        assert_eq!(c.padding, 0);
        assert_eq!(c.lines[0].right, Vec::<String>::new(), "bad right list → no right group");
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
        assert_eq!(c.lines[0].left, vec!["clock"], "the good item stays");
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
            assert_eq!(c.lines.len(), 4, "{text}: the default lines stand in");
        }
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
        assert_eq!(c.lines[0].left, Vec::<String>::new(), "the unknown id is removed");

        // An explicit colors.text wins over the shorthand; text and gap are plain.
        let text = "[modules.text.x]\ntext = \"\\u001b[31mred\\u001b[0m\\tnote\"\ngap = \" \\u001b[5m·\\u001b[0m \"\ncolor = \"muted\"\n[modules.text.x.colors]\ntext = \"red\"\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        let x = c.texts.get("x").unwrap();
        assert_eq!(x.color("text"), Color::Ansi(1), "explicit colors.text wins");
        assert_eq!(x.str("text"), "rednote");
        assert_eq!(x.str("gap"), " · ");
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
        assert_eq!(c.separator_at(&c.lines[0], 1), " ┃ ");
        assert_eq!(c.separator_at(&c.lines[0], 7), " │ ", "out of range → static");
        let line = LineCfg { separator: Some("--".into()), ..c.lines[0].clone() };
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
        assert_eq!(c.lines.len(), 1);
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
        assert_eq!(c.lines[0].separator.as_deref(), Some("+"), "a DCS loses its payload too");
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
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "modules.context.width",
                "modules.limit5h.bar_width",
                "modules.text.a.pad",
                "modules.text.a.width"
            ],
            "{errs:?}"
        );
        assert!(errs.iter().all(|e| e.message == "must be at most 1024 cells"), "{errs:?}");
        assert_eq!(c.modules.get("context").unwrap().size("width"), 20, "default stands in");
        let a = c.texts.get("a").unwrap();
        assert_eq!((a.size("width"), a.size("pad")), (0, 0));
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
        assert_eq!(c.lines[0].left, ["clock"]);
        assert_eq!(c.lines[0].right, ["path"]);
        let shown = crate::docs::config_toml(&c, false);
        let (again, errs) = parse(&shown, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new(), "{shown}");
        assert_eq!(crate::docs::config_toml(&again, false), shown);
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
        assert_eq!(c.lines.len(), 1);
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
        assert_eq!(c.lines.len(), 2);
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
