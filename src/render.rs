//! The tick pipeline: payload + config → lines of styled text.

use std::path::Path;

use crate::ansi::{ColorMode, Painter, Segment, Style, segments_width};
use crate::config::{self, Config, Loaded, Overlay, StaleStyle};
use crate::frame::{BLANK_CELL, Ticker};
use crate::icons::IconSet;
use crate::modules::{self, Ctx, Freshness, Rendered, SCHEMAS, decorate};
use crate::payload::Payload;
use crate::theme::Role;

/// What a render needs from the outside world.
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    /// The JSON payload text from stdin.
    pub payload_json: &'a str,
    /// Explicit config path (`--config`).
    pub config_path: Option<&'a Path>,
    /// Command-line overrides (`preview --icons …`).
    pub overlay: Overlay,
    /// Terminal width (`COLUMNS`), when known.
    pub columns: Option<usize>,
    /// `NO_COLOR` is set and not empty ([`config::no_color_env`]).
    pub no_color: bool,
    /// Draw every row faint, as Claude Code draws the status line on screen
    /// (`preview`, SPEC § 2.1); the tick leaves that to the harness.
    pub dim: bool,
    /// Whether a cached module may look its entry up and spawn a worker:
    /// the tick does, `preview` never (SPEC § 14: a preview is not a
    /// tick, so it neither reads the cache nor forks; a cached module
    /// shows its not-yet-refreshed state).
    pub workers: bool,
}

/// Render a full tick. Never fails (SPEC § 5).
///
/// The one render that reads the process environment: the tick and
/// `preview` come through here, everything else names its [`Clock`].
#[must_use]
pub fn render(req: &Request<'_>) -> String {
    let payload = match Payload::parse(req.payload_json) {
        Ok(payload) => payload,
        Err(e) => {
            // The row stays the one SPEC § 5 pins; the parser's message,
            // which names the line, the column and what it expected, is what
            // a Claude Code release that changed the payload needs.
            let note = format!("garnish: bad payload: {e}");
            eprintln!("{note}");
            crate::debug::log(&note);
            return "⚠ garnish: bad payload\n".to_owned();
        }
    };
    let loaded = config::load_with(req.config_path, &SCHEMAS, &req.overlay);
    let config_file = loaded.path.as_deref().and_then(|p| std::path::absolute(p).ok());
    let clock = Clock { workers: req.workers, config_file, ..Clock::from_env() };
    render_loaded(&payload, &loaded, req.columns, req.no_color, req.dim, &clock)
}

/// Render with an already loaded config on `clock`: every row painted, then
/// the `⚠ config:` row when the config had problems. `dim` is
/// [`Request::dim`].
#[must_use]
pub fn render_loaded(
    payload: &Payload,
    loaded: &Loaded,
    columns: Option<usize>,
    no_color: bool,
    dim: bool,
    clock: &Clock,
) -> String {
    let config = &loaded.config;
    let mode = config.color.mode(no_color);
    let painter = Painter { mode, links: mode != ColorMode::Never, dim };
    let mut lines = render_lines_at(payload, config, columns, clock);
    if !loaded.errors.is_empty() {
        lines.push(config_warning(loaded, config.width(columns)));
    }
    let mut out = String::new();
    for line in &lines {
        out.push_str(&hold_leading_cells(painter.paint(line), mode));
        out.push('\n');
    }
    // Every row hid: one empty line, which the harness trims to nothing and
    // so clears the status line (SPEC § 5).
    if out.is_empty() {
        out.push('\n');
    }
    out
}

/// A painted row whose leading cells survive Claude Code's trim.
///
/// The harness trims every row's raw bytes before drawing it (SPEC § 2.1),
/// so a row that starts with whitespace (a column's padding line, a `none`
/// box's pad, the spaces that place a module under a frame with no caps)
/// would be drawn shifted left by those cells. With colour on, an empty SGR
/// in front keeps them: the trim meets a byte that is not whitespace, and
/// the harness parses the sequence away. With colour off, the first of them
/// becomes [`BLANK_CELL`], the trade-off `blank` makes (§ 4.1). A row that is
/// whitespace throughout is the spacer rule's, and is left as it is.
fn hold_leading_cells(row: String, mode: ColorMode) -> String {
    if !row.starts_with(char::is_whitespace) || row.chars().all(char::is_whitespace) {
        return row;
    }
    if mode != ColorMode::Never {
        return format!("\x1b[0m{row}");
    }
    let mut out = String::with_capacity(row.len().saturating_add(BLANK_CELL.len_utf8()));
    let mut held = false;
    for c in row.chars() {
        if held || !c.is_whitespace() {
            held = true;
            out.push(c);
            continue;
        }
        // A whitespace character that takes no cell (U+2028) is trimmed
        // with the rest and shows nothing: dropping it moves no cell.
        let cells = crate::ansi::display_width(c.encode_utf8(&mut [0; 4]));
        if cells > 0 {
            out.push(BLANK_CELL);
            out.extend(std::iter::repeat_n(' ', cells.saturating_sub(1)));
            held = true;
        }
    }
    out
}

/// The trailing `⚠ config: <path>:<line> <message>` line, truncated to the width.
fn config_warning(loaded: &Loaded, width: usize) -> Vec<Segment> {
    let config = &loaded.config;
    let first = loaded.errors.first();
    let line = first.and_then(|e| e.line);
    // `<path>:<line> `; with no file (a bad `--theme`, say) there is no path
    // to name, and a line alone reads `line N: `.
    let origin = match (&loaded.path, line) {
        (Some(p), Some(l)) => format!("{}:{l} ", p.display()),
        (Some(p), None) => format!("{} ", p.display()),
        (None, Some(l)) => format!("line {l}: "),
        (None, None) => String::new(),
    };
    let message = first.map_or_else(String::new, |e| {
        if e.path.is_empty() { e.message.clone() } else { format!("{}: {}", e.path, e.message) }
    });
    let suffix = crate::config::more(loaded.errors.len());
    let glyph = if config.icons == IconSet::Ascii { "!" } else { "⚠" };
    let line = vec![Segment::styled(
        format!("{glyph} config: {origin}{message}{suffix}"),
        Style::fg(config.theme.role(Role::Warn)).dimmed(),
    )];
    crate::ansi::truncate(&line, width, config.icons.ellipsis())
}

/// The environment-dependent inputs of a render, so docs and tests can pin them.
// Four independent switches (git, animate, settings, workers), each set on
// its own over `..Clock::fixed()`; an enum per pair would name nothing.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct Clock {
    /// The current instant.
    pub now: jiff::Timestamp,
    /// The local time zone.
    pub tz: jiff::tz::TimeZone,
    /// The home directory (for `~` collapsing).
    pub home: Option<String>,
    /// Claude Code's auto-compaction environment.
    pub settings_env: crate::claude_settings::Env,
    /// Whether repository discovery is allowed.
    pub git: bool,
    /// Whether animations advance with the clock; off freezes every frame
    /// index and text-module offset at 0 and cuts a ticker line with the
    /// ellipsis (`GARNISH_ANIMATE=0`, SPEC § 4.2).
    pub animate: bool,
    /// Whether Claude Code's settings chain may be read at all (the
    /// autocompact keys of SPEC § 2.3, `prefersReducedMotion` of § 4.2).
    /// Off for docs and goldens, which must not depend on the settings of
    /// the machine rendering them.
    pub settings: bool,
    /// The organisation's managed settings file, first in the chain: the
    /// platform path (or the `GARNISH_MANAGED_SETTINGS` hook's) for a real
    /// run, `None` for a pinned one (and for tests that must not see the
    /// machine's).
    pub managed: Option<std::path::PathBuf>,
    /// `CLAUDE_CONFIG_DIR`, which moves the user settings file off
    /// `~/.claude` (`claude_settings::user_dir`); `None` when
    /// unset, and for a pinned render.
    pub claude_config_dir: Option<std::path::PathBuf>,
    /// The cache root, or `None` to take it from the environment.
    ///
    /// The last thing a render read from the process environment on its own.
    /// A caller that must not touch the machine's cache (`benches/tick.rs`,
    /// which otherwise read and wrote the developer's real one and forked a
    /// worker per miss) names its own here.
    pub cache: Option<std::path::PathBuf>,
    /// Whether a cached module may look its entry up and spawn a worker.
    /// Off under the pinned clock, so docs, goldens and the in-process
    /// matrices never touch a cache directory (SPEC § 9); `git = false`
    /// alone covers only the repo group.
    pub workers: bool,
    /// Keys standing in for the settings chain: a pinned render that must
    /// show a settings-derived module on (the docs samples of `sandbox`
    /// and `voice`, SPEC § 3.8) seeds them here and still reads no file.
    /// `None` reads the chain, or nothing under `settings = false`.
    pub settings_keys: Option<Vec<crate::claude_settings::FileKeys>>,
    /// The config file this render loaded, absolute, handed to the workers
    /// it spawns (SPEC § 6); `None` when it loaded none.
    pub config_file: Option<std::path::PathBuf>,
}

impl Clock {
    /// From `GARNISH_NOW`, `TZ`/`/etc/localtime`, `HOME`, `GARNISH_ANIMATE`,
    /// `GARNISH_MANAGED_SETTINGS` and the process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            now: crate::time::now(),
            tz: crate::time::local_zone(),
            home: crate::claude_settings::home_dir().map(|h| h.display().to_string()),
            settings_env: crate::claude_settings::Env::from_process(),
            git: true,
            animate: crate::time::animate_from_env(),
            settings: true,
            managed: crate::claude_settings::managed_settings_path(),
            claude_config_dir: crate::claude_settings::config_dir_from_env(),
            cache: None,
            workers: true,
            settings_keys: None,
            config_file: None,
        }
    }

    /// A fixed clock: 2025-02-01T16:00:00Z, UTC, home `/home/dev`, no
    /// auto-compaction overrides, no repository discovery, no settings
    /// files, no workers and animations frozen at frame 0 — what the
    /// generated docs use, so they come out identical on every machine.
    #[must_use]
    pub fn fixed() -> Self {
        Self {
            now: jiff::Timestamp::from_second(1_738_425_600).unwrap_or_default(),
            tz: jiff::tz::TimeZone::UTC,
            home: Some("/home/dev".to_owned()),
            settings_env: crate::claude_settings::Env::default(),
            git: false,
            animate: false,
            settings: false,
            managed: None,
            claude_config_dir: None,
            cache: None,
            workers: false,
            settings_keys: None,
            config_file: None,
        }
    }

    /// The settings files a render of `payload` may read, highest
    /// precedence first and labelled as `doctor` labels them: the managed
    /// file, the chain of the directory Claude Code was launched in, the
    /// user's; none under a pinned clock.
    #[must_use]
    pub fn settings_chain(&self, payload: &Payload) -> Vec<(&'static str, std::path::PathBuf)> {
        if !self.settings {
            return Vec::new();
        }
        let project = payload.project_dir().map(Path::new);
        let user = crate::claude_settings::user_dir_in(
            self.claude_config_dir.as_deref(),
            self.home.as_deref().map(Path::new),
        );
        crate::claude_settings::settings_chain(self.managed.as_deref(), project, user.as_deref())
    }
}

/// Every configured row rendered to segments (no escape sequences yet), its
/// lines in order.
#[must_use]
pub fn render_lines_at(
    payload: &Payload,
    config: &Config,
    columns: Option<usize>,
    clock: &Clock,
) -> Vec<Vec<Segment>> {
    render_tree_at(payload, config, columns, clock)
        .into_iter()
        .flat_map(|(_, lines)| lines)
        .map(crate::layout::Line::into_segments)
        .collect()
}

/// Every row that survives `hide_empty_rows`, as its terminal lines tagged
/// by the index of its `[[row]]` (SPEC § 4.3).
///
/// A row is several lines once it carries a stack or a box, and each line
/// says what its pieces are; with the dropped rows gone, the index is what
/// tells `setup`'s row list and placement map which configured row a line
/// belongs to (SPEC § 14).
#[must_use]
pub fn render_tree_at(
    payload: &Payload,
    config: &Config,
    columns: Option<usize>,
    clock: &Clock,
) -> Vec<(usize, Vec<crate::layout::Line>)> {
    let width = config.width(columns);
    let cache =
        clock.cache.clone().map_or_else(crate::cache::Cache::from_env, crate::cache::Cache::at);
    let ctx = context(payload, config, clock, &cache, width);
    let ellipsis: String = config.icons.ellipsis().into();
    let layout = crate::layout::Layout {
        chars: &config.frame.chars,
        style: config.frame.style,
        theme: &config.theme,
        separator_color: &config.frame.separator_color,
        fill: config.frame.fill,
        width,
        truncate: config.truncate,
        ellipsis: &ellipsis,
        // The effective animation switch is decided once, on `ctx`; with it
        // off there is no ticker and an over-wide line is cut (SPEC § 4.2).
        ticker: (config.overflow == config::Overflow::Ticker && ctx.animate).then(|| Ticker {
            step: config.ticker_step,
            gap: config.ticker_gap.clone(),
            now: clock.now,
        }),
        rule: rule_pattern(config, &ctx),
        boxes: &config.boxes,
    };
    let frame = ctx.frame(config.frame.separator_step, config.frame.separator_frames.len());
    // Every module renders before anything is laid out: aligned columns and
    // `auto` widths need the widths of every row.
    let mut tree: Vec<RowRender<'_>> = config
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let sep = config.separator_at(row, frame);
            RowRender { index, ..render_row(&ctx, config, row, sep, &ellipsis) }
        })
        .collect();
    if config.align {
        align_tree(&mut tree, config);
    }
    // A row whose modules all rendered nothing is dropped unless it is an
    // intentional spacer or `hide_empty_rows = false`; an inner row goes the
    // same way, and its stack shortens (SPEC § 4.1, § 4.3).
    if config.hide_empty_rows {
        for row in &mut tree {
            for col in &mut row.cols {
                col.rows.retain(|inner| inner.cfg.spacer || !inner.is_empty());
            }
        }
        tree.retain(|row| row.cfg.spacer || !row.is_empty());
    }
    let rows: Vec<crate::layout::Row<'_>> = tree.iter().map(RowRender::to_layout).collect();
    tree.iter().map(|r| r.index).zip(layout.lines(&rows)).collect()
}

/// The context a render of `config` on `clock` hands every module, its
/// cached modules looking in `cache`.
///
/// Built in one place so that the tick and the per-module benchmarks
/// (`benches/tick.rs`) render modules in the same context.
#[must_use]
pub fn context<'a>(
    payload: &'a Payload,
    config: &'a Config,
    clock: &Clock,
    cache: &'a crate::cache::Cache,
    width: usize,
) -> Ctx<'a> {
    let mut ctx = Ctx {
        payload,
        theme: &config.theme,
        icons: config.icons,
        now: clock.now,
        width,
        cache,
        tz: clock.tz.clone(),
        home: clock.home.clone(),
        settings_env: clock.settings_env.clone(),
        git: clock.git,
        stale_after: config.stale_after,
        durations: config.durations,
        format: config.format,
        animate: false,
        dirs: std::cell::OnceCell::new(),
        head: std::cell::OnceCell::new(),
        settings_chain: clock.settings_chain(payload),
        settings: clock
            .settings_keys
            .clone()
            .map_or_else(std::cell::OnceCell::new, std::cell::OnceCell::from),
        // A refused root (SPEC § 6) is no cache at all: render as a pinned
        // tick does, never spawning a worker that could not write.
        workers: clock.workers && cache.refused().is_none(),
        config_file: clock.config_file.clone(),
    };
    // SPEC § 4.2, strongest first: `GARNISH_ANIMATE=0` freezes, an explicit
    // `animate` decides, else Claude Code's prefersReducedMotion freezes,
    // else animations run. The chain is read only when the answer depends
    // on it, and once for the tick (the context module shares the keys).
    ctx.animate = clock.animate
        && config
            .animate
            .unwrap_or_else(|| !crate::claude_settings::reduced_motion(ctx.settings()));
    ctx
}

/// One configured row with every module of it rendered, before any width is
/// decided: alignment and `auto` columns both need the whole tree first.
struct RowRender<'a> {
    /// The row's index in `config.rows` (0 for an inner row, which is
    /// addressed through its column).
    index: usize,
    cfg: &'a config::RowCfg,
    separator: &'a str,
    cols: Vec<ColRender<'a>>,
}

/// One column of a [`RowRender`].
struct ColRender<'a> {
    cfg: &'a config::ColCfg,
    /// The column's justification, with an inner row following its column
    /// unless it set one of its own (SPEC § 4.3).
    justify: config::Justify,
    left: Vec<Vec<Segment>>,
    right: Vec<Vec<Segment>>,
    /// The id behind each entry of `left` and `right` (SPEC § 14).
    left_ids: Vec<String>,
    right_ids: Vec<String>,
    rows: Vec<RowRender<'a>>,
}

impl<'a> RowRender<'a> {
    fn is_empty(&self) -> bool {
        self.cols.iter().all(ColRender::is_empty)
    }

    fn to_layout(&'a self) -> crate::layout::Row<'a> {
        crate::layout::Row {
            cols: self.cols.iter().map(ColRender::to_layout).collect(),
            gap: self.cfg.gap,
            separator: self.separator,
            title: self.cfg.title.as_ref(),
            boxed: self.cfg.boxed.as_ref(),
            blank: self.cfg.blank,
        }
    }
}

impl<'a> ColRender<'a> {
    const fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty() && self.rows.is_empty()
    }

    fn to_layout(&'a self) -> crate::layout::Col<'a> {
        // A stack stays a stack once `hide_empty_rows` has emptied it, so
        // its share renders as the empty lines of a stack (spaces) rather
        // than as an empty flex column (a rule): the two look different, and
        // the column's neighbours have not changed (SPEC § 4.3).
        let content = if self.cfg.rows.is_empty() {
            crate::layout::Content::Groups {
                left: &self.left,
                right: &self.right,
                left_ids: &self.left_ids,
                right_ids: &self.right_ids,
            }
        } else {
            crate::layout::Content::Stack(self.rows.iter().map(RowRender::to_layout).collect())
        };
        crate::layout::Col {
            width: self.cfg.width,
            justify: self.justify,
            valign: self.cfg.valign,
            boxed: self.cfg.boxed.as_ref(),
            content,
        }
    }
}

/// Render every module of one row, its stacks included.
fn render_row<'a>(
    ctx: &Ctx<'_>,
    config: &'a Config,
    row: &'a config::RowCfg,
    separator: &'a str,
    ellipsis: &str,
) -> RowRender<'a> {
    let cols = row
        .cols
        .iter()
        .map(|col| {
            let rows = col
                .rows
                .iter()
                .map(|inner| {
                    // An inner row's own separator wins over the outer row's,
                    // as a row's wins over the frame's (SPEC § 4.3).
                    let sep = inner.separator.as_deref().unwrap_or(separator);
                    let mut rendered = render_row(ctx, config, inner, sep, ellipsis);
                    // …and its own `justify` wins over the column's, which is
                    // what places the stack when the inner row says nothing.
                    for c in &mut rendered.cols {
                        if !c.cfg.justify_set {
                            c.justify = col.justify;
                        }
                    }
                    rendered
                })
                .collect();
            let (left_ids, left) = render_group(ctx, config, &col.left, ellipsis);
            let (right_ids, right) = render_group(ctx, config, &col.right, ellipsis);
            ColRender { cfg: col, justify: col.justify, left, right, left_ids, right_ids, rows }
        })
        .collect();
    RowRender { index: 0, cfg: row, separator, cols }
}

/// `align = true` (SPEC § 4.3): module *k* of a column is padded to the
/// widest module *k* of the columns in the same position, among the rows
/// with the same column count. Inner rows align with the inner rows at the
/// same position, never with the rows around them, and a column whose
/// left group hangs off its right end, which counts *k* from there, only
/// with other such columns.
fn align_tree(tree: &mut [RowRender<'_>], config: &Config) {
    type Key = (usize, usize, usize, bool);
    let mut buckets: std::collections::BTreeMap<Key, Vec<&mut ColRender<'_>>> =
        std::collections::BTreeMap::new();
    for row in tree.iter_mut() {
        let n = row.cols.len();
        for (j, col) in row.cols.iter_mut().enumerate() {
            if col.rows.is_empty() {
                buckets.entry((n, j, 0, hangs_right(col))).or_default().push(col);
            } else {
                for inner in &mut col.rows {
                    for c in &mut inner.cols {
                        buckets.entry((n, j, 1, hangs_right(c))).or_default().push(c);
                    }
                }
            }
        }
    }
    for cols in buckets.into_values() {
        align_bucket(cols, config);
    }
}

/// Whether a column's left group is anchored to its right end: a
/// right-justified lone group is. A column with a `right` group is drawn
/// in the flex form, its left group anchored left whatever `justify` says
/// (SPEC § 4.3).
fn hangs_right(col: &ColRender<'_>) -> bool {
    col.justify == config::Justify::Right && col.right.is_empty()
}

/// One bucket of columns aligned against each other.
fn align_bucket(mut cols: Vec<&mut ColRender<'_>>, config: &Config) {
    let take = |cols: &mut Vec<&mut ColRender<'_>>, right: bool| -> Vec<Vec<Vec<Segment>>> {
        cols.iter_mut()
            .map(|c| std::mem::take(if right { &mut c.right } else { &mut c.left }))
            .collect()
    };
    // A right-justified lone group hangs off the right edge, so its
    // positions count from the right end as a `right` group's do, and
    // `right_justify` picks the pad side for both (SPEC § 4, § 4.3). Every
    // column of a bucket hangs right or none does (`align_tree`).
    let from_right = cols.first().is_some_and(|c| hangs_right(c));
    let pad_left = config.right_justify == config::RightJustify::End;
    if config.frame.fill {
        let mut lefts = take(&mut cols, false);
        align_columns(&mut lefts, from_right, from_right && pad_left);
        let mut rights = take(&mut cols, true);
        align_columns(&mut rights, true, pad_left);
        for ((col, left), right) in cols.iter_mut().zip(lefts).zip(rights) {
            col.left = left;
            col.right = right;
        }
    } else {
        // Left-packed, the right group follows the left one after a
        // separator, so the column is one sequence of modules (SPEC § 4).
        let lefts = take(&mut cols, false);
        let rights = take(&mut cols, true);
        let split: Vec<usize> = lefts.iter().map(Vec::len).collect();
        let mut rows: Vec<Vec<Vec<Segment>>> = lefts
            .into_iter()
            .zip(rights)
            .map(|(mut l, mut r)| {
                l.append(&mut r);
                l
            })
            .collect();
        align_columns(&mut rows, false, false);
        for ((col, mut row), n) in cols.iter_mut().zip(rows).zip(split) {
            col.right = row.split_off(n.min(row.len()));
            col.left = row;
        }
    }
}

/// The rule pattern at this tick, if the frame has one (SPEC § 4.2): the
/// pattern index drawn in the rule's first cell advances `fill_step` per
/// tick, so a `right` pattern appears to travel toward the right cap and a
/// `left` one toward the left.
fn rule_pattern(config: &Config, ctx: &Ctx<'_>) -> Option<crate::frame::Rule> {
    let cells = &config.frame.fill_pattern;
    if cells.is_empty() || !config.frame.fill {
        return None;
    }
    let n = cells.len();
    let frame = ctx.frame(config.frame.fill_step, n);
    let offset = match config.frame.fill_direction {
        config::FillDirection::Left => frame,
        config::FillDirection::Right => n.saturating_sub(frame).checked_rem(n).unwrap_or(0),
    };
    Some(crate::frame::Rule { cells: cells.clone(), offset })
}

/// Pad module `k` of every group to the widest module `k` among the groups
/// that have a module after it, so the separators after it fall on the
/// same cell in every line (SPEC § 4). Left groups count from the left;
/// right groups (`from_right`) count from the right end. A group's last
/// module is never padded. `pad_left` puts the pad before the text (right
/// groups with `right_justify = "end"`, so the text hugs the cap); otherwise
/// it goes after (left groups, and right groups with `start`).
fn align_columns(groups: &mut [Vec<Vec<Segment>>], from_right: bool, pad_left: bool) {
    let columns = groups.iter().map(Vec::len).max().unwrap_or(0);
    for k in 0..columns {
        let padded = |g: &Vec<Vec<Segment>>| g.len() > k.saturating_add(1);
        let target = groups
            .iter()
            .filter(|g| padded(g))
            .filter_map(|g| if from_right { g.iter().rev().nth(k) } else { g.get(k) })
            .map(|m| segments_width(m))
            .max();
        let Some(target) = target else { continue };
        for g in groups.iter_mut().filter(|g| padded(g)) {
            let module = if from_right { g.iter_mut().rev().nth(k) } else { g.get_mut(k) };
            let Some(module) = module else { continue };
            let gap = target.saturating_sub(segments_width(module));
            if gap == 0 {
                continue;
            }
            let pad = Segment::plain(" ".repeat(gap));
            if pad_left {
                module.insert(0, pad);
            } else {
                module.push(pad);
            }
        }
    }
}

/// Every module of a group rendered and decorated, each cut to its
/// `max_width` (SPEC § 3), with the ids behind them in step; a module that
/// rendered nothing is left out of both.
fn render_group(
    ctx: &Ctx<'_>,
    config: &Config,
    ids: &[String],
    ellipsis: &str,
) -> (Vec<String>, Vec<Vec<Segment>>) {
    ids.iter()
        .filter_map(|id| {
            // `text.<name>` comes from the config, not the fixed registry
            // (SPEC § 3.7); it has `width`, never a `max_width` to cap.
            if let Some(name) = id.strip_prefix(modules::text::PREFIX) {
                let cfg = config.texts.get(name).filter(|c| c.enabled)?;
                let rendered = modules::text::render(ctx, cfg);
                return Some((id.clone(), decorate(rendered, cfg, &config.theme, config.icons)));
            }
            let entry = modules::entry(id)?;
            let cfg = config.modules.get(entry.schema.id)?;
            if !cfg.enabled {
                return None;
            }
            // Icons with `<key>_frames` show this tick's frame (SPEC § 4.2).
            let view = cfg.animated(|n| ctx.frame(1.0, n));
            let rendered = entry.module.render(ctx, &view);
            let rendered = match (config.stale_style, &rendered.freshness) {
                (StaleStyle::Hide, Freshness::Stale | Freshness::Failed) => Rendered::empty(),
                (StaleStyle::Plain, _) => Rendered { freshness: Freshness::Fresh, ..rendered },
                _ => rendered,
            };
            // The `hide` list (SPEC § 3) reads the measure the module
            // attached; a hidden module rendered nothing, never a `–`. It
            // runs after the stale mapping on purpose: `sync` measures the
            // counts of its cache entry, and under `stale_style = "hide"` an
            // overdue one is already empty, measure and all, so it follows
            // `hide_when_empty` like any empty render rather than `zero`.
            if modules::hidden_by(&rendered, &cfg.hide) {
                return None;
            }
            let module = decorate(rendered, cfg, &config.theme, config.icons);
            Some((id.clone(), cap_width(module, cfg.max_width, ellipsis)))
        })
        // A module that rendered nothing is not a column (SPEC § 4).
        .filter(|(_, module)| !module.is_empty())
        .unzip()
}

/// `max_width` (SPEC § 3): the decorated module cut to `max` cells with the
/// ellipsis, before alignment and before the line is cut, so one long value
/// cannot push the rest of its line off. `0` leaves the module alone without
/// measuring it, so the default tick pays nothing. [`crate::ansi::truncate`]
/// keeps each segment's link, so a cut link is still opened and closed
/// around its remaining text when painted.
fn cap_width(module: Vec<Segment>, max: usize, ellipsis: &str) -> Vec<Segment> {
    if max == 0 || segments_width(&module) <= max {
        module
    } else {
        crate::ansi::truncate(&module, max, ellipsis)
    }
}

/// Plain-text render of the configured lines with a pinned clock (docs).
#[must_use]
pub fn render_plain_at(
    payload: &Payload,
    config: &Config,
    columns: Option<usize>,
    clock: &Clock,
) -> String {
    render_lines_at(payload, config, columns, clock)
        .iter()
        .map(|line| Painter::PLAIN.paint(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::{display_width, strip_ansi};

    /// A whole render as `render` paints it, `⚠ config:` row included, on the
    /// pinned clock: no git, no settings files, no cache, no workers.
    fn render_plain(payload: &Payload, loaded: &Loaded, columns: Option<usize>) -> String {
        strip_ansi(&render_loaded(payload, loaded, columns, true, false, &Clock::fixed()))
    }

    fn fixture(name: &str) -> Payload {
        let path = format!("{}/tests/fixtures/payloads/{name}.json", env!("CARGO_MANIFEST_DIR"));
        Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn loaded(text: &str) -> Loaded {
        let (config, errors) = config::parse(text, &SCHEMAS);
        Loaded { config, path: None, errors }
    }

    #[test]
    fn default_render_has_four_lines_at_exact_width() {
        let out = render_plain(&fixture("subscription-full"), &loaded(""), Some(100));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "{out}");
        // COLUMNS=100 leaves 96 cells inside Claude Code's box (SPEC § 2.1).
        for l in &lines {
            assert_eq!(display_width(l), 96, "{l}");
        }
        assert!(lines[0].starts_with("╭─"));
        assert!(lines[3].starts_with("╰─"));
        assert!(out.contains("Opus"), "{out}");
        assert!(out.contains("42%"), "{out}");
    }

    #[test]
    fn bad_payload_and_bad_config_never_go_silent() {
        assert_eq!(
            render(&Request { payload_json: "{", ..Default::default() }),
            "⚠ garnish: bad payload\n"
        );
        let out = render_plain(&fixture("api-key"), &loaded("theme = \"nope\""), Some(80));
        assert!(out.lines().last().unwrap().starts_with("⚠ config: theme: unknown theme"), "{out}");
        assert!(out.lines().count() >= 2);
        // syntax errors carry the line number and the warning never overflows the width
        let out =
            render_plain(&fixture("api-key"), &loaded("preset = \"default\"\n[frame\nx"), Some(40));
        let last = out.lines().last().unwrap();
        assert!(last.starts_with("⚠ config: line 2: "), "{last}");
        assert!(display_width(last) <= 36, "COLUMNS=40 leaves 36 cells: {last}");
        assert!(last.ends_with('…'), "{last}");
        let out = render_plain(
            &fixture("api-key"),
            &loaded("icons = \"ascii\"\ntheme = \"nope\""),
            Some(80),
        );
        assert!(out.lines().last().unwrap().starts_with("! config:"), "{out}");
    }

    /// SPEC § 4.1 Empty lines: outside a repository the repo modules render
    /// nothing, so a line made only of them is dropped and the caps follow
    /// the survivors; `modules = []` is a spacer that always stays;
    /// `hide_empty_lines = false` keeps the accidental empty row too.
    #[test]
    fn empty_lines_are_dropped_unless_spacer_or_kept_by_config() {
        let payload = fixture("pr-absent");
        let plain = |text: &str| {
            let (config, errs) = config::parse(&format!("icons = \"unicode\"\n{text}"), &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            strip_ansi(&render_plain_at(&payload, &config, Some(60), &Clock::fixed()))
        };
        let lines = "[[line]]\nmodules = [\"branch\", \"sync\", \"pr\"]\n[[line]]\nmodules = [\"model\"]\nright = [\"clock\"]\n";
        let out = plain(lines);
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(
            out.starts_with("── ❖ Opus ") && out.ends_with("⠋ 16:00:00 ──"),
            "single caps: {out}"
        );
        let out = plain(&format!("hide_empty_lines = false\n{lines}"));
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows.len(), 2, "{out}");
        assert!(rows[0].starts_with("╭─") && rows[0].ends_with("─╮"), "{}", rows[0]);
        assert_eq!(display_width(rows[0]), 56);
        assert_eq!(rows[0].trim_matches(|c| c == '╭' || c == '╮' || c == '─' || c == ' '), "");
        let out = plain(
            "[[line]]\nmodules = [\"model\"]\n[[line]]\nmodules = []\n[[line]]\nmodules = [\"session\"]\n",
        );
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows.len(), 3, "spacer kept: {out}");
        assert!(rows[1].starts_with("├─") && rows[1].ends_with("┤"), "{}", rows[1]);
        assert_eq!(display_width(rows[1]), 56);
    }

    /// SPEC § 5: the payload's own strings (session name, model, agent,
    /// output style, directories, PR URL) never add a row and never put an
    /// escape of their own on one.
    #[test]
    fn hostile_payload_strings_never_add_a_row_or_an_escape() {
        // Whole-stack review: those strings reached the row raw. A `\n`
        // split the frame, an escape passed `--color never`, a cut could
        // split the sequence. Segments are plain by construction.
        // Built with serde so the escapes arrive as JSON escapes, the way
        // any serializer emits them (a raw ESC byte is not valid JSON).
        let dir = "/home/dev/pro\x1b[2Jjects/de\u{202e}mo";
        let json = serde_json::json!({
            "session_id": "s",
            "session_name": "s\x1b]8;;http://evil\x1b\\link\nX",
            "cwd": dir,
            "model": {"id": "m", "display_name": "\x1b[31mEvil\x1b[0m\nrow"},
            "output_style": {"name": "style"},
            "agent": {"name": "ag\nent"},
            "workspace": {"current_dir": dir, "project_dir": dir},
            "cost": {"total_cost_usd": 1.0, "total_duration_ms": 1000, "total_api_duration_ms": 100,
                     "total_lines_added": 1, "total_lines_removed": 0},
            "context_window": {"context_window_size": 200_000, "used_percentage": 10,
                               "remaining_percentage": 90, "total_input_tokens": 1, "total_output_tokens": 1},
            "pr": {"number": 7, "url": "javascript:alert(1)\x1b\\\x1b[31mINJECT", "review_state": "weird"}
        })
        .to_string();
        assert!(!json.contains('\x1b'), "escaped on the wire");
        let payload = Payload::parse(&json).unwrap();
        let loaded = loaded("preset = \"full\"\ncolor = \"always\"\n[modules.pr]\nlink = true\n");
        let out = render_loaded(&payload, &loaded, Some(160), false, false, &Clock::fixed());
        let plain = render_plain(&payload, &loaded, Some(160));
        assert_eq!(plain.lines().count(), loaded.config.rows.len(), "{plain}");
        assert!(plain.contains("Evilrow") && plain.contains("slink"), "{plain}");
        assert!(plain.contains("projects/demo") && plain.contains("agent"), "{plain}");
        // The only escapes are the painter's own SGR: no OSC 8 for a bad
        // URL, no OSC/CSI/DCS/BEL from the payload, no bidi override.
        assert!(
            !out.contains("\x1b]") && !out.contains('\u{7}') && !out.contains("\x1bP"),
            "{out:?}"
        );
        assert!(!out.contains("[2J") && !out.contains('\u{202e}'), "{out:?}");
        for seq in out.split('\x1b').skip(1) {
            assert!(seq.starts_with('[') && seq.contains('m'), "{seq:?} is not SGR");
        }
    }

    /// SPEC § 3.4: the clock's `tz` is read the way `TZ` is, a POSIX rule
    /// included; one that names nothing is the tick's zone.
    #[test]
    fn the_clock_tz_reads_like_tz() {
        let payload = Payload::parse("{\"session_id\": \"s\"}").unwrap();
        let clock_at = |tz: &str| {
            let text = format!(
                "[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"clock\"]\n[modules.clock]\nspinner = false\ntz = {tz:?}\n"
            );
            let (config, errs) = config::parse(&text, &SCHEMAS);
            // One that names no zone is also reported (cfg-12).
            assert_eq!(errs.is_empty(), tz != "Not/AZone", "{errs:?}");
            render_plain_at(&payload, &config, Some(40), &Clock::fixed()).trim().to_owned()
        };
        assert_eq!(clock_at("JST-9"), "01:00:00");
        assert_eq!(clock_at("<-0330>3:30"), "12:30:00");
        assert_eq!(clock_at("Not/AZone"), "16:00:00");
        assert_eq!(clock_at(""), "16:00:00");
    }

    /// SPEC § 5: `max_length` counts the text the row shows. A bold session
    /// name lost cells to the escapes' bytes, and a cut inside a sequence
    /// left it open, so the row's plain-text pass swallowed the ellipsis.
    #[test]
    fn max_length_counts_the_text_the_row_shows() {
        let row = |name: &str, table: &str| {
            let json = serde_json::json!({"session_id": "s", "session_name": name}).to_string();
            let text = format!(
                "[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"session_name\"]\n[modules.session_name]\n{table}"
            );
            render_plain(&Payload::parse(&json).unwrap(), &loaded(&text), Some(80))
        };
        let bold = format!("\x1b[1m{}\x1b[0m", "a".repeat(30));
        let out = row(&bold, "");
        assert!(out.contains(&"a".repeat(30)) && !out.contains('…'), "{out}");
        let out = row("ab\x1b[31mcdefgh", "max_length = 5\n");
        assert!(out.contains("abcd…"), "{out}");
        let out = row("\x1b]0;title\x07abcdefgh", "max_length = 5\n");
        assert!(out.contains("abcd…"), "{out}");
    }

    #[test]
    fn pathological_sizes_render_inside_the_box() {
        // Whole-stack review: `width = i64::MAX` aborted the tick with an
        // allocation failure and a giant bar spun forever. Sizes are capped
        // at config time (reported) and clamped again when rendering.
        let payload = fixture("subscription-full");
        let cfg = "[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"text.a\", \"context\"]\n[modules.text.a]\ntext = \"hi\"\nwidth = 9223372036854775807\npad = 4000000000\n[modules.context]\nwidth = 99999999999\n";
        let (config, errs) = config::parse(cfg, &SCHEMAS);
        assert_eq!(errs.len(), 3, "{errs:?}");
        let out = render_plain_at(&payload, &config, Some(80), &Clock::fixed());
        assert!(out.lines().all(|row| display_width(row) <= 76), "{out}");
        assert_eq!(config::Config::defaults(&SCHEMAS).width(Some(usize::MAX)), config::MAX_WIDTH);
        assert_eq!(config::Config::defaults(&SCHEMAS).width(Some(4100)), config::MAX_WIDTH);
    }

    #[test]
    fn a_one_cell_ascii_box_still_shows_its_clip_mark() {
        // `..` did not fit a one-cell box, so the module vanished and the
        // "always pad + box + pad cells" promise broke (whole-stack review).
        let payload = fixture("subscription-full");
        let cfg = "icons = \"ascii\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"text.a\", \"text.b\", \"clock\"]\n[modules.text.a]\ntext = \"hello\"\nwidth = 1\noverflow = \"clip\"\n[modules.text.b]\ntext = \"hello\"\nwidth = 2\noverflow = \"clip\"\n";
        let (config, errs) = config::parse(cfg, &SCHEMAS);
        assert_eq!(errs, Vec::new());
        let out = render_plain_at(&payload, &config, Some(80), &Clock::fixed());
        assert!(out.starts_with(".  ..  "), "{out:?}");
    }

    /// SPEC § 3 `max_width`: the decorated module is cut to N cells with the
    /// ellipsis before the columns are aligned, a cut link is still opened
    /// and closed around what is left of its text when painted, and
    /// `branch.max_length` keeps cutting the name by characters first.
    #[test]
    fn max_width_caps_a_module_before_alignment_and_keeps_links_balanced() {
        let payload = fixture("worktree-session");
        let base =
            "icons = \"unicode\"\ncolor = \"always\"\n[frame]\nstyle = \"none\"\nfill = false\n";
        let render = |extra: &str| {
            let (config, errs) = config::parse(&format!("{base}{extra}"), &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            render_lines_at(&payload, &config, Some(80), &Clock::fixed())
        };
        // A linked `pr` cut inside its number: one OSC 8 open, one close,
        // around the `#` that survived; the ellipsis sits outside the link.
        let lines = render("[[line]]\nmodules = [\"pr\"]\n[modules.pr]\nmax_width = 4\n");
        let painter = Painter { mode: ColorMode::TrueColor, links: true, dim: false };
        let out = painter.paint(&lines[0]);
        assert_eq!(strip_ansi(&out), "⇄ #…");
        let parts: Vec<&str> = out.split("\x1b]8;;").collect();
        assert_eq!(parts.len(), 3, "one open and one close: {out:?}");
        assert!(
            parts[1].starts_with("https://github.com/dschwartz/garnish/pull/42\x1b\\"),
            "{out:?}"
        );
        assert!(parts[1].ends_with("#\x1b[0m"), "the link closes right after the `#`: {out:?}");
        assert!(parts[2].starts_with("\x1b\\") && parts[2].contains('…'), "{out:?}");
        // A wide branch name: `max_width` counts cells of the whole module
        // (icon and space included), `max_length` characters of the name.
        let plain = |lines: &[Vec<Segment>]| Painter::PLAIN.paint(&lines[0]);
        let out =
            plain(&render("[[line]]\nmodules = [\"branch\"]\n[modules.branch]\nmax_width = 10\n"));
        assert_eq!(out, "⎇ worktre…");
        assert_eq!(display_width(&out), 10);
        let out =
            plain(&render("[[line]]\nmodules = [\"branch\"]\n[modules.branch]\nmax_length = 5\n"));
        assert_eq!(out, "⎇ work…");
        let out = plain(&render(
            "[[line]]\nmodules = [\"branch\"]\n[modules.branch]\nmax_length = 5\nmax_width = 4\n",
        ));
        assert_eq!(out, "⎇ w…");
        // The cap runs before alignment: the capped module is padded to the
        // column, so the bars still stack, and a placeholder is capped too.
        let (config, errs) = config::parse(
            "icons = \"unicode\"\nalign = true\n[[line]]\nmodules = [\"session_name\", \"clock\"]\n[[line]]\nmodules = [\"vim\", \"clock\"]\n[modules.session_name]\nmax_width = 5\n[modules.vim]\nhide_when_empty = false\nlabel = \"vim\"\nmax_width = 3\n",
            &SCHEMAS,
        );
        assert!(errs.is_empty(), "{errs:?}");
        let out = strip_ansi(&render_plain_at(&payload, &config, Some(60), &Clock::fixed()));
        let rows: Vec<&str> = out.lines().collect();
        assert!(rows[0].contains("❯ ga…") && !rows[0].contains("garnish"), "{out}");
        assert!(rows[1].contains("vi…") && !rows[1].contains("vim –"), "{out}");
        let bar = |row: &str| row.chars().position(|c| c == '│');
        assert_eq!(bar(rows[0]), bar(rows[1]), "{out}");
        // `max_width = 0` is the default and changes nothing.
        assert_eq!(
            render("[[line]]\nmodules = [\"path\", \"branch\"]\n[modules.path]\nmax_width = 0\n"),
            render("[[line]]\nmodules = [\"path\", \"branch\"]\n")
        );
    }

    /// SPEC § 3.7: a text module is a fixed-width box; short text is
    /// justified, long text clipped or scrolled by the clock, the scroller
    /// frozen at frame 0 without animation, and the common decorations apply.
    #[test]
    fn text_modules_render_as_fixed_width_boxes() {
        let payload = fixture("subscription-full");
        let base = "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"text.a\", \"text.b\", \"text.c\", \"text.d\"]\n";
        let texts = "[modules.text.a]\ntext = \"a rather long note\"\nwidth = 8\noverflow = \"clip\"\n[modules.text.b]\ntext = \"v0.2\"\nwidth = 8\njustify = \"right\"\nlabel = \"tag\"\n[modules.text.c]\ntext = \"hi\"\nwidth = 6\npad = 1\njustify = \"center\"\nprefix = \"[\"\nsuffix = \"]\"\n[modules.text.d]\ntext = \"\\u001b[31mred\\u001b[0m scroll me please\"\nwidth = 10\noverflow = \"scroll\"\n";
        let (config, errs) = config::parse(&format!("{base}{texts}"), &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        let at = |secs: i64, animate: bool| Clock {
            now: jiff::Timestamp::from_second(secs).unwrap(),
            animate,
            ..Clock::fixed()
        };
        let frozen = strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()));
        // clip: 8 cells with the ellipsis; right: label then the text hugging
        // the box's right edge; center: pad, box, pad inside prefix/suffix;
        // scroll at offset 0: the first 10 cells, escapes stripped.
        assert_eq!(frozen.trim_end(), "a rathe…  tag     v0.2  [   hi   ]  red scroll");
        let moving =
            strip_ansi(&render_plain_at(&payload, &config, Some(80), &at(1_738_425_601, true)));
        // text.d is 20 cells: offset 1738425601 % 20 = 1 (the window keeps
        // its trailing cell, so no trimming here).
        assert!(moving.ends_with("ed scroll "), "{moving:?}");
        let later =
            strip_ansi(&render_plain_at(&payload, &config, Some(80), &at(1_738_425_605, true)));
        assert!(later.ends_with("croll me p"), "{later:?}");
        // Every render is the same width: the boxes never move.
        for out in [&frozen, &moving, &later] {
            assert_eq!(display_width(out), display_width(&frozen), "{out:?}");
        }
        // An empty text hides the module; a missing table skips the id.
        let (config, errs) = config::parse(
            "icons = \"unicode\"\n[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"text.e\", \"model\"]\n[modules.text.e]\ntext = \"\"\n",
            &SCHEMAS,
        );
        assert!(errs.is_empty(), "{errs:?}");
        let out = strip_ansi(&render_plain_at(&payload, &config, Some(40), &Clock::fixed()));
        assert!(out.starts_with("❖ Opus"), "{out}");
    }

    /// SPEC § 4.2 Animated glyphs: an icon with `<key>_frames` cycles one
    /// frame per tick and stays on frame 0 when animations are off;
    /// `spinner_frames` is the general form of the clock's spinner.
    #[test]
    fn icon_frames_cycle_with_the_clock() {
        let payload = fixture("subscription-full");
        let render = |secs: i64, animate: bool| {
            let text = "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"model\", \"clock\"]\n[modules.model.icons]\nmodel_frames = [\"◐\", \"◓\", \"◑\", \"◒\"]\n[modules.clock.icons]\nspinner_frames = [\"ab\", \"cd\", \"ef\"]\n";
            let (config, errs) = config::parse(text, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = Clock {
                now: jiff::Timestamp::from_second(secs).unwrap(),
                animate,
                ..Clock::fixed()
            };
            strip_ansi(&render_plain_at(&payload, &config, Some(60), &clock))
        };
        // 1738425600 % 4 = 0 and % 3 = 0.
        assert!(
            render(1_738_425_600, true).starts_with("◐ Opus  ab 16:00:00"),
            "{}",
            render(1_738_425_600, true)
        );
        assert!(
            render(1_738_425_601, true).starts_with("◓ Opus  cd 16:00:01"),
            "{}",
            render(1_738_425_601, true)
        );
        assert!(
            render(1_738_425_605, true).starts_with("◓ Opus  ef 16:00:05"),
            "{}",
            render(1_738_425_605, true)
        );
        // Off: frame 0 of each cycle, like every other animation.
        assert!(
            render(1_738_425_601, false).starts_with("◐ Opus  ab 16:00:01"),
            "{}",
            render(1_738_425_601, false)
        );
    }

    /// SPEC § 4.2: the rule pattern travels one step per tick in the
    /// configured direction and the separator cycles its frames; both freeze
    /// with `animate = false`, and a per-line separator wins over the frames.
    #[test]
    fn frame_animation_moves_with_the_clock_and_freezes_on_request() {
        let payload = fixture("subscription-full");
        let render = |text: &str, secs: i64, animate: bool| {
            let (config, errs) = config::parse(&format!("icons = \"unicode\"\n{text}"), &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = Clock {
                now: jiff::Timestamp::from_second(secs).unwrap(),
                animate,
                ..Clock::fixed()
            };
            strip_ansi(&render_plain_at(&payload, &config, Some(40), &clock))
        };
        let pattern = "[frame]\nfill_pattern = \"·  \"\n[[line]]\nmodules = [\"model\"]\nright = [\"clock\"]\n";
        // 1738425600 % 3 = 0: frame 0 → the pattern starts with its first cell.
        let f0 = render(pattern, 1_738_425_600, true);
        let f1 = render(pattern, 1_738_425_601, true);
        let f2 = render(pattern, 1_738_425_602, true);
        assert!(f0.contains("Opus ·  ·  ·"), "{f0}");
        assert!(
            f1.contains("Opus  ·  ·  ·"),
            "right: the dots moved one cell toward the cap: {f1}"
        );
        assert!(f2.contains("Opus   ·  ·"), "{f2}");
        assert!(render(pattern, 1_738_425_603, true).contains("Opus ·  ·  ·"), "period 3");
        for out in [&f0, &f1, &f2] {
            assert_eq!(display_width(out), 36, "{out}");
        }
        let frozen = render(pattern, 1_738_425_601, false);
        assert!(frozen.contains("Opus ·  ·  ·") && frozen.contains("⠋ 16:00:01"), "{frozen}");
        let left = "[frame]\nfill_pattern = \"·  \"\nfill_direction = \"left\"\n[[line]]\nmodules = [\"model\"]\nright = [\"clock\"]\n";
        assert!(render(left, 1_738_425_601, true).contains("Opus   ·  ·"), "left: the other way");
        let frames = "[frame]\nseparator_frames = [\" │ \", \" ┃ \", \" ╎ \"]\n[[line]]\nmodules = [\"model\", \"session\"]\n[[line]]\nmodules = [\"api\", \"cache\"]\nseparator = \" · \"\n";
        let s0 = render(frames, 1_738_425_600, true);
        let s1 = render(frames, 1_738_425_601, true);
        assert!(s0.contains(" │ ") && !s0.contains(" ┃ "), "{s0}");
        assert!(s1.contains(" ┃ ") && !s1.contains(" │ "), "{s1}");
        assert!(s1.lines().nth(1).unwrap().contains(" · "), "per-line separator wins: {s1}");
        let frozen = render(frames, 1_738_425_601, false);
        assert!(frozen.contains(" │ ") && !frozen.contains(" ┃ "), "frozen at frame 0: {frozen}");
    }

    /// SPEC § 4.2 reduced motion: with `animate` unset, Claude Code's
    /// `prefersReducedMotion` freezes every animation; an explicit `animate`
    /// wins over the setting, the session switch (`GARNISH_ANIMATE=0`) wins
    /// over both, and a clock that may not read the settings chain (docs,
    /// goldens) never sees the setting.
    #[test]
    fn reduced_motion_freezes_animations_unless_the_config_decides() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        let home = dir.path().join("home");
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let project_settings = project.join(".claude/settings.json");
        std::fs::write(&project_settings, r#"{"prefersReducedMotion": true}"#).unwrap();
        let path = format!(
            "{}/tests/fixtures/payloads/subscription-full.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let mut json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        json["workspace"]["project_dir"] = serde_json::json!(project.to_str().unwrap());
        let payload = Payload::parse(&json.to_string()).unwrap();
        // No managed file: the test must not see the machine's.
        let clock = |animate: bool, settings: bool| Clock {
            now: jiff::Timestamp::from_second(1_738_425_601).unwrap(),
            home: Some(home.to_str().unwrap().to_owned()),
            animate,
            settings,
            managed: None,
            ..Clock::fixed()
        };
        // The docs and goldens render with the fixed clock: no settings file.
        assert!(!Clock::fixed().settings && Clock::fixed().managed.is_none());
        assert_eq!(Clock::fixed().settings_chain(&payload), Vec::new());
        assert_eq!(clock(true, true).settings_chain(&payload).len(), 3, "local, project, user");
        let cfg = |text: &str| {
            let (config, errs) = config::parse(text, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            config
        };
        let base = "icons = \"unicode\"\noverflow = \"ticker\"\n[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"model\", \"effort\", \"context\", \"session\", \"api\", \"cache\", \"lines\"]\nright = [\"clock\"]\n";
        // At 1738425601 the clock's spinner (in the right group, which a
        // ticker never scrolls) is on frame 1 (`⠙`) when it turns and on
        // `⠋` when frozen; frozen, the over-wide left group is cut with `…`
        // instead of scrolled.
        let render = |c: &Config, k: &Clock| strip_ansi(&render_plain_at(&payload, c, Some(50), k));
        let frozen = |out: &str| out.contains("⠋ 16:00:01") && out.contains('…');
        let moving = |out: &str| out.contains("⠙ 16:00:01") && !out.contains('…');
        assert!(frozen(&render(&cfg(base), &clock(true, true))), "unset + setting: frozen");
        assert!(
            moving(&render(&cfg(&format!("animate = true\n{base}")), &clock(true, true))),
            "an explicit key wins over the setting"
        );
        assert!(
            frozen(&render(&cfg(&format!("animate = true\n{base}")), &clock(false, true))),
            "the session switch wins over both"
        );
        assert!(
            moving(&render(&cfg(base), &clock(true, false))),
            "a clock without settings access never sees the setting"
        );
        // A managed file outranks the project's.
        let managed = dir.path().join("managed.json");
        std::fs::write(&managed, r#"{"prefersReducedMotion": false}"#).unwrap();
        let with_managed = Clock { managed: Some(managed), ..clock(true, true) };
        assert!(moving(&render(&cfg(base), &with_managed)), "the managed file wins");
        // The user file is read when the project files do not decide.
        std::fs::remove_file(&project_settings).unwrap();
        assert!(moving(&render(&cfg(base), &clock(true, true))), "no file: on");
        std::fs::write(home.join(".claude/settings.json"), r#"{"prefersReducedMotion": true}"#)
            .unwrap();
        assert!(frozen(&render(&cfg(base), &clock(true, true))), "the user file counts");
        std::fs::write(&project_settings, r#"{"prefersReducedMotion": false}"#).unwrap();
        assert!(moving(&render(&cfg(base), &clock(true, true))), "the project file wins");
    }

    /// SPEC § 3: a module its `hide` list takes off the row rendered
    /// nothing, never the `–` an empty render gets under `hide_when_empty
    /// = false`; a rule that does not fire leaves the module alone.
    #[test]
    fn a_module_hidden_by_its_list_never_prints_the_placeholder() {
        let payload = fixture("api-key");
        let base = "[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"context\"]\n[modules.context]\nhide_when_empty = false\n";
        let render = |rule: &str| {
            let (config, errs) = config::parse(&format!("{base}hide = [\"{rule}\"]\n"), &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            render_lines_at(&payload, &config, Some(80), &Clock::fixed())
        };
        assert!(render("above:0").is_empty(), "hidden, yet a row was drawn");
        let shown = render("below:0");
        assert_eq!(shown.len(), 1);
        assert!(shown[0].iter().any(|s| s.text().contains("42%")), "{shown:?}");
    }

    /// A spacer takes whatever cap its position calls for: first, last or,
    /// alone, the single-line caps.
    #[test]
    fn spacer_caps_follow_its_position() {
        let payload = fixture("subscription-full");
        let plain = |text: &str| {
            let (config, errs) = config::parse(text, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            strip_ansi(&render_plain_at(&payload, &config, Some(40), &Clock::fixed()))
        };
        let first = plain("[[line]]\nmodules = []\n[[line]]\nmodules = [\"model\"]\n");
        let rows: Vec<&str> = first.lines().collect();
        assert!(rows[0].starts_with("╭─") && rows[0].ends_with("╮"), "{first}");
        let last = plain("[[line]]\nmodules = [\"model\"]\n[[line]]\nmodules = []\n");
        let rows: Vec<&str> = last.lines().collect();
        assert!(rows[1].starts_with("╰─") && rows[1].ends_with("╯"), "{last}");
        let only = plain("[[line]]\nmodules = []\n");
        assert_eq!(only.lines().count(), 1, "{only}");
        assert!(only.starts_with("──") && only.ends_with("──"), "{only}");
        assert_eq!(display_width(&only), 36);
        // Unframed, a spacer is whitespace only: shown by preview, dropped by
        // Claude Code (SPEC § 2.1), which is why the docs ask for a frame.
        let none = plain("[frame]\nstyle = \"none\"\n[[line]]\nmodules = []\n");
        assert_eq!(none.trim(), "", "{none:?}");
    }

    /// SPEC § 4.1 `blank = true`: an unframed spacer carries one braille
    /// blank so Claude Code's trim keeps the row; the width is unchanged, a
    /// framed spacer is untouched, and the default stays whitespace only.
    #[test]
    fn a_blank_spacer_keeps_one_invisible_cell_without_a_frame() {
        let payload = fixture("subscription-full");
        let plain = |text: &str| {
            let (config, errs) = config::parse(text, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            strip_ansi(&render_plain_at(&payload, &config, Some(40), &Clock::fixed()))
        };
        let kept = plain("[frame]\nstyle = \"none\"\n[[line]]\nmodules = []\nblank = true\n");
        let row = kept.lines().next().unwrap();
        assert_eq!(display_width(row), 36, "{row:?}");
        assert_eq!(row.chars().next(), Some(BLANK_CELL), "{row:?}");
        assert!(row.chars().skip(1).all(|c| c == ' '), "{row:?}");
        assert!(!row.trim().is_empty(), "the harness keeps it: {row:?}");
        // JavaScript's trim strips the Unicode White_Space set plus U+FEFF;
        // the braille blank (category So) is in neither.
        assert!(!BLANK_CELL.is_whitespace() && BLANK_CELL != '\u{feff}');
        let framed = plain("[[line]]\nmodules = []\nblank = true\n");
        assert!(!framed.contains(BLANK_CELL), "a visible frame needs no cell: {framed:?}");
        assert_eq!(display_width(framed.lines().next().unwrap()), 36);
        // `fill = false` and no frame: the row is empty, so the cell is the row.
        let bare = plain(
            "[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = []\nblank = true\n",
        );
        assert_eq!(bare.lines().next().unwrap(), BLANK_CELL.to_string(), "{bare:?}");
        // A rule of no-break spaces is whitespace to the harness too.
        let nbsp = plain(
            "[frame]\nstyle = \"custom\"\nfill_char = \"\\u00a0\"\n[[line]]\nmodules = []\nblank = true\n",
        );
        let row = nbsp.lines().next().unwrap();
        assert_eq!(row.chars().next(), Some(BLANK_CELL), "{row:?}");
        assert!(row.chars().skip(1).all(|c| c == '\u{a0}'), "{row:?}");
        assert_eq!(display_width(row), 36);
        // With colour on, the frame's colour codes already keep the default
        // spacer: the harness trims raw bytes, escapes included (SPEC § 2.1).
        let (config, _) =
            config::parse("[frame]\nstyle = \"none\"\n[[line]]\nmodules = []\n", &SCHEMAS);
        let rows = render_lines_at(&payload, &config, Some(40), &Clock::fixed());
        let painter = crate::ansi::Painter {
            mode: crate::ansi::ColorMode::TrueColor,
            links: false,
            dim: false,
        };
        let bytes = painter.paint(rows.first().unwrap());
        assert!(bytes.contains('\u{1b}') && !bytes.trim().is_empty(), "{bytes:?}");
        // Two blank spacers around a module row: only the spacers change.
        let three = plain(
            "[frame]\nstyle = \"none\"\n[[line]]\nmodules = []\nblank = true\n[[line]]\nmodules = [\"model\"]\n[[line]]\nmodules = []\nblank = true\n",
        );
        let rows: Vec<&str> = three.lines().collect();
        assert_eq!(rows.len(), 3, "{three:?}");
        assert!(rows[0].starts_with(BLANK_CELL) && rows[2].starts_with(BLANK_CELL), "{three:?}");
        assert!(rows[1].contains("Opus") && !rows[1].contains(BLANK_CELL), "{three:?}");
    }

    /// SPEC § 9: an in-process render names its clock, and the pinned one
    /// reads no cache and spawns nothing. `render_plain` rendered on the
    /// environment's clock: every run of the unit tests took a lock in the
    /// developer's real cache for `account` and forked the test binary as
    /// its worker.
    #[test]
    fn cache_a_pinned_render_touches_no_cache_and_keeps_the_config_row() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("c");
        let clock = Clock { cache: Some(root.clone()), ..Clock::fixed() };
        let payload = fixture("subscription-full");
        let every_id = SCHEMAS
            .iter()
            .map(|s| format!("[[line]]\nmodules = [\"{}\"]\n", s.id))
            .collect::<Vec<_>>()
            .concat();
        for text in [every_id.clone(), format!("theme = \"nope\"\n{every_id}")] {
            let loaded = loaded(&text);
            let out = strip_ansi(&render_loaded(&payload, &loaded, Some(120), true, false, &clock));
            assert!(!root.exists(), "a pinned render touched {}", root.display());
            // The helper is that render: the rows `render_plain_at` draws,
            // then the `⚠ config:` row when the config has problems.
            let rows = render_plain_at(&payload, &loaded.config, Some(120), &Clock::fixed());
            let warning = (!loaded.errors.is_empty())
                .then(|| Painter::PLAIN.paint(&config_warning(&loaded, 116)));
            let want: Vec<String> = std::iter::once(rows).chain(warning).collect();
            assert_eq!(out, format!("{}\n", want.join("\n")));
            assert_eq!(render_plain(&payload, &loaded, Some(120)), out);
        }
    }

    /// A cached entry is fresh for the TTL its reader passes, the module's
    /// `refresh`, whatever TTL it was written with: what `benches/tick.rs`
    /// relies on to keep its seeded entries warm past the default `refresh`
    /// without spawning a worker.
    #[test]
    fn cache_a_seed_under_a_long_refresh_stays_warm_past_the_default_ttl() {
        use crate::cache::{Cache, Entry, Scope};
        let tmp = tempfile::tempdir().unwrap();
        let cache = Cache::at(tmp.path().to_path_buf());
        let payload = fixture("subscription-full");
        let scope = Scope::Session(payload.session_id.clone().unwrap());
        let values = [("email".to_owned(), "dev@example.com".to_owned())].into();
        let mut entry = Entry::ok(60_000, values);
        entry.computed_at_ms = entry.computed_at_ms.saturating_sub(700_000);
        cache.write(&scope, "account", &entry).unwrap();
        assert!(!cache.lookup(&scope, "account", 600_000).fresh, "past the default refresh");
        let (config, errs) = config::parse(
            "[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"account\"]\n[modules.account]\nrefresh = 86400\n",
            &SCHEMAS,
        );
        assert!(errs.is_empty(), "{errs:?}");
        let clock =
            Clock { workers: true, cache: Some(tmp.path().to_path_buf()), ..Clock::fixed() };
        let out = render_plain_at(&payload, &config, Some(80), &clock);
        assert!(out.contains("dev@example.com") && !out.contains('⟳'), "{out}");
        assert!(!cache.lock_path(&scope, "account").exists(), "a worker was started");
    }

    /// SPEC § 2.1: Claude Code trims every row's raw bytes, so a row that
    /// starts with whitespace was drawn shifted left. With colour on an empty
    /// SGR holds its cells, with colour off the braille blank does; the width
    /// never changes, and a row that is whitespace only stays the spacer
    /// rule's (§ 4.1).
    #[test]
    fn a_row_that_starts_with_spaces_keeps_them_through_the_harness_trim() {
        let payload = fixture("subscription-full");
        let tick = |color: &str, text: &str| -> Vec<String> {
            let loaded = loaded(&format!(
                "icons = \"unicode\"\ncolor = \"{color}\"\n[frame]\nstyle = \"none\"\n{text}"
            ));
            assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
            let out = render_loaded(&payload, &loaded, Some(40), false, false, &Clock::fixed());
            out.lines().map(str::to_owned).collect()
        };
        let kept = |rows: &[String]| {
            for row in rows {
                assert!(row.trim().is_empty() || row.trim_start() == row, "trimmed: {row:?}");
                assert_eq!(display_width(&strip_ansi(row)), 36, "{row:?}");
            }
        };
        // Only a right group: the rule's spaces lead the row, styled with
        // colour on and plain with it off.
        let right = "[[row]]\nright = [\"clock\"]\n";
        let rows = tick("never", right);
        kept(&rows);
        assert!(rows[0].starts_with(BLANK_CELL) && rows[0].ends_with("⠋ 16:00:00"), "{rows:?}");
        let rows = tick("always", right);
        kept(&rows);
        assert!(!rows[0].contains(BLANK_CELL), "{rows:?}");
        // A padding line above a short column: plain spaces even with
        // colour on, so the reset goes in front.
        let tall = "[[row]]\n[[row.col]]\nvalign = \"bottom\"\nmodules = [\"model\"]\n[[row.col]]\n[[row.col.row]]\nmodules = [\"session\"]\n[[row.col.row]]\nmodules = [\"clock\"]\n";
        let rows = tick("always", tall);
        kept(&rows);
        assert!(rows[0].starts_with("\x1b[0m "), "{rows:?}");
        assert!(rows[1].contains("Opus") && !rows[1].starts_with("\x1b[0m"), "{rows:?}");
        let rows = tick("never", tall);
        kept(&rows);
        assert!(rows[0].starts_with(BLANK_CELL), "{rows:?}");
        // A spacer that is spaces only is still dropped by the harness with
        // colour off: holding its cells is `blank`'s job.
        let rows = tick("never", "[[row]]\nmodules = [\"model\"]\n[[row]]\nmodules = []\n");
        assert!(rows[1].trim().is_empty() && !rows[1].is_empty(), "{rows:?}");
    }

    /// SPEC § 5: a render whose rows all hid prints one empty line, which
    /// Claude Code trims to nothing and so clears the status line. The docs
    /// promised "always prints something", which that line does not change
    /// on screen; this pins what the tick does.
    #[test]
    fn a_render_whose_rows_all_hid_is_one_empty_line() {
        let out =
            render_plain(&fixture("pr-absent"), &loaded("[[row]]\nmodules = [\"pr\"]\n"), Some(80));
        assert_eq!(out, "\n");
    }

    #[test]
    fn config_warning_names_the_first_problem_and_counts_the_rest() {
        let out = render_plain(
            &fixture("api-key"),
            &loaded("theme = \"nope\"\ndurations = \"loose\"\nmystery = 1"),
            Some(160),
        );
        let last = out.lines().last().unwrap();
        assert!(last.starts_with("⚠ config: durations: unknown value \"loose\""), "{last}");
        assert!(last.ends_with("(+2 more)"), "{last}");
    }

    /// Cell at which `needle` starts in `line`.
    fn column_of(line: &str, needle: &str) -> usize {
        let at = line.find(needle).unwrap_or_else(|| panic!("{needle:?} not in {line:?}"));
        display_width(line.get(..at).unwrap())
    }

    /// Cell of the last ` │ ` in `line`.
    fn last_bar(line: &str) -> usize {
        let at = line.rfind(" │ ").unwrap_or_else(|| panic!("no bar in {line:?}"));
        display_width(line.get(..at).unwrap())
    }

    #[test]
    fn align_stacks_separators_and_never_pads_the_last_module() {
        // Left groups: the first modules differ in width (column 2 must
        // start on the same cell) and so do the last ones (they must not be
        // padded). Right groups: the rightmost modules differ in width, so
        // the bar before them only lines up when the pad goes on the left.
        let base = "[[line]]\nmodules = [\"model\", \"session\"]\nright = [\"session\", \"api\"]\n[[line]]\nmodules = [\"session\", \"clock\"]\nright = [\"model\", \"lines\"]\n";
        let payload = fixture("subscription-full");
        let plain = |text: &str| {
            let (config, errs) = config::parse(&format!("icons = \"unicode\"\n{text}"), &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()))
        };
        let loose: Vec<String> = plain(base).lines().map(str::to_owned).collect();
        // Top-level keys go before the [[line]] tables.
        let aligned: Vec<String> =
            plain(&format!("align = true\n{base}")).lines().map(str::to_owned).collect();
        assert_eq!(loose.len(), 2, "{loose:?}");
        assert_eq!(aligned.len(), 2, "{aligned:?}");
        // Left column 2: `⏱ 1h12m` on line 1 (its first `⏱` is the left one),
        // `⠋ 16:00:00` on line 2.
        assert_ne!(
            column_of(&loose[0], "⏱ 1h12m"),
            column_of(&loose[1], "⠋ 16:00:00"),
            "without align the second column drifts: {loose:?}"
        );
        assert_eq!(
            column_of(&aligned[0], "⏱ 1h12m"),
            column_of(&aligned[1], "⠋ 16:00:00"),
            "{aligned:?}"
        );
        // The last left module keeps a single pad cell before the rule.
        assert!(aligned[0].contains("1h12m ──"), "{}", aligned[0]);
        assert!(aligned[1].contains("16:00:00 ──"), "{}", aligned[1]);
        // Right group: `⇄ 8m20s` and `Δ +156 −23` differ in width; the bar
        // before them lands on the same cell only if the pad is on their left.
        assert_ne!(last_bar(&loose[0]), last_bar(&loose[1]), "{loose:?}");
        assert_eq!(last_bar(&aligned[0]), last_bar(&aligned[1]), "{aligned:?}");
        assert!(aligned[0].ends_with("⇄ 8m20s ─╮"), "{}", aligned[0]);
        assert!(aligned[1].ends_with("Δ +156 −23 ─╯"), "{}", aligned[1]);
        for l in &aligned {
            assert_eq!(display_width(l), 76, "{l}");
        }
        // `right_justify = "start"`: the right group pads on the right, so the
        // text follows the separator and the gap sits before the cap; the
        // bars still stack (SPEC § 4.1).
        let start: Vec<String> = plain(&format!("align = true\nright_justify = \"start\"\n{base}"))
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(last_bar(&start[0]), last_bar(&start[1]), "{start:?}");
        assert!(start[0].ends_with("│ ⇄ 8m20s    ─╮"), "{}", start[0]);
        assert!(start[1].ends_with("│ Δ +156 −23 ─╯"), "{}", start[1]);
        // Left-packed lines anchor the right group on its left, so it pads
        // on the right and the bars still stack.
        let packed: Vec<String> = plain(&format!("align = true\n[frame]\nfill = false\n{base}"))
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(last_bar(&packed[0]), last_bar(&packed[1]), "{packed:?}");
        assert!(packed[0].ends_with("⇄ 8m20s"), "no trailing pad: {}", packed[0]);
        // Left-packed, the right group is already one sequence with the left
        // one, so `right_justify` has nothing to decide.
        assert_eq!(
            plain(&format!(
                "align = true\nright_justify = \"start\"\n[frame]\nfill = false\n{base}"
            )),
            packed.join("\n"),
            "right_justify is a no-op with fill = false"
        );
        // The default (align = false) render is untouched.
        assert_eq!(plain(base), plain(&format!("align = false\n{base}")));
    }

    /// SPEC § 4.3: with `align = true`, *k* counts from the right end in a
    /// right-justified column, column by column. The bucket used to take
    /// the first column's justification for all of them, so a left-justified
    /// column in one row made the right-justified ones below it count from
    /// the left, and their separators did not stack.
    #[test]
    fn align_counts_each_column_from_its_own_end() {
        let payload = fixture("subscription-full");
        let text = "icons = \"unicode\"\nalign = true\n[[row]]\n[[row.col]]\nmodules = [\"model\"]\n[[row.col]]\njustify = \"left\"\nmodules = [\"text.a\", \"text.b\"]\n[[row]]\n[[row.col]]\nmodules = [\"model\"]\n[[row.col]]\nmodules = [\"text.x\", \"text.yy\"]\n[[row]]\n[[row.col]]\nmodules = [\"model\"]\n[[row.col]]\nmodules = [\"text.xxx\", \"text.y\"]\n[modules.text.a]\ntext = \"aaaa\"\n[modules.text.b]\ntext = \"b\"\n[modules.text.x]\ntext = \"x\"\n[modules.text.yy]\ntext = \"yy\"\n[modules.text.xxx]\ntext = \"xxx\"\n[modules.text.y]\ntext = \"y\"\n";
        let (config, errs) = config::parse(text, &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        let out = strip_ansi(&render_plain_at(&payload, &config, Some(60), &Clock::fixed()));
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows.len(), 3, "{out}");
        assert_eq!(last_bar(rows[1]), last_bar(rows[2]), "{out}");
        assert!(rows[1].ends_with("x │ yy ─┤") && rows[2].ends_with("xxx │  y ─╯"), "{out}");
    }

    /// SPEC § 4.3: a column with a `right` group is drawn in the flex form,
    /// its left group anchored left, so with `align = true` that group
    /// counts *k* from the left and aligns with the left-justified columns,
    /// whatever the column's `justify` says. A last column is
    /// right-justified by default, and its left group used to count from
    /// the right, which padded nothing: the bars did not stack.
    #[test]
    fn align_counts_a_flex_columns_left_group_from_the_left() {
        let payload = fixture("subscription-full");
        let col = |justify: &str, left: &str, right: &str| {
            format!(
                "[[row]]\n[[row.col]]\nmodules = [\"model\"]\n[[row.col]]\n{justify}modules = [\"text.{left}\", \"text.b\"]\n{right}"
            )
        };
        let text = [
            "icons = \"unicode\"\nalign = true\n".to_owned(),
            col("", "a", "right = [\"text.r\"]\n"),
            col("", "aaaa", "right = [\"text.r\"]\n"),
            col("justify = \"left\"\n", "aa", ""),
            "[modules.text.a]\ntext = \"a\"\n[modules.text.aa]\ntext = \"aa\"\n[modules.text.aaaa]\ntext = \"aaaa\"\n[modules.text.b]\ntext = \"b\"\n[modules.text.r]\ntext = \"r\"\n".to_owned(),
        ]
        .concat();
        let (config, errs) = config::parse(&text, &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        let out = strip_ansi(&render_plain_at(&payload, &config, Some(60), &Clock::fixed()));
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows.len(), 3, "{out}");
        assert!(rows.iter().all(|r| r.contains("aaaa │ b") || r.contains("  │ b")), "{out}");
        assert_eq!(last_bar(rows[0]), last_bar(rows[1]), "{out}");
        assert_eq!(last_bar(rows[1]), last_bar(rows[2]), "{out}");
    }

    #[test]
    fn align_ignores_modules_that_render_nothing() {
        let payload = fixture("pr-absent");
        let plain = |lines: &str| {
            let (config, errs) =
                config::parse(&format!("align = true\nicons = \"unicode\"\n{lines}"), &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()))
        };
        // A hidden first module is not a column: no phantom bar before the clock.
        let out = plain(
            "[[line]]\nmodules = [\"pr\", \"clock\"]\n[[line]]\nmodules = [\"model\", \"clock\"]\n",
        );
        let first = out.lines().next().unwrap();
        assert!(first.starts_with("╭─ ⠋ 16:00:00 ─"), "{out}");
        // A hidden last module does not turn the visible last module into a
        // padded one: a single pad cell before the rule.
        let out = plain(
            "[[line]]\nmodules = [\"model\", \"pr\"]\n[[line]]\nmodules = [\"session\", \"clock\"]\n",
        );
        let first = out.lines().next().unwrap();
        assert!(!first.contains('│') && !first.contains("  ─"), "{out}");
    }

    #[test]
    fn fixed_durations_render_two_units() {
        let (config, _) = config::parse("durations = \"fixed\"", &SCHEMAS);
        let out = strip_ansi(&render_plain_at(
            &fixture("subscription-full"),
            &config,
            Some(100),
            &Clock::fixed(),
        ));
        assert!(out.contains("1h12m") && out.contains("8m20s"), "{out}");
        assert!(out.contains("2h13m") && out.contains("3d04h"), "{out}");
        let (config, _) = config::parse("", &SCHEMAS);
        let out = strip_ansi(&render_plain_at(
            &fixture("subscription-full"),
            &config,
            Some(100),
            &Clock::fixed(),
        ));
        assert!(out.contains("3d4h"), "compact stays the default: {out}");
    }

    #[test]
    fn a_ticker_pins_timers_fixed_and_a_module_can_opt_out() {
        // SPEC § 4.1: under `overflow = "ticker"` the timers print fixed so
        // the window slides; one module's own `durations` wins over that.
        let wide = |cfg: &str| {
            strip_ansi(&render_plain_at(
                &fixture("subscription-full"),
                &config::parse(cfg, &SCHEMAS).0,
                Some(400),
                &Clock::fixed(),
            ))
        };
        // `3d04h`/`47m00s` are the fixed forms of the limit and cache
        // countdowns, `3d4h`/`47m` the compact ones (`1h12m` reads the same
        // in both styles, so it tells nothing).
        let out = wide("overflow = \"ticker\"");
        assert!(out.contains("3d04h") && out.contains("47m00s"), "{out}");
        let out = wide("overflow = \"ticker\"\n[modules.limit7d]\ndurations = \"compact\"");
        assert!(out.contains("3d4h") && out.contains("47m00s"), "{out}");
        let out = wide("overflow = \"ticker\"\ndurations = \"compact\"");
        assert!(out.contains("3d4h") && !out.contains("47m00s"), "the opt-in wins: {out}");
        let out = wide("[modules.cache]\ndurations = \"fixed\"");
        assert!(out.contains("47m00s") && out.contains("3d4h"), "pinned the other way: {out}");
    }

    #[test]
    fn minimal_preset_is_one_unframed_line() {
        let out = render_plain(&fixture("api-key"), &loaded("preset = \"minimal\""), Some(80));
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(!out.contains('╭'));
        assert!(out.contains('$'), "{out}");
    }

    /// SPEC § 3.6: an ascii row is ascii whatever the payload. The
    /// placeholder of an absent value (`hide_when_empty = false`, a null
    /// `used_percentage` on the first tick of every session, a cache
    /// without a ratio) was U+2013 in every set, so the `ascii-only`
    /// gallery preset broke its own promise.
    #[test]
    fn an_ascii_row_is_ascii_for_every_payload() {
        let preset = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/presets/ascii-only.toml"
        ))
        .unwrap();
        let every: String =
            std::iter::once("icons = \"ascii\"\n[frame]\nstyle = \"none\"\n".into())
                .chain(SCHEMAS.iter().map(|s| {
                    format!(
                        "[[line]]\nmodules = [\"{0}\"]\n[modules.{0}]\nhide_when_empty = false\n",
                        s.id
                    )
                }))
                .collect();
        // With colour off, a row that would start with whitespace (a right
        // group alone, unframed) and a `blank` spacer lead with the braille
        // blank that keeps them through the harness's trim: the one
        // character § 3.6 lets an ascii row carry, and only there.
        let held = "icons = \"ascii\"\n[frame]\nstyle = \"none\"\n[[row]]\nmodules = [\"model\"]\n[[row]]\nright = [\"clock\"]\n[[row]]\nblank = true\nmodules = []\n";
        for text in [preset.as_str(), every.as_str(), held] {
            let loaded = loaded(text);
            assert_eq!(loaded.errors, Vec::new());
            for f in &crate::fixtures::FIXTURES {
                let out = render_plain(&Payload::parse(f.text).unwrap(), &loaded, Some(100));
                for l in out.lines() {
                    let rest = l.strip_prefix(BLANK_CELL).unwrap_or(l);
                    assert!(rest.is_ascii(), "{}: {l:?}", f.name);
                }
                let leading = out.lines().filter(|l| l.starts_with(BLANK_CELL)).count();
                assert_eq!(leading, if text == held { 2 } else { 0 }, "{}: {out}", f.name);
            }
        }
        let every = render_plain(&fixture("pre-first-response"), &loaded(&every), Some(100));
        assert!(every.lines().any(|l| l.trim_end().ends_with("-------------------| -")), "{every}");
    }

    #[test]
    fn ascii_icons_and_narrow_width_never_overflow() {
        let out =
            render_plain(&fixture("subscription-full"), &loaded("icons = \"ascii\""), Some(40));
        // COLUMNS=40 leaves 36 cells inside Claude Code's box.
        for l in out.lines() {
            assert!(display_width(l) <= 36, "{l}");
            assert!(l.is_ascii() || l.contains('─') || l.contains('╭'), "{l}");
        }
    }

    /// SPEC § 9 config matrix: every frame style, one module per line and
    /// every module on one line render inside the box with the expected
    /// number of rows.
    #[test]
    fn every_frame_style_and_line_shape_renders_inside_the_box() {
        let payload = fixture("subscription-full");
        let ids: Vec<&str> = SCHEMAS.iter().map(|s| s.id).collect();
        for style in crate::frame::FrameStyle::ALL {
            let text = format!("preset = \"full\"\n[frame]\nstyle = \"{}\"\n", style.name());
            let out = render_plain(&payload, &loaded(&text), Some(120));
            assert_eq!(out.lines().count(), 4, "{style:?}: {out}");
            for l in out.lines() {
                assert!(display_width(l) <= 116, "{style:?}: {l}");
            }
        }
        let one_per_line: String = std::iter::once("hide_empty_lines = false\n".to_owned())
            .chain(ids.iter().map(|id| format!("[[line]]\nmodules = [\"{id}\"]\n")))
            .collect();
        let out = render_plain(&payload, &loaded(&one_per_line), Some(120));
        assert_eq!(out.lines().count(), ids.len(), "{out}");
        for l in out.lines() {
            assert_eq!(display_width(l), 116, "{l}");
        }
        let quoted: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
        let all_on_one = format!("[[line]]\nmodules = [{}]\n", quoted.join(", "));
        let out = render_plain(&payload, &loaded(&all_on_one), Some(120));
        assert_eq!(out.lines().count(), 1, "{out}");
        assert_eq!(display_width(out.trim_end()), 116, "{out}");
        assert!(out.contains('…'), "twenty-five modules do not fit in 116 cells: {out}");
    }

    #[test]
    fn every_fixture_renders_every_preset_without_panicking() {
        use rayon::prelude::*;
        let dir = format!("{}/tests/fixtures/payloads", env!("CARGO_MANIFEST_DIR"));
        let paths: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        // 400+ renders: fan out so a slow CI runner stays inside the test timeout.
        paths.par_iter().for_each(|path| {
            let payload = Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            for preset in ["default", "minimal", "full", "compact"] {
                for icons in ["nerd", "unicode", "emoji", "ascii"] {
                    let text = format!("preset = \"{preset}\"\nicons = \"{icons}\"");
                    let out = render_plain(&payload, &loaded(&text), Some(120));
                    assert!(!out.trim().is_empty(), "{path:?} {preset} {icons}");
                    // COLUMNS=120 leaves 116 cells inside Claude Code's box.
                    for l in out.lines() {
                        assert!(display_width(l) <= 116, "{path:?} {preset} {icons}: {l}");
                    }
                }
            }
        });
    }

    /// One module alone on an unframed line with a preset, an icon set, a
    /// `max_width` and one of its own options set, for the schema matrix.
    fn matrix_config(
        id: &str,
        preset: crate::config::schema::Preset,
        icons: IconSet,
        max: usize,
        hide_when_empty: bool,
        extra: &str,
    ) -> Config {
        let text = format!(
            "icons = \"{}\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"{id}\"]\n[modules.{id}]\npreset = \"{}\"\nmax_width = {max}\nhide_when_empty = {hide_when_empty}\n{extra}",
            icons.name(),
            preset.name()
        );
        let (config, errs) = config::parse(&text, &SCHEMAS);
        assert!(errs.is_empty(), "{id} {extra:?} {errs:?}");
        config
    }

    /// Every switch a module's schema declares, as a TOML line: both values
    /// of a `Bool`, every variant of an `Enum`, and `0` for an `Int` whose
    /// default is not (a width, a length or a count turned off). This is
    /// what makes the matrix cover a new *option*, not just a new module.
    fn matrix_switches(schema: &crate::config::schema::ModuleSchema) -> Vec<String> {
        use crate::config::schema::{Kind, Value};
        schema
            .opts
            .iter()
            .flat_map(|opt| match opt.kind {
                Kind::Bool => {
                    vec![format!("{} = true\n", opt.key), format!("{} = false\n", opt.key)]
                }
                Kind::Enum(values) => {
                    values.iter().map(|v| format!("{} = \"{v}\"\n", opt.key)).collect()
                }
                Kind::Int if opt.default != Value::Int(0) => vec![format!("{} = 0\n", opt.key)],
                _ => Vec::new(),
            })
            .collect()
    }

    /// One row of the schema matrix: a module id, a preset, an icon set, a
    /// `max_width`, and one of the module's own switches as a TOML line.
    type Case = (&'static str, crate::config::schema::Preset, IconSet, usize, String);

    /// SPEC § 9 module matrix from the schema: every module × every preset
    /// × every icon set × a few `max_width` values, plus every switch each
    /// schema declares, alone on an unframed line, against every payload
    /// fixture, holds the invariants every module shares. A new module or
    /// option gets them checked without a hand-written test.
    #[test]
    fn schema_matrix_holds_the_shared_invariants() {
        use rayon::prelude::*;
        let dir = format!("{}/tests/fixtures/payloads", env!("CARGO_MANIFEST_DIR"));
        let payloads: Vec<(String, Payload)> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| {
                let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                (name, Payload::parse(&std::fs::read_to_string(&p).unwrap()).unwrap())
            })
            .collect();
        assert!(payloads.len() > 20, "the fixture directory moved");
        let mut cases: Vec<Case> = SCHEMAS
            .iter()
            .flat_map(|s| {
                crate::config::schema::Preset::ALL.into_iter().flat_map(move |preset| {
                    IconSet::ALL.into_iter().flat_map(move |icons| {
                        [0_usize, 1, 4, 12]
                            .into_iter()
                            .map(move |max| (s.id, preset, icons, max, String::new()))
                    })
                })
            })
            .collect();
        // Every switch a schema declares, at one preset and icon set: this
        // is what makes a new *option* inherit the invariants too.
        let switches: Vec<Case> = SCHEMAS
            .iter()
            .flat_map(|s| {
                matrix_switches(s).into_iter().flat_map(move |extra| {
                    [0_usize, 4].into_iter().map(move |max| {
                        let preset = crate::config::schema::Preset::Default;
                        (s.id, preset, IconSet::Unicode, max, extra.clone())
                    })
                })
            })
            .collect();
        assert!(switches.len() > 100, "the schemas lost their switches: {}", switches.len());
        cases.extend(switches);
        cases.par_iter().for_each(|case| matrix_case_holds(case, &payloads));
    }

    /// One matrix case against every fixture. Per case and fixture: the
    /// module as configured, with `hide_when_empty = false`, uncapped for
    /// comparison, and painted with links on.
    fn matrix_case_holds(case: &Case, payloads: &[(String, Payload)]) {
        let (id, preset, icons, max, extra) = case;
        let (id, preset, icons, max) = (*id, *preset, *icons, *max);
        let extra = extra.as_str();
        let painter = Painter { mode: ColorMode::TrueColor, links: true, dim: false };
        let is_escape = |c: char| c.is_control() || c == '\u{7}';
        {
            let hidden = matrix_config(id, preset, icons, max, true, extra);
            let shown = matrix_config(id, preset, icons, max, false, extra);
            let uncapped = matrix_config(id, preset, icons, 0, true, extra);
            // The ellipsis a cut ends in: `…`, or as much of `..` as fits.
            let cut_mark: String = icons.ellipsis().chars().take(max.max(1)).collect();
            let clock = Clock::fixed();
            for (name, payload) in payloads {
                let label = format!(
                    "{id} {} {} max_width={max} {name} {}",
                    preset.name(),
                    icons.name(),
                    extra.trim_end()
                );
                let lines = render_lines_at(payload, &hidden, Some(200), &clock);
                // A hidden state renders nothing at all: the line is dropped
                // rather than left blank, and a shown one has visible text.
                assert!(lines.len() <= 1, "{label}: {lines:?}");
                for line in &lines {
                    assert!(line.iter().any(|s| !s.text().trim().is_empty()), "{label}: blank row");
                }
                // With the placeholder the module always shows something,
                // and the placeholder obeys the cap like a value.
                let placeholder = render_lines_at(payload, &shown, Some(200), &clock);
                assert_eq!(placeholder.len(), 1, "{label}: no placeholder row");
                // Wider than the cap without it means the module was cut,
                // and a cut ends in the ellipsis.
                let free = render_lines_at(payload, &uncapped, Some(200), &clock);
                let was_cut = max > 0 && free.first().is_some_and(|l| segments_width(l) > max);
                for (i, line) in lines.iter().chain(placeholder.iter()).enumerate() {
                    let width = segments_width(line);
                    if max > 0 {
                        assert!(width <= max, "{label}: {width} cells > {max}: {line:?}");
                    }
                    let text = Painter::PLAIN.paint(line);
                    // A part carries the space before it only when something
                    // precedes it, and a lead's own space is that space: a
                    // module never opens with a space nor doubles one.
                    assert!(
                        !text.starts_with(' ') && !text.contains("  "),
                        "{label}: stray space in {text:?}"
                    );
                    if i == 0 && !lines.is_empty() && !was_cut {
                        assert_eq!(
                            Some(line),
                            free.first(),
                            "{label}: the cap changed an uncut module"
                        );
                    }
                    if was_cut && i == 0 && !lines.is_empty() {
                        assert!(
                            text.ends_with(&cut_mark),
                            "{label}: {text:?} was cut without a mark"
                        );
                    }
                    for seg in line {
                        assert!(
                            !seg.text().chars().any(is_escape),
                            "{label}: escape or control byte in {:?}",
                            seg.text()
                        );
                        // A link the painter would refuse is dropped at paint
                        // time (SPEC § 5), so a module that builds a
                        // malformed URL loses it silently on screen. Fail
                        // here instead.
                        assert!(
                            seg.link.as_deref().is_none_or(crate::ansi::safe_link),
                            "{label}: unsafe link {:?}",
                            seg.link
                        );
                    }
                    let styled = painter.paint(line);
                    // OSC 8 wrappers stay balanced: every open (`ESC ] 8 ; ;
                    // <url> ESC \`) is followed by its close before the next.
                    let mut open = false;
                    for part in styled.split("\x1b]8;;").skip(1) {
                        let closes = part.starts_with("\x1b\\");
                        assert_ne!(open, !closes, "{label}: unbalanced OSC 8 in {styled:?}");
                        open = !closes;
                    }
                    assert!(!open, "{label}: OSC 8 left open in {styled:?}");
                }
            }
        }
    }
}
