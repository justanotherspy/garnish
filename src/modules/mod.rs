//! The module system: the trait every module implements, the registry of the
//! fixed built-in set, the render context, and shared rendering helpers.

use std::sync::LazyLock;

use jiff::Timestamp;

use std::collections::BTreeMap;

use crate::ansi::{Color, Segment, Style};
use crate::cache::{Cache, Entry as CacheEntry, LockOutcome, Lookup, Scope};
use crate::config::schema::{Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::IconSet;
use crate::payload::Payload;
use crate::theme::Theme;

pub mod context;
pub mod identity;
pub mod model;
pub mod repo;
pub mod session;
pub mod text;
pub mod usage;
pub mod util;

/// Choices for a module's `durations` option; `inherit` follows the
/// top-level key (SPEC § 4.1).
pub const DURATION_CHOICES: &[&str] = &["inherit", "compact", "fixed"];

/// The `durations` option carried by every module that prints a timer or a
/// countdown, so one module can be pinned while the rest follow the
/// top-level `durations` (which is `fixed` by default under a ticker).
#[must_use]
pub fn durations_opt() -> OptSpec {
    OptSpec::new(
        "durations",
        Kind::Enum(DURATION_CHOICES),
        "How this module's timers and countdowns print: `inherit` follows the top-level `durations`; `compact` or `fixed` pins this module.",
        Value::Str("inherit".into()),
    )
}

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
        self.segments.iter().all(|s| s.text().is_empty())
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
    /// Whether animations advance with the clock (SPEC § 4.2); off, every
    /// [`Ctx::frame`] is 0.
    pub animate: bool,
    /// The repository for the payload's directory, discovered at most once.
    pub dirs: std::cell::OnceCell<Option<crate::git::Dirs>>,
    /// Claude Code's settings files this tick may read (SPEC § 2.3, § 4.2),
    /// highest precedence first: the chain of the directory Claude Code was
    /// launched in (not whatever subdirectory the session moved to) and the
    /// home; empty for a pinned render, which reads no settings file.
    pub settings_files: Vec<std::path::PathBuf>,
    /// The keys of those files, read at most once per tick.
    pub settings: std::cell::OnceCell<Vec<crate::claude_settings::FileKeys>>,
}

impl Ctx<'_> {
    /// The keys of the settings chain, read on first use and shared by
    /// every reader on the tick (the autocompact marker, reduced motion).
    #[must_use]
    pub fn settings(&self) -> &[crate::claude_settings::FileKeys] {
        self.settings.get_or_init(|| crate::claude_settings::read_keys(&self.settings_files))
    }

    /// The session id the payload reports (or a placeholder).
    #[must_use]
    pub fn session_id(&self) -> &str {
        self.payload.session_id.as_deref().filter(|s| !s.is_empty()).unwrap_or("no-session")
    }

    /// The duration style for a module: its own `durations` option unless
    /// that is `inherit`, then the top-level key (SPEC § 4.1).
    #[must_use]
    pub fn durations_for(&self, cfg: &ModuleCfg) -> crate::time::DurationStyle {
        match cfg.str("durations") {
            "compact" => crate::time::DurationStyle::Compact,
            "fixed" => crate::time::DurationStyle::Fixed,
            _ => self.durations,
        }
    }

    /// A duration in the module's style: `9m` (compact) or `9m00s` (fixed).
    #[must_use]
    pub fn duration(&self, cfg: &ModuleCfg, total_secs: u64) -> String {
        self.durations_for(cfg).format(total_secs)
    }

    /// Countdown from this tick's clock to an epoch-seconds instant in the
    /// module's style, or `None` once passed.
    #[must_use]
    pub fn countdown(&self, cfg: &ModuleCfg, until_epoch_secs: i64) -> Option<String> {
        self.durations_for(cfg).countdown_at(until_epoch_secs, self.now.as_second())
    }

    /// The wall-clock time of a future epoch-seconds instant in the tick's
    /// zone, in one of the [`crate::time::WallClock`] forms, the absolute
    /// twin of [`Ctx::countdown`] (SPEC § 3.3); `None` once passed, like it.
    #[must_use]
    pub fn wall_clock(&self, epoch_secs: i64, form: crate::time::WallClock) -> Option<String> {
        if epoch_secs <= self.now.as_second() {
            return None;
        }
        let at = Timestamp::from_second(epoch_secs).ok()?;
        Some(crate::time::wall_clock(at, &self.tz, form))
    }

    /// The animation frame (or scroll offset) at this tick: [`crate::time::frame`]
    /// of the tick's clock, or 0 when animations are off. Every moving part
    /// goes through here so `animate = false` and `GARNISH_ANIMATE=0` freeze
    /// all of them at once.
    #[must_use]
    pub fn frame(&self, step: f64, period: usize) -> usize {
        if self.animate { crate::time::frame(self.now, step, period) } else { 0 }
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

/// A trailing badge: a space and the icon in its own colour, or nothing when
/// the icon set (or an override) leaves that glyph empty.
///
/// The twin of [`icon`] for a glyph that follows the value — the dirty
/// marker, the exceeds-200k mark, a review state. Without the empty check a
/// dropped glyph leaves a lone space, which is a segment like any other: the
/// module gains a cell and `align = true` shifts the whole column.
#[must_use]
pub fn badge(cfg: &ModuleCfg, icon_key: &str, color_key: &str) -> Vec<Segment> {
    let glyph = cfg.icon(icon_key);
    if glyph.is_empty() { Vec::new() } else { vec![seg(cfg, format!(" {glyph}"), color_key)] }
}

/// The segment with its style dimmed (an overdue or failed value).
#[must_use]
pub const fn dimmed(mut segment: Segment) -> Segment {
    segment.style = segment.style.dimmed();
    segment
}

/// Dim, muted text.
#[must_use]
pub fn muted(theme: &Theme, text: impl Into<String>) -> Segment {
    Segment::styled(text, Style::fg(theme.role(crate::theme::Role::Muted)).dimmed())
}

/// Apply `label`, `prefix`, `suffix`, and staleness styling to a render.
///
/// An overdue or failed module always keeps its `⟳`/`✗` mark (SPEC § 3.6),
/// even when it had nothing to say: `sync` at the default preset is built
/// wholly from its cache entry, so a failed refresh leaves it with no
/// segments at all, and hiding it then would report a broken git as an
/// ordinary empty row. The placeholder `–` stands in for the value and the
/// mark still follows.
#[must_use]
pub fn decorate(
    rendered: Rendered,
    cfg: &ModuleCfg,
    theme: &Theme,
    stale_glyphs: (&str, &str),
) -> Vec<Segment> {
    let fresh = rendered.freshness == Freshness::Fresh;
    if rendered.is_empty() && fresh && cfg.hide_when_empty {
        return Vec::new();
    }
    // The wrapping is the same for every state; only the middle differs.
    let mut out: Vec<Segment> = Vec::new();
    if !cfg.prefix.is_empty() {
        out.push(Segment::plain(&cfg.prefix));
    }
    if !cfg.label.is_empty() {
        out.push(muted(theme, format!("{} ", cfg.label)));
    }
    let value = if rendered.is_empty() { vec![muted(theme, "–")] } else { rendered.segments };
    match &rendered.freshness {
        Freshness::Fresh => out.extend(value),
        Freshness::Stale => {
            out.extend(value.into_iter().map(dimmed));
            if !stale_glyphs.0.is_empty() {
                out.push(muted(theme, format!(" {}", stale_glyphs.0)));
            }
        }
        Freshness::Failed(_) => {
            out.extend(value.into_iter().map(dimmed));
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

    fn module_cfg(id: &str, overrides: &str) -> ModuleCfg {
        let text = format!("[modules.{id}]\n{overrides}");
        let (cfg, errs) = crate::config::parse(&text, &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        cfg.modules.get(id).cloned().unwrap_or_else(|| panic!("no module {id}"))
    }

    /// A glyph set to `""` drops the badge entirely. Two of the seven
    /// trailing-badge sites used to skip the check and emit a lone space,
    /// which is a segment like any other: the module kept a cell and
    /// `align = true` shifted the whole column.
    #[test]
    fn a_badge_with_no_glyph_takes_no_cell() {
        let cfg = module_cfg("branch", "[modules.branch.icons]\ndirty = \"\"\n");
        assert_eq!(badge(&cfg, "dirty", "dirty"), Vec::new());
        let cfg = module_cfg("branch", "");
        let marked = badge(&cfg, "dirty", "dirty");
        assert_eq!(marked.len(), 1);
        assert_eq!(marked.first().map(Segment::text), Some(&*format!(" {}", cfg.icon("dirty"))));
        // The same rule as the leading icon, which has always had it.
        assert_eq!(
            icon(&module_cfg("path", "[modules.path.icons]\nfolder = \"\"\n"), "folder", "icon"),
            Vec::new()
        );
    }

    /// SPEC § 3.6: an overdue or failed module keeps its `⟳`/`✗` mark even
    /// when it had nothing to show. `sync` at the default preset is built
    /// wholly out of its cache entry, so a failed refresh leaves it with no
    /// segments, and hiding it then reported a broken git as an empty row.
    #[test]
    fn a_failed_module_with_no_value_still_carries_its_mark() {
        let theme = Theme::default();
        let marks = ("⟳", "✗");
        let cfg = module_cfg("sync", "");
        assert!(cfg.hide_when_empty, "the default that used to swallow the mark");
        let text =
            |r: Rendered| crate::ansi::Painter::PLAIN.paint(&decorate(r, &cfg, &theme, marks));
        assert_eq!(text(Rendered::empty()), "", "a fresh empty module is still hidden");
        assert_eq!(
            text(Rendered { segments: Vec::new(), freshness: Freshness::Failed("boom".into()) }),
            "– ✗"
        );
        assert_eq!(text(Rendered { segments: Vec::new(), freshness: Freshness::Stale }), "– ⟳");
        // A module that did render keeps its value, dimmed, with the mark.
        let value =
            || Rendered { segments: vec![Segment::plain("⇡2")], freshness: Freshness::Stale };
        assert_eq!(text(value()), "⇡2 ⟳");
        assert!(decorate(value(), &cfg, &theme, marks).first().is_some_and(|s| s.style.dim));
    }

    /// The string literals that are direct arguments of the call starting at
    /// `open` (the byte after the `(`; literals inside a nested call such as
    /// `format!("…")` are skipped), and whether the last argument is one of
    /// them.
    fn literal_arguments(src: &str, open: usize) -> (Vec<String>, bool) {
        let mut depth = 1_usize;
        let mut in_str = false;
        let mut literals = Vec::new();
        let mut last_is_literal = false;
        let mut current = String::new();
        let Some(rest) = src.get(open..) else { return (literals, false) };
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            if in_str {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => {
                        in_str = false;
                        if depth == 1 {
                            literals.push(std::mem::take(&mut current));
                            last_is_literal = true;
                        }
                        current.clear();
                    }
                    _ => current.push(c),
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                '(' | '[' | '{' => depth = depth.saturating_add(1),
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return (literals, last_is_literal);
                    }
                }
                // An identifier or number after a literal means the literal
                // was not the last argument (`"x", y)`).
                c if depth == 1 && (c.is_alphanumeric() || c == '_' || c == '.') => {
                    last_is_literal = false;
                }
                _ => {}
            }
        }
        (literals, false)
    }

    /// SPEC § 9 schema completeness: every icon, colour and option key the
    /// render code reads by name (`cfg.icon("…")`, `seg(cfg, …, "…")`,
    /// `icon(cfg, "…", "…")`, `cfg.str("…")` and the other typed readers)
    /// exists in a schema of a module defined in that file (a file that
    /// defines none, like this one, is checked against every schema).
    /// `ModuleCfg` answers an unknown key with an empty icon or the default
    /// colour, so a typo would render silently; this scan of the module
    /// sources is what catches it.
    #[test]
    fn every_key_the_modules_read_is_in_a_schema() {
        let all: Vec<&ModuleSchema> =
            SCHEMAS.iter().chain(std::iter::once(&*text::SCHEMA)).collect();
        let keys_of = |schemas: &[&ModuleSchema]| -> std::collections::BTreeSet<&str> {
            schemas
                .iter()
                .flat_map(|s| {
                    s.opts
                        .iter()
                        .map(|o| o.key)
                        .chain(s.icons.iter().map(|i| i.key))
                        .chain(s.colors.iter().map(|c| c.key))
                })
                .collect()
        };
        let readers = [
            ".icon(\"",
            ".color(\"",
            ".str(\"",
            ".int(\"",
            ".size(\"",
            ".bool(\"",
            ".float(\"",
            ".nums(\"",
            ".strs(\"",
            ".value(\"",
            ".color_list(\"",
            ".icon_frames(\"",
        ];
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/modules");
        let mut seen = 0_usize;
        let mut unknown = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // Test modules render nothing (and this test quotes the patterns
            // it searches for).
            let src = std::fs::read_to_string(&path).unwrap();
            let src = src.split("#[cfg(test)]").next().unwrap().to_owned();
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            // The schemas a file of module implementations defines: those
            // whose id it quotes. A helper file (this one, `util.rs`) reads
            // on behalf of any module.
            let here: Vec<&ModuleSchema> = if src.contains("impl Module for") {
                all.iter().copied().filter(|s| src.contains(&format!("\"{}\"", s.id))).collect()
            } else {
                all.clone()
            };
            let known = keys_of(&here);
            // A read quoted in a comment (this test's own doc, say) is not one.
            let in_comment = |at: usize| {
                let line_start = src.get(..at).and_then(|s| s.rfind('\n')).map_or(0, |i| i + 1);
                src.get(line_start..).is_some_and(|l| l.trim_start().starts_with("//"))
            };
            let mut check = |key: &str, at: usize| {
                if in_comment(at) {
                    return;
                }
                seen += 1;
                if !known.contains(key) {
                    unknown.push(format!("{file}: {key:?} at byte {at}"));
                }
            };
            for reader in readers {
                for (at, _) in src.match_indices(reader) {
                    let rest = src.get(at + reader.len()..).unwrap();
                    let key = rest.split('"').next().unwrap();
                    check(key, at);
                }
            }
            // `seg(cfg, text, "color")`: the key is the last argument, when
            // that is a literal (`seg(cfg, x, key)` passes a variable).
            for (at, _) in src.match_indices("seg(") {
                let (literals, last_is_literal) = literal_arguments(&src, at + "seg(".len());
                if last_is_literal && let Some(key) = literals.last() {
                    check(key, at);
                }
            }
            // `icon(cfg, "icon", "color")`: both literals are keys.
            for call in [" icon(", "(icon("] {
                for (at, _) in src.match_indices(call) {
                    for key in literal_arguments(&src, at + call.len()).0 {
                        check(&key, at);
                    }
                }
            }
        }
        assert!(unknown.is_empty(), "keys read but not in any schema: {unknown:#?}");
        // A count well under what the sources hold today (≈ 200) means a
        // reader pattern went stale, not that the modules read less.
        assert!(seen > 180, "the scan found only {seen} reads; are the patterns stale?");
    }

    #[test]
    fn literal_arguments_follow_nesting_and_the_last_argument() {
        let src = "seg(cfg, format!(\"{} \", x), \"key\")";
        assert_eq!(literal_arguments(src, 4), (vec!["key".to_owned()], true));
        let src = "icon(cfg, \"branch\", \"icon\")";
        assert_eq!(literal_arguments(src, 5), (vec!["branch".to_owned(), "icon".to_owned()], true));
        assert_eq!(literal_arguments("seg(cfg, \"x\", key)", 4), (vec!["x".to_owned()], false));
        assert!(literal_arguments("seg(\n    cfg,\n    text,\n    \"k\",\n)", 4).1);
        // An escape is skipped whole; keys never carry one, the scan only
        // has to get past it.
        assert_eq!(literal_arguments("f(\"a\\\"b\")", 2).0, vec!["ab"]);
    }

    /// Why a glyph is unsafe for a built-in icon set, if it is.
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
    /// counted two, walkthrough bug 10). A Nerd Font's private-use glyphs
    /// (U+E000–F8FF) are designed for one cell, so the `nerd` set is checked
    /// only where it borrows a glyph from outside that range.
    #[test]
    fn built_in_glyphs_have_one_width_in_every_terminal() {
        let private_use = |g: &str| g.chars().all(|c| ('\u{e000}'..='\u{f8ff}').contains(&c));
        let mut offenders = Vec::new();
        for schema in SCHEMAS.iter() {
            for icon in &schema.icons {
                for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
                    let g = icon.glyph.get(set);
                    if set == IconSet::Nerd && private_use(g) {
                        continue;
                    }
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
        for good in ["█", "░", "▏", "▁▃▅▇█", "─", "╭", "⏱", "⚡", "🌿", "❖", "✓", "↻", "≣"]
        {
            assert_eq!(glyph_problem(good), None, "{good:?} must be accepted");
        }
    }

    /// The `fill`/`empty`/`marker` vocabulary belongs to the bar: every
    /// schema that declares one of those keys declares a one-cell glyph in
    /// every icon set, which is what lets the config reject a wider
    /// override outright (`IconSpec::one_cell`). Reusing one of the names
    /// for something that is not a bar cell fails here.
    #[test]
    fn bar_glyph_keys_are_one_cell_in_every_set() {
        use crate::config::schema::ONE_CELL_ICONS;
        let mut seen = 0_usize;
        for schema in SCHEMAS.iter() {
            for icon in schema.icons.iter().filter(|i| i.one_cell()) {
                seen = seen.saturating_add(1);
                for set in IconSet::ALL {
                    let g = icon.glyph.get(set);
                    assert_eq!(
                        crate::ansi::display_width(g),
                        1,
                        "{}.{} in {}: {g:?}",
                        schema.id,
                        icon.key,
                        set.name()
                    );
                }
            }
        }
        assert!(seen >= ONE_CELL_ICONS.len(), "the bar keys vanished from the schemas: {seen}");
    }
}
