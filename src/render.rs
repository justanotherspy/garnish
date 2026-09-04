//! The tick pipeline: payload + config → lines of styled text.

use std::path::Path;

use crate::ansi::{ColorMode, Painter, Segment, Style, strip_ansi};
use crate::config::{self, Config, Loaded, Overlay, StaleStyle};
use crate::frame::{Layout, compose_line, join_modules};
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
}

impl Clock {
    /// From `GARNISH_NOW`, `TZ`/`/etc/localtime` and `HOME`.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            now: crate::time::now(),
            tz: crate::time::local_zone(),
            home: std::env::var("HOME").ok().filter(|h| !h.is_empty()),
        }
    }

    /// A fixed clock: 2025-02-01T16:00:00Z, UTC, home `/home/dev` — what the
    /// golden renders and the generated docs use.
    #[must_use]
    pub fn fixed() -> Self {
        Self {
            now: jiff::Timestamp::from_second(1_738_425_600).unwrap_or_default(),
            tz: jiff::tz::TimeZone::UTC,
            home: Some("/home/dev".to_owned()),
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
    };
    let stale = stale_glyphs(config.icons);
    let layout = Layout {
        chars: config.frame.chars.clone(),
        fill: config.frame.fill,
        width,
        truncate: config.truncate,
        ellipsis: if config.icons == IconSet::Ascii { "..".into() } else { "…".into() },
    };
    let count = config.lines.len();
    config
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let sep = config.separator(line);
            let left =
                join_modules(&render_group(&ctx, config, &line.left, stale), sep, &config.theme);
            let right =
                join_modules(&render_group(&ctx, config, &line.right, stale), sep, &config.theme);
            compose_line(&layout, &config.theme, i, count, &left, &right)
        })
        .collect()
}

fn render_group(
    ctx: &Ctx<'_>,
    config: &Config,
    ids: &[String],
    stale: (&str, &str),
) -> Vec<Vec<Segment>> {
    ids.iter()
        .filter_map(|id| {
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
        for l in &lines {
            assert_eq!(display_width(l), 100, "{l}");
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
        assert!(display_width(last) <= 40, "{last}");
        assert!(last.ends_with('…'), "{last}");
        let out = render_plain(
            &fixture("api-key"),
            &loaded("icons = \"ascii\"\ntheme = \"nope\""),
            Some(80),
        );
        assert!(out.lines().last().unwrap().starts_with("! config:"), "{out}");
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
        for l in out.lines() {
            assert!(display_width(l) <= 40, "{l}");
            assert!(l.is_ascii() || l.contains('─') || l.contains('╭'), "{l}");
        }
    }

    #[test]
    fn every_fixture_renders_every_preset_without_panicking() {
        let dir = format!("{}/tests/fixtures/payloads", env!("CARGO_MANIFEST_DIR"));
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let payload = Payload::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for preset in ["default", "minimal", "full", "compact"] {
                for icons in ["nerd", "unicode", "emoji", "ascii"] {
                    let text = format!("preset = \"{preset}\"\nicons = \"{icons}\"");
                    let out = render_plain(&payload, &loaded(&text), Some(120));
                    assert!(!out.trim().is_empty(), "{path:?} {preset} {icons}");
                    for l in out.lines() {
                        assert!(display_width(l) <= 120, "{path:?} {preset} {icons}: {l}");
                    }
                }
            }
        }
    }
}
