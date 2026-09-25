//! The builder's row list (SPEC § 14): the config's rows, columns and
//! stacks as lines to move a cursor over, and every edit those lines allow,
//! each an operation on the draft.

use std::fmt::Write as _;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use toml::{Table, Value};

use super::draft::{Draft, RowAt, TITLE_KEYS, dropped_boxes, string_list};
use super::ui::{Chrome, cells, clip_spans, window};

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
    /// A spacer: a row (or inner row) with an empty `modules` and nothing
    /// else placed (SPEC § 4.1).
    pub spacer: bool,
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

    /// Its index in the list it sits in: `[[row]]`, a row's `[[row.col]]`
    /// or a column's `[[row.col.row]]`.
    fn index(&self) -> usize {
        match self.kind {
            ItemKind::Row => self.at.row,
            ItemKind::Col => self.at.col.unwrap_or(0),
            ItemKind::Inner => self.at.inner.unwrap_or(0),
        }
    }

    /// Where the entry at `index` of that same list sits.
    fn sibling(&self, index: usize) -> RowAt {
        RowAt {
            row: if self.kind == ItemKind::Row { index } else { self.at.row },
            col: if self.kind == ItemKind::Col { Some(index) } else { self.at.col },
            inner: if self.kind == ItemKind::Inner { Some(index) } else { None },
        }
    }
}

/// Write a group back into its table; an emptied `right` goes with its key.
fn set_group(table: &mut Table, side: Side, list: &[String]) {
    if list.is_empty() && side == Side::Right {
        table.remove("right");
    } else {
        table.insert(side.key().to_owned(), string_list(list));
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
    /// The cell range of every chip on every drawn line, recorded by
    /// `draw` so a click is measured against what is on screen.
    hits: Vec<Vec<(usize, usize)>>,
}

/// The string list under `key` of a table.
fn ids(table: &Table, key: &str) -> Vec<String> {
    table.get(key).and_then(Value::as_array).map_or_default(|items| {
        items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()
    })
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

/// Whether a row table is a spacer: `modules` written and empty, nothing on
/// the right, no columns (a column is never one).
fn is_spacer(table: &Table, kind: ItemKind) -> bool {
    kind != ItemKind::Col
        && !table.contains_key("col")
        && table.contains_key("modules")
        && ids(table, "modules").is_empty()
        && ids(table, "right").is_empty()
}

/// What a row or column table sets besides its modules, in a few words.
fn note_of(table: &Table) -> String {
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
                spacer: is_spacer(table, ItemKind::Row),
                note: note_of(table),
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
                    spacer: false,
                    note: note_of(ct),
                });
                for (i, row) in inner.into_iter().flatten().enumerate() {
                    let Some(it) = row.as_table() else { continue };
                    let at = RowAt { row: r, col: Some(c), inner: Some(i) };
                    items.push(Item {
                        at,
                        kind: ItemKind::Inner,
                        chips: chips_of(it),
                        spacer: is_spacer(it, ItemKind::Inner),
                        note: note_of(it),
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

    /// `Tab`: the next chip, across lines; `Shift-Tab` the previous. With
    /// no chip anywhere the cursor stays where it is.
    pub fn next_chip(&mut self, forward: bool) {
        let n = self.items.len();
        let chips = self.item().map_or(0, |i| i.chips.len());
        let along = match (self.chip, forward) {
            (Some(c), true) if c.saturating_add(1) < chips => Some(c.saturating_add(1)),
            (None, true) if chips > 0 => Some(0),
            (Some(c), false) if c > 0 => Some(c.saturating_sub(1)),
            _ => None,
        };
        if along.is_some() {
            self.chip = along;
            return;
        }
        // The first chip of the next line holding one (the last chip of the
        // previous, backwards), coming round to this line last.
        let mut line = self.cursor;
        for _ in 0..n {
            line = if forward {
                line.saturating_add(1).checked_rem(n).unwrap_or(0)
            } else {
                line.checked_sub(1).unwrap_or_else(|| n.saturating_sub(1))
            };
            let chips = self.items.get(line).map_or(0, |i| i.chips.len());
            if chips > 0 {
                self.cursor = line;
                self.chip = Some(if forward { 0 } else { chips.saturating_sub(1) });
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

    /// The row above the selected one in its own list (a `[[row]]` above a
    /// `[[row]]`, an inner row above an inner row), with the name of the
    /// box it is in when that is a named one; `None` for a column or the
    /// first row of its list.
    #[must_use]
    pub fn above(&self, draft: &Draft) -> Option<(RowAt, Option<String>)> {
        let item = self.item()?;
        let above = match item.kind {
            ItemKind::Col => return None,
            ItemKind::Row => RowAt::row(item.at.row.checked_sub(1)?),
            ItemKind::Inner => RowAt { inner: Some(item.at.inner?.checked_sub(1)?), ..item.at },
        };
        let name = draft.row(above)?.get("box").and_then(Value::as_str).map(str::to_owned);
        Some((above, name))
    }

    /// Select the line of kind `kind` standing for the table at `at`, when
    /// the list has one.
    fn select_at(&mut self, at: RowAt, kind: ItemKind) {
        if let Some(i) = self.items.iter().position(|it| it.at == at && it.kind == kind) {
            self.cursor = i;
        }
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
        let x = x.saturating_sub(usize::from(area.x));
        let visible = y.saturating_sub(usize::from(area.y));
        self.chip = self
            .hits
            .get(visible)
            .and_then(|ranges| ranges.iter().position(|(start, end)| x >= *start && x < *end));
        true
    }

    /// Draw the list into `area`.
    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, draft: &Draft) {
        let height = usize::from(area.height);
        self.scroll = window(self.cursor, self.items.len(), height, self.scroll);
        let width = usize::from(area.width);
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut hits: Vec<Vec<(usize, usize)>> = Vec::new();
        for (i, item) in self.items.iter().enumerate().skip(self.scroll).take(height) {
            let selected = i == self.cursor;
            let label = format!("{:<8}", item.label());
            // The labels are ASCII, so a char is a cell.
            let mut x = label.chars().count().saturating_add(2);
            let mut ranges: Vec<(usize, usize)> = Vec::new();
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                label,
                if selected && self.chip.is_none() { Chrome::selected() } else { Chrome::title() },
            )];
            spans.push(Span::raw("  "));
            for (c, chip) in item.chips.iter().enumerate() {
                if c > 0
                    && chip.side == Side::Right
                    && item.chips.get(c.saturating_sub(1)).is_some_and(|p| p.side == Side::Left)
                {
                    spans.push(Span::styled(" │ ", Chrome::muted()));
                    x = x.saturating_add(3);
                }
                let on = selected && self.chip == Some(c);
                // A plain mark: the geometric dots draw two cells in some
                // terminals (CLAUDE.md § Conventions).
                let dot = if draft.module_has_overrides(&chip.id) { "*" } else { " " };
                let text = format!("{}{dot}", chip.id);
                let w = text.chars().count();
                ranges.push((x, x.saturating_add(w)));
                x = x.saturating_add(w).saturating_add(1);
                spans.push(Span::styled(text, if on { Chrome::selected() } else { Style::new() }));
                spans.push(Span::raw(" "));
            }
            hits.push(ranges);
            if item.spacer {
                spans.push(Span::styled("(spacer)", Chrome::muted()));
            } else if item.chips.is_empty()
                && item.kind != ItemKind::Col
                && !draft.row(item.at).is_some_and(|t| t.contains_key("col"))
            {
                spans.push(Span::styled("(empty)", Chrome::muted()));
            }
            if !item.note.is_empty() {
                spans.push(Span::styled(format!("  {}", item.note), Chrome::muted()));
            }
            // Cut span by span, so a selection past the cut still shows.
            lines.push(Line::from(clip_spans(spans, width)));
        }
        self.hits = hits;
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
        let Some(mut item) = self.item().cloned() else {
            return Err("no row selected; add one with a".into());
        };
        // A row of columns and a stacked column hold no modules themselves:
        // the module goes to the last column, or the last inner row of the
        // stack, and the cursor follows it.
        let landed = if item.chips.is_empty() && holds_lists(draft, item.at) {
            let target = self
                .items
                .iter()
                .rposition(|it| {
                    it.at.row == item.at.row
                        && (item.at.col.is_none() || it.at.col == item.at.col)
                        && !holds_lists(draft, it.at)
                })
                .ok_or("this line holds columns; select a column or an inner row")?;
            self.cursor = target;
            self.chip = None;
            item = self.items.get(target).cloned().ok_or("no such line")?;
            format!(" to {}", item.label().trim())
        } else {
            String::new()
        };
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
        Ok(format!("added {id}{landed}"))
    }

    /// Remove the selected module, or the selected line when no chip is;
    /// the last `[[row]]` stays, and a box the line was the last member of
    /// goes with its table.
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
            set_group(table, chip.side, &list);
            self.rebuild(draft);
            return Ok(format!("removed {}", chip.id));
        }
        let siblings = draft.siblings_mut(item.at).ok_or("no such row")?;
        // No `[[row]]` at all means the preset's rows (SPEC § 4), which the
        // list cannot show and the next `a` would write back.
        if item.kind == ItemKind::Row && siblings.len() <= 1 {
            return Err("a status line needs a row; space makes it a spacer".into());
        }
        let index = item.index();
        if index < siblings.len() {
            siblings.remove(index);
        }
        // An emptied list is dropped with its key, so the column is a
        // plain column again (and the row a plain row) rather than a
        // stack or a grid of nothing that refuses every edit.
        if siblings.is_empty() {
            let parent = match item.kind {
                ItemKind::Inner => draft.row_mut(RowAt { inner: None, ..item.at }),
                ItemKind::Col => draft.row_mut(RowAt::row(item.at.row)),
                ItemKind::Row => None,
            };
            if let Some(table) = parent {
                table.remove(if item.kind == ItemKind::Inner { "row" } else { "col" });
            }
        }
        // The line may have been a box's last member.
        let orphans = draft.prune_orphan_boxes();
        self.rebuild(draft);
        let mut out = format!("deleted {}", item.label().trim());
        if !orphans.is_empty() {
            out.push_str("; ");
            out.push_str(&dropped_boxes(&orphans));
        }
        Ok(out)
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
        let index = item.index();
        let at = if after { index.saturating_add(1).min(siblings.len()) } else { index };
        let fresh = if item.kind == ItemKind::Col {
            Value::Table(Table::new())
        } else {
            Value::Table(new_row())
        };
        siblings.insert(at, fresh);
        self.rebuild(draft);
        self.select_at(item.sibling(at), item.kind);
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
        let index = item.index();
        let Some(copy) = siblings.get(index).cloned() else {
            return Err("nothing to clone".into());
        };
        siblings.insert(index.saturating_add(1), copy);
        self.rebuild(draft);
        self.select_at(item.sibling(index.saturating_add(1)), item.kind);
        self.chip = None;
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
        let index = item.index();
        let to = if down { index.saturating_add(1) } else { index.saturating_sub(1) };
        if to >= siblings.len() || to == index {
            return Err("already at the end".into());
        }
        siblings.swap(index, to);
        self.rebuild(draft);
        self.select_at(item.sibling(to), item.kind);
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
        set_group(table, chip.side, &from);
        set_group(table, other, &to);
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
    /// Past the last (or first) column a new one is made for it, and a
    /// plain row gets its columns on the way, so a column starts from the
    /// module that goes in it; a module alone in its column is not moved
    /// into a new one, since that would only leave an empty column behind.
    ///
    /// # Errors
    /// Why nothing moved, for the status bar.
    pub fn switch_column(&mut self, draft: &mut Draft, next: bool) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let Some(chip) = self.selected_chip().cloned() else {
            return Err("select a module first".into());
        };
        let row = RowAt::row(item.at.row);
        let alone = item.chips.len() == 1;
        // A plain row becomes one column holding its groups; the module's
        // own table is then that column.
        let mut from_at = if item.at.col.is_none() {
            if alone {
                return Err("the row's only module; m adds another first".into());
            }
            split_into_column(draft.row_mut(row).ok_or("no such row")?);
            RowAt { row: item.at.row, col: Some(0), inner: None }
        } else {
            item.at
        };
        let col = from_at.col.unwrap_or(0);
        let count =
            draft.row(row).and_then(|t| t.get("col")).and_then(Value::as_array).map_or(0, Vec::len);
        let at_edge = if next { col.saturating_add(1) >= count } else { col == 0 };
        let mut made = false;
        let target = if at_edge {
            if alone {
                return Err(format!(
                    "already in the {} column, and alone in it",
                    if next { "last" } else { "first" }
                ));
            }
            // A new column beside this one; inserting before shifts this
            // one right.
            let insert_at = if next { col.saturating_add(1) } else { col };
            let cols = draft
                .row_mut(row)
                .and_then(|t| t.get_mut("col"))
                .and_then(Value::as_array_mut)
                .ok_or("no columns")?;
            cols.insert(insert_at.min(cols.len()), Value::Table(Table::new()));
            made = true;
            if !next {
                from_at.col = Some(col.saturating_add(1));
            }
            insert_at
        } else if next {
            col.saturating_add(1)
        } else {
            col.saturating_sub(1)
        };
        let target_at = RowAt { row: item.at.row, col: Some(target), inner: None };
        if draft.row(target_at).is_none() {
            return Err("no column there".into());
        }
        if draft.row(target_at).is_some_and(|t| t.contains_key("row")) {
            return Err("that column is a stack; select one of its rows".into());
        }
        let from_table = draft.row_mut(from_at).ok_or("no such row")?;
        let mut from = ids(from_table, chip.side.key());
        if chip.index >= from.len() {
            return Err("no such module".into());
        }
        let id = from.remove(chip.index);
        set_group(from_table, chip.side, &from);
        let to_table = draft.row_mut(target_at).ok_or("no such column")?;
        let mut to = ids(to_table, "modules");
        to.push(id.clone());
        set_group(to_table, Side::Left, &to);
        self.rebuild(draft);
        self.select_at(target_at, ItemKind::Col);
        self.chip = self.item().and_then(|i| i.chips.iter().position(|c| c.id == id));
        let n = target.saturating_add(1);
        Ok(if made {
            format!("{id} moved to a new column {n}")
        } else {
            format!("{id} moved to column {n}")
        })
    }

    /// Give the selected row a column: a plain row's own groups become the
    /// first column and an empty one follows; on a row that has columns the
    /// new one goes after the selected column (at the end from the row
    /// line). The cursor lands on the new column, so `m` fills it.
    ///
    /// # Errors
    /// Why no column was added, for the status bar.
    pub fn add_column(&mut self, draft: &mut Draft) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        let at = RowAt::row(item.at.row);
        let table = draft.row_mut(at).ok_or("no such row")?;
        let index = if let Some(cols) = table.get_mut("col").and_then(Value::as_array_mut) {
            let index = item.at.col.map_or(cols.len(), |c| c.saturating_add(1).min(cols.len()));
            cols.insert(index, Value::Table(Table::new()));
            index
        } else {
            split_into_column(table);
            if let Some(cols) = table.get_mut("col").and_then(Value::as_array_mut) {
                cols.push(Value::Table(Table::new()));
            }
            1
        };
        self.rebuild(draft);
        self.select_at(RowAt { row: at.row, col: Some(index), inner: None }, ItemKind::Col);
        self.chip = None;
        Ok(format!("added column {}; m adds a module to it", index.saturating_add(1)))
    }

    /// Box the selected row together with the row above it (`B`): the row
    /// joins the named box the row above is in, or both go into a new
    /// `[box.<name>]`, which takes the title either row carried (a row in
    /// a named box has none of its own, SPEC § 4.3).
    ///
    /// # Errors
    /// Why nothing was boxed, for the status bar.
    pub fn box_with_above(&mut self, draft: &mut Draft, name: &str) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        if item.kind == ItemKind::Col {
            return Err("a column is boxed on its own: b".into());
        }
        let above_at = item.sibling(item.index().checked_sub(1).ok_or("no row above this one")?);
        let joined = draft
            .row(above_at)
            .and_then(|t| t.get("box"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let name = joined.clone().unwrap_or_else(|| name.to_owned());
        if name.is_empty() {
            return Err("a box needs a name".into());
        }
        // A row in a named box has no title of its own (SPEC § 4.3): the
        // first title either row carries becomes a new box's, and every
        // other goes, all of them when the box exists already.
        let existed = draft.get(&["box", &name]).is_some();
        let mut carried: Vec<(String, Value)> = Vec::new();
        let mut titled_rows = 0_usize;
        for at in [above_at, item.at] {
            let Some(t) = draft.row_mut(at) else { continue };
            let titled = t.contains_key("title");
            if titled {
                titled_rows = titled_rows.saturating_add(1);
            }
            for key in TITLE_KEYS {
                if let Some(v) = t.remove(key)
                    && titled
                    && carried.iter().all(|(k, _)| k != key)
                {
                    carried.push((key.to_owned(), v));
                }
            }
        }
        let lost = if existed { titled_rows } else { titled_rows.saturating_sub(1) };
        if !existed {
            if carried.is_empty() {
                carried.push(("title".to_owned(), Value::String(name.clone())));
            }
            for (key, v) in carried {
                draft.set(&["box", &name, &key], v);
            }
        }
        for at in [above_at, item.at] {
            let table = draft.row_mut(at).ok_or("no such row")?;
            table.insert("box".to_owned(), Value::String(name.clone()));
        }
        // A row that left another box may have been its last member.
        let orphans = draft.prune_orphan_boxes();
        self.rebuild(draft);
        let mut out = match (joined.is_some(), existed) {
            (true, _) => format!("joined box {name} with the row above"),
            (false, true) => format!("both rows joined box {name}"),
            (false, false) => format!(
                "both rows in a new box {name}; enter on a row edits it, [box.{name}] holds the title"
            ),
        };
        if lost > 0 {
            let what = if lost == 1 { "the row's title went" } else { "the rows' titles went" };
            let _ = write!(out, "; {what} ([box.{name}] carries one)");
        }
        if !orphans.is_empty() {
            out.push_str("; ");
            out.push_str(&dropped_boxes(&orphans));
        }
        Ok(out)
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

    /// Make the selected row a spacer (`modules = []`); its modules go, and
    /// `m` is the way back.
    ///
    /// # Errors
    /// Why nothing changed, for the status bar.
    pub fn make_spacer(&mut self, draft: &mut Draft) -> Result<String, String> {
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

    /// Set the selected row's, column's or inner row's `box` as the form's
    /// `box` field reads it: a name (its `[box.<name>]` made when missing),
    /// `true` for a box of its own, `None` for no box. A box the line was
    /// the last member of goes with its table, which the parser would
    /// otherwise report on every tick.
    ///
    /// # Errors
    /// Why nothing was boxed, for the status bar.
    pub fn set_box(&mut self, draft: &mut Draft, value: Option<Value>) -> Result<String, String> {
        let Some(item) = self.item().cloned() else { return Err("nothing selected".into()) };
        if let Some(Value::String(name)) = &value
            && draft.get(&["box", name]).is_none()
        {
            draft.set(&["box", name, "title"], Value::String(name.clone()));
        }
        let table = draft.row_mut(item.at).ok_or("no such row")?;
        let mut out = match value {
            None => {
                table.remove("box");
                "unboxed".to_owned()
            }
            Some(Value::String(name)) => {
                let out = format!("in box {name}");
                table.insert("box".to_owned(), Value::String(name));
                out
            }
            Some(other) => {
                table.insert("box".to_owned(), other);
                "boxed".to_owned()
            }
        };
        let orphans = draft.prune_orphan_boxes();
        if !orphans.is_empty() {
            out.push_str("; ");
            out.push_str(&dropped_boxes(&orphans));
        }
        self.rebuild(draft);
        Ok(out)
    }
}

/// A fresh `[[row]]`: no modules yet, so it reads as a spacer until one is
/// added.
fn new_row() -> Table {
    let mut t = Table::new();
    t.insert("modules".to_owned(), Value::Array(Vec::new()));
    t
}

/// Whether the table at `at` holds `[[row.col]]` or `[[row.col.row]]`
/// tables rather than modules of its own.
fn holds_lists(draft: &Draft, at: RowAt) -> bool {
    draft.row(at).is_some_and(|t| t.contains_key("col") || t.contains_key("row"))
}

/// Turn a plain row's groups into its first (and only) `[[row.col]]`.
fn split_into_column(table: &mut Table) {
    let mut first = Table::new();
    if let Some(m) = table.remove("modules") {
        first.insert("modules".to_owned(), m);
    }
    if let Some(r) = table.remove("right") {
        first.insert("right".to_owned(), r);
    }
    table.insert("col".to_owned(), Value::Array(vec![Value::Table(first)]));
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
        d.row(at).map_or_default(|t| ids(t, key))
    }

    /// The rules a mutation pass found no test for: where a row-selected
    /// add lands, an emptied `right` going with its key, a spacer dropping
    /// its right group, a move into a stack refused, the cursor after a
    /// clone, and a shift up.
    #[test]
    fn edge_rules_of_the_edits_hold() {
        let mut d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        assert_eq!(b.add_module(&mut d, "clock").unwrap(), "added clock");
        assert_eq!(ids_at(&d, RowAt::row(0), "modules"), vec!["path", "branch", "clock"]);
        // Delete the lone right module: the key goes with it.
        b.select_row(0);
        b.chip = b.items[0].chips.iter().position(|c| c.side == Side::Right);
        assert!(b.chip.is_some());
        b.delete(&mut d).unwrap();
        assert!(d.row(RowAt::row(0)).unwrap().get("right").is_none());
        // A spacer drops its right group too.
        let mut d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        b.make_spacer(&mut d).unwrap();
        let row = d.row(RowAt::row(0)).unwrap();
        assert!(row.get("right").is_none());
        assert_eq!(ids(row, "modules"), Vec::<String>::new());
        // A move into a stacked column is refused.
        let mut d = Draft::from_text(
            "[[row]]\n[[row.col]]\nmodules = [\"path\"]\n[[row.col]]\n[[row.col.row]]\nmodules = [\"clock\"]\n",
        );
        let mut b = Builder::default();
        b.rebuild(&d);
        b.move_line(true);
        b.move_chip(true);
        assert_eq!(b.selected_id(), Some("path"));
        let err = b.switch_column(&mut d, true).unwrap_err();
        assert!(err.contains("stack"), "{err}");
        // A clone selects the copy; a shift up swaps with the line above.
        let mut d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        b.clone_line(&mut d).unwrap();
        assert_eq!(b.cursor, 1);
        b.move_line(true);
        b.move_line(true);
        assert_eq!(b.cursor, 2);
        b.shift(&mut d, false).unwrap();
        assert_eq!(b.cursor, 1);
        assert_eq!(ids_at(&d, RowAt::row(1), "modules"), vec!["model"]);
    }

    /// Columns grow from where the cursor is (2026-09-20): `C` inserts after
    /// the selected column and selects the new one, `]` and `[` past the
    /// edge make a column for the module, a plain row splits from its
    /// module, `m` on a row of columns lands in the last one, and `B` boxes
    /// two rows together.
    #[test]
    fn columns_grow_from_the_selection_and_boxes_join_the_row_above() {
        let col = |c: usize| RowAt { row: 0, col: Some(c), inner: None };
        let mut d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        let msg = b.add_column(&mut d).unwrap();
        assert!(msg.starts_with("added column 2"), "{msg}");
        assert_eq!(b.item().map(|i| (i.kind, i.at)), Some((ItemKind::Col, col(1))));
        assert_eq!(ids_at(&d, col(0), "modules"), vec!["path", "branch"]);
        // `C` on column 1 inserts after it, not at the end.
        b.move_line(false);
        let msg = b.add_column(&mut d).unwrap();
        assert!(msg.starts_with("added column 2"), "{msg}");
        let cols = d.row(RowAt::row(0)).unwrap().get("col").unwrap().as_array().unwrap().len();
        assert_eq!(cols, 3);
        assert_eq!(b.item().map(|i| i.at), Some(col(1)));
        // `m` on the row line goes to the last column, and says so.
        b.select_row(0);
        assert_eq!(b.add_module(&mut d, "model").unwrap(), "added model to col 3");
        assert_eq!(ids_at(&d, col(2), "modules"), vec!["model"]);
        assert_eq!(b.selected_id(), Some("model"));
        // `]` past the last column makes a new one, unless the module is
        // alone in its column.
        let err = b.switch_column(&mut d, true).unwrap_err();
        assert!(err.contains("alone"), "{err}");
        assert!(b.select_module("branch", Some(0)));
        assert_eq!(b.switch_column(&mut d, true).unwrap(), "branch moved to column 2");
        assert_eq!(b.selected_id(), Some("branch"));
        b.switch_column(&mut d, true).unwrap();
        assert_eq!(ids_at(&d, col(2), "modules"), vec!["model", "branch"]);
        assert_eq!(b.switch_column(&mut d, true).unwrap(), "branch moved to a new column 4");
        assert_eq!(ids_at(&d, col(3), "modules"), vec!["branch"]);
        // `[` at the first column makes one before it, shifting the rest.
        assert!(b.select_module("path", Some(0)));
        assert_eq!(b.switch_column(&mut d, false).unwrap(), "path moved to a new column 1");
        assert_eq!(ids_at(&d, col(0), "modules"), vec!["path"]);
        assert_eq!(ids_at(&d, col(1), "right"), vec!["clock"]);
        assert_eq!(b.selected_id(), Some("path"));
        let (_, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        // A plain row splits into columns from its module; its only module
        // is refused.
        let mut d = draft();
        let mut b = Builder::default();
        b.rebuild(&d);
        assert!(b.select_module("branch", Some(0)));
        assert_eq!(b.switch_column(&mut d, true).unwrap(), "branch moved to a new column 2");
        assert_eq!(ids_at(&d, col(0), "modules"), vec!["path"]);
        assert_eq!(ids_at(&d, col(1), "modules"), vec!["branch"]);
        assert_eq!(d.resolved().1, Vec::new());
        assert!(b.select_module("model", Some(1)));
        assert!(b.switch_column(&mut d, true).unwrap_err().contains("only module"));
        // `B`: a new box for two rows takes the title the row above carried;
        // a third row joins it; the first row and a column have no above.
        let mut d = Draft::from_text(
            "[[row]]\nmodules = [\"path\"]\ntitle = \"Repo\"\ntitle_justify = \"center\"\n[[row]]\nmodules = [\"model\"]\n[[row]]\nmodules = [\"clock\"]\n",
        );
        let mut b = Builder::default();
        b.rebuild(&d);
        assert!(b.above(&d).is_none(), "the first row has none above");
        assert!(b.box_with_above(&mut d, "x").is_err());
        b.select_row(1);
        assert_eq!(b.above(&d), Some((RowAt::row(0), None)));
        let msg = b.box_with_above(&mut d, "repo").unwrap();
        assert!(msg.contains("new box repo"), "{msg}");
        assert_eq!(d.get(&["box", "repo", "title"]).and_then(Value::as_str), Some("Repo"));
        assert_eq!(
            d.get(&["box", "repo", "title_justify"]).and_then(Value::as_str),
            Some("center")
        );
        assert!(d.row(RowAt::row(0)).unwrap().get("title").is_none(), "the title moved");
        b.select_row(2);
        assert_eq!(b.above(&d), Some((RowAt::row(1), Some("repo".into()))));
        let msg = b.box_with_above(&mut d, "ignored").unwrap();
        assert!(msg.starts_with("joined box repo"), "{msg}");
        let (config, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        assert!(config.rows.iter().all(|r| r.boxed.is_some()));
        assert!(b.box_with_above(&mut d, "").is_ok(), "a joined box needs no name");
        let mut d = Draft::from_text(
            "[[row]]\n[[row.col]]\nmodules = [\"path\"]\n[[row.col]]\nmodules = [\"clock\"]\n",
        );
        let mut b = Builder::default();
        b.rebuild(&d);
        b.move_line(true);
        b.move_line(true);
        assert!(b.above(&d).is_none());
        assert!(b.box_with_above(&mut d, "x").unwrap_err().contains("column"));
    }

    /// app-30: `Tab` on a list with no chip leaves the cursor; a row is a
    /// spacer by its keys, not by a note that a title can spell; a line
    /// wider than the list keeps its selection when it is cut.
    #[test]
    fn tab_stays_put_spacers_are_flags_and_cut_lines_keep_the_selection() {
        let mut d = Draft::from_text(
            "[[row]]\nmodules = []\n[[row]]\nmodules = []\n[[row]]\nmodules = []\n",
        );
        let mut b = Builder::default();
        b.rebuild(&d);
        b.move_line(true);
        b.next_chip(true);
        assert_eq!((b.cursor, b.chip), (1, None), "no chip anywhere: Tab stays");
        b.next_chip(false);
        assert_eq!((b.cursor, b.chip), (1, None));
        assert!(b.items.iter().all(|i| i.spacer && !i.note.contains("spacer")));
        let titled = Draft::from_text("[[row]]\ntitle = \"spacer row\"\n");
        b.rebuild(&titled);
        assert!(!b.items[0].spacer, "a title is not a spacer");
        let shot = |b: &mut Builder, d: &Draft, width: u16| {
            let Ok(mut t) = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 3));
            let Ok(_) = t.draw(|f| b.draw(f, f.area(), d));
            t.backend().buffer().clone()
        };
        let text = |buf: &ratatui::buffer::Buffer| -> String {
            (0..buf.area.width).map(|x| buf.cell((x, 0)).unwrap().symbol().to_owned()).collect()
        };
        assert!(text(&shot(&mut b, &titled, 40)).contains("(empty)"));
        // A chip past the width it is cut to still shows selected.
        d = Draft::from_text(
            "[[row]]\nmodules = [\"path\", \"branch\", \"sync\", \"model\", \"context\", \"limit5h\"]\n",
        );
        b = Builder::default();
        b.rebuild(&d);
        b.move_chip(true);
        b.move_chip(true);
        assert_eq!(b.selected_id(), Some("branch"));
        let buf = shot(&mut b, &d, 30);
        let line = text(&buf);
        assert!(line.trim_end().ends_with('…'), "{line}");
        let at = line.find("branch").unwrap();
        let x = u16::try_from(line.char_indices().take_while(|(i, _)| *i < at).count()).unwrap();
        let cell = buf.cell((x, 0)).unwrap();
        assert!(cell.modifier.contains(ratatui::style::Modifier::REVERSED), "{line}");
    }

    /// app-13: `B` naming a box that exists joins it, says so, and says
    /// that the rows' titles went (the box carries its own); a new box
    /// takes one title and says so when the other went.
    #[test]
    fn b_into_an_existing_box_says_the_titles_went() {
        let text = "[box.x]\ntitle = \"X\"\n[[row]]\nbox = \"x\"\nmodules = [\"a\"]\n[[row]]\ntitle = \"T\"\nmodules = [\"b\"]\n[[row]]\nmodules = [\"c\"]\n";
        let mut d = Draft::from_text(text);
        let mut b = Builder::default();
        b.rebuild(&d);
        b.select_row(2);
        let msg = b.box_with_above(&mut d, "x").unwrap();
        assert!(msg.contains("title went") && !msg.contains("new box"), "{msg}");
        assert!(msg.starts_with("both rows joined box x"), "{msg}");
        assert_eq!(d.get(&["box", "x", "title"]).and_then(Value::as_str), Some("X"));
        let text = "[[row]]\ntitle = \"A\"\nmodules = [\"a\"]\n[[row]]\ntitle = \"B\"\nmodules = [\"b\"]\n";
        let mut d = Draft::from_text(text);
        let mut b = Builder::default();
        b.rebuild(&d);
        b.select_row(1);
        let msg = b.box_with_above(&mut d, "ab").unwrap();
        assert!(msg.contains("new box ab") && msg.contains("title went"), "{msg}");
        assert_eq!(d.get(&["box", "ab", "title"]).and_then(Value::as_str), Some("A"));
    }

    /// app-12: a clone selects the copy, not the next line of the list,
    /// which for a row of columns or a stacked column is the original's
    /// own first child.
    #[test]
    fn a_clone_selects_the_copy() {
        let mut d = Draft::from_text(
            "[[row]]\n[[row.col]]\nmodules = [\"path\"]\n[[row.col]]\n[[row.col.row]]\nmodules = [\"clock\"]\n",
        );
        let mut b = Builder::default();
        b.rebuild(&d);
        b.clone_line(&mut d).unwrap();
        assert_eq!(b.item().map(|i| (i.kind, i.at)), Some((ItemKind::Row, RowAt::row(1))));
        b.select_row(0);
        b.move_line(true);
        b.move_line(true);
        let stack = RowAt { row: 0, col: Some(1), inner: None };
        assert_eq!(b.item().map(|i| (i.kind, i.at)), Some((ItemKind::Col, stack)));
        b.clone_line(&mut d).unwrap();
        let copy = RowAt { col: Some(2), ..stack };
        assert_eq!(b.item().map(|i| (i.kind, i.at)), Some((ItemKind::Col, copy)));
    }

    /// app-08: the last `[[row]]` is not deleted: an empty row list means
    /// the preset's rows to the parser, which the list could not show and
    /// the next `a` would write back.
    #[test]
    fn the_last_row_stays() {
        let mut d = Draft::from_text("[[row]]\nmodules = [\"clock\"]\n");
        let mut b = Builder::default();
        b.rebuild(&d);
        let err = b.delete(&mut d).unwrap_err();
        assert!(err.contains("spacer"), "{err}");
        assert_eq!(d.rows().len(), 1);
        // Its module still goes, and a second row can go.
        b.move_chip(true);
        b.delete(&mut d).unwrap();
        b.insert(&mut d, true).unwrap();
        b.delete(&mut d).unwrap();
        assert_eq!(d.rows().len(), 1);
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
        assert!(b.items[2].spacer);
        b.make_spacer(&mut d).unwrap();
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
        b.set_box(&mut d, Some(Value::String("repo".into()))).unwrap();
        assert!(d.get(&["box", "repo", "title"]).is_some());
        let (config, errs) = d.resolved();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(config.rows[1].cols.len(), 2);
        assert_eq!(config.rows[1].cols[1].rows.len(), 1);
        // The whole thing still saves as a file the parser reads back.
        let again = Draft::from_text(&d.text().unwrap());
        assert_eq!(again.resolved().0, config);
    }
}
