//! Configuration: loading the TOML file, validating it against the module
//! schemas, applying presets, and producing a fully resolved [`Config`].
//!
//! The file is read as a plain TOML table and each key converted on its
//! own (SPEC § 5). This module owns the top level and the resolution; the
//! parts live beside it: `load` (finding and reading the file), `rows`
//! (rows, columns and boxes), `frame` (the `[frame]` table), `overrides`
//! (the module tables) and `read` (the one-value readers they share).

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::ansi::{Color, ColorMode};
use crate::icons::IconSet;
use crate::theme::{PALETTES, Role, Theme, palette};
use crate::time::DurationStyle;

pub mod format;
pub mod presets;
pub mod schema;

mod frame;
mod load;
mod overrides;
mod read;
mod rows;
mod vocab;

#[cfg(test)]
pub(crate) use frame::FRAME_KEYS;
pub use frame::{FillDirection, FrameCfg, SeparatorColor};
pub use load::{
    CONFIG_ENV, ReadTarget, WriteTarget, default_path, explicit, load, load_with, locate, parse,
    parse_table, parse_with, read_target, syntax_error, write_target,
};
pub(crate) use load::{env_path, xdg_base};
pub(crate) use overrides::text_takes;
pub(crate) use read::is_bare_key;
pub use rows::{
    BoxCfg, BoxRef, ColCfg, DEFAULT_GAP, DEFAULT_TITLE_PAD, Justify, MAX_COLS, MAX_FR, MAX_GAP,
    MAX_TITLE_PAD, RowCfg, TitleCfg, VAlign, Width,
};
pub use vocab::Vocab;

use format::{CostStyle, FormatCfg, ParensStyle, PercentStyle, TokenStyle};
use frame::RawFrame;
use presets::TopPreset;
use read::{enum_field, field, problem, string_table};
use rows::RawRow;
use schema::{ModuleCfg, ModuleSchema, Overrides};

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

/// What a one-line summary naming the first of `count` problems ends in:
/// ` (+N more)` for the rest, or nothing. The `⚠ config:` row and the
/// setup screen's status line both summarise this way.
#[must_use]
pub fn more(count: usize) -> String {
    match count.saturating_sub(1) {
        0 => String::new(),
        extra => format!(" (+{extra} more)"),
    }
}

/// Stale-value styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// Every style, in the order the reference lists them.
    pub const ALL: [Self; 3] = [Self::Dim, Self::Hide, Self::Plain];

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightJustify {
    /// Pad on the left: the text hugs the right cap.
    #[default]
    End,
    /// Pad on the right: the text follows the separator, the gap sits before the cap.
    Start,
}

impl RightJustify {
    /// Both sides, in the order the reference lists them.
    pub const ALL: [Self; 2] = [Self::End, Self::Start];

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    /// Cut it with the ellipsis.
    #[default]
    Truncate,
    /// Scroll it: a window that advances `ticker_step` cells per tick and
    /// wraps around with `ticker_gap` between the end and the start.
    Ticker,
}

impl Overflow {
    /// Both behaviours, in the order the reference lists them.
    pub const ALL: [Self; 2] = [Self::Truncate, Self::Ticker];

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorChoice {
    /// Truecolor unless `NO_COLOR` is set and not empty.
    #[default]
    Auto,
    /// Always truecolor.
    Always,
    /// Never.
    Never,
    /// 256-color palette.
    Ansi256,
    /// 24-bit color.
    TrueColor,
}

impl ColorChoice {
    /// Every choice, in the order the reference lists them.
    pub const ALL: [Self; 5] =
        [Self::Auto, Self::Always, Self::Never, Self::Ansi256, Self::TrueColor];

    /// The choice a config name stands for.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

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

    /// Resolve to a concrete mode given the environment ([`no_color_env`]).
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

/// Whether `NO_COLOR` asks for no colour: set *and not empty*, as
/// no-color.org defines it (an empty value is the shell's "unset", the
/// rule garnish applies to every path variable too).
#[must_use]
pub fn no_color_env() -> bool {
    no_color_from(std::env::var_os("NO_COLOR").as_deref())
}

/// [`no_color_env`] for an explicit value of the variable.
#[must_use]
pub fn no_color_from(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty())
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

/// Longest string a config may put on a row, in characters: a status line,
/// not a document.
///
/// It caps a text module's `text`, `gap` and `url`, `label`, `prefix`,
/// `suffix`, a row or box `title` and `ticker_gap`: the schema `max` of the
/// module options, checked by hand for `title` and `ticker_gap`.
pub const MAX_TEXT_CHARS: usize = 4096;

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
    /// Cells the ticker advances per tick (within [`STEP_RANGE`]; 0.5 =
    /// every second tick).
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
    /// The command line's preset replaced the file's rows (`preview
    /// --preset`), so no `[box.<name>]` of the file is expected to be joined.
    rows_replaced: bool,
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

/// Default `ticker_gap`: three blanks between the end of a scrolled group and its start.
pub const DEFAULT_TICKER_GAP: &str = "   ";
/// The theme a file without `theme` gets.
pub const DEFAULT_THEME: &str = "garnish";
/// `stale_after` when the file does not set it (SPEC § 3.6).
pub const DEFAULT_STALE_AFTER: u32 = 5;

impl RawConfig {
    /// The array name the file used, so an error points at what was typed.
    const fn rows_key(&self) -> &'static str {
        if self.rows_alias { "line" } else { "row" }
    }

    // The table is taken by value so every field moves into place: cloning
    // each value cost a fifth of the parse on the full annotated file.
    fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut raw = Self::default();
        // The two array names are reconciled after the loop: TOML gives no
        // order between two arrays of tables, so a file carries one or the
        // other (SPEC § 4.3).
        let (mut rows, mut alias): (Option<Vec<RawRow>>, Option<Vec<RawRow>>) = (None, None);
        // The same for the two switch names, so a bad value under the new
        // one never erases a good alias written before it.
        let (mut hide_rows, mut hide_lines): (Option<bool>, Option<bool>) = (None, None);
        for (key, value) in table {
            match key.as_str() {
                "preset" => raw.preset = enum_field(&key, &value, errors),
                "icons" => raw.icons = enum_field(&key, &value, errors),
                "theme" => raw.theme = field(&key, value, errors),
                "color" => raw.color = enum_field(&key, &value, errors),
                "truncate" => raw.truncate = field(&key, value, errors),
                "stale_style" => raw.stale_style = enum_field(&key, &value, errors),
                "stale_after" => raw.stale_after = field(&key, value, errors),
                "padding" => raw.padding = field(&key, value, errors),
                "align" => raw.align = field(&key, value, errors),
                "right_justify" => raw.right_justify = enum_field(&key, &value, errors),
                // `hide_empty_lines` is the permanent alias of
                // `hide_empty_rows` (SPEC § 4.3); the new name wins when a
                // file carries both.
                "hide_empty_rows" => hide_rows = field(&key, value, errors),
                "hide_empty_lines" => hide_lines = field(&key, value, errors),
                "overflow" => raw.overflow = enum_field(&key, &value, errors),
                "ticker_step" => raw.ticker_step = field(&key, value, errors),
                "ticker_gap" => raw.ticker_gap = field(&key, value, errors),
                "animate" => raw.animate = field(&key, value, errors),
                "durations" => raw.durations = enum_field(&key, &value, errors),
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
                "row" => rows = Some(rows::row_array("row", value, errors)),
                "line" => alias = Some(rows::row_array("line", value, errors)),
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
        raw.hide_empty_rows = hide_rows.or(hide_lines);
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

/// The `[format]` table as written (SPEC § 4, Number formats), each key
/// reported and defaulted on its own like the rest of the file.
#[derive(Debug, Default)]
struct RawFormat {
    tokens: Option<TokenStyle>,
    percent: Option<PercentStyle>,
    cost: Option<CostStyle>,
    parens: Option<ParensStyle>,
}

/// Every key `[format]` takes.
const FORMAT_KEYS: [&str; 4] = ["tokens", "percent", "cost", "parens"];

impl RawFormat {
    fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut f = Self::default();
        for (key, value) in table {
            let path = format!("format.{key}");
            match key.as_str() {
                "tokens" => f.tokens = enum_field(&path, &value, errors),
                "percent" => f.percent = enum_field(&path, &value, errors),
                "cost" => f.cost = enum_field(&path, &value, errors),
                "parens" => f.parens = enum_field(&path, &value, errors),
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
        let path = format!("colors.{k}");
        match (Role::parse(k), read::literal_color(v)) {
            (Some(role), Ok(color)) => {
                overrides.insert(role, color);
            }
            (None, _) => errors.push(problem(
                &path,
                &format!("unknown color role; expected one of {}", Role::choices()),
            )),
            (_, Err(message)) => errors.push(problem(&path, &message)),
        }
    }
    overrides
}

fn resolve(raw: &RawConfig, schemas: &[ModuleSchema], errors: &mut Vec<ConfigError>) -> Config {
    let preset = raw.preset.unwrap_or_default();
    let icons = raw.icons.unwrap_or_default();

    let requested = raw.theme.clone().unwrap_or_else(|| DEFAULT_THEME.to_owned());
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

    let frame = frame::resolve_frame(raw.frame.as_ref(), preset, &theme, errors);
    let boxes = rows::resolve_boxes(&raw.boxes, &theme, errors);
    let mut rows: Vec<RowCfg> = if raw.row.is_empty() {
        preset.rows()
    } else {
        rows::resolve_rows(raw.rows_key(), &raw.row, &boxes, &theme, errors)
    };
    // A box nothing joins draws nothing: said once, here, rather than left
    // for the user to wonder about on screen.
    for name in boxes.keys().filter(|_| !raw.rows_replaced) {
        if !rows.iter().any(|r| rows::joins_box(r, name)) {
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
        let ov = table.map_or_else(Overrides::default, |t| {
            overrides::parse_overrides(schema, &base, t, errors)
        });
        let module_preset = ov.preset.unwrap_or_else(|| preset.module_preset());
        modules.insert(schema.id, ModuleCfg::resolve(schema, module_preset, icons, &theme, &ov));
    }
    let texts = overrides::resolve_texts(raw.modules.get("text"), icons, &theme, errors);
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
        rows::check_row_ids(raw.rows_key(), &mut rows, schemas, &texts, errors);
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
    let stale_after = raw.unwrap_or(DEFAULT_STALE_AFTER);
    if stale_after == 0 {
        errors.push(ConfigError {
            path: "stale_after".into(),
            message: "must be at least 1 (TTL periods before a value is styled stale)".into(),
            line: None,
        });
    }
    stale_after.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ColorSpec, IconSpec, Kind, OptSpec, Preset, Value};
    use crate::frame::FrameStyle;
    use crate::icons::glyph;

    /// Two small schemas the unit tests of every part of the parser share: a
    /// payload-only `path` with one option, icon and colour, and a `clock`
    /// with one enum option.
    pub(super) fn schemas() -> Vec<ModuleSchema> {
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
        assert_eq!(c.separator_at(&c.rows[0], 0), "  ");
        let path = c.modules.get("path").unwrap();
        assert_eq!(path.preset, Preset::Full);
        assert_eq!(path.int("depth"), 3);
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

    /// x-10: one owner for the summary's tail and for a problem's text, so
    /// the setup screen says what the `⚠ config:` row says.
    #[test]
    fn a_summary_counts_the_rest_and_a_problem_prints_its_path_once() {
        assert_eq!(
            (more(0), more(1), more(3)),
            (String::new(), String::new(), " (+2 more)".into())
        );
        let at = |path: &str| ConfigError { path: path.into(), message: "bad".into(), line: None };
        assert_eq!((at("gap").to_string(), at("").to_string()), ("gap: bad".into(), "bad".into()));
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

    /// cfg-10: every key list is walked, so a key added to a match arm but
    /// not to its message (or the reverse) fails here. Each listed key is
    /// written with a value of the wrong type, which its own arm must
    /// refuse as a value, never as an unknown key; a key that is not in the
    /// list is refused naming the whole list.
    #[test]
    fn every_key_list_is_the_keys_its_table_takes() {
        type Text = fn(&str) -> String;
        let lists: [(&[&str], Text, &str); 7] = [
            (&TOP_KEYS, |k| format!("{k} = {{}}\n"), ""),
            (&frame::FRAME_KEYS, |k| format!("[frame]\n{k} = {{}}\n"), "frame."),
            (&FORMAT_KEYS, |k| format!("[format]\n{k} = {{}}\n"), "format."),
            (&rows::ROW_KEYS, |k| format!("[[row]]\n{k} = {{}}\n"), "row[0]."),
            (
                &rows::INNER_ROW_KEYS,
                |k| format!("[[row]]\n[[row.col]]\n[[row.col.row]]\n{k} = {{}}\n"),
                "row[0].col[0].row[0].",
            ),
            (&rows::COL_KEYS, |k| format!("[[row]]\n[[row.col]]\n{k} = {{}}\n"), "row[0].col[0]."),
            (
                &rows::BOX_KEYS,
                |k| format!("[box.a]\n{k} = {{}}\n[[row]]\nbox = \"a\"\nmodules = []\n"),
                "box.a.",
            ),
        ];
        let all = &crate::modules::SCHEMAS;
        for (keys, text, prefix) in lists {
            for key in keys {
                let (_, errs) = parse(&text(key), all);
                let path = format!("{prefix}{key}");
                assert!(
                    !errs.iter().any(|e| e.path == path && e.message.starts_with("unknown key")),
                    "{path} is listed but not taken: {errs:?}"
                );
            }
            let (_, errs) = parse(&text("zz_not_a_key"), all);
            let path = format!("{prefix}zz_not_a_key");
            let problem = errs.iter().find(|e| e.path == path).unwrap_or_else(|| panic!("{path}"));
            assert_eq!(
                problem.message,
                format!("unknown key; expected one of {}", keys.join(", ")),
                "{path}"
            );
        }

        // The other direction: every key `config show` writes for a config
        // using every layout form is in its table's list.
        let text = "[frame]\nstyle = \"custom\"\n[box.b]\ntitle = \"B\"\ntitle_justify = \"center\"\ntitle_pad = 2\ntitle_color = \"accent\"\nstyle = \"double\"\ncolor = \"warn\"\n[[row]]\ntitle = \"T\"\ntitle_pad = 2\nseparator = \" \"\nmodules = [\"path\"]\nright = [\"clock\"]\n[[row]]\ngap = 2\nblank = true\n[[row.col]]\nwidth = 9\njustify = \"center\"\nvalign = \"bottom\"\nbox = \"b\"\n[[row.col.row]]\ntitle = \"I\"\nseparator = \" \"\nmodules = [\"path\"]\nright = [\"clock\"]\n[[row.col]]\nbox = true\nmodules = [\"model\"]\n";
        let (c, errs) = parse(text, all);
        assert_eq!(errs, Vec::new());
        let shown: toml::Table = toml::from_str(&crate::docs::config_toml(&c, false)).unwrap();
        let listed = |table: &toml::Table, keys: &[&str], what: &str| {
            for key in table.keys() {
                assert!(keys.contains(&key.as_str()), "config show writes {what}.{key}");
            }
        };
        let tables = |key: &str| -> Vec<toml::Table> {
            shown.get(key).and_then(toml::Value::as_array).map_or_else(Vec::new, |a| {
                a.iter().filter_map(toml::Value::as_table).cloned().collect()
            })
        };
        listed(&shown, &TOP_KEYS, "");
        listed(shown["frame"].as_table().unwrap(), &frame::FRAME_KEYS, "frame");
        listed(shown["format"].as_table().unwrap(), &FORMAT_KEYS, "format");
        for b in shown["box"].as_table().unwrap().values() {
            listed(b.as_table().unwrap(), &rows::BOX_KEYS, "box");
        }
        for row in tables("row") {
            listed(&row, &rows::ROW_KEYS, "row");
            for col in row.get("col").and_then(toml::Value::as_array).into_iter().flatten() {
                let col = col.as_table().unwrap();
                listed(col, &rows::COL_KEYS, "row.col");
                for inner in col.get("row").and_then(toml::Value::as_array).into_iter().flatten() {
                    listed(inner.as_table().unwrap(), &rows::INNER_ROW_KEYS, "row.col.row");
                }
            }
        }
    }

    /// cfg-15: every key that takes a colour refuses a bad one with the one
    /// message, naming the value and what to write; `separator_color` adds
    /// `inherit`, a `[colors]` role takes a literal only, a list names the
    /// item.
    #[test]
    fn every_colour_key_refuses_a_bad_colour_the_same_way() {
        let all = &crate::modules::SCHEMAS;
        let specs = "a role name, a color name, 0-255, or #rrggbb";
        let row = "[[row]]\nbox = \"a\"\nmodules = [\"clock\"]\n";
        for (text, path, want) in [
            (
                "[[row]]\ntitle = \"T\"\ntitle_color = \"nope\"\nmodules = [\"clock\"]\n"
                    .to_owned(),
                "row[0].title_color",
                format!("invalid color \"nope\"; use {specs}"),
            ),
            (
                format!("[box.a]\ncolor = \"nope\"\n{row}"),
                "box.a.color",
                format!("invalid color \"nope\"; use {specs}"),
            ),
            (
                format!("[box.a]\ntitle = \"B\"\ntitle_color = \"nope\"\n{row}"),
                "box.a.title_color",
                format!("invalid color \"nope\"; use {specs}"),
            ),
            (
                "[modules.text.t]\ntext = \"x\"\ncolor = \"nope\"\n".to_owned(),
                "modules.text.t.color",
                format!("invalid color \"nope\"; use {specs}"),
            ),
            (
                "[modules.context.colors]\nmarker = \"nope\"\n".to_owned(),
                "modules.context.colors.marker",
                format!("invalid color \"nope\"; use {specs}"),
            ),
            (
                "[modules.context]\nband_colors = [\"ok\", \"nope\"]\n".to_owned(),
                "modules.context.band_colors",
                format!("item 1: invalid color \"nope\"; use {specs}"),
            ),
            (
                "[frame]\nseparator_color = \"nope\"\n".to_owned(),
                "frame.separator_color",
                format!("invalid color \"nope\"; use inherit, {specs}"),
            ),
            (
                "[colors]\naccent = \"warn\"\n".to_owned(),
                "colors.accent",
                "invalid color \"warn\"; use a color name, 0-255, or #rrggbb".to_owned(),
            ),
        ] {
            let (_, errs) = parse(&text, all);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(problems, [(path, want.as_str())], "{text}");
        }
    }

    /// A config that sets most keys, each to a valid value, parses clean.
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

        // cfg-11: the row, inner-row and box titles and the box glyphs too.
        let text = concat!(
            "[frame]\nstyle = \"custom\"\nside = \"\\u001b[31m|\\u001b[0m\"\n",
            "top_left = \"\\u001b]0;x\\u0007+\"\n",
            "[box.a]\ntitle = \"\\u001b]0;x\\u0007B\\u202e\"\n",
            "[[row]]\ntitle = \"\\u001b[2JR\\u200e\"\nmodules = [\"model\"]\n",
            "[[row]]\nbox = \"a\"\nmodules = [\"model\"]\n",
            "[[row]]\n[[row.col]]\n[[row.col.row]]\ntitle = \"\\u001bP dcs \\u001b\\\\I\"\n",
            "modules = [\"model\"]\n",
        );
        let (c, errs) = parse(text, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        let title = |t: Option<&TitleCfg>| t.map_or_default(|t| t.text.clone());
        assert_eq!(title(c.boxes["a"].title.as_ref()), "B");
        assert_eq!(title(c.rows[0].title.as_ref()), "R");
        assert_eq!(title(c.rows[2].cols[0].rows[0].title.as_ref()), "I");
        assert_eq!((c.frame.chars.side.as_str(), c.frame.chars.top_left.as_str()), ("|", "+"));
        let shown = crate::docs::config_toml(&c, false);
        for c in ['\u{1b}', '\u{7}', '\u{202e}', '\u{200e}'] {
            assert!(!shown.contains(c), "{c:?} in {shown}");
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
        // no-color.org: present *and not empty*.
        assert!(!no_color_from(None));
        assert!(!no_color_from(Some(std::ffi::OsStr::new(""))));
        assert!(no_color_from(Some(std::ffi::OsStr::new("1"))));
        assert!(no_color_from(Some(std::ffi::OsStr::new("0"))), "any value but empty");
    }
}
