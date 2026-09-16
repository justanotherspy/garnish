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
    /// Upper bound: the largest integer a [`Kind::Int`] option accepts, or
    /// the most characters a [`Kind::Str`] option may hold. A value above it
    /// is reported at config time and the default stands in. Set on every
    /// option whose value sizes an allocation or a loop on the tick (cell
    /// counts, row text, decimal places), so the cap is part of the
    /// reference docs rather than a rule buried in the parser.
    pub max: Option<usize>,
}

impl OptSpec {
    /// Option with the same value in every preset.
    #[must_use]
    pub const fn new(key: &'static str, kind: Kind, doc: &'static str, default: Value) -> Self {
        Self { key, kind, doc, default, minimal: None, full: None, max: None }
    }

    /// Bound the option (see [`OptSpec::max`]).
    #[must_use]
    pub const fn max(mut self, max: usize) -> Self {
        self.max = Some(max);
        self
    }

    /// Why `value` exceeds [`OptSpec::max`], if it does. Anything else
    /// passes (a negative integer never gets here: the coercion to
    /// [`Kind::Int`] rejects it first).
    #[must_use]
    pub fn over_max(&self, value: &Value) -> Option<String> {
        let max = self.max?;
        match value {
            // `is_none_or`, not `is_ok_and`: a value too large for `usize`
            // (a 32-bit build) is over any `max` by definition, and letting
            // it through is what SPEC § 5 records as an aborted tick.
            Value::Int(n) if usize::try_from(*n).ok().is_none_or(|n| n > max) => {
                Some(format!("must be at most {max}"))
            }
            Value::Str(s) if s.chars().count() > max => {
                Some(format!("must be at most {max} characters"))
            }
            _ => None,
        }
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

impl IconSpec {
    /// Whether an override for this key has to be exactly one cell wide.
    ///
    /// True for the glyphs [`crate::modules::util::bar`] repeats cell by
    /// cell: a wider one would break the width arithmetic of the whole row,
    /// so `bar` substitutes a safe glyph and the user's choice vanishes with
    /// nothing said. The config reports it instead, as it does for
    /// `frame.fill_char`, which is the same rule for the rule's own glyph.
    /// A unit test pins the vocabulary: a schema that declares one of these
    /// keys declares a one-cell glyph in every icon set.
    #[must_use]
    pub fn one_cell(&self) -> bool {
        ONE_CELL_ICONS.contains(&self.key)
    }

    /// True when blanking this glyph is how the user turns the thing off.
    ///
    /// An empty glyph means "draw nothing" everywhere in garnish, and the
    /// marker is the one bar glyph that can be left out: `util::bar` skips
    /// it and the bar is still the same width. `fill` and `empty` are the
    /// cells themselves, so blanking either would collapse the row.
    #[must_use]
    pub fn may_be_blank(&self) -> bool {
        self.key == "marker"
    }
}

/// The icon keys that are drawn one per bar cell (see [`IconSpec::one_cell`]).
pub const ONE_CELL_ICONS: [&str; 3] = ["fill", "empty", "marker"];

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

/// The keys every module accepts besides its own options, in the order the
/// "expected one of" message names them.
///
/// Derived from [`COMMON_OPTS`] rather than listed again, so adding a common
/// option cannot leave it out of the message: only the three hand-parsed
/// keys and the two tables are spelled here.
pub fn common_keys() -> impl Iterator<Item = &'static str> {
    const HAND_PARSED: [&str; 3] = ["enabled", "preset", "refresh"];
    HAND_PARSED.into_iter().chain(COMMON_OPTS.iter().map(|o| o.key)).chain(std::iter::once("icons"))
}

/// The common options every module takes besides its own, as specs.
///
/// The parser bounds them like any option ([`OptSpec::max`]) and the
/// reference prints them with their caps (SPEC § 3, § 5). `enabled`,
/// `preset` and `refresh` stay hand-parsed: a preset is a name and
/// `refresh` depends on whether the module is cached. Text modules
/// (SPEC § 3.7) take every entry but `max_width`, which `config check`
/// rejects there in favour of `width`.
pub static COMMON_OPTS: [OptSpec; 5] = [
    OptSpec::new("label", Kind::Str, "Dim text before the value.", Value::Str(String::new()))
        .max(crate::config::MAX_TEXT_CHARS),
    OptSpec::new("prefix", Kind::Str, "Text before the module.", Value::Str(String::new()))
        .max(crate::config::MAX_TEXT_CHARS),
    OptSpec::new("suffix", Kind::Str, "Text after the module.", Value::Str(String::new()))
        .max(crate::config::MAX_TEXT_CHARS),
    OptSpec::new(
        "hide_when_empty",
        Kind::Bool,
        "Hide the module when it has nothing to show (else a dim `–`).",
        Value::Bool(true),
    ),
    OptSpec::new(
        "max_width",
        Kind::Int,
        "Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited.",
        Value::Int(0),
    )
    .max(crate::config::MAX_CELLS),
];

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
    /// Cells the decorated module is cut to with the ellipsis; 0 = unlimited
    /// (SPEC § 3). Always 0 for a text module, which has `width` instead.
    pub max_width: usize,
    opts: BTreeMap<&'static str, Value>,
    icons: BTreeMap<&'static str, String>,
    /// Frames an icon cycles through when animations run (`<key>_frames`);
    /// absent keys keep their static glyph.
    icon_frames: BTreeMap<&'static str, Vec<String>>,
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
            // The ASCII set stays ASCII: `=`/`-` instead of `━`/`─`.
            let (fill, empty) =
                if icon_set == IconSet::Ascii { ("=", "-") } else { ("━", "─") };
            for (key, glyph) in [("fill", fill), ("empty", empty)] {
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
            max_width: crate::num::u64_to_usize(overrides.max_width.unwrap_or(0)),
            opts,
            icons,
            icon_frames: schema
                .icons
                .iter()
                .filter_map(|i| overrides.icon_frames.get(i.key).map(|f| (i.key, f.clone())))
                .collect(),
            colors,
            schema: schema.clone(),
        }
    }

    /// The animation frames of an icon (`<key>_frames`); empty when static.
    #[must_use]
    pub fn icon_frames(&self, key: &str) -> &[String] {
        self.icon_frames.get(key).map_or(&[], Vec::as_slice)
    }

    /// Every icon that has animation frames, for `config show` and docs.
    #[must_use]
    pub const fn all_icon_frames(&self) -> &BTreeMap<&'static str, Vec<String>> {
        &self.icon_frames
    }

    /// This config as seen at one tick: every icon that has frames shows
    /// `frames[frame_of(frames.len())]` (SPEC § 4.2). Borrowed when nothing
    /// animates, so the common case costs nothing.
    #[must_use]
    pub fn animated(&self, frame_of: impl Fn(usize) -> usize) -> std::borrow::Cow<'_, Self> {
        if self.icon_frames.is_empty() {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut view = self.clone();
        for (key, frames) in &self.icon_frames {
            if let Some(frame) = frames.get(frame_of(frames.len()))
                && let Some(slot) = view.icons.get_mut(key)
            {
                frame.clone_into(slot);
            }
        }
        std::borrow::Cow::Owned(view)
    }

    /// The schema this config was resolved from.
    #[must_use]
    pub const fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    /// The resolved value of a [`COMMON_OPTS`] key, for `config show` and
    /// the docs (`None` for a key that is not a common option).
    #[must_use]
    pub fn common(&self, key: &str) -> Option<Value> {
        match key {
            "label" => Some(Value::Str(self.label.clone())),
            "prefix" => Some(Value::Str(self.prefix.clone())),
            "suffix" => Some(Value::Str(self.suffix.clone())),
            "hide_when_empty" => Some(Value::Bool(self.hide_when_empty)),
            "max_width" => Some(Value::Int(i64::try_from(self.max_width).unwrap_or(i64::MAX))),
            _ => None,
        }
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
    /// `max_width` (cells; 0 = unlimited).
    pub max_width: Option<u64>,
    /// Module-specific options.
    pub opts: BTreeMap<String, Value>,
    /// Icon overrides.
    pub icons: BTreeMap<String, String>,
    /// Color overrides (unresolved specs).
    pub colors: BTreeMap<String, String>,
    /// `<key>_frames`: animation frames for an icon key (SPEC § 4.2).
    pub icon_frames: BTreeMap<String, Vec<String>>,
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
        // Frames: the animated view swaps the glyph per tick; borrowed when static.
        assert!(matches!(cfg.animated(|_| 1), std::borrow::Cow::Borrowed(_)));
        o.icon_frames.insert("leaf".into(), vec!["a".into(), "b".into(), "c".into()]);
        o.icon_frames.insert("ghost".into(), vec!["x".into()]);
        let cfg = ModuleCfg::resolve(&s, Preset::Full, IconSet::Nerd, &theme, &o);
        assert_eq!(cfg.icon_frames("leaf"), ["a", "b", "c"]);
        assert!(cfg.icon_frames("ghost").is_empty(), "unknown keys are dropped");
        assert_eq!(cfg.animated(|n| 2 % n).icon("leaf"), "c");
        assert_eq!(cfg.animated(|_| 0).icon("leaf"), "a");
        assert_eq!(cfg.icon("leaf"), "🌿", "the static glyph is untouched");
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
