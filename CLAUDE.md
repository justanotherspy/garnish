# CLAUDE.md — working rules for garnish

garnish is a Rust CLI that renders the Claude Code status line. It is built
entirely by Claude across many sessions, so this file is the durable memory of
how to work here. Read it, then read `PLAN.md` for where things stand.

## Session protocol

1. Read `PLAN.md` → find the first unchecked item in the current phase.
2. Work in small commits; run `make check` before every commit.
3. Before ending a session: tick checkboxes in `PLAN.md`, append a dated entry
   to its **Session log**, commit. Never leave the tree red.
4. Create a sprite checkpoint after finishing **every** phase (not optional):
   `sprite-env checkpoints create --comment "garnish: phase N done"`.
5. No PRs. Work is local to `/home/sprite/repo/garnish`; `origin` is
   `github.com/justanotherspy/garnish` (SSH key registered 2026-09-04).
   Push `main` only when the user asks.
6. **Edit files with the Read/Edit/Write tools**, never with Bash heredocs,
   `sed`, or Python one-liners. Bash is for running commands (cargo, git,
   make), not for changing source. (Bash edits bypass the harness's file
   tracking and have already caused a lost write in this project.)
7. **Use the rust-analyzer LSP tools** (installed as a Claude Code plugin):
   check diagnostics after edits, go-to-definition and find-references before
   renaming or changing a signature.
8. **Stop and ask** the user when a decision is theirs to make (a design
   change against `SPEC.md`, a new dependency, a behaviour the spec leaves
   open). Do not guess silently; do not widen scope.

## Phase protocol (documents first, code second, review last)

`PLAN.md` tracks progress of tasks. `SPEC.md` is the final goal state of the
whole system. Keeping them honest is what stops sessions drifting from the
goal without a documented reason.

**Starting a phase**

1. Re-read `SPEC.md` and `PLAN.md` for that phase and compare them with the
   codebase as it actually is. If they disagree (a decision changed, a design
   was simplified, a name moved), update the documents *first*, with the
   reason, then start coding.

**Finishing a phase**

1. Validate the phase goals in `PLAN.md` are met (tests, `make check`, manual
   `preview` when rendering changed).
2. Update `PLAN.md` (checkboxes, session log) and `SPEC.md` (anything that
   changed in the target design, with why).
3. Update `CLAUDE.md` with anything learned about *how to do things* here
   (toolchain quirks, lint workarounds, testing tricks), and `README.md` when
   anything a human user needs to know changed (commands, config keys,
   requirements).
4. If code was written, spawn an **adversarial code-review subagent** whose
   brief is to attack the phase's changes: find broken behaviour, subtle bugs,
   lint escapes, deviations from `SPEC.md`, and missing tests. Fix what it
   finds.
5. For every real bug found (by the review, by you, or by the user), add a
   unit test and, where the behaviour is user-visible, an integration/golden
   test so it cannot regress.
6. Commit, then checkpoint.

## Toolchain

- **Nightly Rust** via rustup (`rust-toolchain.toml`, rolling `nightly`).
  Last known-good: `1.100.0-nightly (a69a63265 2026-09-03)`. If a fresh nightly
  breaks the build or nursery lints, pin `channel = "nightly-YYYY-MM-DD"` to the
  last good date and note it in `PLAN.md`; unpin later.
- Edition 2024. `cargo nextest run` for tests, `cargo test --doc` for doctests,
  `cargo bench` for criterion, `./bench/run.sh` for hyperfine end-to-end.
- rust-analyzer is installed as a nightly component and the rust-analyzer LSP
  plugin is installed in Claude Code. Use the LSP tool: diagnostics after
  edits, go-to-definition, references, hover for types.
- `hyperfine` and `cargo-nextest` live in the cargo bin dir
  (`/.sprite/languages/rust/cargo/bin` on this host).

## Commands

```
make check        # fmt --check + clippy -D warnings + nextest + doctests
make lint         # fmt --check + clippy
make test         # cargo nextest run && cargo test --doc
make docs         # regenerate docs/ from module schemas (release build)
make bench        # hyperfine budget gate (fails when over budget)
make install      # cargo install --path . --locked  → ~/.cargo/bin/garnish
./scripts/ci.sh   # everything above plus docs-sync check
```

GitHub Actions (`.github/workflows/ci.yml`) runs `scripts/ci.sh` on Linux and
`make check` on macOS for every push to `main`, tag and PR; the bench job is
`workflow_dispatch` only and never gates (shared-runner timings are noise).
The macOS job is the only coverage of the Mac-only code paths.

## Style: strict lints, never panic (namtao.com/rust)

`Cargo.toml` denies `clippy::pedantic`, `clippy::nursery`, and every panic
path (`unwrap_used`, `expect_used`, `indexing_slicing`, `arithmetic_side_effects`,
`unreachable`, `unimplemented`, `unchecked_time_subtraction`, `todo`,
`string_slice`, `panic_in_result_fn`, `panic`, `exit`, `as_conversions`).
`clippy.toml` allows unwrap/expect/panic/indexing **in tests only**.

What that means when writing code:

- Never `unwrap()`/`expect()` outside `#[cfg(test)]`. Use `?`, `unwrap_or`,
  `unwrap_or_default`, `map_or`, `and_then`, `ok_or_else`, `let-else`.
- Never index: `v[i]` → `v.get(i)`; `&s[a..b]` on strings is banned, iterate
  chars/graphemes instead (see `ansi::truncate`).
- Never `as`: use `u32::from(x)`, `usize::try_from(x)?`, `f64::from(x)`.
  For float→int use `to_int_unchecked`-free helpers in `num.rs` that saturate.
- Arithmetic must be `checked_*`, `saturating_*`, or `wrapping_*` when the
  operands are not compile-time constants. Prefer `saturating_sub` for durations.
- `main` returns `color_eyre::Result<()>`. `render` must never exit non-zero:
  errors become a dim `⚠ garnish: …` line on stdout and a note on stderr.
- `#[allow(clippy::…)]` only with a one-line justification comment above it.
- Prefer combinator pipelines over `if let` towers. Data in → data out.
- Prototype freely inside unit tests; clippy ignores unwraps there.

## Comments and documentation

- Every file has a `//!` module doc saying what it owns; every `pub` item
  has a `///` doc (the `missing_docs` lint enforces the latter). Docs
  describe *what* and *why*, follow rustdoc conventions (`# Errors`,
  `# Panics`, intra-doc links), and read as reference material.
- Inline `//` comments are for the non-obvious only: an invariant, a
  workaround for a quirk, a subtle ordering, a reference to the spec. Do not
  narrate what the code plainly does, do not leave "changed X" notes, and do
  not comment out code. When in doubt, leave the comment out.
- Reviewing a file's comments is part of finishing a phase: delete fluff.

## Crate map (the chosen crate for each job; never add an alternative)

Every crate below was chosen deliberately (see the namtao 2026 toolkit). When
a task fits one of these jobs, use the listed crate; do not reach for a
different crate that does the same thing (no `chrono` for time, no `anyhow`
for errors, no `structopt`/`argh` for CLI, no `regex` for parsing, no
`crossbeam`/`tokio` for parallelism). Adding any new dependency needs the
user's OK first.

| crate | job it owns | where |
|---|---|---|
| clap (derive) | argument parsing; no subcommand = `render` from stdin | `cli.rs` |
| color-eyre | error type (`Result`, `eyre!`, `.context()`) and panic/error reports; installed once in `main` | everywhere fallible |
| serde + serde_json | JSON: the stdin payload, Claude settings files | `payload.rs`, `claude_settings.rs` |
| toml | the TOML config file (parse); config *generation* is hand-written in `docs.rs` | `config/` |
| jiff | all date/time: now, zones, formatting, durations, countdowns; `GARNISH_NOW` freezes it | `time.rs`, `session.rs` |
| itertools | iterator helpers (interspersing, joining, grouping) | rendering |
| std::process + `git::run_program` | every external command (status, rev-list, fetch, `--version`): kill-on-timeout, pipes drained on threads | `git.rs` |

`command-run` (from the toolkit) is deliberately **not** used: every subprocess
in garnish needs kill-on-timeout, which it does not offer. Do not add it back
for a "quick" command; route through `git::run_program`.
| rayon | data parallelism: `refresh --all`, `preview --all`, docs generation; **never on the tick path** | `cli.rs`, `docs.rs` |
| unicode-width | terminal cell width of text | `ansi.rs` |
| criterion (dev) | micro-benchmarks | `benches/` |
| tempfile (dev) | temp dirs for repos/caches in tests | tests |
| hyperfine (binary, not a crate) | end-to-end latency gate | `bench/` |

`reqwest` is not a dependency: PR data comes from the harness payload and
garnish makes no network calls. If HTTP is ever needed, `reqwest` is the
chosen crate.

## Architecture in one breath

stdin JSON → `Payload` → `Config` (TOML + presets) → for each `[[line]]`, each
module id renders `Vec<Segment>` from the payload or from its cache file → the
frame joins left/right groups and fills to `$COLUMNS` → stdout. Cached modules
that are stale render dimmed with `⟳` and spawn a detached
`garnish refresh` worker (own process group, lock file). **No child process on
a warm tick.** See `SPEC.md` for the contract and `docs/` for user docs.

## Conventions

- The module **schema is the single source of truth**: every option, icon,
  color and preset value lives in `ModuleSchema`. Adding an option means:
  schema → render → `make docs` → `UPDATE_GOLDEN=1 cargo nextest run` → commit
  the regenerated `docs/` and `tests/golden/`.
- The module set is fixed (21 ids listed in `SPEC.md`). No generic/plugin module.
- Every time read goes through `time::now()`. Every path goes through
  `paths::*`. Every env hook is documented in `SPEC.md` § Test hooks:
  `GARNISH_NOW`, `GARNISH_CACHE_DIR`, `GARNISH_CONFIG`, `GARNISH_NO_SPAWN`,
  `GARNISH_COLUMNS`, `GARNISH_DEBUG`.
- Fixtures: `tests/fixtures/payloads/*.json`, `tests/fixtures/configs/*.toml`;
  golden renders in `tests/golden/`. Tests that touch the cache dir or PATH
  shims are named `cache_*`, `spawn_*`, `worker_*`, `gc_*` so nextest runs them
  serially (`.config/nextest.toml`).
- Performance budget (warm tick, release): mean < 3 ms, p99 < 8 ms; cold < 30 ms.
  `bench/run.sh` enforces it. If a change costs more than 0.2 ms, justify it in
  the commit message.

## Claude Code facts we depend on (verified 2026-09-04, v2.1.260)

- Payload fields and absence rules: see `SPEC.md` § Payload. `rate_limits`
  present ⇒ subscription; absent ⇒ show `cost`.
- The harness cancels an in-flight status line script on a new trigger and
  debounces at 300 ms; `refreshInterval` minimum is 1 s.
- Autocompact fires at `effective_window − 13_000` tokens
  (`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` lowers it). Constant observed in the
  2.1.260 binary; configurable as `modules.context.compact_buffer_tokens`.
- `COLUMNS`/`LINES` are set for the script; OSC 8 links and ANSI colors work.

## Cache and worker invariants (learned the hard way)

- A lock file always carries `pid epoch_ms`; it is created by `hard_link`
  from a pre-written temp file and re-stamped by `rename`, never truncated
  in place, so no reader ever sees an empty lock.
- A lock younger than `LOCK_GRACE_MS` is live regardless of pid: the tick
  writes it, exits, and the worker adopts it a few ms later.
- Only Linux hands the lock to the worker (`--lock-held`); `/proc/<pid>` is
  the liveness check. Elsewhere the worker takes the lock itself.
- A failed entry is fresh for its TTL like any other. Never make failure
  "not fresh": that spawns a worker on every tick while git is broken.
- Entries carry the situation they were computed for (`head`, `upstream`);
  renders pass a validator to `Ctx::cached` so a branch switch is a miss.
- `refresh = 0` is only legal for payload-only modules; config validation
  rejects it for cached ones.
- GC compares file mtimes with the wall clock, not `GARNISH_NOW`.
- Docs and goldens render with `Clock::fixed()`: no git discovery, no
  settings env, no cache. Tests that run the binary must set
  `GARNISH_CACHE_DIR` and `GARNISH_NO_SPAWN` and clear `CLAUDE_*`/`DISABLE_*`.

## Gotchas

- rustup on this host prints `can't determine memory limit: sysinfo failure`
  warnings; harmless.
- `cargo install` puts binaries in `/.sprite/languages/rust/cargo/bin`, not
  `~/.cargo/bin`; both are on PATH.
