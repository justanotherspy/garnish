//! Criterion micro-benchmarks for the in-process parts of a tick: payload
//! parsing, config resolution, each module's render, and the whole pipeline
//! without process start-up. `bench/run.sh` measures the end-to-end tick.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use garnish::config::{self, Overlay};
use garnish::modules::{Ctx, SCHEMAS};
use garnish::payload::Payload;
use garnish::render::{Clock, render_lines_at};

const PAYLOAD: &str = include_str!("../tests/fixtures/payloads/subscription-full.json");

fn parse_payload(c: &mut Criterion) {
    c.bench_function("parse_payload", |b| b.iter(|| Payload::parse(black_box(PAYLOAD))));
}

fn resolve_config(c: &mut Criterion) {
    let text = include_str!("../examples/garnish.toml");
    c.bench_function("resolve_config_defaults", |b| {
        b.iter(|| config::parse(black_box(""), &SCHEMAS));
    });
    c.bench_function("resolve_config_full_file", |b| {
        b.iter(|| config::parse(black_box(text), &SCHEMAS));
    });
}

/// The cache directory the whole bench uses.
///
/// Every render here names it through `Clock.cache`. `render_lines_at` used
/// to build its own from the environment, so the whole-tick benchmarks read
/// and wrote the *developer's* real cache and forked a detached worker on
/// every miss, which polluted it and made the timings depend on whatever
/// that machine had lying around.
fn bench_cache_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("garnish-bench-cache")
}

/// A payload whose directories are this checkout, and a cache seeded for the
/// repo modules, so `branch` and `sync` do the work a warm tick does.
///
/// `Clock::fixed()` has `git: false` and the fixture's `cwd` does not exist,
/// so `sync` used to bail at its first line and `branch` to skip `head`,
/// `head_commit` and its cache lookup: the only two modules that read files
/// were timed as function calls. The cache is seeded because an empty one
/// makes the lookup spawn a worker, which is the cold path (`bench/run.sh`
/// measures that one) and would fork a process per iteration here.
fn repo_payload_and_cache() -> (Payload, garnish::cache::Cache) {
    use garnish::cache::{Cache, Entry, Scope};
    let cwd = env!("CARGO_MANIFEST_DIR");
    let payload =
        Payload::parse(&PAYLOAD.replace("/home/dev/projects/garnish", cwd)).unwrap_or_default();
    let cache = Cache::at(bench_cache_dir());
    if let Some(dirs) = garnish::git::discover(std::path::Path::new(cwd)) {
        let scope = Scope::Repo(dirs.cache_key());
        let mut sync = std::collections::BTreeMap::new();
        sync.insert("ahead".to_owned(), "1".to_owned());
        sync.insert("behind".to_owned(), "0".to_owned());
        let _ = cache.write(&scope, "sync", &Entry::ok(60_000, sync));
        let mut branch = std::collections::BTreeMap::new();
        branch.insert("dirty".to_owned(), "1".to_owned());
        let _ = cache.write(&scope, "branch", &Entry::ok(60_000, branch));
    }
    (payload, cache)
}

fn render_modules(c: &mut Criterion) {
    let (payload, cache) = repo_payload_and_cache();
    let (cfg, _) = config::parse_with(
        "",
        &SCHEMAS,
        &Overlay { preset: Some(config::presets::TopPreset::Full), ..Default::default() },
    );
    let clock = Clock { git: true, ..Clock::fixed() };
    let ctx = Ctx {
        payload: &payload,
        theme: &cfg.theme,
        icons: cfg.icons,
        now: clock.now,
        width: cfg.width(Some(120)),
        cache: &cache,
        tz: clock.tz.clone(),
        home: clock.home,
        settings_env: clock.settings_env,
        git: clock.git,
        stale_after: cfg.stale_after,
        durations: cfg.durations,
        animate: clock.animate,
        dirs: std::cell::OnceCell::new(),
        settings_files: Vec::new(),
        settings: std::cell::OnceCell::new(),
    };
    let mut group = c.benchmark_group("render_module");
    for entry in garnish::modules::REGISTRY.iter() {
        let Some(mcfg) = cfg.modules.get(entry.schema.id) else { continue };
        group.bench_function(entry.schema.id, |b| {
            b.iter(|| entry.module.render(black_box(&ctx), mcfg));
        });
    }
    group.finish();
}

fn tick_in_process(c: &mut Criterion) {
    let (payload, _seeded) = repo_payload_and_cache();
    let (cfg, _) = config::parse("", &SCHEMAS);
    // Git on: the default preset carries the repo group, and reading `.git`
    // plus a cache entry is where a warm tick actually spends its time. The
    // cache is the seeded one, never the machine's.
    let clock = Clock { git: true, cache: Some(bench_cache_dir()), ..Clock::fixed() };
    c.bench_function("tick_in_process_default", |b| {
        b.iter(|| render_lines_at(black_box(&payload), &cfg, Some(120), &clock));
    });
    // `max_width` on every module: the cap measures and cuts each decorated
    // module before alignment, which the default tick skips entirely.
    let (capped, _) = config::parse(
        "preset = \"full\"\n[modules.path]\nmax_width = 20\n[modules.branch]\nmax_width = 20\n[modules.context]\nmax_width = 20\n[modules.model]\nmax_width = 20\n",
        &SCHEMAS,
    );
    c.bench_function("tick_in_process_max_width", |b| {
        b.iter(|| render_lines_at(black_box(&payload), &capped, Some(120), &clock));
    });
    // A full-preset row squeezed into 60 columns scrolls: the scroller's cost
    // on top of a plain overflow (which truncates).
    let (ticker, _) = config::parse(
        "preset = \"full\"\noverflow = \"ticker\"\n[[line]]\nmodules = [\"path\", \"model\", \"effort\", \"context\", \"limit5h\", \"limit7d\", \"session\", \"api\", \"cache\"]\nright = [\"clock\"]\n",
        &SCHEMAS,
    );
    let mut animated = Clock::fixed();
    animated.animate = true;
    animated.cache = Some(bench_cache_dir());
    c.bench_function("tick_in_process_ticker", |b| {
        b.iter(|| render_lines_at(black_box(&payload), &ticker, Some(60), &animated));
    });
}

/// The layout shapes of SPEC § 4.3 against the same warm tick: columns are
/// arithmetic over the segments the modules already rendered, so the cost of
/// a shape is the cost of the cells it draws, not of anything new being read.
fn tick_in_process_layout(c: &mut Criterion) {
    let (payload, _seeded) = repo_payload_and_cache();
    let clock = Clock { git: true, cache: Some(bench_cache_dir()), ..Clock::fixed() };
    let cases = [
        (
            "columns",
            "[[row]]\ngap = 2\n[[row.col]]\nmodules = [\"path\", \"branch\"]\n[[row.col]]\nmodules = [\"model\", \"effort\"]\n[[row.col]]\nmodules = [\"context\"]\n[[row.col]]\nmodules = [\"limit5h\"]\n[[row.col]]\nmodules = [\"session\"]\n[[row.col]]\nmodules = [\"clock\"]\n",
        ),
        (
            "boxes",
            "[box.repo]\ntitle = \"Repository\"\n[[row]]\nbox = \"repo\"\nmodules = [\"path\", \"branch\"]\nright = [\"clock\"]\n[[row]]\nbox = \"repo\"\nmodules = [\"model\", \"context\"]\n[[row]]\nbox = true\nmodules = [\"limit5h\", \"cost\"]\n",
        ),
        (
            "dashboard",
            "[frame]\nstyle = \"none\"\n[box.repo]\nstyle = \"double\"\ntitle = \"Repository\"\n[[row]]\ngap = 2\n[[row.col]]\nbox = \"repo\"\n[[row.col.row]]\nmodules = [\"path\", \"branch\"]\n[[row.col.row]]\nmodules = [\"model\", \"effort\"]\n[[row.col]]\njustify = \"center\"\n[[row.col.row]]\nmodules = [\"clock\"]\n[[row.col]]\njustify = \"center\"\n[[row.col.row]]\nbox = true\nmodules = [\"context\"]\n[[row.col.row]]\nbox = true\nmodules = [\"limit5h\"]\n[[row.col.row]]\nbox = true\nmodules = [\"cost\"]\n",
        ),
    ];
    for (name, text) in cases {
        let (cfg, _) = config::parse(text, &SCHEMAS);
        c.bench_function(&format!("tick_in_process_{name}"), |b| {
            b.iter(|| render_lines_at(black_box(&payload), &cfg, Some(120), &clock));
        });
    }
}

criterion_group!(
    benches,
    parse_payload,
    resolve_config,
    render_modules,
    tick_in_process,
    tick_in_process_layout
);
criterion_main!(benches);
