//! Configuration: loading the TOML file, validating it against the module
//! schemas, applying presets, and producing a fully resolved [`Config`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ansi::{Color, ColorMode};
use crate::frame::{FrameChars, FrameStyle};
use crate::icons::IconSet;
use crate::theme::{PALETTES, Role, Theme, palette};

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
    /// Extra horizontal padding subtracted from the width.
    pub padding: usize,
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
    /// The config in effect (defaults when the file was invalid).
    pub config: Config,
    /// The file that was read, if any.
    pub path: Option<PathBuf>,
    /// Validation errors; non-empty means `config` is the built-in default.
    pub errors: Vec<ConfigError>,
}

// ---------------------------------------------------------------- raw TOML

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawConfig {
    preset: Option<TopPreset>,
    icons: Option<IconSet>,
    theme: Option<String>,
    color: Option<ColorChoice>,
    truncate: Option<bool>,
    stale_style: Option<StaleStyle>,
    padding: Option<u16>,
    colors: BTreeMap<String, String>,
    frame: Option<RawFrame>,
    line: Vec<RawLine>,
    modules: BTreeMap<String, toml::Table>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawFrame {
    style: Option<FrameStyle>,
    fill: Option<bool>,
    first: Option<String>,
    middle: Option<String>,
    last: Option<String>,
    single: Option<String>,
    #[serde(rename = "fill_char")]
    fill_char: Option<String>,
    right_first: Option<String>,
    right_middle: Option<String>,
    right_last: Option<String>,
    right_single: Option<String>,
    pad: Option<String>,
    separator: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawLine {
    modules: Vec<String>,
    right: Vec<String>,
    separator: Option<String>,
}

// ---------------------------------------------------------------- loading

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

/// Load and resolve the configuration. Never fails: an invalid file yields
/// the built-in defaults plus the list of errors.
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

/// Parse and resolve TOML text. Returns defaults plus errors when invalid.
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
    let mut raw: RawConfig = match toml::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            let line = e.span().map(|s| line_of(text, s.start));
            let message = e.message().to_owned();
            return (
                Config::defaults(schemas),
                vec![ConfigError { path: String::new(), message, line }],
            );
        }
    };
    if overlay.preset.is_some() {
        raw.preset = overlay.preset;
        raw.line.clear();
    }
    raw.icons = overlay.icons.or(raw.icons);
    raw.theme = overlay.theme.clone().or(raw.theme);
    raw.color = overlay.color.or(raw.color);
    let mut errors = Vec::new();
    let config = resolve(&raw, schemas, &mut errors);
    if errors.is_empty() {
        return (config, errors);
    }
    // Fall back to the defaults, keeping the icon set (a validated enum) so
    // the warning line and glyphs still match what the user asked for.
    let fallback = RawConfig { icons: raw.icons, ..RawConfig::default() };
    (resolve(&fallback, schemas, &mut Vec::new()), errors)
}

fn line_of(text: &str, byte: usize) -> usize {
    text.bytes().take(byte).filter(|&b| b == b'\n').count().saturating_add(1)
}

// ---------------------------------------------------------------- resolution

impl Config {
    /// Built-in defaults.
    #[must_use]
    pub fn defaults(schemas: &[ModuleSchema]) -> Self {
        let mut errors = Vec::new();
        resolve(&RawConfig::default(), schemas, &mut errors)
    }

    /// Effective width for rendering: `COLUMNS` (or `GARNISH_COLUMNS`) minus padding.
    #[must_use]
    pub fn width(&self, columns: Option<usize>) -> usize {
        columns.unwrap_or(120).saturating_sub(self.padding).max(10)
    }

    /// Separator for a line.
    #[must_use]
    pub fn separator<'a>(&'a self, line: &'a LineCfg) -> &'a str {
        line.separator.as_deref().unwrap_or(&self.frame.chars.separator)
    }
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
    let mut overrides: BTreeMap<Role, Color> = BTreeMap::new();
    for (k, v) in &raw.colors {
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

    Config {
        preset,
        icons,
        theme_name,
        theme,
        color: raw.color.unwrap_or_default(),
        truncate: raw.truncate.unwrap_or(true),
        stale_style: raw.stale_style.unwrap_or_default(),
        padding: usize::from(raw.padding.unwrap_or(0)),
        frame,
        lines,
        modules,
    }
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
        assert_eq!(c.width(Some(100)), 100);
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
        assert_eq!(c.width(Some(100)), 98);
    }

    #[test]
    fn errors_have_paths_and_fall_back_to_defaults() {
        let text = r#"
theme = "solarized"
[colors]
accent = "bogus"
nope = "red"
[[line]]
modules = ["path", "ghost"]
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
        assert!(paths.contains(&"theme"), "{paths:?}");
        assert!(paths.contains(&"colors.accent"));
        assert!(paths.contains(&"colors.nope"));
        assert!(paths.contains(&"line[0].modules[1]"));
        assert!(paths.contains(&"modules.path.depth"));
        assert!(paths.contains(&"modules.path.wat"));
        assert!(paths.contains(&"modules.path.icons.nope"));
        assert!(paths.contains(&"modules.clock.format"));
        assert!(paths.contains(&"modules.ghost"));
        // defaults in effect
        assert_eq!(c, Config::defaults(&schemas()));
    }

    #[test]
    fn syntax_errors_carry_a_line() {
        let (_, errs) = parse("preset = \"default\"\n[frame\nstyle = 1", &schemas());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, Some(2));
        assert!(errs[0].to_string().starts_with("line 2: "));
        let (_, errs) = parse("unknown_top = 1", &schemas());
        assert!(errs[0].message.contains("unknown field"));
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
