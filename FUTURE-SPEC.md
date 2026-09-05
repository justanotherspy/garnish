# FUTURE-SPEC.md — ideas mined from ccstatusline

**Status: proposals, not decisions.** Nothing in this file is part of the
target design until it is moved into `SPEC.md` (with the reason) and given a
phase in `PLAN.md`. It exists so that the next planning session starts from
an inventory of what a mature competitor ships, what garnish already does
better, and which ideas are worth the cost of porting.

Sources studied (all MIT), read on 2026-09-05/06 from shallow clones in a
job scratch directory that is not kept:

| project | what it is | version read |
|---|---|---|
| [sirmalloc/ccstatusline](https://github.com/sirmalloc/ccstatusline) | the mature status line, Node/TypeScript with a React/Ink TUI | commit `016be1fcf19453bd4362439b197e9cf841d7006a`, v2.2.29 |
| [krayong/ccsidekick](https://github.com/krayong/ccsidekick) | a status line with a reacting ASCII character, tips, cost engine | v1.8.0 |
| [vincent-k2026/codachi](https://github.com/vincent-k2026/codachi) | a tamagotchi pet in the status line | v0.3.0 |
| [refinist/ccstatusline-editor](https://github.com/refinist/ccstatusline-editor) | a browser editor for ccstatusline configs with share links | v2.2.26-ccse.1 |

File paths in Parts I–II are relative to the ccstatusline clone; Parts III
and IV name their repository. Facts about Claude Code were re-verified
against the 2.1.261 binary (see § 8.3).

How to read this. **Part I** (§ 1–10) is the decision document for
ccstatusline: § 3 lists what garnish must not give up while porting
anything, § 4 is the widget inventory, § 5 holds the proposals tiered by
cost (**A** fits every current invariant, **B** needs a worker or cache
adaptation, **C** needs a `SPEC.md` non-goal lifted or a new crate and is a
decision for Daniel, **D** is deliberately not ported), § 7 the interactive
installer, § 9 the decision checklist, § 10 a phase order after Phase 18.
**Part II** (§ 11–22) records how ccstatusline built each thing, close to
the code, and the Rust shape for garnish. **Part III** (§ 23–24) mines the
two companion projects into a garnish companion design. **Part IV**
(§ 25–26) covers the web editor and what garnish takes from it. Each part
ends with its own decision list; § 10 collects the phase order for all of
them.

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

The three smaller projects, on 2026-09-06:

| project | stars / forks | created | last push | note |
|---|---|---|---|---|
| ccsidekick | 30 / 1 | 2026-07-06 | 2026-09-02 | active; 18 packs, plugin marketplace, landing site |
| codachi | 12 / 5 | 2026-04-02 | 2026-04-18 | small and finished-looking; the pet mechanics are the value |
| ccstatusline-editor | 12 / 0 | 2026-07-03 | 2026-07-25 | tracks ccstatusline releases by version suffix |

An ecosystem has formed around ccstatusline: a config gallery
(statuslin.es), the browser editor above, a companion apply CLI
(`@refinist/ccsa`), and interoperability with `ccusage`. Its README explicitly answers "why is it slow" with a startup
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
      NBSP behaviour in VS Code; where the autocompact notice renders; the
      `spinnerVerbs` settings key (§ 24.9).
- [ ] **Companion** (Part III): see § 24.11.
- [ ] **Sharing, rotation, web preview** (Part IV): see § 26.1.

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
| 30 | sharing and rotation | `config share` / `config apply` / `preview --config` / `preview --html`; theme and preset rotation keys (§ 26) |
| 31 | companion core | `garnish hook` classifier and event summary (shared with B4), `pet` one-row with the `sprig` voice, `say`, mood and pressure, `preview --live` (§ 24.1–24.6) |
| 32 | companion memory and tips | memory file, tiers, `garnish stats`, `tip` catalog, pace delta and pace colors on the limit modules, `provider`, branch operation glyph (§ 24.7–24.10) |
| 33 | companion gutter and packs | `[gutter]` three-row layout, `pack.toml` static art, spinner verbs after the binary check (§ 24.4–24.5, 24.9) |

Each of these follows the phase protocol in `CLAUDE.md`: `SPEC.md` first,
then code, then the adversarial review. When a phase is started, move its
text out of this file into `SPEC.md` and delete it here, so this document
shrinks to what is still undecided.

---

# Part II — Rust designs, translated from the Node implementation

Part I says *what* to port and why. Part II records *how ccstatusline built
each thing*, close enough to the code that a garnish phase can be planned
from it without re-reading their repository, and what the Rust version
should do differently. Every subsection ends with the garnish shape.

## 11. Line composition: flex points, separators, merge, padding

### 11.1 How ccstatusline composes a line (`renderStatusLine`, `src/utils/renderer.ts`)

1. **Pre-render once.** Every widget of every line is rendered to plain text
   first (`preRenderAllWidgets`); layout decisions use these strings, so a
   widget is never rendered twice and empty widgets are known before
   separators are placed.
2. **Manual separators collapse.** A `separator` item is kept only if some
   widget *before it in the same flex part* rendered content; walking back,
   a run of default "spacing" separators is replaced by the manual one, and
   an empty widget in between is skipped unless it is merged into the one
   before it. Trailing separators are popped. When the width is unknown, a
   spacing separator next to a flex point is dropped too.
3. **Default separator insertion.** Between consecutive elements unless one
   of them is a flex point or the previous widget has `merge` set. With
   `inheritSeparatorColors` the separator takes the previous widget's fg/bg,
   bold and dim.
4. **Padding.** `defaultPadding` (a string, usually one space) is split by
   `defaultPaddingSide` into leading/trailing pieces; a widget with a
   background paints its padding in that background; `merge = "no-padding"`
   drops the padding between the merged pair.
5. **Flex distribution.** The element list is split at flex points into
   parts; `total = width − Σ visible(part)`, `per = ⌊total / n⌋`,
   `extra = total mod n`; the first `extra` gaps get one more cell. Without a
   known width a flex renders as a gray ` | `.
6. **Truncate** the finished string to the width with an ANSI/OSC-aware
   `…`, then apply a whole-line gradient if a gradient foreground override is
   set (after truncation so the trailing reset survives).

### 11.2 garnish shape

garnish already has steps 1, 2 (via `hide_when_empty` collapse), 4 and 6 in
`frame.rs`/`render.rs` for two groups. The generalisation:

```rust
/// One run of modules between flex points. `Left`/`Center`/`Right` today;
/// a `Vec<Group>` of any length is the same algorithm.
struct Group { segments: Vec<Rendered>, align_columns: Vec<usize> }

fn distribute(width: usize, groups: &[Group]) -> Vec<usize> {
    let used: usize = groups.iter().map(Group::width).sum();
    let Some(gaps) = NonZeroUsize::new(groups.len().saturating_sub(1)) else {
        return Vec::new();
    };
    let total = width.saturating_sub(used);
    let (per, extra) = (total / gaps, total % gaps);   // NonZero: no panic path
    (0..gaps.get()).map(|i| per.saturating_add(usize::from(i < extra))).collect()
}
```

Overflow keeps the `SPEC.md` § 4 rule (drop the fill, cut the left group,
never the right); with a center group the cut order is left, then center.
Their `merge` becomes a per-module `glue = true` in garnish ("no separator
after me; with `glue = "tight"` no padding either"), which is also how a
`text.<name>` label can sit flush against the module it labels. Their
`merge-target-hidden` hide state ("a decorative item hides when the widget
it is glued to hides") is the missing half of garnish § 3.7 text modules:
`hide_with = "pr"` on a text module makes a label vanish with its module.

## 12. Powerline segments: the painter algorithm

### 12.1 Their algorithm (`renderPowerlineStatusLine`)

Inputs per line: the rendered widgets (separators filtered out; flex points
kept as positions), the theme's `fg[]`/`bg[]` arrays for the current color
level, `separators[]` with a parallel `separatorInvertBackground[]`,
`startCaps[]`, `endCaps[]`, `autoAlign`, `continueThemeAcrossLines`, and
three counters carried across lines: separator index, theme color index,
start-cap slot index.

1. **Elements.** For each rendered widget: pad the text (respecting
   `no-padding` merges), pick `fg = widget.color ?? default`, `bg =
   widget.backgroundColor`; if a theme is active, `fg/bg = theme[idx mod
   len]`, and `idx` advances only when the widget does **not** merge with
   the next one, so a merged pair shares a color. A widget that preserves
   its own colors keeps its fg but still takes the theme bg.
2. **Flex bookkeeping.** Flex points are counted against *rendered*
   elements (an empty widget must not move a flex slot). Each flex starts a
   new "segment offset", which selects the next start/end cap in the cap
   arrays.
3. **Auto-align.** Element *k* (merged runs count as one) is padded on the
   right to the pre-computed column width; an element flagged
   `excludeFromAutoAlign` stops alignment for the rest of the line.
4. **Emission.** For each element: optional start cap painted `fg =
   bgToFg(element.bg)` with no background; then `SGR 1`/`SGR 2` if bold/dim,
   fg code, bg code, text (parens-dim applied if `dim = "parens"`), then
   `\x1b[49m\x1b[39m` and an intensity reset chosen so bold survives up to
   the separator (`\x1b[22;1m` when dim+bold and a separator or end cap
   follows). Then either an end cap + flex sentinel(s) or a **separator**:
   - normal: `fg = bg(this)`, `bg = bg(next)`; if both backgrounds are
     equal, `fg = fg(this)` instead so the glyph stays visible;
   - inverted (`separatorInvertBackground[i]`): `fg = bg(next)`,
     `bg = bg(this)`; same-bg case uses `fg(next)`;
   - only one side has a background: paint the glyph in that background's
     color as foreground, no background;
   - after the glyph, `\x1b[22m` if the element was bold or dim.
   The separator glyph index is `(globalOffset + local) mod len`.
5. **End cap** after the last element unless a flex follows it (the flex
   already emitted one). Flex sentinels are then split and spaced exactly
   as in § 11.1 step 5, the line is truncated to width, and a
   `chalk.reset('')` is appended.
6. **Cross-line state** after a line is printed: separator index advances
   by the separators actually emitted, the theme index by the number of
   color-consuming elements (only if `continueThemeAcrossLines`), the cap
   slot index by the number of segments on the line.

Enabling Powerline in their TUI rewrites the config (`buildEnabledPowerlineSettings`,
`src/utils/powerline-settings.ts`): default padding becomes `" "`, every
manual separator is removed, theme defaults to `nord-aurora`; fonts are
detected by scanning font directories for names matching
`/powerline|nerd font|meslo.*lg|cascadia.*code.*pl|fira.*code.*nerd/i` and
by `fc-list | grep -i powerline`, with an offer to clone
`powerline/fonts` and run its `install.sh` (`src/utils/powerline.ts`).

### 12.2 Their theme table (truecolor level; MIT, ccstatusline `src/utils/colors.ts`)

Each theme is five `(fg, bg)` pairs cycled per segment; 16- and 256-color
variants are hand-picked in the source. garnish can derive the 256 level
with the mapper in § 13.3 and keep only a hand-picked 16-color row.

| theme | bg₁ | bg₂ | bg₃ | bg₄ | bg₅ | fg rule |
|---|---|---|---|---|---|---|
| nord | `88C0D0` | `4C566A` | `5E81AC` | `B48EAD` | `A3BE8C` | dark on light, `D8DEE9`/`FDF6E3` on dark |
| nord-aurora (default) | `BF616A` | `EBCB8B` | `5E81AC` | `A3BE8C` | `B48EAD` | `ECEFF4` on red, `2E3440` on the rest, `FDF6E3` on blue |
| monokai | `A6E22E` | `49483E` | `E6DB74` | `AE81FF` | `66D9EF` | `272822`, `F8F8F2` on the gray |
| solarized | `268BD2` | `B58900` | `586E75` | `2AA198` | `EEE8D5` | `073642`, `FDF6E3` on the gray |
| minimal | `585858` | `D0D0D0` | `1A1A1A` | `A8A8A8` | `303030` | white/black alternating |
| dracula | `BD93F9` | `F8F8F2` | `FF5555` | `8BE9FD` | `44475A` | `282A36`, `F8F8F2` on the last |
| catppuccin | `CBA6F7` | `45475A` | `A6E3A1` | `F38BA8` | `585B70` | `1E1E2E`, `CDD6F4` on the grays |
| gruvbox | `CC241D` | `FABD2F` | `A89984` | `458588` | `98971A` | `EBDBB2`/`FDF6E3` on red/blue, `282828` elsewhere |
| onedark | `61AFEF` | `3E4452` | `98C379` | `E06C75` | `E5C07B` | `282C34`, `ABB2BF` on the gray |
| tokyonight | `7AA2F7` | `D5D6DB` | `BB9AF7` | `E0AF68` | `7DCFFF` | `1A1B26` throughout |

garnish already ships catppuccin-mocha, nord, dracula and tokyonight as
foreground themes; a `[segments]` palette per theme is five extra pairs in
`theme.rs`, not a new theme system.

### 12.3 garnish shape

```rust
/// Painted per line after modules rendered; pure function of its inputs.
struct SegmentPlan<'a> {
    parts: Vec<Vec<Seg<'a>>>,      // split at flex points
    palette: &'a [(Rgb, Rgb)],     // (fg, bg) cycled per color-consuming segment
    seps: &'a [SepGlyph],          // { glyph: &str, invert: bool }
    caps: Caps<'a>,                // start/end arrays cycled per part
    carry: &'a mut Carry,          // sep_idx, theme_idx, cap_idx across lines
}
fn separator(prev: &Seg, next: &Seg, g: &SepGlyph) -> Ansi { /* § 12.1 step 4 */ }
```

Differences worth making: (a) alignment reuses garnish's `align` columns
rather than a second `autoAlign`; (b) the "same background" rule should
pick a contrasting fg from the theme (their `fg(this)`) — keep it; (c)
garnish knows the exact width, so the flex split happens before painting
and the sentinel trick is unnecessary; (d) `hide_when_empty` removes the
segment *and* its palette slot, so colors do not shift when a module hides
(theirs advances the theme index only for rendered elements, which is the
same behaviour; keep it); (e) `color = never`/`mono` renders segments as
plain text with the frame separator, never as unreadable same-color blocks.
Output cost: two SGR sequences and two resets per segment, about 40 bytes;
a 4-line, 16-module layout adds ~1 KB per tick. Within budget, but pin it
in `bench/`.

## 13. Gradients

### 13.1 Grammar and presets (`src/utils/gradient.ts`)

`gradient:<name>` | `gradient:<stop>-<stop>[-…]` | `gradient:<stop>,<stop>[,…]`
where a stop is `RRGGBB`, `#RRGGBB` or `hex:RRGGBB`; the delimiter is `,`
if the body contains one, else `-`; fewer than two valid stops → not a
gradient. Presets (from gradient-string, MIT, re-expressed as explicit
stops because interpolation is OKLab rather than an HSV hue spin):

| name | stops |
|---|---|
| atlas | `feac5e c779d0 4bc0c8` |
| cristal | `bdfff3 4ac29a` |
| teen | `77a1d3 79cbca e684ae` |
| mind | `473b7b 3584a7 30d2be` |
| morning | `ff5f6d ffc371` |
| vice | `5ee7df b490ca` |
| passion | `f43b47 453a94` |
| fruit | `ff4e50 f9d423` |
| instagram | `833ab4 fd1d1d fcb045` |
| retro | `3f51b1 5a55ae 7b5fac 8f6aae a86aa4 cc6b8e f18271 f3a469 f7c978` |
| summer | `fdbb2d 22c1c3` |
| rainbow | `ff0000 ffff00 00ff00 00ffff 0000ff ff00ff ff0000` |
| pastel | `aee9d8 cdeeb0 f6f0a8 f7c8a8 f3aecb c3b6f0 aee9d8` |

### 13.2 Sampling and application

- sRGB → linear (`c ≤ 0.04045 ? c/12.92 : ((c+0.055)/1.055)^2.4`) → OKLab
  (Björn Ottosson's matrices; the inverse is the `4.0767416621 …` matrix in
  their source) → interpolate `L, a, b` linearly between the two bracketing
  stops: `scaled = t·(n−1)`, `lower = min(n−2, ⌊scaled⌋)`, `frac = scaled −
  lower` → back to sRGB, clamped.
- Per widget: one SGR per **non-whitespace** code point; whitespace passes
  through and does not consume a step; CSI/OSC sequences pass through
  untouched; the sweep restarts at `t = 0` per widget. Per line: the sweep
  spans the visible cells of the whole line (walking display clusters), and
  is applied after truncation.
- ansi16 → no gradient (first stop or plain). Powerline mode → background
  collapses to the first stop; a foreground gradient may still sweep across
  all non-color-preserving segments (`powerlineGradientWidth`).
- Their known limitation: the per-widget path walks code points, so a ZWJ
  emoji gets several steps and inert codes on zero-width joiners.

### 13.3 xterm-256 mapping

```text
gray (r == g == b):  r < 8 → 16;  r > 248 → 231;  else 232 + round((r − 8) / 247 · 24)
else:                16 + 36·round(r/255·5) + 6·round(g/255·5) + round(b/255·5)
```

### 13.4 garnish shape

`theme.rs` gains `enum Paint { Solid(Rgb), Gradient(Vec<Rgb>) }` wherever a
role or module color is resolved; `ansi.rs` gains
`paint_gradient(text, stops, level)`. Do better than their limitation
without a new crate: step the gradient per **cell**, using `unicode-width`
— a zero-width code point (ZWJ, variation selector, combining mark) gets no
SGR and no step, a wide glyph consumes two steps, so the sweep is uniform in
terminal cells. `color = 256` maps each sample with § 13.3; `mono` and
ansi16 fall back to the role's solid color. Keys: any color value may be a
gradient string; `[line].gradient = "<spec>"` sweeps the line. Docs list
the presets from a `GRADIENT_PRESETS` table so `garnish docs` stays the
source of truth.

## 14. Number formats and dim-parens

- Kinds: `tokens | speed | percent | memory | cost`; style `precise |
  compact | whole`; optional `decimals`. Resolution order: global per-kind
  → per-widget → the widget's own baseline (percent 1 decimal, context bar
  0, cost 2). `compact` trims trailing zeros (`512.0 → 512`, `5.2 → 5.2`);
  `whole` forces 0 decimals. The TUI cycles precise → compact → whole with
  `.`.
- `dim = "parens"` dims every `(...)` span: `\x1b[2m(...)\x1b[22m`, re-asserting
  bold with `\x1b[22;1m` when the surrounding text is bold, because SGR 22
  clears both.

garnish shape: `num.rs` gets `struct NumFormat { style, decimals }` and a
`[format]` table with those five kinds; module schemas declare which kind
each number is so docs can say what `format.tokens` affects. `dim =
"parens"` is a segment-level flag: garnish already knows which segments are
"detail" (the full-preset extras), so dim those segments rather than regex
over the rendered text.

## 15. Hide states

Observed vocabulary and where it applies (`getHideableStates` across
`src/widgets/`, shared constants in `src/widgets/shared/hideable.ts`):

| key | meaning | widgets |
|---|---|---|
| `no-git` / `no-jj` | not in a repo | every git/jj widget |
| `no-remote`, `no-upstream` | remote or upstream missing | origin/upstream widgets, ahead/behind |
| `not-fork` | repo is not a fork | is-fork, upstream owner |
| `no-data` | source unavailable | usage, speed, PR/MR, CI checks, block timer |
| `zero` | count is zero | tokens, file counts, conflicts, changes, ahead/behind, cost `$0.00`, timer under 1 min |
| `empty` | nothing observed yet | cache activity, skills |
| `disabled` | feature off | extra usage |
| `default-value` | value equals the default | output style |
| `merge-target-hidden` | glued decoration hides with its target | custom text / symbol |

Storage: `metadata.hide = "a,b,c"`; absent means "widget defaults"; the
editor writes the key only when the enabled set differs from the defaults,
so untouched configs stay minimal. The v3→v4 migration folded older
per-widget booleans (`hideNoGit`, …) into this list.

garnish shape: `ModuleSchema.hide_states: &[HideState { key, doc, default }]`
per module; config `hide = ["no_upstream", "zero"]` (snake_case, validated
by `config check`, listed by `garnish docs`); `hide_when_empty` stays as the
alias for a module's `empty` state; `text.<name>.hide_with = "<module>"` is
the `merge-target-hidden` equivalent.

## 16. Per-module option vocabulary (from the editor keybinds)

Every option ccstatusline exposes per widget, mapped to a garnish key. Most
exist already; the rest are the A-tier keys in Part I.

| their key | option | garnish |
|---|---|---|
| `p` progress toggle | bar `none → slider (+%) → slider-only`, or long/short bar (32/16 cells) | `bar = "blocks" \| "line"`, `width` (have) |
| `v` invert fill | show remaining instead of used | `show = "remaining"` (have on bars) |
| `u` used/remaining | same for usage widgets | same |
| `s` short time | compact durations | `durations` (have) |
| `t` timestamp | absolute reset time instead of countdown | `reset = "countdown" \| "absolute"` (A10) |
| `z`/`l`/`h`/`w` | timezone, locale, 12/24 h, weekday for absolute times | `[time] tz, hour12, weekday` shared by `clock` and the usage modules (jiff) |
| `t` time cursor | `│` at elapsed position on the slider | `cursor = true` (A10) |
| `f` format / `.` precision | number style | `[format]` (A6) |
| `n` nerd font | per-widget glyph set | `icons` (have, per set) |
| `g` glyph, symbol override | replace the widget's symbol | `icons.<slot>` (have) |
| `l` limit | show the limit value next to used | `show_limit = true` on `spend`/extra usage |
| `h` hours only | block timer shows hours | `durations` handles |
| `h` history toggle | 48 h incident strip | C3 |
| `t` turn/session | cache scope | garnish `cache` is payload-scoped; n/a |
| `t` ttl | show the cache TTL badge | have |
| `w` window | speed rolling window seconds | C1 |
| `w` width | bar width | `width` (have) |
| `s` segments, `f` fish style, `h` home `~` | path display | A7 |
| `l` link to repo / IDE, `u` url | OSC 8 links | A8 |
| `o` owner only when fork | upstream owner display | B2 (`show = ["owner"]`) |
| `v` view last/count/list, `t` tokens reclaimed, `s` split by trigger | compaction counter views | C1 |
| `z` zero conflicts display | show `0` or hide | `hide = ["zero"]` (A4) |
| `e` edit text / edit cmd, `t` timeout, `p` preserve colors | custom text / command | `text.<name>` (have); command: out |
| `r` raw value, `m` merge, `x` exclude align, `h` hide… | item-level | `label = ""`, `glue`, `align_stop`, `hide` |

## 17. Output and lifecycle tricks

- **Row prefix `\x1b[0m`** (A1) — the harness wraps rows in `dimColor`.
- **NBSP** — every space becomes U+00A0 before printing so VS Code's
  terminal keeps trailing padding; verify before adopting (§ 5.4).
- **One-shot notices.** `settings.updatemessage = { message, remaining }`
  prints a line under the status line for `remaining` renders (decrementing
  each time), used after installs. garnish equivalent: a `<cache>/notice`
  file `{text, ticks_left}` written by `install`, `config migrate` or a
  version change, printed as a dim extra row and decremented per tick.
- **Config-error badge.** On an unparseable settings file the line renders
  from in-memory defaults with a red `⚠ invalid config` prefix, and the file
  is never overwritten. garnish already has the `⚠ garnish:` row; add the
  "never overwrite" sentence to `SPEC.md` § 5.
- **Needs-based work.** `renderMultipleLines` computes transcript analysis,
  usage prefetch and status prefetch only when a configured widget needs
  them. garnish's worker model already has this shape; C1–C3 workers must
  spawn only when their module is configured on some line.

## 18. Data sources: worker and cache designs

Everything here runs off the tick in garnish. The subsections record the
exact commands, file formats and fallbacks ccstatusline uses, then the
worker shape for garnish.

### 18.1 Local git (`src/utils/git.ts`)

Commands, one per widget, each cached separately: `status --porcelain -z`
(flags for staged / unstaged / untracked, and conflicts when a line starts
with `DD|AU|UD|UA|DU|AA|UU`), `diff --shortstat` (insertions/deletions),
`rev-list --left-right --count HEAD...@{upstream}`, `ls-files --unmerged`
(conflict count), `rev-parse --short HEAD`, `symbolic-ref --short HEAD`
(null when detached), `stash list`, `remote get-url origin`, `rev-parse
--git-dir` (worktree detection: `.git/worktrees/<name>`). Cache: in-memory
key `command|cwd`; on disk one file per repository
`~/.cache/ccstatusline/git-cache/git-<sha256(gitdir)>.json` keyed by
command, entries `{output | null, createdAt, headMtimeMs, indexMtimeMs}`;
fresh iff both mtimes are unchanged **and** (`ttl == 0` or age ≤ ttl, ttl
0–60 s, default 5); failures cached as `null`; written via one stable
`.tmp` path and `rename`. `GIT_OPTIONAL_LOCKS=0` in the environment, not
the flag, for old git.

garnish shape: the `branch` worker already runs `status --porcelain=v2
--branch`, which yields branch, upstream, ahead/behind, and every file
flag in one call; add `stash list` to the same run and store
`head_mtime`/`index_mtime` in the entry so the existing validator treats a
commit or a `git add` between ticks as a miss. Expose the extra data as
options (`show = ["sha", "dirty", "counts", "stash", "conflicts"]`), not
modules. Display vocabulary worth copying: `+staged *unstaged ?untracked
!conflicts` as single glyphs with counts.

### 18.2 PR and CI (`src/utils/git-review-cache.ts`, `git-remote.ts`)

- Cache file `git-review-<sha256(cwd ‖ "\0" ‖ ref)[..16]>.json` where `ref`
  is `branch:<name>` or `head:<short sha>`; TTL 30 s; entry records whether
  checks were queried so enabling the CI widget forces one refresh.
- Miss or stale → spawn self detached with
  `--internal-refresh-git-review-cache <cwd> <metadata|checks> <lockPath>`
  after taking a lock next to the cache file (stale after 30 s); the tick
  returns the stale data meanwhile. The refresh mode reads no stdin and
  prints nothing, and only unlinks the lock path it can derive itself (so
  the hidden flag is not an arbitrary-delete primitive).
- Provider: origin URL → ssh alias resolved with `ssh -G <host>` → github.com
  ⇒ `gh`, gitlab.com ⇒ `glab`, anything else ⇒ probe `gh auth status
  --hostname` and `glab auth status --hostname`, keep the authed ones; CLI
  timeout 5 s shared across the attempts.
- Fetch: `gh pr view --json url,number,title,state,reviewDecision[,statusCheckRollup]`;
  if the checks field errors (token lacks scope), retry without it; if the
  CLI resolves nothing, retry `gh pr view <branch> --repo <origin ref>`
  (forks). `glab mr view --output json` with state mapping.
- Label: MERGED, CLOSED, APPROVED, CHANGES_REQ, OPEN. CI rollup: CheckRun
  rows use `status`+`conclusion`, StatusContext rows use `state`;
  NEUTRAL/SKIPPED ignored; glyphs `✓ ✗ ●`, `-` when no checks.

garnish shape: `pr` already has number/url/review state from the payload,
so only checks need a worker. Option B3a reads shuck's daemon state if it
persists one; B3b is a `refresh --module pr` worker running `gh pr view
--json statusCheckRollup` with the lock/TTL machinery (60 s), the
"checks unavailable → metadata only" fallback, and `head` in the validator.

### 18.3 Usage API (`src/utils/usage-fetch.ts`, `usage-prefetch.ts`, `usage-windows.ts`)

Flow of `fetchUsageData`: in-memory cache (180 s; error entries 30 s) →
disk cache `~/.cache/ccstatusline/usage.json` (180 s by mtime, only if the
stored `tokenHash` matches the current token's fingerprint and the fields
the configured widgets need are present) → no token ⇒ `no-credentials`
error → active lock (`usage.lock` JSON `{blockedUntil, error}`, capped at
24 h) ⇒ serve stale → write lock `now + 30 s` → HTTPS GET
`https://api.anthropic.com/api/oauth/usage` with `Authorization: Bearer
<token>` and `anthropic-beta: oauth-2025-04-20`, via `HTTPS_PROXY` if set,
5 s timeout → 429 ⇒ lock `now + Retry-After` (default 300 s) → parse →
write cache with token hash. Token: `~/.claude/.credentials.json`
`.claudeAiOauth.accessToken`, or on macOS the keychain item "Claude
Code-credentials" (newest of several candidates by modification date).
Response: `five_hour`/`seven_day` `{utilization, resets_at}` (a null bucket
means 0 % on Enterprise, issue #343), `limits[]` of `{kind: session |
weekly_all | weekly_scoped, utilization, resets_at, scope.model.display_name}`
(newer accounts, issue #503), `extra_usage {is_enabled, monthly_limit,
used_credits, utilization, currency}`. Per-model registry
(`WEEKLY_MODEL_USAGE_BUCKETS`): Sonnet ↔ `seven_day_sonnet`, Opus ↔
`seven_day_opus`, Fable ↔ `weekly_scoped` only. Stdin `rate_limits` wins
over the API for the fields it carries; only missing fields trigger a
fetch (`usage-prefetch.ts` computes the requirement set from the configured
widgets).

garnish shape, if § 9 approves network: `refresh --module usage` worker
(`reqwest::blocking`, 5 s timeout, proxy from env), cache `<cache>/usage.json`
plus the existing lock file (with the 24 h horizon), stale-serve on any
failure, token fingerprint in the entry. Modules: `limit7d` gains `model =
"all" | "sonnet" | "opus" | "<display name>"` (server-supplied names, so no
hard-coded model list) and a new `extra` module for extra-usage credits
with `show_limit`. Everything reads the cache on the tick.

### 18.4 Transcript (`src/utils/jsonl-*.ts`, `compaction.ts`)

Row shapes that matter: `{type: "user" | "assistant" | "system", subtype?,
isSidechain?, isApiErrorMessage?, timestamp, message: {usage:
{input_tokens, output_tokens, cache_read_input_tokens,
cache_creation_input_tokens}, stop_reason?, content}}`; compaction rows are
`type: "system", subtype: "compact_boundary", compactMetadata: {trigger:
"auto" | "manual", preTokens, postTokens}`; subagent transcripts live in
`subagents/agent-<id>.jsonl` beside the main file and are read only when
speed metrics include subagents. Derivations: token totals (sidechain and
API-error rows excluded from "main chain"; when `stop_reason` exists only
rows with a stop reason count, to avoid double-counting streamed
partials); context length = usage of the newest main-chain row after the
last boundary, else `postTokens`, else 0; compaction stats = count, split
by trigger, Σ max(0, pre − post) reclaimed; speed = per assistant row an
interval from the previous user row's timestamp, intervals merged, tokens ÷
merged duration, optionally over a rolling window of 0–120 s; session
duration = first to last timestamp when the payload lacks
`total_duration_ms`; session name from title rows; thinking effort from
`<local-command-stdout>` rows printed by `/model` and `/effort` (garnish
has `effort.level` in the payload; skip). Reading: a streaming line
iterator, a reverse iterator for tail lookups, and for the cache timer a
32 KB tail read that grows backwards until a full row is found.

garnish shape (C1): a `refresh --module compactions` worker that stores
`{offset, size, inode}` in its cache entry and reads only bytes appended
since the last run, so a long session costs O(new rows) per refresh; TTL
2–5 s (the harness debounces at 300 ms). `speed` keeps a ring of the last
N `(user_ts, assistant_ts, output_tokens)` triples in the same entry. The
tick reads the cache. Render `compactions` as `⟲ 3` with `(2 auto, 1
manual)` and `↓ 240k reclaimed` in the full preset.

### 18.5 Hooks and skills (`src/utils/hooks.ts`, `hook-handler.ts`, `skills.ts`)

Hook stdin: `{session_id, hook_event_name, tool_name, tool_input: {skill},
prompt}`. `PreToolUse` with matcher `Skill` → `tool_input.skill`;
`UserPromptSubmit` → `/^\/([a-zA-Z0-9_:-]+)/` on the prompt. Appends
`{timestamp, session_id, skill, source}` to
`~/.cache/ccstatusline/skills/skills-<session>.jsonl`; metrics are total
invocations, unique skills, last skill. Settings entries are written as
`{_tag: "ccstatusline-managed", matcher, hooks: [{type: "command",
command: "<statusLine command> --hook"}]}`, re-synced on every save
(needed set derived from configured widgets), stripped on uninstall, and
legacy untagged entries matching the command pattern are removed too.

garnish shape (B4): `garnish hook` (hidden) appends to
`<cache>/<session>/skills.jsonl`; `install --hooks` writes tagged entries
(`_tag: "garnish-managed"`), `install --no-hooks`/uninstall removes them;
a `skill` module with `show = "last" | "count" | "list"` and `hide =
["empty"]`; `gc` sweeps the log with the session dir.

### 18.6 Settings-derived state (`src/utils/claude-settings.ts`)

Layer order project `.claude/settings.local.json` → project
`.claude/settings.json` → user `settings.local.json` → user `settings.json`
(user dir honours `CLAUDE_CONFIG_DIR`); first layer that defines the key
wins; "no file exists at all" → `null` (hide), "files exist but no key" →
`false`. Keys: `sandbox.enabled`, `voice.enabled`. Remote control: scan
`<config>/sessions/*.json` for the file whose `sessionId` equals the
payload's `session_id`; enabled iff `bridgeSessionId` is a non-empty
string. Account: `~/.claude.json` (or `<CLAUDE_CONFIG_DIR>/.claude.json`)
→ `oauthAccount.emailAddress`.

garnish shape (A9): one cached settings read (30 s, already exists for the
autocompact override) feeding `sandbox`, `voice`, `remote`, `account`;
`remote` matches on `session_id`, which garnish has in the payload.

### 18.7 Timers recorded for completeness (Tier D)

- Cache timer: newest main-chain row is a `user` row ⇒ HOT (🔥, Claude is
  working); else countdown `ttl − 5 s − (now − last assistant row with
  cache activity)`, TTL 5 m or 1 h; glyphs 🟢 > 50 %, 🟡 > 20 %, 🔴, ❄️
  COLD. garnish's `cache` reads `prompt_cache.expires_at`; only the glyph
  ladder is worth borrowing as `cache.glyphs = true` mapped to ok/warn/danger.
- Block timer: glob `~/.claude/projects/**/*.jsonl`, newest mtime first,
  progressive lookback 10 → 20 → 48 h, collect every row timestamp, walk
  from the newest until a gap ≥ 5 h, floor the block start to the hour,
  cache until the window ends, cache an empty result for 1 min. garnish:
  `resets_at` arithmetic (A10).

### 18.8 Service status (`src/utils/claude-service-status.ts`) — Tier C

`GET status.claude.com/api/v2/status.json` → `status.indicator` (none /
minor / major / critical / maintenance); `incidents.json` only when a
widget enables history; 5 min cache, 30 s failure backoff, stale served on
failure; history = 8 buckets × 6 h, each colored by the worst overlapping
incident impact, drawn with `▮`.

## 19. Config lifecycle: schema versions, recovery, import/export

- `version: 4` in the settings file; migrations v1→v2 (add version),
  v2→v3, v3→v4 (per-widget `hideNoGit`-style booleans → `metadata.hide`);
  `detectVersion` treats a missing field as v1.
- Load: parse JSON → if unversioned, validate against the v1 schema then
  migrate → else migrate if older → validate current schema → **persist the
  migrated file only if validation passed** → run. Any failure: log, set
  `lastLoadError`, run on in-memory defaults, render the red badge, and
  never write the file. The TUI asks for confirmation before `Save` would
  replace an invalid file.
- Writes are atomic through the symlink target: resolve the link, write
  `<name>.<pid>.<ts>.tmp` in the target's directory, `rename`.
- Import: refuses a newer `version`, migrates an older one, validates;
  preview lists the keys that would change; `replace` keeps `installation`,
  `merge` overlays only the keys present in the import; `installation`,
  `version`, `updatemessage`, `exportedBy` are never imported.

garnish shape: TOML plus per-key fallback already survives unknown keys, so
no version field is needed until a key is *renamed*; when that day comes,
`garnish config migrate` (rename in place, `.bak` first, refuse on parse
error) is the whole feature. Add to `SPEC.md` § 5: "a config that fails to
parse is never rewritten by any command". `config export` is `cat`; a
`config import <file> [--merge]` that validates then writes is cheap and
gives the TUI its import screen.

## 20. Installer and TUI mechanics (detail for § 7)

- **Install** (`installStatusLine`): backup `settings.json.orig` (first
  install) and `.bak` (every save); write `statusLine = {type: "command",
  command, padding: 0}`; keep an existing `refreshInterval`, else set 10
  when the detected Claude Code version supports it (`claude --version`,
  compared semver-wise); save installation metadata `{method: auto-update |
  pinned | self-managed | unknown, packageManager, installedVersion}`; sync
  hooks. `isInstalled` = the command is one of the known forms and
  `padding` is 0 or absent. `classifyInstallation` recognises the install
  style from the command string. Uninstall removes `statusLine`, the
  metadata and the managed hooks.
- **Screens** (24): main, lines, items, colors, colorLines, powerline,
  terminalConfig, terminalWidth, globalOverrides, configureStatusLine,
  refreshInterval, exportConfig, importConfig, importPreview, install,
  manageInstallation, uninstallOptions, updates, confirm, save, exit,
  flowNotice, starGithub, valid.
- **Items editor keys**: ↑↓ select, Enter toggles move mode, `a`dd / `i`nsert
  via the picker, `k` clone, `d`elete, `c`lear line, `r`aw value, `m`erge,
  e`x`clude from align, `.` precision, `h`ide…, plus the widget's own keys;
  the footer lists only keys that apply to the highlighted item; each row
  shows modifiers in dim text (`(compact)`, `(raw value)`, `(merged→)`).
- **Widget picker**: two-level fuzzy search, category then widget, with
  initialism matching (`gab` → GitAheadBehind).
- **Color menu keys**: `f` toggles foreground/background editing, `h`ex
  input (truecolor only), `a`nsi256 input (256 mode only), `g`radient
  picker (presets plus custom start/end hex), `b`old, `d`im cycles off →
  whole → parens, `r`eset item, `c`lear all (confirm), `s`how separators.
  Footer warns VS Code users that "Terminal › Integrated: Minimum Contrast
  Ratio" alters colors.
- **Powerline setup**: `t`oggle mode (warns it removes manual separators),
  `a`lign, `c`ontinue theme, `i`nstall fonts after a "what will happen"
  list (clone URL, install script steps, `fc-cache`, requirements, restart
  terminal); separator editor accepts 4–6 hex digits for a code point;
  theme selector with `c`ustomize (copies theme colors to widgets after a
  confirm) and a 16-color warning.
- **Preview pane**: bordered box titled "Preview (ctrl+s to save
  configuration at any time)", one truncated row per line, a flag when any
  line would be cut.
- **Terminal options**: width mode (three options with long descriptions
  of the autocompact wrap problem), color level with sanitisation.
- **Configure Status Line**: refresh interval (empty removes the key), git
  cache TTL 0–60 s. **Export/Import**: path prompts; preview; Replace All /
  Merge / Cancel. **Manage installation**: shows method, checks the npm
  registry, uninstall "settings only" or "package too". **Update check**
  messages differ for auto-update (nothing to do) vs pinned (run this
  command).

## 21. Their test suite, and what garnish should copy

About 170 `bun test` files: renderer tests per feature (`renderer-ansi`,
`renderer-dim`, `renderer-flex-width`, `renderer-merge-target-hidden`,
`renderer-padding-side`, `renderer-powerline-theme`,
`renderer-separator-collapse`, `renderer-config-warning`), one test per
widget plus shared-behaviour suites (`GitWidgetSharedBehavior`,
`JjWidgetSharedBehavior`) that run the same assertions over every widget
of a family, TUI component tests through ink-testing-library, migration
tests per version step, and a schema/registry parity test that fails when
a usage field is added to one table but not the other.

garnish already has goldens and config goldens. Worth adding: a
**module-matrix test generated from `ModuleSchema`** that renders every
module × every hide state × `max_width` × icon set and asserts the
invariants (never wider than `max_width`, hidden states render nothing,
OSC 8 wrappers balanced), so a new module gets the shared behaviour for
free. Their hand-written shared suites are the same idea done manually.

## 22. Payload comparison

Their stdin schema (`src/types/StatusJSON.ts`, a loose object) declares
`session_id`, `transcript_path`, `cwd`, `model` (string or object),
`workspace.{current_dir, project_dir}`, `version`, `output_style`, `effort`,
`cost`, `context_window` (with `current_usage`), `vim`, `worktree`,
`rate_limits`. It does **not** model `session_name`, `prompt_cache`,
`fast_mode`, `thinking`, `agent`, `pr`, `exceeds_200k_tokens`,
`workspace.added_dirs / git_worktree / repo`. garnish's `SPEC.md` § 2.2 is
the more complete map; ccstatusline recovers several of those from the
transcript or from git instead. Their model-context table assumes 200 k
with `[1m]` suffix inference for 1 M models and an 80 % "usable" ratio;
garnish's `effective_window − 13 000` autocompact threshold was verified
against the binary and is the better number for A11.

---

# Part III — A companion character (ccsidekick, codachi)

Daniel likes the idea of a sidekick or tamagotchi-style pet in the status
line. Two MIT projects do it well in different ways; this part records how
each is built and then designs a garnish companion that keeps garnish's
invariants (no child process on a warm tick, payload first, clock-driven
animation, a fixed module set) and improves on both.

## 23. The two projects

### 23.1 ccsidekick (krayong, MIT, v1.8.0, Bun/TypeScript workspace)

A status line *with a character*: a single sourced ASCII figure (≤ 9 rows ×
25 columns) sits in a left gutter beside a five-row field block; a
one-line in-character comment renders under the block and an optional
"helpful" tip above it. 18 character packs ship as pure data
(`pack.json`: figure rows, attribution, tone, an optional theme, ~620
voice lines in tiered pools, ≥ 25 spinner verbs), 33 widgets, 58 built-in
themes plus one per pack. Numbers that matter:

- **Two binaries.** `ccsidekick-render render` (the tick) and
  `ccsidekick-render classify` (the hook) load no UI; `ccsidekick` is the
  Ink TUI plus `setup`/`list`/`uninstall`. Same split garnish has between
  `render` and its subcommands.
- **Hooks.** `PostToolUse` and `PostToolUseFailure` (matcher = the union of
  tool names) run `classify`, which maps the tool name or Bash command to
  one of 31 outcome categories (`test_pass/fail`, `build_pass/fail`,
  `typecheck_pass/fail`, `lint`, `format`, `install`, eight `git_*`,
  `force_push`, `dangerous`, `file_edit`, `file_read`, `search`,
  `web_fetch`, `todo_update`, `agent_spawn`, `skill_run`, `docker`, `k8s`,
  `deploy`, `db_migrate`, `server_start`) plus a stack tag from the program
  (`cargo → rust`, `pytest → python`, `kubectl → kubernetes`, …). A
  successful tool flips to a failure when `tool_response` has `isError` or
  `interrupted`, or stdout+stderr matches a tight failure regex
  (`\bFAILED\b|\bFAIL\b|✗|✕|\bnot ok\b|error:|error\[|error TS\d|error CS\d|
  \bpanic:|Traceback \(most recent call last\)|[1-9]\d* (?:failed|failing|errors?)\b`);
  a non-empty stderr alone is never a failure. Events append to
  `sessions/<id>/events.jsonl`, bounded to 200 lines; the hook exits 0
  and prints nothing. `PostToolBatch` is deliberately not wired (it co-fires
  and would double count).
- **Mood.** From events inside a 5-minute window: none → `idle`; no
  pass/fail among them → `busy`; latest signal a pass with an earlier fail →
  `recovery`; ≥ 3 fails and latest not a pass → `struggling`; latest a pass →
  `happy`. Three *pressure* moods override the figure, first match wins:
  `compact_hint` (context near autocompact), `block_limit`, `weekly_limit`
  (quota > 80 %). Mood never changes a glyph, only color (a static tint
  over a diagonal shimmer gradient that drifts top-left → bottom-right on a
  24 s wall-clock period).
- **Quota by pace, not by percentage.** `r = used_fraction /
  max(elapsed_fraction, 0.01)`; `r ≤ 1` nominal, `≤ 1.5` caution, else
  critical; below 20 % used the pace is ignored, above 80 % always
  critical. Context bands are fixed at < 34 / 34–66 / ≥ 67 %.
- **Voice selection** (`compose/character.ts`) walks a fixed chain and the
  first slot with content wins: first contact → hot event reaction →
  pressure or non-idle mood (tier-nested pools) → milestone (tier up,
  comeback after ≥ 3 days, streak 3/7/30/100, anniversary) → date egg →
  egg (every 12th tick) → greeting by time bucket (morning/day/evening/
  night/weekend) → idle. The pick is `sha1(seed parts) mod pool`, so the
  same situation shows the same line instead of flickering. Lines are
  capped at 66 columns. Pressure and milestone lines fire once per session
  (latches in `state.json`, merged under a lock so concurrent ticks never
  drop one).
- **Familiarity.** Cross-session analytics (`analytics/store.json`, one
  record per session joined from the transcript cost cache) give a tier —
  stranger / acquaintance / friend / partner / legend at 3 / 15 / 50 / 100
  sessions — streaks with a one-day grace, days since last session, and
  "seen this project". Random mode picks the least-recently-used character
  by that store, tie-broken by a hash of the session id over the sorted
  candidate set.
- **Helpful tips** (`compose/helpful/catalog.ts`, ~45 triggers, core-owned,
  never pack-authored): safety (untracked secret file, destructive command
  just ran, kube context or terraform workspace looks like prod, commit on
  detached HEAD, force push), billing (API key while subscribed, PAYG near
  cap, low balance), quota (block/weekly almost spent, will exhaust at this
  pace), context (compact urgent, commit before compact, compaction thrash
  ≥ 3, cache inefficiency < 50 % after 20 turns, compact soon ≥ 60 %), git
  (merge conflict, upstream gone, pushed to default, commit on default, big
  diff > 1000 lines, diverged, behind upstream, dirty default branch,
  unpushed, no upstream, > 20 untracked, ≥ 5 stashes, > 20 commits behind
  default, rebase/merge/cherry-pick/revert in progress, uninitialised
  submodule, detached HEAD), workflow (todo in progress > 30 min, effort
  low). Each has a severity, a "momentary" flag, a 5-minute show window
  and a 10-minute cooldown; the user sets a severity floor. Kube and
  terraform context are read from files, never by spawning the tools.
- **Spinner verbs.** Install writes `spinnerVerbs: {mode: "replace",
  verbs: [...]}` into Claude Code's settings so the harness's loading text
  speaks in character. A real hook into the harness that no other project
  uses.
- **Cost engine.** Prices every transcript in the tree in-house with a
  bundled `pricing.json` (per-model input/output/cache-write 5 m and 1 h/
  cache-read per million, `fast_mult`), deduplicated globally by
  `(message.id, requestId)` keeping the write with the largest
  `output_tokens`, cached per file by `{mtime, size, byteOffset,
  headHash}` so a growing transcript re-prices only its appended tail and a
  compaction rewrite (head hash mismatch) forces a full reparse. Burn rate
  over the live 5-hour window. Also reconstructs the current **todo list**
  from `TaskCreate`/`TaskUpdate`/`TodoWrite` rows in the transcript.
- **Git.** One `rev-parse` answers three location probes; `status
  --porcelain=v2 --branch`; `diff HEAD --numstat`; `describe --tags
  --exact-match`; in-progress operation from the existence of
  `.git/rebase-merge|rebase-apply|MERGE_HEAD|CHERRY_PICK_HEAD|REVERT_HEAD`;
  submodule branches via `submodule status --recursive`; `GIT_DIR`-style
  location env vars stripped so a hook environment cannot redirect the
  reads. Git runs fresh every tick (their tick is ~100 ms+; garnish's
  cannot).
- **Layout.** Figure gutter (25 cols + 2 gap) only when `columns ≥ 80`;
  below that a `[name] │` chip leads the line. Every externally sourced
  string (cwd, branch, session name, pack text) is stripped of C0/C1 and
  ESC sequences before painting; the model field keeps a protected tail
  (`(1M)`, effort) and ellipsizes the name first.
- **Setup.** `npx ccsidekick` opens a wizard (character → theme →
  comments) on first run and a dashboard later (sections: Character, Theme,
  Statusline widgets, Comments, Stats, Install; keys `w a s d`/arrows,
  `tab`, `1–7`, `/` find, `?` help, `ctrl+p` preview, `ctrl+s` save &
  install, `ctrl+w`/`ctrl+d` switch views). `ccsidekick setup --character
  … --theme … --widgets …` is the non-interactive twin that validates every
  value against the live registry and fails loudly on a typo; `ccsidekick
  list characters|themes|widgets` prints the valid sets. The repo is its
  own Claude plugin marketplace with a `/ccsidekick-setup` slash command
  whose instructions tell Claude to run `list` first, map plain-English
  intent onto flags, and never pass a value that did not match — a
  template for garnish's Phase 18 skill. Config is TOML with
  `schema_version = 1` and a per-project `.ccsidekick/config.toml`
  override; settings writes are verify-then-rollback (write, re-read,
  parse, restore the old text on failure) with the oldest and newest
  backups retained.
- **Stats.** A dashboard section with sessions, uptime, tier progress,
  a 60-day activity heatmap, a sparkline, per-model bars, weekday/weekend
  split, cost against budget.

### 23.2 codachi (vincent-k2026, MIT, v0.3.0, zero-dependency TypeScript)

"A productivity copilot disguised as a tamagotchi." Three rows: widgets
(model, context bar with `555K/1.0M`, burn velocity `^3%/m` and an ETA
`~15m` to a full context, 5 h and 7 d limits with a pace delta `⇡5%`/`⇣2%`
and a reset countdown), git (`git:(main*) ~12 ?3 | +489 -84 lines | last:
<commit subject>`), and the pet row (`Mochi *slow blink* ...I love you`).

- **Frames are procedural.** Five species (cat, penguin, owl, octopus,
  bunny) × five body sizes (tiny/small/medium/chubby/thicc, chosen by
  context-usage bucket, so *the pet grows as the context fills*) × five
  moods (idle, happy, busy, danger, sleep) × four frames. A species is ~40
  lines: a template takes an eye glyph and a tail glyph (`o`/`^`/`-`/`O`
  eyes; `~` wag, `!` alarm, `z`/`Z` sleep) and the width is derived, so
  every frame of an animation is padded to the same width and the figure
  never jitters. Frame index is `floor(now / 1.5 s) mod 4`.
- **Events.** A `PostToolExecution` hook appends to `events.json` (50
  entries, optimistic generation counter). ~40 categories including
  file-type edits (`edit_test/docs/style/config/code` from the path),
  `creating_file`, `rapid_editing` (5 edits in 60 s), `recovered`,
  `struggling` (3 failures), `first_action`, `many_edits`, `web_research`,
  `agent_spawned`. Freshness tiers: hot < 15 s, warm < 60 s, cold < 5 min.
- **Mood engine** is a 15-tier priority list: tier-upgrade celebration →
  danger (context > 85 %) → smart `/compact` suggestion (context > 70 % and
  fast burn, once per trigger) → hot event → high usage → warm event → busy
  → welcome back (first ticks) → session stats → cold event → rare egg →
  velocity → time of day → file type → git mood → body-size line → species
  idle. 900+ messages, file-aware (names the file being edited), with
  Simplified Chinese bundled and user locale overrides.
- **Memory.** `memory.json` with a schema version: first met, sessions,
  uptime; tiers stranger / acquaintance / friend / bestie at 0 / 3 / 15 /
  50; a one-time upgrade celebration. Identity (species, palette) is a hash
  of `transcript_path`, so a session keeps its pet.
- **Context ETA.** A 20-entry ring of `(pct, t)` samples (≥ 1 s apart)
  gives velocity in %/min over the last ~30 s; ETA = remaining / velocity
  when velocity > 0.3 %/min. This needs state written on the tick.
- **Pace delta.** `used% − elapsed%` of the window, from `resets_at` and
  the window length (300 or 10 080 min); positive is red (over pace),
  negative green. Payload-only.
- **Plugins.** `~/.config/codachi/plugins/*.mjs` export message packs and
  RGB palettes (executed code, which garnish will not do). `codachi stats`
  prints the relationship dashboard; `init` / `uninstall` / `config` (TUI)
  / `demo` (live preview loop) round out the CLI.

### 23.3 What each got right

| aspect | ccsidekick | codachi | take for garnish |
|---|---|---|---|
| figure | sourced art, static, color-only mood | procedural, animated, grows with context | codachi's procedural frames through garnish's clock-driven animation; ccsidekick's static-art path as an optional pack |
| events | 31 outcome categories, two hooks, soft-fail regex | 40 categories incl. file types and rapid editing | ccsidekick's vocabulary plus codachi's file-type and rapid-edit detectors |
| mood | 5 moods + 3 pressure moods, 5-min window | 5 moods incl. sleep, 15-tier message priority | ccsidekick's mood rule; codachi's `sleep`; ccsidekick's slot chain for messages |
| memory | 5 tiers, streaks, comeback, anniversary, LRU rotation | 4 tiers, one-time celebration | ccsidekick's |
| quota | pace band `r` | pace delta `⇡5%` | both: delta on the module, band for the color |
| extras | tips catalog, spinner verbs, cost engine, todo | context ETA, `stats`, i18n | tips, spinner verbs, ETA, `stats` |
| setup | wizard + dashboard, flags twin, plugin slash command | one-line `init`, config TUI, `demo` | flags twin and the skill contract; `demo` as `preview --live` |
| data | packs are pure data, lint-enforced | messages in code, plugins are code | pure data packs (TOML), lint in tests |

## 24. garnish companion design

Everything below is payload-plus-cache on the tick; the only new process is
the hook, which runs in Claude Code's hook slot, not on the tick.

### 24.1 Modules

- **`pet`** — the figure. `species = "cat" | "penguin" | "owl" | "octopus"
  | "bunny" | "sprig"` (a garnish-original default, so the bundled set has
  no trademark exposure), `size = "auto" | "tiny" | … | "thicc"` (`auto`
  follows the context bucket), `rows = 1 | 3` (a one-row pet `(o w o)~`
  fits today's four-line layouts; three rows use the gutter in § 24.5),
  `name = "Mochi"`, `animate = true`, `mood_colors = true`.
- **`say`** — the one-line voice, usually alone on its own line or right
  after `pet`; `max_width` (default 66), `tone` filter if a pack declares
  tones, `hide = ["idle"]` to speak only when something happened.
- **`tip`** — the helpful line, `min_severity = "medium"`, `show_for = 300`,
  `cooldown = 600`, `hide_when_empty` default true.
- **`provider`** — badge for bedrock / vertex / foundry / proxy / api from
  the environment (`CLAUDE_CODE_USE_BEDROCK` etc., `ANTHROPIC_BASE_URL`),
  hidden for a subscription; small, payload/env only, and ccsidekick users
  asked for it.

### 24.2 Events: the hook

`garnish hook` (shared with B4) is registered by `install --hooks` for
`PostToolUse`, `PostToolUseFailure`, `PreToolUse` (matcher `Skill`) and
`UserPromptSubmit`, tagged `_tag: "garnish-managed"`. It reads the hook
JSON from stdin, classifies in-process, appends one line to
`<cache>/<session>/events.jsonl` (bounded 200), rewrites a small
`<cache>/<session>/events.json` summary (`freshest`, `consecutive_failures`,
`edits_60s`, counts by category, last skill), and exits 0 without output.
The tick reads the summary only.

```rust
/// Written to events.jsonl by `garnish hook`; the tick never parses Bash text.
struct Event { ts: i64, kind: EventKind, stack: Option<Stack>, detail: Option<String> }

enum EventKind { TestPass, TestFail, BuildPass, BuildFail, TypecheckPass, TypecheckFail,
    Lint, Format, Install, GitCommit, GitPush, GitPull, GitMerge, GitRebase, GitBranch,
    GitTag, GitStash, ForcePush, Dangerous, FileEdit(FileClass), FileCreate, FileRead,
    Search, WebFetch, TodoUpdate, AgentSpawn, SkillRun, Docker, K8s, Deploy, DbMigrate,
    ServerStart }
enum FileClass { Test, Docs, Style, Config, Code }
```

Classification is table-driven (tool-name map, then Bash rules
most-specific first, wrappers `npx bunx sudo time env command` skipped) with
the pass/fail suffix taken from the hook event plus the soft-fail check.
The crate map bans `regex`, so the failure markers are a list of literal
substrings and two tiny hand-written matchers (`[1-9]\d* failed`, `error
TS\d`); `tests/hook.rs` pins them against real tool outputs (cargo, pytest,
go test, jest, tsc). Rapid editing = ≥ 5 `FileEdit` in 60 s, computed by the
hook into the summary.

### 24.3 Mood and pressure

```rust
enum Mood { Idle, Sleep, Busy, Happy, Struggling, Recovery }
enum Pressure { CompactHint, BlockLimit, WeeklyLimit }

fn mood(summary: &Summary, now: i64) -> Mood            // ccsidekick's rule, § 23.1
fn pressure(payload: &Payload, cfg: &Config) -> Option<Pressure>
```

Pressure uses garnish's exact autocompact threshold (`context ≥ window −
compact_buffer_tokens − margin` → `CompactHint`), then `limit5h`/`limit7d`
by the pace band (§ 23.1) with the 80 % floor. `Sleep` = idle and context <
10 % (codachi). Freshness (`hot < 15 s`, `warm < 60 s`, `cold < 5 min`)
weights the message choice, not the mood.

### 24.4 Frames and animation

codachi's model, expressed once:

```rust
struct Species { name: &'static str, build: fn(size: Size, eye: char, tail: char) -> Frame }
const FRAMES: [(Mood, [(char, char); 4]); 6] = [ /* eye, tail per frame */ ];
```

Frame index comes from `SPEC.md` § 4.2 (`floor(now × step) mod 4`, `step`
from `[modules.pet].fps`, default 2/3 for codachi's 1.5 s), so the pet
needs no state and freezes under `GARNISH_ANIMATE=0` for goldens. All four
frames of an animation are padded to one width (their `pickFrame`), and the
one-row variant is the face row alone. Mood colors: `happy` accent, `busy`
text, `struggling` warn, `recovery` ok, pressure danger; with A3 gradients
a `sprig` in the `full` preset can carry ccsidekick's shimmer as a gradient
whose phase is the clock. ASCII species render in every icon set; `emoji`
icon set may swap the face for a single glyph in one-row mode.

Optional static art: `~/.config/garnish/packs/<name>/pack.toml` with
`[figure] rows = [...]` (≤ 9 × 25), `[attribution]`, `tone`, and the line
pools of § 24.6. Pure data, validated by `config check`, never executed;
mood is color-only for static art (ccsidekick's rule), which keeps a
sourced figure from strobing.

### 24.5 Gutter layout (three-row pet)

A new top-level key, kept out of `[[line]]` so existing layouts are
untouched:

```toml
[gutter]
module = "pet"        # the only gutter-capable module for now
side = "left"
rows = 3              # lines 1..3 get the gutter; extra lines render flush
gap = 2
min_width = 80        # below this the gutter is dropped and `pet` renders one-row inline
```

`compose_line` reserves `figure_width + gap` on the gutter lines and the
frame's fill still reaches the right edge, so `align` and the right group
behave as before. Below `min_width` the figure is dropped and, if `pet` is
also on a line, its one-row form shows there (ccsidekick's chip fallback,
without a second element).

### 24.6 Voice

Pools per slot, tier-nested where ccsidekick nests them: `first_contact`,
`greeting.{morning,day,evening,night,weekend}`, `mood.<mood>`,
`event.<reaction>` (the 18-cell reaction set: three fail kinds, lint,
format, install, git, file_edit, search, web_fetch, todo_update,
agent_spawn, skill_run, docker, k8s, deploy, db_migrate, dangerous),
`pressure.<kind>`, `milestone.{tier_up,comeback,streak,anniversary}`,
`positive_git.{clean_tree,op_cleared,branch_created,tag_pushed}`, `egg`,
`date_egg`, `stack.<stack>.{slow,fail}`, `file.<class>` (codachi's
file-type lines). Selection is ccsidekick's chain (§ 23.1) with codachi's
freshness gating on the event slot. Deterministic pick: `fnv1a(seed) mod
len` where the seed is `(slot, tier, mood, bucket, session_id, 10-second
tick)`; a ten-line FNV-1a in `num.rs` avoids a hash crate. Templates may
name the file or branch (`{file}`, `{branch}`), sanitised first.

The bundled voice lives in `packs/sprig.toml` and is loaded with
`include_str!`; a unit test lints it the way `pack:lint` does (pool counts,
≤ 66 columns, no near-duplicates by token-set Jaccard ≥ 0.8, no control
characters). Further packs are user data under the config dir.

### 24.7 Memory, tiers, stats

`<cache>/companion/memory.json` (`schema_version`, `first_met`, `sessions`,
`uptime_s`, `last_seen`, `projects_seen`, `streak_days`, `last_day`) is
updated when a tick sees a new `session_id` (the tick writes one small file
once per session; workers own every other write). Tiers at 3 / 15 / 50 /
100 sessions; milestone and pressure lines latch once per session in
`<cache>/<session>/state.json`, merged as sets so overlapping ticks never
drop a latch. `garnish stats` prints the dashboard: first met, sessions,
uptime, tier progress bar, streak, last-24 h events from `events.jsonl`
(tests pass/fail, commits, edits), this session's duration. No network, no
telemetry.

### 24.8 Tips

Port ccsidekick's catalog as a `&[Tip]` table: `id`, `severity`,
`momentary`, `test: fn(&Derived) -> Option<String>`. Inputs garnish already
has: payload (context, limits, effort, `pr`), the `branch` worker (dirty
flags, upstream, ahead/behind, untracked paths, in-progress operation
from `.git/*` file existence, stash count, tag), events (dangerous, force
push, compaction count if C1 lands), settings (API key vs subscription from
`rate_limits` presence). Secret detection matches untracked names against
`.env*`, `*.pem`, `*.key`, `id_rsa*`, `*credentials*`, `*.p12`. Kube and
terraform prod contexts come from `~/.kube/config` `current-context` and
`.terraform/environment` files, read on the settings cache cadence. Show
window and cooldown are latched in `state.json`.

### 24.9 Spinner verbs

`install --spinner-verbs` writes `spinnerVerbs: {mode: "replace", verbs:
[...]}` from the active pack (≥ 25 verbs) into Claude settings, preserving
key order; `install --no-spinner-verbs`/uninstall removes it. **Verify**
the key name and shape against the 2.1.261 binary before building; if it
is not there, drop the idea (ccsidekick may target a newer harness).

### 24.10 Small ports from these two (Tier A unless noted)

- `limit5h`/`limit7d`: `pace = true` shows `⇡5%`/`⇣2%` (used − elapsed);
  `pace_colors = true` colors by the pace band instead of raw percentage.
- `branch`: in-progress operation glyph (`rebase 2/7`, `merge`,
  `cherry-pick`, `revert`) from `.git/*` existence, no process; `tag` via
  `describe --tags --exact-match` in the worker; submodule branches
  optional.
- `context`: `eta = true` prints `~15m` from a velocity ring — needs a
  tick-side sample file (`<cache>/<session>/ctx.json`, ≤ 20 rows); a
  decision, because today only workers write on the tick path.
- `todo` module (in-progress task name) — C1, transcript.
- `cost`: `burn = true` (`$/h` over the 5-hour window) — C1, transcript.
- `provider` module — Tier A.
- Sanitisation invariant: every externally sourced string (cwd, branch,
  session name, agent name, pack text) is stripped of C0/C1 and ESC before
  painting; add a golden with a hostile branch name.
- `preview --live` (codachi's `demo`): loop a fixture through the renderer
  on a TTY with the clock running, for screenshots and for checking
  animation without Claude Code.
- Settings write contract from ccsidekick: write temp, rename, re-read,
  parse, restore the previous text on failure; keep the oldest and newest
  backups only.

### 24.11 Decisions for Part III

- [ ] Ship a companion at all, and which of `pet` / `say` / `tip` /
      `provider` (each is a module; the hook is shared with B4).
- [ ] Bundled species: garnish-original `sprig` plus codachi-style animals
      (procedural ASCII, no trademark) — yes/no on each.
- [ ] Allow user packs as pure data under the config dir (`pack.toml`)?
- [ ] Tick-side writes for memory (once per session) and the context-ETA
      ring (every tick, tiny) — accept, or restrict ETA to a worker?
- [ ] Gutter layout (`[gutter]`) versus one-row pets only.
- [ ] Spinner verbs, after the binary check.
- [ ] `garnish stats` as a new subcommand.

---

# Part IV — A visual editor (ccstatusline-editor)

## 25. What it is

[refinist/ccstatusline-editor](https://github.com/refinist/ccstatusline-editor)
(MIT, Vue 3 + Vite + Pinia + shadcn-vue, deployed as a Cloudflare Worker;
clone at v2.2.26-ccse.1) is a browser editor for ccstatusline configs:

- **Editor page**: a widget palette (drag and drop, `vue-draggable-plus`),
  a line editor for up to five lines, an inspector for the selected widget
  (colors, bold, dim, raw value, per-widget options) and global settings
  (padding, separator, powerline, color level), a JSON panel, undo history
  (editor-only preferences such as "auto separator" are kept out of the
  history and the export), and a **true-to-terminal preview**: the app
  re-implements each widget's `isPreview` render branch of ccstatusline
  v2.2.26 (`src/preview/previewText.ts`, "faithful port"), reproduces the
  renderer's separator-collapse and inherit-separator-color rules
  (`renderers.ts`), down-samples colors to the chosen terminal level, and
  draws powerline arrows as SVG so no Nerd Font is needed.
- **Templates page**: ready-made configs applied in one click; a template
  share link is `?tpl=<id>` and needs no backend.
- **Share**: `POST /api/share` stores a user config in Workers KV and
  returns `?s=<id>`; the receiver's editor loads it on open. Rate-limited
  and "unavailable" are distinct user-facing failures.
- **Apply**: a one-line command,
  `npx -y @refinist/ccsa@latest '<json>'` (single-quoted JSON, `'\''`
  escaping), where the companion CLI backs up the current settings to a
  timestamped copy under `~/.config/ccsa/` and writes the file; `ccsa
  export` pulls the live config back into the editor for round trips.
- **Export image**: `html-to-image` renders the preview card at a fixed
  terminal width and 2× scale so every screenshot is identical.
- **Rotation page**: build a pool of themes, pick a period (hourly / daily
  / weekly / custom) and a strategy (cycle / shuffle), or the "one look per
  weekday" preset; export a bundle `{version: 1, period, strategy, themes:
  [...], preset?}` that the CLI runs with `rotate on`; the CLI picks
  `slotIndex(date, period) mod themeCount`, and because epoch day 0 was a
  Thursday the editor rotates the Sunday-first card order into epoch order
  on export.
- i18n (en, zh-CN, zh-TW); tests colocated, heaviest under `src/preview/`.

## 26. What garnish can take from it

garnish has no web surface and does not need one to get most of the value.

1. **Config as the sharing unit.** The editor's whole loop is "a config file
   you can hand to someone, preview without installing, and apply with one
   command". garnish's TOML already is that file. Tier A pieces:
   - `garnish config share` prints the config as a single line
     (`garnish config apply '<toml>'` on the other end, validating before
     writing and backing up like `install` does). No server: the payload is
     the TOML itself, base64 if it must survive a chat client.
   - `garnish preview --config <file|->` renders a foreign config against
     the bundled fixtures at a chosen width, so a shared config can be seen
     before it is applied. `preview` already has `--width`; it needs
     `--config` (or `GARNISH_CONFIG=-`).
   - `garnish preview --png`/`--svg`: **not** a crate-free job; instead
     `preview --html` emitting a self-contained HTML page (spans with
     inline colors, monospace, fixed width) that a browser or
     `html-to-image` can capture. Zero dependencies, and it doubles as the
     README gallery generator (memory note: gallery samples must fit
     GitHub's width).
2. **Templates = the presets gallery** (`SPEC.md` § 12). The editor's
   "apply a template then tweak" is `garnish config init --preset <gallery
   name>` followed by editing; a setup TUI (§ 7) should open on the
   gallery, not on an empty line, and show each preset rendered.
3. **Rotation.** A daily/weekly theme rotation is a stateless function of
   the clock, which garnish already has: `theme = ["nord", "dracula",
   "catppuccin-mocha"]` with `[rotation] period = "day" | "hour" | "week"`,
   `strategy = "cycle" | "shuffle"`, picked as `floor(now / period) mod n`
   (shuffle = FNV of the slot). One weekday-pinned preset: `[rotation]
   weekdays = { mon = "nord", tue = "dracula", … }`. No CLI toggle, no
   bundle file, no schedule registration: the config re-reads every tick.
   Same for `preset` and `icons` if wanted. Tier A, small.
4. **Undo / history in the TUI** (if 7.3c): keep editor preferences out of
   the saved config, keep `.bak` as the undo of last resort, and write the
   live file on every change so Claude Code shows it immediately (§ 7.2).
5. **A web preview later, if ever.** The honest version for garnish is a
   static page that runs the *real* renderer compiled to WebAssembly
   (`wasm32-unknown-unknown`, the render path has no I/O once payload and
   config are strings), fed by a TOML textarea and a fixture picker. It
   would never drift from the terminal output the way a hand-ported
   preview can. Cost: a `wasm-bindgen` build target and a static site;
   value: a gallery and share links (`?toml=<base64>`) with no backend.
   Tier C (new build target); decision for Daniel, not a recommendation.

### 26.1 Decisions for Part IV

- [ ] `config share` / `config apply` / `preview --config` (Tier A).
- [ ] `preview --html` for the gallery and screenshots (Tier A).
- [ ] Theme/preset rotation keys (Tier A).
- [ ] A WebAssembly preview page (Tier C).

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

### ccsidekick (`packages/core/src/`, v1.8.0)

| topic | files |
|---|---|
| invariants and architecture | repository `CLAUDE.md`, `README.md` |
| classifier, failure regex, hook | `derived/classifier.ts`, `cli/classify.ts`, `cli/settings.ts` (hook matcher, `spinnerVerbs`, verify-then-rollback write) |
| mood, pressure, pace band | `derived/mood.ts`, `derived/signals.ts`, `derived/quota.ts`, `domain/constants.ts` |
| voice selection, pack schema, lint | `compose/character.ts`, `domain/pack.ts`, `packs/lint.ts`, `packages/packs/<name>/pack.json` |
| figure and layout | `render/figure.ts`, `render/layout.ts`, `render/strip.ts`, `render/theme.ts`, `data/themes.ts` |
| tips | `compose/helpful/catalog.ts`, `sources/helpfulEnv.ts`, `sources/markers.ts` |
| cost engine and transcript scan | `derived/cost.ts`, `derived/pricing.ts`, `data/pricing.json`, `sources/transcript.ts`, `sources/costCache.ts` |
| git | `sources/git.ts` |
| state, events, analytics | `sources/state.ts`, `sources/events.ts`, `sources/storage/*`, `derived/analytics.ts`, `derived/persona.ts` |
| setup CLI, TUI, plugin command | `cli/setup.ts`, `tui/shell/Wizard.tsx`, `tui/shell/Dashboard.tsx`, `tui/nav/keymap.data.ts`, `commands/ccsidekick-setup.md`, `.claude-plugin/plugin.json` |

### codachi (`src/`, v0.3.0)

| topic | files |
|---|---|
| species and frames | `animals/types.ts`, `animals/cat.ts` (and the four siblings) |
| mood priority, messages | `mood.ts`, `messages/{events,idle,context,social,git}.ts` |
| events and hook | `events.ts`, `hook.ts` |
| state, memory, velocity, ETA | `state.ts` |
| pace delta, widgets, compositor | `stdin.ts` (`computePaceDelta`), `widgets/*.ts`, `render/index.ts` |
| plugins, i18n, stats | `plugins.ts`, `i18n.ts`, `stats.ts` |

### ccstatusline-editor (`src/`, v2.2.26-ccse.1)

| topic | files |
|---|---|
| faithful preview port | `preview/previewText.ts`, `preview/renderers.ts`, `preview/powerline.ts`, `preview/colors.ts` |
| config store, undo, editor preferences | `stores/config.ts` |
| share links, apply command, image export | `lib/shareConfig.ts`, `lib/applyCommand.ts`, `lib/exportImage.ts` |
| rotation bundles and the weekday preset | `lib/rotationBundle.ts`, `lib/weeklyPreset.ts`, `stores/rotation.ts`, `ccsa-rotation-rainbow-week.json` |
| templates, widget options | `templates/index.ts`, `widgets/options.ts` |
