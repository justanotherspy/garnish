//! Rows, columns, stacks and boxes (SPEC § 4.1, § 4.3): the layout tree as
//! the file writes it, and its resolution into [`RowCfg`]s with every
//! layout rule checked under its TOML path.

use std::collections::BTreeMap;

use super::read::{
    bounded_count, color_spec, enum_field, field, id_list, is_bare_key, problem, text_field,
};
use super::schema::{ModuleCfg, ModuleSchema};
use super::{ConfigError, MAX_CELLS};
use crate::ansi::Color;
use crate::frame::FrameStyle;
use crate::theme::Theme;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// Every place, left to right.
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// Every place, top to bottom.
    pub const ALL: [Self; 3] = [Self::Top, Self::Center, Self::Bottom];

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

/// One `[[row]]` (or `[[line]]`) array, item by item.
///
/// A non-table keeps its place as a `bad_list` placeholder, so every later
/// row keeps the index it has in the file: dropping it would renumber the
/// survivors and send the user to a `row[n]` that is not theirs. The flag
/// stops it reading as a spacer, and it renders nothing.
pub(super) fn row_array(
    name: &str,
    value: toml::Value,
    errors: &mut Vec<ConfigError>,
) -> Vec<RawRow> {
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
pub(super) struct RawRow {
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

/// Every key a `[[row]]` takes, in the order the "expected one of" message
/// names them.
pub(super) const ROW_KEYS: [&str; 11] = [
    "modules",
    "right",
    "separator",
    "blank",
    "gap",
    "title",
    "title_justify",
    "title_pad",
    "title_color",
    "box",
    "col",
];
/// An inner row (`[[row.col.row]]`) is one line of a stack: it takes no
/// columns of its own and no `gap`, so the tree is two levels deep and never
/// deeper (SPEC § 4.3).
pub(super) const INNER_ROW_KEYS: [&str; 9] = [
    "modules",
    "right",
    "separator",
    "blank",
    "title",
    "title_justify",
    "title_pad",
    "title_color",
    "box",
];
/// Every key a `[[row.col]]` takes.
pub(super) const COL_KEYS: [&str; 7] =
    ["width", "modules", "right", "justify", "valign", "box", "row"];
/// Every key a `[box.<name>]` takes.
pub(super) const BOX_KEYS: [&str; 7] =
    ["title", "title_justify", "title_pad", "title_color", "style", "fill", "color"];

/// The message for a key a table does not take, naming the ones it does.
fn unknown_key(keys: &[&str]) -> String {
    format!("unknown key; expected one of {}", keys.join(", "))
}

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
                "title_justify" => row.title_justify = enum_field(&path, &value, errors),
                "title_pad" => {
                    row.title_pad = bounded_count(&path, &value, MAX_TITLE_PAD, errors);
                }
                "title_color" => row.title_color = field(&path, value, errors),
                "box" => row.boxed = box_ref(&path, value, errors),
                "gap" if !inner => row.gap = bounded_count(&path, &value, MAX_GAP, errors),
                // Not an array (`[row.col]` with one bracket): reported, and
                // the row keeps its own groups rather than becoming columns
                // that are not there, which would read as a spacer.
                "col" if !inner => match col_array(&path, value, errors) {
                    Some(cols) => row.cols = Some(cols),
                    None => row.bad_list = true,
                },
                _ => {
                    let keys: &[&str] = if inner { &INNER_ROW_KEYS } else { &ROW_KEYS };
                    errors.push(problem(&path, &unknown_key(keys)));
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
                "justify" => col.justify = enum_field(&path, &value, errors),
                "valign" => col.valign = enum_field(&path, &value, errors),
                "box" => col.boxed = box_ref(&path, value, errors),
                "row" => {
                    let toml::Value::Array(items) = value else {
                        errors.push(problem(&path, "expected [[row.col.row]] tables"));
                        col.bad_list = true;
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
                _ => errors.push(problem(&path, &unknown_key(&COL_KEYS))),
            }
        }
        col
    }
}

/// The `[[row.col]]` array of one row, bounded at [`MAX_COLS`]; `None`
/// (reported) when the value is not an array at all.
fn col_array(path: &str, value: toml::Value, errors: &mut Vec<ConfigError>) -> Option<Vec<RawCol>> {
    let toml::Value::Array(items) = value else {
        errors.push(problem(path, "expected [[row.col]] tables"));
        return None;
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
    Some(cols)
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

/// The `[[row]]` tables as configured (SPEC § 4.1, § 4.3): normalised to the
/// resolved shape (every row at least one column), with the layout rules
/// checked against the tree the file wrote. `key` is the array name the file
/// used, so an error points at what was typed.
pub(super) fn resolve_rows(
    key: &str,
    raw: &[RawRow],
    defined: &BTreeMap<String, BoxCfg>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> Vec<RowCfg> {
    let mut rows: Vec<RowCfg> = raw
        .iter()
        .enumerate()
        .map(|(i, r)| resolve_row(&format!("{key}[{i}]"), r, defined, theme, errors))
        .collect();
    check_box_run(key, &mut rows, &mut Vec::new(), errors);
    rows
}

/// One `[[row]]` or `[[row.col.row]]`.
fn resolve_row(
    path: &str,
    raw: &RawRow,
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
    // boxed row keeps its place but loses its own box, and so does a row of
    // its stack.
    if boxed.is_some() {
        for (j, col) in cols.iter_mut().enumerate() {
            if col.boxed.take().is_some() {
                errors.push(problem(
                    &format!("{path}.col[{j}].box"),
                    "boxes never nest: this column is already inside its row's box",
                ));
            }
            for (k, inner) in col.rows.iter_mut().enumerate() {
                if inner.boxed.take().is_some() {
                    errors.push(problem(
                        &format!("{path}.col[{j}].row[{k}].box"),
                        "boxes never nest: this row is already inside its outer row's box",
                    ));
                }
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
    RowCfg {
        cols,
        explicit_cols,
        // An inner row never has one: its reader refuses the key.
        gap: raw.gap.unwrap_or(DEFAULT_GAP),
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
        .map(|(k, r)| resolve_row(&format!("{path}.row[{k}]"), r, defined, theme, errors))
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
///
/// One call checks one list of rows. A stack is a run of its own, so a name
/// inside one is never adjacent to a name outside it, but `seen` spans the
/// whole tree: a box is one run in the config, not one run per list.
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

/// The four `title*` keys as one value (SPEC § 4.3). The other three with
/// no `title` have nothing to act on and are reported, as the parser
/// reports every other dead key.
fn resolve_title(
    path: &str,
    text: Option<&str>,
    justify: Option<Justify>,
    pad: Option<usize>,
    color: Option<&str>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> Option<TitleCfg> {
    let Some(text) = text else {
        let set = [
            ("title_justify", justify.is_some()),
            ("title_pad", pad.is_some()),
            ("title_color", color.is_some()),
        ];
        for (key, _) in set.into_iter().filter(|(_, set)| *set) {
            errors.push(problem(&format!("{path}.{key}"), "has no effect without title"));
        }
        return None;
    };
    let color = color.and_then(|spec| {
        color_spec(theme, spec)
            .inspect_err(|msg| errors.push(problem(&format!("{path}.title_color"), msg)))
            .ok()
    });
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
pub(super) fn joins_box(row: &RowCfg, name: &str) -> bool {
    let joins = |b: &Option<BoxRef>| b.as_ref().and_then(BoxRef::name) == Some(name);
    joins(&row.boxed)
        || row.cols.iter().any(|c| joins(&c.boxed) || c.rows.iter().any(|r| joins_box(r, name)))
}

/// The `[box.<name>]` tables (SPEC § 4.3), each validated under its own path.
pub(super) fn resolve_boxes(
    raw: &BTreeMap<String, toml::Table>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<String, BoxCfg> {
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
                "title_justify" => justify = enum_field(&path, &value, errors),
                "title_pad" => pad = bounded_count(&path, &value, MAX_TITLE_PAD, errors),
                "title_color" => color = field::<String>(&path, value, errors),
                // Powerline has caps, not a box shape; the box is drawn
                // rounded rather than silently losing its sides.
                "style" => {
                    cfg.style = enum_field(&path, &value, errors);
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
                        color_spec(theme, &spec)
                            .inspect_err(|msg| errors.push(problem(&path, msg)))
                            .ok()
                    });
                }
                _ => errors.push(problem(&path, &unknown_key(&BOX_KEYS))),
            }
        }
        cfg.title =
            resolve_title(&base, title.as_deref(), justify, pad, color.as_deref(), theme, errors);
        out.insert(name.clone(), cfg);
    }
    out
}

/// Every id on a `[[row]]` is a registered module or a defined `text.<name>`;
/// an unknown one is reported and removed, so the resolved config (and
/// `config show`) carries only ids that render.
pub(super) fn check_row_ids(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;
    use crate::config::tests::schemas;
    use crate::theme::Role;

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

    /// cfg-01: `[row.col]` (one bracket: a table where an array of tables
    /// belongs) is reported under its path and the row keeps its own
    /// groups; it never turns the row into a spacer, and neither does a
    /// `[row.col.row]` (SPEC § 4.1: a reported mistake is never a spacer).
    #[test]
    fn a_column_or_stack_that_is_not_an_array_is_reported_and_never_a_spacer() {
        let schemas = schemas();
        let paths =
            |errs: &[ConfigError]| -> Vec<String> { errs.iter().map(|e| e.path.clone()).collect() };
        let (c, errs) =
            parse("[[row]]\nmodules = [\"path\"]\n[row.col]\nmodules = [\"clock\"]\n", &schemas);
        assert_eq!(paths(&errs), ["row[0].col"], "{errs:?}");
        assert_eq!(c.rows[0].cols[0].left, ["path"], "the row keeps its own modules");
        assert!(!c.rows[0].explicit_cols && !c.rows[0].spacer);
        let (c, errs) = parse("[[row]]\n[row.col]\nmodules = [\"clock\"]\n", &schemas);
        assert_eq!(paths(&errs), ["row[0].col"], "{errs:?}");
        assert!(!c.rows[0].spacer, "an empty row, dropped like any other");
        let (c, errs) =
            parse("[[row]]\n[[row.col]]\n[row.col.row]\nmodules = [\"path\"]\n", &schemas);
        assert_eq!(paths(&errs), ["row[0].col[0].row"], "{errs:?}");
        assert!(!c.rows[0].spacer);
    }

    /// cfg-02: boxes never nest in either direction, however deep: a boxed
    /// row's inner rows lose their boxes too, and a named box only they
    /// joined is then unused.
    #[test]
    fn a_boxed_rows_stack_carries_no_box_of_its_own() {
        let schemas = schemas();
        let (c, errs) = parse(
            "[[row]]\nbox = true\n[[row.col]]\n[[row.col.row]]\nbox = true\nmodules = [\"path\"]\n",
            &schemas,
        );
        let problems: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(problems, ["row[0].col[0].row[0].box"], "{errs:?}");
        assert!(errs[0].message.contains("boxes never nest"), "{}", errs[0].message);
        assert_eq!(c.rows[0].boxed, Some(BoxRef::Anon));
        assert_eq!(c.rows[0].cols[0].rows[0].boxed, None);
        let (c, errs) = parse(
            "[box.a]\n[[row]]\nbox = true\n[[row.col]]\n[[row.col.row]]\nbox = \"a\"\nmodules = [\"path\"]\n",
            &schemas,
        );
        let problems: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(problems, ["row[0].col[0].row[0].box", "box.a"], "{errs:?}");
        assert_eq!(c.rows[0].cols[0].rows[0].boxed, None);
    }

    /// cfg-07: a count that is negative, quoted or over its cap is one
    /// message naming the range, never a Rust type.
    #[test]
    fn a_bad_count_names_its_range() {
        let schemas = schemas();
        for (text, path, max) in [
            ("[[row]]\ngap = -1\nmodules = [\"path\"]\n", "row[0].gap", MAX_GAP),
            ("[[row]]\ngap = \"2\"\nmodules = [\"path\"]\n", "row[0].gap", MAX_GAP),
            ("[[row]]\ngap = 17\nmodules = [\"path\"]\n", "row[0].gap", MAX_GAP),
            (
                "[[row]]\ntitle = \"T\"\ntitle_pad = -1\nmodules = [\"path\"]\n",
                "row[0].title_pad",
                MAX_TITLE_PAD,
            ),
            (
                "[box.a]\ntitle = \"T\"\ntitle_pad = \"2\"\n[[row]]\nbox = \"a\"\nmodules = [\"path\"]\n",
                "box.a.title_pad",
                MAX_TITLE_PAD,
            ),
        ] {
            let (_, errs) = parse(text, &schemas);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(problems, [(path, &*format!("expected an integer 0–{max}"))], "{text}");
        }
        let (_, errs) = parse("stale_after = 5000000000\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("0–4294967295"), "{}", errs[0].message);
    }

    /// cfg-21: a `title_justify`, `title_pad` or `title_color` with no
    /// `title` has nothing to act on, and is said so rather than dropped.
    #[test]
    fn title_keys_without_a_title_are_reported() {
        let schemas = schemas();
        for (text, path) in [
            ("[[row]]\ntitle_pad = 3\nmodules = [\"path\"]\n", "row[0].title_pad"),
            ("[[row]]\ntitle_color = \"accent\"\nmodules = [\"path\"]\n", "row[0].title_color"),
            (
                "[box.a]\ntitle_justify = \"center\"\n[[row]]\nbox = \"a\"\nmodules = [\"path\"]\n",
                "box.a.title_justify",
            ),
        ] {
            let (_, errs) = parse(text, &schemas);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(problems, [(path, "has no effect without title")], "{text}");
        }
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
        // cfg-17: an invalid new name leaves the alias standing, in either
        // order, and is reported once.
        for text in [
            "hide_empty_lines = false\nhide_empty_rows = \"no\"\n",
            "hide_empty_rows = \"no\"\nhide_empty_lines = false\n",
        ] {
            let (c, errs) = parse(text, &schemas);
            let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
            assert_eq!(paths, ["hide_empty_rows"], "{text}");
            assert!(!c.hide_empty_rows, "{text}");
        }
        let (_, errs) = parse("[[row]]\nmodules = [\"nope\"]\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "row[0].modules[0]");
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
}
