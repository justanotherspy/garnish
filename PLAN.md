# PLAN.md — implementation plan and progress

Check items off as they land. Keep the **Session log** at the bottom current;
it is how the next session knows where to resume. Spec: `SPEC.md`. Rules:
`CLAUDE.md`.

## Phase 0 — Environment & scaffold

- [x] Install nightly toolchain + rustfmt/clippy/rust-analyzer/rust-src
- [x] `cargo new garnish`, `rust-toolchain.toml` (rolling nightly)
- [x] Strict lints in `Cargo.toml`, `clippy.toml`, `rustfmt.toml`, release profile
- [x] `.config/nextest.toml`, `Makefile`, `.gitignore`
- [x] Dependencies: clap, color-eyre, itertools, rayon, serde, serde_json, toml, jiff, command-run, unicode-width; dev: criterion, tempfile
- [x] `hyperfine` installed
- [x] `cargo-nextest` installed (nightly)
- [x] CLAUDE.md, SPEC.md, PLAN.md, README.md
- [x] `scripts/ci.sh`
- [x] Scaffold compiles under strict lints; `make check` green
- [x] First commit; sprite checkpoint

## Phase 1 — Payload, time, ANSI, preview

- [x] `payload.rs`: serde model of the stdin JSON, all optional fields `Option`
- [x] `tests/fixtures/payloads/`: subscription-full, api-key, pre-first-response, no-git, worktree-session, git-worktree, pr-approved/pending/changes/draft/mr/absent, spend-limit, fast-mode, ctx-1m-3/50/80/96, ctx-200k, vim, agent, session-name, no-effort, exceeds-200k
- [x] `time.rs`: `now()` honoring `GARNISH_NOW`; duration/countdown formatting (`1h12m`, `3d4h`, `47m`)
- [x] `ansi.rs`: styles, 256/truecolor, OSC 8, display width, ANSI-aware truncation with `…`
- [x] `num.rs`: saturating float→int helpers (no `as`)
- [x] `garnish preview <fixture|--all>` and golden-test harness (`UPDATE_GOLDEN=1`)
- [x] Unit tests for time/ansi/num

## Phase 2 — Schema, config, layout, frame

- [x] `config/schema.rs`: `Opt`, `IconOpt`, `ColorOpt`, `ModuleSchema`, preset tables
- [x] `config/mod.rs`: top-level model, `[frame]`, `[[line]]` (left `modules` + `right`), `[modules.<id>]`, resolution order, validation with TOML paths
- [x] Icon sets: nerd, unicode, emoji, ascii
- [x] Themes: garnish, catppuccin-mocha, nord, dracula, tokyonight, mono; role overrides
- [x] `frame.rs`: none/rounded/square/double/heavy/powerline/custom; fill to `$COLUMNS`; overflow rules
- [x] `garnish config init|check|path|show`
- [x] Top-level presets: default, minimal, full, compact
- [~] Config fixtures + matrix test — in-process matrix test exists (`render::tests`); TOML config fixtures still to add

## Phase 3 — Payload-only modules

- [x] `model`, `effort`, `style`
- [x] `context` (+ `claude_settings.rs` autocompact resolution, `exceeds_200k`, `warn_at`)
- [x] `limit5h`, `limit7d`, `spend`, `cost`
- [x] `session`, `api`, `cache`, `clock` (jiff, spinner)
- [x] `session_name`, `vim`, `agent`, `lines`
- [x] Unit + golden tests per module × preset × icon set × theme
- [x] Failure rendering (`⚠` lines)
- [ ] `GARNISH_DEBUG` log

## Phase 4 — Cache & workers

- [ ] `cache.rs`: root resolution, entry format, atomic write, TTL, GC
- [ ] `spawn.rs`: detached worker (`process_group(0)`), lock files, `GARNISH_NO_SPAWN`
- [ ] `garnish refresh --module|--all` (rayon for `--all`)
- [ ] Tests (serial group): TTL expiry, live lock, stale lock/dead pid, tmp/truncated ignored, tick killed mid-run, 32 concurrent ticks → one worker, GC bounds
- [ ] Sprite checkpoint

## Phase 5 — Repo modules

- [ ] `git.rs`: direct `.git` reads (HEAD, loose refs, packed-refs, worktree gitdir, upstream from config), reftable detection
- [ ] Worker: ahead/behind (`rev-list --left-right --count`), dirty (`status --porcelain=v2`, 2 s timeout), opt-in `git fetch` with `fetch_interval`
- [ ] `path`, `branch`, `sync`, `worktree`, `pr`
- [ ] Temp-repo tests (ahead/behind/diverged/no-upstream/detached/dirty/worktree) + PATH shim tests (slow/failing git never blocks a tick)

## Phase 6 — Docs

- [ ] `docs.rs` + `garnish docs`: `docs/README.md`, `docs/config.md`, `docs/modules/<id>.md` with preset renders
- [ ] `docs/guide.md` (hand-written)
- [ ] `garnish modules`
- [ ] Docs-sync test; `scripts/ci.sh` fails on drift
- [ ] Sprite checkpoint

## Phase 7 — Install, doctor, polish

- [ ] `garnish install` (settings merge + backup, `--absolute`, PATH warning, default config)
- [ ] `garnish doctor`, `garnish gc`
- [ ] Theme/icon polish across all four icon sets; stale styling; `examples/garnish.toml`

## Phase 8 — Performance

- [ ] `benches/tick.rs` (criterion): parse, resolve, render per module, in-process tick
- [ ] `bench/run.sh` + `bench/check.py` hyperfine gate (warm default, warm full, cold, refresh)
- [ ] `--time` per-phase timing flag
- [ ] Profile and optimize until green: mean < 3 ms, p99 < 8 ms, cold < 30 ms

## Phase 9 — Hardening & release

- [ ] Garbage/missing payloads, extreme COLUMNS, non-UTF8 paths, macOS path fallbacks
- [ ] Final `scripts/ci.sh` green; `cargo doc --no-deps` clean
- [ ] Tag `v0.1.0` locally; sprite checkpoint

## Session log

- **2026-09-04** — Research (statusline contract, autocompact internals,
  namtao toolkit), spec and plan approved. Phase 0 done: nightly
  1.100.0 (2026-09-03) installed, project scaffolded, deps added, docs written.
  Phases 1–3 done in one pass: payload/time/ansi/num, schema-driven config with
  presets/themes/icon sets/frames, all 21 modules registered (branch/sync are
  payload-only stubs until Phase 5), render pipeline, docs generator, golden
  suite (416 renders). Deviation from SPEC: role overrides live under
  `[colors]`, not `[theme.colors]` (TOML cannot have `theme` be both a string
  and a table). Next: adversarial review of phases 1–3, then Phase 4 cache/workers.
