//! The module system: the trait every module implements, the registry of the
//! fixed built-in set, the render context, and shared rendering helpers.

use std::sync::LazyLock;

use jiff::Timestamp;

use std::collections::BTreeMap;

use crate::ansi::{Segment, Style};
use crate::cache::{Cache, Entry as CacheEntry, LockOutcome, Lookup, Scope};
use crate::config::format::{CostStyle, FormatCfg, ParensStyle, PercentStyle, TokenStyle};
use crate::config::schema::{HideRule, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::IconSet;
use crate::payload::Payload;
use crate::theme::Theme;

pub mod badges;
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

/// In which presets a module shows its leading icon: the three shapes its
/// `show_icon` option takes ([`show_icon_opt`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconShown {
    /// On, and off in `minimal`: nearly every module.
    ExceptMinimal,
    /// Off, and on in `full`: `vim` and `version`, whose value is the badge.
    OnlyFull,
    /// On in every preset: the settings badges, whose glyph is the value.
    Always,
}

/// The `show_icon` option [`lead`] reads, in one of the [`IconShown`]
/// shapes; `doc` says which icon, so each module's reference names its own.
#[must_use]
pub fn show_icon_opt(doc: &'static str, shown: IconShown) -> OptSpec {
    let spec =
        OptSpec::new("show_icon", Kind::Bool, doc, Value::Bool(shown != IconShown::OnlyFull));
    match shown {
        IconShown::ExceptMinimal => spec.minimal(Value::Bool(false)),
        IconShown::OnlyFull => spec.full(Value::Bool(true)),
        IconShown::Always => spec,
    }
}

/// The kinds of number a module prints, each with a `[format]` style and a
/// per-module override of the same name (SPEC § 4, Number formats).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    /// Token counts (`tokens`).
    Tokens,
    /// Percentages (`percent`).
    Percent,
    /// Money (`cost`).
    Cost,
}

/// Choices of the per-module `tokens` option; `inherit` follows `[format]`.
pub const TOKEN_CHOICES: &[&str] = &["inherit", "compact", "precise", "whole"];
/// Choices of the per-module `percent` option; `inherit` follows `[format]`.
pub const PERCENT_CHOICES: &[&str] = &["inherit", "whole", "precise"];
/// Choices of the per-module `cost` option; `inherit` follows `[format]`.
pub const COST_CHOICES: &[&str] = &["inherit", "precise", "whole"];

/// The per-module override of one `[format]` style, carried by every module
/// that prints that kind of number, so one module can be pinned while the
/// rest follow the table (the shape of [`durations_opt`]).
#[must_use]
pub fn format_opt(kind: NumberKind) -> OptSpec {
    match kind {
        NumberKind::Tokens => OptSpec::new(
            "tokens",
            Kind::Enum(TOKEN_CHOICES),
            "How this module's token counts print: `inherit` follows `[format] tokens`; `compact` (128k, 1.0M), `precise` (128,400) or `whole` (128400) pins this module.",
            Value::Str("inherit".into()),
        ),
        NumberKind::Percent => OptSpec::new(
            "percent",
            Kind::Enum(PERCENT_CHOICES),
            "How this module's percentages print: `inherit` follows `[format] percent`; `whole` (42%) or `precise` (42.3%) pins this module.",
            Value::Str("inherit".into()),
        ),
        NumberKind::Cost => OptSpec::new(
            "cost",
            Kind::Enum(COST_CHOICES),
            "How this module's amounts print: `inherit` follows `[format] cost`; `precise` ($1.23, `decimals` places) or `whole` ($1) pins this module.",
            Value::Str("inherit".into()),
        ),
    }
}

/// How fresh a module's data is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Freshness {
    /// Rendered from live data (payload) or a cache entry within its TTL.
    #[default]
    Fresh,
    /// Rendered from a cache entry past its TTL; a refresh is under way.
    Stale,
    /// The last refresh failed. The message is not carried: `doctor`
    /// re-reads the `err` entries from disk, and a row shows only the mark.
    Failed,
}

/// The number a module's output measures this tick (SPEC § 3).
///
/// Attached to the [`Rendered`] so the render loop can apply the module's
/// `hide` list without the module spelling the rule: a count, an amount,
/// or the percentage the row prints (rounded as printed, so `below:50`
/// reads the same number the eye does).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measure {
    /// A count (lines changed, commits ahead and behind).
    Count(u64),
    /// An amount of money in dollars, rounded as the row prints it
    /// ([`Ctx::dollars_shown`]), so `zero` is what reads as zero.
    Amount(f64),
    /// The percentage the row prints.
    Percent(f64),
}

/// A module's output for one tick.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rendered {
    /// Segments, in order. Empty means "nothing to show".
    pub segments: Vec<Segment>,
    /// Data freshness.
    pub freshness: Freshness,
    /// What the output measures, for the `hide` list; `None` when the
    /// module has no count, amount or percentage this tick.
    pub measure: Option<Measure>,
}

impl Rendered {
    /// Nothing to show.
    #[must_use]
    pub const fn empty() -> Self {
        Self { segments: Vec::new(), freshness: Freshness::Fresh, measure: None }
    }

    /// Fresh segments.
    #[must_use]
    pub const fn fresh(segments: Vec<Segment>) -> Self {
        Self { segments, freshness: Freshness::Fresh, measure: None }
    }

    /// The output with its measure attached (SPEC § 3), the one call a
    /// module makes so its `hide` list can be applied by the render loop.
    #[must_use]
    pub fn measured(mut self, measure: impl Into<Option<Measure>>) -> Self {
        self.measure = measure.into();
        self
    }

    /// True when there is nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.text().is_empty())
    }
}

/// Whether a module's `hide` list takes its output off the row this tick
/// (SPEC § 3).
///
/// `zero` matches a count of none or an amount that prints as zero (the
/// amount arrives rounded as printed: `$0.00` under two decimals, `$0`
/// under `cost = "whole"`), `below:N` and `above:N` match the percentage
/// the row prints. `empty` is [`decorate`]'s business, through
/// [`ModuleCfg::hides_empty`], since it is about having nothing to print.
#[must_use]
pub fn hidden_by(rendered: &Rendered, rules: &[HideRule]) -> bool {
    rules.iter().any(|rule| match (rule, rendered.measure) {
        (HideRule::Zero, Some(Measure::Count(n))) => n == 0,
        // Rounded as printed, so zero is exact and nothing prints below it.
        (HideRule::Zero, Some(Measure::Amount(a))) => a <= 0.0,
        (HideRule::Below(n), Some(Measure::Percent(p))) => p < *n,
        (HideRule::Above(n), Some(Measure::Percent(p))) => p > *n,
        _ => false,
    })
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
    /// How numbers print (`[format]`, SPEC § 4).
    pub format: FormatCfg,
    /// Whether animations advance with the clock (SPEC § 4.2); off, every
    /// [`Ctx::frame`] is 0.
    pub animate: bool,
    /// The repository for the payload's directory, discovered at most once.
    pub dirs: std::cell::OnceCell<Option<crate::git::Dirs>>,
    /// That repository's `HEAD`, read at most once, so `branch` and `sync`
    /// agree on it even while a checkout rewrites the file.
    pub head: std::cell::OnceCell<Option<crate::git::Head>>,
    /// Claude Code's settings files this tick may read (SPEC § 2.3, § 4.2),
    /// highest precedence first, each with its `doctor` label: the chain of
    /// the directory Claude Code was launched in (not whatever subdirectory
    /// the session moved to) and the user's; empty for a pinned render,
    /// which reads no settings file.
    pub settings_chain: Vec<(&'static str, std::path::PathBuf)>,
    /// The keys of those files, read at most once per tick (a pinned
    /// render may seed them, `Clock.settings_keys`).
    pub settings: std::cell::OnceCell<Vec<crate::claude_settings::FileKeys>>,
    /// Whether a cached module may look its entry up and spawn a worker
    /// (`Clock.workers`). Off for docs, goldens and the in-process matrices,
    /// so a pinned render never touches a cache directory (SPEC § 9); a
    /// cached module then renders as if its worker had not run yet.
    pub workers: bool,
    /// The config file the tick loaded (absolute), passed to every worker
    /// it spawns so both read the same options (`Clock.config_file`).
    pub config_file: Option<std::path::PathBuf>,
}

impl Ctx<'_> {
    /// The keys of the settings chain, read on first use and shared by
    /// every reader on the tick (the autocompact marker, reduced motion,
    /// the `sandbox` and `voice` badges).
    #[must_use]
    pub fn settings(&self) -> &[crate::claude_settings::FileKeys] {
        self.settings.get_or_init(|| crate::claude_settings::read_keys(&self.settings_chain))
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
        crate::time::DurationStyle::parse(cfg.str("durations")).unwrap_or(self.durations)
    }

    /// A duration in the module's style: `9m` (compact) or `9m00s` (fixed).
    #[must_use]
    pub fn duration(&self, cfg: &ModuleCfg, total_secs: u64) -> String {
        self.durations_for(cfg).format(total_secs)
    }

    /// A token count in the module's style: its `tokens` option unless that
    /// is `inherit`, then `[format] tokens` (SPEC § 4, Number formats).
    #[must_use]
    pub fn tokens(&self, cfg: &ModuleCfg, n: u64) -> String {
        TokenStyle::parse(cfg.str("tokens")).unwrap_or(self.format.tokens).format(n)
    }

    /// A percentage held to `0..=100` in the module's style: its `percent`
    /// option unless that is `inherit`, then `[format] percent`.
    #[must_use]
    pub fn percent(&self, cfg: &ModuleCfg, p: f64) -> String {
        self.percent_with(cfg, p, true)
    }

    /// [`Ctx::percent`], held to `0..=100` only when `clamp` says so: `spend`
    /// prints a number that may pass 100 (SPEC § 3.3).
    #[must_use]
    pub fn percent_with(&self, cfg: &ModuleCfg, p: f64, clamp: bool) -> String {
        self.percent_style(cfg).format(p, clamp)
    }

    /// The number [`Ctx::percent`] prints, as a number: what a band
    /// threshold and a `below:N` / `above:N` rule compare, so they agree
    /// with the printed value at the boundaries whatever the style (SPEC
    /// § 3, § 4).
    #[must_use]
    pub fn percent_shown(&self, cfg: &ModuleCfg, p: f64) -> f64 {
        self.percent_shown_with(cfg, p, true)
    }

    /// The number [`Ctx::percent_with`] prints, as a number.
    #[must_use]
    pub fn percent_shown_with(&self, cfg: &ModuleCfg, p: f64, clamp: bool) -> f64 {
        self.percent_style(cfg).shown(p, clamp)
    }

    fn percent_style(&self, cfg: &ModuleCfg) -> PercentStyle {
        PercentStyle::parse(cfg.str("percent")).unwrap_or(self.format.percent)
    }

    /// An amount in the module's style: its `cost` option unless that is
    /// `inherit`, then `[format] cost`; `decimals` is read by `precise`.
    #[must_use]
    pub fn dollars(&self, cfg: &ModuleCfg, usd: f64, decimals: usize) -> String {
        self.cost_style(cfg).format(usd, decimals)
    }

    /// The amount [`Ctx::dollars`] prints, as a number: what `zero` in a
    /// `hide` list reads (SPEC § 3).
    #[must_use]
    pub fn dollars_shown(&self, cfg: &ModuleCfg, usd: f64, decimals: usize) -> f64 {
        self.cost_style(cfg).shown(usd, decimals)
    }

    fn cost_style(&self, cfg: &ModuleCfg) -> CostStyle {
        CostStyle::parse(cfg.str("cost")).unwrap_or(self.format.cost)
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

    /// Where [`Ctx::git_dirs`]'s `HEAD` points, read on first use: `None`
    /// outside a repository and where the file cannot be read (a reftable
    /// repository, a refused link).
    #[must_use]
    pub fn git_head(&self) -> Option<&crate::git::Head> {
        self.head.get_or_init(|| self.git_dirs().and_then(crate::git::head)).as_ref()
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
        if !self.workers {
            // A pinned render: no entry, nothing overdue, no worker.
            let lookup = Lookup { entry: None, fresh: true, in_progress: false };
            return (lookup, Freshness::Fresh);
        }
        let ttl_ms = cfg.refresh.saturating_mul(1000);
        let mut lookup = self.cache.lookup(scope, cfg.id, ttl_ms);
        let mismatched = lookup.entry.as_ref().is_some_and(|e| !valid(e));
        if mismatched && lookup.fresh {
            // `lookup` reads the lock only for an entry that is not fresh.
            lookup.fresh = false;
            lookup.in_progress = self.cache.lock_is_live(&self.cache.lock_path(scope, cfg.id));
        }
        let failed = lookup.entry.as_ref().filter(|e| e.status == crate::cache::Status::Err);
        if lookup.fresh {
            let freshness = if failed.is_some() { Freshness::Failed } else { Freshness::Fresh };
            return (lookup, freshness);
        }
        if !lookup.in_progress {
            self.spawn_refresh(cfg, scope);
        }
        let grace_ms = ttl_ms.saturating_mul(u64::from(self.stale_after.max(1)));
        let overdue = mismatched || lookup.entry.as_ref().is_none_or(|e| !e.is_fresh(grace_ms));
        let freshness = match failed {
            Some(_) => Freshness::Failed,
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
            config: self.config_file.clone(),
        };
        if cfg!(target_os = "linux") {
            match self.cache.lock(scope, cfg.id) {
                LockOutcome::Acquired(mut guard) => {
                    match crate::spawn::spawn(&job, self.cache.root(), true) {
                        crate::spawn::Spawned::Process | crate::spawn::Spawned::Logged => {
                            guard.disarm();
                        }
                        crate::spawn::Spawned::Failed(e) => {
                            crate::debug::log(&format!("spawn {} failed: {e}", cfg.id));
                        }
                    }
                }
                LockOutcome::Held => {}
                // A cache on a filesystem without hard links: nothing can
                // lock there, which `doctor` also reports.
                LockOutcome::Unavailable(e) => {
                    crate::debug::log(&format!("lock {} unavailable: {e}", cfg.id));
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
    /// Render for one tick. Must be cheap: never a process, and no I/O
    /// beyond small reads (a cache entry, the `.git` files the repo group
    /// reads, the settings chain through [`Ctx::settings`], once a tick).
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

/// Record that a worker could not take its module's lock, as a failed
/// entry, and return it.
///
/// A cache on a filesystem without hard links (exFAT, some SMB mounts)
/// refuses every lock, and a worker that gave up without a word left no
/// entry, so the next tick spawned another: one failing process per tick.
/// An entry needs only a rename, so it can still be written: the row shows
/// `✗`, `doctor` names the cause, and the TTL spaces the retries.
///
/// # Errors
/// Propagates cache write errors.
pub fn record_lock_failure(
    module: &dyn Module,
    ctx: &RefreshCtx<'_>,
    error: &std::io::Error,
) -> std::io::Result<CacheEntry> {
    let scope = module.scope(ctx.session, ctx.cwd);
    let ttl_ms = ctx.cfg.refresh.saturating_mul(1000);
    let root = ctx.cache.root().display();
    let entry = CacheEntry::err(ttl_ms, format!("cannot take the lock under {root}: {error}"));
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
        Box::new(identity::VersionModule),
        Box::new(badges::SandboxModule),
        Box::new(badges::VoiceModule),
        Box::new(badges::AccountModule),
    ]
}

/// Look up a registry entry by module id.
#[must_use]
pub fn entry(id: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.schema.id == id)
}

/// A styled text segment using a module color key.
#[must_use]
pub fn seg(cfg: &ModuleCfg, text: impl Into<String>, color_key: &str) -> Segment {
    Segment::styled(text, Style::fg(cfg.color(color_key)))
}

/// The leading glyph in the `icon` colour followed by `after`, or nothing
/// when `show_icon` is off or the glyph is blank: [`lead`] and
/// [`lead_only`] differ only in `after`.
fn leading(cfg: &ModuleCfg, icon_key: &str, after: &str) -> Vec<Segment> {
    let glyph = cfg.icon(icon_key);
    if !cfg.bool("show_icon") || glyph.is_empty() {
        return Vec::new();
    }
    vec![Segment::styled(format!("{glyph}{after}"), Style::fg(cfg.color("icon")))]
}

/// A module's leading icon: its `show_icon` option ([`show_icon_opt`]) and
/// its `icon` colour, then the space before the value, which is how every
/// module with a leading icon opens.
///
/// The one place the option and the colour key are spelled, so a module
/// cannot quietly ignore `show_icon` or reach for a different colour.
#[must_use]
pub fn lead(cfg: &ModuleCfg, icon_key: &str) -> Vec<Segment> {
    leading(cfg, icon_key, " ")
}

/// The leading glyph as a module's whole value (`sandbox` and `voice`
/// under `style = "glyph"`, SPEC § 3.8).
///
/// [`lead`] without the space that separates an icon from the value after
/// it, so the badge is one cell and `align = true` counts it as one.
#[must_use]
pub fn lead_only(cfg: &ModuleCfg, icon_key: &str) -> Vec<Segment> {
    leading(cfg, icon_key, "")
}

/// A trailing badge: a space and the icon in its own colour, or nothing when
/// the icon set (or an override) leaves that glyph empty.
///
/// The twin of [`lead`] for a glyph that follows the value — the dirty
/// marker, the exceeds-200k mark, a review state. Without the empty check a
/// dropped glyph leaves a lone space, which is a segment like any other: the
/// module gains a cell and `align = true` shifts the whole column.
#[must_use]
pub fn badge(cfg: &ModuleCfg, icon_key: &str, color_key: &str) -> Vec<Segment> {
    let glyph = cfg.icon(icon_key);
    if glyph.is_empty() { Vec::new() } else { vec![seg(cfg, format!(" {glyph}"), color_key)] }
}

/// A glyph and the space after it, ready to be interpolated before text, or
/// the empty string when that glyph is blank.
///
/// [`badge`] covers a glyph that is a segment of its own; this covers the
/// other shape, a glyph built into a longer string (`⚡ 1h`). It exists for
/// the same reason: written by hand the emptiness check gets forgotten, and
/// the leftover space is a cell that shifts an aligned column.
#[must_use]
pub fn glyph_prefix(cfg: &ModuleCfg, icon_key: &str) -> String {
    let glyph = cfg.icon(icon_key);
    if glyph.is_empty() { String::new() } else { format!("{glyph} ") }
}

/// The segment with its style dimmed (an overdue or failed value).
const fn dimmed(mut segment: Segment) -> Segment {
    segment.style = segment.style.dimmed();
    segment
}

/// Dim, muted text.
#[must_use]
pub fn muted(theme: &Theme, text: impl Into<String>) -> Segment {
    Segment::styled(text, Style::fg(theme.role(crate::theme::Role::Muted)).dimmed())
}

/// A parenthesised detail after a value (`api`'s share, `lines`' net, the
/// `both` reset form's time), drawn as `[format] parens` says (SPEC § 4).
///
/// `plain` is one segment, `before (inner)` in the module's colour, so a
/// config that leaves the default renders byte for byte as it always has;
/// `dim` is `before` in that colour and ` (inner)` in the muted role, the
/// way a `label` is drawn. `before` carries its own leading space, as the
/// segment it replaces did, and may be empty.
#[must_use]
pub fn detail(
    ctx: &Ctx<'_>,
    cfg: &ModuleCfg,
    before: &str,
    inner: &str,
    color_key: &str,
) -> Vec<Segment> {
    match ctx.format.parens {
        ParensStyle::Plain => vec![seg(cfg, format!("{before} ({inner})"), color_key)],
        ParensStyle::Dim => {
            let mut out: Vec<Segment> = Vec::new();
            if !before.is_empty() {
                out.push(seg(cfg, before, color_key));
            }
            out.push(muted(ctx.theme, format!(" ({inner})")));
            out
        }
    }
}

/// Apply `label`, `prefix`, `suffix`, and staleness styling to a render.
///
/// A *failed* module keeps its `✗` (SPEC § 3.6) even when it had nothing to
/// say: `sync` at the default preset is built wholly from its cache entry,
/// so a failed refresh leaves it with no segments at all, and hiding it
/// then would report a broken git as an ordinary empty row. The icon set's
/// placeholder (`–`, `-` in the ascii set) stands in for the value and the
/// mark follows it.
///
/// An *overdue* module with nothing to say still hides, because its last
/// value really was nothing and a `– ⟳` would flicker in every idle pause.
/// Only a value that exists is dimmed and marked with `⟳`.
///
/// Every mark comes from `icons` ([`IconSet::placeholder`],
/// [`IconSet::stale_glyphs`]), so an ascii-only row stays ascii.
#[must_use]
pub fn decorate(
    rendered: Rendered,
    cfg: &ModuleCfg,
    theme: &Theme,
    icons: IconSet,
) -> Vec<Segment> {
    if rendered.is_empty() && rendered.freshness != Freshness::Failed && cfg.hides_empty() {
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
    let value = if rendered.is_empty() {
        vec![muted(theme, icons.placeholder())]
    } else {
        rendered.segments
    };
    let (overdue, failed) = icons.stale_glyphs();
    match rendered.freshness {
        Freshness::Fresh => out.extend(value),
        Freshness::Stale => {
            out.extend(value.into_iter().map(dimmed));
            out.push(muted(theme, format!(" {overdue}")));
        }
        Freshness::Failed => {
            out.extend(value.into_iter().map(dimmed));
            out.push(Segment::styled(
                format!(" {failed}"),
                Style::fg(theme.role(crate::theme::Role::Danger)).dimmed(),
            ));
        }
    }
    if !cfg.suffix.is_empty() {
        out.push(Segment::plain(&cfg.suffix));
    }
    out
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

    /// A glyph set to `""` leaves no cell behind, in either shape a trailing
    /// glyph takes.
    ///
    /// Four of the nine trailing-badge sites used to skip the check (branch
    /// dirty, context exceeds, cache warm, cache cold) and emit a lone space,
    /// which is a segment like any other: the module kept a cell and
    /// `align = true` shifted the whole column. [`glyph_prefix`] is the same
    /// rule for a glyph built into a longer string, where the leftover was a
    /// *double* space (`cache`'s countdown arm, which the first pass at this
    /// missed because it fixed the example rather than the class).
    #[test]
    fn a_badge_with_no_glyph_takes_no_cell() {
        let cfg = module_cfg("branch", "[modules.branch.icons]\ndirty = \"\"\n");
        assert_eq!(badge(&cfg, "dirty", "dirty"), Vec::new());
        let cfg = module_cfg("branch", "");
        let marked = badge(&cfg, "dirty", "dirty");
        assert_eq!(marked.len(), 1);
        assert_eq!(marked.first().map(Segment::text), Some(&*format!(" {}", cfg.icon("dirty"))));
        // The same rule as the leading icon, which has always had it.
        let blank = module_cfg("path", "[modules.path.icons]\nfolder = \"\"\n");
        assert_eq!(lead(&blank, "folder"), Vec::new());
        assert_eq!(lead_only(&blank, "folder"), Vec::new());
        // The interpolated shape: glyph and its space, or nothing at all.
        let cfg = module_cfg("cache", "");
        assert_eq!(glyph_prefix(&cfg, "warm"), format!("{} ", cfg.icon("warm")));
        assert_eq!(
            glyph_prefix(&module_cfg("cache", "[modules.cache.icons]\nwarm = \"\"\n"), "warm"),
            ""
        );
    }

    /// SPEC § 3.6: a *failed* module keeps its `✗` even when it had nothing
    /// to show. `sync` at the default preset is built wholly out of its cache
    /// entry, so a failed refresh leaves it with no segments, and hiding it
    /// then reported a broken git as an empty row.
    ///
    /// An *overdue* one is the opposite case and still hides: its last value
    /// really was nothing (a repository in sync renders no segments), and
    /// `sync`'s 5 s TTL times the default `stale_after = 5` means every idle
    /// pause over 25 s would otherwise flash a `– ⟳` row until the worker
    /// lands. That is the flicker `stale_after` exists to remove.
    #[test]
    fn a_failed_module_with_no_value_still_carries_its_mark() {
        let theme = Theme::default();
        let marks = IconSet::Unicode;
        let cfg = module_cfg("sync", "");
        assert!(cfg.hide_when_empty, "the default that used to swallow the mark");
        let text =
            |r: Rendered| crate::ansi::Painter::PLAIN.paint(&decorate(r, &cfg, &theme, marks));
        let empty = |f: Freshness| Rendered { segments: Vec::new(), freshness: f, measure: None };
        assert_eq!(text(Rendered::empty()), "", "a fresh empty module is hidden");
        assert_eq!(text(empty(Freshness::Stale)), "", "so is an overdue one with no value");
        assert_eq!(text(empty(Freshness::Failed)), "– ✗", "a broken one is never silent");
        // mod-02: the ascii set's marks, the placeholder included, are ascii.
        let ascii = |r: Rendered| {
            crate::ansi::Painter::PLAIN.paint(&decorate(r, &cfg, &theme, IconSet::Ascii))
        };
        assert_eq!(ascii(empty(Freshness::Failed)), "- x");
        let shown = module_cfg("sync", "hide_when_empty = false\n");
        let plain = |r: Rendered, icons: IconSet| {
            crate::ansi::Painter::PLAIN.paint(&decorate(r, &shown, &theme, icons))
        };
        assert_eq!(plain(Rendered::empty(), IconSet::Ascii), "-");
        assert_eq!(plain(Rendered::empty(), IconSet::Nerd), "–");
        // A module that did render keeps its value, dimmed, with the mark.
        let value = || Rendered {
            segments: vec![Segment::plain("⇡2")],
            freshness: Freshness::Stale,
            measure: None,
        };
        assert_eq!(text(value()), "⇡2 ⟳");
        assert!(decorate(value(), &cfg, &theme, marks).first().is_some_and(|s| s.style.dim));
    }

    /// A worker that cannot lock (a cache on a filesystem without hard
    /// links) records why as a failed entry, which a rename alone can
    /// write: the row shows `✗`, `doctor` names it, and the TTL keeps the
    /// next tick from spawning another worker at once.
    #[test]
    fn cache_a_worker_that_cannot_lock_records_a_failed_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("cache"));
        let cfg = module_cfg("branch", "");
        let ctx = RefreshCtx { session: "s", cwd: dir.path(), cfg: &cfg, cache: &cache };
        let error = std::io::Error::other("Operation not permitted");
        let written = record_lock_failure(&repo::BranchModule, &ctx, &error).unwrap();
        let scope = repo::BranchModule.scope("s", dir.path());
        let read = cache.read(&scope, "branch").unwrap();
        assert_eq!(read, written);
        assert_eq!(read.status, crate::cache::Status::Err);
        assert!(
            read.error.contains("cannot take the lock") && read.error.contains("not permitted")
        );
        assert!(read.is_fresh(cfg.refresh.saturating_mul(1000)), "fresh for its TTL: no storm");
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
            // `seg(cfg, text, "color")` and `detail(ctx, cfg, before, inner,
            // "color")`: the key is the last argument, when that is a
            // literal (`seg(cfg, x, key)` passes a variable).
            for call in ["seg(", " detail(", "(detail("] {
                for (at, _) in src.match_indices(call) {
                    let (literals, last_is_literal) = literal_arguments(&src, at + call.len());
                    if last_is_literal && let Some(key) = literals.last() {
                        check(key, at);
                    }
                }
            }
            // `icon(cfg, "icon", "color")` and `badge(cfg, "icon", "color")`:
            // both literals are keys. The space or `(` before the name keeps
            // the pattern from matching inside another word.
            for call in [" icon(", "(icon(", " badge(", "(badge("] {
                for (at, _) in src.match_indices(call) {
                    for key in literal_arguments(&src, at + call.len()).0 {
                        check(&key, at);
                    }
                }
            }
            // `glyph_prefix(cfg, "icon")`: one icon key, no colour of its
            // own (the caller's `seg` carries that).
            for call in [" glyph_prefix(", "(glyph_prefix("] {
                for (at, _) in src.match_indices(call) {
                    for key in literal_arguments(&src, at + call.len()).0 {
                        check(&key, at);
                    }
                }
            }
            // `lead(cfg, "icon")` and `lead_only(cfg, "icon")` carry the
            // icon key and, implicitly, the `icon` colour every module's
            // leading glyph takes.
            for call in [" lead(", "(lead(", " lead_only(", "(lead_only("] {
                for (at, _) in src.match_indices(call) {
                    for key in literal_arguments(&src, at + call.len()).0 {
                        check(&key, at);
                    }
                    check("icon", at);
                }
            }
        }
        assert!(unknown.is_empty(), "keys read but not in any schema: {unknown:#?}");
        // A count well under what the sources hold today (≈ 200) means a
        // reader pattern went stale, not that the modules read less.
        assert!(seen > 180, "the scan found only {seen} reads; are the patterns stale?");
    }

    /// SPEC § 3: the `hide` list reads the measure a module attached; a
    /// module without one is never hidden by `zero`, `below` or `above`,
    /// and a count is never read as a percentage or the other way round.
    #[test]
    fn hide_rules_read_the_measure() {
        use crate::config::schema::HideRule::{Above, Below, Empty, Zero};
        let with = |m: Option<Measure>| Rendered::fresh(vec![Segment::plain("x")]).measured(m);
        assert!(hidden_by(&with(Some(Measure::Count(0))), &[Zero]));
        assert!(!hidden_by(&with(Some(Measure::Count(1))), &[Zero]));
        assert!(hidden_by(&with(Some(Measure::Amount(0.0))), &[Zero]));
        assert!(!hidden_by(&with(Some(Measure::Amount(0.01))), &[Zero]));
        assert!(!hidden_by(&with(Some(Measure::Amount(0.004))), &[Zero]), "rounded as printed");
        assert!(hidden_by(&with(Some(Measure::Percent(9.0))), &[Below(10.0)]));
        assert!(!hidden_by(&with(Some(Measure::Percent(10.0))), &[Below(10.0)]));
        assert!(hidden_by(&with(Some(Measure::Percent(91.0))), &[Above(90.0)]));
        assert!(!hidden_by(&with(Some(Measure::Percent(90.0))), &[Above(90.0)]));
        assert!(hidden_by(&with(Some(Measure::Percent(50.0))), &[Below(10.0), Above(40.0)]));
        assert!(!hidden_by(&with(Some(Measure::Percent(50.0))), &[Empty]));
        assert!(!hidden_by(&with(None), &[Zero, Below(100.0), Above(0.0)]));
        assert!(!hidden_by(&with(Some(Measure::Count(0))), &[Below(100.0)]));
        assert!(!hidden_by(&with(Some(Measure::Percent(0.0))), &[Zero]));
        assert!(!hidden_by(&with(Some(Measure::Count(0))), &[]));
        // `measured` takes the measure or an option of one.
        assert_eq!(Rendered::empty().measured(Measure::Count(2)).measure, Some(Measure::Count(2)));
        assert_eq!(Rendered::empty().measured(None).measure, None);
    }

    /// SPEC § 4 `parens`: a detail is one segment with its value under
    /// `plain`, so today's renders keep their bytes, and its own muted
    /// segment under `dim`; an empty `before` leaves only the detail.
    #[test]
    fn a_detail_is_one_segment_plain_and_two_dim() {
        let payload = Payload::parse("{\"session_id\": \"s\"}").unwrap();
        // The pinned clock turns workers off, so the cache is never touched.
        let cache = Cache::at(std::env::temp_dir().join("garnish-detail-test"));
        let (config, _) = crate::config::parse("", &SCHEMAS);
        let theme = config.theme.clone();
        let cfg = config.modules.get("api").unwrap();
        let clock = crate::render::Clock::fixed();
        let mut ctx = crate::render::context(&payload, &config, &clock, &cache, 80);
        assert_eq!(ctx.format, FormatCfg::default());
        let plain = detail(&ctx, cfg, " 8m20s", "12%", "share");
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].text(), " 8m20s (12%)");
        assert_eq!(plain[0].style, Style::fg(cfg.color("share")));
        let bare = detail(&ctx, cfg, "", "12%", "share");
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].text(), " (12%)");
        ctx.format.parens = ParensStyle::Dim;
        let dim = detail(&ctx, cfg, " 8m20s", "12%", "share");
        assert_eq!(dim.len(), 2);
        assert_eq!((dim[0].text(), dim[1].text()), (" 8m20s", " (12%)"));
        assert_eq!(dim[0].style, Style::fg(cfg.color("share")));
        assert_eq!(dim[1], muted(&theme, " (12%)"));
        let bare = detail(&ctx, cfg, "", "12%", "share");
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0], muted(&theme, " (12%)"));
        // The number styles resolve `inherit` to the table and a module
        // option pins its own.
        ctx.format.tokens = TokenStyle::Precise;
        assert_eq!(ctx.tokens(cfg, 128_400), "128,400");
        let (config, _) = crate::config::parse(
            "[modules.context]\ntokens = \"whole\"\npercent = \"precise\"\n[modules.cost]\ncost = \"whole\"\n",
            &SCHEMAS,
        );
        let context = config.modules.get("context").unwrap();
        assert_eq!(ctx.tokens(context, 128_400), "128400");
        assert_eq!(ctx.percent(context, 42.34), "42.3%");
        assert_eq!(ctx.percent(cfg, 42.34), "42%");
        assert_eq!(ctx.percent_with(cfg, 112.4, false), "112%");
        assert_eq!(ctx.percent_with(cfg, 112.4, true), "100%");
        assert_eq!(ctx.dollars(config.modules.get("cost").unwrap(), 1.2345, 2), "$1");
        assert_eq!(ctx.dollars(cfg, 1.2345, 2), "$1.23");
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

    /// SPEC § 14: the glyph picker's suggested alternatives pass the same
    /// guard as the sets, one or two cells by every table (a spinner
    /// suggestion is a string of one-cell frames), and every one names a
    /// module and key that exist.
    #[test]
    fn suggested_glyphs_pass_the_same_guard_as_the_sets() {
        let private_use = |g: &str| g.chars().all(|c| ('\u{e000}'..='\u{f8ff}').contains(&c));
        let mut seen = 0_usize;
        let mut offenders = Vec::new();
        for schema in SCHEMAS.iter() {
            for icon in &schema.icons {
                for glyph in crate::icons::suggestions(schema.id, icon.key) {
                    seen = seen.saturating_add(1);
                    // `all` is true of an empty string, and an empty
                    // suggestion once shipped that way.
                    assert!(!glyph.is_empty(), "{}.{}: an empty suggestion", schema.id, icon.key);
                    if private_use(glyph) {
                        continue;
                    }
                    let frames: Vec<String> = if icon.key == "spinner" {
                        glyph.chars().map(|c| c.to_string()).collect()
                    } else {
                        vec![(*glyph).to_owned()]
                    };
                    for frame in frames {
                        let problem = glyph_problem(&frame).map(str::to_owned).or_else(|| {
                            let cells = frame.width();
                            // A glyph repeated cell by cell is one cell, as
                            // the parser requires of an override.
                            let one = icon.key == "spinner"
                                || crate::config::schema::ONE_CELL_ICONS.contains(&icon.key);
                            let limit = if one { 1 } else { 2 };
                            (cells == 0 || cells > limit).then(|| format!("{cells} cells"))
                        });
                        if let Some(problem) = problem {
                            offenders
                                .push(format!("{}.{}: {frame:?} {problem}", schema.id, icon.key));
                        }
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "suggestions terminals disagree on:\n{}",
            offenders.join("\n")
        );
        assert!(seen >= 40, "{seen} suggestions");
        let none: [&str; 0] = [];
        assert_eq!(crate::icons::suggestions("nope", "model"), none);
        assert_eq!(crate::icons::suggestions("model", "nope"), none);
        // A suggestion for a key no schema declares would never be shown.
        for (module, key) in [("model", "model"), ("sync", "ahead"), ("clock", "spinner")] {
            assert!(
                SCHEMAS.iter().any(|s| s.id == module && s.icon(key).is_some()),
                "{module}.{key}"
            );
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
