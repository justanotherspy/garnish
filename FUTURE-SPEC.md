# FUTURE-SPEC.md — ideas mined from ccstatusline

**Status: proposals, not decisions.** Nothing in this file is part of the
target design until it is moved into `SPEC.md` (with the reason) and given a
phase in `PLAN.md`. It exists so that the next planning session starts from
an inventory of what a mature competitor ships, what garnish already does
better, and which ideas are worth the cost of porting.

Source studied: [sirmalloc/ccstatusline](https://github.com/sirmalloc/ccstatusline),
MIT, Node/TypeScript with a React/Ink TUI. Clone at commit
`016be1fcf19453bd4362439b197e9cf841d7006a` (package version 2.2.29),
read on 2026-09-05. File paths below are relative to that repository at that
commit; the local clone lived in a job scratch directory and is not kept.
Facts about Claude Code were re-verified against the 2.1.261 binary on the
same day (see § 8.3).

How to read this: § 3 is the list of things garnish must not give up while
porting anything. § 4 is the widget inventory. § 5 holds the proposals,
tiered by what they cost: **A** fits every current invariant, **B** needs a
worker or cache adaptation, **C** needs a `SPEC.md` non-goal lifted or a new
crate and is therefore a decision for Daniel, **D** is deliberately not
ported. § 7 is the interactive installer, which gets its own section because
Daniel asked for it. § 9 is the decision checklist; § 10 a suggested phase
order after Phase 18.

---

## 1. Method

Read in full: `README.md`, `docs/USAGE.md`, `docs/DEVELOPMENT.md`,
`docs/WINDOWS.md`, `AGENTS.md`, `package.json`, `configTemplates/*.json`,
the entry point `src/ccstatusline.ts`, every file under `src/types/`,
`src/utils/` and `src/widgets/`, the TUI under `src/tui/`, and the CI and
publish workflows. Each feature was traced to the code that implements it,
so the notes below say how a thing is built, not only that it exists.

Repository shape at the commit: ~90 widget types in `WIDGET_MANIFEST`
(`src/utils/widget-manifest.ts`), one class per widget under `src/widgets/`,
a single 1 392-line renderer (`src/utils/renderer.ts`), Zod-validated
settings with four schema versions and migrations (`src/utils/config.ts`,
`src/utils/migrations.ts`), and a TUI of ~30 Ink components.

## 2. Market context

Public numbers on 2026-09-05:

| measure | ccstatusline |
|---|---|
| GitHub stars / forks | 12 781 / 558 |
| open issues | 109 |
| created | 2025-08-08 |
| busiest months (commits) | 2025-08 (93), 2026-03 (62) |
| runtime | Node ≥ 18 or Bun; installed via `npx`/`bunx` or a pinned global install |
| distribution | npm with trusted-publishing provenance; GitHub release per `v*` tag; Remotion-rendered demo GIF |

An ecosystem has formed around it: a config gallery (statuslin.es), a
browser-based visual editor (ccstatusline-editor), and interoperability with
`ccusage`. Its README explicitly answers "why is it slow" with a startup
table: `bunx ccstatusline@latest` 633 ms, a pinned global install 202 ms,
`npx` about 1.1 s per repaint. That number is the single biggest
differentiator garnish has (§ 3).

What its users asked for, judging by the changelog and issue-driven widgets:
rate-limit windows beyond the two the harness sends (per-model weekly limits,
extra-usage credits), a service-status indicator, "how fast is it
generating", "how many times has it compacted", PR review and CI state, a
skills indicator, Powerline visuals, and a way to stop VS Code's terminal
from trimming the line. Several of those are already in the garnish payload
for free; the rest are weighed in § 5.

## 3. Where garnish is already ahead (do not regress)

These are the properties every proposal below is checked against.

1. **Never blocks a tick.** ccstatusline runs `git` synchronously
   (`execFileSync` in `src/utils/git.ts`), scans transcript JSONL files, and
   performs HTTPS fetches on the render path, relying on caches to make it
   usually fast. garnish's tick reads the payload and cache files only, with
   a measured budget (< 3 ms warm) and a detached worker for anything slower
   (`SPEC.md` § 6, § 8). Every port must state its tick class: payload-only,
   cached worker, or network worker.
2. **Payload-first.** `prompt_cache`, `rate_limits`, `context_window`,
   `pr`, `worktree`, `effort`, `vim`, `agent` come from the harness.
   ccstatusline derives the cache timer, the context size after compaction,
   the thinking effort and the session duration from the transcript because
   its schema predates those payload fields, and each is a heuristic. Where
   the payload has the value, garnish keeps using it.
3. **Exact width.** garnish derives the box width from the harness's own
   layout (`COLUMNS − 4 − 2 × statusLine.padding`, `CLAUDE.md`). ccstatusline
   probes the terminal by walking parent PIDs to find a TTY and running
   `stty -F /dev/tty size` (`src/utils/terminal.ts`), then subtracts a guessed
   6 or 40 cells (`resolveEffectiveTerminalWidth` in `src/utils/renderer.ts`)
   depending on a `flexMode` the user must choose. Do not import the guess.
4. **Generated docs, goldens, docs-sync.** Their `docs/USAGE.md` is
   hand-written; module docs in garnish come from `ModuleSchema` and CI fails
   when they drift.
5. **Per-key config fallback and a live config.** garnish re-reads TOML on
   every tick, so any editor (a TUI included) shows its effect within a
   second. ccstatusline loads once per process too, but the TUI has an
   explicit save step and an "unsaved changes" dialog.
6. **Stateless animation.** `floor(now × step) mod period` (`SPEC.md` § 4.2)
   needs no state file; ccstatusline has no animation at all.
7. **Fixed module set, no command execution.** `SPEC.md` § 3.7 rules out a
   "run this shell command" module; ccstatusline's `CustomCommand` widget
   exists and has a timeout knob because it needs one.

## 4. Widget inventory against garnish modules

Status: **have** (garnish module covers it), **partial** (covered with a
gap noted), **missing** (candidate, see § 5 tier), **out** (deliberately not
ported, § 5.4).

| ccstatusline widget | garnish | status | note |
|---|---|---|---|
| Model | `model` | have | |
| Version | — | missing (A) | `version` payload field; trivial |
| OutputStyle | `style` | have | |
| ThinkingEffort | `effort` | have | theirs is transcript-derived; ours is payload |
| ContextLength / ContextPercentage / ContextPercentageUsable / ContextBar | `context` | have | "usable" = percentage of the autocompact threshold; see A11 |
| TokensInput / Output / Cached / Total | `context` full preset | partial | no separate cache-read / cache-write split; A6 formats cover the numbers |
| CacheTimer | `cache` | have | theirs infers the TTL countdown from transcript timestamps; ours reads `prompt_cache.expires_at` |
| CompactionCounter | — | missing (C1) | transcript-only |
| TokenSpeed (t/s) | — | missing (C1) | transcript-only |
| SessionClock | `session` | have | |
| SessionCost | `cost` | have | |
| BlockTimer / BlockResetTimer | `limit5h` | partial | A10: elapsed view of the 5-hour window |
| UsageSession / UsageWeekly / UsageSonnet / UsageOpus / ExtraUsage | `limit5h`, `limit7d` | partial | per-model weekly and extra usage need the usage API (C2) |
| ClaudeStatus (service status) | — | missing (C3) | network |
| GitBranch | `branch` | have | theirs can hyperlink to the repo (A8) |
| GitChanges (+N −M) | `lines` | have | they count git-diff lines; ours counts Claude's edits, a different number worth keeping |
| GitStatus (file counts) / GitConflicts / GitStash / GitSha / GitOrigin / GitOriginOwner | `branch` full preset | partial | B2 |
| GitAheadBehind | `sync` | have | |
| GitRootDir / GitWorktree | `path`, `worktree` | have | root-dir IDE link is A8 |
| GitPr / GitCiStatus | `pr` | partial | CI checks need `gh` in the worker (B3) |
| CurrentWorkingDir (fish abbreviation, `~`, N segments) | `path` | partial | A7 |
| ProjectDir / AddedDirs | `path` full preset | have | |
| SessionName / SessionId | `session_name` | have | |
| VimMode | `vim` | have | |
| AgentName | `agent` | have | |
| AccountEmail | — | missing (A9) | read `~/.claude.json` |
| SandboxStatus / VoiceMode / RemoteControlStatus | — | missing (A9) | layered settings + `sessions/<pid>.json` |
| Skills (active skill via hooks) | — | missing (B4) | hooks write a per-session log |
| FreeMemory / memory usage | — | out | not a property of the session |
| Link (arbitrary OSC 8) | `text.<name>` | partial | a `url` key on text modules (A8) |
| Separator / FlexSeparator / Spacer / CustomText | `frame.separator`, `right`, `text.<name>` | partial | multiple flex points (A2) |
| CustomCommand | — | out | `SPEC.md` § 3.7 |
| jj (Jujutsu) widgets | — | out | |
| Gradients, Powerline segments, global overrides, number formats, hide states, dim-parens | — | missing | A3 A4 A5 A6, B1 |

## 5. Proposals

Each proposal names what ccstatusline does and where, what garnish would do
instead as a TOML sketch, the invariant it touches, and its tick class.

### 5.1 Tier A — fits every current invariant

**A1. Reset the harness's dim at the start of every row.** *Verify first.*
The 2.1.261 binary renders each status line row as
`<Text dimColor wrap="truncate">` (found next to `statuslineIssueCount` in
the strings dump), so the whole row is wrapped in SGR 2. ccstatusline
prefixes every rendered line with `\x1b[0m` for exactly this reason
(`renderMultipleLines` in `src/utils/renderer.ts`, comment "override Claude
Code's dim"). garnish emits resets only *after* painted segments
(`src/ansi.rs`), so its first segment, plain separators and frame glyphs
render dim in the real harness while `preview` shows them at full intensity.
Proposal: prefix each output row with `\x1b[0m` when color is on; add the
fact to `CLAUDE.md` § Claude Code facts once confirmed on screen, and a
golden that pins the prefix. Tick class: payload-only. Cost: a few bytes per
row.

**A2. More than two groups per line (flex points).** ccstatusline lets any
number of `FlexSeparator` widgets sit anywhere in a line; free space is
divided evenly with the remainder going to the first ones (`spacePerFlex =
floor(total / count)` in `src/utils/renderer.ts`). garnish has `modules`
(left) and `right`. Proposal: allow a `center` group first, since it covers
the common "title in the middle" layout, and generalise `compose_line` to N
groups later only if asked:

```toml
[[line]]
modules = ["path", "branch"]
center  = ["session_name"]
right   = ["clock"]
```

Overflow order stays: drop fill, cut left, then cut center; the right group
is never cut. `align` treats the center group as its own column set. Tick
class: payload-only.

**A3. Color gradients.** ccstatusline accepts `gradient:<preset>`,
`gradient:RRGGBB-RRGGBB` or `gradient:hex:a,b,c` as any color value,
interpolates in OKLab, applies per widget or across the whole line, degrades
to the nearest ansi16 color, and collapses to the first stop in Powerline
mode (`src/utils/gradient.ts`, 13 named presets ported from gradient-string
with attribution). Proposal in garnish terms: a `gradient` value accepted
wherever a color is, computed in `theme.rs` with an OKLab helper (pure
arithmetic; no crate):

```toml
[colors]
accent = "gradient:#89b4fa-#f5c2e7"          # two stops
[modules.context]
bar_gradient = "gradient:ok-warn-danger"     # role names as stops
```

Rules: per-segment gradient by default, `[line].gradient = true` to span the
visible text of the line; `color = 256` picks the nearest cube entry per
cell; `mono` ignores gradients. Tick class: payload-only; cost is one
interpolation per cell on painted text, so keep it out of the
fill and frame glyphs by default and measure with `bench/run.sh`.

**A4. Hide conditions as a list.** ccstatusline has a unified
`metadata.hide = "no-git,zero,..."` per widget with per-widget defaults and
a TUI checklist (`getHideableStates` on the Widget interface;
`src/utils/migrations.ts` v3→v4 folded older flags into it). garnish has
`hide_when_empty` only. Proposal: keep `hide_when_empty` as the shorthand
and add a per-module list whose vocabulary comes from the schema:

```toml
[modules.sync]
hide = ["no_upstream", "zero"]      # schema-declared states; docs generated
[modules.limit7d]
hide = ["below:10"]                 # value-threshold states where the module has a percentage
```

Each module declares its hideable states in `ModuleSchema`; `config check`
rejects unknown ones; `garnish docs` lists them. Tick class: payload-only.

**A5. Per-module `max_width`.** ccstatusline truncates a widget's rendered
text at `metadata.maxWidth` with `...`, ANSI- and OSC 8-aware. garnish
truncates only the whole left group. Proposal: `max_width = N` on every
module (schema-wide, like `label`), applied before alignment so a long
branch name or session title cannot push the rest of the line off. Truncate
graphemes with `…`, keep the OSC 8 wrapper intact (`ansi::truncate` already
exists). Tick class: payload-only.

**A6. Number formats.** ccstatusline's global override
`numberFormat` picks a style per kind (tokens, speed, percent, memory, cost)
with `precise | compact | whole` and a decimals count
(`src/utils/number-format.ts`). garnish has `durations`. Proposal: a sibling
key with the same shape and per-module override:

```toml
[format]
tokens  = "compact"   # 128k | precise 128,400 | whole 128400
percent = "whole"     # 42% | precise 42.3%
cost    = "cents"     # $1.23 | whole $1
```

Also from their global overrides: `dim = "parens"` to dim only the
parenthesised detail of a full preset (`(11%)` in `api`, the marker label in
`context`) instead of the whole module. Tick class: payload-only.

**A7. Path display options.** `CurrentWorkingDir` offers fish-style
abbreviation (`~/r/g/src`), a segment count, and `~` collapsing. garnish's
`path` has three presets. Proposal: `style = "full" | "fish" | "tail:N"` on
`path`, applied to the base part (the subpath stays dim as today). Tick
class: payload-only.

**A8. Links.** Three things from `src/utils/hyperlink.ts` and the widgets:
`GitBranch` links to `<repo url>/tree/<branch>`; `GitRootDir` links to
`vscode://file/<path>` or `cursor://file/<path>`; the `Link` widget is an
arbitrary OSC 8 anchor. garnish already emits OSC 8 for `pr`. Proposal:
`link = true` on `branch` (uses `workspace.repo.{host,owner,name}` from the
payload, so no git call), `link = "vscode" | "cursor" | "none"` on `path`,
and a `url` key on `text.<name>` modules (static text stays static; the URL
is a string in the config, so § 3.7 holds). Tick class: payload-only.

**A9. Settings-derived identity modules.** ccstatusline reads Claude's
settings in layer order project-local > project > user-local > user
(`src/utils/claude-settings.ts`) for sandbox and voice mode, reads
`<config>/sessions/<pid>.json` for a `bridgeSessionId` (remote control), and
`~/.claude.json` for the account email. garnish already reads settings for
the autocompact override, cached 30 s. Proposal: three small modules on the
same cached read: `sandbox` (glyph when sandboxing is on), `remote` (glyph
when the session is bridged to Remote Control; `sessions/<pid>.json` needs
the harness pid, which is garnish's parent), `account` (email, hidden by
default; useful with several accounts). This raises the fixed module count
from 21; the schema, docs and goldens absorb it. Tick class: cached file
reads (30 s), no process.

**A10. Elapsed view on `limit5h`.** ccstatusline's BlockTimer infers the
current 5-hour block by scanning every transcript under `~/.claude/projects`
for the oldest timestamp within a progressive 10/20/48-hour lookback,
flooring to the hour, and caching the result (`src/widgets/BlockTimer.ts`).
garnish has `rate_limits.five_hour.resets_at`, so the same view is
arithmetic: elapsed = 5 h − (resets_at − now). Proposal: `mode = "elapsed" |
"remaining"` on `limit5h` plus an optional time-cursor glyph on its mini bar
(the bar shows usage; the cursor shows how far through the window we are, so
"60 % used at 20 % elapsed" is visible at a glance). Absolute reset times
(`resets 14:30`) with the local zone via jiff are the natural companion. Tick
class: payload-only.

**A11. `context` percentage of usable window.** `ContextPercentageUsable`
shows usage as a share of the autocompact threshold instead of the raw
window, and the transcript-derived context length resets to zero after a
`compact_boundary` row. garnish already knows the threshold
(`compact_buffer_tokens`, § 2.3). Proposal: `scale = "window" | "usable"` on
`context`; the bar's 100 % becomes the autocompact point. Tick class:
payload-only.

**A12. `version` module.** The `version` payload field exists and has no
module; a dim `v2.1.261` costs nothing and helps bug reports.

**A13. Separator collapse and color inheritance.** ccstatusline drops a
manual separator whose neighbour rendered empty and paints a separator in
the color of the previous visible widget (`inheritSeparatorColors`).
garnish collapses around hidden modules already; the inheritance is a
`frame.separator_color = "inherit" | "frame" | <role>` key. Tick class:
payload-only.

### 5.2 Tier B — fits with a worker or cache adaptation

**B1. Real Powerline segments.** This is ccstatusline's headline visual and
the biggest layout change here. Their model (`src/utils/powerline.ts`,
`src/utils/powerline-theme-index.ts`, `src/utils/separator-index.ts`,
`src/types/PowerlineConfig.ts`): every widget becomes a segment with its own
background; a theme supplies fg/bg pairs that cycle per segment at three
color levels (16/256/truecolor variants of each theme); start and end caps
are arrays cycling per line; several separator glyphs cycle per segment
with an optional inverted-background variant; `autoAlign` pads segments to
equal widths across lines with a per-widget exclusion; `merge = true |
"no-padding"` joins a widget into the previous segment; `continueThemeAcrossLines`
keeps the cycle running from line to line; the FLEX sentinel `\x01FLEX_SEP\x01`
survives painting so flex spacing is resolved after colors. Enabling
Powerline in their TUI rewrites the config: default padding becomes a space,
manual separators are stripped, and the default theme is `nord-aurora`
(`buildEnabledPowerlineSettings`). Gradients collapse to their first stop in
this mode.

garnish's `frame.style = "powerline"` is a frame (caps on a rule), not
segments. Proposal: a new `[segments]` table, orthogonal to `frame`:

```toml
[segments]
enabled = true
theme   = "nord-aurora"            # or a list of {fg, bg} pairs under [[segments.palette]]
separators = ["", ""]            # cycled per segment; "" alone is the classic look
caps = { start = [""], end = [""] }
merge = []                         # module ids merged into their predecessor
continue_across_lines = true
```

Behaviour: each rendered module gets the next palette pair; separators are
painted fg = previous bg, bg = next bg; `align` already gives equal columns,
so `autoAlign` needs no separate key; `hide_when_empty` removes the segment
and its separator together. Frame and segments compose (a rule can still
fill the gap). Tick class: payload-only, but it doubles the SGR bytes per
row; measure. Why B rather than A: it touches the painter, the frame, the
alignment pass and the goldens together, and `theme.rs` needs a background
role set that does not exist today.

**B2. Git details in the existing worker.** ccstatusline's git widgets
cover staged/unstaged/untracked file counts, conflicted files, stash count,
short SHA, origin URL and origin owner, all via `execFileSync` per widget
with an in-memory plus on-disk cache keyed by command and invalidated by the
mtimes of `.git/HEAD` and `.git/index` with a 0–60 s TTL
(`src/utils/git.ts`; 0 means mtime-only). garnish's `branch` worker already
runs `status --porcelain=v2 --branch`, which contains everything except the
stash count. Proposal: expose the counts as options of `branch` rather than
new modules (`show = ["sha", "dirty", "counts", "stash", "conflicts"]`),
add `stash list --porcelain` to the same worker run, and take the
**invalidation lesson**: record the `.git/HEAD` and `.git/index` mtimes in
the cache entry so a commit or a stage between ticks is a miss even inside
the TTL (garnish already invalidates on `head`/`upstream`; the index mtime
covers the dirty flag). Tick class: cached worker, no new process type.

**B3. CI status on `pr`.** `GitCiStatus` runs `gh pr view --json
statusCheckRollup` (or `glab mr view --output json`), counts failing /
pending / success checks while ignoring `NEUTRAL` and `SKIPPED`, falls back
to `--repo <upstream>` when the PR lives in a fork, resolves ssh aliases via
`ssh -G`, detects self-hosted providers through `gh auth status --hostname`,
caches on disk for 30 s under a versioned key, and refreshes by re-spawning
itself detached with a lock that goes stale after 30 s
(`src/utils/git-review-cache.ts`, `src/utils/git-remote.ts`,
`src/widgets/GitCiStatus.ts`). garnish gets the PR number, URL and review
state from the payload and makes no network calls. Proposal, two options:

- **B3a (preferred on this machine).** Daniel's `shuck` daemon already
  follows the working tree's PR and its CI. If it persists state on disk,
  a `ci` option on `pr` can read that file with no process at all. Check
  what `shuck monitor` writes before choosing.
- **B3b.** A `garnish refresh --module pr` worker running `gh pr view --json
  statusCheckRollup` (a subprocess that talks to the network) with the
  existing lock and TTL machinery, 60 s TTL, failure cached like any other
  entry. This lifts "no network calls" indirectly, so it is a decision
  (§ 9) even though the tick itself stays payload-plus-cache.

Rendering: `✓ 12` / `● 3` / `✗ 1` after the state glyph, colored ok / warn /
danger.

**B4. Active skill via Claude hooks.** ccstatusline registers a
`PreToolUse` hook with matcher `Skill` and a `UserPromptSubmit` hook that
call it in `--hook` mode; the handler appends
`{skill, ts}` rows to `~/.cache/ccstatusline/skills/skills-<session>.jsonl`,
and the widget shows the last skill until the next prompt clears it
(`src/utils/hooks.ts`, `src/utils/hook-handler.ts`, `src/widgets/Skills.ts`).
The hooks are written into Claude settings tagged `_tag: ccstatusline-managed`
so install/uninstall can find them. garnish already edits `settings.json` for
`install` and preserves key order. Proposal: `garnish install --hooks` adds
the two hooks (tagged the same way), `garnish hook <event>` is a hidden
subcommand that appends to `<cache>/<session>/skills.jsonl`, and a `skill`
module reads the last row. Cost: a new hidden subcommand, a new file class
under the cache dir for `gc`, and a documented settings edit. Tick class:
cached file read.

### 5.3 Tier C — needs a non-goal lifted or a new crate (Daniel decides)

**C1. Transcript-derived metrics.** Compaction counter (rows with
`type: "system"`, `subtype: "compact_boundary"`, `compactMetadata.trigger`
auto/manual and `preTokens`/`postTokens` reclaimed — `src/utils/compaction.ts`),
token speed (output tokens over a rolling 0–120 s window with merged
intervals, subagent transcripts under `subagents/agent-<id>.jsonl` included —
`src/utils/speed-metrics.ts`, `src/utils/speed-window.ts`), and a split of
cache-read vs cache-write tokens. Their reader streams the JSONL once and
keeps a reverse iterator for tail lookups (`src/utils/jsonl-*.ts`).
`SPEC.md` § 2.2 marks `transcript_path` "not used". Cost: a bounded tail read
(last N KB) in a cached worker keyed by file size and mtime; the tick reads
the cache. Value: the compaction counter is the one users ask for most and
has no payload equivalent. Decision: lift "transcript not used" for a
`compactions` module (and optionally `speed`), tail-read only, worker-only.

**C2. Usage API for per-model weekly limits and extra usage.** The harness
payload in 2.1.261 builds `rate_limits` from exactly three windows:
`five_hour`, `seven_day`, and `spend_limit` (the last only for gateway
sessions) — verified in the `ulo(...)` payload builder in the binary. The
richer schema with `seven_day_sonnet`, `seven_day_opus`, `model_scoped[]`
(`display_name`, `utilization`, `resets_at`) and `extra_usage`
(`is_enabled`, `monthly_limit`, `used_credits`, `utilization`, `currency`)
exists in the binary but belongs to a different structure (the usage
dialog / SDK session object), not the status line payload. ccstatusline
therefore calls `GET https://api.anthropic.com/api/oauth/usage` itself with
`Authorization: Bearer <claudeAiOauth.accessToken>` from
`~/.claude/.credentials.json` (or the macOS keychain item "Claude
Code-credentials") and `anthropic-beta: oauth-2025-04-20`, honours
`HTTPS_PROXY`, caches 180 s, locks 30 s, backs off on 429 `Retry-After`,
treats a lock older than 24 h as poisoned, and invalidates the cache when
the token hash changes (`src/utils/usage-fetch.ts`, `usage-prefetch.ts`,
`usage-windows.ts`). Cost for garnish: `reqwest` (the crate map's chosen
HTTP crate), reading the user's OAuth token, and lifting "no network
calls". Would be opt-in (`[usage] api = true`), network-worker only, cache
served stale on failure. Decision: whether per-model limits and extra-usage
credits are worth a network dependency and token handling at all, or whether
to wait for the harness to add them to the payload (the schema suggests it
is moving that way).

**C3. Claude service status.** `ClaudeStatus` polls
`status.claude.com/api/v2/status.json` and `incidents.json` every 5 min,
backs off 30 s on failure, and renders a 48-hour strip of eight six-hour
buckets (`▮`) colored by worst impact (`src/utils/claude-service-status.ts`,
`src/widgets/ClaudeStatus.ts`). Same cost class as C2 (network, `reqwest`),
no credentials. Decision: bundle with C2 or skip.

**C4. Interactive setup TUI.** See § 7. Needs `ratatui` + `crossterm` (two
crates) or a prompt-only wizard (zero crates).

**C5. Update check.** ccstatusline queries the npm registry from the TUI and
offers the install command (`src/utils/update-checker.ts`). For garnish this
would be a crates.io or GitHub Releases request from `doctor` or the setup
TUI, never from the tick. Same network decision as C2/C3; low value while
installs are `cargo install --locked`.

### 5.4 Tier D — deliberately not ported

- **CustomCommand** (arbitrary shell): `SPEC.md` § 3.7 rules it out; its
  existence is why they need a per-widget timeout and a "preserve rendered
  colors" flag.
- **Windows** support and the `docs/WINDOWS.md` caveats: non-goal.
- **jj widgets**: no demand.
- **CacheTimer heuristics**: `prompt_cache.expires_at` is exact.
- **BlockTimer transcript inference**: `resets_at` is exact (A10).
- **`flexMode = full-minus-40`**: a heuristic for the autocompact notice
  wrapping the line; garnish's width is exact. Replace with a verify item:
  confirm where the autocompact notice renders in 2.1.261 relative to the
  status line and whether it can shorten the box (`CLAUDE.md` already notes
  the mode/shortcut block wraps below when the status line is wide).
- **NBSP substitution**: they replace spaces with U+00A0 so VS Code's
  terminal does not trim trailing padding. It would break garnish's width
  math if adopted blindly (NBSP is a real cell, but trailing NBSP changes
  copy/paste and the harness's `trim()` treats it as non-whitespace, so a
  spacer row would suddenly render). Verify in VS Code whether garnish's
  right group is affected; only then decide.
- **FreeMemory / memory usage**: host telemetry, not session state.
- **npm-style auto-update** (`npx -y ccstatusline@latest`): the pinned-vs-
  latest lesson (200 ms vs 1.1 s startup) translates to recommending
  `garnish install --absolute` as the default.

## 6. Implementation lessons to adopt

Cheap habits worth copying into garnish regardless of which features land.

1. **Settings recovery contract.** Their loader migrates, then validates,
   then persists the migrated file; an invalid file is never overwritten, the
   process runs on in-memory defaults, and the line shows a red
   `⚠ invalid config` badge (`src/utils/config.ts`). garnish's `config check`
   and the `⚠ garnish:` line match this; add the "never overwrite a file we
   could not parse" sentence to `SPEC.md` § 5 so a future `config migrate`
   or a TUI honours it.
2. **Cache poison horizon.** A lock older than 24 h is treated as abandoned
   even if the pid looks alive. garnish uses `/proc/<pid>` liveness and a
   grace window; a horizon is a one-line safety net against pid reuse.
3. **Stale-serve on fetch failure** and **failure cached for a full TTL** —
   garnish already does the latter (`CLAUDE.md` invariants); the former is
   what any network worker must do.
4. **Token-hash / situation-keyed cache entries.** garnish's `head`/`upstream`
   validator is the same idea; extend it with the `.git/index` mtime (B2).
5. **`GIT_OPTIONAL_LOCKS=0`** — already set in `src/git.rs`; keep it.
6. **Widget interface as a checklist.** Their `Widget` interface names every
   capability a widget may opt into (`supportsRawValue`, `supportsColors`,
   `supportsNumberFormat`, `getHideableStates`, `preservesRenderedColors`).
   garnish's `ModuleSchema` is the equivalent; A4 and A6 add `hide_states`
   and `number_kinds` to it so docs and the TUI (§ 7) can be generated from
   one place.
7. **Fuzzy widget picker with initialism matching** (`src/utils/fuzzy.ts`):
   `gab` finds `GitAheadBehind`. Worth copying into any picker garnish ships.
8. **Config templates as JSON files in the repo** (`configTemplates/`),
   loaded by the TUI as starting points: garnish's `presets/` gallery (§ 12)
   is the same idea and should be what a setup TUI offers first.
9. **Publish workflow**: npm provenance plus `gh release create` on `v*`
   tags. garnish's equivalent is a release job attaching Linux and macOS
   binaries to the tag so `install` can point at a URL for people without a
   Rust toolchain. Not urgent; note for v0.2.0.

## 7. The interactive installer

### 7.1 What ccstatusline does

Running the binary on a TTY (no piped stdin) opens the TUI
(`src/tui/App.tsx`). Its structure:

- **Live preview** at the top of every screen, rendered by the same renderer
  as production at the current terminal width, with a truncation warning
  when a line would overflow.
- **Main menu**: Edit Lines, Edit Colors, Powerline Setup, Terminal Options,
  Global Overrides, Configure Status Line (refresh interval 1–60 s, git
  cache TTL), Export / Import Config (JSON to the clipboard or a file, with a
  replace-or-merge preview before applying), Install / Manage Installation,
  Check for Updates, Star on GitHub, Save & Exit, Exit without saving;
  `Ctrl+S` saves from anywhere; an unsaved-changes dialog guards exit.
- **Line editor**: a list of widgets per line with move / add / remove /
  edit; a **widget picker** with fuzzy and initialism search grouped by
  category; per-widget keybinds shown in a footer that lists only the keys
  that apply to the highlighted widget (`getCustomKeybinds`); a per-widget
  editor for raw value, colors, number format, hide states, symbols.
- **Colors**: theme selection with a "customize" action that copies the
  theme's colors onto each widget so they can be edited individually;
  changing the color level (16/256/truecolor) sanitises colors that the
  new level cannot show.
- **Powerline setup**: enable/disable, theme, separators, caps, font
  detection (scans font directories and `fc-list`) with an offer to install
  the powerline fonts (clone `powerline/fonts`, run its installer,
  `fc-cache`).
- **Install flow**: choose "Pinned global install" (`npm install -g` or
  `bun add -g` at the exact version; command `ccstatusline`) or
  "Auto-update" (`npx -y ccstatusline@latest`); a confirmation dialog lists
  every side effect before anything is written (settings path, global
  install command, final `statusLine.command`, hook command); backups are
  written as `.orig` (first ever) and `.bak` (latest); an existing
  `statusLine` triggers a warning; a pinned version that differs from the
  running one gets its own screen; uninstall offers to remove the status
  line, the hooks, or both.
- **Manage installation**: shows the current command, detects the install
  style, checks the registry for a newer version.

### 7.2 What garnish can do better

- garnish re-reads its TOML every tick, so a TUI that edits the live file
  is reflected in Claude Code within a second: no save-to-apply step and no
  "unsaved changes" dialog are needed, only an undo (keep a `.bak`).
- The preview can be exact: garnish knows the box width from `COLUMNS`
  (§ 3.3) and has `preview` fixtures; ccstatusline's preview is at terminal
  width minus a guess.
- Every option, its type, default and doc string already lives in
  `ModuleSchema`; a TUI's per-module editor can be generated from it, as the
  docs are, so a new option never needs TUI code.
- The `presets/` gallery gives the picker thirteen complete starting points
  instead of an empty line.

### 7.3 Options for garnish (Daniel's choice)

| option | dependencies | what it gives | cost |
|---|---|---|---|
| **7.3a Skill only** (already planned, `SPEC.md` § 13, Phase 18) | none | Claude edits the config in conversation, with `preview` for the check | no visual picker; not what Daniel said he liked |
| **7.3b Prompt wizard** `garnish setup` | none | numbered menus on stdin/stdout: preset, icons, theme, frame, lines; writes the config and runs `install`; prints a preview after each step | no live preview pane; ~500 lines |
| **7.3c Full TUI** `garnish setup` | `ratatui` + `crossterm` | the ccstatusline experience: preview pane, line editor, schema-driven module editor, theme/frame pickers, install and doctor screens, import/export | two new crates (a crate-map decision), a new binary-size and compile-time cost, a large test surface (snapshot tests via ratatui's `TestBackend`), ~3–4 k lines |

Recommendation if 7.3c is chosen: keep the TUI in its own module tree behind
a cargo feature (`--features setup`) so the tick path and `bench/run.sh` are
unaffected, enter it only when `garnish` runs on a TTY with no stdin (the
same heuristic ccstatusline uses, so `garnish` alone opens setup and
`garnish < payload.json` renders), and generate every editor screen from
`ModuleSchema`. The install screen should mirror their confirmation dialog:
list the settings path, the exact `statusLine.command`, the backup name and
whether hooks (B4) will be added, then ask once.

## 8. Spec changes this would imply

### 8.1 Non-goals

| current non-goal | proposals that touch it |
|---|---|
| no network calls | B3b (`gh` subprocess), C2, C3, C5 — all worker-only, never on the tick, opt-in |
| `transcript_path` not used | C1 — bounded tail read in a worker |
| fixed module set | A9 (`sandbox`, `remote`, `account`), A12 (`version`), B4 (`skill`), C1 (`compactions`, `speed`), C3 (`status`) — still fixed, just larger |
| no daemon | untouched; every proposal is a one-shot worker or a file read |
| no Windows | untouched |

### 8.2 Crate map candidates

`reqwest` for C2/C3/C5 (already named as the chosen HTTP crate);
`ratatui` and `crossterm` for 7.3c. Gradients (A3) need no crate: OKLab is
~40 lines of arithmetic.

### 8.3 Claude Code facts verified 2026-09-05 (v2.1.261) while writing this

- Each status line row is rendered as `<Text dimColor wrap="truncate">`
  (A1). The output rows are otherwise handled as `CLAUDE.md` describes.
- The payload's `rate_limits` is built from `five_hour`, `seven_day` and,
  for gateway sessions only, `spend_limit`; `seven_day_sonnet`,
  `seven_day_opus`, `model_scoped` and `extra_usage` are **not** in the
  status line payload even though the binary has them in another schema.
  ccstatusline's stdin schema declares `seven_day_sonnet` / `seven_day_opus`
  optionally; nothing fills them today. `SPEC.md` § 2.2 is correct as it
  stands; revisit after each upgrade.
- The binary contains the `api/oauth/usage` path and the `weekly_scoped`
  limit kind, confirming the API shape ccstatusline consumes.

### 8.4 Payload table

No change needed now. If a later version adds `model_scoped` or
`extra_usage` to the payload, C2 becomes a Tier A payload-only module and
should be re-tiered before anything network-related is built.

## 9. Decisions needed

- [ ] **Network.** Lift "no network calls" for opt-in, worker-only fetches
      (C2 usage API, C3 service status, B3b `gh`)? If yes, `reqwest` joins
      the crate map. If no, C2/C3/C5 are closed and B3 is B3a only.
- [ ] **Transcript.** Allow a bounded tail read of `transcript_path` in a
      worker for `compactions` (and `speed`)? (C1)
- [ ] **Installer form.** 7.3a skill only, 7.3b prompt wizard, or 7.3c
      `ratatui` TUI behind a feature flag?
- [ ] **Hooks.** May `garnish install --hooks` write two tagged hooks into
      Claude settings for a `skill` module? (B4)
- [ ] **Module count.** Accept growing the fixed set beyond 21 for A9, A12,
      B4 (and C1/C3 if approved)?
- [ ] **Segments.** Is the Powerline-segment look (B1) wanted enough to
      justify a background role set in `theme.rs` and a second painter path?
- [ ] **Verify items** (no decision, just work): A1 dim reset on screen;
      NBSP behaviour in VS Code; where the autocompact notice renders.

## 10. Suggested phase order after Phase 18

Grouped so each phase is one PR-sized concern and the cheap, invariant-safe
work lands first; network and TUI phases wait on § 9.

| phase | concern | contents |
|---|---|---|
| 19 | harness fidelity | A1 dim reset (verify, then golden), NBSP/autocompact verify items, A12 `version` |
| 20 | per-module presentation | A4 `hide = [...]`, A5 `max_width`, A6 `[format]` + `dim = "parens"`, A7 path styles, A13 separator color |
| 21 | links and identity | A8 links, A9 `sandbox` / `remote` / `account` |
| 22 | usage views | A10 `limit5h` elapsed + absolute reset times, A11 `context` usable scale |
| 23 | color | A3 gradients (OKLab, presets with attribution, 256/mono degrade) |
| 24 | layout | A2 center group; B1 Powerline segments if approved |
| 25 | git worker | B2 counts/stash/conflicts on `branch`, index-mtime invalidation; B3a `ci` from shuck state if available |
| 26 | hooks | B4 `skill` module and `install --hooks` (if approved) |
| 27 | transcript worker | C1 `compactions`, `speed` (if approved) |
| 28 | network workers | C2 usage API, C3 service status, B3b (if approved; adds `reqwest`) |
| 29 | setup | 7.3b or 7.3c `garnish setup` (if approved), import/export, doctor screen |

Each of these follows the phase protocol in `CLAUDE.md`: `SPEC.md` first,
then code, then the adversarial review. When a phase is started, move its
text out of this file into `SPEC.md` and delete it here, so this document
shrinks to what is still undecided.

## Appendix — where to look in ccstatusline (commit 016be1f)

| topic | files |
|---|---|
| entry, mode selection (`--hook`, `--config`, TTY → TUI) | `src/ccstatusline.ts` |
| settings schema, migrations, recovery | `src/types/Settings.ts`, `src/utils/config.ts`, `src/utils/migrations.ts` |
| renderer: flex, padding, powerline, gradients, NBSP, dim reset, truncation | `src/utils/renderer.ts`, `src/utils/powerline.ts`, `src/utils/gradient.ts`, `src/utils/colors.ts`, `src/utils/number-format.ts` |
| terminal width probe | `src/utils/terminal.ts` |
| git, PR/CI, remotes | `src/utils/git.ts`, `src/utils/git-review-cache.ts`, `src/utils/git-remote.ts`, `src/widgets/GitPr.ts`, `src/widgets/GitCiStatus.ts` |
| usage API | `src/utils/usage-fetch.ts`, `usage-prefetch.ts`, `usage-windows.ts`, `usage-types.ts` |
| service status | `src/utils/claude-service-status.ts`, `src/widgets/ClaudeStatus.ts` |
| transcript readers | `src/utils/jsonl-*.ts`, `compaction.ts`, `speed-metrics.ts`, `speed-window.ts` |
| hooks and skills | `src/utils/hooks.ts`, `hook-handler.ts`, `skills.ts`, `src/widgets/Skills.ts` |
| layered Claude settings, sessions, account | `src/utils/claude-settings.ts`, `src/widgets/SandboxStatus.ts`, `RemoteControlStatus.ts` |
| block / cache timers | `src/widgets/BlockTimer.ts`, `BlockResetTimer.ts`, `CacheTimer.ts` |
| links | `src/utils/hyperlink.ts`, `src/widgets/Link.ts`, `GitBranch.ts`, `GitRootDir.ts` |
| widget registry and interface | `src/utils/widget-manifest.ts`, `src/types/Widget.ts`, `src/widgets/index.ts` |
| TUI | `src/tui/App.tsx`, `src/tui/components/*` (install flow, powerline setup, widget picker, list, color editor) |
| fuzzy search | `src/utils/fuzzy.ts` |
| update check | `src/utils/update-checker.ts` |
| CI / publish | `.github/workflows/ci.yml`, `.github/workflows/publish.yml` |
