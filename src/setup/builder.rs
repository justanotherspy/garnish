//! The builder's row list (SPEC § 14): the config's rows, columns and
//! stacks as lines to move a cursor over, and every edit those lines allow,
//! each an operation on the draft.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use toml::{Table, Value};

use super::draft::{Draft, RowAt, string_list};
use super::ui::{Chrome, cells, clip, window};

/// Which group of a row a module sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// `modules`.
    Left,
    /// `right`.
    Right,
}

impl Side {
    /// The key of the group.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Left => "modules",
            Self::Right => "right",
        }
    }
}

/// One module placed on a line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    /// The module id.
    pub id: String,
    /// Its group.
    pub side: Side,
    /// Its place in the group.
    pub index: usize,
}

/// What a line of the list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A `[[row]]`.
    Row,
    /// A `[[row.col]]`.
    Col,
    /// A `[[row.col.row]]`.
    Inner,
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The table it stands for.
    pub at: RowAt,
    /// Row, column or inner row.
    pub kind: ItemKind,
    /// The modules it places (none for a row with columns or a stacked
    /// column).
    pub chips: Vec<Chip>,
    /// What else the table sets, in a few words.
    pub note: String,
}

impl Item {
    fn label(&self) -> String {
        match self.kind {
            ItemKind::Row => format!("row {}", self.at.row.saturating_add(1)),
            ItemKind::Col => format!("  col {}", self.at.col.map_or(0, |c| c.saturating_add(1))),
            ItemKind::Inner => format!(
                "    row {}.{}",
                self.at.col.map_or(0, |c| c.saturating_add(1)),
                self.at.inner.map_or(0, |i| i.saturating_add(1))
            ),
        }
    }
}

/// The list and its cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Builder {
    /// The lines, top to bottom.
    pub items: Vec<Item>,
    /// The selected line.
    pub cursor: usize,
    /// The selected chip of that line, when one is.
    pub chip: Option<usize>,
    scroll: usize,
}

/// The string list under `key` of a table.
fn ids(table: &Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn chips_of(table: &Table) -> Vec<Chip> {
    let mut chips: Vec<Chip> = ids(table, "modules")
        .into_iter()
        .enumerate()
        .map(|(index, id)| Chip { id, side: Side::Left, index })
        .collect();
    chips.extend(ids(table, "right").into_iter().enumerate().map(|(index, id)| Chip {
        id,
        side: Side::Right,
        index,
    }));
    chips
}

/// What a row or column table sets besides its modules, in a few words.
fn note_of(table: &Table, kind: ItemKind) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(w) = table.get("width") {
        parts.push(super::form::show(w));
    }
    if let Some(j) = table.get("justify").and_then(Value::as_str) {
        parts.push(j.to_owned());
    }
    if let Some(t) = table.get("title").and_then(Value::as_str) {
        parts.push(format!("title {t:?}"));
    }
    match table.get("box") {
        Some(Value::String(name)) => parts.push(format!("box {name}")),
        Some(Value::Boolean(true)) => parts.push("box".to_owned()),
        _ => {}
    }
    if let Some(g) = table.get("gap").and_then(Value::as_integer) {
        parts.push(format!("gap {g}"));
    }
    if table.get("blank").and_then(Value::as_bool) == Some(true) {
        parts.push("blank".to_owned());
    }
    if kind != ItemKind::Col
        && !table.contains_key("col")
        && ids(table, "modules").is_empty()
        && ids(table, "right").is_empty()
        && table.contains_key("modules")
    {
        parts.push("spacer".to_owned());
    }
    parts.join(" · ")
}

impl Builder {
    /// Rebuild the lines from the draft's rows, keeping the cursor where it
    /// can.
    pub fn rebuild(&mut self, draft: &Draft) {
        let mut items: Vec<Item> = Vec::new();
        for (r, row) in draft.rows().iter().enumerate() {
            let Some(table) = row.as_table() else { continue };
            let cols = table.get("col").and_then(Value::as_array);
            let chips = if cols.is_some() { Vec::new() } else { chips_of(table) };
            items.push(Item {
                at: RowAt::row(r),
                kind: ItemKind::Row,
                chips,
                note: note_of(table, ItemKind::Row),
            });
            for (c, col) in cols.into_iter().flatten().enumerate() {
                let Some(ct) = col.as_table() else { continue };
                let inner = ct.get("row").and_then(Value::as_array);
                let at = RowAt { row: r, col: Some(c), inner: None };
                let chips = if inner.is_some() { Vec::new() } else { chips_of(ct) };
                items.push(Item {
                    at,
                    kind: ItemKind::Col,
                    chips,
                    note: note_of(ct, ItemKind::Col),
                });
                for (i, row) in inner.into_iter().flatten().enumerate() {
                    let Some(it) = row.as_table() else { continue };
                    let at = RowAt { row: r, col: Some(c), inner: Some(i) };
                    items.push(Item {
                        at,
                        kind: ItemKind::Inner,
                        chips: chips_of(it),
                        note: note_of(it, ItemKind::Inner),
                    });
                }
            }
        }
        self.items = items;
        self.clamp();
    }

    fn clamp(&mut self) {
        self.cursor = self.cursor.min(self.items.len().saturating_sub(1));
        let chips = self.items.get(self.cursor).map_or(0, |i| i.chips.len());
        self.chip = self.chip.filter(|_| chips > 0).map(|c| c.min(chips.saturating_sub(1)));
    }

    /// The selected line.
    #[must_use]
    pub fn item(&self) -> Option<&Item> {
        self.items.get(self.cursor)
    }

    /// The selected chip.
    #[must_use]
    pub fn selected_chip(&self) -> Option<&Chip> {
        self.item()?.chips.get(self.chip?)
    }

    /// The id of the selected module, when a chip is selected.
    #[must_use]
    pub fn selected_id(&self) -> Option<&str> {
        self.selected_chip().map(|c| c.id.as_str())
    }

    /// Move the cursor a line up or down.
    pub fn move_line(&mut self, down: bool) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        self.cursor = if down {
            self.cursor.saturating_add(1).min(n.saturating_sub(1))
        } else {
            self.cursor.saturating_sub(1)
        };
        self.chip = None;
    }

    /// Move the chip selection along the line: past the first chip lands on
    /// the line itself, so a row and its modules are one keyboard path.
    pub fn move_chip(&mut self, right: bool) {
        let chips = self.item().map_or(0, |i| i.chips.len());
        if chips == 0 {
            return;
        }
        self.chip = match (self.chip, right) {
            (None, true) => Some(0),
            (None, false) => Some(chips.saturating_sub(1)),
            (Some(c), true) => Some(c.saturating_add(1).min(chips.saturating_sub(1))),
            (Some(0), false) => None,
            (Some(c), false) => Some(c.saturating_sub(1)),
        };
    }

    /// `Tab`: the next chip, across lines; `Shift-Tab` the previous.
    pub fn next_chip(&mut self, forward: bool) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        for _ in 0..n.saturating_mul(2).saturating_add(2) {
            let chips = self.item().map_or(0, |i| i.chips.len());
            let next = match (self.chip, forward) {
                (Some(c), true) if c.saturating_add(1) < chips => Some(c.saturating_add(1)),
                (None, true) if chips > 0 => Some(0),
                (Some(c), false) if c > 0 => Some(c.saturating_sub(1)),
                _ => None,
            };
            if next.is_some() {
                self.chip = next;
                return;
            }
            self.cursor = if forward {
                self.cursor.saturating_add(1).checked_rem(n).unwrap_or(0)
            } else {
                self.cursor.checked_sub(1).unwrap_or_else(|| n.saturating_sub(1))
            };
            let chips = self.item().map_or(0, |i| i.chips.len());
            self.chip = if forward || chips == 0 { None } else { Some(chips.saturating_sub(1)) };
            if forward && chips > 0 {
                self.chip = Some(0);
                return;
            }
            if !forward && chips > 0 {
                return;
            }
        }
    }

    /// Select the first chip carrying `id`, in `row` first, anywhere else
    /// otherwise; true when one was found.
    pub fn select_module(&mut self, id: &str, row: Option<usize>) -> bool {
        let found = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| row.is_none_or(|r| item.at.row == r))
            .chain(self.items.iter().enumerate())
            .find_map(|(i, item)| item.chips.iter().position(|c| c.id == id).map(|c| (i, c)));
        if let Some((line, chip)) = found {
            self.cursor = line;
            self.chip = Some(chip);
        }
        found.is_some()
    }

    /// Select the first line of row `row`.
    pub fn select_row(&mut self, row: usize) {
        if let Some(i) = self.items.iter().position(|item| item.at.row == row) {
            self.cursor = i;
            self.chip = None;
        }
    }

    /// Select the line under `y` of the list's last drawing, and the chip
    /// under `x` when the click landed on one; true when a line is there.
    pub fn click(&mut self, x: usize, y: usize, area: Rect) -> bool {
        let line = y.saturating_sub(usize::from(area.y)).saturating_add(self.scroll);
        if y < usize::from(area.y) || line >= self.items.len() {
            return false;
        }
        self.cursor = line;
        self.chip = None;
        let Some(item) = self.items.get(line) else { return true };
        let mut at = item.label().chars().count().saturating_add(2);
        let x = x.saturating_sub(usize::from(area.x));
        for (i, chip) in item.chips.iter().enumerate() {
            let w = chip.id.chars().count().saturating_add(2);
            if i > 0
                && chip.side == Side::Right
                && item.chips.get(i.saturating_sub(1)).is_some_and(|p| p.side == Side::Left)
            {
                at = at.saturating_add(4);
            }
            if x >= at && x < at.saturating_add(w) {
                self.chip = Some(i);
                break;
            }
            at = at.saturating_add(w).saturating_add(1);
        }
        true
    }

    /// Draw the list into `area`.
    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, draft: &Draft) {
        let height = usize::from(area.height);
        self.scroll = window(self.cursor, self.items.len(), height, self.scroll);
        let width = usize::from(area.width);
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, item) in self.items.iter().enumerate().skip(self.scroll).take(height) {
            let selected = i == self.cursor;
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                format!("{:<8}", item.label()),
                if selected && self.chip.is_none() { Chrome::selected() } else { Chrome::title() },
            )];
            spans.push(Span::raw("  "));
            for (c, chip) in item.chips.iter().enumerate() {
                if c > 0
                    && chip.side == Side::Right
                    && item.chips.get(c.saturating_sub(1)).is_some_and(|p| p.side == Side::Left)
                {
                    spans.push(Span::styled(" │ ", Chrome::muted()));
                }
                let on = selected && self.chip == Some(c);
                let dot = if draft.module_has_overrides(&chip.id) { "●" } else { " " };
                spans.push(Span::styled(
                    format!("{}{dot}", chip.id),
                    if on { Chrome::selected() } else { Style::new() },
                ));
                spans.push(Span::raw(" "));
            }
            if item.chips.is_empty() && item.kind != ItemKind::Col {
                if item.note.contains("spacer") {
                    spans.push(Span::styled("(spacer)", Chrome::muted()));
                } else if !draft.row(item.at).is_some_and(|t| t.contains_key("col")) {
                    spans.push(Span::styled("(empty)", Chrome::muted()));
                }
            }
            if !item.note.is_empty() {
                spans.push(Span::styled(format!("  {}", item.note), Chrome::muted()));
            }
            let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
            if text.chars().count() > width {
                lines.push(Line::from(clip(&text, width)));
            } else {
                lines.push(Line::from(spans));
            }
        }
        frame.render_widget(Paragraph::new(lines), Rect { height: cells(height), ..area });
    }
}

/// The editing operations, each a change to the draft at the cursor. Every
/// one returns a line for the status bar, or why nothing happened.
impl Builder {
    /// Add a module after the selected chip (or at the end of the left
    /// group, or the right group when the cursor is there).
    ///
    /// # Errors
    /// Why nothing was added, for the status bar.
    pub fn add_module(&mut self, draft: &mut Draft, id: &str) -> Result<String, String> {
        let Some(item) = self.item().cloned() else {
            return Err("no row selected; add one with a".into());
        };
        if item.chips.is_empty()
            && draft.row(item.at).is_some_and(|t| t.contains_key("col") || t.contains_key("row"))
        {
            return Err("this line holds columns; select a column or an inner row".into());
        }
        let (side, index) = self
            .selected_chip()
            .map_or((Side::Left, usize::MAX), |c| (c.side, c.index.saturating_add(1)));
        let table = draft.row_mut(item.at).ok_or("no such row")?;
        let mut list = ids(table, side.key());
        let at = index.min(list.len());
        list.insert(at, id.to_owned());
        table.insert(side.key().to_owned(), string_list(&list));
        self.rebuild(draft);
        if let Some(c) = self
            .items
            .get(self.cursor)
            .and_then(|i| i.chips.iter().position(|c| c.side == side && c.index == at))
        {
            self.chip = Some(c);
        }
        Ok(format!("added {id}"))
    }

    /// Remove the selected module, or the selected line when no chip is.
    ///
    /// # Errors
    /// Why nothing was removed, for the status bar.
    pub fn delete(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        if let Some(chip) = self.selected_chip().cloned() {
            let table = draft.row_mut(item.at).ok_or("no such row")?;
            let mut list = ids(table, chip.side.key());
            if chip.index < list.len() {
                list.remove(chip.index);
            }
            if list.is_empty() && chip.side == Side::Right {
                table.remove("right");
            } else {
                table.insert(chip.side.key().to_owned(), string_list(&list));
            }
            self.rebuild(draft);
            return Ok(format!("removed {}", chip.id));
        }
        let siblings = draft.siblings_mut(item.at).ok_or("no such row")?;
        let index = match item.kind {
            ItemKind::Row => item.at.row,
            ItemKind::Col => item.at.col.unwrap_or(0),
            ItemKind::Inner => item.at.inner.unwrap_or(0),
        };
        if index < siblings.len() {
            siblings.remove(index);
        }
        self.rebuild(draft);
        Ok(format!("deleted {}", item.label().trim()))
    }

    /// Insert a new row after (or before) the selected one, in the same
    /// list: a top-level row, a column, or an inner row.
    ///
    /// # Errors
    /// Why nothing was inserted, for the status bar.
    pub fn insert(&mut self, draft: &mut Draft, after: bool) -> Result<String, String> {
        let Some(item) = self.item().cloned() else {
            draft.rows_mut().ok_or("no rows")?.push(Value::Table(new_row()));
            self.rebuild(draft);
            self.cursor = self.items.len().saturating_sub(1);
            return Ok("added a row".into());
        };
        let siblings = draft.siblings_mut(item.at).ok_or("no such row")?;
        let index = match item.kind {
            ItemKind::Row => item.at.row,
            ItemKind::Col => item.at.col.unwrap_or(0),
            ItemKind::Inner => item.at.inner.unwrap_or(0),
        };
        let at = if after { index.saturating_add(1).min(siblings.len()) } else { index };
        let fresh = if item.kind == ItemKind::Col {
            Value::Table(Table::new())
        } else {
            Value::Table(new_row())
        };
        siblings.insert(at, fresh);
        self.rebuild(draft);
        let target = RowAt {
            row: if item.kind == ItemKind::Row { at } else { item.at.row },
            col: if item.kind == ItemKind::Col { Some(at) } else { item.at.col },
            inner: if item.kind == ItemKind::Inner { Some(at) } else { None },
        };
        if let Some(i) = self.items.iter().position(|it| it.at == target && it.kind == item.kind) {
            self.cursor = i;
        }
        self.chip = None;
        Ok(match item.kind {
            ItemKind::Col => "added a column".into(),
            _ => "added a row".into(),
        })
    }

    /// Duplicate the selected line after itself.
    ///
    /// # Errors
    /// Why nothing was cloned, for the status bar.
    pub fn clone_line(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let siblings = draft.siblings_mut(item.at).ok_or("no such row")?;
        let index = match item.kind {
            ItemKind::Row => item.at.row,
            ItemKind::Col => item.at.col.unwrap_or(0),
            ItemKind::Inner => item.at.inner.unwrap_or(0),
        };
        let Some(copy) = siblings.get(index).cloned() else {
            return Err("nothing to clone".into());
        };
        siblings.insert(index.saturating_add(1), copy);
        self.rebuild(draft);
        self.move_line(true);
        Ok("cloned".into())
    }

    /// Move the selected line up or down among its siblings, or the
    /// selected chip along its group.
    ///
    /// # Errors
    /// Why nothing moved, for the status bar.
    pub fn shift(&mut self, draft: &mut Draft, down: bool) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        if let Some(chip) = self.selected_chip().cloned() {
            let table = draft.row_mut(item.at).ok_or("no such row")?;
            let mut list = ids(table, chip.side.key());
            let to = if down { chip.index.saturating_add(1) } else { chip.index.saturating_sub(1) };
            if to >= list.len() || to == chip.index {
                return Err("already at the end".into());
            }
            list.swap(chip.index, to);
            table.insert(chip.side.key().to_owned(), string_list(&list));
            self.rebuild(draft);
            if let Some(c) = self
                .items
                .get(self.cursor)
                .and_then(|i| i.chips.iter().position(|c| c.side == chip.side && c.index == to))
            {
                self.chip = Some(c);
            }
            return Ok(format!("moved {}", chip.id));
        }
        let siblings = draft.siblings_mut(item.at).ok_or("no such row")?;
        let index = match item.kind {
            ItemKind::Row => item.at.row,
            ItemKind::Col => item.at.col.unwrap_or(0),
            ItemKind::Inner => item.at.inner.unwrap_or(0),
        };
        let to = if down { index.saturating_add(1) } else { index.saturating_sub(1) };
        if to >= siblings.len() || to == index {
            return Err("already at the end".into());
        }
        siblings.swap(index, to);
        self.rebuild(draft);
        let target = RowAt {
            row: if item.kind == ItemKind::Row { to } else { item.at.row },
            col: if item.kind == ItemKind::Col { Some(to) } else { item.at.col },
            inner: if item.kind == ItemKind::Inner { Some(to) } else { None },
        };
        if let Some(i) = self.items.iter().position(|it| it.at == target && it.kind == item.kind) {
            self.cursor = i;
        }
        Ok("moved".into())
    }

    /// Move the selected module to the other group of its line.
    ///
    /// # Errors
    /// Why nothing moved, for the status bar.
    pub fn switch_side(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let Some(chip) = self.selected_chip().cloned() else {
            return Err("select a module first".into());
        };
        let table = draft.row_mut(item.at).ok_or("no such row")?;
        let mut from = ids(table, chip.side.key());
        if chip.index >= from.len() {
            return Err("no such module".into());
        }
        let id = from.remove(chip.index);
        let other = if chip.side == Side::Left { Side::Right } else { Side::Left };
        let mut to = ids(table, other.key());
        to.push(id.clone());
        table.insert(chip.side.key().to_owned(), string_list(&from));
        table.insert(other.key().to_owned(), string_list(&to));
        if from.is_empty() && chip.side == Side::Right {
            table.remove("right");
        }
        self.rebuild(draft);
        if let Some(c) = self
            .items
            .get(self.cursor)
            .and_then(|i| i.chips.iter().position(|c| c.side == other && c.id == id))
        {
            self.chip = Some(c);
        }
        Ok(format!("{id} moved to {}", other.key()))
    }

    /// Move the selected module to the previous or next column of its row.
    ///
    /// # Errors
    /// Why nothing moved, for the status bar.
    pub fn switch_column(&mut self, draft: &mut Draft, next: bool) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let Some(chip) = self.selected_chip().cloned() else {
            return Err("select a module first".into());
        };
        let Some(col) = item.at.col else {
            return Err("this row has no columns; C adds them".into());
        };
        let target = if next {
            col.saturating_add(1)
        } else {
            col.checked_sub(1).ok_or("already in the first column")?
        };
        let target_at = RowAt { row: item.at.row, col: Some(target), inner: None };
        if draft.row(target_at).is_none() {
            return Err("no column there".into());
        }
        if draft.row(target_at).is_some_and(|t| t.contains_key("row")) {
            return Err("that column is a stack; select one of its rows".into());
        }
        let from_table = draft.row_mut(item.at).ok_or("no such row")?;
        let mut from = ids(from_table, chip.side.key());
        if chip.index >= from.len() {
            return Err("no such module".into());
        }
        let id = from.remove(chip.index);
        from_table.insert(chip.side.key().to_owned(), string_list(&from));
        let to_table = draft.row_mut(target_at).ok_or("no such column")?;
        let mut to = ids(to_table, "modules");
        to.push(id.clone());
        to_table.insert("modules".to_owned(), string_list(&to));
        self.rebuild(draft);
        if let Some(i) =
            self.items.iter().position(|it| it.at == target_at && it.kind == ItemKind::Col)
        {
            self.cursor = i;
        }
        self.chip = self.item().and_then(|i| i.chips.iter().position(|c| c.id == id));
        Ok(format!("{id} moved to column {}", target.saturating_add(1)))
    }

    /// Give the selected row columns: its own groups become the first
    /// column and an empty second one follows; on a row that has columns,
    /// add one more.
    ///
    /// # Errors
    /// Why no column was added, for the status bar.
    pub fn add_column(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let at = RowAt::row(item.at.row);
        let table = draft.row_mut(at).ok_or("no such row")?;
        if let Some(cols) = table.get_mut("col").and_then(Value::as_array_mut) {
            cols.push(Value::Table(Table::new()));
        } else {
            let mut first = Table::new();
            if let Some(m) = table.remove("modules") {
                first.insert("modules".to_owned(), m);
            }
            if let Some(r) = table.remove("right") {
                first.insert("right".to_owned(), r);
            }
            table.insert(
                "col".to_owned(),
                Value::Array(vec![Value::Table(first), Value::Table(Table::new())]),
            );
        }
        self.rebuild(draft);
        self.select_row(at.row);
        Ok("added a column".into())
    }

    /// Turn the selected column into a stack of rows (its modules become the
    /// first inner row), or add an inner row after the selected one.
    ///
    /// # Errors
    /// Why nothing changed, for the status bar.
    pub fn stack(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        match item.kind {
            ItemKind::Row => Err("select a column first (C adds columns)".into()),
            ItemKind::Inner => self.insert(draft, true),
            ItemKind::Col => {
                let table = draft.row_mut(item.at).ok_or("no such column")?;
                if table.contains_key("row") {
                    return Err("already a stack; select one of its rows to add another".into());
                }
                let mut first = Table::new();
                if let Some(m) = table.remove("modules") {
                    first.insert("modules".to_owned(), m);
                }
                if let Some(r) = table.remove("right") {
                    first.insert("right".to_owned(), r);
                }
                if first.is_empty() {
                    first = new_row();
                }
                table.insert("row".to_owned(), Value::Array(vec![Value::Table(first)]));
                self.rebuild(draft);
                Ok("the column is a stack now".into())
            }
        }
    }

    /// Make the selected row a spacer (`modules = []`), or a row again.
    ///
    /// # Errors
    /// Why nothing changed, for the status bar.
    pub fn toggle_spacer(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        if item.kind == ItemKind::Col {
            return Err("a column cannot be a spacer".into());
        }
        let table = draft.row_mut(item.at).ok_or("no such row")?;
        if table.contains_key("col") {
            return Err("a row with columns cannot be a spacer".into());
        }
        let spacer = ids(table, "modules").is_empty() && ids(table, "right").is_empty();
        table.remove("right");
        table.insert("modules".to_owned(), Value::Array(Vec::new()));
        self.rebuild(draft);
        Ok(if spacer { "already a spacer; m adds a module".into() } else { "now a spacer".into() })
    }

    /// Put the selected row, column or inner row in the named box (or a
    /// box of its own with an empty name); `[box.<name>]` is created when
    /// missing.
    ///
    /// # Errors
    /// Why nothing was boxed, for the status bar.
    pub fn set_box(&mut self, draft: &mut Draft, name: &str) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let value =
            if name.is_empty() { Value::Boolean(true) } else { Value::String(name.to_owned()) };
        if !name.is_empty() && draft.get(&["box", name]).is_none() {
            draft.set(&["box", name, "title"], Value::String(name.to_owned()));
        }
        let table = draft.row_mut(item.at).ok_or("no such row")?;
        table.insert("box".to_owned(), value);
        self.rebuild(draft);
        Ok(if name.is_empty() { "boxed".into() } else { format!("in box {name}") })
    }
}

/// A fresh `[[row]]`: no modules yet, so it reads as a spacer until one is
/// added.
fn new_row() -> Table {
    let mut t = Table::new();
    t.insert("modules".to_owned(), Value::Array(Vec::new()));
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        Draft::from_text(
            "[[row]]\nmodules = [\"path\", \"branch\"]\nright = [\"clock\"]\n[[row]]\nmodules = [\"model\"]\n",
        )
    }

    fn ids_at(d: &Draft, at: RowAt, key: &str) -> Vec<String> {
        d.row(at).map(|t| ids(t, key)).unwrap_or_default()
    }

    #[test]
    fn the_list_mirrors_the_rows_and_the_cursor_walks_chips() {
        let d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        assert_eq!(b.items.len(), 2);
        assert_eq!(b.items[0].chips.len(), 3);
        assert_eq!(b.items[0].chips[2], Chip { id: "clock".into(), side: Side::Right, index: 0 });
        assert!(b.selected_id().is_none());
        b.move_chip(true);
        assert_eq!(b.selected_id(), Some("path"));
        b.move_chip(false);
        assert!(b.chip.is_none(), "left of the first chip is the row");
        b.next_chip(true);
        b.next_chip(true);
        b.next_chip(true);
        assert_eq!(b.selected_id(), Some("clock"));
        b.next_chip(true);
        assert_eq!((b.cursor, b.selected_id()), (1, Some("model")), "tab crosses lines");
        b.next_chip(false);
        assert_eq!((b.cursor, b.selected_id()), (0, Some("clock")));
        assert!(b.select_module("model", None));
        assert_eq!(b.cursor, 1);
        assert!(!b.select_module("nope", None));
        b.move_line(false);
        assert_eq!((b.cursor, b.chip), (0, None));
    }

    #[test]
    fn edits_change_the_draft_and_keep_the_cursor_sensible() {
        let mut d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        b.move_chip(true);
        assert_eq!(b.add_module(&mut d, "sync").unwrap(), "added sync");
        assert_eq!(ids_at(&d, RowAt::row(0), "modules"), vec!["path", "sync", "branch"]);
        assert_eq!(b.selected_id(), Some("sync"));
        b.shift(&mut d, true).unwrap();
        assert_eq!(ids_at(&d, RowAt::row(0), "modules"), vec!["path", "branch", "sync"]);
        assert_eq!(b.selected_id(), Some("sync"));
        b.switch_side(&mut d).unwrap();
        assert_eq!(ids_at(&d, RowAt::row(0), "right"), vec!["clock", "sync"]);
        b.delete(&mut d).unwrap();
        assert_eq!(ids_at(&d, RowAt::row(0), "right"), vec!["clock"]);
        b.select_row(1);
        b.insert(&mut d, true).unwrap();
        assert_eq!(d.rows().len(), 3);
        assert_eq!(b.cursor, 2);
        assert!(b.items[2].note.contains("spacer"));
        b.toggle_spacer(&mut d).unwrap();
        b.clone_line(&mut d).unwrap();
        assert_eq!(d.rows().len(), 4);
        b.delete(&mut d).unwrap();
        b.delete(&mut d).unwrap();
        assert_eq!(d.rows().len(), 2);
        b.select_row(0);
        b.shift(&mut d, true).unwrap();
        assert_eq!(ids_at(&d, RowAt::row(1), "right"), vec!["clock"]);
        assert_eq!(b.cursor, 1);
        assert!(b.shift(&mut d, true).is_err(), "at the end");
        // Columns and stacks.
        b.add_column(&mut d).unwrap();
        assert_eq!(b.items.len(), 4, "row, two columns, and the other row");
        let first = RowAt { row: 1, col: Some(0), inner: None };
        assert_eq!(ids_at(&d, first, "right"), vec!["clock"]);
        b.select_module("path", Some(1));
        b.switch_column(&mut d, true).unwrap();
        assert_eq!(
            ids_at(&d, RowAt { row: 1, col: Some(1), inner: None }, "modules"),
            vec!["path"]
        );
        b.stack(&mut d).unwrap();
        assert!(d.row(RowAt { row: 1, col: Some(1), inner: Some(0) }).is_some());
        assert_eq!(
            ids_at(&d, RowAt { row: 1, col: Some(1), inner: Some(0) }, "modules"),
            vec!["path"]
        );
        b.set_box(&mut d, "repo").unwrap();
        assert!(d.get(&["box", "repo", "title"]).is_some());
        let (config, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(config.rows[1].cols.len(), 2);
        assert_eq!(config.rows[1].cols[1].rows.len(), 1);
        // The whole thing still saves as a file the parser reads back.
        let again = Draft::from_text(&d.text());
        assert_eq!(again.resolved().0, config);
    }
}
