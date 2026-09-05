//! The tick pipeline: payload + config → lines of styled text.

use std::path::Path;

use crate::ansi::{ColorMode, Painter, Segment, Style, segments_width, strip_ansi};
use crate::config::{self, Config, Loaded, Overlay, StaleStyle};
use crate::frame::{Layout, Ticker, compose_line, join_modules};
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
    /// `NO_COLOR` is set.
    pub no_color: bool,
}

/// Render a full tick. Never fails and never prints nothing.
#[must_use]
pub fn render(req: &Request<'_>) -> String {
    let Ok(payload) = Payload::parse(req.payload_json) else {
        return "⚠ garnish: bad payload\n".to_owned();
    };
    let loaded = config::load_with(req.config_path, &SCHEMAS, &req.overlay);
    render_loaded(&payload, &loaded, req.columns, req.no_color)
}

/// Render with an already loaded config (used by tests, previews and benches).
#[must_use]
pub fn render_loaded(
    payload: &Payload,
    loaded: &Loaded,
    columns: Option<usize>,
    no_color: bool,
) -> String {
    let config = &loaded.config;
    let mode = config.color.mode(no_color);
    let painter = Painter { mode, links: mode != ColorMode::Never };
    let mut lines = render_lines(payload, config, columns);
    if !loaded.errors.is_empty() {
        lines.push(config_warning(loaded, config.width(columns)));
    }
    let mut out = String::new();
    for line in &lines {
        out.push_str(&painter.paint(line));
        out.push('\n');
    }
    if out.is_empty() {
        out.push('\n');
    }
    out
}

/// The trailing `⚠ config: <path>:<line> <message>` line, truncated to the width.
fn config_warning(loaded: &Loaded, width: usize) -> Vec<Segment> {
    let config = &loaded.config;
    let path =
        loaded.path.as_ref().map_or_else(|| "config".to_owned(), |p| p.display().to_string());
    let first = loaded.errors.first();
    let location = first.and_then(|e| e.line).map_or(String::new(), |l| format!(":{l}"));
    let message = first.map_or_else(String::new, |e| {
        if e.path.is_empty() { e.message.clone() } else { format!("{}: {}", e.path, e.message) }
    });
    let extra = loaded.errors.len().saturating_sub(1);
    let suffix = if extra > 0 { format!(" (+{extra} more)") } else { String::new() };
    let glyph = if config.icons == IconSet::Ascii { "!" } else { "⚠" };
    let line = vec![Segment::styled(
        format!("{glyph} config: {path}{location} {message}{suffix}"),
        Style::fg(config.theme.role(Role::Warn)).dimmed(),
    )];
    let ellipsis = if config.icons == IconSet::Ascii { ".." } else { "…" };
    crate::ansi::truncate(&line, width, ellipsis)
}

/// The environment-dependent inputs of a render, so docs and tests can pin them.
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
    /// index and scroll offset at 0 (`GARNISH_ANIMATE=0`, SPEC § 4.2).
    pub animate: bool,
}

impl Clock {
    /// From `GARNISH_NOW`, `TZ`/`/etc/localtime`, `HOME`, `GARNISH_ANIMATE`
    /// and the process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            now: crate::time::now(),
            tz: crate::time::local_zone(),
            home: std::env::var("HOME").ok().filter(|h| !h.is_empty()),
            settings_env: crate::claude_settings::Env::from_process(),
            git: true,
            animate: crate::time::animate_from_env(),
        }
    }

    /// A fixed clock: 2025-02-01T16:00:00Z, UTC, home `/home/dev`, no
    /// auto-compaction overrides, no repository discovery and animations
    /// frozen at frame 0 — what the generated docs use, so they come out
    /// identical on every machine.
    #[must_use]
    pub fn fixed() -> Self {
        Self {
            now: jiff::Timestamp::from_second(1_738_425_600).unwrap_or_default(),
            tz: jiff::tz::TimeZone::UTC,
            home: Some("/home/dev".to_owned()),
            settings_env: crate::claude_settings::Env::default(),
            git: false,
            animate: false,
        }
    }
}

/// Render every configured line to segments (no escape sequences yet).
#[must_use]
pub fn render_lines(
    payload: &Payload,
    config: &Config,
    columns: Option<usize>,
) -> Vec<Vec<Segment>> {
    render_lines_at(payload, config, columns, &Clock::from_env())
}

/// [`render_lines`] with an explicit clock, zone and home.
#[must_use]
pub fn render_lines_at(
    payload: &Payload,
    config: &Config,
    columns: Option<usize>,
    clock: &Clock,
) -> Vec<Vec<Segment>> {
    let width = config.width(columns);
    let cache = crate::cache::Cache::from_env();
    let ctx = Ctx {
        payload,
        theme: &config.theme,
        icons: config.icons,
        now: clock.now,
        width,
        cache: &cache,
        tz: clock.tz.clone(),
        home: clock.home.clone(),
        settings_env: clock.settings_env.clone(),
        git: clock.git,
        stale_after: config.stale_after,
        durations: config.durations,
        animate: clock.animate && config.animate,
        dirs: std::cell::OnceCell::new(),
    };
    let stale = stale_glyphs(config.icons);
    let layout = Layout {
        chars: config.frame.chars.clone(),
        fill: config.frame.fill,
        width,
        truncate: config.truncate,
        ellipsis: if config.icons == IconSet::Ascii { "..".into() } else { "…".into() },
        ticker: (config.overflow == config::Overflow::Ticker).then(|| Ticker {
            step: config.ticker_step,
            gap: config.ticker_gap.clone(),
            now: clock.now,
            animate: clock.animate && config.animate,
        }),
        rule: rule_pattern(config, &ctx),
    };
    let separator_frame =
        ctx.frame(config.frame.separator_step, config.frame.separator_frames.len());
    // Every line renders before any is composed: aligned columns need the
    // widths of all lines.
    let (mut lefts, mut rights): (Vec<_>, Vec<_>) = config
        .lines
        .iter()
        .map(|line| {
            (
                render_group(&ctx, config, &line.left, stale),
                render_group(&ctx, config, &line.right, stale),
            )
        })
        .unzip();
    if config.align {
        if config.frame.fill {
            align_columns(&mut lefts, false, false);
            let pad_left = config.right_justify == config::RightJustify::End;
            align_columns(&mut rights, true, pad_left);
        } else {
            // Left-packed, the right group follows the left one after a
            // separator, so the line is one sequence of columns (SPEC § 4).
            let split: Vec<usize> = lefts.iter().map(Vec::len).collect();
            let mut rows: Vec<Vec<Vec<Segment>>> = lefts
                .iter_mut()
                .zip(rights.iter_mut())
                .map(|(l, r)| {
                    let mut row = std::mem::take(l);
                    row.append(r);
                    row
                })
                .collect();
            align_columns(&mut rows, false, false);
            for ((mut row, n), (l, r)) in
                rows.into_iter().zip(split).zip(lefts.iter_mut().zip(rights.iter_mut()))
            {
                *r = row.split_off(n.min(row.len()));
                *l = row;
            }
        }
    }
    // A line whose modules all rendered nothing is dropped unless it is an
    // intentional spacer or `hide_empty_lines = false`; the caps follow the
    // survivors (SPEC § 4.1).
    let kept: Vec<_> = config
        .lines
        .iter()
        .zip(lefts.iter().zip(rights.iter()))
        .map(|(line, (left, right))| (line, left, right))
        .filter(|(line, left, right)| {
            let empty = left.is_empty() && right.is_empty();
            let hidden = config.hide_empty_lines && !line.spacer && empty;
            !hidden
        })
        .collect();
    let count = kept.len();
    kept.into_iter()
        .enumerate()
        .map(|(i, (line, left, right))| {
            let sep = config.separator_at(line, separator_frame);
            let left = join_modules(left, sep, &config.theme);
            let right = join_modules(right, sep, &config.theme);
            compose_line(&layout, &config.theme, i, count, &left, &right, sep)
        })
        .collect()
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

fn render_group(
    ctx: &Ctx<'_>,
    config: &Config,
    ids: &[String],
    stale: (&str, &str),
) -> Vec<Vec<Segment>> {
    ids.iter()
        .filter_map(|id| {
            // `text.<name>` comes from the config, not the fixed registry (SPEC § 3.7).
            if let Some(name) = id.strip_prefix(modules::text::PREFIX) {
                let cfg = config.texts.get(name).filter(|c| c.enabled)?;
                let rendered = modules::text::render(ctx, cfg);
                return Some(decorate(rendered, cfg, &config.theme, stale));
            }
            let entry = modules::entry(id)?;
            let cfg = config.modules.get(entry.schema.id)?;
            if !cfg.enabled {
                return None;
            }
            let rendered = entry.module.render(ctx, cfg);
            let rendered = match (config.stale_style, &rendered.freshness) {
                (StaleStyle::Hide, Freshness::Stale | Freshness::Failed(_)) => Rendered::empty(),
                (StaleStyle::Plain, _) => Rendered::fresh(rendered.segments),
                _ => rendered,
            };
            Some(decorate(rendered, cfg, &config.theme, stale))
        })
        // A module that rendered nothing is not a column (SPEC § 4).
        .filter(|module| !module.is_empty())
        .collect()
}

const fn stale_glyphs(icons: IconSet) -> (&'static str, &'static str) {
    match icons {
        IconSet::Ascii => ("~", "x"),
        IconSet::Nerd | IconSet::Unicode | IconSet::Emoji => ("⟳", "✗"),
    }
}

/// Plain-text render (no escapes), for tests and docs.
#[must_use]
pub fn render_plain(payload: &Payload, loaded: &Loaded, columns: Option<usize>) -> String {
    strip_ansi(&render_loaded(payload, loaded, columns, true))
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
    use crate::ansi::display_width;

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
        assert!(
            out.lines().last().unwrap().starts_with("⚠ config: config theme: unknown theme"),
            "{out}"
        );
        assert!(out.lines().count() >= 2);
        // syntax errors carry the line number and the warning never overflows the width
        let out =
            render_plain(&fixture("api-key"), &loaded("preset = \"default\"\n[frame\nx"), Some(40));
        let last = out.lines().last().unwrap();
        assert!(last.starts_with("⚠ config: config:2 "), "{last}");
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

    #[test]
    fn config_warning_names_the_first_problem_and_counts_the_rest() {
        let out = render_plain(
            &fixture("api-key"),
            &loaded("theme = \"nope\"\ndurations = \"loose\"\nmystery = 1"),
            Some(160),
        );
        let last = out.lines().last().unwrap();
        assert!(last.starts_with("⚠ config: config durations: unknown value \"loose\""), "{last}");
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
    fn minimal_preset_is_one_unframed_line() {
        let out = render_plain(&fixture("api-key"), &loaded("preset = \"minimal\""), Some(80));
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(!out.contains('╭'));
        assert!(out.contains('$'), "{out}");
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
}
