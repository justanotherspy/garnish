//! Laying a configured row out into terminal lines (SPEC § 4.3).
//!
//! A **row** is columns side by side, one or more **lines** tall; a column
//! holds its own module groups or a stack of inner rows; titles decorate
//! rules and boxes decorate rows and columns. Everything here is arithmetic
//! over segments the modules have already rendered: nothing is read, nothing
//! is spawned, and a row of one `1fr` column is exactly the flex line garnish
//! has always drawn.

use std::ops::Range;

use crate::ansi::{
    Color, Segment, Style, cluster_width, display_width, scroll, scroll_period, segments_width,
    truncate,
};
use crate::config::{BoxCfg, BoxRef, Justify, SeparatorColor, VAlign, Width};
use crate::frame::{BLANK_CELL, FrameChars, FrameStyle, Rule, Ticker};
use crate::theme::{Role, Theme};

/// What one piece of a rendered line is.
///
/// The kinds are what `setup`'s placement map reads (SPEC § 14), so a click
/// in the preview can land on the thing under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elem {
    /// A frame cap (`╭─`, `─╮`) at the end of a line.
    Cap,
    /// A box corner or side glyph.
    BoxEdge,
    /// Rule cells: the frame's `fill_char`, or this tick's `fill_pattern`.
    Rule,
    /// Empty cells between two columns.
    Gap,
    /// The pad between a cap or edge and the content, and around a group.
    Pad,
    /// One module's decorated render, by its id (`text.<name>` for a text
    /// module).
    Module(String),
    /// The separator between two modules.
    Separator,
    /// A run of modules cut or scrolled as one, with the cells each module
    /// still owns, counted from the run's first cell: a module may own two
    /// runs when it straddles the ticker's wrap, the module a cut lands in
    /// owns the ellipsis cells, and a module wholly outside the window owns
    /// none (SPEC § 14). Empty for a whole line recut to the width or given
    /// the braille blank of a `blank` row: neither names a module.
    Group(Vec<(String, Range<usize>)>),
    /// A row's or a box's title.
    Title,
}

impl Elem {
    /// The module ids that own cells of this piece, each with the cells it
    /// owns counted from the piece's first cell.
    #[must_use]
    pub fn owners(&self, width: usize) -> Vec<(String, Range<usize>)> {
        match self {
            Self::Module(id) => vec![(id.clone(), 0..width)],
            Self::Group(map) => map.clone(),
            _ => Vec::new(),
        }
    }
}

/// One piece of a line: what it is, and the segments that draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// What this piece is.
    pub elem: Elem,
    /// Its segments, already reduced to plain text by their constructors.
    pub segs: Vec<Segment>,
}

/// One terminal line: its pieces, left to right.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    /// The pieces, in the order they are drawn.
    pub pieces: Vec<Piece>,
}

impl Line {
    /// The line's segments, ready for the painter.
    #[must_use]
    pub fn segments(&self) -> Vec<Segment> {
        self.pieces.iter().flat_map(|p| p.segs.iter().cloned()).collect()
    }

    /// [`Self::segments`] for a caller that owns the line: the tick paints
    /// each line once and drops it, and copying every segment to do that was
    /// a measurable part of the render.
    #[must_use]
    pub fn into_segments(self) -> Vec<Segment> {
        self.pieces.into_iter().flat_map(|p| p.segs).collect()
    }

    /// The line's width in cells.
    #[must_use]
    pub fn width(&self) -> usize {
        self.pieces.iter().map(|p| segments_width(&p.segs)).sum()
    }

    /// Each piece with the cells it occupies, for the placement map.
    #[must_use]
    pub fn spans(&self) -> Vec<(Elem, Range<usize>)> {
        let mut at = 0_usize;
        self.pieces
            .iter()
            .map(|p| {
                let end = at.saturating_add(segments_width(&p.segs));
                let span = (p.elem.clone(), at..end);
                at = end;
                span
            })
            .collect()
    }

    /// Every module that owns cells of this line, with the cells it owns:
    /// the placement map of SPEC § 14, in line order.
    #[must_use]
    pub fn modules(&self) -> Vec<(String, Range<usize>)> {
        self.spans()
            .into_iter()
            .flat_map(|(elem, range)| {
                elem.owners(range.end.saturating_sub(range.start)).into_iter().map(
                    move |(id, r)| {
                        (id, range.start.saturating_add(r.start)..range.start.saturating_add(r.end))
                    },
                )
            })
            .collect()
    }

    /// The line cut to `width` cells as one piece: a cut can split any piece,
    /// so the pieces, and the placement map with them, cannot be kept.
    fn recut(&self, width: usize, ellipsis: &str) -> Self {
        let segs = truncate(&self.segments(), width, ellipsis);
        Self { pieces: vec![Piece { elem: Elem::Group(Vec::new()), segs }] }
    }
}

/// One row's worth of content, rendered and ready to be laid out.
#[derive(Debug, Clone)]
pub struct Row<'a> {
    /// The row's columns, left to right; never empty.
    pub cols: Vec<Col<'a>>,
    /// Cells between two drawn columns: the rule on a one-line row under a
    /// rule, spaces otherwise.
    pub gap: usize,
    /// The separator between this row's modules.
    pub separator: &'a str,
    /// The row's title, or the title of the box it is drawn as.
    pub title: Option<&'a crate::config::TitleCfg>,
    /// The box this row joins.
    pub boxed: Option<&'a BoxRef>,
    /// Keep the row on screen even when it is whitespace only (SPEC § 4.1).
    pub blank: bool,
}

/// One column's content.
#[derive(Debug, Clone)]
pub struct Col<'a> {
    /// The column's share of the row's width.
    pub width: Width,
    /// Where a lone group sits.
    pub justify: Justify,
    /// Where a short stack sits in a taller row.
    pub valign: VAlign,
    /// The box drawn around the whole column.
    pub boxed: Option<&'a BoxRef>,
    /// What the column holds.
    pub content: Content<'a>,
}

/// A column holds module groups or a stack of rows, never both.
#[derive(Debug, Clone)]
pub enum Content<'a> {
    /// `modules` and `right`, one entry per module that rendered something.
    ///
    /// Borrowed from the render, which holds them for the whole tick: the
    /// layout only measures and copies what it draws.
    Groups {
        /// The left-anchored modules.
        left: &'a [Vec<Segment>],
        /// The right-anchored modules.
        right: &'a [Vec<Segment>],
        /// The id behind each entry of `left`, for the placement map
        /// (SPEC § 14); an entry with none is drawn as an unnamed module.
        left_ids: &'a [String],
        /// The id behind each entry of `right`.
        right_ids: &'a [String],
    },
    /// `[[row.col.row]]`: the column is a stack of rows.
    Stack(Vec<Row<'a>>),
}

/// How the empty cells of a line are filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    /// The frame's rule (or this tick's pattern): `[frame] fill = true`.
    Rule,
    /// Spaces: inside a box, whose sides already close the line.
    Spaces,
    /// Nothing: `[frame] fill = false` packs a row to the left and the
    /// right group follows the left one after a separator (SPEC § 4.1).
    Packed,
}

/// What the layout needs that does not change between rows.
#[derive(Debug, Clone)]
pub struct Layout<'a> {
    /// The frame characters in effect.
    pub chars: &'a FrameChars,
    /// The frame's style, which a box inherits when it names none.
    pub style: FrameStyle,
    /// The theme, for the frame and separator colours.
    pub theme: &'a Theme,
    /// `[frame] separator_color` (SPEC § 4.1).
    pub separator_color: &'a SeparatorColor,
    /// `[frame] fill`.
    pub fill: bool,
    /// Cut a line that overflows its width.
    pub truncate: bool,
    /// The ellipsis a cut ends in.
    pub ellipsis: &'a str,
    /// The scrolling window, when one is in effect this tick.
    pub ticker: Option<Ticker>,
    /// This tick's rule pattern, when the frame has one.
    pub rule: Option<Rule>,
    /// The width of Claude Code's box in cells.
    pub width: usize,
    /// The `[box.<name>]` tables a row or column may join.
    pub boxes: &'a std::collections::BTreeMap<String, BoxCfg>,
}

/// A piece before its rule cells are painted: the pattern's phase runs over
/// the whole line, so the rule text is only known once the line is complete
/// (SPEC § 4.3).
#[derive(Debug, Clone)]
enum Draft {
    /// Ready as it is.
    Done(Piece),
    /// `n` cells of rule, painted once the line's total is known.
    Rule(usize),
    /// `n` cells of space that are there only to place what follows: the
    /// cells a packed row would not draw at all are these, and only these
    /// are dropped from the end of such a row.
    Space(usize, Elem),
    /// No cells: this line belongs to a `blank = true` inner row, so once
    /// it is finished it keeps the braille cell if it is whitespace only
    /// (SPEC § 4.1). Only the finished line can tell: the outer row's caps,
    /// a box's sides or another column may already keep it on screen.
    Blank,
}

impl Draft {
    /// `n` spaces beside a title (`title_pad`): drawn as they are, never a
    /// placement run another title could land in.
    fn title_pad(n: usize) -> Self {
        Self::Done(Piece { elem: Elem::Pad, segs: vec![Segment::plain(" ".repeat(n))] })
    }

    fn cells(&self) -> usize {
        match self {
            Self::Done(p) => segments_width(&p.segs),
            Self::Rule(n) | Self::Space(n, _) => *n,
            Self::Blank => 0,
        }
    }
}

impl Layout<'_> {
    /// Lay every row out, returning the terminal lines of each configured
    /// row in order. A box around a run of rows puts its top line on the
    /// first row of the run and its bottom line on the last.
    ///
    /// A column's stack holds rows, so the type is recursive, but the
    /// nesting is two deep at most: an inner row takes no `col` key and the
    /// config rejects one (`row[0].col[0].row[0].col` is an unknown key), so
    /// nothing here walks deeper than `row → col → row → col`.
    #[must_use]
    pub fn lines(&self, rows: &[Row<'_>]) -> Vec<Vec<Line>> {
        let blocks = blocks(rows);
        // The frame's caps are decided over the lines that carry them: a
        // box draws its own ends, so its lines are not counted (SPEC § 4.3).
        let fill = self.frame_fill();
        let framed: Vec<&Row<'_>> = blocks
            .iter()
            .filter(|b| b.boxed.is_none())
            .flat_map(|b| b.rows.iter().map(|(_, r)| *r))
            .collect();
        let plan = self.frame_plan(&framed, fill);
        let count: usize = plan.iter().map(|(_, height)| *height).sum();
        let mut plan = plan.into_iter();
        let mut out: Vec<Vec<Line>> = vec![Vec::new(); rows.len()];
        let mut index = 0_usize;
        for block in blocks {
            let tagged = block.boxed.map_or_else(
                || {
                    let mut tagged = Vec::with_capacity(block.rows.len());
                    for (at, row) in &block.rows {
                        let Some((room, height)) = plan.next() else { break };
                        tagged.push((*at, self.frame_row(row, room, height, (index, count), fill)));
                        index = index.saturating_add(height);
                    }
                    tagged
                },
                // A box replaces the frame's caps with its own edges.
                |boxed| self.boxed_block(&block, boxed),
            );
            for (at, lines) in tagged {
                if let Some(slot) = out.get_mut(at) {
                    slot.extend(lines);
                }
            }
        }
        out
    }

    /// Each row of the frame's room and height, in order: the cells its
    /// columns share, and the lines it takes when laid out to them.
    ///
    /// A row's lines take the caps of where they land among the frame's
    /// (`first`, `middle`, `last`), and a `custom` frame's need not be one
    /// width: a row is laid out to the room the widest pair on its lines
    /// leaves, so no line overflows and the others fill the difference with
    /// rule cells. Where its lines land depends on the heights, and a
    /// height on the room (a box adds its two lines only where it fits,
    /// SPEC § 4.3), so every row starts on the fewest cells any pair of
    /// caps leaves ([`Self::frame_room`]) and each round measures it again
    /// on the caps its lines land on, until no row moves; under a style,
    /// whose caps are one width, the first round moves none. A height
    /// changes only where a box starts or stops fitting, so this settles
    /// within a few rounds or never: a row whose box fits under the narrow
    /// `single` caps but not under the `first` and `last` ones its three
    /// lines would take has no height that holds, and a frame that does
    /// not settle keeps the first measure, laid out to those cells.
    fn frame_plan(&self, rows: &[&Row<'_>], fill: Fill) -> Vec<(usize, usize)> {
        const ROUNDS: usize = 8;
        let start = || -> Vec<(usize, usize)> {
            rows.iter()
                .map(|row| {
                    let room = self.frame_room(row);
                    (room, self.row_height(row, room, fill))
                })
                .collect()
        };
        let mut plan = start();
        for _ in 0..ROUNDS {
            let count: usize = plan.iter().map(|(_, height)| *height).sum();
            let mut index = 0_usize;
            let mut settled = true;
            for (row, (room, height)) in rows.iter().zip(plan.iter_mut()) {
                let lines = index..index.saturating_add(*height);
                index = lines.end;
                let fits = lines.map(|i| self.inner_width(row, i, count)).min().unwrap_or(*room);
                if fits != *room {
                    settled = false;
                    *room = fits;
                    *height = self.row_height(row, fits, fill);
                }
            }
            if settled {
                return plan;
            }
        }
        start()
    }

    /// One row of the frame laid out to `room` cells and `height` lines,
    /// its first line being line `index` of the `count` that carry caps.
    fn frame_row(
        &self,
        row: &Row<'_>,
        room: usize,
        height: usize,
        (index, count): (usize, usize),
        fill: Fill,
    ) -> Vec<Line> {
        let widths = self.share(row, room, fill);
        let ends = self.ends_in_content(row, &widths);
        self.row_columns(row, &widths, height, fill, Fit::default())
            .into_iter()
            .enumerate()
            .map(|(i, drafts)| {
                // A row's title goes into its first line alone, however tall
                // the row is. A boxed row's belongs to its box, which draws
                // it on its top edge; none reaches here.
                let title = row.title.filter(|_| i == 0);
                self.wrap_frame(drafts, (index.saturating_add(i), count), row, fill, title, ends)
            })
            .collect()
    }

    /// The lines of a block drawn inside a box, each tagged with the
    /// configured row it belongs to: the top rule with the title goes with
    /// the first row, the bottom rule with the last.
    fn boxed_block(&self, block: &Block<'_, '_>, boxed: &BoxRef) -> Vec<(usize, Vec<Line>)> {
        let cfg = self.box_cfg(boxed);
        let chars = self.box_chars(&cfg);
        let (inner, pad) = self.box_interior(&chars, self.width).unwrap_or((0, 0));
        let body = self.block_body(block, inner, &cfg, Fit::default());
        let mut lines = self
            .box_lines(&cfg, &chars, self.width, pad, box_title(&cfg, block), body)
            .into_iter()
            .map(|drafts| self.paint(drafts, false));
        let first = block.rows.first().map_or(0, |(at, _)| *at);
        let last = block.rows.last().map_or(0, |(at, _)| *at);
        let mut out: Vec<(usize, Vec<Line>)> = vec![(first, lines.next().into_iter().collect())];
        for (at, row) in &block.rows {
            let height = self.row_height(row, inner, Fill::from_box(&cfg));
            out.push((*at, lines.by_ref().take(height).collect()));
        }
        out.push((last, lines.collect()));
        out
    }

    /// The cells line `index` of the `count` that carry the frame's caps
    /// leaves a row's columns, once its caps are taken off (the caps of a
    /// style are the same width on every line; a `custom` frame whose caps
    /// differ has the difference filled with rule cells before the cap).
    fn inner_width(&self, row: &Row<'_>, index: usize, count: usize) -> usize {
        let (prefix, cap) = self.chars.ends(index, count);
        let pad = display_width(&self.chars.pad);
        let prefix_w =
            if prefix.is_empty() { 0 } else { display_width(prefix).saturating_add(pad) };
        let cap_w = self.cap_width(cap, row);
        self.width.saturating_sub(prefix_w).saturating_sub(cap_w)
    }

    /// The cap and the pad before it. The pad is there only when the row's
    /// content would otherwise touch the cap, which is what garnish has
    /// always drawn: with no right group the rule runs into the cap.
    fn cap_width(&self, cap: &str, row: &Row<'_>) -> usize {
        if !self.fill || cap.is_empty() {
            return 0;
        }
        let pad = if row_ends_in_content(row) { display_width(&self.chars.pad) } else { 0 };
        display_width(cap).saturating_add(pad)
    }

    /// The cells a row of the frame can count on whichever of the frame's
    /// lines it lands on: the fewest any pair of caps leaves, where
    /// [`Self::frame_plan`] starts and what it falls back to. Under a style,
    /// whose caps are all one width, they are exactly the cells the row is
    /// laid out to.
    fn frame_room(&self, row: &Row<'_>) -> usize {
        [(0, 1), (0, 3), (1, 3), (2, 3)]
            .into_iter()
            .map(|(index, count)| self.inner_width(row, index, count))
            .min()
            .unwrap_or(0)
    }

    /// How a row of the frame fills its empty cells.
    const fn frame_fill(&self) -> Fill {
        if self.fill { Fill::Rule } else { Fill::Packed }
    }

    /// [`row_ends_in_content`] for a row laid out to `widths`: a last column
    /// that draws nothing at its share (it took no cells, being an `fr` one
    /// whose share floored to nothing or one dropped from a row too narrow
    /// for it, or its box does not fit them) leaves the column before it to
    /// end the row, and that one keeps a pad of its own (SPEC § 4.3).
    fn ends_in_content(&self, row: &Row<'_>, widths: &[usize]) -> bool {
        let fits = |boxed: Option<&BoxRef>, width: usize| {
            boxed.is_none_or(|b| {
                self.box_interior(&self.box_chars(&self.box_cfg(b)), width).is_some()
            })
        };
        let last_draws = row.cols.last().zip(widths.last()).is_some_and(|(col, w)| {
            *w > 0
                && fits(col.boxed, *w)
                && match &col.content {
                    Content::Groups { .. } => true,
                    Content::Stack(rows) => {
                        rows.iter().any(|r| fits(r.boxed, *w) && row_ends_in_content(r))
                    }
                }
        });
        last_draws && row_ends_in_content(row)
    }
}

/// A run of rows drawn together: the rows of one named box, one row in its
/// own `box = true`, or one bare row.
struct Block<'a, 'b> {
    rows: Vec<(usize, &'b Row<'a>)>,
    boxed: Option<&'a BoxRef>,
}

/// Group rows into blocks: adjacent rows naming the same box are one.
fn blocks<'a, 'b>(rows: &'b [Row<'a>]) -> Vec<Block<'a, 'b>> {
    let mut out: Vec<Block<'a, 'b>> = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        let joined = match (&row.boxed, out.last()) {
            (Some(BoxRef::Named(name)), Some(last)) => {
                last.boxed.and_then(BoxRef::name) == Some(name.as_str())
            }
            _ => false,
        };
        if joined {
            if let Some(last) = out.last_mut() {
                last.rows.push((at, row));
            }
        } else {
            out.push(Block { rows: vec![(at, row)], boxed: row.boxed });
        }
    }
    out
}

/// Whether the right edge of a row's last column carries content rather than
/// fill, on any of its lines. With no `fr` column the free width is a rule
/// after the last column, which keeps a pad of its own (SPEC § 4.3): the
/// row then ends in that rule, never in content.
fn row_ends_in_content(row: &Row<'_>) -> bool {
    has_fr(row) && row.cols.last().is_some_and(col_ends_in_content)
}

/// Whether any column of the row shares the free width.
fn has_fr(row: &Row<'_>) -> bool {
    row.cols.iter().any(|c| matches!(c.width, Width::Fr(_)))
}

/// The cells the drawn columns of `widths` take, with a gap between each
/// two of them.
fn drawn_cells(widths: &[usize], gap: usize) -> usize {
    let drawn = widths.iter().filter(|w| **w > 0).count();
    widths.iter().sum::<usize>().saturating_add(gap.saturating_mul(drawn.saturating_sub(1)))
}

/// Whether a column's text can meet a rule beside it: not when a box's side
/// stands between them on every line, as it does for a boxed column and a
/// stack of boxed rows.
fn meets_rule(col: &Col<'_>) -> bool {
    col.boxed.is_none()
        && match &col.content {
            Content::Groups { .. } => true,
            Content::Stack(rows) => rows.iter().any(|r| r.boxed.is_none()),
        }
}

/// Whether a column's content reaches its right edge. A `width = 0` column
/// takes no cells and draws nothing, so it never does.
fn col_ends_in_content(col: &Col<'_>) -> bool {
    col.width != Width::Cells(0)
        && match &col.content {
            Content::Groups { left, right, .. } => {
                !right.is_empty() || (col.justify == Justify::Right && !left.is_empty())
            }
            Content::Stack(rows) => rows.iter().any(row_ends_in_content),
        }
}

impl Fill {
    /// A box's interior: its `fill` decides the glyph, never the layout —
    /// the groups still anchor to the box's sides (SPEC § 4.3).
    const fn from_box(cfg: &BoxCfg) -> Self {
        if cfg.fill { Self::Rule } else { Self::Spaces }
    }

    /// Whether empty cells are drawn at all.
    const fn draws(self) -> bool {
        !matches!(self, Self::Packed)
    }
}

/// Heights: a bare row is one line, a boxed one its lines plus two, and a
/// row is as tall as its tallest column (SPEC § 4.3). Each is measured at
/// the width it is laid out to: a box too narrow to draw there renders
/// nothing, so it adds no lines.
impl Layout<'_> {
    fn row_height(&self, row: &Row<'_>, width: usize, fill: Fill) -> usize {
        let (widths, _) = self.shares(row, width, fill);
        row.cols.iter().zip(&widths).map(|(c, w)| self.col_height(c, *w, fill)).max().unwrap_or(1)
    }

    /// A boxed column is its content plus the two edge lines, so a boxed
    /// one-line column is a three-line box.
    fn col_height(&self, col: &Col<'_>, width: usize, fill: Fill) -> usize {
        let Some(boxed) = col.boxed else {
            return self.content_height(col, width, fill);
        };
        let cfg = self.box_cfg(boxed);
        self.box_interior(&self.box_chars(&cfg), width).map_or_else(
            || self.content_height(col, width, fill),
            |(inner, _)| self.content_height(col, inner, Fill::from_box(&cfg)).saturating_add(2),
        )
    }

    /// A column's own lines: one for its groups, its blocks' for a stack.
    fn content_height(&self, col: &Col<'_>, width: usize, fill: Fill) -> usize {
        match &col.content {
            Content::Groups { .. } => 1,
            Content::Stack(rows) => {
                blocks(rows).iter().map(|b| self.block_height(b, width, fill)).sum::<usize>().max(1)
            }
        }
    }

    /// One block of a stack: its rows, plus the two edge lines when they are
    /// inside a box that draws. A top-level row's box is drawn by
    /// `boxed_block`, which adds the two lines there instead.
    fn block_height(&self, block: &Block<'_, '_>, width: usize, fill: Fill) -> usize {
        let rows = |width: usize, fill: Fill| -> usize {
            block.rows.iter().map(|(_, r)| self.row_height(r, width, fill)).sum()
        };
        let Some(boxed) = block.boxed else {
            return rows(width, fill);
        };
        let cfg = self.box_cfg(boxed);
        self.box_interior(&self.box_chars(&cfg), width).map_or_else(
            || rows(width, fill),
            |(interior, _)| rows(interior, Fill::from_box(&cfg)).saturating_add(2),
        )
    }

    /// The pad inside a box: the frame's, or one cell when the frame has
    /// none, so a box never has its content against its side (and a `none`
    /// box indents by it).
    fn box_pad(&self) -> usize {
        display_width(&self.chars.pad).max(1)
    }
}

/// The columns of one row, laid out and joined.
impl Layout<'_> {
    /// One row's lines as drafts, each `width` cells wide.
    ///
    /// `inherit` is how the column holding this row is laid out, for an
    /// inner row of a stack: an `auto` column's rows are exact, and the
    /// column's own pads belong to the ends of the row that reach its edges
    /// (SPEC § 4.3).
    fn row_body(
        &self,
        row: &Row<'_>,
        width: usize,
        height: usize,
        fill: Fill,
        inherit: Fit,
    ) -> Vec<Vec<Draft>> {
        let widths = self.share(row, width, fill);
        let mut lines = self.row_columns(row, &widths, height, fill, inherit);
        // The cells no column took: with no `fr` column to share them, the
        // free width is filler after the last column.
        let left = width.saturating_sub(drawn_cells(&widths, row.gap));
        if left > 0 && fill.draws() {
            for line in &mut lines {
                line.push(Self::filler(left, fill));
            }
        }
        lines
    }

    /// One row's columns laid out to `widths` and joined by their gaps, a
    /// list of drafts per line, without the cells no column took: a row in
    /// a column or a box has them filled by [`Self::row_body`], and a row of
    /// the frame by [`Self::wrap_frame`], which puts them behind the pad
    /// before its cap.
    fn row_columns(
        &self,
        row: &Row<'_>,
        widths: &[usize],
        height: usize,
        fill: Fill,
        inherit: Fit,
    ) -> Vec<Vec<Draft>> {
        let gap = row.gap;
        // Inside a box the gap is spaces, and they alone keep two columns
        // apart; only at `gap = 0` does a side facing a neighbour need a
        // fill cell and a pad of its own (SPEC § 4.3).
        let spaced = fill == Fill::Spaces && gap > 0;
        let last = row.cols.len().saturating_sub(1);
        let cols: Vec<Vec<Vec<Draft>>> = row
            .cols
            .iter()
            .zip(widths)
            .enumerate()
            .map(|(j, (col, w))| {
                let (l, r) = self.edge_pads(row, j, fill);
                let (outer_l, outer_r) = inherit.pads;
                let fit = Fit {
                    exact: inherit.exact || col.width == Width::Auto,
                    pads: (
                        if j == 0 { l.max(outer_l) } else { l },
                        if j == last { r.max(outer_r) } else { r },
                    ),
                    trailing: inherit.trailing && j == last,
                    sides: (
                        if j == 0 { inherit.sides.0 } else { spaced },
                        if j == last { inherit.sides.1 } else { spaced },
                    ),
                };
                self.col_lines(col, *w, height, row, fill, fit)
            })
            .collect();
        (0..height)
            .map(|i| {
                let mut line: Vec<Draft> = Vec::new();
                let mut drawn = false;
                for (col, w) in cols.iter().zip(widths) {
                    // A column clamped to nothing renders nothing, and so
                    // does its gap (SPEC § 4.3).
                    if *w == 0 {
                        continue;
                    }
                    if drawn {
                        // On a one-line row the rule runs through the gaps
                        // too, so a centred module floats on one rule; on a
                        // taller row a gap beside a box's side is spaces.
                        line.push(if height == 1 && fill == Fill::Rule {
                            Draft::Rule(gap)
                        } else {
                            Draft::Space(gap, Elem::Gap)
                        });
                    }
                    line.extend(col.get(i).cloned().unwrap_or_default());
                    drawn = true;
                }
                line
            })
            .collect()
    }

    /// The cells each column takes: fixed and `auto` columns first, then the
    /// `fr` columns share what is left, then the row is clamped left to
    /// right so a narrow terminal drops whole columns rather than spilling
    /// (SPEC § 4.3).
    fn share(&self, row: &Row<'_>, width: usize, fill: Fill) -> Vec<usize> {
        let (widths, dropped) = self.shares(row, width, fill);
        // A dropped column is invisible on screen, so the one place it can
        // be explained is the debug log (SPEC § 4.3).
        if dropped > 0 {
            crate::debug::log(&format!(
                "layout: {dropped} of {} columns dropped, {width} cells is too narrow for them",
                row.cols.len()
            ));
        }
        widths
    }

    /// [`Self::share`] without the log, with the number of columns that
    /// wanted cells and got none, which the log reports: a row's height is
    /// measured on its shares before it is laid out, and the log would say
    /// it all twice.
    fn shares(&self, row: &Row<'_>, width: usize, fill: Fill) -> (Vec<usize>, usize) {
        let want: Vec<usize> = row
            .cols
            .iter()
            .enumerate()
            .map(|(j, col)| match col.width {
                Width::Cells(c) => c,
                Width::Auto => match self.auto_width(col, row) {
                    0 => 0,
                    // A box's side stands between its text and the rule.
                    content if !meets_rule(col) => content,
                    // Beside a rule, an `auto` column keeps the pad every
                    // other group has, so the rule never runs into its text.
                    content => {
                        let (l, r) = self.edge_pads(row, j, fill);
                        content.saturating_add(l).saturating_add(r)
                    }
                },
                Width::Fr(_) => 0,
            })
            .collect();
        // A gap sits between two columns that are drawn, and a column that
        // takes no cells (`width = 0`, an `auto` one with nothing to show,
        // an `fr` one whose share floors to nothing) draws nothing:
        // `row_columns` skips it and its gap, so reserving that gap would leave
        // its cells over. An `fr` column counts until its share is known;
        // the last one that comes to nothing is dropped and the row shared
        // again, so the gap cells it freed go to the `fr` columns that
        // remain. Each round drops one, so this ends within the row's
        // columns.
        let mut live: Vec<bool> = row
            .cols
            .iter()
            .zip(&want)
            .map(|(col, w)| matches!(col.width, Width::Fr(_)) || *w > 0)
            .collect();
        let mut desired = loop {
            let desired = Self::fr_shares(row, &want, &live, width);
            let emptied = row.cols.iter().zip(&desired).zip(live.iter_mut()).rev().find(
                |((col, take), drawn)| **drawn && **take == 0 && matches!(col.width, Width::Fr(_)),
            );
            match emptied {
                Some((_, drawn)) => *drawn = false,
                None => break desired,
            }
        };
        // Left to right, gap then column: a column whose gap plus one cell
        // does not fit renders nothing, and so does everything to its right.
        // A column that takes no cells takes no gap either, as `row_columns`
        // draws a gap only between two columns it draws; an `fr` one that
        // came to nothing was dropped for want of room.
        let mut left = width;
        let mut dropped = 0_usize;
        let mut after_one = false;
        for (col, take) in row.cols.iter().zip(desired.iter_mut()) {
            if *take == 0 {
                if matches!(col.width, Width::Fr(_)) {
                    dropped = dropped.saturating_add(1);
                }
                continue;
            }
            let cost = if after_one { row.gap } else { 0 };
            if left < cost.saturating_add(1) {
                *take = 0;
                dropped = dropped.saturating_add(1);
                continue;
            }
            *take = (*take).min(left.saturating_sub(cost));
            left = left.saturating_sub(cost).saturating_sub(*take);
            after_one = true;
        }
        (desired, dropped)
    }

    /// Every column's cells before the clamp: its `want`, or for a `live`
    /// `fr` column its part of what the fixed columns and the gaps between
    /// the live columns leave (a dropped `fr` column takes nothing).
    fn fr_shares(row: &Row<'_>, want: &[usize], live: &[bool], width: usize) -> Vec<usize> {
        let drawn = live.iter().filter(|l| **l).count();
        let gaps = row.gap.saturating_mul(drawn.saturating_sub(1));
        let available = width.saturating_sub(gaps);
        let fixed: usize = want.iter().sum();
        let free = available.saturating_sub(fixed);
        let weight = |col: &Col<'_>, drawn: bool| match col.width {
            Width::Fr(n) if drawn => usize::try_from(n).ok(),
            _ => None,
        };
        let total_fr: usize = row.cols.iter().zip(live).filter_map(|(c, l)| weight(c, *l)).sum();
        // floor(free × n ÷ Σfr) each, and the cells that floor left over go
        // one each to the first of them, so the shares differ by at most one
        // and always add up. The leftover is what the shares did not take,
        // never `free % Σfr`: with a weight above 1 the two differ, and
        // handing out the larger one overshoots the row.
        let fr = total_fr.max(1);
        let mut desired: Vec<usize> = row
            .cols
            .iter()
            .zip(want)
            .zip(live)
            .map(|((col, w), l)| match (col.width, weight(col, *l)) {
                (_, Some(n)) => free.saturating_mul(n).checked_div(fr).unwrap_or(0),
                (Width::Fr(_), None) => 0,
                _ => *w,
            })
            .collect();
        let shared: usize = row
            .cols
            .iter()
            .zip(live)
            .zip(&desired)
            .filter(|((col, l), _)| weight(col, **l).is_some())
            .map(|(_, w)| *w)
            .sum();
        let mut leftover = free.saturating_sub(shared);
        for ((col, l), take) in row.cols.iter().zip(live).zip(desired.iter_mut()) {
            if leftover == 0 {
                break;
            }
            if weight(col, *l).is_some() {
                *take = take.saturating_add(1);
                leftover = leftover.saturating_sub(1);
            }
        }
        desired
    }

    /// The pad a column keeps on each side, so a rule never runs into its
    /// text where its content reaches the edge: none at the row's own edges,
    /// where the frame's cap or the box's side has already padded it, and
    /// none where nothing is drawn (`fill = false`, or inside a box).
    fn edge_pads(&self, row: &Row<'_>, j: usize, fill: Fill) -> (usize, usize) {
        if fill != Fill::Rule {
            return (0, 0);
        }
        let pad = display_width(&self.chars.pad);
        let last = j.saturating_add(1) >= row.cols.len();
        // With no `fr` column the free width is a rule after the last one,
        // so even the last column has a rule to its right (SPEC § 4.3).
        (if j > 0 { pad } else { 0 }, if last && has_fr(row) { 0 } else { pad })
    }

    /// An `auto` column is exactly its content: its groups joined by the
    /// separator, or its widest inner row, with the sides and pads of any
    /// box drawn around it.
    ///
    /// The width is re-measured every tick, so a column whose content
    /// changes width moves its neighbours: `auto` is for values that hold
    /// still (SPEC § 4.3).
    fn auto_width(&self, col: &Col<'_>, row: &Row<'_>) -> usize {
        let content = match &col.content {
            Content::Groups { left, right, .. } => {
                let sep = display_width(row.separator);
                let join = |g: &[Vec<Segment>]| {
                    let text: usize = g.iter().map(|m| segments_width(m)).sum();
                    text.saturating_add(sep.saturating_mul(g.len().saturating_sub(1)))
                };
                let (l, r) = (join(left), join(right));
                // An `auto` flex column joins its two groups with the
                // separator and draws no rule between them (SPEC § 4.3).
                let joined = if l > 0 && r > 0 { sep } else { 0 };
                l.saturating_add(joined).saturating_add(r)
            }
            Content::Stack(rows) => blocks(rows)
                .iter()
                .map(|block| {
                    let widest = block
                        .rows
                        .iter()
                        .flat_map(|(_, inner)| inner.cols.iter().map(|c| self.auto_width(c, inner)))
                        .max()
                        .unwrap_or(0);
                    self.boxed_width(block.boxed, widest)
                })
                .max()
                .unwrap_or(0),
        };
        self.boxed_width(col.boxed, content)
    }

    /// `content` cells with the box around them, when there is one: its two
    /// sides and a pad inside each. Nothing is drawn around no content.
    fn boxed_width(&self, boxed: Option<&BoxRef>, content: usize) -> usize {
        match boxed {
            Some(b) if content > 0 => {
                let side = display_width(&self.box_chars(&self.box_cfg(b)).side);
                content.saturating_add(side.saturating_add(self.box_pad()).saturating_mul(2))
            }
            _ => content,
        }
    }

    /// One column's lines, its own box drawn around them when it has one.
    fn col_lines(
        &self,
        col: &Col<'_>,
        width: usize,
        height: usize,
        row: &Row<'_>,
        fill: Fill,
        fit: Fit,
    ) -> Vec<Vec<Draft>> {
        let Some(boxed) = col.boxed else {
            return Self::placed(col, self.col_content(col, width, row, fill, fit), width, height);
        };
        let cfg = self.box_cfg(boxed);
        let chars = self.box_chars(&cfg);
        // Too narrow to draw at all: the column keeps its share and renders
        // nothing, and `col_height` gave it no edge lines (SPEC § 4.3). Its
        // cells are empty cells like any other: rule on a one-line row under
        // a rule, spaces on a taller one.
        let empty = || {
            let empty =
                if height > 1 { Draft::Space(width, Elem::Pad) } else { Self::filler(width, fill) };
            (0..height.max(1)).map(|_| vec![empty.clone()]).collect()
        };
        let Some((inner, pad)) = self.box_interior(&chars, width) else {
            return empty();
        };
        // Inside its own box a column is padded by the box, not by the row.
        let content = self.col_content(col, inner, row, Fill::from_box(&cfg), fit.in_box());
        // A row is measured at the width it is laid out to (`frame_plan`
        // for a row of the frame), so its height holds every line of a box
        // that fits there. Were it ever short, the box would be empty cells
        // too, never a top edge alone or a box cut through its content.
        let interior = height.saturating_sub(2);
        if content.len().max(1) > interior {
            return empty();
        }
        let body = Self::placed(col, content, inner, interior);
        self.box_lines(&cfg, &chars, width, pad, cfg.title.as_ref(), body)
    }

    /// The cells inside a box of `width`, and the pad each side of them.
    ///
    /// A box is its two sides, a pad each side of the content and the
    /// content: `None` when even the sides do not fit, and no pads when
    /// they would leave nothing to draw in. The corners are drawn whatever
    /// the side is, so a `custom` box with corners and no side needs their
    /// cells too, or its top and bottom lines would overflow.
    fn box_interior(&self, chars: &BoxChars, width: usize) -> Option<(usize, usize)> {
        let sides = display_width(&chars.side).saturating_mul(2);
        let pair = |a: &str, b: &str| display_width(a).saturating_add(display_width(b));
        let corners = pair(&chars.top_left, &chars.top_right)
            .max(pair(&chars.bottom_left, &chars.bottom_right));
        if width < corners {
            return None;
        }
        let left = width.checked_sub(sides)?;
        let pad = self.box_pad();
        if left > pad.saturating_mul(2) {
            Some((left.saturating_sub(pad.saturating_mul(2)), pad))
        } else {
            Some((left, 0))
        }
    }

    /// A column's content lines: its groups' one line, or its stack's.
    fn col_content(
        &self,
        col: &Col<'_>,
        width: usize,
        row: &Row<'_>,
        fill: Fill,
        fit: Fit,
    ) -> Vec<Vec<Draft>> {
        match &col.content {
            Content::Groups { left, right, left_ids, right_ids } => {
                let group = Group {
                    left,
                    right,
                    left_ids,
                    right_ids,
                    justify: col.justify,
                    separator: row.separator,
                };
                vec![self.compose_group(&group, width, fill, fit)]
            }
            Content::Stack(rows) => blocks(rows)
                .iter()
                .flat_map(|block| self.stack_block(block, width, fill, fit))
                .collect(),
        }
    }

    /// A column's `content` lines, `width` cells each, padded to `height`
    /// by `valign`.
    fn placed(
        col: &Col<'_>,
        content: Vec<Vec<Draft>>,
        width: usize,
        height: usize,
    ) -> Vec<Vec<Draft>> {
        // A padding line is the column's cells as spaces, whatever the fill
        // mode: they place the columns to its right, and a packed row drops
        // them again at the end of the line, where nothing follows.
        let blank =
            |n: usize| (0..n).map(|_| vec![Draft::Space(width, Elem::Pad)]).collect::<Vec<_>>();
        let missing = height.saturating_sub(content.len());
        let (above, below) = match col.valign {
            VAlign::Top => (0, missing),
            VAlign::Bottom => (missing, 0),
            VAlign::Center => {
                let a = missing.checked_div(2).unwrap_or(0);
                (a, missing.saturating_sub(a))
            }
        };
        let mut out = blank(above);
        out.extend(content.into_iter().take(height.max(1)));
        out.extend(blank(below));
        out
    }

    /// One block of a stack: the rows of one box drawn inside it, or a bare
    /// row's own line. Adjacent inner rows naming the same box are one box
    /// here exactly as they are at the top level (SPEC § 4.3).
    fn stack_block(
        &self,
        block: &Block<'_, '_>,
        width: usize,
        fill: Fill,
        fit: Fit,
    ) -> Vec<Vec<Draft>> {
        let Some(boxed) = block.boxed else {
            return block
                .rows
                .iter()
                .flat_map(|(_, inner)| self.stack_row(inner, width, fill, fit))
                .collect();
        };
        let cfg = self.box_cfg(boxed);
        let chars = self.box_chars(&cfg);
        // Too narrow to draw: its rows' lines are empty, as an empty row's
        // are, and the box adds none of its own (`block_height`).
        let Some((interior, pad)) = self.box_interior(&chars, width) else {
            return (0..self.block_height(block, width, fill))
                .map(|_| vec![Self::filler(width, fill)])
                .collect();
        };
        let body = self.block_body(block, interior, &cfg, fit);
        self.box_lines(&cfg, &chars, width, pad, box_title(&cfg, block), body)
    }

    /// One bare row of a stack: its line with its title set into it.
    fn stack_row(&self, inner: &Row<'_>, width: usize, fill: Fill, fit: Fit) -> Vec<Vec<Draft>> {
        let mut lines = self.row_body(inner, width, self.row_height(inner, width, fill), fill, fit);
        // An inner row's title goes into its own first line.
        if let (Some(title), Some(first)) = (inner.title, lines.first_mut()) {
            self.place_title(first, title, fill);
        }
        if inner.blank {
            mark_blank(&mut lines);
        }
        lines
    }

    /// The rows of a block laid out to a box's interior, their lines in
    /// order, each `blank` row's marked (SPEC § 4.1).
    fn block_body(
        &self,
        block: &Block<'_, '_>,
        interior: usize,
        cfg: &BoxCfg,
        fit: Fit,
    ) -> Vec<Vec<Draft>> {
        let fill = Fill::from_box(cfg);
        block
            .rows
            .iter()
            .flat_map(|(_, row)| {
                let height = self.row_height(row, interior, fill);
                let mut lines = self.row_body(row, interior, height, fill, fit.in_box());
                if row.blank {
                    mark_blank(&mut lines);
                }
                lines
            })
            .collect()
    }
}

/// Mark lines of a `blank = true` row: each keeps the braille cell once it
/// is finished, if it is whitespace only then ([`Draft::Blank`]).
fn mark_blank(lines: &mut [Vec<Draft>]) {
    for line in lines {
        line.insert(0, Draft::Blank);
    }
}

/// A box's title: its own, or its first row's, since `box = true` on a row
/// is the one way to title a one-row box.
fn box_title<'t>(cfg: &'t BoxCfg, block: &Block<'t, '_>) -> Option<&'t crate::config::TitleCfg> {
    cfg.title.as_ref().or_else(|| block.rows.first().and_then(|(_, r)| r.title))
}

/// Composing one column's line from its groups.
impl Layout<'_> {
    /// `n` cells of whatever fills empty space in this mode.
    const fn filler(n: usize, fill: Fill) -> Draft {
        match fill {
            Fill::Rule => Draft::Rule(n),
            Fill::Spaces | Fill::Packed => Draft::Space(n, Elem::Rule),
        }
    }

    /// `n` cells of the frame's `pad`: its own text, unstyled, wherever the
    /// frame pads (after a cap, around a group, beside a column's text,
    /// inside a box's sides). Spaces when `n` is not its width: a box pads
    /// by one cell under a frame whose pad is empty, and a column narrower
    /// than its pads keeps only what fits.
    fn pad(&self, n: usize) -> Draft {
        let pad = &self.chars.pad;
        let text = if display_width(pad) == n { pad.clone() } else { " ".repeat(n) };
        Draft::Done(Piece { elem: Elem::Pad, segs: vec![Segment::plain(text)] })
    }

    /// One cell of filler, in cells: the rule glyph may be wider than one.
    fn fill_cell(&self, fill: Fill) -> usize {
        match fill {
            Fill::Rule => display_width(&self.chars.fill).max(1),
            Fill::Spaces => 1,
            Fill::Packed => 0,
        }
    }

    /// The modules of a group as pieces, with the separator between them.
    ///
    /// A module that rendered nothing is not a column of its own (SPEC § 4),
    /// so it takes no separator with it: the render has already left it out.
    fn group_pieces(&self, group: &[Vec<Segment>], ids: &[String], separator: &str) -> Vec<Piece> {
        let mut pieces: Vec<Piece> = Vec::with_capacity(group.len().saturating_mul(2));
        for (i, module) in group.iter().enumerate() {
            if !separator.is_empty()
                && let Some(before) = i.checked_sub(1).and_then(|j| group.get(j))
            {
                pieces.push(self.separator_piece(separator, before));
            }
            pieces.push(Piece {
                elem: Elem::Module(ids.get(i).cloned().unwrap_or_default()),
                segs: module.clone(),
            });
        }
        pieces
    }

    /// A separator after the module `before` (SPEC § 4.1 `separator_color`):
    /// in the fixed colour, or with `inherit` in the colour of the first
    /// coloured, undimmed segment of that module (an icon or a value, never
    /// a label or an align pad), muted when it has none.
    fn separator_piece(&self, separator: &str, before: &[Segment]) -> Piece {
        let color = match self.separator_color {
            SeparatorColor::Fixed { color, .. } => *color,
            SeparatorColor::Inherit => before
                .iter()
                .find(|s| {
                    s.style.fg != Color::Default && !s.style.dim && !s.text().trim().is_empty()
                })
                .map_or_else(|| self.theme.role(Role::Muted), |s| s.style.fg),
        };
        Piece { elem: Elem::Separator, segs: vec![Segment::styled(separator, Style::fg(color))] }
    }

    /// Cut or scroll a group that does not fit its budget (SPEC § 4.1): the
    /// window advances with the tick's clock, so a cancelled tick loses
    /// nothing; with animations off there is no ticker and the group is cut.
    ///
    /// A group that fits is handed back untouched: measuring it costs a sum,
    /// and the copy is made only when something is actually cut.
    fn fit_group(&self, pieces: Vec<Piece>, budget: usize, cut: bool, scrolls: bool) -> Vec<Piece> {
        let width: usize = pieces.iter().map(|p| segments_width(&p.segs)).sum();
        if !cut || width <= budget {
            return pieces;
        }
        // Where each module sits in the uncut run, so the placement map can
        // say which cells of the window it still owns (SPEC § 14), counted
        // the way the cut and the scroller count.
        let mut owned: Vec<(String, Range<usize>)> = Vec::new();
        let mut at = 0_usize;
        for piece in &pieces {
            let end = at.saturating_add(cluster_width(&piece.segs));
            if let Elem::Module(id) = &piece.elem {
                owned.push((id.clone(), at..end));
            }
            at = end;
        }
        let segs: Vec<Segment> = pieces.iter().flat_map(|p| p.segs.iter().cloned()).collect();
        let (segs, map) = self.ticker.as_ref().filter(|_| scrolls).map_or_else(
            || {
                let cut = truncate(&segs, budget, self.ellipsis);
                // The map follows the cut as made: a two-cell cluster at the
                // cut leaves it a cell short of `budget`.
                let end = segments_width(&cut);
                let mark = display_width(crate::ansi::fit(self.ellipsis, budget));
                let map = cut_map(&owned, end.saturating_sub(mark), end);
                (cut, map)
            },
            |ticker| {
                let period = scroll_period(&segs, &ticker.gap, true);
                let offset = crate::time::frame(ticker.now, ticker.step, period);
                (
                    scroll(&segs, budget, offset, &ticker.gap, true),
                    scrolled_map(&owned, offset, period, budget),
                )
            },
        );
        vec![Piece { elem: Elem::Group(map), segs }]
    }

    /// One column's line: the flex form when the column has a `right` group,
    /// a lone group placed by `justify` otherwise (SPEC § 4.3).
    ///
    /// `fit.exact` is a column laid out to its own content (`width = "auto"`):
    /// it draws no rule, so its two groups join with the separator and
    /// nothing is cut to leave room for cells that are never drawn.
    fn compose_group(&self, group: &Group<'_>, width: usize, fill: Fill, fit: Fit) -> Vec<Draft> {
        // A column's own cells, once the pads that keep a neighbouring rule
        // off its text are taken off the ends its content reaches. A group
        // the rule already surrounds needs none.
        let (touch_left, touch_right) = group.touches(fit.exact);
        let (before, after) = fit.pads;
        // A column narrower than its two pads keeps what fits of them.
        let before = if touch_left { before.min(width) } else { 0 };
        let after = if touch_right { after.min(width.saturating_sub(before)) } else { 0 };
        let inner = width.saturating_sub(before).saturating_sub(after);
        let mut drafts: Vec<Draft> = Vec::new();
        if before > 0 {
            drafts.push(self.pad(before));
        }
        drafts.extend(self.compose_cells(group, inner, fill, fit));
        if after > 0 {
            drafts.push(self.pad(after));
        }
        drafts
    }

    /// [`Self::compose_group`] inside the column's own cells.
    fn compose_cells(&self, group: &Group<'_>, width: usize, fill: Fill, fit: Fit) -> Vec<Draft> {
        let Group { left, right, left_ids, right_ids, justify, separator } = *group;
        let pad_w = display_width(&self.chars.pad);
        let cell = self.fill_cell(fill);
        // `truncate = false` lets the last column run past the box; every
        // other column is cut to its share, or it would spill into a
        // neighbour and move the whole row (SPEC § 4.3).
        let cut = self.truncate || !fit.trailing;
        let exact = fit.exact;
        let packed = fill == Fill::Packed || exact;
        // The right group is never *scrolled* (SPEC § 4.1), but it is cut
        // to its column like anything else: a group wider than the column
        // used to push the columns beside it off their shares. Beside a
        // rule it keeps the pad before it, so it gets the cells after that.
        let right_room = if packed { width } else { width.saturating_sub(pad_w) };
        let right_pieces =
            self.fit_group(self.group_pieces(right, right_ids, separator), right_room, cut, false);
        let right_w: usize = right_pieces.iter().map(|p| segments_width(&p.segs)).sum();

        if packed {
            // Left-packed: the right group follows the left one after a
            // separator, and nothing fills the rest (SPEC § 4.1).
            let sep_w = if right_w == 0 { 0 } else { display_width(separator) };
            let budget = width.saturating_sub(right_w).saturating_sub(sep_w);
            // An `auto` column is its content, so it is only ever short of
            // room when the row clamped it; scrolling there would move a
            // column that is meant to hold still (SPEC § 4.3).
            let pieces = self.group_pieces(left, left_ids, separator);
            let mut pieces = self.fit_group(pieces, budget, cut, !exact);
            let left_w: usize = pieces.iter().map(|p| segments_width(&p.segs)).sum();
            if right_w > 0 {
                // A left group cut to nothing is not there to be joined.
                if left_w > 0 && !separator.is_empty() {
                    let before = left.last().map_or(&[][..], Vec::as_slice);
                    pieces.push(self.separator_piece(separator, before));
                }
                pieces.extend(right_pieces);
            }
            let used: usize = pieces.iter().map(|p| segments_width(&p.segs)).sum();
            let mut drafts: Vec<Draft> = Vec::new();
            // A lone group still sits where `justify` says; the cells that
            // place it are spaces, since nothing fills a packed row.
            let (before, after) = split(width.saturating_sub(used), justify);
            if before > 0 {
                drafts.push(Draft::Space(before, Elem::Pad));
            }
            drafts.extend(pieces.into_iter().map(Draft::Done));
            if after > 0 {
                drafts.push(Draft::Space(after, Elem::Pad));
            }
            return drafts;
        }

        if right_w > 0 {
            // The flex form: left anchored left, right anchored right, the
            // rule between them, the left group cut first.
            let right_block = right_w.saturating_add(pad_w);
            let left_pieces = self.group_pieces(left, left_ids, separator);
            let has_left = !left_pieces.is_empty();
            let join = cell.saturating_add(if has_left { pad_w } else { 0 });
            let budget = width.saturating_sub(right_block).saturating_sub(join);
            let left_pieces = self.fit_group(left_pieces, budget, cut, true);
            let left_w: usize = left_pieces.iter().map(|p| segments_width(&p.segs)).sum();
            let left_pad = if left_w == 0 { 0 } else { pad_w };
            let rule =
                width.saturating_sub(left_w).saturating_sub(left_pad).saturating_sub(right_block);
            let mut drafts: Vec<Draft> = left_pieces.into_iter().map(Draft::Done).collect();
            if left_pad > 0 {
                drafts.push(self.pad(left_pad));
            }
            drafts.push(Self::filler(rule, fill));
            drafts.push(self.pad(pad_w));
            drafts.extend(right_pieces.into_iter().map(Draft::Done));
            return drafts;
        }

        // A lone group: the rule on one side, or both when it is centred.
        // A side facing the rule or a neighbour keeps a cell of fill and a
        // pad; inside a box, a side that already stands clear needs
        // neither (`Fit::sides`).
        let (fills_left, fills_right) = match justify {
            Justify::Left => (false, true),
            Justify::Right => (true, false),
            Justify::Center => (true, true),
        };
        let keeps = |fills: bool, box_side: bool| fills && !(fill == Fill::Spaces && box_side);
        let (keep_left, keep_right) =
            (keeps(fills_left, fit.sides.0), keeps(fills_right, fit.sides.1));
        let reserve = |keep: bool| if keep { cell.saturating_add(pad_w) } else { 0 };
        let budget = width.saturating_sub(reserve(keep_left)).saturating_sub(reserve(keep_right));
        let pieces = self.group_pieces(left, left_ids, separator);
        let pieces = self.fit_group(pieces, budget, cut, true);
        let text_w: usize = pieces.iter().map(|p| segments_width(&p.segs)).sum();
        let pad = |keep: bool| if keep && text_w > 0 { pad_w } else { 0 };
        let (pad_left, pad_right) = (pad(keep_left), pad(keep_right));
        let space = width.saturating_sub(text_w).saturating_sub(pad_left).saturating_sub(pad_right);
        let (before, after) = split(space, justify);
        let mut drafts: Vec<Draft> = Vec::new();
        if before > 0 || justify != Justify::Left {
            drafts.push(Self::filler(before, fill));
        }
        if pad_left > 0 {
            drafts.push(self.pad(pad_left));
        }
        drafts.extend(pieces.into_iter().map(Draft::Done));
        if pad_right > 0 {
            drafts.push(self.pad(pad_right));
        }
        drafts.push(Self::filler(after, fill));
        drafts
    }
}

/// One column's groups and how they sit, as [`Layout::compose_group`] reads
/// them.
#[derive(Debug, Clone, Copy)]
struct Group<'a> {
    left: &'a [Vec<Segment>],
    right: &'a [Vec<Segment>],
    left_ids: &'a [String],
    right_ids: &'a [String],
    justify: Justify,
    separator: &'a str,
}

/// The cells each module keeps once a run is cut to `budget` cells with
/// `kept` of text before the ellipsis (SPEC § 14): the module the cut lands
/// in owns the ellipsis cells too, and a module wholly past the cut owns
/// nothing.
fn cut_map(
    owned: &[(String, Range<usize>)],
    kept: usize,
    budget: usize,
) -> Vec<(String, Range<usize>)> {
    let mut out: Vec<(String, Range<usize>)> = Vec::new();
    let mut ellipsis_owned = kept >= budget;
    for (id, r) in owned {
        let start = r.start.min(kept);
        let mut end = r.end.min(kept);
        if !ellipsis_owned && r.end > kept {
            end = budget;
            ellipsis_owned = true;
        }
        if end > start {
            out.push((id.clone(), start..end));
        }
    }
    out
}

/// The cells each module keeps in a `budget`-cell window `offset` cells into
/// a run that repeats every `period` cells (SPEC § 14): a module straddling
/// the wrap owns a run at each end of the window.
fn scrolled_map(
    owned: &[(String, Range<usize>)],
    offset: usize,
    period: usize,
    budget: usize,
) -> Vec<(String, Range<usize>)> {
    let window_end = offset.saturating_add(budget);
    let mut out: Vec<(String, Range<usize>)> = Vec::new();
    // Two repetitions cover the window: the offset is below one period and
    // the window is narrower than one.
    for base in [0_usize, period] {
        for (id, r) in owned {
            let start = r.start.saturating_add(base).max(offset);
            let end = r.end.saturating_add(base).min(window_end);
            if end > start {
                out.push((id.clone(), start.saturating_sub(offset)..end.saturating_sub(offset)));
            }
        }
    }
    out.sort_by_key(|(_, r)| r.start);
    out
}

impl Group<'_> {
    /// Which edges of its column this group's content reaches, and so where
    /// a pad is needed to keep a neighbouring rule off it.
    const fn touches(&self, exact: bool) -> (bool, bool) {
        let (has_left, has_right) = (!self.left.is_empty(), !self.right.is_empty());
        if exact {
            // An `auto` column is exactly its content: both edges.
            return (has_left || has_right, has_left || has_right);
        }
        if has_right {
            // The flex form: the left group is anchored left, the right one
            // right, and the rule sits between them.
            return (has_left, true);
        }
        match self.justify {
            Justify::Left => (has_left, false),
            Justify::Right => (false, has_left),
            Justify::Center => (false, false),
        }
    }
}

/// How a column is laid out to its width (SPEC § 4.3).
#[derive(Debug, Clone, Copy)]
struct Fit {
    /// The column is exactly its content (`width = "auto"`, or an inner row
    /// of one): it draws no rule, so nothing is cut to make room for cells
    /// that are never drawn.
    exact: bool,
    /// Cells of pad to keep on each side, so a rule beside the column never
    /// runs into its text ([`Layout::edge_pads`]): drawn only on a side the
    /// column's content reaches ([`Group::touches`]), since the rule already
    /// stands clear of a group it surrounds.
    pads: (usize, usize),
    /// Nothing of the row is to this column's right, so `truncate = false`
    /// lets its content run past the box. Every other column is cut to its
    /// share whatever `truncate` says, or it would spill into a neighbour
    /// (SPEC § 4.3).
    trailing: bool,
    /// Whether each end of the column already stands clear of what is
    /// beside it inside a box: the box's side, whose pad keeps the text off
    /// it, or a gap of one space or more. A lone group there needs no fill
    /// cell and no pad of its own.
    sides: (bool, bool),
}

impl Default for Fit {
    fn default() -> Self {
        Self { exact: false, pads: (0, 0), trailing: true, sides: (false, false) }
    }
}

impl Fit {
    /// The fit of what is drawn inside a box: its sides stand at both ends,
    /// so no rule pad is kept there.
    const fn in_box(self) -> Self {
        Self { pads: (0, 0), sides: (true, true), ..self }
    }
}

/// The glyphs a box is drawn with (SPEC § 4.3).
#[derive(Debug, Clone)]
struct BoxChars {
    top_left: String,
    top_right: String,
    bottom_left: String,
    bottom_right: String,
    side: String,
    fill: String,
}

/// Boxes: which glyphs, and the three lines they are drawn as.
impl Layout<'_> {
    fn box_cfg(&self, boxed: &BoxRef) -> BoxCfg {
        match boxed {
            BoxRef::Named(name) => self.boxes.get(name).cloned().unwrap_or_default(),
            BoxRef::Anon => BoxCfg::default(),
        }
    }

    /// A box inherits the frame's style, and its glyphs with it when the
    /// frame is `custom`. A frame style with no box shape (`none`,
    /// `powerline`) leaves an unstyled box `rounded`; a box that asks for
    /// `none` itself is invisible, which is how a dashboard indents a
    /// column without drawing anything (SPEC § 4.3).
    fn box_chars(&self, cfg: &BoxCfg) -> BoxChars {
        let inherited = cfg.style.is_none();
        let mut style = cfg.style.unwrap_or(self.style);
        if (inherited && style == FrameStyle::None) || style == FrameStyle::Powerline {
            style = FrameStyle::Rounded;
        }
        let base = if inherited && style == self.style {
            self.chars.clone()
        } else {
            FrameChars::for_style(style)
        };
        BoxChars {
            top_left: base.top_left,
            top_right: base.top_right,
            bottom_left: base.bottom_left,
            bottom_right: base.bottom_right,
            side: base.side,
            fill: if base.fill.is_empty() { " ".to_owned() } else { base.fill },
        }
    }

    /// A box's top or bottom line: corner, rule, corner, with the title set
    /// into the top one.
    fn edge_drafts(
        &self,
        chars: &BoxChars,
        width: usize,
        top: bool,
        title: Option<&crate::config::TitleCfg>,
        cfg: &BoxCfg,
    ) -> Vec<Draft> {
        let style = Style::fg(cfg.color.unwrap_or_else(|| self.theme.role(Role::Frame)));
        let (open, close) = if top {
            (&chars.top_left, &chars.top_right)
        } else {
            (&chars.bottom_left, &chars.bottom_right)
        };
        let ends = display_width(open).saturating_add(display_width(close));
        let middle = width.saturating_sub(ends);
        let mut drafts: Vec<Draft> = Vec::new();
        if !open.is_empty() {
            drafts.push(Draft::Done(Piece {
                elem: Elem::BoxEdge,
                segs: vec![Segment::styled(open, style)],
            }));
        }
        drafts.extend(self.edge_middle(chars, middle, title.filter(|_| top), style));
        if !close.is_empty() {
            drafts.push(Draft::Done(Piece {
                elem: Elem::BoxEdge,
                segs: vec![Segment::styled(close, style)],
            }));
        }
        drafts
    }

    /// The rule between a box's corners, with the title set into it.
    ///
    /// A box's own rule never animates: the pattern belongs to the frame,
    /// and a travelling top edge would read as an error.
    fn edge_middle(
        &self,
        chars: &BoxChars,
        middle: usize,
        title: Option<&crate::config::TitleCfg>,
        style: Style,
    ) -> Vec<Draft> {
        let rule = |n: usize| {
            let fill_w = display_width(&chars.fill).max(1);
            let text = if chars.fill.trim().is_empty() {
                " ".repeat(n)
            } else {
                chars.fill.repeat(n.checked_div(fill_w).unwrap_or(0))
            };
            Draft::Done(Piece { elem: Elem::Rule, segs: vec![Segment::styled(text, style)] })
        };
        let fitted = title.filter(|t| !t.text.is_empty()).and_then(|t| self.fit_title(t, middle));
        let (Some(title), Some((text, pad, spare))) = (title, fitted) else {
            return vec![rule(middle)];
        };
        // One rule cell between the corner and the title, so it reads as a
        // label on the box's edge rather than a word stuck to its corner:
        // `╭─ Repository ───╮`, the shape of the frame's own two-cell caps.
        let inset = spare.min(1);
        let (before, after) = match title.justify {
            Justify::Left => (inset, spare.saturating_sub(inset)),
            Justify::Right => (spare.saturating_sub(inset), inset),
            Justify::Center => split(spare, Justify::Center),
        };
        let mut out = vec![rule(before)];
        if pad > 0 {
            out.push(Draft::title_pad(pad));
        }
        out.push(Draft::Done(Piece { elem: Elem::Title, segs: text }));
        if pad > 0 {
            out.push(Draft::title_pad(pad));
        }
        out.push(rule(after));
        out
    }

    /// One interior line of a box: the sides, and a pad inside each.
    ///
    /// A side is one of the box's glyphs, so it takes the box's `color`
    /// (SPEC § 4.3) — the same colour its corners and rules are drawn in.
    fn side_drafts(
        &self,
        drafts: Vec<Draft>,
        chars: &BoxChars,
        cfg: &BoxCfg,
        pad: usize,
    ) -> Vec<Draft> {
        let style = Style::fg(cfg.color.unwrap_or_else(|| self.theme.role(Role::Frame)));
        let mut out: Vec<Draft> = Vec::new();
        if !chars.side.is_empty() {
            out.push(Draft::Done(Piece {
                elem: Elem::BoxEdge,
                segs: vec![Segment::styled(&chars.side, style)],
            }));
        }
        if pad > 0 {
            out.push(self.pad(pad));
        }
        out.extend(drafts);
        if pad > 0 {
            out.push(self.pad(pad));
        }
        if !chars.side.is_empty() {
            out.push(Draft::Done(Piece {
                elem: Elem::BoxEdge,
                segs: vec![Segment::styled(&chars.side, style)],
            }));
        }
        out
    }

    /// A box `width` cells wide around `body`, whose lines are laid out to
    /// its interior: the top edge with `title`, each body line between the
    /// sides with `pad` inside them, the bottom edge. Every box is drawn
    /// here, a row's, a column's and a stack's alike.
    fn box_lines(
        &self,
        cfg: &BoxCfg,
        chars: &BoxChars,
        width: usize,
        pad: usize,
        title: Option<&crate::config::TitleCfg>,
        body: Vec<Vec<Draft>>,
    ) -> Vec<Vec<Draft>> {
        let mut lines = Vec::with_capacity(body.len().saturating_add(2));
        lines.push(self.edge_drafts(chars, width, true, title, cfg));
        lines.extend(body.into_iter().map(|drafts| self.side_drafts(drafts, chars, cfg, pad)));
        lines.push(self.edge_drafts(chars, width, false, None, cfg));
        lines
    }
}

/// Titles: plain text set into the rule of a row or a box (SPEC § 4.3).
impl Layout<'_> {
    /// `title` fitted to `cells`: its text in its colour, cut to what is
    /// left once `title_pad` (at most half the cells) is taken on each side,
    /// that pad, and the cells to spare. `None` when the pads leave no room.
    fn fit_title(
        &self,
        title: &crate::config::TitleCfg,
        cells: usize,
    ) -> Option<(Vec<Segment>, usize, usize)> {
        let pad = title.pad.min(cells.checked_div(2).unwrap_or(0));
        let room = cells.saturating_sub(pad.saturating_mul(2));
        if room == 0 {
            return None;
        }
        let style = Style::fg(title.color.unwrap_or_else(|| self.theme.role(Role::Frame)));
        let text = truncate(&[Segment::styled(&title.text, style)], room, self.ellipsis);
        let spare = room.saturating_sub(segments_width(&text));
        Some((text, pad, spare))
    }

    /// Put `title` into the widest run of rule cells the justification
    /// allows, cutting it rather than widening the line.
    fn place_title(&self, drafts: &mut Vec<Draft>, title: &crate::config::TitleCfg, fill: Fill) {
        if title.text.is_empty() {
            return;
        }
        // The runs of empty cells this line offers, in line order.
        let runs: Vec<(usize, usize)> = drafts
            .iter()
            .enumerate()
            .filter(|(_, d)| matches!(d, Draft::Rule(_) | Draft::Space(_, _)))
            .map(|(i, d)| (i, d.cells()))
            .collect();
        // A left title wants the run right after the cap and a right title
        // the run before it, but a run too narrow to hold the title would
        // cut it away: either falls back to the widest run, which is where
        // a centred title always goes.
        let widest = || runs.iter().copied().max_by_key(|(_, n)| *n);
        let need = display_width(&title.text).saturating_add(title.pad.saturating_mul(2));
        let target = match title.justify {
            Justify::Left => runs.iter().copied().find(|(_, n)| *n >= need).or_else(widest),
            Justify::Right => runs.iter().rev().copied().find(|(_, n)| *n >= need).or_else(widest),
            Justify::Center => widest(),
        };
        let Some((at, (text, pad, spare))) = target
            .filter(|(_, n)| *n > 0)
            .and_then(|(at, cells)| self.fit_title(title, cells).map(|fitted| (at, fitted)))
        else {
            return;
        };
        let (before, after) = split(spare, title.justify);
        // Right after the frame's cap there is already a pad, and doubling
        // it reads as a typo: `├─ Repository ──┤`, not `├─  Repository`.
        let capped = drafts.get(..at).is_some_and(|before| {
            before.iter().all(|d| match d {
                Draft::Done(p) => matches!(p.elem, Elem::Cap | Elem::Pad | Elem::BoxEdge),
                Draft::Blank => true,
                Draft::Rule(_) | Draft::Space(_, _) => false,
            })
        });
        let pad_before = if capped && before == 0 { 0 } else { pad };
        // The cell that pad would have taken goes back to the rule, so the
        // title never changes the line's width.
        let after = after.saturating_add(pad.saturating_sub(pad_before));
        // The cells around the title stay what the run was: a gap or a
        // padding line is spaces on a multi-line row (SPEC § 4.3).
        let around = |n: usize| match drafts.get(at) {
            Some(Draft::Space(_, elem)) => Draft::Space(n, elem.clone()),
            _ => Self::filler(n, fill),
        };
        let mut replacement: Vec<Draft> = Vec::new();
        if before > 0 {
            replacement.push(around(before));
        }
        if pad_before > 0 {
            replacement.push(Draft::title_pad(pad_before));
        }
        replacement.push(Draft::Done(Piece { elem: Elem::Title, segs: text }));
        if pad > 0 {
            replacement.push(Draft::title_pad(pad));
        }
        if after > 0 {
            replacement.push(around(after));
        }
        // A title never widens the line: it only ever replaces rule cells
        // with the same number of cells. There is no assertion here on
        // purpose — a render never panics, whatever a config asks for
        // (SPEC § 5), and `paint` cuts a line that is over the width.
        drafts.splice(at..at.saturating_add(1), replacement);
    }
}

/// Turning drafts into a finished line.
impl Layout<'_> {
    /// Paint the line's rule cells and assemble its pieces.
    ///
    /// The pattern's phase runs over the line's rule cells together, so the
    /// dots travel across a column boundary instead of restarting at each,
    /// and a line whose rule is shorter than one period keeps the static
    /// fill (SPEC § 4.2, § 4.3).
    fn paint(&self, drafts: Vec<Draft>, blank: bool) -> Line {
        let total: usize = drafts
            .iter()
            .map(|d| match d {
                Draft::Rule(n) => *n,
                Draft::Done(_) | Draft::Space(_, _) | Draft::Blank => 0,
            })
            .sum();
        let blank = blank || drafts.iter().any(|d| matches!(d, Draft::Blank));
        let style = Style::fg(self.theme.role(Role::Frame));
        let pattern = self.rule.as_ref().filter(|r| !r.cells.is_empty() && total >= r.cells.len());
        let fill_w = display_width(&self.chars.fill).max(1);
        let mut at = 0_usize;
        let mut pieces: Vec<Piece> = Vec::new();
        for draft in drafts {
            match draft {
                // An empty piece is dropped, but a segment that carries a
                // link is never empty in the way that matters: the OSC 8 is
                // the point of it (SPEC § 3.7).
                Draft::Done(piece) => {
                    let empty = piece.segs.iter().all(|s| s.text().is_empty() && s.link.is_none());
                    if !empty {
                        pieces.push(piece);
                    }
                }
                Draft::Space(n, elem) if n > 0 => {
                    pieces.push(Piece { elem, segs: vec![Segment::plain(" ".repeat(n))] });
                }
                Draft::Space(_, _) | Draft::Rule(0) | Draft::Blank => {}
                Draft::Rule(n) => {
                    let text = pattern.map_or_else(
                        || self.chars.fill.repeat(n.checked_div(fill_w).unwrap_or(0)),
                        |rule| rule.paint_at(at, n),
                    );
                    at = at.saturating_add(n);
                    if !text.is_empty() {
                        pieces.push(Piece {
                            elem: Elem::Rule,
                            segs: vec![Segment::styled(text, style)],
                        });
                    }
                }
            }
        }
        let line = Line { pieces };
        let line = if self.truncate && line.width() > self.width {
            line.recut(self.width, self.ellipsis)
        } else {
            line
        };
        if blank { blank_line(line) } else { line }
    }

    /// One row line between the frame's caps: line `index` of the `count`
    /// that carry them. `ends` is whether the row's content reaches the cap
    /// ([`Self::ends_in_content`]).
    fn wrap_frame(
        &self,
        drafts: Vec<Draft>,
        (index, count): (usize, usize),
        row: &Row<'_>,
        fill: Fill,
        title: Option<&crate::config::TitleCfg>,
        ends: bool,
    ) -> Line {
        let (prefix, cap) = self.chars.ends(index, count);
        let style = Style::fg(self.theme.role(Role::Frame));
        let pad = display_width(&self.chars.pad);
        let mut line: Vec<Draft> = Vec::new();
        if !prefix.is_empty() {
            line.push(Draft::Done(Piece {
                elem: Elem::Cap,
                segs: vec![Segment::styled(prefix, style)],
            }));
            if pad > 0 {
                line.push(self.pad(pad));
            }
        }
        line.extend(drafts);
        if fill.draws() {
            // The cap is padded from the content only: with no right group
            // the rule runs into it, as it always has. The pad stands
            // against the content, and every cell left over goes to the rule
            // after it: the free width no `fr` column took, and on a later
            // line of a tall row what a cap narrower than the first's (or
            // none) leaves. So the rule never touches text, and every line
            // is the same width. A line with no cap takes the pad only when
            // it has the cells for it: nothing reserved them.
            let used: usize = line.iter().map(Draft::cells).sum();
            let room = self.width.saturating_sub(used);
            let padded = ends && pad > 0 && (!cap.is_empty() || room >= pad);
            if padded {
                line.push(self.pad(pad));
            }
            let taken = display_width(cap).saturating_add(if padded { pad } else { 0 });
            let spare = room.saturating_sub(taken);
            if spare > 0 {
                line.push(Self::filler(spare, fill));
            }
            if !cap.is_empty() {
                line.push(Draft::Done(Piece {
                    elem: Elem::Cap,
                    segs: vec![Segment::styled(cap, style)],
                }));
            }
        }
        // The title goes in last, so the rule after the columns is a run it
        // can take; a run is only ever rule or space cells, never a cap.
        if let Some(title) = title {
            self.place_title(&mut line, title, fill);
        }
        if !fill.draws() {
            // A packed row draws no filler at all, so the cells that only
            // place the content go once nothing follows them.
            while line.last().is_some_and(|d| matches!(d, Draft::Space(_, _))) {
                line.pop();
            }
        }
        self.paint(line, row.blank)
    }
}

/// Spare cells either side of something placed by a justification.
const fn split(spare: usize, justify: Justify) -> (usize, usize) {
    match justify {
        Justify::Left => (0, spare),
        Justify::Right => (spare, 0),
        Justify::Center => {
            let before = spare.div_euclid(2);
            (before, spare.saturating_sub(before))
        }
    }
}

/// A `blank = true` line (SPEC § 4.1), finished: [`keep_blank`] as one
/// piece when it needs the cell, the line as it is otherwise.
fn blank_line(line: Line) -> Line {
    keep_blank(&line.segments())
        .map_or(line, |segs| Line { pieces: vec![Piece { elem: Elem::Group(Vec::new()), segs }] })
}

/// The segments of a whitespace-only line with its first one-cell
/// whitespace character turned into [`BLANK_CELL`], so the harness keeps
/// the line; an empty line becomes that one cell. `None` when the line
/// already shows something (a frame, a box's side, a module) or has no
/// one-cell whitespace to spare: the width never changes.
fn keep_blank(row: &[Segment]) -> Option<Vec<Segment>> {
    // JavaScript's `trim` strips the Unicode White_Space set (and U+FEFF,
    // which `plain_text` has already dropped): the same set as
    // `char::is_whitespace`, so this is the harness's own test.
    if row.iter().any(|s| s.text().chars().any(|c| !c.is_whitespace())) {
        return None;
    }
    let one_cell = |c: char| display_width(c.encode_utf8(&mut [0; 4])) == 1;
    let empty = row.iter().all(|s| s.text().is_empty());
    let mut row = row.to_vec();
    match row.iter().position(|s| s.text().chars().any(one_cell)).and_then(|i| row.get_mut(i)) {
        Some(seg) => {
            let mut done = false;
            let text: String = seg
                .text()
                .chars()
                .map(|c| {
                    if !done && one_cell(c) {
                        done = true;
                        BLANK_CELL
                    } else {
                        c
                    }
                })
                .collect();
            *seg = seg.clone().with_text(text);
        }
        None if empty => row.push(Segment::plain(BLANK_CELL)),
        None => return None,
    }
    Some(row)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::ansi::Painter;

    fn boxes() -> BTreeMap<String, BoxCfg> {
        BTreeMap::new()
    }

    /// The layout of a one-column row, which is the flex line garnish has
    /// always drawn: these are the frame tests, kept as they were.
    struct Fixture {
        chars: FrameChars,
        theme: Theme,
        boxes: BTreeMap<String, BoxCfg>,
        style: FrameStyle,
        fill: bool,
        width: usize,
        truncate: bool,
        ticker: Option<Ticker>,
        rule: Option<Rule>,
        separator_color: SeparatorColor,
    }

    impl Fixture {
        fn new(style: FrameStyle, fill: bool, width: usize) -> Self {
            let theme = Theme::default();
            let separator_color =
                SeparatorColor::Fixed { spec: "muted".into(), color: theme.role(Role::Muted) };
            Self {
                chars: FrameChars::for_style(style),
                theme,
                boxes: boxes(),
                style,
                fill,
                width,
                truncate: true,
                ticker: None,
                rule: None,
                separator_color,
            }
        }

        fn layout(&self) -> Layout<'_> {
            Layout {
                chars: &self.chars,
                style: self.style,
                theme: &self.theme,
                separator_color: &self.separator_color,
                fill: self.fill,
                truncate: self.truncate,
                ellipsis: "…",
                ticker: self.ticker.clone(),
                rule: self.rule.clone(),
                width: self.width,
                boxes: &self.boxes,
            }
        }

        /// One row at line `index` of `count`, as the frame's caps see it.
        fn compose(
            &self,
            index: usize,
            count: usize,
            left: &[Segment],
            right: &[Segment],
            separator: &str,
        ) -> String {
            let groups = |segs: &[Segment]| {
                if segs.is_empty() { Vec::new() } else { vec![segs.to_vec()] }
            };
            let (left, right) = (groups(left), groups(right));
            let row = Row {
                cols: vec![Col {
                    width: Width::Fr(1),
                    justify: Justify::Left,
                    valign: VAlign::Top,
                    boxed: None,
                    content: Content::Groups {
                        left: &left,
                        right: &right,
                        left_ids: &[],
                        right_ids: &[],
                    },
                }],
                gap: 1,
                separator,
                title: None,
                boxed: None,
                blank: false,
            };
            let l = self.layout();
            let fill = if self.fill { Fill::Rule } else { Fill::Packed };
            let inner = l.inner_width(&row, index, count);
            let widths = l.share(&row, inner, fill);
            let mut body = l.row_columns(&row, &widths, 1, fill, Fit::default());
            let drafts = body.pop().unwrap_or_default();
            let ends = l.ends_in_content(&row, &widths);
            let line = l.wrap_frame(drafts, (index, count), &row, fill, None, ends);
            Painter::PLAIN.paint(&line.segments())
        }
    }

    #[test]
    fn rounded_frame_fills_to_width_with_right_group() {
        let f = Fixture::new(FrameStyle::Rounded, true, 30);
        let left = [Segment::plain("left")];
        let right = [Segment::plain("R")];
        let sep = f.chars.separator.clone();
        let s = f.compose(0, 2, &left, &right, &sep);
        assert_eq!(s, format!("╭─ left {} R ─╮", "─".repeat(17)));
        assert_eq!(display_width(&s), 30);
        let last = f.compose(1, 2, &left, &[], &sep);
        assert_eq!(last, format!("╰─ left {}╯", "─".repeat(21)));
        assert_eq!(display_width(&last), 30);
        let single = f.compose(0, 1, &left, &right, &sep);
        assert!(single.starts_with("── left") && single.ends_with("R ──"));
        assert_eq!(display_width(&single), 30);
    }

    /// SPEC § 4.1 `separator_color = "inherit"`: each separator takes the
    /// first coloured, undimmed segment of the module before it, skipping a
    /// muted label and an align pad, and is muted when there is none; the
    /// packed join before the right group follows the last left module.
    #[test]
    fn inherited_separators_follow_the_module_before_them() {
        let separators = |f: &Fixture| -> Vec<Color> {
            let (red, green, blue) =
                (Color::Rgb(255, 0, 0), Color::Rgb(0, 255, 0), Color::Rgb(0, 0, 255));
            // Module 1: an icon and a value in two colours, so the first
            // coloured segment is the one taken, not the last. Module 4: a
            // coloured but blank segment (a `prefix = " "` in a role) before
            // the value, which is skipped like the pad.
            let left = vec![
                vec![
                    Segment::styled("a", Style::fg(red)),
                    Segment::styled(" a2", Style::fg(Color::Rgb(5, 5, 5))),
                ],
                vec![
                    Segment::styled("lbl ", Style::fg(Color::Rgb(9, 9, 9)).dimmed()),
                    Segment::plain("  "),
                    Segment::styled("b", Style::fg(green)),
                ],
                vec![Segment::plain("plain only")],
                vec![
                    Segment::styled("  ", Style::fg(Color::Rgb(7, 7, 7))),
                    Segment::styled("d", Style::fg(Color::Rgb(9, 0, 9))),
                ],
            ];
            let right = vec![vec![Segment::styled("r", Style::fg(blue))]];
            let row = Row {
                cols: vec![Col {
                    width: Width::Fr(1),
                    justify: Justify::Left,
                    valign: VAlign::Top,
                    boxed: None,
                    content: Content::Groups {
                        left: &left,
                        right: &right,
                        left_ids: &[],
                        right_ids: &[],
                    },
                }],
                gap: 1,
                separator: " | ",
                title: None,
                boxed: None,
                blank: false,
            };
            let l = f.layout();
            let inner = l.inner_width(&row, 0, 1);
            let mut body = l.row_body(&row, inner, 1, Fill::Packed, Fit::default());
            let drafts = body.pop().unwrap_or_default();
            let line = l.wrap_frame(drafts, (0, 1), &row, Fill::Packed, None, false);
            line.pieces
                .iter()
                .filter(|p| matches!(p.elem, Elem::Separator))
                .map(|p| p.segs[0].style.fg)
                .collect()
        };
        let mut f = Fixture::new(FrameStyle::Rounded, false, 80);
        let muted = f.theme.role(Role::Muted);
        assert_eq!(separators(&f), [muted; 4], "the fixed default is muted");
        f.separator_color = SeparatorColor::Inherit;
        assert_eq!(
            separators(&f),
            [Color::Rgb(255, 0, 0), Color::Rgb(0, 255, 0), muted, Color::Rgb(9, 0, 9)]
        );
        f.separator_color = SeparatorColor::Fixed { spec: "x".into(), color: Color::Rgb(1, 2, 3) };
        assert_eq!(separators(&f), [Color::Rgb(1, 2, 3); 4]);
    }

    #[test]
    fn overflow_truncates_left_never_right() {
        let f = Fixture::new(FrameStyle::Rounded, true, 20);
        let left = [Segment::plain("a very long left group")];
        let right = [Segment::plain("RIGHT")];
        let s = f.compose(0, 1, &left, &right, " │ ");
        assert_eq!(display_width(&s), 20);
        assert!(s.ends_with("RIGHT ──"), "{s}");
        assert!(s.contains('…'));
    }

    #[test]
    fn no_fill_uses_the_rows_separator_and_prefix_only() {
        let f = Fixture::new(FrameStyle::Square, false, 80);
        let (left, right) = ([Segment::plain("L")], [Segment::plain("R")]);
        let s = f.compose(1, 3, &left, &right, &f.chars.separator.clone());
        assert_eq!(s, "├─ L │ R");
        // A per-row separator joins the groups (walkthrough bug 6: the frame
        // default used to be taken regardless).
        let s = f.compose(1, 3, &left, &right, " · ");
        assert_eq!(s, "├─ L · R");
        let none = Fixture::new(FrameStyle::None, false, 80);
        let sep = none.chars.separator.clone();
        assert_eq!(none.compose(0, 1, &left, &right, &sep), "L  R");
        assert_eq!(none.compose(0, 1, &left, &[], &sep), "L");
    }

    /// SPEC § 4.1 Ticker: an over-budget left group scrolls one step per
    /// tick and wraps around after the gap; the right group and the frame
    /// are untouched, the row keeps its width, a layout without a ticker
    /// (animations off) cuts the row like `truncate`, and `truncate = false`
    /// hands the row over whole.
    #[test]
    fn ticker_scrolls_the_left_group_and_leaves_the_right_alone() {
        let at = |secs: i64| jiff::Timestamp::from_second(secs).unwrap();
        let mut f = Fixture::new(FrameStyle::Rounded, true, 24);
        let left = [Segment::plain("abcdefghijklmnop")]; // 16 cells
        let right = [Segment::plain("R")];
        // Budget: 24 − "╭─ " (3) − " R ─╮" (5) − rule + pad (2) = 14 cells.
        let cut = f.compose(0, 1, &left, &right, " │ ");
        assert_eq!(cut, "── abcdefghijklm… ─ R ──");
        let ticker = |secs: i64| Ticker { step: 1.0, gap: " · ".into(), now: at(secs) };
        // period = 16 + 3 = 19; 1738425600 % 19 = 4 → the window starts at "e"
        // and, 14 cells later, shows the first two cells of the gap.
        f.ticker = Some(ticker(1_738_425_600));
        let s0 = f.compose(0, 1, &left, &right, " │ ");
        assert_eq!(s0, "── efghijklmnop · ─ R ──");
        assert_eq!(display_width(&s0), 24);
        f.ticker = Some(ticker(1_738_425_601));
        let s1 = f.compose(0, 1, &left, &right, " │ ");
        assert_eq!(s1, "── fghijklmnop ·  ─ R ──", "one cell further: the whole gap shows");
        f.ticker = Some(ticker(1_738_425_611));
        let wrapped = f.compose(0, 1, &left, &right, " │ ");
        assert_eq!(wrapped, "── p · abcdefghij ─ R ──", "offset 15: end, gap, start");
        // Animations off: the layout carries no ticker and the row is the
        // `…` cut, so the cut is visible to the readers the switch is for.
        f.ticker = None;
        assert_eq!(f.compose(0, 1, &left, &right, " │ "), cut);
        // A group that fits is never scrolled.
        let short = [Segment::plain("abc")];
        f.ticker = Some(ticker(1_738_425_601));
        assert_eq!(
            f.compose(0, 1, &short, &right, " │ "),
            format!("── abc {} R ──", "─".repeat(12))
        );
        // truncate = false: the whole row, ticker or not.
        f.truncate = false;
        let whole = f.compose(0, 1, &left, &right, " │ ");
        assert!(whole.contains("abcdefghijklmnop"), "{whole}");
    }

    /// SPEC § 4.2 Animated rule: the pattern fills the rule cell by cell from
    /// `offset`, the rule keeps its width, and the caps and groups are as
    /// with a plain fill.
    #[test]
    fn patterned_rule_keeps_its_width_and_shifts_with_the_offset() {
        let mut f = Fixture::new(FrameStyle::Rounded, true, 20);
        let (left, right) = ([Segment::plain("L")], [Segment::plain("R")]);
        let plain = f.compose(0, 1, &left, &right, " │ ");
        // "── L " (5) + rule (10) + " R ──" (5)
        assert_eq!(plain, format!("── L {} R ──", "─".repeat(10)));
        let cells =
            |offset| Some(Rule { cells: vec!["·".into(), " ".into(), " ".into()], offset });
        f.rule = cells(0);
        let s0 = f.compose(0, 1, &left, &right, " │ ");
        assert_eq!(s0, "── L ·  ·  ·  · R ──");
        assert_eq!(display_width(&s0), 20);
        let expected = |offset: usize| {
            let pattern = ["·", " ", " "];
            let rule: String = (0..10).map(|i| pattern[(i + offset) % 3]).collect();
            format!("── L {rule} R ──")
        };
        for offset in 1..=4 {
            f.rule = cells(offset);
            let s = f.compose(0, 1, &left, &right, " │ ");
            assert_eq!(s, expected(offset), "offset {offset}");
            assert_eq!(display_width(&s), 20);
        }
        // A rule shorter than the pattern's period keeps the static fill: the
        // collapsed one-cell join after an overflow must not blink.
        let mut narrow = Fixture::new(FrameStyle::Rounded, true, 12);
        narrow.rule = cells(1);
        let wide_left = [Segment::plain("abcdefgh")];
        let s = narrow.compose(0, 1, &wide_left, &right, " │ ");
        assert_eq!(s, "── a… ─ R ──", "collapsed rule: static fill, not a blinking dot");
        let short = [Segment::plain("ab")];
        let s = narrow.compose(0, 1, &short, &right, " │ ");
        assert_eq!(s, "── ab ─ R ──", "one rule cell, period three: static fill");
        assert_eq!(
            Rule { cells: vec!["ab".into()], offset: 5 }.paint_at(0, 3),
            "ababab",
            "offset wraps"
        );
        assert_eq!(
            Rule { cells: Vec::new(), offset: 0 }.paint_at(0, 3),
            "",
            "no pattern, no rule text"
        );
        // An empty pattern falls back to the fill character.
        f.rule = Some(Rule { cells: Vec::new(), offset: 0 });
        assert_eq!(f.compose(0, 1, &left, &right, " │ "), plain);
    }

    #[test]
    fn powerline_caps_are_padded() {
        let f = Fixture::new(FrameStyle::Powerline, true, 20);
        let s = f.compose(0, 1, &[Segment::plain("L")], &[Segment::plain("R")], " ");
        assert_eq!(s, "\u{e0b6} L              R \u{e0b4}");
        assert_eq!(display_width(&s), 20);
    }

    #[test]
    fn none_style_with_fill_pads_to_width() {
        let f = Fixture::new(FrameStyle::None, true, 12);
        let s = f.compose(0, 1, &[Segment::plain("ab")], &[Segment::plain("cd")], "  ");
        assert_eq!(s, "ab        cd");
        assert_eq!(display_width(&s), 12);
    }

    /// A column of `modules` alone, for the layout tests below.
    ///
    /// The layout borrows the rendered groups from the render, which holds
    /// them for the whole tick; a test has no such owner, so the one module
    /// is leaked. It is a handful of segments per test.
    fn col(width: Width, text: &str) -> Col<'static> {
        let left: &'static [Vec<Segment>] = if text.is_empty() {
            &[]
        } else {
            Box::leak(Box::new(vec![vec![Segment::plain(text)]]))
        };
        Col {
            width,
            justify: Justify::Left,
            valign: VAlign::Top,
            boxed: None,
            content: Content::Groups { left, right: &[], left_ids: &[], right_ids: &[] },
        }
    }

    fn row(cols: Vec<Col<'static>>, gap: usize) -> Row<'static> {
        Row { cols, gap, separator: " │ ", title: None, boxed: None, blank: false }
    }

    /// A flex column: `left` anchored left, `right` anchored right, each a
    /// single module (none when empty), leaked as [`col`] does.
    fn flex(width: Width, left: &str, right: &str) -> Col<'static> {
        let group = |text: &str| -> &'static [Vec<Segment>] {
            if text.is_empty() {
                &[]
            } else {
                Box::leak(Box::new(vec![vec![Segment::plain(text)]]))
            }
        };
        Col {
            content: Content::Groups {
                left: group(left),
                right: group(right),
                left_ids: &[],
                right_ids: &[],
            },
            ..col(width, "")
        }
    }

    /// The text a line shows, for assertion messages.
    fn show(line: &Line) -> String {
        Painter::PLAIN.paint(&line.segments())
    }

    /// SPEC § 4.3: content never spills into a neighbour. A right group as
    /// wide as its column was cut to the whole column with its pad on top,
    /// and a left group cut to nothing still took its pad (and, packed, a
    /// separator), so the column came out over its share; `paint` then recut
    /// the whole line, which took its cap and its placement map with it.
    #[test]
    fn a_columns_drafts_fill_exactly_its_share() {
        let f = Fixture::new(FrameStyle::Rounded, true, 80);
        let l = f.layout();
        let text = |n: usize| -> Vec<Vec<Segment>> {
            if n == 0 { Vec::new() } else { vec![vec![Segment::plain("x".repeat(n))]] }
        };
        for fill in [Fill::Rule, Fill::Spaces, Fill::Packed] {
            for exact in [false, true] {
                for pads in [(0, 0), (0, 1), (1, 1)] {
                    for justify in [Justify::Left, Justify::Center, Justify::Right] {
                        for (left_w, right_w) in
                            (0..=12).flat_map(|l| (0..=12).map(move |r| (l, r)))
                        {
                            let (left, right) = (text(left_w), text(right_w));
                            let group = Group {
                                left: &left,
                                right: &right,
                                left_ids: &[],
                                right_ids: &[],
                                justify,
                                separator: " │ ",
                            };
                            for width in 0..=14_usize {
                                let fit = Fit { exact, pads, ..Fit::default() };
                                let drafts = l.compose_group(&group, width, fill, fit);
                                let cells: usize = drafts.iter().map(Draft::cells).sum();
                                assert_eq!(
                                    cells, width,
                                    "{fill:?} exact={exact} pads={pads:?} {justify:?} left={left_w} right={right_w} width={width}: {drafts:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        // The shape the review found: a 12-cell flex column whose right
        // group leaves no room for the left one, beside a second column.
        for width in [30_usize, 60, 96] {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let r = row(
                vec![flex(Width::Cells(12), "❖ Opus", "⠋ 16:00:00"), col(Width::Fr(1), "s")],
                1,
            );
            let lines = l.lines(std::slice::from_ref(&r));
            let line = lines.first().and_then(|r| r.first()).unwrap();
            assert_eq!(line.width(), width, "{}", show(line));
            assert!(show(line).ends_with("──"), "the cap survives: {}", show(line));
            assert!(show(line).contains("⠋ 16:00:00"), "{}", show(line));
            assert!(
                !line.pieces.iter().any(|p| p.elem == Elem::Group(Vec::new())),
                "the line was recut: {}",
                show(line)
            );
        }
    }

    /// SPEC § 4.3: with no `fr` column the free width is a rule after the
    /// last column, and that column keeps its own right pad, so the rule
    /// runs into the cap. The cap used to take a pad of its own as well,
    /// which left a hole in the rule: `⠋ 16:00:00 ──── ─┤`.
    #[test]
    fn a_row_with_no_fr_column_rules_into_its_cap() {
        for width in [60_usize, 120] {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let last = Col { justify: Justify::Right, ..col(Width::Auto, "⠋ 16:00:00") };
            let r = row(vec![col(Width::Auto, "a"), col(Width::Cells(20), "b"), last], 2);
            let lines = l.lines(std::slice::from_ref(&r));
            let line = lines.first().and_then(|r| r.first()).unwrap();
            assert_eq!(line.width(), width, "{}", show(line));
            let text = show(line);
            let tail = text.split_once("16:00:00 ").map_or("", |(_, t)| t);
            assert!(tail.len() > 3 && tail.chars().all(|c| c == '─'), "a hole: {text}");
        }
    }

    /// SPEC § 4.3 Pads: a rule never runs into a module's text, in a stack
    /// as out of one. An inner row's column took its pads from the inner
    /// row, where it is both first and last, so the outer column's pad was
    /// lost and a flex inner row's right group ran into the gap's rule.
    #[test]
    fn a_stacked_row_keeps_its_columns_pads() {
        for width in [40_usize, 80] {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let stacked = Col {
                content: Content::Stack(vec![Row {
                    cols: vec![flex(Width::Fr(1), "❖ Opus", "⚙ high")],
                    ..row(Vec::new(), 1)
                }]),
                ..col(Width::Fr(1), "")
            };
            let plain = flex(Width::Fr(1), "❖ Opus", "⚙ high");
            let line = |first: Col<'static>| {
                let r = row(vec![first, col(Width::Fr(1), "⠋ 16:00:00")], 1);
                l.lines(std::slice::from_ref(&r))
                    .into_iter()
                    .flatten()
                    .map(|l| show(&l))
                    .collect::<Vec<_>>()
            };
            assert_eq!(line(stacked), line(plain));
            // An `auto` stack gets a pad on each side of its text, where
            // `share` reserved them, never both on one side.
            let auto = Col {
                content: Content::Stack(vec![row(vec![col(Width::Fr(1), "mid")], 1)]),
                ..col(Width::Auto, "")
            };
            let r = row(vec![col(Width::Fr(1), "a"), auto, col(Width::Fr(1), "z")], 1);
            let lines = l.lines(std::slice::from_ref(&r));
            let text = lines.first().and_then(|r| r.first()).map_or_default(show);
            assert!(text.contains("─ mid ─"), "{text}");
        }
    }

    /// SPEC § 4.3: an `auto` column always fits its content, a box around
    /// it included. The width used to leave out the box's sides and pads,
    /// so a boxed `auto` column, or an `auto` stack of boxed rows, cut its
    /// content with `…` inside a box drawn two to four cells too narrow.
    #[test]
    fn an_auto_column_fits_its_content_inside_a_box() {
        let ten = "0123456789";
        let boxed_col = || Col { boxed: Some(&BoxRef::Anon), ..col(Width::Auto, ten) };
        let boxed_stack = || Col {
            content: Content::Stack(
                (0..2)
                    .map(|_| Row {
                        boxed: Some(&BoxRef::Anon),
                        ..row(vec![col(Width::Fr(1), ten)], 1)
                    })
                    .collect(),
            ),
            ..col(Width::Auto, "")
        };
        for width in [40_usize, 80] {
            for fill in [true, false] {
                let f = Fixture::new(FrameStyle::Rounded, fill, width);
                let l = f.layout();
                for stacked in [false, true] {
                    for at in 0..3_usize {
                        let mut cols: Vec<Col<'static>> =
                            (0..2).map(|_| col(Width::Fr(1), "x")).collect();
                        cols.insert(at, if stacked { boxed_stack() } else { boxed_col() });
                        let r = row(cols, 1);
                        let lines: Vec<Line> =
                            l.lines(std::slice::from_ref(&r)).into_iter().flatten().collect();
                        let text = lines.iter().map(show).collect::<Vec<_>>().join("\n");
                        assert!(!text.contains('…'), "cut at {at}, fill={fill}:\n{text}");
                        let top = lines.first().map_or_default(Line::spans);
                        let edges: Vec<Range<usize>> = top
                            .into_iter()
                            .filter(|(e, _)| *e == Elem::BoxEdge)
                            .map(|(_, r)| r)
                            .collect();
                        assert_eq!(edges.len(), 2, "{text}");
                        assert_eq!(edges[1].end - edges[0].start, 14, "at {at}:\n{text}");
                    }
                }
            }
        }
    }

    /// SPEC § 4.3: a box's corners are drawn whatever its side is, so a
    /// `custom` box with corners and no side needs their two cells even
    /// where its side would fit in none. The room used to count the side
    /// alone: a one-cell boxed column drew `++`, the line overflowed, and
    /// `paint` recut the whole of it to `…`, placement map and all.
    #[test]
    fn a_box_never_draws_corners_wider_than_its_room() {
        let plus = || "+".to_owned();
        for width in [12_usize, 40] {
            for fill in [true, false] {
                let mut f = Fixture::new(FrameStyle::Custom, fill, width);
                f.chars.top_left = plus();
                f.chars.top_right = plus();
                f.chars.bottom_left = plus();
                f.chars.bottom_right = plus();
                f.chars.side = String::new();
                let l = f.layout();
                for cells in 0..=3_usize {
                    let boxed = Col { boxed: Some(&BoxRef::Anon), ..col(Width::Cells(cells), "x") };
                    let stack = Col {
                        content: Content::Stack(vec![Row {
                            boxed: Some(&BoxRef::Anon),
                            ..row(vec![col(Width::Fr(1), "y")], 1)
                        }]),
                        ..col(Width::Cells(cells), "")
                    };
                    for last in [boxed, stack] {
                        let r = row(vec![col(Width::Fr(1), "❖ Opus"), last], 1);
                        for line in l.lines(std::slice::from_ref(&r)).into_iter().flatten() {
                            let fits =
                                if fill { line.width() == width } else { line.width() <= width };
                            assert!(fits, "{cells}: {} cells: {}", line.width(), show(&line));
                            assert!(
                                !line.pieces.iter().any(|p| p.elem == Elem::Group(Vec::new())),
                                "{cells}: the line was recut: {}",
                                show(&line)
                            );
                        }
                    }
                }
            }
        }
    }

    /// SPEC § 4.3: a column that draws nothing (an `auto` column with
    /// nothing to show, a `width = 0` one, an `fr` one whose share floors
    /// to nothing) takes no cells and no gap, and a last one leaves the
    /// column before it last. `share` used to reserve the gap, which
    /// `row_body` never drew, so the cells turned up as a stray rule after
    /// the last column and a right-justified module ended against it:
    /// `⠋ 16:00:00─ ─╮`. A last column that drew nothing still counted as
    /// ending in content, so the cap kept a pad beside the rule: `end ─── ──`.
    #[test]
    fn a_column_that_draws_nothing_takes_no_gap() {
        let right = |c: Col<'static>| Col { justify: Justify::Right, ..c };
        let nothing = [|| col(Width::Auto, ""), || col(Width::Cells(0), "⠋ 16:00:00")];
        for (width, gap, empty) in [30_usize, 60, 101]
            .into_iter()
            .flat_map(|w| [1, 3].map(|g| (w, g)))
            .flat_map(|(w, g)| nothing.map(|e| (w, g, e)))
        {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let last = || right(col(Width::Fr(1), "end"));
            for cols in [
                vec![empty(), col(Width::Fr(1), "a"), last()],
                vec![col(Width::Fr(1), "a"), empty(), last()],
                // Last, right-justified as a last column is by default: the
                // column before it keeps its own pad, so the cap takes none.
                vec![col(Width::Fr(1), "a"), last(), right(empty())],
            ] {
                let r = row(cols, gap);
                let widths = l.share(&r, l.inner_width(&r, 0, 1), Fill::Rule);
                let gaps = widths.iter().filter(|w| **w > 0).count().saturating_sub(1);
                assert_eq!(
                    widths.iter().sum::<usize>() + gaps * gap,
                    l.inner_width(&r, 0, 1),
                    "the drawn columns and their gaps fill the row: {widths:?}"
                );
                let lines = l.lines(std::slice::from_ref(&r));
                let line = lines.first().and_then(|r| r.first()).unwrap();
                assert_eq!(line.width(), width, "{}", show(line));
                assert!(show(line).ends_with(" end ──"), "{}", show(line));
            }
        }
        // `fr` columns squeezed to nothing: by fixed columns (the review's
        // `width = 20` beside two bare columns and an `auto` one), and by a
        // weight so large the other's share floors to nothing. The gap cells
        // no `fr` column is left to take are the rule after the last column,
        // behind its pad, as in a row with no `fr` column at all.
        let squeezed = |gap: usize| {
            [
                row(
                    vec![
                        col(Width::Cells(20), "a"),
                        col(Width::Fr(1), "b"),
                        col(Width::Fr(1), "c"),
                        right(col(Width::Auto, "end")),
                    ],
                    gap,
                ),
                row(
                    vec![
                        col(Width::Fr(64), "a"),
                        col(Width::Fr(1), "b"),
                        right(col(Width::Auto, "end")),
                    ],
                    gap,
                ),
            ]
        };
        let rules_into_cap = |text: &str| {
            text.rsplit_once(" end ").is_some_and(|(_, tail)| {
                tail.chars().count() >= 2 && tail.chars().all(|c| c == '─')
            })
        };
        for (width, gap) in (33_usize..=48).flat_map(|w| [1, 3].map(|g| (w, g))) {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            for r in squeezed(gap) {
                let lines = l.lines(std::slice::from_ref(&r));
                let line = lines.first().and_then(|r| r.first()).unwrap();
                assert_eq!(line.width(), width, "{}", show(line));
                assert!(rules_into_cap(&show(line)), "{width}, gap {gap}: {}", show(line));
            }
        }
        // A last `fr` column squeezed to nothing: the row ends in the rule,
        // which runs into the cap with no pad before it.
        for width in [30_usize, 40, 60] {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let r = row(vec![col(Width::Fr(64), "a"), right(col(Width::Fr(1), "end"))], 3);
            let lines = l.lines(std::slice::from_ref(&r));
            let line = lines.first().and_then(|r| r.first()).unwrap();
            let text = show(line);
            assert_eq!(line.width(), width, "{text}");
            assert!(!text.contains("end"), "the column is squeezed out: {text}");
            assert!(text.ends_with(&"─".repeat(6)), "a hole before the cap: {text}");
        }
    }

    /// SPEC § 4.3: the `fr` columns share the free width as
    /// `floor(free × n ÷ Σfr)` each, the leftover one cell each to the first
    /// of them, so the shares differ by at most one cell and always add up.
    /// A share that floors to nothing drops its column, gap and all, and the
    /// columns that remain share again: the gaps are counted between the
    /// columns that are drawn, never `n − 1` of them.
    #[test]
    fn column_shares_add_up_and_differ_by_at_most_one_cell() {
        let f = Fixture::new(FrameStyle::Rounded, true, 80);
        let l = f.layout();
        for n in 1..=6_usize {
            let r = row((0..n).map(|_| col(Width::Fr(1), "x")).collect(), 1);
            for width in 1..=400_usize {
                let widths = l.share(&r, width, Fill::Rule);
                let drawn: Vec<usize> = widths.iter().copied().filter(|w| *w > 0).collect();
                let gaps = drawn.len().saturating_sub(1);
                let total: usize = widths.iter().sum::<usize>() + gaps;
                assert!(total <= width, "{n} columns at {width}: {widths:?}");
                assert_eq!(total, width, "the shares fill the width: {widths:?}");
                let (min, max) = (
                    drawn.iter().min().copied().unwrap_or(0),
                    drawn.iter().max().copied().unwrap_or(0),
                );
                assert!(max - min <= 1, "{n} columns at {width}: {widths:?}");
                // The leftover goes to the *first* columns, so the shares
                // never rise from left to right.
                assert!(
                    widths.windows(2).all(|w| w.first() >= w.last()),
                    "{n} columns at {width}: {widths:?}"
                );
            }
        }
        // `fr` weights are shares, not equal parts.
        let r = row(vec![col(Width::Fr(3), "a"), col(Width::Fr(1), "b")], 0);
        assert_eq!(l.share(&r, 40, Fill::Rule), vec![30, 10]);
        // Weights above one: the cells `floor` leaves over go one each to
        // the first columns, so two equal columns differ by at most one
        // (`free % Σfr` would have handed out two cells each here).
        let r = row(vec![col(Width::Fr(2), "a"), col(Width::Fr(2), "b")], 0);
        assert_eq!(l.share(&r, 102, Fill::Rule), vec![51, 51]);
        assert_eq!(l.share(&r, 103, Fill::Rule), vec![52, 51]);
        let r =
            row(vec![col(Width::Fr(3), "a"), col(Width::Fr(3), "b"), col(Width::Fr(3), "c")], 0);
        assert_eq!(l.share(&r, 104, Fill::Rule), vec![35, 35, 34]);
    }

    /// SPEC § 4.3: when the width runs out the row is laid out left to
    /// right, gap then column; a column whose gap plus one cell does not fit
    /// renders nothing, and so does everything to its right.
    #[test]
    fn a_row_too_narrow_for_its_columns_drops_them_from_the_right() {
        let f = Fixture::new(FrameStyle::Rounded, true, 80);
        let l = f.layout();
        let r = row(vec![col(Width::Cells(10), "a"), col(Width::Cells(10), "b")], 2);
        assert_eq!(l.share(&r, 22, Fill::Rule), vec![10, 10]);
        assert_eq!(l.share(&r, 15, Fill::Rule), vec![10, 3], "the last column takes what is left");
        assert_eq!(l.share(&r, 11, Fill::Rule), vec![10, 0], "gap plus one cell does not fit");
        assert_eq!(l.share(&r, 4, Fill::Rule), vec![4, 0], "nor does the first column whole");
        assert_eq!(l.share(&r, 0, Fill::Rule), vec![0, 0]);
    }

    /// Every line of a row is exactly the width it was given, whatever the
    /// shape: boxes, stacks of unequal height, and a bare row alike.
    #[test]
    fn every_line_of_a_row_is_exactly_the_box_width() {
        let boxes: BTreeMap<String, BoxCfg> = BTreeMap::new();
        for width in [10_usize, 24, 40, 80, 120, 400] {
            let mut f = Fixture::new(FrameStyle::Rounded, true, width);
            f.boxes = boxes.clone();
            let l = f.layout();
            let stack = Col {
                width: Width::Fr(1),
                justify: Justify::Center,
                valign: VAlign::Center,
                boxed: Some(&BoxRef::Anon),
                content: Content::Stack(vec![
                    row(vec![col(Width::Fr(1), "one")], 1),
                    row(vec![col(Width::Fr(1), "two")], 1),
                ]),
            };
            let rows = vec![
                row(vec![col(Width::Fr(1), "left"), col(Width::Auto, "auto")], 2),
                Row { cols: vec![stack], ..row(Vec::new(), 1) },
                row(vec![col(Width::Fr(1), "plain")], 1),
            ];
            for (at, lines) in l.lines(&rows).into_iter().enumerate() {
                for line in lines {
                    assert_eq!(
                        line.width(),
                        width,
                        "row {at} at width {width}: {:?}",
                        crate::ansi::Painter::PLAIN.paint(&line.segments())
                    );
                }
            }
        }
    }

    /// A boxed column's edges land on the column's own cells. The line's
    /// width alone cannot see this: a box drawn one cell too narrow is
    /// absorbed by the row's fill and the line is still exactly the box
    /// width, so the placement map is what pins the interior.
    #[test]
    fn a_boxed_columns_edges_land_on_the_columns_own_cells() {
        for width in [40_usize, 60, 101] {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let boxed = |text| Col { boxed: Some(&BoxRef::Anon), ..col(Width::Fr(1), text) };
            let r = row(vec![boxed("one"), boxed("two")], 2);
            let inner = l.inner_width(&r, 0, l.row_height(&r, l.frame_room(&r), Fill::Rule));
            let lines: Vec<Line> =
                l.lines(std::slice::from_ref(&r)).into_iter().flatten().collect();
            let edges = |line: &Line| -> Vec<Range<usize>> {
                line.spans()
                    .into_iter()
                    .filter(|(elem, _)| *elem == Elem::BoxEdge)
                    .map(|(_, range)| range)
                    .collect()
            };
            let show = |line: &Line| Painter::PLAIN.paint(&line.segments());
            let top = lines.first().map_or_default(edges);
            let text = lines.iter().map(show).collect::<Vec<_>>().join("\n");
            assert_eq!(top.len(), 4, "two corners per box:\n{text}");
            // The sides of the interior lines stand in the corners' cells: a
            // box whose interior is one cell short still has its corners in
            // the right place, and the row's fill hides the difference.
            for line in &lines {
                assert_eq!(edges(line), top, "the edges move:\n{text}");
            }
            let (first, second) = (top[1].end - top[0].start, top[3].end - top[2].start);
            assert_eq!(top[2].start - top[1].end, 2, "the row's gap:\n{text}");
            assert!(first.abs_diff(second) <= 1, "{first} and {second}:\n{text}");
            assert_eq!(first + 2 + second, inner, "the boxes fill the row:\n{text}");
        }
    }

    /// SPEC § 4.3: `truncate = false` lets the last column's content run
    /// past the box; every other column is still cut to its share, or it
    /// would spill into its neighbour and move the whole row.
    #[test]
    fn truncate_false_lets_only_the_last_column_run_past_the_box() {
        let long = "a module far wider than any share of this row";
        for truncate in [true, false] {
            let mut f = Fixture::new(FrameStyle::Rounded, true, 40);
            f.truncate = truncate;
            let l = f.layout();
            let r = row(vec![col(Width::Fr(1), long), col(Width::Fr(1), long)], 1);
            let lines = l.lines(&[r]);
            let line = lines.into_iter().flatten().next().unwrap_or_default();
            let text = crate::ansi::Painter::PLAIN.paint(&line.segments());
            if truncate {
                assert_eq!(line.width(), 40, "{text}");
                assert_eq!(text.matches('…').count(), 2, "both columns are cut: {text}");
            } else {
                assert!(line.width() > 40, "the last column runs past the box: {text}");
                assert_eq!(text.matches('…').count(), 1, "only the first is cut: {text}");
                assert!(text.contains(long), "the last column is whole: {text}");
            }
        }
    }

    /// A row is as tall as its tallest column, a boxed column is its content
    /// plus two edge lines, and a short stack is padded by `valign`.
    #[test]
    fn heights_follow_the_tallest_column_and_valign_places_the_short_ones() {
        let f = Fixture::new(FrameStyle::Rounded, true, 40);
        let l = f.layout();
        let stack = |n: usize, valign: VAlign, boxed| Col {
            width: Width::Fr(1),
            justify: Justify::Left,
            valign,
            boxed,
            content: Content::Stack(
                (0..n).map(|i| row(vec![col(Width::Fr(1), &format!("r{i}"))], 1)).collect(),
            ),
        };
        let height = |r: &Row<'_>| l.row_height(r, l.frame_room(r), Fill::Rule);
        let r = row(vec![stack(3, VAlign::Top, None), col(Width::Fr(1), "one")], 1);
        assert_eq!(height(&r), 3);
        let r = row(vec![stack(2, VAlign::Top, Some(&BoxRef::Anon))], 1);
        assert_eq!(height(&r), 4, "a boxed column is its content plus two edge lines");
        // A stack is the *sum* of its rows' heights, not their count: three
        // boxed inner rows are three cells of content and six edge lines.
        let boxed_stack = Col {
            width: Width::Fr(1),
            justify: Justify::Left,
            valign: VAlign::Top,
            boxed: None,
            content: Content::Stack(
                (0..3)
                    .map(|i| Row {
                        boxed: Some(&BoxRef::Anon),
                        ..row(vec![col(Width::Fr(1), &format!("r{i}"))], 1)
                    })
                    .collect(),
            ),
        };
        assert_eq!(height(&row(vec![boxed_stack], 1)), 9);

        // The short column's own line sits where `valign` says.
        let text = |rows: &[Row<'_>]| {
            l.lines(rows)
                .into_iter()
                .flatten()
                .map(|line| crate::ansi::Painter::PLAIN.paint(&line.segments()))
                .collect::<Vec<_>>()
        };
        for (valign, at) in [(VAlign::Top, 0), (VAlign::Center, 1), (VAlign::Bottom, 2)] {
            let short = Col { valign, ..col(Width::Fr(1), "short") };
            let lines = text(&[row(vec![stack(3, VAlign::Top, None), short], 1)]);
            let found = lines.iter().position(|l| l.contains("short"));
            assert_eq!(found, Some(at), "{valign:?}: {lines:?}");
        }
    }

    /// SPEC § 4.3: a box too narrow to draw (its corners or its sides do not
    /// fit the column's share) renders nothing and adds no lines. It kept
    /// its two edge lines, so its row came out two lines taller, empty ones
    /// under a frame with caps, and a one-line row's gaps turned to spaces:
    /// `╭─ ❖ Opus ───   ─── ⏱ 1h12m ─╮` over two empty framed lines.
    #[test]
    fn a_box_too_narrow_to_draw_adds_no_lines() {
        let plus = || "+".to_owned();
        let model = || col(Width::Fr(1), "❖ Opus");
        let end = || col(Width::Fr(1), "end");
        let boxed = |width| Col { boxed: Some(&BoxRef::Anon), ..col(width, "⠋ 16:00:00") };
        let stacked = |width| Col {
            content: Content::Stack(vec![Row {
                boxed: Some(&BoxRef::Anon),
                ..row(vec![col(Width::Fr(1), "⠋ 16:00:00")], 1)
            }]),
            ..col(width, "")
        };
        for (fill, width) in [true, false].into_iter().flat_map(|f| [40_usize, 60].map(|w| (f, w)))
        {
            // The rounded box needs two cells; a `custom` one with corners
            // and no side needs two for its corners, one for none of it.
            for custom in [false, true] {
                let mut f = Fixture::new(FrameStyle::Rounded, fill, width);
                if custom {
                    f.chars.top_left = plus();
                    f.chars.top_right = plus();
                    f.chars.bottom_left = plus();
                    f.chars.bottom_right = plus();
                    f.chars.side = String::new();
                }
                let l = f.layout();
                let lines = |cols: Vec<Col<'static>>, gap: usize| -> Vec<String> {
                    let r = row(cols, gap);
                    l.lines(std::slice::from_ref(&r))
                        .into_iter()
                        .flatten()
                        .map(|l| show(&l))
                        .collect()
                };
                for narrow in [
                    boxed(Width::Cells(0)),
                    boxed(Width::Cells(1)),
                    stacked(Width::Cells(0)),
                    stacked(Width::Cells(1)),
                ] {
                    let last = lines(vec![model(), narrow.clone()], 1);
                    assert_eq!(last.len(), 1, "custom={custom}: {last:?}");
                    // Its cells are empty ones, rule on a one-line row, and
                    // as the last column it does not end the row in
                    // content: the rule runs into the cap with no hole.
                    let tail = last.first().and_then(|l| l.split_once("Opus ")).map(|(_, t)| t);
                    assert!(
                        !fill || tail.is_some_and(|t| t.chars().all(|c| c == '─')),
                        "custom={custom}: {last:?}"
                    );
                    let between = lines(vec![model(), narrow, end()], 3);
                    assert_eq!(between.len(), 1, "custom={custom}: {between:?}");
                }
                // A zero-width box takes no cells, no gap and no lines: the
                // row is the row without it, its gap a rule again.
                assert_eq!(
                    lines(vec![model(), boxed(Width::Cells(0)), end()], 3),
                    lines(vec![model(), end()], 3),
                    "custom={custom}"
                );
                // Two cells hold either box: it draws, three lines tall.
                let two = lines(vec![model(), boxed(Width::Cells(2))], 1);
                assert_eq!(two.len(), 3, "custom={custom}: {two:?}");
            }
        }
    }

    /// SPEC § 4.3: a title takes the run of empty cells its justification
    /// names, but only when that run can hold it. The gap between two
    /// columns is a run of empty cells too, and it is the *first* one on the
    /// line: taking it literally cut a left title down to its ellipsis.
    #[test]
    fn a_title_skips_a_run_too_narrow_to_hold_it() {
        let f = Fixture::new(FrameStyle::Rounded, true, 40);
        let l = f.layout();
        let title = |justify| crate::config::TitleCfg {
            text: "Repo".to_owned(),
            justify,
            pad: 1,
            color: None,
        };
        let line = |r: &Row<'_>| {
            l.lines(std::slice::from_ref(r))
                .into_iter()
                .flatten()
                .map(|line| Painter::PLAIN.paint(&line.segments()))
                .collect::<Vec<_>>()
                .join("\n")
        };
        // The first run is the one-cell gap after a column its content
        // fills exactly; the title goes to the rule beyond it instead.
        let t = title(Justify::Left);
        let narrow = Row {
            title: Some(&t),
            ..row(vec![col(Width::Cells(6), "abcd"), col(Width::Fr(1), "x")], 1)
        };
        let out = line(&narrow);
        assert!(out.contains("Repo"), "the title is whole, not cut to its ellipsis: {out}");
        assert!(!out.contains('…'), "{out}");
        // A first run that does hold it still wins over the widest one.
        let wide = Row { title: Some(&t), ..row(vec![col(Width::Fr(1), "x")], 1) };
        let out = line(&wide);
        let at = out.find("Repo").unwrap_or(usize::MAX);
        assert!(at < 12, "a left title stays near the start of the line: {out}");
        // The right title mirrors it.
        let t = title(Justify::Right);
        let right = Row { title: Some(&t), ..row(vec![col(Width::Fr(1), "x")], 1) };
        let out = line(&right);
        assert!(out.contains("Repo"), "{out}");
    }

    /// SPEC § 4.3: `blank` on an inner row follows the § 4.1 rule, a line
    /// that would be whitespace only gets the braille cell and a line with
    /// a visible frame or box needs none. The inner row used to mark every
    /// one of its lines before the outer row had drawn its caps or sides.
    #[test]
    fn a_blank_inner_row_marks_only_a_line_that_is_whitespace() {
        let stack = || Col {
            content: Content::Stack(vec![
                row(vec![col(Width::Fr(1), "model")], 1),
                Row { blank: true, ..row(vec![col(Width::Fr(1), "")], 1) },
                row(vec![col(Width::Fr(1), "clock")], 1),
            ]),
            ..col(Width::Fr(1), "")
        };
        let blank = BLANK_CELL;
        let lines = |style: FrameStyle, boxed: Option<&'static BoxRef>| -> Vec<String> {
            let f = Fixture::new(style, false, 40);
            let l = f.layout();
            let r = row(vec![Col { boxed, ..stack() }], 1);
            l.lines(std::slice::from_ref(&r)).into_iter().flatten().map(|l| show(&l)).collect()
        };
        for (style, boxed) in [(FrameStyle::Rounded, None), (FrameStyle::None, Some(&BoxRef::Anon))]
        {
            let out = lines(style, boxed);
            assert!(out.iter().all(|l| !l.contains(blank)), "{style:?}: {out:?}");
        }
        // No frame and nothing packed after it: the line is the cell alone,
        // as a blank spacer row is.
        let out = lines(FrameStyle::None, None);
        assert_eq!(out.get(1).map(String::as_str), Some(blank.to_string().as_str()), "{out:?}");
    }

    /// `[frame] pad` is text (docs/config.md: "Text between prefix/content
    /// and content/rule"), drawn wherever the frame pads: after the prefix,
    /// around the groups, before the cap, beside a column's text and inside
    /// a box's sides. Phase 21 drew its width in spaces instead, so `pad =
    /// "·"` showed as a space.
    #[test]
    fn a_frame_pad_is_drawn_as_its_text() {
        let mut f = Fixture::new(FrameStyle::Rounded, true, 30);
        f.chars.pad = "·".to_owned();
        let (left, right) = ([Segment::plain("left")], [Segment::plain("R")]);
        let s = f.compose(0, 2, &left, &right, " │ ");
        assert_eq!(s, format!("╭─·left·{}·R·─╮", "─".repeat(17)));
        let l = f.layout();
        let r = row(vec![col(Width::Fr(1), "a"), col(Width::Fr(1), "b")], 1);
        let boxed = Row { boxed: Some(&BoxRef::Anon), ..row(vec![col(Width::Fr(1), "c")], 1) };
        let lines: Vec<String> =
            l.lines(&[r, boxed]).into_iter().flatten().map(|line| show(&line)).collect();
        assert!(lines[0].starts_with("──·a·─"), "{lines:?}");
        assert!(lines[2].starts_with("│·c "), "{lines:?}");
        assert!(lines[2].ends_with(" ·│"), "{lines:?}");
    }

    /// SPEC § 14: the module a cut lands in owns the ellipsis and nothing
    /// past it. When a two-cell glyph straddles the cut, the cut ends a cell
    /// early, and the map used to hand that module the cell after the piece.
    #[test]
    fn a_cut_before_a_wide_glyph_maps_no_cell_past_the_piece() {
        let f = Fixture::new(FrameStyle::None, false, 40);
        let l = f.layout();
        let group = vec![vec![Segment::plain("aaaaa")], vec![Segment::plain("漢bbb")]];
        let ids = vec!["a".to_owned(), "b".to_owned()];
        for budget in 6..=9_usize {
            let pieces = l.fit_group(l.group_pieces(&group, &ids, " "), budget, true, false);
            let piece = pieces.first().unwrap();
            let Elem::Group(map) = &piece.elem else { panic!("not cut: {pieces:?}") };
            let end = segments_width(&piece.segs);
            assert_eq!(map.last().map(|(_, r)| r.end), Some(end), "budget {budget}: {map:?}");
        }
    }

    /// SPEC § 4.3 Pads: inside a box a lone group needs no pad and no fill
    /// cell on a side that faces the box's side, whose own pad already keeps
    /// the text off it. Both were reserved, so a module up to two cells
    /// narrower than the interior was cut: `│ ❖ O… │`. Between two columns
    /// the reservation stays: with `gap = 0` it is what keeps them apart.
    #[test]
    fn a_lone_group_in_a_box_may_fill_its_interior() {
        // 14 cells: two sides and two pads leave an interior of 10.
        let f = Fixture::new(FrameStyle::Rounded, true, 14);
        let l = f.layout();
        for text in ["0123456789", "012345678", "01234567"] {
            for justify in [Justify::Left, Justify::Center, Justify::Right] {
                let r = Row {
                    boxed: Some(&BoxRef::Anon),
                    ..row(vec![Col { justify, ..col(Width::Fr(1), text) }], 1)
                };
                for line in l.lines(std::slice::from_ref(&r)).into_iter().flatten() {
                    assert_eq!(line.width(), 14, "{}", show(&line));
                    assert!(!show(&line).contains('…'), "{justify:?}: {}", show(&line));
                }
            }
        }
        let f = Fixture::new(FrameStyle::Rounded, true, 30);
        let l = f.layout();
        let long = "abcdefghijklmnopqrstuvwxyz";
        let r = Row {
            boxed: Some(&BoxRef::Anon),
            ..row(vec![col(Width::Fr(1), long), col(Width::Fr(1), long)], 0)
        };
        let lines = l.lines(std::slice::from_ref(&r));
        let body = lines.first().and_then(|r| r.get(1)).map_or_default(show);
        assert!(!body.contains("…a"), "the columns touch: {body}");
        assert!(body.ends_with("… │"), "the last column fills to the box's pad: {body}");
    }

    /// SPEC § 4.3 Pads (decided with Daniel 2026-09-25): inside a box the
    /// gap's spaces keep two columns apart, so a side facing a neighbour
    /// keeps a fill cell and a pad only at `gap = 0`. They were reserved at
    /// every gap, and the `box-columns` golden cut `context` to `4…` at
    /// `gap = 2`.
    #[test]
    fn inside_a_box_a_gap_is_what_keeps_two_columns_apart() {
        // 34 cells: two sides and two pads leave an interior of 30.
        let f = Fixture::new(FrameStyle::Rounded, true, 34);
        let l = f.layout();
        let body = |gap: usize, texts: [&str; 3]| -> String {
            let cols = [Justify::Left, Justify::Center, Justify::Right]
                .into_iter()
                .zip(texts)
                .map(|(justify, text)| Col { justify, ..col(Width::Fr(1), text) })
                .collect();
            let r = Row { boxed: Some(&BoxRef::Anon), ..row(cols, gap) };
            let lines = l.lines(std::slice::from_ref(&r));
            lines.first().and_then(|r| r.get(1)).map_or_default(show)
        };
        // Shares of 9, 9 and 8 at `gap = 2`, of 10, 9 and 9 at `gap = 1`:
        // each text fills its column exactly and stands uncut.
        assert_eq!(
            body(2, ["aaaaaaaaa", "bbbbbbbbb", "cccccccc"]),
            "│ aaaaaaaaa  bbbbbbbbb  cccccccc │"
        );
        assert_eq!(
            body(1, ["aaaaaaaaaa", "bbbbbbbbb", "ccccccccc"]),
            "│ aaaaaaaaaa bbbbbbbbb ccccccccc │"
        );
        // At `gap = 0` nothing else separates them: each keeps its cell and
        // pad on the sides that face a neighbour, and is cut to make room.
        let touching = body(0, ["aaaaaaaaaa", "bbbbbbbbbb", "cccccccccc"]);
        assert_eq!(display_width(&touching), 34, "{touching}");
        for joined in ["ab", "bc", "…b", "…c"] {
            assert!(!touching.contains(joined), "{joined}: {touching}");
        }
    }

    /// SPEC § 4.3: on a multi-line row gap cells and padding lines are
    /// spaces, and a title set into one keeps them spaces. The cells around
    /// the title were always built as rule, so a centred title landing in a
    /// short column's padding line drew a rule across it.
    #[test]
    fn a_title_in_a_run_of_spaces_keeps_them_spaces() {
        let f = Fixture::new(FrameStyle::Rounded, true, 60);
        let l = f.layout();
        let t = crate::config::TitleCfg {
            text: "Panel".to_owned(),
            justify: Justify::Center,
            pad: 1,
            color: None,
        };
        let short = Col { valign: VAlign::Bottom, ..col(Width::Fr(1), "model") };
        let stack = Col {
            content: Content::Stack(vec![
                row(vec![col(Width::Fr(1), "r0")], 1),
                row(vec![col(Width::Fr(1), "r1")], 1),
            ]),
            ..col(Width::Fr(1), "")
        };
        let r = Row { title: Some(&t), ..row(vec![short, stack], 2) };
        let lines = l.lines(std::slice::from_ref(&r));
        let first = lines.first().and_then(|r| r.first()).map_or_default(show);
        let (before, after) = first.split_once("Panel").unwrap_or_default();
        let before = before.trim_start_matches(['╭', '─']);
        let between = after.split_once("r0").map_or("", |(b, _)| b);
        assert!(!before.contains('─') && !between.contains('─'), "{first}");
        assert!(!between.is_empty(), "{first}");
    }

    /// SPEC § 4.3: a title is cut to the room it has and never widens the
    /// line. This used to be a `debug_assert!`, which is a panic path on the
    /// render path: a title wider than its box aborted the tick.
    #[test]
    fn an_over_wide_title_is_cut_and_the_line_keeps_its_width() {
        let long = "a title far wider than any line this row will ever be given";
        let t = crate::config::TitleCfg {
            text: long.to_owned(),
            justify: Justify::Center,
            pad: 1,
            color: None,
        };
        for width in [12_usize, 20, 40] {
            let f = Fixture::new(FrameStyle::Rounded, true, width);
            let l = f.layout();
            let mut boxed = row(vec![col(Width::Fr(1), "x")], 1);
            boxed.boxed = Some(&BoxRef::Anon);
            for r in [Row { title: Some(&t), ..row(vec![col(Width::Fr(1), "x")], 1) }, boxed] {
                for line in l.lines(std::slice::from_ref(&r)).into_iter().flatten() {
                    let text = Painter::PLAIN.paint(&line.segments());
                    assert_eq!(line.width(), width, "{text:?}");
                    assert!(!text.contains(long), "{text:?}");
                }
            }
        }
    }

    /// SPEC § 4.3: a line's rule cells are numbered together, so a
    /// `fill_pattern` travels across a column boundary instead of restarting
    /// in every column.
    #[test]
    fn a_rule_pattern_runs_across_a_column_boundary() {
        let mut f = Fixture::new(FrameStyle::None, true, 30);
        f.rule = Some(Rule { cells: vec!["a".into(), "b".into(), "c".into()], offset: 0 });
        let l = f.layout();
        // Two columns with a module each: the rule is broken into runs by
        // the modules, and the pattern picks up where the previous run left
        // off rather than starting at `a` again.
        let r = row(vec![col(Width::Fr(1), "LL"), col(Width::Fr(1), "RR")], 2);
        let line = l
            .lines(std::slice::from_ref(&r))
            .into_iter()
            .flatten()
            .next()
            .unwrap_or_else(|| Line { pieces: Vec::new() });
        let runs: Vec<String> = line
            .pieces
            .iter()
            .filter(|p| p.elem == Elem::Rule)
            .map(|p| Painter::PLAIN.paint(&p.segs))
            .collect();
        assert!(runs.len() >= 2, "the modules break the rule into runs: {runs:?}");
        let joined = runs.concat();
        let want: String = "abc".chars().cycle().take(joined.chars().count()).collect();
        assert_eq!(joined, want, "the runs of {:?}", Painter::PLAIN.paint(&line.segments()));
    }

    /// SPEC § 4.3: a tall row's lines take different caps (`first` then
    /// `middle`, then `last`), and a `custom` frame's need not be the same
    /// width. The row is laid out to the room the *widest* pair leaves;
    /// laying it out to the first line's, as it once was, left the taller
    /// caps hanging past the box and the row's own recut ate them into `…`.
    /// A narrower cap's spare cells go to the rule, an empty cap's too: they
    /// once went only before a cap, so a line whose right cap was empty
    /// came out short of the box.
    #[test]
    fn a_tall_row_under_uneven_custom_caps_fits_every_line() {
        let uneven = FrameChars {
            first: "<".to_owned(),
            middle: "<<<".to_owned(),
            last: "<<<<<".to_owned(),
            single: "<".to_owned(),
            right_first: ">".to_owned(),
            right_middle: ">>>".to_owned(),
            right_last: ">>>>>".to_owned(),
            right_single: ">".to_owned(),
            ..FrameChars::for_style(FrameStyle::Rounded)
        };
        let emptied = FrameChars {
            right_first: "你".to_owned(),
            right_middle: String::new(),
            right_last: String::new(),
            ..uneven.clone()
        };
        let cases = [
            (uneven, [("<", ">"), ("<<<", ">>>"), ("<<<<<", ">>>>>")]),
            (emptied, [("<", "你"), ("<<<", ""), ("<<<<<", "")]),
        ];
        for (width, (chars, ends)) in
            [24_usize, 40, 80].into_iter().flat_map(|w| cases.clone().map(|c| (w, c)))
        {
            let mut f = Fixture::new(FrameStyle::Custom, true, width);
            f.chars = chars;
            let l = f.layout();
            let stack = Col {
                width: Width::Fr(1),
                justify: Justify::Left,
                valign: VAlign::Top,
                boxed: None,
                content: Content::Stack(vec![
                    row(vec![col(Width::Fr(1), "one")], 1),
                    row(vec![col(Width::Fr(1), "two")], 1),
                    row(vec![col(Width::Fr(1), "three")], 1),
                ]),
            };
            let rows = vec![Row { cols: vec![stack], ..row(Vec::new(), 1) }];
            for (line, (prefix, cap)) in l.lines(&rows).into_iter().flatten().zip(ends) {
                let text = Painter::PLAIN.paint(&line.segments());
                assert_eq!(line.width(), width, "{text:?}");
                assert!(text.starts_with(prefix) && text.ends_with(cap), "{text:?}");
            }
            // With a right group on every line the pad stands against it and
            // the spare cells go to the rule behind it (content, pad, rule,
            // cap). They went between the text and the pad, so a later line
            // under a narrower or empty cap read `⏱ 1h12m──`; a line with no
            // cap and no cells to spare ends in its text.
            let flexed = |right: &'static str| row(vec![flex(Width::Fr(1), "l", right)], 1);
            let stack = Col {
                content: Content::Stack(vec![flexed("r0"), flexed("r1"), flexed("r2")]),
                ..col(Width::Fr(1), "")
            };
            let rows = vec![Row { cols: vec![stack], ..row(Vec::new(), 1) }];
            let lines = l.lines(&rows).into_iter().flatten().zip(ends).enumerate();
            for (i, (line, (_, cap))) in lines {
                let text = Painter::PLAIN.paint(&line.segments());
                assert_eq!(line.width(), width, "{text:?}");
                let after = text.strip_suffix(cap).and_then(|t| t.rsplit_once(&format!("r{i}")));
                assert!(
                    after.is_some_and(|(_, t)| t.is_empty()
                        || (t.starts_with(' ') && t.chars().skip(1).all(|c| c == '─'))),
                    "the rule touches the text: {text:?}"
                );
            }
        }
    }

    /// SPEC § 4.3: under a `custom` frame whose caps differ, a row of the
    /// frame is measured on the caps of the lines it lands on. It was
    /// measured on the narrowest pair (here the wide `right_last`, which only
    /// the last row carries) and laid out on its own, so a box that fitted
    /// only there was drawn at full height and cut to the smaller measure:
    /// its bottom edge and the rows after it were gone
    /// (`| ⠋ 16:00… | ]` with no `+---+` under it), or its top edge stood
    /// alone.
    #[test]
    fn a_row_is_measured_on_the_caps_its_lines_land_on() {
        let wide = "]".repeat(12);
        let chars = FrameChars {
            first: "[".to_owned(),
            middle: "|".to_owned(),
            last: "[".to_owned(),
            single: "[".to_owned(),
            right_first: "]".to_owned(),
            right_middle: "]".to_owned(),
            right_last: wide.clone(),
            right_single: "]".to_owned(),
            fill: "-".to_owned(),
            pad: " ".to_owned(),
            top_left: "A".to_owned(),
            top_right: "B".to_owned(),
            bottom_left: "C".to_owned(),
            bottom_right: "D".to_owned(),
            side: "|".to_owned(),
            ..FrameChars::for_style(FrameStyle::Rounded)
        };
        let bare = |text: &str| row(vec![col(Width::Fr(1), text)], 1);
        let boxed = |text: &str| Row { boxed: Some(&BoxRef::Anon), ..bare(text) };
        let shapes = [
            // A stack with a boxed row, and a boxed column holding a stack.
            Col {
                content: Content::Stack(vec![bare("x"), boxed("y"), bare("z")]),
                ..col(Width::Fr(1), "")
            },
            Col {
                boxed: Some(&BoxRef::Anon),
                content: Content::Stack(vec![bare("x"), bare("y"), bare("z")]),
                ..col(Width::Fr(1), "")
            },
        ];
        let lines_of = |l: &Layout<'_>, rows: &[Row<'_>]| -> Vec<Vec<String>> {
            l.lines(rows).iter().map(|lines| lines.iter().map(show).collect()).collect()
        };
        for (width, shape) in (16_usize..=48).flat_map(|w| shapes.clone().map(|s| (w, s))) {
            let mut f = Fixture::new(FrameStyle::Custom, true, width);
            f.chars = chars.clone();
            let l = f.layout();
            let rows = [row(vec![col(Width::Cells(12), "m"), shape], 1), bare("p")];
            let lines = lines_of(&l, &rows);
            let text = lines.iter().flatten().cloned().collect::<Vec<_>>().join("\n");
            for line in l.lines(&rows).iter().flatten() {
                assert_eq!(line.width(), width, "{width}:\n{text}");
            }
            let top = lines.first().cloned().unwrap_or_default();
            let count = |c: char| top.iter().filter(|line| line.contains(c)).count();
            assert_eq!(
                count('A'),
                count('C'),
                "a box is drawn whole or not at all, {width}:\n{text}"
            );
            assert_eq!(count('x'), count('z'), "no row of the stack is cut away, {width}:\n{text}");
            // The first row carries `first` and `middle` caps, one cell
            // each, and never the last line's: its stack column has what
            // the pads, the caps, the fixed column and the gap leave, and
            // its box, two sides wide, is drawn wherever that holds it.
            let share = width.saturating_sub(2 + 1 + 12 + 1);
            assert_eq!(count('A'), usize::from(share >= 2), "{width}:\n{text}");
            assert!(lines.get(1).is_some_and(|p| p.iter().all(|l| l.ends_with(&wide))), "{text}");
        }
        // A row that never settles: one line under the narrow `single` caps
        // holds its box, and the three lines the box needs carry `first`
        // and `last` caps too wide for it. It keeps the first measure, on
        // the fewest cells any pair of caps leaves, and is laid out there:
        // one line, no box, the rest of the line rule before the cap.
        let tall = FrameChars { right_first: wide.clone(), right_middle: wide, ..chars };
        for width in 16_usize..=48 {
            let mut f = Fixture::new(FrameStyle::Custom, true, width);
            f.chars = tall.clone();
            let l = f.layout();
            let r = row(
                vec![
                    col(Width::Cells(12), "m"),
                    Col { boxed: Some(&BoxRef::Anon), ..col(Width::Fr(1), "x") },
                ],
                1,
            );
            let lines = lines_of(&l, std::slice::from_ref(&r)).concat();
            let text = lines.join("\n");
            let fits_single = width.saturating_sub(2 + 1 + 12 + 1) >= 2;
            let fits_tall = width.saturating_sub(2 + 12 + 12 + 1) >= 2;
            let want = if fits_tall { 3 } else { 1 };
            assert_eq!(lines.len(), want, "{width} (single caps fit: {fits_single}):\n{text}");
            assert_eq!(
                lines.iter().filter(|l| l.contains('A')).count(),
                lines.iter().filter(|l| l.contains('C')).count(),
                "{width}:\n{text}"
            );
            for line in l.lines(std::slice::from_ref(&r)).iter().flatten() {
                assert_eq!(line.width(), width, "{width}:\n{text}");
            }
        }
    }

    /// SPEC § 4.3: a box under a frame that has no shape (`none`,
    /// `powerline`) borrows the rounded glyphs, or it would silently become
    /// indentation — powerline's corners and side are empty strings.
    #[test]
    fn a_box_under_a_shapeless_frame_borrows_the_rounded_glyphs() {
        for style in [FrameStyle::Powerline, FrameStyle::None] {
            let f = Fixture::new(style, true, 40);
            let l = f.layout();
            let r = Row { boxed: Some(&BoxRef::Anon), ..row(vec![col(Width::Fr(1), "x")], 1) };
            let lines = l
                .lines(std::slice::from_ref(&r))
                .into_iter()
                .flatten()
                .map(|line| Painter::PLAIN.paint(&line.segments()))
                .collect::<Vec<_>>();
            let (first, last) = (lines.first().cloned().unwrap_or_default(), lines.last().cloned());
            assert!(first.starts_with('╭') && first.ends_with('╮'), "{style:?}: {lines:?}");
            let last = last.unwrap_or_default();
            assert!(last.starts_with('╰') && last.ends_with('╯'), "{style:?}: {lines:?}");
            assert!(lines.len() >= 3, "{style:?}: {lines:?}");
        }
    }

    /// SPEC § 14: the placement map is the line's pieces with the cells they
    /// occupy, so a click lands on the thing under it. The spans tile the
    /// line: they start at zero, touch, and end at its width.
    #[test]
    fn the_placement_map_tiles_the_line_with_the_kinds_it_drew() {
        let f = Fixture::new(FrameStyle::Rounded, true, 60);
        let l = f.layout();
        let t =
            crate::config::TitleCfg { text: "T".to_owned(), ..crate::config::TitleCfg::default() };
        let rows = vec![
            Row {
                title: Some(&t),
                ..row(vec![col(Width::Fr(1), "one"), col(Width::Fr(1), "two")], 2)
            },
            Row { boxed: Some(&BoxRef::Anon), ..row(vec![col(Width::Fr(1), "three")], 1) },
        ];
        let mut kinds: Vec<Elem> = Vec::new();
        for line in l.lines(&rows).into_iter().flatten() {
            let spans = line.spans();
            let mut at = 0_usize;
            for (elem, range) in spans {
                assert_eq!(range.start, at, "the spans touch: {:?}", line.spans());
                at = range.end;
                kinds.push(elem);
            }
            assert_eq!(at, line.width(), "the spans cover the line: {:?}", line.spans());
            assert_eq!(at, f.width);
        }
        for want in [Elem::Cap, Elem::Pad, Elem::Rule, Elem::Title, Elem::BoxEdge] {
            assert!(kinds.contains(&want), "{want:?} never drawn: {kinds:?}");
        }
        assert!(kinds.iter().any(|k| matches!(k, Elem::Module(_))), "no module: {kinds:?}");
    }

    /// SPEC § 4.1, § 14: the ticker's offset, its window and the placement
    /// map count cells the same way, cluster by cluster. `لا` is one cell to
    /// `unicode-width` but two clusters to the scroller: the offset used to
    /// wrap a cell early and the map to place the wrapped module a cell off.
    #[test]
    fn a_ticker_over_a_ligature_keeps_its_period_and_its_map() {
        let left: Vec<Vec<Segment>> = vec![vec![Segment::plain("xلاy")], vec![Segment::plain("pq")]];
        let ids = vec!["a".to_owned(), "b".to_owned()];
        let line_at = |secs: i64| {
            let mut f = Fixture::new(FrameStyle::None, false, 5);
            f.ticker = Some(Ticker {
                step: 1.0,
                gap: "   ".to_owned(),
                now: jiff::Timestamp::from_second(secs).unwrap(),
            });
            let l = f.layout();
            let rows = vec![Row {
                cols: vec![Col {
                    width: Width::Fr(1),
                    justify: Justify::Left,
                    valign: VAlign::Top,
                    boxed: None,
                    content: Content::Groups {
                        left: &left,
                        right: &[],
                        left_ids: &ids,
                        right_ids: &[],
                    },
                }],
                gap: 1,
                separator: " ",
                title: None,
                boxed: None,
                blank: false,
            }];
            let lines = l.lines(&rows);
            let line = lines.first().and_then(|r| r.first()).unwrap();
            (Painter::PLAIN.paint(&line.segments()), line.modules())
        };
        // `xلاy pq` is seven clusters, and the gap three: a period of ten.
        let (text, map) = line_at(8);
        assert_eq!(text, "  xلا");
        assert_eq!(map, vec![("a".to_owned(), 2..5)]);
        let (text, map) = line_at(9);
        assert_eq!(text, " xلاy");
        assert_eq!(map, vec![("a".to_owned(), 1..5)]);
        assert_eq!(line_at(18), line_at(8));
        assert_ne!(line_at(17), line_at(8));
    }

    /// SPEC § 14: the placement map names the module behind every cell it
    /// owns, in a cut run as well as a scrolled one. A cut module owns the
    /// ellipsis, a module past the cut owns nothing, and a module straddling
    /// the ticker's wrap owns a run at each end of the window.
    #[test]
    fn the_placement_map_names_modules_through_a_cut_and_a_scroll() {
        let owned = |ids: &[&str]| -> Vec<(String, Range<usize>)> {
            let mut at = 0_usize;
            ids.iter()
                .map(|id| {
                    let r = at..at + 5;
                    at += 6; // five cells of text, a one-cell separator
                    ((*id).to_owned(), r)
                })
                .collect()
        };
        // 17 cells (`aaaaa bbbbb ccccc`) cut to 9: `aaaaa bb…`, so `a` keeps
        // its five, `b` keeps two and the ellipsis, `c` nothing.
        let cut = cut_map(&owned(&["a", "b", "c"]), 8, 9);
        assert_eq!(cut, vec![("a".to_owned(), 0..5), ("b".to_owned(), 6..9)]);
        // A cut landing on the separator gives the ellipsis to the module
        // after it, whose text it stands for.
        let cut = cut_map(&owned(&["a", "b"]), 5, 6);
        assert_eq!(cut, vec![("a".to_owned(), 0..5), ("b".to_owned(), 5..6)]);
        // Scrolled 14 cells into a 20-cell period (17 + a three-cell gap)
        // with a 9-cell window: `b` has scrolled off, `c` shows its last
        // three cells, the gap, then `a` wraps in with three cells.
        let scrolled = scrolled_map(&owned(&["a", "b", "c"]), 14, 20, 9);
        assert_eq!(scrolled, vec![("c".to_owned(), 0..3), ("a".to_owned(), 6..9)]);
        // Through the layout: a ticker line's `Group` piece carries the map,
        // and `Line::modules` offsets it to the line's own cells.
        let left: Vec<Vec<Segment>> =
            vec![vec![Segment::plain("aaaaa")], vec![Segment::plain("bbbbb")]];
        let ids = vec!["a".to_owned(), "b".to_owned()];
        let mut f = Fixture::new(FrameStyle::None, false, 8);
        f.ticker = Some(Ticker {
            step: 1.0,
            gap: "   ".to_owned(),
            now: jiff::Timestamp::from_second(3).unwrap(),
        });
        let l = f.layout();
        let rows = vec![Row {
            cols: vec![Col {
                width: Width::Fr(1),
                justify: Justify::Left,
                valign: VAlign::Top,
                boxed: None,
                content: Content::Groups {
                    left: &left,
                    right: &[],
                    left_ids: &ids,
                    right_ids: &[],
                },
            }],
            gap: 1,
            separator: " ",
            title: None,
            boxed: None,
            blank: false,
        }];
        let lines = l.lines(&rows);
        let line = lines.first().and_then(|r| r.first()).unwrap();
        assert_eq!(Painter::PLAIN.paint(&line.segments()), "aa bbbbb");
        assert_eq!(line.modules(), vec![("a".to_owned(), 0..2), ("b".to_owned(), 3..8)]);
        // And an uncut run names each module piece by piece.
        let f = Fixture::new(FrameStyle::Rounded, true, 30);
        let l = f.layout();
        let lines = l.lines(&rows);
        let line = lines.first().and_then(|r| r.first()).unwrap();
        let map = line.modules();
        assert_eq!(map.len(), 2);
        assert_eq!(map[0].0, "a");
        assert_eq!(map[0].1, 3..8, "{map:?}");
        assert_eq!(map[1].1, 9..14, "{map:?}");
    }
}
