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
- [ ] `cargo-nextest` installed (nightly)
- [x] CLAUDE.md, SPEC.md, PLAN.md, README.md
- [ ] `scripts/ci.sh`
- [ ] Scaffold compiles under strict lints; `make check` green
- [ ] First commit; sprite checkpoint

## Phase 1 — Payload, time, ANSI, preview

- [ ] `payload.rs`: serde model of the stdin JSON, all optional fields `Option`
- [ ] `tests/fixtures/payloads/`: subscription-full, api-key, pre-first-response, no-git, worktree-session, git-worktree, pr-approved/pending/changes/draft/mr/absent, spend-limit, fast-mode, ctx-1m-3/50/80/96, ctx-200k, vim, agent, session-name, no-effort, exceeds-200k
- [ ] `time.rs`: `now()` honoring `GARNISH_NOW`; duration/countdown formatting (`1h12m`, `3d4h`, `47m`)
- [ ] `ansi.rs`: styles, 256/truecolor, OSC 8, display width, ANSI-aware truncation with `…`
- [ ] `num.rs`: saturating float→int helpers (no `as`)
- [ ] `garnish preview <fixture|--all>` and golden-test harness (`UPDATE_GOLDEN=1`)
- [ ] Unit tests for time/ansi/num

## Phase 2 — Schema, config, layout, frame

- [ ] `config/schema.rs`: `Opt`, `IconOpt`, `ColorOpt`, `ModuleSchema`, preset tables
- [ ] `config/mod.rs`: top-level model, `[frame]`, `[[line]]` (left `modules` + `right`), `[modules.<id>]`, resolution order, validation with TOML paths
- [ ] Icon sets: nerd, unicode, emoji, ascii
- [ ] Themes: garnish, catppuccin-mocha, nord, dracula, tokyonight, mono; role overrides
- [ ] `frame.rs`: none/rounded/square/double/heavy/powerline/custom; fill to `$COLUMNS`; overflow rules
- [ ] `garnish config init|check|path|show`
- [ ] Top-level presets: default, minimal, full, compact
- [ ] Config fixtures + matrix test (no panic, line count, width)

## Phase 3 — Payload-only modules

- [ ] `model`, `effort`, `style`
- [ ] `context` (+ `claude_settings.rs` autocompact resolution, `exceeds_200k`, `warn_at`)
- [ ] `limit5h`, `limit7d`, `spend`, `cost`
- [ ] `session`, `api`, `cache`, `clock` (jiff, spinner)
- [ ] `session_name`, `vim`, `agent`, `lines`
- [ ] Unit + golden tests per module × preset × icon set × theme
- [ ] Failure rendering (`⚠` lines), `GARNISH_DEBUG` log

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
  namtao toolkit), spec and plan approved. Phase 0 started: nightly
  1.100.0 (2026-09-03) installed, project scaffolded, deps added, docs written.
