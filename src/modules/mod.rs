//! The module system: the trait every module implements, the registry of the
//! fixed built-in set, the render context, and shared rendering helpers.

use std::sync::LazyLock;

use jiff::Timestamp;

use std::collections::BTreeMap;

use crate::ansi::{Color, Segment, Style};
use crate::cache::{Cache, Entry as CacheEntry, LockOutcome, Lookup, Scope};
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
    /// The on-disk cache.
    pub cache: &'a Cache,
    /// The local time zone, resolved once per tick.
    pub tz: jiff::tz::TimeZone,
    /// The home directory, for `~` collapsing.
    pub home: Option<String>,
}

impl Ctx<'_> {
    /// The session id the payload reports (or a placeholder).
    #[must_use]
    pub fn session_id(&self) -> &str {
        self.payload.session_id.as_deref().filter(|s| !s.is_empty()).unwrap_or("no-session")
    }

    /// Look a cached module up and, when it is stale and nobody is refreshing
    /// it, take the lock and spawn a detached worker.
    ///
    /// Returns the lookup plus the [`Freshness`] the render should carry.
    #[must_use]
    pub fn cached(&self, cfg: &ModuleCfg, scope: &Scope) -> (Lookup, Freshness) {
        let ttl_ms = cfg.refresh.saturating_mul(1000);
        let lookup = self.cache.lookup(scope, cfg.id, ttl_ms);
        if lookup.fresh {
            return (lookup, Freshness::Fresh);
        }
        if !lookup.in_progress
            && let LockOutcome::Acquired(mut guard) = self.cache.lock(scope, cfg.id)
        {
            let job = crate::spawn::Job {
                module: cfg.id.to_owned(),
                session: self.session_id().to_owned(),
                cwd: std::path::PathBuf::from(self.payload.current_dir().unwrap_or(".")),
            };
            match crate::spawn::spawn(&job, self.cache.root(), true) {
                crate::spawn::Spawned::Process | crate::spawn::Spawned::Logged => guard.disarm(),
                crate::spawn::Spawned::Failed(_) => {}
            }
        }
        let freshness = match &lookup.entry {
            Some(e) if e.status == crate::cache::Status::Err => Freshness::Failed(e.error.clone()),
            _ => Freshness::Stale,
        };
        (lookup, freshness)
    }
}

/// What a worker needs to refresh a module.
#[derive(Debug, Clone)]
pub struct RefreshCtx<'a> {
    /// Session id.
    pub session: &'a str,
    /// Working directory the tick reported.
    pub cwd: &'a std::path::Path,
    /// The module's resolved config.
    pub cfg: &'a ModuleCfg,
    /// The cache.
    pub cache: &'a Cache,
}

/// A built-in module.
pub trait Module: Send + Sync {
    /// The module's configuration schema.
    fn schema(&self) -> ModuleSchema;
    /// Render for one tick. Must be cheap: no I/O beyond reading cache files.
    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered;
    /// Cache scope for this module given a session and working directory.
    /// Payload-only modules never call this.
    fn scope(&self, session: &str, _cwd: &std::path::Path) -> Scope {
        Scope::Session(session.to_owned())
    }
    /// Compute fresh values in the background worker. Payload-only modules
    /// return an error, which is recorded as a failed entry.
    ///
    /// # Errors
    /// Any failure is returned as text and cached as an `err` entry.
    fn refresh(&self, _ctx: &RefreshCtx<'_>) -> Result<BTreeMap<String, String>, String> {
        Err("module is not cached".to_owned())
    }
}

/// Run a module's refresh and store the result. Returns the written entry.
///
/// # Errors
/// Propagates cache write errors.
pub fn run_refresh(module: &dyn Module, ctx: &RefreshCtx<'_>) -> std::io::Result<CacheEntry> {
    let scope = module.scope(ctx.session, ctx.cwd);
    let ttl_ms = ctx.cfg.refresh.saturating_mul(1000);
    let entry = match module.refresh(ctx) {
        Ok(values) => CacheEntry::ok(ttl_ms, values),
        Err(e) => CacheEntry::err(ttl_ms, e),
    };
    ctx.cache.write(&scope, ctx.cfg.id, &entry)?;
    Ok(entry)
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
        if cfg.hide_when_empty {
            return Vec::new();
        }
        let mut out: Vec<Segment> = Vec::new();
        if !cfg.prefix.is_empty() {
            out.push(Segment::plain(&cfg.prefix));
        }
        if !cfg.label.is_empty() {
            out.push(muted(theme, format!("{} ", cfg.label)));
        }
        out.push(muted(theme, "–"));
        if !cfg.suffix.is_empty() {
            out.push(Segment::plain(&cfg.suffix));
        }
        return out;
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
