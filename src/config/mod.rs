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

/// Frame configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCfg {
    /// Style.
    pub style: FrameStyle,
    /// Characters in effect.
    pub chars: FrameChars,
    /// Fill the rule to the full width.
    pub fill: bool,
}

/// Cells Claude Code's status line box loses to the harness's own footer
/// padding (2 on each side, `COLUMNS − 4`; SPEC § 2.1, verified in 2.1.261).
/// A row wider than the box is cut with `…` by the harness.
pub const HARNESS_PADDING: usize = 4;

/// Narrowest width garnish will render to, whatever `COLUMNS` says.
pub const MIN_WIDTH: usize = 10;

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
    /// How elapsed times and countdowns print.
    pub durations: DurationStyle,
    /// Frame.
    pub frame: FrameCfg,
    /// Lines.
    pub lines: Vec<LineCfg>,
    /// Resolved module configs, keyed by id, for every registered module.
    pub modules: BTreeMap<&'static str, ModuleCfg>,
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
    durations: Option<DurationStyle>,
    colors: BTreeMap<String, String>,
    frame: Option<RawFrame>,
    line: Vec<RawLine>,
    modules: BTreeMap<String, toml::Table>,
}

const TOP_KEYS: [&str; 15] = [
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
}

const FRAME_KEYS: [&str; 13] = [
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
];

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
                _ => None,
            };
            if let Some(slot) = text_slot {
                *slot = field(&path, value, errors);
                continue;
            }
            match key.as_str() {
                "style" => f.style = enum_field(&path, value, &styles, errors),
                "fill" => f.fill = field(&path, value, errors),
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
}

impl RawLine {
    fn from_table(path: &str, table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut line = Self::default();
        for (key, value) in table {
            let path = format!("{path}.{key}");
            match key.as_str() {
                "modules" => line.modules = id_list(&path, value, errors),
                "right" => line.right = id_list(&path, value, errors),
                "separator" => line.separator = field(&path, value, errors),
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
            errors.push(problem(path, e.message()));
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
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.unwrap_or_else(|| PathBuf::from(".")).join("garnish").join("garnish.toml")
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
        Err(e) => Loaded {
            config: Config::defaults(schemas),
            path,
            errors: vec![ConfigError {
                path: String::new(),
                message: format!("cannot read: {e}"),
                line: None,
            }],
        },
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
    let table: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => {
            let line = e.span().map(|s| line_of(text, s.start));
            let message = e.message().to_owned();
            return (
                Config::defaults(schemas),
                vec![ConfigError { path: String::new(), message, line }],
            );
        }
    };
    let mut errors = Vec::new();
    let mut raw = RawConfig::from_table(table, &mut errors);
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
    /// never below [`MIN_WIDTH`].
    #[must_use]
    pub fn width(&self, columns: Option<usize>) -> usize {
        columns
            .unwrap_or(120)
            .saturating_sub(HARNESS_PADDING)
            .saturating_sub(self.padding)
            .max(MIN_WIDTH)
    }

    /// Separator for a line.
    #[must_use]
    pub fn separator<'a>(&'a self, line: &'a LineCfg) -> &'a str {
        line.separator.as_deref().unwrap_or(&self.frame.chars.separator)
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

    let theme_name = raw.theme.clone().unwrap_or_else(|| "garnish".to_owned());
    let pal = palette(&theme_name).unwrap_or_else(|| {
        errors.push(ConfigError {
            path: "theme".into(),
            message: format!(
                "unknown theme {theme_name:?}; expected one of {}",
                PALETTES.iter().map(|p| p.name).collect::<Vec<_>>().join(", ")
            ),
            line: None,
        });
        &PALETTES[0]
    });
    let overrides = resolve_colors(&raw.colors, errors);
    let theme = Theme::from_palette(pal, &overrides);

    let frame = resolve_frame(raw.frame.as_ref(), preset);
    let lines: Vec<LineCfg> = if raw.line.is_empty() {
        preset.lines()
    } else {
        raw.line
            .iter()
            .map(|l| LineCfg {
                left: l.modules.clone(),
                right: l.right.clone(),
                separator: l.separator.clone(),
            })
            .collect()
    };
    // Preset lines are valid by construction; only explicit lines need checking.
    for (i, line) in lines.iter().enumerate().filter(|_| !raw.line.is_empty()) {
        for (field, ids) in [("modules", &line.left), ("right", &line.right)] {
            for (j, id) in ids.iter().enumerate() {
                if !schemas.iter().any(|s| s.id == id) {
                    errors.push(ConfigError {
                        path: format!("line[{i}].{field}[{j}]"),
                        message: format!(
                            "unknown module {id:?}; expected one of {}",
                            schemas.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
                        ),
                        line: None,
                    });
                }
            }
        }
    }

    let mut modules: BTreeMap<&'static str, ModuleCfg> = BTreeMap::new();
    for schema in schemas {
        let table = raw.modules.get(schema.id);
        let ov = table.map_or_else(Overrides::default, |t| parse_overrides(schema, t, errors));
        let module_preset = ov.preset.unwrap_or_else(|| preset.module_preset());
        modules.insert(schema.id, ModuleCfg::resolve(schema, module_preset, icons, &theme, &ov));
    }
    for id in raw.modules.keys() {
        if !schemas.iter().any(|s| s.id == id) {
            errors.push(ConfigError {
                path: format!("modules.{id}"),
                message: format!(
                    "unknown module; expected one of {}",
                    schemas.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
                ),
                line: None,
            });
        }
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
        durations: raw.durations.unwrap_or_default(),
        frame,
        lines,
        modules,
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

fn resolve_frame(raw: Option<&RawFrame>, preset: TopPreset) -> FrameCfg {
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
        set(&mut chars.fill, &f.fill_char);
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
    if crate::ansi::display_width(&chars.fill) != 1 {
        chars.fill = " ".into();
    }
    FrameCfg { style, chars, fill }
}

fn parse_overrides(
    schema: &ModuleSchema,
    table: &toml::Table,
    errors: &mut Vec<ConfigError>,
) -> Overrides {
    let mut ov = Overrides::default();
    let base = format!("modules.{}", schema.id);
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
                    let s = s.to_owned();
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
                Some(spec) => match coerce(spec.kind, value) {
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
        match (schema.icon(ik), iv.as_str()) {
            (Some(_), Some(s)) => {
                ov.icons.insert(ik.clone(), s.to_owned());
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
        assert_eq!(c.lines[0].left, vec!["path", "ghost"]);
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
        let text = "preset = \"compact\"\nicons = \"ascii\"\ntheme = \"nord\"\ncolor = \"never\"\ntruncate = false\nstale_style = \"hide\"\nstale_after = 3\npadding = 2\nalign = true\nright_justify = \"start\"\ndurations = \"fixed\"\n[colors]\naccent = \"red\"\n[frame]\nstyle = \"custom\"\nfill = false\nfirst = \"a\"\nmiddle = \"b\"\nlast = \"c\"\nsingle = \"d\"\nfill_char = \"-\"\nright_first = \"e\"\nright_middle = \"f\"\nright_last = \"g\"\nright_single = \"h\"\npad = \" \"\nseparator = \" | \"\n[[line]]\nmodules = [\"path\"]\nright = [\"clock\"]\nseparator = \"  \"\n[modules.path]\ndepth = 1\n";
        let (_, errs) = parse(text, &schemas());
        assert_eq!(errs, Vec::new());
    }

    #[test]
    fn preset_overlay_drops_line_errors_with_the_lines() {
        let overlay = Overlay { preset: Some(TopPreset::Minimal), ..Default::default() };
        let (c, errs) = parse_with("[[line]]\nmodules = [3]\n", &schemas(), &overlay);
        assert_eq!(errs, Vec::new(), "the overlay replaces the lines, so their problems are moot");
        assert_eq!(c.lines.len(), 1);
    }

    #[test]
    fn syntax_errors_carry_a_line_and_fall_back_wholesale() {
        let (c, errs) = parse("preset = \"minimal\"\n[frame\nstyle = 1", &schemas());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, Some(2));
        assert!(errs[0].to_string().starts_with("line 2: "));
        assert_eq!(c, Config::defaults(&schemas()), "not TOML: nothing can be trusted");
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
        let (wide, _) = parse("[frame]\nfill_char = \"ab\"", &schemas());
        assert_eq!(wide.frame.chars.fill, " ");
        let (empty, _) = parse("[frame]\nfill_char = \"\"", &schemas());
        assert_eq!(empty.frame.chars.fill, " ");
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
