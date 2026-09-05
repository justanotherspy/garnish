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
    /// Claude Code's auto-compaction environment (empty for pinned renders).
    pub settings_env: crate::claude_settings::Env,
    /// Whether repository discovery is allowed (off for docs and goldens, so
    /// fixture paths never touch a real repository or the cache).
    pub git: bool,
    /// TTL periods a cached value may be overdue before it renders stale
    /// (`stale_after`, ≥ 1).
    pub stale_after: u32,
    /// How elapsed times and countdowns print (`durations`).
    pub durations: crate::time::DurationStyle,
    /// The repository for the payload's directory, discovered at most once.
    pub dirs: std::cell::OnceCell<Option<crate::git::Dirs>>,
}

impl Ctx<'_> {
    /// The session id the payload reports (or a placeholder).
    #[must_use]
    pub fn session_id(&self) -> &str {
        self.payload.session_id.as_deref().filter(|s| !s.is_empty()).unwrap_or("no-session")
    }

    /// A duration in the configured style: `9m` (compact) or `9m00s` (fixed).
    #[must_use]
    pub fn duration(&self, total_secs: u64) -> String {
        self.durations.format(total_secs)
    }

    /// Countdown from this tick's clock to an epoch-seconds instant in the
    /// configured style, or `None` once passed.
    #[must_use]
    pub fn countdown(&self, until_epoch_secs: i64) -> Option<String> {
        self.durations.countdown_at(until_epoch_secs, self.now.as_second())
    }

    /// The repository containing the payload's current directory, if any.
    #[must_use]
    pub fn git_dirs(&self) -> Option<&crate::git::Dirs> {
        self.dirs
            .get_or_init(|| {
                self.git.then(|| {
                    crate::git::discover(std::path::Path::new(self.payload.current_dir()?))
                })?
            })
            .as_ref()
    }

    /// Look a cached module up and, when it is stale and nobody is refreshing
    /// it, take the lock and spawn a detached worker.
    ///
    /// `valid` rejects an entry that was computed for a different situation
    /// (another branch, another upstream); a rejected entry is overdue at
    /// once. A failed entry is honoured for its TTL too, so a broken git does
    /// not spawn a worker on every tick. Returns the lookup plus the
    /// [`Freshness`] the render should carry: a value past its TTL still
    /// renders [`Freshness::Fresh`] while the worker runs and only becomes
    /// [`Freshness::Stale`] after `stale_after` TTLs (SPEC § 3.6).
    #[must_use]
    pub fn cached(
        &self,
        cfg: &ModuleCfg,
        scope: &Scope,
        valid: impl Fn(&crate::cache::Entry) -> bool,
    ) -> (Lookup, Freshness) {
        let ttl_ms = cfg.refresh.saturating_mul(1000);
        let mut lookup = self.cache.lookup(scope, cfg.id, ttl_ms);
        let mismatched = lookup.entry.as_ref().is_some_and(|e| !valid(e));
        if mismatched {
            lookup.fresh = false;
        }
        let failed = lookup.entry.as_ref().filter(|e| e.status == crate::cache::Status::Err);
        if lookup.fresh {
            let freshness = failed.map_or(Freshness::Fresh, |e| Freshness::Failed(e.error.clone()));
            return (lookup, freshness);
        }
        if !lookup.in_progress {
            self.spawn_refresh(cfg, scope);
        }
        let grace_ms = ttl_ms.saturating_mul(u64::from(self.stale_after.max(1)));
        let overdue = mismatched || lookup.entry.as_ref().is_none_or(|e| !e.is_fresh(grace_ms));
        let freshness = match failed {
            Some(e) => Freshness::Failed(e.error.clone()),
            None if overdue => Freshness::Stale,
            None => Freshness::Fresh,
        };
        (lookup, freshness)
    }

    /// Start a detached worker for a module. On Linux the tick takes the lock
    /// and hands it over (`--lock-held`); elsewhere pid liveness cannot be
    /// checked, so the worker takes the lock itself and a lock left behind by
    /// a killed tick cannot block refreshes.
    fn spawn_refresh(&self, cfg: &ModuleCfg, scope: &Scope) {
        let job = crate::spawn::Job {
            module: cfg.id.to_owned(),
            session: self.session_id().to_owned(),
            cwd: std::path::PathBuf::from(self.payload.current_dir().unwrap_or(".")),
        };
        if cfg!(target_os = "linux") {
            if let LockOutcome::Acquired(mut guard) = self.cache.lock(scope, cfg.id) {
                match crate::spawn::spawn(&job, self.cache.root(), true) {
                    crate::spawn::Spawned::Process | crate::spawn::Spawned::Logged => {
                        guard.disarm();
                    }
                    crate::spawn::Spawned::Failed(e) => {
                        crate::debug::log(&format!("spawn {} failed: {e}", cfg.id));
                    }
                }
            }
        } else if let crate::spawn::Spawned::Failed(e) =
            crate::spawn::spawn(&job, self.cache.root(), false)
        {
            crate::debug::log(&format!("spawn {} failed: {e}", cfg.id));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    /// Why a glyph is unsafe for the `unicode`/`emoji` sets, if it is.
    fn glyph_problem(g: &str) -> Option<&'static str> {
        // Box Drawing and Block Elements are East Asian Ambiguous by table,
        // but every terminal font draws them one cell wide (the bars and
        // frames depend on it), so they are exempt from the width rule.
        let drawing = |c: char| ('\u{2500}'..='\u{259f}').contains(&c);
        // Geometric Shapes: COSMIC Terminal drew `◔` and `◫` two cells wide
        // although no table says so (walkthrough bug 1); fonts in this block
        // are unreliable, so none of it is allowed.
        let geometric = |c: char| ('\u{25a0}'..='\u{25ff}').contains(&c);
        // The Misc Math hourglasses `⧖ ⧗` were drawn wide in the same terminal.
        let hourglass = |c: char| matches!(c, '\u{29d6}' | '\u{29d7}');
        if g.contains('\u{fe0f}') {
            Some("variation selector")
        } else if g.chars().any(geometric) {
            Some("Geometric Shapes block")
        } else if g.chars().any(hourglass) {
            Some("drawn two cells wide in COSMIC Terminal")
        } else if !g.chars().all(drawing) && g.width() != g.width_cjk() {
            Some("East Asian Ambiguous width")
        } else {
            None
        }
    }

    /// SPEC § 4.1 Glyph sets: every glyph in the built-in `unicode` and
    /// `emoji` sets must be one cell wide in the common terminals or two cells
    /// by every table. Terminals disagree on East Asian Ambiguous characters
    /// (`unicode-width` counts them 1 under `width`, 2 under `width_cjk`), on
    /// the Geometric Shapes block, and on emoji that need a variation
    /// selector (`U+FE0F`; COSMIC drew `⏱️ 🗄️` one cell wide while garnish
    /// counted two, walkthrough bug 10). The `nerd` set is out of scope: a
    /// Nerd Font's private-use glyphs are designed for one cell.
    #[test]
    fn unicode_and_emoji_glyphs_have_one_width_in_every_terminal() {
        let mut offenders = Vec::new();
        for schema in SCHEMAS.iter() {
            for icon in &schema.icons {
                for set in [IconSet::Unicode, IconSet::Emoji] {
                    let g = icon.glyph.get(set);
                    if let Some(problem) = glyph_problem(g) {
                        let points: Vec<String> =
                            g.chars().map(|c| format!("U+{:04X}", u32::from(c))).collect();
                        offenders.push(format!(
                            "{}.{} {}: {g:?} [{}] {problem}",
                            schema.id,
                            icon.key,
                            set.name(),
                            points.join(" "),
                        ));
                    }
                }
            }
        }
        assert!(offenders.is_empty(), "glyphs terminals disagree on:\n{}", offenders.join("\n"));
        // The rules themselves: the walkthrough offenders fail, the bars pass.
        for bad in ["◆", "◔", "◫", "⧗", "▦", "●", "→", "¤", "⏱\u{fe0f}"] {
            assert!(glyph_problem(bad).is_some(), "{bad:?} must be rejected");
        }
        for good in ["█", "░", "▏", "▁▃▅▇█", "─", "╭", "⏱", "⚡", "🌿", "❖", "✓"]
        {
            assert_eq!(glyph_problem(good), None, "{good:?} must be accepted");
        }
    }
}
