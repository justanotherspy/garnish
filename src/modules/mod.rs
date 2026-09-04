//! The module system: the trait every module implements, the registry of the
//! fixed built-in set, the render context, and shared rendering helpers.

use std::sync::LazyLock;

use jiff::Timestamp;

use crate::ansi::{Color, Segment, Style};
use crate::config::schema::{ModuleCfg, ModuleSchema};
use crate::icons::IconSet;
use crate::payload::Payload;
use crate::theme::Theme;

pub mod context;
pub mod identity;
pub mod model;
pub mod repo;
pub mod session;
pub mod usage;
pub mod util;

/// How fresh a module's data is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Freshness {
    /// Rendered from live data (payload) or a cache entry within its TTL.
    #[default]
    Fresh,
    /// Rendered from a cache entry past its TTL; a refresh is under way.
    Stale,
    /// The last refresh failed; the message is kept for `doctor`.
    Failed(String),
}

/// A module's output for one tick.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rendered {
    /// Segments, in order. Empty means "nothing to show".
    pub segments: Vec<Segment>,
    /// Data freshness.
    pub freshness: Freshness,
}

impl Rendered {
    /// Nothing to show.
    #[must_use]
    pub const fn empty() -> Self {
        Self { segments: Vec::new(), freshness: Freshness::Fresh }
    }

    /// Fresh segments.
    #[must_use]
    pub const fn fresh(segments: Vec<Segment>) -> Self {
        Self { segments, freshness: Freshness::Fresh }
    }

    /// True when there is nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.text.is_empty())
    }
}

/// Everything a module may look at while rendering.
#[derive(Debug, Clone)]
pub struct Ctx<'a> {
    /// The harness payload.
    pub payload: &'a Payload,
    /// Resolved theme.
    pub theme: &'a Theme,
    /// Icon set in effect.
    pub icons: IconSet,
    /// The (possibly frozen) current instant.
    pub now: Timestamp,
    /// Terminal width available to the status line.
    pub width: usize,
}

/// A built-in module.
pub trait Module: Send + Sync {
    /// The module's configuration schema.
    fn schema(&self) -> ModuleSchema;
    /// Render for one tick. Must be cheap: no I/O beyond reading cache files.
    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered;
}

/// A registry entry.
pub struct Entry {
    /// The module.
    pub module: Box<dyn Module>,
    /// Its schema, built once.
    pub schema: ModuleSchema,
}

/// The fixed set of built-in modules, in documentation order.
pub static REGISTRY: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    let modules: Vec<Box<dyn Module>> = builtin();
    modules.into_iter().map(|m| Entry { schema: m.schema(), module: m }).collect()
});

/// Every module's schema, in documentation order.
pub static SCHEMAS: LazyLock<Vec<ModuleSchema>> =
    LazyLock::new(|| REGISTRY.iter().map(|e| e.schema.clone()).collect());

fn builtin() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(repo::PathModule),
        Box::new(repo::BranchModule),
        Box::new(repo::SyncModule),
        Box::new(repo::WorktreeModule),
        Box::new(repo::PrModule),
        Box::new(model::ModelModule),
        Box::new(model::EffortModule),
        Box::new(context::ContextModule),
        Box::new(model::StyleModule),
        Box::new(usage::LimitModule(usage::Window::FiveHour)),
        Box::new(usage::LimitModule(usage::Window::SevenDay)),
        Box::new(usage::LimitModule(usage::Window::Spend)),
        Box::new(usage::CostModule),
        Box::new(session::SessionModule),
        Box::new(session::ApiModule),
        Box::new(session::CacheModule),
        Box::new(session::ClockModule),
        Box::new(identity::SessionNameModule),
        Box::new(identity::VimModule),
        Box::new(identity::AgentModule),
        Box::new(identity::LinesModule),
    ]
}

/// Look up a registry entry by module id.
#[must_use]
pub fn entry(id: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.schema.id == id)
}

/// All module ids, in documentation order.
#[must_use]
pub fn ids() -> Vec<&'static str> {
    REGISTRY.iter().map(|e| e.schema.id).collect()
}

/// A styled text segment using a module color key.
#[must_use]
pub fn seg(cfg: &ModuleCfg, text: impl Into<String>, color_key: &str) -> Segment {
    Segment::styled(text, Style::fg(cfg.color(color_key)))
}

/// A styled icon segment (empty when the icon set has no glyph), followed by a space.
#[must_use]
pub fn icon(cfg: &ModuleCfg, icon_key: &str, color_key: &str) -> Vec<Segment> {
    let glyph = cfg.icon(icon_key);
    if glyph.is_empty() {
        Vec::new()
    } else {
        vec![Segment::styled(format!("{glyph} "), Style::fg(cfg.color(color_key)))]
    }
}

/// Dim, muted text.
#[must_use]
pub fn muted(theme: &Theme, text: impl Into<String>) -> Segment {
    Segment::styled(text, Style::fg(theme.role(crate::theme::Role::Muted)).dimmed())
}

/// Apply `label`, `prefix`, `suffix`, and staleness styling to a render.
#[must_use]
pub fn decorate(
    rendered: Rendered,
    cfg: &ModuleCfg,
    theme: &Theme,
    stale_glyphs: (&str, &str),
) -> Vec<Segment> {
    if rendered.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Segment> = Vec::new();
    if !cfg.prefix.is_empty() {
        out.push(Segment::plain(&cfg.prefix));
    }
    if !cfg.label.is_empty() {
        out.push(muted(theme, format!("{} ", cfg.label)));
    }
    match &rendered.freshness {
        Freshness::Fresh => out.extend(rendered.segments),
        Freshness::Stale => {
            out.extend(
                rendered.segments.into_iter().map(|s| Segment { style: s.style.dimmed(), ..s }),
            );
            if !stale_glyphs.0.is_empty() {
                out.push(muted(theme, format!(" {}", stale_glyphs.0)));
            }
        }
        Freshness::Failed(_) => {
            out.extend(
                rendered.segments.into_iter().map(|s| Segment { style: s.style.dimmed(), ..s }),
            );
            if !stale_glyphs.1.is_empty() {
                out.push(Segment::styled(
                    format!(" {}", stale_glyphs.1),
                    Style::fg(theme.role(crate::theme::Role::Danger)).dimmed(),
                ));
            }
        }
    }
    if !cfg.suffix.is_empty() {
        out.push(Segment::plain(&cfg.suffix));
    }
    out
}

/// Convenience: a plain-colored segment.
#[must_use]
pub fn colored(text: impl Into<String>, color: Color) -> Segment {
    Segment::styled(text, Style::fg(color))
}
