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

fn render_modules(c: &mut Criterion) {
    let payload = Payload::parse(PAYLOAD).unwrap_or_default();
    let (cfg, _) = config::parse_with(
        "",
        &SCHEMAS,
        &Overlay { preset: Some(config::presets::TopPreset::Full), ..Default::default() },
    );
    let cache = garnish::cache::Cache::at(std::env::temp_dir().join("garnish-bench-cache"));
    let clock = Clock::fixed();
    let ctx = Ctx {
        payload: &payload,
        theme: &cfg.theme,
        icons: cfg.icons,
        now: clock.now,
        width: 120,
        cache: &cache,
        tz: clock.tz.clone(),
        home: clock.home,
        settings_env: clock.settings_env,
        git: clock.git,
        dirs: std::cell::OnceCell::new(),
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
    let payload = Payload::parse(PAYLOAD).unwrap_or_default();
    let (cfg, _) = config::parse("", &SCHEMAS);
    let clock = Clock::fixed();
    c.bench_function("tick_in_process_default", |b| {
        b.iter(|| render_lines_at(black_box(&payload), &cfg, Some(120), &clock));
    });
}

criterion_group!(benches, parse_payload, resolve_config, render_modules, tick_in_process);
criterion_main!(benches);
