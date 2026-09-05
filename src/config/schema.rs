//! Declarative option schemas.
//!
//! Every module describes its options, icons and colors with a
//! [`ModuleSchema`]. The same description drives config validation, preset
//! resolution, `garnish config init`, and the generated docs, so the three can
//! never disagree.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ansi::Color;
use crate::icons::{Glyph, IconSet};
use crate::theme::Theme;

/// A configuration value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Boolean.
    Bool(bool),
    /// Integer (validated non-negative when the option says so).
    Int(i64),
    /// Float.
    Float(f64),
    /// String.
    Str(String),
    /// List of strings.
    StrList(Vec<String>),
    /// List of numbers.
    NumList(Vec<f64>),
}

impl Value {
    /// String value, if any.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Render for docs / `config show`, in TOML syntax.
    #[must_use]
    pub fn to_toml(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => format_float(*f),
            Self::Str(s) => toml_string(s),
            Self::StrList(v) => {
                format!("[{}]", v.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "))
            }
            Self::NumList(v) => {
                format!("[{}]", v.iter().map(|f| format_float(*f)).collect::<Vec<_>>().join(", "))
            }
        }
    }
}

/// Quote a string as a TOML basic string (UTF-8 kept verbatim, controls escaped).
#[must_use]
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_add(2));
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        "nan".into()
    } else if f.is_infinite() {
        if f > 0.0 { "inf".into() } else { "-inf".into() }
    } else if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{f:.0}")
    } else {
        f.to_string()
    }
}

/// The kind of an option, used for validation and docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `true` / `false`.
    Bool,
    /// Non-negative integer.
    Int,
    /// Number.
    Float,
    /// Free-form string.
    Str,
    /// One of a fixed set of strings.
    Enum(&'static [&'static str]),
    /// List of strings.
    StrList,
    /// List of numbers.
    NumList,
    /// List of color specs (role names or literal colors).
    ColorList,
}

impl Kind {
    /// Human name for docs.
    #[must_use]
    pub fn doc_name(self) -> String {
        match self {
            Self::Bool => "bool".into(),
            Self::Int => "integer".into(),
            Self::Float => "number".into(),
            Self::Str => "string".into(),
            Self::Enum(vals) => {
                vals.iter().map(|v| format!("`{v}`")).collect::<Vec<_>>().join(" \\| ")
            }
            Self::StrList => "list of strings".into(),
            Self::NumList => "list of numbers".into(),
            Self::ColorList => "list of colors".into(),
        }
    }
}

/// One module option.
#[derive(Debug, Clone, PartialEq)]
pub struct OptSpec {
    /// TOML key under `[modules.<id>]`.
    pub key: &'static str,
    /// Type.
    pub kind: Kind,
    /// One-line documentation.
    pub doc: &'static str,
    /// Value for the `default` preset (and the fallback for everything).
    pub default: Value,
    /// Override for the `minimal` preset.
    pub minimal: Option<Value>,
    /// Override for the `full` preset.
    pub full: Option<Value>,
}

impl OptSpec {
    /// Option with the same value in every preset.
    #[must_use]
    pub const fn new(key: &'static str, kind: Kind, doc: &'static str, default: Value) -> Self {
        Self { key, kind, doc, default, minimal: None, full: None }
    }

    /// Set the `minimal` preset value.
    #[must_use]
    pub fn minimal(mut self, v: Value) -> Self {
        self.minimal = Some(v);
        self
    }

    /// Set the `full` preset value.
    #[must_use]
    pub fn full(mut self, v: Value) -> Self {
        self.full = Some(v);
        self
    }

    /// Value for a preset.
    #[must_use]
    pub fn for_preset(&self, preset: Preset) -> &Value {
        match preset {
            Preset::Minimal => self.minimal.as_ref().unwrap_or(&self.default),
            Preset::Default => &self.default,
            Preset::Full => self.full.as_ref().unwrap_or(&self.default),
        }
    }
}

/// One icon the module uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconSpec {
    /// Key under `[modules.<id>.icons]`.
    pub key: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Default glyph per icon set.
    pub glyph: Glyph,
}

/// One color the module uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSpec {
    /// Key under `[modules.<id>.colors]`.
    pub key: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Default: a theme role name or a literal color.
    pub default: &'static str,
}

/// Module presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Preset {
    /// Bare minimum.
    Minimal,
    /// Sensible default.
    #[default]
    Default,
    /// Everything the module can show.
    Full,
}

impl Preset {
    /// All presets in documentation order.
    pub const ALL: [Self; 3] = [Self::Minimal, Self::Default, Self::Full];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Default => "default",
            Self::Full => "full",
        }
    }

    /// Parse a config name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }
}

/// Everything there is to know about a module's configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleSchema {
    /// Module id (`[modules.<id>]`, and the name used in `[[line]]`).
    pub id: &'static str,
    /// One-line summary.
    pub summary: &'static str,
    /// Longer description (markdown).
    pub doc: &'static str,
    /// Where the data comes from (payload fields, git, settings…).
    pub sources: &'static [&'static str],
    /// Default refresh interval in seconds; 0 = payload-only, every tick.
    pub refresh: u64,
    /// Module-specific options.
    pub opts: Vec<OptSpec>,
    /// Icons.
    pub icons: Vec<IconSpec>,
    /// Colors.
    pub colors: Vec<ColorSpec>,
}

impl ModuleSchema {
    /// Find an option spec.
    #[must_use]
    pub fn opt(&self, key: &str) -> Option<&OptSpec> {
        self.opts.iter().find(|o| o.key == key)
    }

    /// Find an icon spec.
    #[must_use]
    pub fn icon(&self, key: &str) -> Option<&IconSpec> {
        self.icons.iter().find(|i| i.key == key)
    }

    /// Find a color spec.
    #[must_use]
    pub fn color(&self, key: &str) -> Option<&ColorSpec> {
        self.colors.iter().find(|c| c.key == key)
    }
}

/// Keys every module accepts in addition to its own options.
pub const COMMON_KEYS: [&str; 8] =
    ["enabled", "preset", "refresh", "label", "prefix", "suffix", "hide_when_empty", "icons"];

/// The fully resolved configuration of one module instance.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleCfg {
    /// Module id.
    pub id: &'static str,
    /// Enabled.
    pub enabled: bool,
    /// Preset in effect.
    pub preset: Preset,
    /// Refresh interval in seconds (0 = every tick).
    pub refresh: u64,
    /// Optional label shown before the value.
    pub label: String,
    /// Text prepended to the rendered module.
    pub prefix: String,
    /// Text appended to the rendered module.
    pub suffix: String,
    /// Hide the module when it has nothing to say.
    pub hide_when_empty: bool,
    opts: BTreeMap<&'static str, Value>,
    icons: BTreeMap<&'static str, String>,
    colors: BTreeMap<&'static str, Color>,
    schema: ModuleSchema,
}

impl ModuleCfg {
    /// Resolve a schema with a preset, icon set and theme, then apply explicit
    /// overrides (already validated by `config`).
    #[must_use]
    pub fn resolve(
        schema: &ModuleSchema,
        preset: Preset,
        icon_set: IconSet,
        theme: &Theme,
        overrides: &Overrides,
    ) -> Self {
        let opts: BTreeMap<&'static str, Value> = schema
            .opts
            .iter()
            .map(|o| {
                let v = overrides
                    .opts
                    .get(o.key)
                    .cloned()
                    .unwrap_or_else(|| o.for_preset(preset).clone());
                (o.key, v)
            })
            .collect();
        let mut icons: BTreeMap<&'static str, String> = schema
            .icons
            .iter()
            .map(|i| {
                let v = overrides
                    .icons
                    .get(i.key)
                    .cloned()
                    .unwrap_or_else(|| i.glyph.get(icon_set).to_owned());
                (i.key, v)
            })
            .collect();
        // `bar = "line"` is a shorthand for the line glyphs (SPEC § 4.1); it
        // is applied here so the resolved icons are what renders and what
        // `config show` prints. An explicit icon override still wins.
        if matches!(opts.get("bar"), Some(Value::Str(style)) if style == "line") {
            for (key, glyph) in [("fill", "━"), ("empty", "─")] {
                if !overrides.icons.contains_key(key)
                    && let Some(slot) = icons.get_mut(key)
                {
                    glyph.clone_into(slot);
                }
            }
        }
        let colors = schema
            .colors
            .iter()
            .map(|c| {
                let v = overrides
                    .colors
                    .get(c.key)
                    .and_then(|s| theme.resolve(s))
                    .or_else(|| theme.resolve(c.default))
                    .unwrap_or_default();
                (c.key, v)
            })
            .collect();
        Self {
            id: schema.id,
            enabled: overrides.enabled.unwrap_or(true),
            preset,
            refresh: overrides.refresh.unwrap_or(schema.refresh),
            label: overrides.label.clone().unwrap_or_default(),
            prefix: overrides.prefix.clone().unwrap_or_default(),
            suffix: overrides.suffix.clone().unwrap_or_default(),
            hide_when_empty: overrides.hide_when_empty.unwrap_or(true),
            opts,
            icons,
            colors,
            schema: schema.clone(),
        }
    }

    /// The schema this config was resolved from.
    #[must_use]
    pub const fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    /// Raw option value.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<&Value> {
        self.opts.get(key)
    }

    /// Boolean option (false when missing or of another kind).
    #[must_use]
    pub fn bool(&self, key: &str) -> bool {
        matches!(self.opts.get(key), Some(Value::Bool(true)))
    }

    /// Integer option as `u64` (0 when missing/negative).
    #[must_use]
    pub fn int(&self, key: &str) -> u64 {
        match self.opts.get(key) {
            Some(Value::Int(i)) => u64::try_from(*i).unwrap_or(0),
            Some(Value::Float(f)) => crate::num::round_to_u64(*f),
            _ => 0,
        }
    }

    /// Integer option as `usize`.
    #[must_use]
    pub fn size(&self, key: &str) -> usize {
        crate::num::u64_to_usize(self.int(key))
    }

    /// Float option (0.0 when missing).
    #[must_use]
    pub fn float(&self, key: &str) -> f64 {
        match self.opts.get(key) {
            Some(Value::Float(f)) => *f,
            Some(Value::Int(i)) => crate::num::u64_to_f64(u64::try_from(*i).unwrap_or(0)),
            _ => 0.0,
        }
    }

    /// String option ("" when missing).
    #[must_use]
    pub fn str(&self, key: &str) -> &str {
        self.opts.get(key).and_then(Value::as_str).unwrap_or("")
    }

    /// Number-list option.
    #[must_use]
    pub fn nums(&self, key: &str) -> Vec<f64> {
        match self.opts.get(key) {
            Some(Value::NumList(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    /// String-list option.
    #[must_use]
    pub fn strs(&self, key: &str) -> Vec<String> {
        match self.opts.get(key) {
            Some(Value::StrList(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    /// Color-list option resolved through the theme.
    #[must_use]
    pub fn color_list(&self, key: &str, theme: &Theme) -> Vec<Color> {
        self.strs(key).iter().filter_map(|s| theme.resolve(s)).collect()
    }

    /// Icon glyph ("" when unknown).
    #[must_use]
    pub fn icon(&self, key: &str) -> &str {
        self.icons.get(key).map_or("", String::as_str)
    }

    /// Color (default color when unknown).
    #[must_use]
    pub fn color(&self, key: &str) -> Color {
        self.colors.get(key).copied().unwrap_or_default()
    }

    /// Every resolved option, for `config show` and docs.
    #[must_use]
    pub const fn opts(&self) -> &BTreeMap<&'static str, Value> {
        &self.opts
    }

    /// Every resolved icon.
    #[must_use]
    pub const fn icons(&self) -> &BTreeMap<&'static str, String> {
        &self.icons
    }

    /// Every resolved color.
    #[must_use]
    pub const fn colors(&self) -> &BTreeMap<&'static str, Color> {
        &self.colors
    }
}

/// Explicit per-module overrides from the config file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    /// `enabled`.
    pub enabled: Option<bool>,
    /// `preset`.
    pub preset: Option<Preset>,
    /// `refresh`.
    pub refresh: Option<u64>,
    /// `label`.
    pub label: Option<String>,
    /// `prefix`.
    pub prefix: Option<String>,
    /// `suffix`.
    pub suffix: Option<String>,
    /// `hide_when_empty`.
    pub hide_when_empty: Option<bool>,
    /// Module-specific options.
    pub opts: BTreeMap<String, Value>,
    /// Icon overrides.
    pub icons: BTreeMap<String, String>,
    /// Color overrides (unresolved specs).
    pub colors: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::glyph;

    fn schema() -> ModuleSchema {
        ModuleSchema {
            id: "demo",
            summary: "demo",
            doc: "",
            sources: &[],
            refresh: 5,
            opts: vec![
                OptSpec::new("width", Kind::Int, "w", Value::Int(20))
                    .minimal(Value::Int(10))
                    .full(Value::Int(30)),
                OptSpec::new("show", Kind::Bool, "s", Value::Bool(false)).full(Value::Bool(true)),
                OptSpec::new(
                    "bands",
                    Kind::ColorList,
                    "b",
                    Value::StrList(vec!["ok".into(), "#ff0000".into()]),
                ),
            ],
            icons: vec![IconSpec { key: "leaf", doc: "", glyph: glyph("N", "U", "E", "A") }],
            colors: vec![ColorSpec { key: "main", doc: "", default: "accent" }],
        }
    }

    #[test]
    fn presets_and_overrides_resolve_in_order() {
        let theme = Theme::default();
        let s = schema();
        let cfg =
            ModuleCfg::resolve(&s, Preset::Minimal, IconSet::Ascii, &theme, &Overrides::default());
        assert_eq!(cfg.int("width"), 10);
        assert!(!cfg.bool("show"));
        assert_eq!(cfg.icon("leaf"), "A");
        assert_eq!(cfg.refresh, 5);
        assert_eq!(cfg.color("main"), theme.role(crate::theme::Role::Accent));

        let mut o = Overrides { refresh: Some(0), ..Default::default() };
        o.opts.insert("width".into(), Value::Int(7));
        o.icons.insert("leaf".into(), "🌿".into());
        o.colors.insert("main".into(), "#010203".into());
        let cfg = ModuleCfg::resolve(&s, Preset::Full, IconSet::Nerd, &theme, &o);
        assert_eq!(cfg.int("width"), 7);
        assert!(cfg.bool("show"));
        assert_eq!(cfg.icon("leaf"), "🌿");
        assert_eq!(cfg.refresh, 0);
        assert_eq!(cfg.color("main"), Color::Rgb(1, 2, 3));
        assert_eq!(cfg.color_list("bands", &theme).len(), 2);
        assert_eq!(cfg.size("width"), 7);
        assert_eq!(cfg.str("missing"), "");
        assert_eq!(cfg.float("width"), 7.0);
    }

    #[test]
    fn value_toml_rendering() {
        assert_eq!(Value::Bool(true).to_toml(), "true");
        assert_eq!(Value::Int(3).to_toml(), "3");
        assert_eq!(Value::Float(2.5).to_toml(), "2.5");
        assert_eq!(Value::Float(50.0).to_toml(), "50");
        assert_eq!(Value::Float(f64::NAN).to_toml(), "nan");
        assert_eq!(Value::Float(f64::NEG_INFINITY).to_toml(), "-inf");
        assert_eq!(Value::NumList(vec![f64::INFINITY]).to_toml(), "[inf]");
        assert_eq!(Value::Str("a\"b".into()).to_toml(), "\"a\\\"b\"");
        assert_eq!(Value::Str("\u{f06a9}\\".into()).to_toml(), "\"\u{f06a9}\\\\\"");
        assert_eq!(Value::StrList(vec!["x".into()]).to_toml(), "[\"x\"]");
        assert_eq!(Value::NumList(vec![50.0, 75.5]).to_toml(), "[50, 75.5]");
        assert_eq!(Preset::parse("full"), Some(Preset::Full));
        assert!(Kind::Enum(&["a", "b"]).doc_name().contains("`a`"));
    }
}
