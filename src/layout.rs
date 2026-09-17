//! Laying a configured row out into terminal lines (SPEC § 4.3).
//!
//! A **row** is columns side by side, one or more **lines** tall; a column
//! holds its own module groups or a stack of inner rows; titles decorate
//! rules and boxes decorate rows and columns. Everything here is arithmetic
//! over segments the modules have already rendered: nothing is read, nothing
//! is spawned, and a row of one `1fr` column is exactly the flex line garnish
//! has always drawn.

use std::ops::Range;

use itertools::Itertools;

use crate::ansi::{Segment, Style, display_width, scroll, segments_width, truncate};
use crate::config::{BoxCfg, BoxRef, Justify, VAlign, Width};
use crate::frame::{FrameChars, FrameStyle, Rule, Ticker};
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
    /// One module's decorated render.
    Module,
    /// The separator between two modules.
    Separator,
    /// A run of modules the layout could not keep apart, because the group
    /// was cut or scrolled and the boundary falls wherever the window does.
    Group,
    /// A row's or a box's title.
    Title,
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

    /// Replace the line's segments, keeping one piece per original piece is
    /// not possible after a cut, so the result is one piece.
    fn recut(&self, width: usize, ellipsis: &str) -> Self {
        let segs = truncate(&self.segments(), width, ellipsis);
        Self { pieces: vec![Piece { elem: Elem::Group, segs }] }
    }
}

/// One row's worth of content, rendered and ready to be laid out.
#[derive(Debug, Clone)]
pub struct Row<'a> {
    /// The row's columns, left to right; never empty.
    pub cols: Vec<Col<'a>>,
    /// Empty cells between columns.
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
}

impl Draft {
    fn spaces(elem: Elem, n: usize) -> Self {
        Self::Done(Piece { elem, segs: vec![Segment::plain(" ".repeat(n))] })
    }

    fn cells(&self) -> usize {
        match self {
            Self::Done(p) => segments_width(&p.segs),
            Self::Rule(n) | Self::Space(n, _) => *n,
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
        let framed: usize = blocks
            .iter()
            .filter(|b| b.boxed.is_none())
            .map(|b| b.rows.iter().map(|(_, r)| self.row_height(r)).sum::<usize>())
            .sum();
        let mut out: Vec<Vec<Line>> = vec![Vec::new(); rows.len()];
        let mut index = 0_usize;
        for block in blocks {
            for (at, lines) in self.block_lines(&block, &mut index, framed) {
                if let Some(slot) = out.get_mut(at) {
                    slot.extend(lines);
                }
            }
        }
        out
    }

    /// The lines of one block, each tagged with the configured row it
    /// belongs to.
    fn block_lines(
        &self,
        block: &Block<'_, '_>,
        index: &mut usize,
        framed: usize,
    ) -> Vec<(usize, Vec<Line>)> {
        block.boxed.map_or_else(
            || {
                block
                    .rows
                    .iter()
                    .map(|(at, row)| {
                        let height = self.row_height(row);
                        let fill = if self.fill { Fill::Rule } else { Fill::Packed };
                        // A tall row's lines take different caps (`first`
                        // then `middle`), and a `custom` frame's need not
                        // be the same width: the columns share the room the
                        // *widest* pair leaves, so no line overflows and the
                        // others fill the difference with rule cells.
                        let inner = (0..height)
                            .map(|i| self.inner_width(row, index.saturating_add(i), framed))
                            .min()
                            .unwrap_or_else(|| self.inner_width(row, *index, framed));
                        let lines = self
                            .row_body(row, inner, height, fill, Fit::default())
                            .into_iter()
                            .enumerate()
                            .map(|(i, drafts)| {
                                // A row's title goes into its first line
                                // alone, however tall the row is.
                                let title = row.title.filter(|_| i == 0);
                                let line =
                                    self.wrap_frame(drafts, *index, framed, row, fill, title);
                                *index = index.saturating_add(1);
                                line
                            })
                            .collect();
                        (*at, lines)
                    })
                    .collect()
            },
            // A box replaces the frame's caps with its own edges.
            |boxed| self.boxed_block(block, boxed),
        )
    }

    /// The lines of a block drawn inside a box: its top rule with the title,
    /// the rows between its sides, and its bottom rule.
    fn boxed_block(&self, block: &Block<'_, '_>, boxed: &BoxRef) -> Vec<(usize, Vec<Line>)> {
        let cfg = self.box_cfg(boxed);
        let chars = self.box_chars(&cfg);
        let (inner, pad) = self.box_interior(&chars, self.width).unwrap_or((0, 0));
        // `box = true` on a row is the one way to title a one-row box, so
        // the row's own title stands in when the box has none.
        let title = cfg.title.as_ref().or_else(|| block.rows.first().and_then(|(_, r)| r.title));
        let mut out: Vec<(usize, Vec<Line>)> = Vec::new();
        let first = block.rows.first().map_or(0, |(at, _)| *at);
        let last = block.rows.last().map_or(0, |(at, _)| *at);
        out.push((first, vec![self.edge_line(&chars, true, title, &cfg)]));
        for (at, row) in &block.rows {
            let height = self.row_height(row);
            let body = self.row_body(row, inner, height, Fill::from_box(&cfg), Fit::default());
            let lines =
                body.into_iter().map(|d| self.wrap_box(d, &chars, &cfg, pad, row.blank)).collect();
            out.push((*at, lines));
        }
        out.push((last, vec![self.edge_line(&chars, false, None, &cfg)]));
        out
    }

    /// The cells a row's columns share, once the frame's caps of its first
    /// line are taken off (the caps of a style are the same width on every
    /// line; a `custom` frame whose caps differ has the difference filled
    /// with rule cells before the cap).
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
/// fill, on any of its lines.
fn row_ends_in_content(row: &Row<'_>) -> bool {
    row.cols.last().is_some_and(col_ends_in_content)
}

fn col_ends_in_content(col: &Col<'_>) -> bool {
    match &col.content {
        Content::Groups { left, right } => {
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
/// row is as tall as its tallest column (SPEC § 4.3).
impl Layout<'_> {
    fn row_height(&self, row: &Row<'_>) -> usize {
        row.cols.iter().map(|c| self.col_height(c)).max().unwrap_or(1).max(1)
    }

    fn col_height(&self, col: &Col<'_>) -> usize {
        let content = match &col.content {
            Content::Groups { .. } => 1,
            Content::Stack(rows) => {
                blocks(rows).iter().map(|b| self.block_height(b)).sum::<usize>().max(1)
            }
        };
        // A boxed column is its content plus the two edge lines, so a boxed
        // one-line column is a three-line box.
        if col.boxed.is_some() { content.saturating_add(2) } else { content }
    }

    /// One block of a stack: its rows, plus the two edge lines when they are
    /// inside a box. A top-level row's box is drawn by `boxed_block`, which
    /// adds the two lines there instead.
    fn block_height(&self, block: &Block<'_, '_>) -> usize {
        let rows: usize = block.rows.iter().map(|(_, r)| self.row_height(r)).sum();
        if block.boxed.is_some() { rows.saturating_add(2) } else { rows }
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
    /// `exact` marks a row laid out to its own content (an inner row of an
    /// `auto` column): it reserves no rule cell, so nothing is cut to make
    /// room for a rule that is not drawn (SPEC § 4.3).
    fn row_body(
        &self,
        row: &Row<'_>,
        width: usize,
        height: usize,
        fill: Fill,
        inherit: Fit,
    ) -> Vec<Vec<Draft>> {
        let widths = self.share(row, width, fill);
        let gap = row.gap;
        let last = row.cols.len().saturating_sub(1);
        let cols: Vec<Vec<Vec<Draft>>> = row
            .cols
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(j, (col, w))| {
                let fit = Fit {
                    exact: inherit.exact || col.width == Width::Auto,
                    pads: self.edge_pads(row, j, fill),
                    trailing: inherit.trailing && j == last,
                };
                self.col_lines(col, *w, height, row, fill, fit)
            })
            .collect();
        (0..height)
            .map(|i| {
                let mut line: Vec<Draft> = Vec::new();
                let mut used = 0_usize;
                for (j, (col, w)) in cols.iter().zip(&widths).enumerate() {
                    // A column clamped to nothing renders nothing, and so
                    // does its gap (SPEC § 4.3).
                    if *w == 0 {
                        continue;
                    }
                    if j > 0 && used > 0 {
                        // On a one-line row the rule runs through the gaps
                        // too, so a centred module floats on one rule; on a
                        // taller row a gap beside a box's side is spaces.
                        line.push(if height == 1 && fill == Fill::Rule {
                            Draft::Rule(gap)
                        } else {
                            Draft::Space(gap, Elem::Gap)
                        });
                        used = used.saturating_add(gap);
                    }
                    line.extend(col.get(i).cloned().unwrap_or_default());
                    used = used.saturating_add(*w);
                }
                let left = width.saturating_sub(used);
                if left > 0 && fill.draws() {
                    line.push(Self::filler(left, fill, Elem::Rule));
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
        let n = row.cols.len();
        let gaps = row.gap.saturating_mul(n.saturating_sub(1));
        let available = width.saturating_sub(gaps);
        let want: Vec<usize> = row
            .cols
            .iter()
            .enumerate()
            .map(|(j, col)| match col.width {
                Width::Cells(c) => c,
                Width::Auto => match Self::auto_width(col, row) {
                    0 => 0,
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
        let fixed: usize = want.iter().sum();
        let free = available.saturating_sub(fixed);
        let total_fr: u32 = row
            .cols
            .iter()
            .filter_map(|c| match c.width {
                Width::Fr(n) => Some(n),
                _ => None,
            })
            .sum();
        // floor(free × n ÷ Σfr) each, and the cells that floor left over go
        // one each to the first of them, so the shares differ by at most one
        // and always add up. The leftover is what the shares did not take,
        // never `free % Σfr`: with a weight above 1 the two differ, and
        // handing out the larger one overshoots the row.
        let fr = usize::try_from(total_fr).unwrap_or(1).max(1);
        let mut desired: Vec<usize> = row
            .cols
            .iter()
            .zip(&want)
            .map(|(col, w)| match col.width {
                Width::Fr(n) => {
                    let n = usize::try_from(n).unwrap_or(1);
                    free.saturating_mul(n).checked_div(fr).unwrap_or(0)
                }
                _ => *w,
            })
            .collect();
        let shared: usize = row
            .cols
            .iter()
            .zip(&desired)
            .filter(|(col, _)| matches!(col.width, Width::Fr(_)))
            .map(|(_, w)| *w)
            .sum();
        let mut leftover = free.saturating_sub(shared);
        for (col, take) in row.cols.iter().zip(desired.iter_mut()) {
            if leftover == 0 {
                break;
            }
            if matches!(col.width, Width::Fr(_)) {
                *take = take.saturating_add(1);
                leftover = leftover.saturating_sub(1);
            }
        }
        // Left to right, gap then column: a column whose gap plus one cell
        // does not fit renders nothing, and so does everything to its right.
        let mut left = width;
        let mut dropped = 0_usize;
        for (j, take) in desired.iter_mut().enumerate() {
            let cost = if j > 0 { row.gap } else { 0 };
            if left < cost.saturating_add(1) {
                *take = 0;
                dropped = dropped.saturating_add(1);
                continue;
            }
            left = left.saturating_sub(cost);
            *take = (*take).min(left);
            left = left.saturating_sub(*take);
        }
        // A dropped column is invisible on screen, so the one place it can
        // be explained is the debug log (SPEC § 4.3).
        if dropped > 0 {
            crate::debug::log(&format!(
                "layout: {dropped} of {n} columns dropped, {width} cells is too narrow for them"
            ));
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
        let trailing = row.cols.iter().all(|c| !matches!(c.width, Width::Fr(_)));
        (if j > 0 { pad } else { 0 }, if last && !trailing { 0 } else { pad })
    }

    /// An `auto` column is exactly its content: its groups joined by the
    /// separator, or its widest inner row.
    ///
    /// The width is re-measured every tick, so a column whose content
    /// changes width moves its neighbours: `auto` is for values that hold
    /// still (SPEC § 4.3).
    fn auto_width(col: &Col<'_>, row: &Row<'_>) -> usize {
        match &col.content {
            Content::Groups { left, right } => {
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
            Content::Stack(rows) => rows
                .iter()
                .map(|inner| {
                    inner.cols.iter().map(|c| Self::auto_width(c, inner)).max().unwrap_or(0)
                })
                .max()
                .unwrap_or(0),
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
            return self.col_body(col, width, height, row, fill, fit);
        };
        let cfg = self.box_cfg(boxed);
        let chars = self.box_chars(&cfg);
        let Some((inner, pad)) = self.box_interior(&chars, width) else {
            // Too narrow to draw at all: the column keeps its share and
            // renders nothing, as a clamped column does (SPEC § 4.3).
            return (0..height.max(1)).map(|_| vec![Draft::Space(width, Elem::Pad)]).collect();
        };
        let interior = height.saturating_sub(2);
        // Inside its own box a column is padded by the box, not by the row.
        let inside = Fit { pads: (0, 0), ..fit };
        let body = self.col_body(col, inner, interior, row, Fill::from_box(&cfg), inside);
        let mut lines = vec![self.edge_drafts(&chars, width, true, cfg.title.as_ref(), &cfg)];
        for drafts in body {
            lines.push(self.side_drafts(drafts, &chars, &cfg, pad));
        }
        lines.push(self.edge_drafts(&chars, width, false, None, &cfg));
        lines.truncate(height.max(1));
        lines
    }

    /// The cells inside a box of `width`, and the pad each side of them.
    ///
    /// A box is its two sides, a pad each side of the content and the
    /// content: `None` when even the sides do not fit, and no pads when
    /// they would leave nothing to draw in.
    fn box_interior(&self, chars: &BoxChars, width: usize) -> Option<(usize, usize)> {
        let sides = display_width(&chars.side).saturating_mul(2);
        let left = width.checked_sub(sides)?;
        let pad = self.box_pad();
        if left > pad.saturating_mul(2) {
            Some((left.saturating_sub(pad.saturating_mul(2)), pad))
        } else {
            Some((left, 0))
        }
    }

    /// A column's content lines, padded to `height` by `valign`.
    fn col_body(
        &self,
        col: &Col<'_>,
        width: usize,
        height: usize,
        row: &Row<'_>,
        fill: Fill,
        fit: Fit,
    ) -> Vec<Vec<Draft>> {
        let content: Vec<Vec<Draft>> = match &col.content {
            Content::Groups { left, right } => {
                let group = Group { left, right, justify: col.justify, separator: row.separator };
                vec![self.compose_group(&group, width, fill, fit)]
            }
            Content::Stack(rows) => blocks(rows)
                .iter()
                .flat_map(|block| self.stack_block(block, width, fill, fit))
                .collect(),
        };
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
        let Some((interior, pad)) = self.box_interior(&chars, width) else {
            return (0..self.block_height(block))
                .map(|_| vec![Draft::Space(width, Elem::Pad)])
                .collect();
        };
        // `box = true` on a row is the one way to title a one-row box, so a
        // block's first row stands in when the box has no title of its own.
        let title = cfg.title.as_ref().or_else(|| block.rows.first().and_then(|(_, r)| r.title));
        let mut lines = vec![self.edge_drafts(&chars, width, true, title, &cfg)];
        for (_, inner) in &block.rows {
            let body =
                self.row_body(inner, interior, self.row_height(inner), Fill::from_box(&cfg), fit);
            lines.extend(body.into_iter().map(|d| self.side_drafts(d, &chars, &cfg, pad)));
        }
        lines.push(self.edge_drafts(&chars, width, false, None, &cfg));
        lines
    }

    /// One bare row of a stack: its line with its title set into it.
    fn stack_row(&self, inner: &Row<'_>, width: usize, fill: Fill, fit: Fit) -> Vec<Vec<Draft>> {
        let mut lines = self.row_body(inner, width, self.row_height(inner), fill, fit);
        // An inner row's title goes into its own first line.
        if let (Some(title), Some(first)) = (inner.title, lines.first_mut()) {
            self.place_title(first, title, fill);
        }
        // A blank inner row carries the cell that keeps its line on screen
        // (SPEC § 4.1): the outer row cannot add it for one of its columns.
        if inner.blank {
            for line in &mut lines {
                mark_blank(line);
            }
        }
        lines
    }
}

/// Composing one column's line from its groups.
impl Layout<'_> {
    /// `n` cells of whatever fills empty space in this mode.
    const fn filler(n: usize, fill: Fill, elem: Elem) -> Draft {
        match fill {
            Fill::Rule => Draft::Rule(n),
            Fill::Spaces | Fill::Packed => Draft::Space(n, elem),
        }
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
    fn group_pieces(&self, group: &[Vec<Segment>], separator: &str) -> Vec<Piece> {
        let modules = group.iter().map(|module| Piece { elem: Elem::Module, segs: module.clone() });
        if separator.is_empty() {
            return modules.collect();
        }
        let sep = Piece {
            elem: Elem::Separator,
            segs: vec![Segment::styled(separator, Style::fg(self.theme.role(Role::Muted)))],
        };
        Itertools::intersperse(modules, sep).collect()
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
        let segs: Vec<Segment> = pieces.iter().flat_map(|p| p.segs.iter().cloned()).collect();
        let segs = self.ticker.as_ref().filter(|_| scrolls).map_or_else(
            || truncate(&segs, budget, self.ellipsis),
            |ticker| {
                let period = segments_width(&segs).saturating_add(display_width(&ticker.gap));
                let offset = crate::time::frame(ticker.now, ticker.step, period);
                scroll(&segs, budget, offset, &ticker.gap, true)
            },
        );
        vec![Piece { elem: Elem::Group, segs }]
    }

    /// One column's line: the flex form when the column has a `right` group,
    /// a lone group placed by `justify` otherwise (SPEC § 4.3).
    ///
    /// `exact` is a column laid out to its own content (`width = "auto"`):
    /// it draws no rule, so its two groups join with the separator and
    /// nothing is cut to leave room for cells that are never drawn.
    fn compose_group(&self, group: &Group<'_>, width: usize, fill: Fill, fit: Fit) -> Vec<Draft> {
        // A column's own cells, once the pads that keep a neighbouring rule
        // off its text are taken off the ends its content reaches. A group
        // the rule already surrounds needs none.
        let (touch_left, touch_right) = group.touches(fit.exact);
        let (before, after) = fit.pads;
        let (before, after) =
            (if touch_left { before } else { 0 }, if touch_right { after } else { 0 });
        let inner = width.saturating_sub(before).saturating_sub(after);
        let mut drafts: Vec<Draft> = Vec::new();
        if before > 0 {
            drafts.push(Draft::Space(before, Elem::Pad));
        }
        drafts.extend(self.compose_cells(group, inner, fill, fit));
        if after > 0 {
            drafts.push(Draft::Space(after, Elem::Pad));
        }
        drafts
    }

    /// [`Self::compose_group`] inside the column's own cells.
    fn compose_cells(&self, group: &Group<'_>, width: usize, fill: Fill, fit: Fit) -> Vec<Draft> {
        let Group { left, right, justify, separator } = *group;
        let pad_w = display_width(&self.chars.pad);
        let cell = self.fill_cell(fill);
        // `truncate = false` lets the last column run past the box; every
        // other column is cut to its share, or it would spill into a
        // neighbour and move the whole row (SPEC § 4.3).
        let cut = self.truncate || !fit.trailing;
        let exact = fit.exact;
        // The right group is never *scrolled* (SPEC § 4.1), but it is cut
        // to its column like anything else: a group wider than the column
        // used to push the columns beside it off their shares.
        let right_pieces = self.fit_group(self.group_pieces(right, separator), width, cut, false);
        let right_w: usize = right_pieces.iter().map(|p| segments_width(&p.segs)).sum();

        if fill == Fill::Packed || exact {
            // Left-packed: the right group follows the left one after a
            // separator, and nothing fills the rest (SPEC § 4.1).
            let sep_w = if right_w == 0 { 0 } else { display_width(separator) };
            let budget = width.saturating_sub(right_w).saturating_sub(sep_w);
            // An `auto` column is its content, so it is only ever short of
            // room when the row clamped it; scrolling there would move a
            // column that is meant to hold still (SPEC § 4.3).
            let pieces = self.group_pieces(left, separator);
            let mut pieces = self.fit_group(pieces, budget, cut, !exact);
            if right_w > 0 {
                if !pieces.is_empty() && !separator.is_empty() {
                    pieces.push(Piece {
                        elem: Elem::Separator,
                        segs: vec![Segment::styled(
                            separator,
                            Style::fg(self.theme.role(Role::Muted)),
                        )],
                    });
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
            let left_pieces = self.group_pieces(left, separator);
            let has_left = !left_pieces.is_empty();
            let join = cell.saturating_add(if has_left { pad_w } else { 0 });
            let budget = width.saturating_sub(right_block).saturating_sub(join);
            let left_pieces = self.fit_group(left_pieces, budget, cut, true);
            let left_w: usize = left_pieces.iter().map(|p| segments_width(&p.segs)).sum();
            let left_pad = if left_pieces.is_empty() { 0 } else { pad_w };
            let rule =
                width.saturating_sub(left_w).saturating_sub(left_pad).saturating_sub(right_block);
            let mut drafts: Vec<Draft> = left_pieces.into_iter().map(Draft::Done).collect();
            if left_pad > 0 {
                drafts.push(Draft::spaces(Elem::Pad, left_pad));
            }
            drafts.push(Self::filler(rule, fill, Elem::Rule));
            drafts.push(Draft::spaces(Elem::Pad, pad_w));
            drafts.extend(right_pieces.into_iter().map(Draft::Done));
            return drafts;
        }

        // A lone group: the rule on one side, or both when it is centred.
        let sides = if justify == Justify::Center { 2 } else { 1 };
        let pieces = self.group_pieces(left, separator);
        let budget = width.saturating_sub(cell.saturating_add(pad_w).saturating_mul(sides));
        let pieces = self.fit_group(pieces, budget, cut, true);
        let text_w: usize = pieces.iter().map(|p| segments_width(&p.segs)).sum();
        let pad_w = if pieces.is_empty() { 0 } else { pad_w };
        let space = width.saturating_sub(text_w).saturating_sub(pad_w.saturating_mul(sides));
        let (before, after) = split(space, justify);
        let mut drafts: Vec<Draft> = Vec::new();
        if before > 0 || justify != Justify::Left {
            drafts.push(Self::filler(before, fill, Elem::Rule));
        }
        if pad_w > 0 && justify != Justify::Left {
            drafts.push(Draft::spaces(Elem::Pad, pad_w));
        }
        drafts.extend(pieces.into_iter().map(Draft::Done));
        if pad_w > 0 && justify != Justify::Right {
            drafts.push(Draft::spaces(Elem::Pad, pad_w));
        }
        drafts.push(Self::filler(after, fill, Elem::Rule));
        drafts
    }
}

/// One column's groups and how they sit, as [`Layout::compose_group`] reads
/// them.
#[derive(Debug, Clone, Copy)]
struct Group<'a> {
    left: &'a [Vec<Segment>],
    right: &'a [Vec<Segment>],
    justify: Justify,
    separator: &'a str,
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
    /// runs into its text. Only an `exact` column needs them: every other
    /// column's own composition pads its groups already.
    pads: (usize, usize),
    /// Nothing of the row is to this column's right, so `truncate = false`
    /// lets its content run past the box. Every other column is cut to its
    /// share whatever `truncate` says, or it would spill into a neighbour
    /// (SPEC § 4.3).
    trailing: bool,
}

impl Default for Fit {
    fn default() -> Self {
        Self { exact: false, pads: (0, 0), trailing: true }
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
        if inherited && matches!(style, FrameStyle::None | FrameStyle::Powerline) {
            style = FrameStyle::Rounded;
        }
        if style == FrameStyle::Powerline {
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
        let Some(title) = title.filter(|t| !t.text.is_empty()) else {
            return vec![rule(middle)];
        };
        let pad = title.pad.min(middle.checked_div(2).unwrap_or(0));
        let room = middle.saturating_sub(pad.saturating_mul(2));
        if room == 0 {
            return vec![rule(middle)];
        }
        let text_style = Style::fg(title.color.unwrap_or_else(|| self.theme.role(Role::Frame)));
        let text = truncate(&[Segment::styled(&title.text, text_style)], room, self.ellipsis);
        let spare = room.saturating_sub(segments_width(&text));
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
            out.push(Draft::spaces(Elem::Pad, pad));
        }
        out.push(Draft::Done(Piece { elem: Elem::Title, segs: text }));
        if pad > 0 {
            out.push(Draft::spaces(Elem::Pad, pad));
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
            out.push(Draft::spaces(Elem::Pad, pad));
        }
        out.extend(drafts);
        if pad > 0 {
            out.push(Draft::spaces(Elem::Pad, pad));
        }
        if !chars.side.is_empty() {
            out.push(Draft::Done(Piece {
                elem: Elem::BoxEdge,
                segs: vec![Segment::styled(&chars.side, style)],
            }));
        }
        out
    }

    /// A box's interior line, its sides coloured by the box.
    fn wrap_box(
        &self,
        drafts: Vec<Draft>,
        chars: &BoxChars,
        cfg: &BoxCfg,
        pad: usize,
        blank: bool,
    ) -> Line {
        self.paint(self.side_drafts(drafts, chars, cfg, pad), blank)
    }

    /// A box's top or bottom line as a finished line.
    fn edge_line(
        &self,
        chars: &BoxChars,
        top: bool,
        title: Option<&crate::config::TitleCfg>,
        cfg: &BoxCfg,
    ) -> Line {
        self.paint(self.edge_drafts(chars, self.width, top, title, cfg), false)
    }
}

/// Titles: plain text set into the rule of a row or a box (SPEC § 4.3).
impl Layout<'_> {
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
        let Some((at, cells)) = target.filter(|(_, n)| *n > 0) else {
            return;
        };
        let pad = title.pad.min(cells.checked_div(2).unwrap_or(0));
        let room = cells.saturating_sub(pad.saturating_mul(2));
        if room == 0 {
            return;
        }
        let style = Style::fg(title.color.unwrap_or_else(|| self.theme.role(Role::Frame)));
        let text = truncate(&[Segment::styled(&title.text, style)], room, self.ellipsis);
        let spare = room.saturating_sub(segments_width(&text));
        let (before, after) = split(spare, title.justify);
        // Right after the frame's cap there is already a pad, and doubling
        // it reads as a typo: `├─ Repository ──┤`, not `├─  Repository`.
        let capped = drafts.get(..at).is_some_and(|before| {
            before.iter().all(|d| {
                matches!(d, Draft::Done(p) if matches!(p.elem, Elem::Cap | Elem::Pad | Elem::BoxEdge))
            })
        });
        let pad_before = if capped && before == 0 { 0 } else { pad };
        // The cell that pad would have taken goes back to the rule, so the
        // title never changes the line's width.
        let after = after.saturating_add(pad.saturating_sub(pad_before));
        let mut replacement: Vec<Draft> = Vec::new();
        if before > 0 {
            replacement.push(Self::filler(before, fill, Elem::Rule));
        }
        if pad_before > 0 {
            replacement.push(Draft::spaces(Elem::Pad, pad_before));
        }
        replacement.push(Draft::Done(Piece { elem: Elem::Title, segs: text }));
        if pad > 0 {
            replacement.push(Draft::spaces(Elem::Pad, pad));
        }
        if after > 0 {
            replacement.push(Self::filler(after, fill, Elem::Rule));
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
                Draft::Done(_) | Draft::Space(_, _) => 0,
            })
            .sum();
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
                Draft::Space(_, _) | Draft::Rule(0) => {}
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

    /// One row line between the frame's caps.
    fn wrap_frame(
        &self,
        drafts: Vec<Draft>,
        index: usize,
        count: usize,
        row: &Row<'_>,
        fill: Fill,
        title: Option<&crate::config::TitleCfg>,
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
                line.push(Draft::spaces(Elem::Pad, pad));
            }
        }
        line.extend(drafts);
        // A boxed row's title belongs to the box, which draws it on its top
        // edge (SPEC § 4.3).
        if let Some(title) = title.filter(|_| row.boxed.is_none()) {
            self.place_title(&mut line, title, fill);
        }
        if fill.draws() && !cap.is_empty() {
            // The cap is padded from the content only: with no right group
            // the rule runs into it, as it always has. Any cell left over
            // (a later line of a tall row whose cap is narrower than the
            // first's) goes to the rule, so every line is the same width.
            let used: usize = line.iter().map(Draft::cells).sum();
            let padded = row_ends_in_content(row) && pad > 0;
            let cap_total = display_width(cap).saturating_add(if padded { pad } else { 0 });
            let spare = self.width.saturating_sub(used).saturating_sub(cap_total);
            if spare > 0 {
                line.push(Self::filler(spare, fill, Elem::Rule));
            }
            if padded {
                line.push(Draft::spaces(Elem::Pad, pad));
            }
            line.push(Draft::Done(Piece {
                elem: Elem::Cap,
                segs: vec![Segment::styled(cap, style)],
            }));
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

/// Turn the first one-cell space of a blank row into the braille cell that
/// keeps the line on screen (SPEC § 4.1). The width never changes, and the
/// config only lets a spacer ask for it, so there is nothing else on the
/// row to hide behind.
fn mark_blank(drafts: &mut [Draft]) {
    let at = drafts.iter().position(|d| matches!(d, Draft::Space(n, _) if *n > 0));
    let Some((at, Draft::Space(n, elem))) = at.and_then(|i| drafts.get(i).map(|d| (i, d.clone())))
    else {
        return;
    };
    let mut text = String::from(crate::render::BLANK_CELL);
    text.push_str(&" ".repeat(n.saturating_sub(1)));
    let piece = Draft::Done(Piece { elem, segs: vec![Segment::plain(text)] });
    if let Some(slot) = drafts.get_mut(at) {
        *slot = piece;
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

/// A `blank = true` row (SPEC § 4.1): when the line is whitespace only, its
/// first one-cell whitespace character becomes the braille blank so Claude
/// Code keeps the row; a line with a visible frame is left alone.
fn blank_line(line: Line) -> Line {
    let segs = line.segments();
    let kept = crate::render::keep_blank(segs.clone());
    if kept == segs { line } else { Line { pieces: vec![Piece { elem: Elem::Group, segs: kept }] } }
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
    }

    impl Fixture {
        fn new(style: FrameStyle, fill: bool, width: usize) -> Self {
            Self {
                chars: FrameChars::for_style(style),
                theme: Theme::default(),
                boxes: boxes(),
                style,
                fill,
                width,
                truncate: true,
                ticker: None,
                rule: None,
            }
        }

        fn layout(&self) -> Layout<'_> {
            Layout {
                chars: &self.chars,
                style: self.style,
                theme: &self.theme,
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
                    content: Content::Groups { left: &left, right: &right },
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
            let mut body = l.row_body(&row, inner, 1, fill, Fit::default());
            let drafts = body.pop().unwrap_or_default();
            let line = l.wrap_frame(drafts, index, count, &row, fill, None);
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
        assert_eq!(Rule { cells: vec!["ab".into()], offset: 5 }.paint(3), "ababab", "offset wraps");
        assert_eq!(Rule { cells: Vec::new(), offset: 0 }.paint(3), "", "no pattern, no rule text");
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
            content: Content::Groups { left, right: &[] },
        }
    }

    fn row(cols: Vec<Col<'static>>, gap: usize) -> Row<'static> {
        Row { cols, gap, separator: " │ ", title: None, boxed: None, blank: false }
    }

    /// SPEC § 4.3: the `fr` columns share the free width as
    /// `floor(free × n ÷ Σfr)` each, the leftover one cell each to the first
    /// of them, so the shares differ by at most one cell and always add up.
    #[test]
    fn column_shares_add_up_and_differ_by_at_most_one_cell() {
        let f = Fixture::new(FrameStyle::Rounded, true, 80);
        let l = f.layout();
        for n in 1..=6_usize {
            let r = row((0..n).map(|_| col(Width::Fr(1), "x")).collect(), 1);
            for width in 10..=400_usize {
                let widths = l.share(&r, width, Fill::Rule);
                let gaps = n.saturating_sub(1);
                let total: usize = widths.iter().sum::<usize>() + gaps;
                assert!(total <= width, "{n} columns at {width}: {widths:?}");
                assert_eq!(total, width, "the shares fill the width: {widths:?}");
                let (min, max) = (
                    widths.iter().min().copied().unwrap_or(0),
                    widths.iter().max().copied().unwrap_or(0),
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
            let inner = l.inner_width(&r, 0, l.row_height(&r));
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
            let top = lines.first().map(edges).unwrap_or_default();
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
        let r = row(vec![stack(3, VAlign::Top, None), col(Width::Fr(1), "one")], 1);
        assert_eq!(l.row_height(&r), 3);
        let r = row(vec![stack(2, VAlign::Top, Some(&BoxRef::Anon))], 1);
        assert_eq!(l.row_height(&r), 4, "a boxed column is its content plus two edge lines");
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
        assert_eq!(l.row_height(&row(vec![boxed_stack], 1)), 9);

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
    #[test]
    fn a_tall_row_under_uneven_custom_caps_fits_every_line() {
        for width in [24_usize, 40, 80] {
            let mut f = Fixture::new(FrameStyle::Custom, true, width);
            f.chars = FrameChars {
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
            let ends = [("<", ">"), ("<<<", ">>>"), ("<<<<<", ">>>>>")];
            for (line, (prefix, cap)) in l.lines(&rows).into_iter().flatten().zip(ends) {
                let text = Painter::PLAIN.paint(&line.segments());
                assert_eq!(line.width(), width, "{text:?}");
                assert!(text.starts_with(prefix) && text.ends_with(cap), "{text:?}");
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
        for want in [Elem::Cap, Elem::Pad, Elem::Module, Elem::Rule, Elem::Title, Elem::BoxEdge] {
            assert!(kinds.contains(&want), "{want:?} never drawn: {kinds:?}");
        }
    }
}
