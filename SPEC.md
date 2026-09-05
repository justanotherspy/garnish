# garnish — Product Requirements & Technical Specification

Status: approved 2026-09-04; `v0.1.0` tagged the same day. Owner: Daniel
Schwartz. Builder: Claude. This document is the target design of the whole
system; when the design changes, it changes here first, with the reason
(`CLAUDE.md` § Phase protocol). Progress lives in `PLAN.md`.

## 1. Purpose

`garnish` is the `statusLine.command` for Claude Code. Every second (and on
every harness trigger) Claude Code pipes a JSON snapshot of the session to the
command and displays whatever it prints. garnish turns that snapshot into a
small, beautiful, information-dense dashboard composed of independent modules,
laid out by a TOML config, and does it so cheaply that dozens of concurrent
sessions on one host do not notice it running.

### Goals

- **Fast**: a warm tick averages < 3 ms (p99 < 8 ms) in release; cold < 30 ms.
- **Never blocks**: anything slow (git ahead/behind, dirty state, optional
  `git fetch`) runs in a detached worker; the tick renders cached data.
- **Composable**: 21 granular modules, any of them on any line, left or right
  aligned, each with `minimal` / `default` / `full` presets.
- **Beautiful**: Nerd Font glyphs, smooth gradient bars, framed lines, named
  color themes, OSC 8 links.
- **Documented from code**: module docs are generated from each module's
  option schema, so they cannot drift.
- **Tested exhaustively**: real-binary integration tests over payload fixtures,
  temp git repos, PATH shims, a frozen clock, and a hyperfine latency gate.

### Non-goals (v0.1)

- No generic/plugin modules; the module set is fixed.
- No network calls (PR state comes from the harness payload).
- No Windows support. Linux and macOS only.
- No daemon. Workers are one-shot detached processes.

## 2. Claude Code contract

Verified against docs (code.claude.com/docs/en/statusline) and v2.1.260.
Minimum supported Claude Code: **2.1.251** (adds `prompt_cache`, `effort`).

### 2.1 Settings

```json
{ "statusLine": { "type": "command", "command": "garnish", "refreshInterval": 1, "padding": 0 } }
```

`refreshInterval` minimum is 1 s. The harness also re-runs the command on:
session start, assistant message, `/compact`, permission-mode change, vim
toggle, `command` change, a rate-limit `resets_at`, a prompt-cache `expires_at`.
Updates are debounced at 300 ms and **an in-flight script is cancelled when a
new trigger fires**, so anything slow must survive the tick being killed.

The script gets `COLUMNS`/`LINES` in its environment. Output supports multiple
lines, ANSI colors, and OSC 8 hyperlinks.

### 2.2 Payload (stdin JSON)

| field | type | notes |
|---|---|---|
| `cwd`, `workspace.current_dir` | string | same value |
| `workspace.project_dir` | string | where Claude was launched |
| `workspace.added_dirs` | string[] | `/add-dir` entries |
| `workspace.git_worktree` | string? | linked-worktree name; absent in main tree |
| `workspace.repo.{host,owner,name}` | ? | parsed from `origin`; absent otherwise |
| `session_id` | string | stable per session; cache key |
| `session_name` | string? | custom or AI title; absent for default names |
| `prompt_id` | string? | |
| `transcript_path` | string | not used |
| `version` | string | Claude Code version |
| `model.{id,display_name}` | string | |
| `output_style.name` | string | |
| `cost.total_cost_usd` | number | estimate; resets on `/clear` |
| `cost.total_duration_ms` | number | wall clock since session start |
| `cost.total_api_duration_ms` | number | |
| `cost.total_lines_added/removed` | number | |
| `context_window.context_window_size` | number | 200000 or 1000000 |
| `context_window.used_percentage` | number? | null early/after compact |
| `context_window.remaining_percentage` | number? | |
| `context_window.total_input_tokens/total_output_tokens` | number | |
| `context_window.current_usage` | object? | `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` |
| `exceeds_200k_tokens` | bool | fixed 200k threshold |
| `prompt_cache` | object? | `warm`, `caching_observed`, `ttl` ("5m"/"1h"), `expires_at?`, `requests`, `misses`, `expected_rebuilds`, `hit_ratio?`, `cache_write_tokens`, `miss_recache_tokens`, `last_miss_at?`, `recache_tokens_if_cold?` |
| `fast_mode` | bool | |
| `effort.level` | string? | low/medium/high/xhigh/max; absent if unsupported |
| `thinking.enabled` | bool | |
| `rate_limits.{five_hour,seven_day,spend_limit}` | object? | each `{used_percentage, resets_at}` epoch s; **present only for Pro/Max** (or gateway spend limits) after first API response; windows independently absent |
| `vim.mode` | string? | NORMAL/INSERT/VISUAL/VISUAL LINE |
| `agent.name` | string? | |
| `pr.{number,url,review_state?,kind?}` | object? | open PR/MR; `review_state` approved/pending/changes_requested/draft; `kind = "mr"` for GitLab |
| `worktree.{name,path,branch?,original_cwd,original_branch?}` | object? | Claude worktree session |

Auth-mode rule: `rate_limits` present ⇒ subscription (show limits); absent ⇒
API key/gateway (show `cost`).

### 2.3 Autocompact threshold (approximation)

Not in the payload. From the 2.1.260 binary:
`threshold = effective_window − 13_000`, or
`min(floor(window × pct / 100), window − 13_000)` when
`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` is set. `effective_window =
min(context_window_size, configured)` where `configured` comes from
`CLAUDE_CODE_AUTO_COMPACT_WINDOW` (env) > `autoCompactWindow` in settings
(managed > `.claude/settings.local.json` > `.claude/settings.json` >
`~/.claude/settings.json`) > model default (= window). `autoCompactEnabled =
false` or `DISABLE_AUTO_COMPACT=1` disables the marker. The buffer constant is
configurable (`modules.context.compact_buffer_tokens`).

## 3. Modules

Every module has: `enabled` (bool), `preset` (`minimal|default|full`),
`refresh` (seconds; `0` = payload-only, rendered every tick; `> 0` = cached with
that TTL and refreshed by a worker), `icons.<key>`, `colors.<key>`, `label`,
`prefix`, `suffix`, `hide_when_empty`. Option resolution: built-in default →
icon-set default → module preset → top-level preset → explicit key.

### 3.1 Repo group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `path` | base dir (git toplevel, else `project_dir`) + cwd subpath | base name | `~/parent/base` + dim `/sub` | full tilde path + subpath + `added_dirs` count | 0 (toplevel cached) |
| `branch` | branch or detached HEAD | name | icon + name | + short SHA, dirty `●` | 5 |
| `sync` | ahead/behind vs `@{upstream}` | `⇡2⇣1` when non-zero | colored counts, dim `no upstream` | + upstream name + fetch-age hint `⇣?12m` | 5 (+ opt-in `fetch_interval`) |
| `worktree` | `workspace.git_worktree` / `worktree.name` | name | icon + name | + `original_branch → branch` | 0 |
| `pr` | open PR/MR | `#123` linked | icon + `#123` linked + state glyph | + state word | 0 |

GitLab merge requests render as `!7` (GitLab's own notation) with the `mr`
icon; GitHub pull requests as `#42`.

PR state glyphs/colors: approved `✓` ok, pending `○` warn, changes_requested
`✗` danger, draft `◌` muted. Link uses OSC 8 to `pr.url`.

### 3.2 Model group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `model` | `display_name`, `⚡` when fast | name | icon + name (+⚡) | + `model.id`, thinking glyph | 0 |
| `effort` | `effort.level` | word | icon + scale `▁▃▅▇█` | scale + word | 0 |
| `context` | bar (100% = window) + % + compaction marker | `42%` | bar(20) + `42%` | bar(30) + `42%` + marker label + window tag + `exceeds_200k` | 0 (settings cached 30 s) |
| `style` | `output_style.name` | name unless default | icon + name unless default | always | 0 |

Context bar: filled cells `█` with partial blocks for sub-cell precision,
empty `░`; the **filled part** takes the color of the current band
(`thresholds = [50, 75, 90]`, `band_colors = [ok, warn, orange, danger]`); a
`▏` marker at the autocompact position; `exceeds_200k = { enabled, glyph = "‼",
color = "danger" }`; `warn_at` adds an extra badge threshold. No token counter.
`used_percentage` null → empty bar and `–`.

### 3.3 Usage group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `limit5h` | 5-hour % + reset countdown | `23%` | icon + `23%` + `⏱2h13m` | + mini bar | 0 |
| `limit7d` | 7-day % + reset countdown | `41%` | icon + `41%` + `⏱3d4h` | + mini bar | 0 |
| `spend` | spend-limit % | `62%` | icon + % + reset | + bar, danger > 100 | 0 |
| `cost` | `total_cost_usd` | `$1.23` | icon + `$1.23` | + `+156 −23` | 0 |

Limit modules render nothing when their window is absent. `cost` has
`only_without_rate_limits = true` so one usage line serves both auth modes.

### 3.4 Session group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `session` | `total_duration_ms` | `1h12m` | icon + `1h12m` | + start time | 0 |
| `api` | `total_api_duration_ms` | `8m20s` | icon + `8m20s` | + `(11%)` of session | 0 |
| `cache` | prompt cache | `91%` | icon + `91%` + TTL badge + `● 47m`/`○` warm countdown | + misses, writes | 0 |
| `clock` | local time + spinner | `HH:MM` | spinner + `HH:MM:SS` | + date, UTC offset | 0 |

`cache` hit % = `prompt_cache.hit_ratio`; fallback to the last request's
cache-read share from `current_usage`; `prompt_cache` absent → `–`.
Spinner frame = `now_secs mod frames.len()` (stateless).

### 3.5 Session-identity group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `session_name` | `session_name` (absent → hidden) | name | icon + name | + short `session_id` | 0 |
| `vim` | `vim.mode` (absent → hidden) | `N`/`I`/`V`/`VL` | colored badge | + icon | 0 |
| `agent` | `agent.name` (absent → hidden) | name | icon + name | + thinking glyph | 0 |
| `lines` | lines added/removed | `+156 −23` | icon + colored `+156 −23` | + net delta | 0 |

### 3.6 Staleness

A cached module whose entry is older than its TTL spawns a worker but keeps
rendering the last value normally: a refresh in flight is not a problem the
user needs to see. Only once the entry is older than `stale_after` TTLs
(default 5, so 25 s for a 5 s module) is it *overdue* and rendered dimmed
with a trailing `⟳`; an entry computed for another situation (branch or
upstream changed) is overdue at once. If the last refresh failed the module
renders dimmed with `✗` and the error is kept in the cache file for
`garnish doctor`. A missing entry renders the module's placeholder.
(Changed 2026-09-04: with a 5 s TTL and a 1 s tick the old rule dimmed the
value on every fifth tick, which read as flicker.)

## 4. Configuration

Location: `--config` > `$GARNISH_CONFIG` > `$XDG_CONFIG_HOME/garnish/garnish.toml`
(default `~/.config/garnish/garnish.toml`) > `~/.garnish.toml` > built-in
defaults. Config is re-read every tick (it is tiny); no daemon.

```toml
preset = "default"        # default | minimal | full | compact
icons  = "nerd"           # nerd | unicode | emoji | ascii
theme  = "garnish"        # garnish | catppuccin-mocha | nord | dracula | tokyonight | mono
color  = "auto"           # auto | always | never | 256 | truecolor
truncate = true           # cut the left group when a line overflows; the right group is never cut
stale_style = "dim"       # dim | hide | plain: how overdue cached values are shown
stale_after = 5           # TTL periods a value may be overdue before it is styled stale (≥ 1)
padding = 0               # cells subtracted from the width; mirror statusLine.padding

[colors]                  # role overrides: accent accent2 muted text ok warn hot danger frame band1..band4
accent = "#89b4fa"

[frame]
style = "rounded"         # none | rounded | square | double | heavy | powerline | custom
fill = true               # rule to $COLUMNS and close with the right cap
separator = " │ "
# custom: first middle last single fill right_first right_middle right_last separator pad

[[line]]
modules = ["path", "branch", "sync", "worktree", "pr"]
right   = ["session_name", "agent"]
separator = "  "
[[line]]
modules = ["model", "effort", "context", "style"]
right   = ["vim"]
[[line]]
modules = ["limit5h", "limit7d", "spend", "cost"]
right   = ["lines"]
[[line]]
modules = ["session", "api", "cache"]
right   = ["clock"]

[modules.context]
preset = "full"
width = 24
thresholds = [50, 75, 90]
band_colors = ["ok", "warn", "#ff8800", "danger"]
compaction_marker = true
compact_buffer_tokens = 13000
[modules.context.icons]
fill = "█"
empty = "░"
marker = "▏"
```

Top-level presets: `default` (four lines above), `minimal` (one line, frame
`none`, all modules minimal: `path branch context limit5h cost` / right
`clock`), `full` (four lines, every module full), `compact` (two lines:
`path branch sync pr` / right `clock`; `model effort context limit5h cost` /
right `cache`).

Layout rules: left group joined by `separator`; right group likewise; the frame
rule fills the gap to `$COLUMNS − padding`; right cap after. Overflow: drop the
fill, then truncate the **left** group (ANSI-aware, `…`); never the right group.

Validation (`garnish config check`): unknown keys, wrong types, unknown module
ids, unknown presets, bad colors, all reported with TOML paths.

## 5. Failure behaviour

`garnish` (render) always exits 0 and always prints something:

- invalid config → render with built-in defaults, append dim
  `⚠ config: <path>:<line> <msg>`;
- malformed stdin → `⚠ garnish: bad payload`;
- internal error → `⚠ garnish: <msg>`.

`GARNISH_DEBUG=1` appends per-tick diagnostics to `<cache>/debug.log` (1 MB
rotation); `garnish doctor` shows the tail plus toolchain, config path/validity,
cache dir, last worker errors, and a glyph test line.

## 6. Cache & workers

- Root: `$GARNISH_CACHE_DIR` > `$XDG_RUNTIME_DIR/garnish` > `$XDG_CACHE_HOME/garnish`
  > `~/.cache/garnish` (macOS `~/Library/Caches/garnish`).
- `<root>/sessions/<session_id>/<module>.cache`; git data in
  `<root>/repos/<hash(git-common-dir + worktree path)>/<module>.cache` so
  sessions in one worktree share it. Never keyed on `transcript_path`.
- Entry: line 1 `v1 <computed_at_ms> <ttl_ms> ok|err`; then `key=value` lines
  or the error text. Malformed = miss. Written as `<file>.tmp.<pid>` + rename.
- Tick: fresh → render; past TTL → spawn worker unless `<module>.lock` is
  live, rendering the last value unchanged; older than `stale_after` TTLs
  (or computed for another head/upstream) → dim `⟳`; `err` → dim `✗`. A failed entry is fresh for its TTL like any
  other (a broken git is retried once per TTL, never once per tick). Entries
  record what they were computed for (`head`, `upstream`); a render whose
  situation differs treats the entry as stale.
- Lock = file `pid epoch_ms`, created by `hard_link` from a pre-written temp
  file and re-stamped by `rename` (never truncated in place). Live when
  younger than 2 s (hand-over window), else while the pid exists (Linux,
  `/proc`) and it is younger than 60 s (15 s where pids cannot be checked).
  Stale locks are reclaimed by an atomic rename so racing ticks cannot both
  win. A guard only unlinks a lock that still carries its own pid.
- Worker: `garnish refresh --module M --session S --cwd D`, null stdio,
  `process_group(0)`, spawned without wait. On Linux the tick takes the lock
  and passes `--lock-held`; elsewhere the worker takes it itself.
  `GARNISH_NO_SPAWN=1` logs intended spawns to `<root>/spawns.log` instead.
- `refresh` must be ≥ 1 for cached modules (`config check` rejects 0).
- GC: bounded sweep when a session dir is first created (session and repo
  dirs idle > 24 h by wall-clock mtime, ≤ 50 per sweep; temp/stale/adopt
  files older than 1 h); `garnish gc` for manual runs.
- **No child process on a warm tick.** Branch/upstream/HEAD are read from
  `.git` files (loose refs, `packed-refs` scanned as bytes with early exit,
  worktree `gitdir`, symref chains capped at 5); reftable repos report no
  head and fall back to the worker. Ahead/behind, dirty, and fetch run in the
  worker only through `git::run_program` (pipes drained on threads, kill on
  timeout: 2 s for local commands, 20 s for `fetch`, `GIT_TERMINAL_PROMPT=0`).
  A failed fetch is recorded in the entry (`fetch_error`, `fetch_attempt`)
  without hiding the local counts and is not retried within `fetch_interval`.

## 7. CLI

`--config FILE` is a global flag on every command and overrides
`GARNISH_CONFIG` and the default location.

| command | purpose |
|---|---|
| `garnish` | render from stdin (default) |
| `garnish refresh --module M --session S --cwd D [--all] [--lock-held]` | worker entry point; hidden from `--help` |
| `garnish install [--settings P] [--refresh-interval 1] [--padding N] [--absolute] [--no-config] [--dry-run]` | merge `statusLine` into settings.json through symlinks, keeping permissions, with a never-clobbered backup; write default config if absent; warn on stderr if not on PATH. `--absolute` writes `current_exe()` (a symlinked launcher resolves to its target). |
| `garnish doctor` | diagnostics |
| `garnish config init [--preset P] [--force] \| check \| path \| show` | config management; `init` refuses to overwrite without `--force`; `show` prints the fully resolved config |
| `garnish preview <file\|dir> [--preset P] [--icons S] [--theme T] [--color M] [--width N]` | render one fixture or every `*.json` in a directory |
| `garnish docs [--out DIR]` | regenerate docs from schemas |
| `garnish modules` | list module ids + summaries |
| `garnish gc` | sweep stale cache dirs |

## 8. Performance budget

Measured with hyperfine (`bench/run.sh`, release build, `-N`, warmup 20,
300 runs) and gated by `bench/check.sh` (jq over hyperfine's JSON):

| scenario | mean | p99 |
|---|---|---|
| warm tick, default preset | < 3 ms | < 8 ms |
| warm tick, full preset, all modules | < 3 ms | < 8 ms |
| cold tick (empty cache, git repo) | < 30 ms | — |
| `refresh --module sync` worker (rev-list, no fetch) | < 50 ms | — |

Criterion micro-benches in `benches/` track parse, config resolution, and
per-module render cost.

## 9. Testing strategy

- **Unit**: each module × preset × icon set × theme with a frozen clock;
  absent/null fields; band edges; duration/countdown formatting; threshold
  math; ANSI width/truncation; frame assembly; preset resolution order;
  schema completeness (every read icon/color/option key exists in the schema).
- **Integration** (real binary): payload fixtures (subscription, API key,
  pre-first-response nulls, no git, worktree session, git worktree, PR states,
  MR, spend_limit, fast_mode, 1M at 3/50/80/96 %, 200k, autocompact via
  settings/env/disabled, vim, agent, session_name, absent effort, narrow
  COLUMNS); temp git repos with a local bare origin (ahead, behind, diverged,
  no upstream, detached, dirty, worktree); PATH shim `git` (slow/failing)
  proving ticks never block; cache TTL expiry; live lock; stale lock with dead
  pid; `.tmp`/truncated entries ignored; tick killed mid-run while the worker
  completes; 32 concurrent ticks → exactly one worker; GC bounds.
- **Config matrix**: every preset, one-module-per-line, all-on-one-line, every
  frame style, custom frame, each module in each preset × fixtures → no panic,
  correct line count, width ≤ COLUMNS; golden files under `tests/golden/`
  (`UPDATE_GOLDEN=1` regenerates).
- **Docs sync**: `garnish docs` output must equal committed `docs/`, and
  `config init` output must equal `examples/garnish.toml`.

### Test hooks (environment)

| var | effect |
|---|---|
| `GARNISH_NOW` | freeze `time::now()` (epoch seconds or RFC 3339) |
| `GARNISH_CACHE_DIR` | cache root override |
| `GARNISH_CONFIG` | config path override |
| `GARNISH_NO_SPAWN` | record intended worker spawns instead of spawning |
| `GARNISH_COLUMNS` | width override when `COLUMNS` is absent |
| `GARNISH_DEBUG` | write `<cache>/debug.log` |

## 10. Documentation

- `docs/README.md`, `docs/config.md` and `docs/modules/<id>.md` are generated
  by `garnish docs` from `ModuleSchema` and the preset/theme/icon tables, with
  sample renders made under a pinned clock and no git or settings lookup.
  They are committed so the reference is readable on GitHub, and
  `tests/docs_sync.rs` fails when they drift from the code.
- `docs/guide.md` is the one hand-written page under `docs/`: install,
  hook-up, first config, troubleshooting. `garnish docs` never writes it.
- `examples/garnish.toml` is what `garnish config init` writes, kept in sync
  by the same test.
- `README.md` is for users and links to the guide and the reference;
  `CLAUDE.md`, `PLAN.md` and `SPRITE.md` are for building the project.

## 11. Assumptions

- Window size comes from `context_window_size`; 1M is assumed only when the
  field is absent or zero.
- The 13k compaction buffer mirrors Claude Code 2.1.260 internals and may
  drift; it is configurable and the marker can be disabled.
- Cache TTL display uses `prompt_cache` only; when absent the module shows `–`.
- Session duration is `cost.total_duration_ms` and resets on `/clear`.
- No GitHub network access; PR presence/state is whatever the harness reports.
- Four default lines cost four terminal rows; `compact`/`minimal` exist for
  small terminals.
