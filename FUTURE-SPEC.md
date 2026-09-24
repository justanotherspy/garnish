# FUTURE-SPEC.md — the target design after Phase 18, with its evidence

**Status: proposals, not decisions.** Nothing in this file is part of the
target design until it is moved into `SPEC.md` (with the reason) and given a
phase in `PLAN.md`. It is the single document to review for everything
garnish could become after the Phase 12–18 stack: what a mature competitor
ships, what garnish already does better, which ideas are worth the cost of
porting, what the Claude Code contract actually is at the current version,
and how garnish and [garlic](https://github.com/justanotherspy/garlic) fit
together. Every claim carries an evidence grade; the appendices hold the
tables, the version history and the sources.

This revision (2026-09-06) folds three earlier documents into one: the
ccstatusline / ccsidekick / codachi / ccstatusline-editor mining (the first
`FUTURE-SPEC.md`), its online validation (`FUTURE-SPEC-RESEARCH.md`, now
deleted), and the garlic integration proposal from pull request #40
(`GARLIC-INTEGRATION.md`, superseded by Part IV). Appendix G maps the old
section numbers to the new ones so references in earlier PR bodies still
resolve.

## How to read this

- **§ 0** is the decision checklist: every choice Daniel has to make, each
  pointing at the section that argues it. Read it first and last.
- **Part I** (§ 1–4) is the ground: method, market, the seven invariants
  garnish must not give up, and the verified Claude Code contract at
  2.1.261/2.1.263, including facts nothing in the codebase records yet.
- **Part II** (§ 5–15) is the target design grouped by concern (rows and
  layout, color and formats, payload and settings modules, workers, hooks,
  subagent rows, config lifecycle and sharing, setup). Each proposal keeps
  its tier (**A** fits every current invariant, **B** needs a worker or
  cache adaptation, **C** needs a `SPEC.md` non-goal lifted or a new crate
  and is a decision for Daniel, **D** is deliberately not ported), its tick
  class (payload-only, cached file read, cached worker, network worker),
  a TOML sketch, the Rust shape, and an evidence line. Proposal ids (A1…,
  B1…, C1…, N1…) are unchanged from the earlier documents.
- **Part III** (§ 16–17) designs the companion character.
- **Part IV** (§ 18–23) covers garlic: how it works, what the harness can
  say about attention, the review of PR #40, the integration design with
  garnish as heartbeat and facet source, and a list of improvements to
  garlic itself.
- **Part V** (§ 24–25) states the spec impact and the phase order.
- **Appendices** A–H: verdict table, changelog versions, OKLab, gradient
  and theme tables, crate facts, repository file maps, the old-to-new
  section map, and sources.

## Sources

Four repositories were read in full from shallow clones (MIT, not kept);
thirty-one more were surveyed by README and API metadata; the official
documentation was fetched as Markdown and grepped locally; two Claude Code
binaries were dumped with `strings`; garlic was read from its working copy.

| source | what | version or date |
|---|---|---|
| [sirmalloc/ccstatusline](https://github.com/sirmalloc/ccstatusline) | the mature status line, Node/TypeScript with a React/Ink TUI | commit `016be1fcf19453bd4362439b197e9cf841d7006a`, v2.2.29 |
| [krayong/ccsidekick](https://github.com/krayong/ccsidekick) | a status line with a reacting ASCII character, tips, cost engine | v1.8.0 |
| [vincent-k2026/codachi](https://github.com/vincent-k2026/codachi) | a tamagotchi pet in the status line | v0.3.0 |
| [refinist/ccstatusline-editor](https://github.com/refinist/ccstatusline-editor) | a browser editor for ccstatusline configs with share links | v2.2.26-ccse.1 |
| [justanotherspy/garlic](https://github.com/justanotherspy/garlic) | daily coding-time tracker and nudger on Claude Code hooks (Rust, `garlic-ward` on crates.io) | v0.3.5, working copy `~/repos/garlic` read 2026-09-06 |
| 31 further status line, HUD and pet projects | README and GitHub API metadata | 2026-09-06, table in § 2.2 |
| `code.claude.com/docs/en/*.md` | official documentation (pages listed in Appendix H) | fetched 2026-09-06; the changelog page lists 2.1.263 |
| Claude Code binary | `strings` dumps | 2.1.261 (2026-09-05) and 2.1.263 (2026-09-06) |
| `anthropics/claude-code` issues | state and labels of the issues named in Appendix H | 2026-09-06 |
| pull request #40 on this repository | `GARLIC-INTEGRATION.md`, reviewed in § 20 | 2026-09-06 |

File paths in Part II are relative to the ccstatusline clone unless a
repository is named; Part III names ccsidekick and codachi paths; Part IV
cites garlic as `src/<file>.rs:<line>` at v0.3.5.

## Evidence grades

| grade | meaning |
|---|---|
| **D** | official docs at `code.claude.com/docs`, fetched 2026-09-06 (the pages describe ≥ 2.1.261; version notes quoted where a page gives them) |
| **B** | strings of the Claude Code binary on this machine; the version is named where it matters (2.1.261 or 2.1.263) |
| **M** | a maintainer or collaborator comment on `anthropics/claude-code` |
| **C** | a community source: an issue comment by a non-member, a project README |
| **L** | a local check on this machine (file *keys* and process state only, never values) |
| **P** | a primary technical source outside Claude Code (git manual, Ottosson's OKLab post, crates.io, a live HTTP probe) |
| **S** | garlic source at v0.3.5, cited by file and line |

A claim with no grade is design reasoning, not a fact.

---

## 0. Decision checklist

Each item is a decision only Daniel can make: a `SPEC.md` non-goal lifted, a
new dependency, a new file class, or a behaviour the spec leaves open.
"Verify" items are work, not decisions, and are listed in § 4.9.

**Cross-cutting**

- [ ] **Network.** Lift "no network calls" for opt-in, worker-only fetches
      (C2 usage API, C3 service status, B3b `gh`)? If yes, `reqwest` joins
      the crate map. If no, C2/C3/C5 are closed and B3 is B3a only. C2's
      case is weaker than first written and has two non-network
      alternatives (§ 9.4). → § 9.3–9.5, § 24.1
- [ ] **Transcript.** Allow a bounded tail read of `transcript_path` in a
      worker? Only `speed` and the exact reclaimed-tokens figure still need
      it; the compaction count and trigger come from a hook now. → § 9.6,
      § 10.3
- [ ] **Hooks.** May `garnish install --hooks` write tagged hooks into Claude
      settings (`skill`, `compactions`, the companion events, the garlic
      liveness hook), and should garnish also ship as a plugin whose
      `hooks/hooks.json` installs them without editing user settings? → § 10
- [ ] **Module count.** Accept growing the fixed set beyond 21 for A9, A12,
      B4, N1's renderer, `compactions`, `today`, `provider`, the companion
      modules and `garlic`? → § 8, § 10, § 17, § 21
- [ ] **Tick-side writes.** Today only workers write. Three proposals want
      the tick to write a small file: companion memory (once per session),
      the context-ETA ring, and a per-day cost ledger. Part IV shows the
      garlic heartbeat does *not* need one. Accept none, some, or all?
      → § 8.6, § 17.7, § 21.3
- [ ] **Installer form.** 7.3a skill only, 7.3b prompt wizard (`inquire`),
      or 7.3c `ratatui` TUI behind a feature flag? → § 13
- [ ] **Segments.** Is the Powerline-segment look (B1) wanted enough to
      justify a background role set in `theme.rs` and a second painter
      path? → § 6.4

**Companion (Part III)**

- [ ] Ship a companion at all, and which of `pet` / `say` / `tip` /
      `provider`. → § 17.1
- [ ] Bundled species: garnish-original `sprig` plus codachi-style animals
      (procedural ASCII, no trademark), yes/no on each. → § 17.4
- [ ] User packs as pure data under the config dir (`pack.toml`)? → § 17.4
- [ ] Gutter layout (`[gutter]`) versus one-row pets only. → § 17.5
- [ ] Spinner verbs and tips exported into Claude settings. → § 17.9
- [ ] `garnish stats` as a new subcommand, reading the harness's own
      `stats-cache.json` aggregates. → § 17.7

**Sharing and rotation (§ 12)**

- [ ] `config share` / `config apply` / `preview --config` (Tier A).
- [ ] `preview --html` for the gallery and screenshots (Tier A).
- [ ] Theme and preset rotation keys (Tier A).
- [ ] A WebAssembly preview page (Tier C).

**garlic (Part IV)**

- [ ] **G1. The `garlic` module** (one more id in the fixed set): a cached
      file read of garlic's `state.toml` and `config.toml` that projects
      the open cursors to now, so the status line shows a live figure
      garlic itself cannot show today. Alternative: a worker running
      `garlic status --json` once per 30 s (PR #40's 6.3), which is simpler
      but shows a number that only moves when a cursor closes. → § 21.2
- [ ] **G2. Heartbeat through the worker machinery**, not the tick: a
      `garnish refresh --module garlic` worker writes the per-session
      heartbeat and facet samples every 30 s, which keeps `SPEC.md` § 6
      intact. PR #40 asked for a tick-side write; § 21.3 argues it is not
      needed. Accept the worker form, and should the heartbeat run only
      when the `garlic` module is displayed or also behind a
      `[garlic] heartbeat = true` switch?
- [ ] **G3. A `garnish hook` entry for `Notification` (matcher
      `idle_prompt`)** as the one presence signal the harness exposes to
      hooks, written into the same session event log. → § 19 item 3, § 21.4
- [ ] **G4. File the upstream ask** for a `presence` object in the status
      line payload (`terminal_focus`, `last_interaction_ms`). The harness
      already tracks both. → § 19 item 1
- [ ] **G5. Which garlic changes to make** from the list in § 22: the two
      persistence fixes (single-lock read-modify-write, temp-and-rename)
      are recommended regardless of garnish; the rest are ordered.
- [ ] **G6. Ownership of the tracking.** Keep garlic as the engine and
      relay (recommended) versus moving tracking into garnish (PR #40's
      6.2). → § 21.1

---

# Part I — Foundations

## 1. Method

The four mined repositories were read in full: for ccstatusline that means
`README.md`, `docs/USAGE.md`, `docs/DEVELOPMENT.md`, `docs/WINDOWS.md`,
`AGENTS.md`, `package.json`, `configTemplates/*.json`, the entry point
`src/ccstatusline.ts`, every file under `src/types/`, `src/utils/` and
`src/widgets/`, the TUI under `src/tui/`, and the CI and publish workflows.
Each feature was traced to the code that implements it, so the notes say
how a thing is built, not only that it exists. Repository shape at the
commit: ~90 widget types in `WIDGET_MANIFEST`
(`src/utils/widget-manifest.ts`), one class per widget under `src/widgets/`,
a single 1 392-line renderer (`src/utils/renderer.ts`), Zod-validated
settings with four schema versions and migrations (`src/utils/config.ts`,
`src/utils/migrations.ts`), and a TUI of ~30 Ink components.

The validation pass then checked every claim that could be checked against
the official documentation (fetched as Markdown so the long pages are not
truncated), the binary strings, the issue tracker and the wider ecosystem,
and graded each one. Where a source could not be found, the claim is marked
as design reasoning. garlic was read from its source with the same
discipline (Part IV cites lines), and three independent audits of it, one
verification of PR #40 and two refutation passes over the research claims
were run as separate agents whose findings are folded in where they bear.

Nothing in the sources was taken on trust: the refresh timer, the payload
builder, the focus tracking and the presence rule were located in the
2.1.263 dump and are quoted in § 4; the width rule was re-derived; the
issue evidence for the usage endpoint is stated as closure state and labels,
not as a maintainer position, because no maintainer commented.

## 2. Market context

### 2.1 The four mined projects

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
(`@refinist/ccsa`), and interoperability with `ccusage`. Its README
explicitly answers "why is it slow" with a startup table: `bunx
ccstatusline@latest` 633 ms, a pinned global install 202 ms, `npx` about
1.1 s per repaint. That number is the single biggest differentiator garnish
has (§ 3).

What its users asked for, judging by the changelog and issue-driven widgets:
rate-limit windows beyond the two the harness sends (per-model weekly limits,
extra-usage credits), a service-status indicator, "how fast is it
generating", "how many times has it compacted", PR review and CI state, a
skills indicator, Powerline visuals, and a way to stop VS Code's terminal
from trimming the line. Several of those are already in the garnish payload
for free; the rest are weighed in Part II.

### 2.2 The wider field

Star counts, language and last push from the GitHub API on 2026-09-06 [C].
"garnish ahead" lists what garnish already does that the project does not,
by its README.

| project | ★ | lang | pushed | license | what it does that garnish does not | garnish ahead |
|---|---|---|---|---|---|---|
| jarrodwatts/claude-hud | 27,837 | JS plugin | 2026-09-05 | MIT | tools/agents/todos lines from the transcript; `elementOrder`, `mergeGroups`, `rightAlign`; usage time formats; external usage snapshot; per-day cost ledger; provider label; jj; `CLAUDE_HUD_DISABLE`; per-`CLAUDE_CONFIG_DIR` overlay config; zh locales | exact width, no transcript on the tick, schema-generated docs, goldens, presets, animation |
| ccusage/ccusage | 18,379 | Rust (was TS) | 2026-09-05 | NOASSERTION | `statusline` subcommand: session/today/block cost, `$/hr` burn rate with color bands, 5-hour blocks, `--cost-source auto/cc/ccusage/both`, offline LiteLLM pricing, model label aliases; 18 agent CLIs | payload-first, no transcript scan, sub-3 ms |
| sirmalloc/ccstatusline | 12,782 | TS | 2026-09-03 | MIT | (mined here) | — |
| Haleclipse/CCometixLine | 3,455 | Rust | 2026-03-14 | MIT (README; API reports none) | `ratatui` TUI with live preview; `models.toml` for display names and context limits; `--patch` of `cli.js` (Tier D); npm-distributed native binary | no binary patching, payload context data, tests |
| Piebald-AI/tweakcc | 2,483 | TS | 2026-09-04 | MIT | patches Claude Code itself: themes, thinking verbs, spinner styles, statusline update pacing | not comparable; Tier D reference |
| Owloops/claude-powerline | 1,164 | TS plugin | 2026-08-31 | MIT | `/powerline` wizard; Powerline Studio web configurator; `tui` boxed style with grid `areas`/`columns`/breakpoints/culling; budgets with warning thresholds; dot-notation subsegments | width exactness, no `npx` start-up, goldens |
| GaoSSR/best-claude-hud | 680 | Rust | 2026-08-13 | Apache-2.0 | `--setup` writes `statusLine` with an absolute path and a timestamped backup; interactive menu when run in a TTY; Nix flake; deps `ratatui`, `crossterm`, `ureq`, `sysinfo`, `tree-sitter` | leaner crate set, cache/worker model |
| daniel3303/ClaudeCodeStatusLine | 600 | Shell | 2026-05-26 | MIT | — (rate limits, tokens, git) | everything structural |
| Nanako0129/coralline | 539 | Bash + PowerShell | 2026-08-23 | MIT | p10k pills; **AI-interview install** via a fetched `INSTALL.md` playbook; themed subagent rows (`VL_SUB_SEGMENTS`); `burn` time-to-limit; cross-session limit sync store; float readout file; one `git status --porcelain=v2 --branch` per render; `BENCHMARK.md` | typed config, docs generation, cache workers |
| rz1989s/claude-code-statusline | 477 | Shell | 2026-08-01 | MIT | 1–9 lines from 18 components; MCP server health; prayer times; XDG cache with SHA-256 checksums and lock backoff; 227-key TOML | speed, no network, tests |
| Ido-Levi/claude-code-tamagotchi | 435 | TS | 2025-10-20 | MIT | Groq LLM "thoughts" and a `PreToolUse` violation blocker (network + LLM + blocking) | no network, no blocking hooks |
| stephenleo/cship | 419 | Rust | 2026-08-04 | Apache-2.0 | Starship-compatible TOML with `$cship.<module>` tokens and **Starship passthrough**; `cship explain` debug view; per-module `warn_/critical_threshold`; ≤ 10 ms budget; `ureq` for usage limits via keychain/libsecret | no network, worker cache, goldens |
| Wangnov/claude-code-statusline-pro | 235 | Rust | 2026-08-17 | MIT | `ratatui` + `dialoguer` configurator; `git2` (libgit2 vendored) and `tokio` | lean deps, subprocess git with timeouts |
| kcchien/claude-code-statusline | 178 | Shell | 2026-03-24 | MIT | truecolor→256→ASCII 3-tier gradient bar; "smart hiding" of zero values; Bash 3.2 notes | structure, tests, docs |
| martinemde/starship-claude | 149 | Shell | 2026-04-30 | MIT | Starship as the renderer; `/starship` wizard skill; named in the official docs | native rendering, width |
| alvinunreal/openpets | 107 | TS | 2026-05-10 | MIT | desktop pet driven by Claude Code events (moved to OpenPets) | in-terminal |
| sorosora/arcade-statusline | 81 | Rust | 2026-04-21 | MIT | Pac-Man chase (context drives the character, limits drive ghosts, cherry at the autocompact point) and Pikmin flower trail per 15-minute slot; serde only | — (a game, not a status line) |
| terryso/ccpet | 63 | TS | 2025-08-30 | MIT | energy decays ~3 days 100→0, +1 per 1M tokens; web leaderboard | — |
| hagan/claudia-statusline | 36 | Rust | 2026-06-15 | NOASSERTION | **hook-based compaction detection** (`Compacting… ⠋` then `✓`); SQLite persistence; `$/h` burn; adaptive context-limit learning; 11 themes, 5 layout presets | no database, payload context |
| spences10/claude-statusline-powerline | 34 | TS | 2026-09-04 | MIT | SQLite usage db for session/usage segments; JSON schema IntelliSense | — |
| kumamaki/Claude-Code-Personalities | 31 | Rust | 2026-06-30 | WTFPL | 30+ kaomoji personalities by activity; `inquire` + `cliclack` + `ratatui` configurator; self-update | — |
| krayong/ccsidekick, vincent-k2026/codachi, refinist/ccstatusline-editor | 30 / 12 / 12 | TS | 2026-09-02 / 04-18 / 07-25 | MIT | (mined here) | — |
| TeXmeijin/claude-code-mascot-statusline | 24 | TS plugin | 2026-04-26 | MIT | half-block pixel sprites; 9 hook-driven states; fur color shifts with context; packs with validator and storybook; `claude-code-safe` render profile (NBSP for transparent cells); documents the 2.1.76 footer layout | width model, no `ps`/`stty` |
| micschr0/claudebar | 12 | Rust | 2026-09-05 | MIT | `claudebar config` full-screen `ratatui` configurator with live preview, theme/style pickers, threshold sliders | — |
| david-strejc/claude-powerline-rust | 13 | Rust | 2025-08-25 | MIT | `memmap2` + `simd-json` + `rayon` transcript scan (150 ms) | no scan at all |
| moon1ite/claude-statusline | 9 | Rust | 2026-03-25 | MIT | tools/agents/todos from transcript, serde only | — |
| ohugonnot/claude-code-statusline | 9 | Shell | 2026-06-10 | MIT | payload `rate_limits` first, OAuth endpoint fallback with `flock`-guarded shared cache; states the endpoint is undocumented | — |
| ndave92/claude-code-status-line | crate 1.2.10 | Rust | 2026-08-20 | MIT | repository returns 404; crates.io lists powerline arrows, quota timers, multiline | — |

Other names seen but not mined: levz0r (3 ★), GordonBeeming (2 ★),
ticpu/ccusage-statusline-rs (16 ★, `reqwest` + `inquire`), khoi/cc-statusline-rs
(42 ★), glauberlima (49 ★), three unrelated "tokengotchi" repos (0 ★),
NadimJebali/Claude-Familiar (4 ★, a desktop mascot).

### 2.3 Patterns across the market, stated as facts

- **Distribution.** The plugin marketplace plus a setup skill is the norm
  for the top projects (claude-hud's `/claude-hud:setup`, starship-claude's
  `/starship`, coralline's fetched `INSTALL.md` playbook); single-binary
  Rust projects distribute through npm wrappers with platform binaries
  (CCometixLine, best-claude-hud), Nix flakes, or `curl | bash` installers
  with `--yes` for CI. Claude Code's own `/statusline` command
  auto-configures a line from the shell prompt [D], so a skill-driven setup
  is the floor, not a differentiator.
- **Data.** Nobody but ccstatusline, ohugonnot and cship calls the usage
  endpoint; claude-hud and coralline explicitly refuse to ("never falls
  back to credential scraping or undocumented API calls") and solve
  idle-session staleness with local snapshot files instead [C].
- **Performance claims** cluster at "< 50 ms" (shell) and "≤ 10 ms"
  (cship); garnish's < 3 ms warm tick is the fastest stated budget found.
- **Nothing found** generates its docs from a schema, ships golden renders,
  or measures width exactly against the harness box.
- **Rust configurators exist**: claudebar, CCometixLine, best-claude-hud and
  claude-code-statusline-pro all ship `ratatui` setup screens with live
  preview, so option 7.3c in § 13 is a proven shape rather than a novelty.
- **Pets are a market**: eight pet projects were found; the most-starred
  pairs the pet with an LLM-driven "violation" blocker over Groq, which
  garnish must not copy (network, LLM, blocking hooks).

## 3. Where garnish is already ahead (do not regress)

These are the properties every proposal in Parts II–IV is checked against.

1. **Never blocks a tick.** ccstatusline runs `git` synchronously
   (`execFileSync` in `src/utils/git.ts`), scans transcript JSONL files, and
   performs HTTPS fetches on the render path, relying on caches to make it
   usually fast. garnish's tick reads the payload and cache files only, with
   a measured budget (< 3 ms warm) and a detached worker for anything slower
   (`SPEC.md` § 6, § 8). Every port states its tick class.
2. **Payload-first.** `prompt_cache`, `rate_limits`, `context_window`,
   `pr`, `worktree`, `effort`, `vim`, `agent` come from the harness.
   ccstatusline derives the cache timer, the context size after compaction,
   the thinking effort and the session duration from the transcript because
   its schema predates those payload fields, and each is a heuristic. The
   hooks documentation adds a reason to stay payload-first: the transcript
   is written asynchronously and may lag the in-memory conversation [D].
3. **Exact width.** garnish derives the box width from the harness's own
   layout (`COLUMNS − 4 − 2 × statusLine.padding`, re-verified in § 4.3).
   ccstatusline probes the terminal by walking parent PIDs to find a TTY
   and running `stty -F /dev/tty size` (`src/utils/terminal.ts`), then
   subtracts a guessed 6 or 40 cells (`resolveEffectiveTerminalWidth` in
   `src/utils/renderer.ts`) depending on a `flexMode` the user must choose.
   `COLUMNS`/`LINES` have been in the script's environment since 2.1.153 and
   the docs say `tput cols` cannot work in the piped child [D]; garnish's
   minimum supported version (2.1.251) is past that, so the `ps`+`stty`
   fallback other projects carry is irrelevant.
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

An eighth property matters for Part IV: **the tick never writes** (`SPEC.md`
§ 6; only refresh workers own the cache directory). PR #40 proposed one
exception; § 21.3 shows the existing worker machinery does the job without
it.

## 4. The Claude Code contract, verified

`SPEC.md` § 2 records the payload and `CLAUDE.md` records the facts the
codebase depends on. This section holds everything Parts II–IV rely on
that is *not* yet in those files, re-verified against the 2.1.263 binary on
2026-09-06 (the changelog entry for 2.1.263 says only "Bug fixes and
reliability improvements", and there is no 2.1.262 entry [D]). Minified
identifiers move between versions, so each recipe names a stable neighbour
rather than a variable name.

### 4.1 The payload builder

The status line payload is assembled by one function that emits the fields
`SPEC.md` § 2.2 lists, and in addition [B, 2.1.263]:

- `rate_limits` is built from exactly three windows, `five_hour`,
  `seven_day` and, for gateway sessions only, `spend_limit`. The richer
  usage schema in the binary (`seven_day_sonnet`, `seven_day_opus`,
  `model_scoped[]` with `display_name`/`utilization`/`resets_at`,
  `extra_usage` with `is_enabled`/`monthly_limit`/`used_credits`/
  `utilization`/`currency`, and the `weekly_scoped` limit kind) belongs to
  the usage dialog and the SDK session object, not to the status line
  payload. ccstatusline's stdin schema declares `seven_day_sonnet` and
  `seven_day_opus` optionally; nothing fills them today.
- `remote: { session_id }` is spread into the payload when the session is
  a remote one (`...jn()!==null&&{remote:{session_id:x.id}}`), which
  `SPEC.md` § 2.2 does not list yet. It is a note for the Phase 12–18 stack
  to pick up, not an edit made here.
- `pr.kind` distinguishes a GitHub PR from a GitLab MR (`"mr"`, since
  2.1.234 [D]); `worktree` carries the worktree object (2.1.69 [D]).
- Documented absence rules that matter for rendering [D]: `rate_limits`
  appears only for Pro/Max subscribers or behind a gateway with spend
  limits, and only after the first API response; `used_percentage` may be
  `null` early; `context_window.current_usage` is `null` right after
  `/compact`; `exceeds_200k_tokens` has been there since 1.0.88.
- Payload fields by the version that introduced them: Appendix B.

### 4.2 When the line runs

- **Timer.** `refreshInterval` is turned into a period of
  `Math.max(1, refreshInterval) * 1000` ms (a function that returns `null`
  when the key is absent) [B, 2.1.263]. The settings schema declares the
  key as `min(1)` with a `.catch(void 0)`, so a value the schema rejects
  (`0`, a string) is dropped to "unset", meaning no timer at all, rather
  than clamped; the clamp only covers values that passed validation. The
  timer runs "in addition to event-driven updates" (the schema's own
  description), re-arms unconditionally, and stops only when the status
  line component unmounts; no focus or idleness gate appears in it
  (§ 4.6 covers what the harness knows about focus).
- **Event triggers.** The controller that re-runs the command compares
  exactly eight values between renders: `tokenUsage`, `permissionMode`,
  `vimMode`, `mainLoopModel`, `fastMode`, `effortValue`,
  `thinkingEnabled`, `prStatus`; a change to any of them, or a new
  `lastAssistantMessageId`, schedules a debounced run; a change of the
  `command` itself runs immediately, bypassing the debounce; and the
  script runs once when the component mounts [B, 2.1.263]. The docs
  describe the same set in prose: session start and resume, each new
  assistant message, the end of `/compact`, a permission-mode change, a
  vim toggle, and a `command` change [D].
- **Scheduled re-renders.** A re-render is scheduled for one second after
  each `resets_at` in the last payload's `rate_limits` and after the
  prompt cache's `expires_at` [B, D], and a window is dropped from the
  payload once its `resets_at` is in the past [D]; so a countdown module
  is never stale at the boundary and `limit5h`/`limit7d` can vanish
  between two ticks.
- **Debounce and cancel.** 300 ms debounce; a new trigger cancels the
  in-flight script [D]. A status line script must therefore be safe to
  kill at any instruction, which is why garnish's tick never writes.
- **Gates.** The command runs only after workspace trust is accepted
  (`claude --debug` logs `Status line command skipped: workspace trust not
  accepted`) [D, B]; `disableAllHooks` outside managed settings disables
  it; `allowManagedHooksOnly` hides it silently; `CLAUDE_CODE_SAFE_MODE=1`
  skips a non-managed status line; `CLAUDE_CODE_SHELL_PREFIX` wraps the
  command; `CLAUDE_CODE_DEBUG_LOG_LEVEL=verbose` logs the full output [D].
  The line hides during autocomplete, help and permission prompts [D].
- **Environment.** `COLUMNS` and `LINES` are `process.stdout.columns`/`rows`
  (since 2.1.153 [D]); harness runs can be told from manual runs by
  `CLAUDE_CODE_CHILD_SESSION=1` / `CLAUDECODE=1` [D].

### 4.3 Width, rows, and what else sits on the row

- **The box.** The footer is a `flexWrap: "wrap"` box with `paddingX` of
  2 and `columnGap` of 1 (in 2.1.261 the constants were `var Vne=2,Gne=1`
  near `FooterHintLine`; in 2.1.263 the same pair is `Xne`/`Jne`, so the
  recipe is "find the `flexWrap:"wrap"` box whose `paddingX`/`columnGap`
  come from a `var X=2,Y=1` pair near `FooterHintLine`") [B]. Inside it
  the status line sits in a `<Box paddingX={statusLine.padding}>`, which
  gives `COLUMNS − 4 − 2 × padding` as `CLAUDE.md` records. The same
  function computes a second width, `COLUMNS − 4 − (stacked ? 0 : 1 +
  measured width of the right-hand block)`, but passes it as the
  `rowWidth` of the hint line (the mode/shortcut block), not to the
  status line element, which receives no width and sizes to the padded
  box [B, 2.1.263]; the flag that picks the stacked (`column`) layout
  was not resolved from the strings and does not touch the status line
  width. The `isNarrow` identifier that the mascot project's README
  documents for the 2.1.76 half-width layout does not occur in either
  dump, which is consistent with the current layout but is weak evidence
  on its own (names move).
- **Row rendering.** In 2.1.261 each status line row is rendered as
  `<Text dimColor wrap="truncate">` (found next to `statuslineIssueCount`
  in the dump) [B, 2.1.261]. The same neighbour in 2.1.263 no longer sits
  beside the row wrapper, and the pattern was not re-located in the newer
  dump; the fact is therefore dated 2.1.261 until A1 (§ 7.1) is verified
  on screen. The multi-line truncation bug behind A1's caution (#28750:
  `wrap: "truncate"` dropping rows, community root cause) was fixed in
  2.1.141 per the changelog [D, C].
- **What shares the row.** Outside fullscreen rendering, notifications
  share the status line row and verbose mode adds a token counter there
  [D]; the fullscreen renderer gives notifications their own row. A
  full-width line can therefore still be squeezed (verify item 1 in
  § 4.9). Since 2.1.176, `footerLinksRegexes` renders link badges
  *alongside* the status line [D], so garnish should not duplicate ids the
  user already badges (A8). The harness also shows its own PR / `MR !N`
  badge (2.1.234) and a `/rc active` indicator when Remote Control is on
  [D], which weakens the `remote` module (A9) to a duplicate at best.
- **Autocompact.** The docs say compaction runs at the model's context
  limit unless a window is set, and that with `autoCompactWindow`
  (setting), `--autocompact` (flag) or `CLAUDE_CODE_AUTO_COMPACT_WINDOW`
  (env, wins) the `used_percentage` "no longer indicates when compaction
  will run" [D]. `SPEC.md` § 2.3 records a 13 000-token buffer observed in
  the 2.1.260 binary. The two are not reconciled here; the buffer stays a
  verify item (§ 4.9) and `compact_buffer_tokens` stays configurable.
- **Hyperlinks.** OSC 8 works; `FORCE_HYPERLINK=1` overrides detection
  [D].

### 4.4 Hooks

The documentation fetched 2026-09-06 lists **33 hook events** [D]:
`SessionStart`, `Setup`, `InstructionsLoaded`, `UserPromptSubmit`,
`UserPromptExpansion`, `MessageDisplay`, `PreToolUse`, `PermissionRequest`,
`PostToolUse`, `PostToolUseFailure`, `PostToolBatch`, `PermissionDenied`,
`Notification`, `SubagentStart`, `SubagentStop`, `TaskCreated`,
`TaskCompleted`, `Stop`, `StopFailure`, `TeammateIdle`, `ConfigChange`,
`CwdChanged`, `DirectoryAdded`, `FileChanged`, `WorktreeCreate`,
`WorktreeRemove`, `PreCompact`, `PostCompact`, `PreModelSwitch`,
`PostModelSwitch`, `SessionEnd`, `Elicitation`, `ElicitationResult`. There
is no focus or presence event (§ 4.6). Facts Parts II–IV depend on:

- **Execution.** All matching hooks run in parallel; the same handler
  defined in more than one settings file runs once, but a plugin's or
  skill's copy stays separate [D]. Command hooks block by default;
  `async: true` runs one in the background without blocking and its
  `timeout` is then not enforced; `asyncRewake` wakes Claude on exit
  code 2 [D]. **An async hook's stdout is delivered into the conversation
  on the next turn**, so a hook that must be invisible exits 0 with empty
  stdout [D]. Default timeouts: 600 s for `command`/`http`/`mcp_tool`,
  lowered to 30 s on `UserPromptSubmit` and `PreModelSwitch` (that hook
  "blocks model processing until it completes"); `SessionEnd` hooks have a
  1.5 s budget, raised to the highest per-hook `timeout` up to 60 s, and
  this applies to exit, `/clear` and switching sessions via `/resume`
  [D].
- **Trust and settings.** Hooks merge across settings levels; plugin
  `hooks/hooks.json` is an official surface, and a plugin's `settings.json`
  may carry only `agent` and `subagentStatusLine`, explicitly not
  `statusLine` or `spinnerVerbs` [D]. `disableAllHooks` and
  `allowManagedHooksOnly` gate hooks as they gate the status line.
- **`SessionStart`** receives `source` ∈ `startup | resume | clear |
  compact | fork` (with `model` possibly absent after `/clear`), and on
  `resume`/`fork` four extra fields about the stale conversation; its
  matcher is the `source` [D]. A hook registered with matcher `startup`
  only, as garlic's is, does not fire for the other four (§ 22).
- **`SessionEnd`** receives `reason` ∈ `clear | resume | logout |
  prompt_input_exit | other` [D, B]; a killed terminal or a crashed
  harness produces no `SessionEnd`.
- **`UserPromptSubmit`** carries `prompt`, and in the binary an optional
  `source` ∈ `user | sdk | system | loop_wakeup | schedule_wakeup |
  poll_event` described as "who authored/injected the prompt" [B, 2.1.263;
  not on the docs page]. A prompt from a loop or schedule wakeup is not a
  human sitting down, which matters for time tracking (§ 22).
- **`Stop`** runs when the main agent finishes responding and *not* on a
  user interrupt [D]; `SubagentStop` is a separate event, and a `Stop`
  hook scoped inside a subagent definition is converted to `SubagentStop`
  [D].
- **`PostToolUse`** also fires inside subagents, with `agent_id` and
  `agent_type` [D]; `PreToolUse` with matcher `Skill` sees
  `tool_input.skill` [D]; `PostToolBatch` fires once per parallel batch
  [D, B].
- **`PostToolUseFailure`** carries `error` (for Bash it starts with `Exit
  code N`), `is_interrupt` and `duration_ms`; it does not fire for
  validation rejections or permission denials (and `PermissionDenied`
  fires only for auto-mode classifier denials, not for a manual deny, a
  `PreToolUse` block or a `deny` rule);
  `tool_response` is an object whose shape depends on the tool (Bash:
  `stdout`, `stderr`, `interrupted`, `isImage`) [D].
- **`PreCompact` / `PostCompact`** carry `trigger` ∈ `manual | auto` as
  the matcher; `PostCompact` input is the common fields plus `trigger` and
  `compact_summary`, and **no token counts** [D].
- **`Notification`** carries `message`, optional `title` and
  `notification_type`, which is the matcher [D, B]. The documented types
  are `permission_prompt` (after about six seconds of no typing),
  `idle_prompt` ("Claude finished responding about 60 seconds ago and you
  haven't typed since"), `auth_success`, `elicitation_dialog`,
  `elicitation_url_dialog`, `elicitation_complete`,
  `elicitation_response`, `agent_needs_input`, `agent_completed` and the
  `quota_auto_resume_*` family; the binary's type array adds
  `worker_permission_prompt`, `push_notification`, `computer_use_enter`
  and `computer_use_exit` [B, 2.1.263]. The docs note that the first four
  types "share their timing with desktop notifications, so in terminal
  sessions you only see them when you appear to be away", and that
  `idle_prompt` is not sent while the session waits for a usage limit to
  reset [D]. § 4.6 shows what "appear to be away" means in the code.
- **`StopFailure`** matchers include `rate_limit`, `overloaded`,
  `authentication_failed`, `billing_error` [D].
- **Hook stdin** always has `session_id`, `prompt_id`, `transcript_path`,
  `cwd`, `permission_mode`, `effort`, `hook_event_name` and the event's
  own fields, and **no timestamp** [D]; a hook that wants a time stamps
  its own run time, which is when the harness got round to spawning it.
  garlic uses only `session_id` (§ 18).
- **Cost fields in the payload** [B, 2.1.263, D]: `cost.total_duration_ms`
  is `Date.now() − session start` (wall clock, so it includes a laptop's
  sleep); `cost.total_api_duration_ms` is a ledger incremented **once per
  completed API request** by that request's whole duration including
  retries. Between samples its delta is zero for the whole in-flight
  period and then one step equal to the request length at the tick after
  completion, and a request still in flight when the session dies is
  never counted. A one-second sampler can attribute API time per request
  after the fact; it cannot see "the model is generating right now" from
  this field. A separate tool-duration ledger exists in the binary but is
  not in the payload. Whether the ledger includes subagent requests is
  not documented (`prompt_cache` is documented as excluding them).

### 4.5 Settings keys the proposals use

| key | what | evidence |
|---|---|---|
| `statusLine.{type, command, padding, refreshInterval, hideVimModeIndicator}` | the line; `padding` defaults to 0; `refreshInterval` since 2.1.97; `hideVimModeIndicator` (inside the `statusLine` object, not top-level) suppresses the built-in `-- INSERT --` row | D, B |
| `subagentStatusLine` | a second render surface (§ 11): one command per refresh receives every visible subagent row as one JSON object and writes back `{"id","content"}` lines; trust-gated like `statusLine`; may ship in a plugin's `settings.json` | D, B |
| `spinnerVerbs: {mode: "append" \| "replace", verbs: [...]}` | since 2.1.23; `replace` with an empty list keeps the built-ins | D, B |
| `spinnerTipsOverride: {excludeDefault, tips: [...], tipsFile, label}` | since 2.1.45; tip objects `{id, text, cooldownSessions, priority}` and `tipsFile` (absolute or `~/` path, ignored from remote managed settings) since 2.1.247; plain strings only from project and local files | D, B |
| `prefersReducedMotion` | `/config` → Reduce motion; a companion should default `animate` off when it is set | D, B |
| `footerLinksRegexes` | since 2.1.176; link badges beside the status line | D |
| `voice.enabled` | `voiceEnabled` deprecated since 2.1.92; the footer's own voice hint is hidden when a custom status line exists | D |
| `sandbox.enabled` | sandboxing on | D |
| `autoCompactWindow`, `modelPricing` (2.1.243) | context and cost knobs | D |
| `messageIdleNotifThresholdMs` (global config file, default 60000) | the idle-notification threshold behind `idle_prompt` | B, 2.1.263 |
| precedence | managed > `--settings` > project local > shared project > user; `CLAUDE_CONFIG_DIR` relocates the user directory | D |

### 4.6 Focus, interaction and presence

What the harness knows about the user's attention, from the 2.1.263 dump
[B]:

- It keeps a `terminalFocus` state of `focused`, `blurred` or `unknown`
  with a `terminalFocusGainedAt` timestamp. Focus comes from the DEC
  focus-report mode (`FOCUS_EVENTS: 1004` in the harness's mode table;
  `CSI I` / `CSI O` parsed as focus in/out), which the harness probes
  tmux for (`show -gv focus-events`, with a hint to add `set -g
  focus-events on`), and is also inferred from input: a keypress or a
  mouse press in a `blurred`/`unknown` session flips it to `focused`.
- It keeps a `lastInteractionTime` bumped by keyboard input, prompt edits
  and submits, and some mouse button events (the focus in/out sequences
  excluded). Scroll has its own 150 ms "scroll activity" flag that does
  **not** update `lastInteractionTime`.
- **Presence rule:** `function l0n(){let e=QQ();if(e!==void 0)return
  e;return Date.now()-$m()<bXt}` with `var bXt=60000`: focus when known,
  else an interaction within the last 60 s.
- The combined rule has exactly one consumer: the `PushNotification`
  tool skips its notification while the user is present
  (`disabledReason: "user_present"`;
  `CLAUDE_CODE_DISABLE_NOTIFICATION_PRESENCE_CHECK` disables that local
  check, and only when not remote [D]). The raw focus state is read in a
  few more places: prompt suggestions are skipped while unfocused,
  narration is parked while blurred, the away-summary feature subscribes
  to it, and the PR-status polling cadence uses focus together with idle
  time. The documented `CLAUDE_CLIENT_PRESENCE_FILE` (2.1.181+) is the
  same gate from the other direction: while a file that a screen-lock
  listener creates exists, Remote Control pushes are skipped [D].
- **None of it reaches the status line payload or any hook.** The payload
  builder (§ 4.1) emits no presence field; the hook list (§ 4.4) has no
  focus event.
- **`idle_prompt` is the one presence-shaped signal a hook can see.** The
  idle notification controller arms a timer for the idle threshold when a
  query completes and, when it fires, sends the notification only if the
  user has not interacted since the query completed, nothing is loading,
  no dialog is on screen, no loop wakeup is pending and no quota
  auto-resume is armed. The threshold is `messageIdleNotifThresholdMs`
  from the global config file (default 60 000 ms). The notification
  dispatcher runs the `Notification` hooks *before* it chooses a delivery
  channel, and the presence check lives in the push-channel path, so **the
  hook fires whether or not the terminal is focused** [B, 2.1.263]. The
  docs' "only when you appear to be away" describes the desktop and push
  channels. So a `Notification` hook with matcher `idle_prompt` means
  "no keystroke or scroll for about 60 s after Claude finished", which
  includes reading a long answer; § 21.4 uses it as a cap on user time,
  never as proof of absence.

### 4.7 Usage data beyond the payload

- **The OAuth usage endpoint** (`GET https://api.anthropic.com/api/oauth/usage`
  with the session's bearer token and `anthropic-beta: oauth-2025-04-20`)
  is what ccstatusline, ohugonnot and cship call. It is undocumented
  (ohugonnot's README says so plainly [C]). Two issues about persistent
  HTTP 429 from it (#31021, #31637) were closed *not planned*, one
  labelled `invalid`, with no maintainer comment, so the closure and label
  are the whole signal [C]; the request for official quota access (#13585)
  is open [C]; Claude Code's own `/usage` degrades to "last-known usage
  within 60 minutes" when the endpoint rate-limits [D]. A community
  comment says the limit is per access token and suggests refreshing
  tokens for a fresh window [C]; garnish must not port that, it evades a
  limit.
- **`get_usage`** is a real control-request subtype of the stream-json
  SDK protocol (`get_usage is not supported in this context (onGetUsage
  callback not registered)`, with a `skip_behaviors` option) [B], so a
  worker could spawn `claude -p --input-format stream-json` and ask, with
  no credential handling, at the cost of a Claude Code process start and
  an undocumented response shape.
- **`~/.claude/stats-cache.json`** holds per-day `tokensByModel`, message,
  session and tool-call counts (`dailyActivity`, `dailyModelTokens`,
  `modelUsage`, `totalSessions`, `hourCounts`), written by the harness for
  `/usage` [D, L]. No quota percentage; no credentials.

### 4.8 The session registry

`~/.claude/sessions/<pid>.json` exists per running session with the keys
`sessionId`, `bridgeSessionId`, `name`, `nameSource`, `status`,
`statusUpdatedAt`, `updatedAt`, `startedAt`, `pid`, `cwd`, `kind`,
`entrypoint`, `agent`, `version` [L]; `bridgeSessionId` and an
`abandoned-stale` state occur in the binary [B]. Two observations on this
machine [L]: the file is keyed by pid, so `/proc/<pid>` answers "is this
session alive" with no heartbeat at all; and `status` stayed `busy` for
hours on an idle session, so it is a liveness record, not an attention
signal. The format is undocumented and must be treated as fragile (a
missing or unparsable file means "unknown", never "dead").

### 4.9 Verify-first items, in order

Work, not decisions. Each is a one-session check.

1. **Notification row squeeze.** Render a full-width line with an active
   notification (classic renderer) and note whether the harness truncates
   the status line or the notification; decide whether a right-hand
   reserve is worth a config key. [§ 4.3]
2. **Autocompact position.** Re-check the 13 000 constant in 2.1.263 and
   reconcile it with the "at the limit" wording; keep
   `compact_buffer_tokens` either way. [§ 4.3]
3. **A1 dim reset on screen**, and re-locate the row wrapper in the
   current binary. [§ 7.1]
4. **NBSP in VS Code** (the mascot project uses background-colored NBSP
   to stop "host trimming", a second data point that NBSP survives).
   [§ 14]
5. **Blurred terminals keep ticking.** Leave a session unfocused with
   `GARNISH_DEBUG` on and read the tick timestamps; the timer has no focus
   gate in the code, and this confirms it on screen. [§ 19]
6. **`idle_prompt` while focused.** Register a logging hook and confirm it
   fires with the terminal focused and idle, as § 4.6 reads the code.
   [§ 21.4]
7. **`get_usage` cost**: time a `claude -p --input-format stream-json`
   round trip before proposing N10. [§ 9.4]
8. **`stats-cache.json` cadence**: when the harness rewrites it decides
   N2's TTL. [§ 8.5]
9. **`sessions/*.json` lifetime**: whether stale files linger after a
   crash, which decides how `remote`, a session-name fallback and the
   garlic liveness check validate. [§ 4.8, § 21.3]
10. **`session_id` across `/clear`**: whether the id changes (the
    `SessionEnd reason=clear` then `SessionStart source=clear` pair
    suggests a new session), which decides how the heartbeat and garlic
    treat a cleared session. [§ 21.3]

---

# Part II — Target design by concern

## 5. Widget inventory against garnish modules

Status: **have** (garnish module covers it), **partial** (covered with a
gap noted), **missing** (candidate, with its proposal id), **out**
(deliberately not ported, § 14).

| ccstatusline widget | garnish | status | note |
|---|---|---|---|
| Model | `model` | have | |
| Version | — | missing (A12) | `version` payload field; trivial |
| OutputStyle | `style` | have | |
| ThinkingEffort | `effort` | have | theirs is transcript-derived; ours is payload |
| ContextLength / ContextPercentage / ContextPercentageUsable / ContextBar | `context` | have | "usable" = percentage of the autocompact threshold; see A11 |
| TokensInput / Output / Cached / Total | `context` full preset | partial | no separate cache-read / cache-write split; A6 formats cover the numbers |
| CacheTimer | `cache` | have | theirs infers the TTL countdown from transcript timestamps; ours reads `prompt_cache.expires_at` |
| CompactionCounter | — | missing (N4, C1) | count and trigger from the `PostCompact` hook; reclaimed tokens from the transcript |
| TokenSpeed (t/s) | — | missing (C1) | transcript-only |
| SessionClock | `session` | have | |
| SessionCost | `cost` | have | |
| BlockTimer / BlockResetTimer | `limit5h` | partial | A10: elapsed view of the 5-hour window |
| UsageSession / UsageWeekly / UsageSonnet / UsageOpus / ExtraUsage | `limit5h`, `limit7d` | partial | per-model weekly and extra usage need the usage API (C2) or an alternative (§ 9.4) |
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
| SandboxStatus / VoiceMode / RemoteControlStatus | — | missing (A9) | layered settings + `sessions/*.json` matched by session id; `remote` duplicates the harness's `/rc active` indicator |
| Skills (active skill via hooks) | — | missing (B4) | hooks write a per-session log |
| FreeMemory / memory usage | — | out | not a property of the session |
| Link (arbitrary OSC 8) | `text.<name>` | partial | a `url` key on text modules (A8) |
| Separator / FlexSeparator / Spacer / CustomText | `frame.separator`, `right`, `text.<name>` | partial | multiple flex points (A2) |
| CustomCommand | — | out | `SPEC.md` § 3.7 |
| jj (Jujutsu) widgets | — | out | claude-hud supports jj opt-in; no demand here (N12) |
| ContextWindow (size only) | `context` full preset | have | window tag |
| ClaudeSessionId | `session_name` full preset | have | short `session_id` |
| GitCleanStatus / GitIsFork / GitUpstreamOwner / GitUpstreamRepo / GitUpstreamOwnerRepo | — | partial | B2 (`show = ["owner"]`, fork detection needs `gh`, B3) |
| GitInsertions / GitDeletions | `lines` | have | see GitChanges note: theirs is `git diff`, ours is Claude's edits |
| WorktreeMode / WorktreeName / WorktreeBranch / WorktreeOriginalBranch | `worktree` | have | payload `worktree.*` |
| CacheHitRate / CacheRead / CacheWrite | `cache` | have | hit ratio, writes and misses in the full preset; per-turn vs session scope is theirs only |
| ResetTimer / WeeklyResetTimer | `limit5h` / `limit7d` | have | reset countdown; absolute time is A10 |
| CustomSymbol | `text.<name>` | have | a text module holding one glyph |
| VoiceStatus | — | missing (A9) | `voice.enabled` from layered settings |
| TerminalWidth (debug) | `garnish doctor` | have | width shown by `doctor`; not a module |
| Gradients, Powerline segments, global overrides, number formats, hide states, dim-parens | — | missing | A3 A4 A5 A6, B1 |

Their stdin schema (`src/types/StatusJSON.ts`, a loose object) declares
`session_id`, `transcript_path`, `cwd`, `model` (string or object),
`workspace.{current_dir, project_dir}`, `version`, `output_style`,
`effort`, `cost`, `context_window` (with `current_usage`), `vim`,
`worktree`, `rate_limits`. It does **not** model `session_name`,
`prompt_cache`, `fast_mode`, `thinking`, `agent`, `pr`,
`exceeds_200k_tokens`, `workspace.added_dirs / git_worktree / repo`.
garnish's `SPEC.md` § 2.2 is the more complete map; ccstatusline recovers
several of those from the transcript or from git instead. Their
model-context table assumes 200 k with `[1m]` suffix inference for 1 M
models and an 80 % "usable" ratio; garnish's `effective_window −
compact_buffer_tokens` threshold is the better number for A11.

## 6. Rows and layout

### 6.1 How ccstatusline composes a line (`renderStatusLine`, `src/utils/renderer.ts`)

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

No official source bears on any of this; claude-powerline's grid engine
(fr units, `auto`, spans, dividers, automatic culling of empty cells and
rows) is the richest precedent in the market [C].

### 6.2 A2. More than two groups per line (flex points) — Tier A, payload-only

ccstatusline lets any number of `FlexSeparator` widgets sit anywhere in a
line; free space is divided evenly with the remainder going to the first
ones (`spacePerFlex = floor(total / count)`). garnish has `modules` (left)
and `right`. Proposal: allow a `center` group first, since it covers the
common "title in the middle" layout, and generalise `compose_line` to N
groups later only if asked:

```toml
[[line]]
modules = ["path", "branch"]
center  = ["session_name"]
right   = ["clock"]
```

Overflow order stays: drop fill, cut left, then cut center; the right group
is never cut. `align` treats the center group as its own column set.

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

Their `merge` becomes a per-module `glue = true` in garnish ("no separator
after me; with `glue = "tight"` no padding either"), which is also how a
`text.<name>` label can sit flush against the module it labels. Their
`merge-target-hidden` hide state ("a decorative item hides when the widget
it is glued to hides") is the missing half of `SPEC.md` § 3.7 text modules:
`hide_with = "pr"` on a text module makes a label vanish with its module.

### 6.3 A5. Per-module `max_width`; A13. Separator collapse and inheritance — Tier A, payload-only

ccstatusline truncates a widget's rendered text at `metadata.maxWidth` with
`...`, ANSI- and OSC 8-aware. garnish truncates only the whole left group.
Proposal: `max_width = N` on every module (schema-wide, like `label`),
applied before alignment so a long branch name or session title cannot
push the rest of the line off. Truncate graphemes with `…`, keep the OSC 8
wrapper intact (`ansi::truncate` already exists).

ccstatusline drops a manual separator whose neighbour rendered empty and
paints a separator in the color of the previous visible widget
(`inheritSeparatorColors`). garnish collapses around hidden modules
already; the inheritance is a `frame.separator_color = "inherit" | "frame"
| <role>` key.

### 6.4 B1. Real Powerline segments — Tier B, payload-only (measure)

This is ccstatusline's headline visual and the biggest layout change here.
Their model (`src/utils/powerline.ts`, `src/utils/powerline-theme-index.ts`,
`src/utils/separator-index.ts`, `src/types/PowerlineConfig.ts`): every
widget becomes a segment with its own background; a theme supplies fg/bg
pairs that cycle per segment at three color levels (16/256/truecolor
variants of each theme); start and end caps are arrays cycling per line;
several separator glyphs cycle per segment with an optional
inverted-background variant; `autoAlign` pads segments to equal widths
across lines with a per-widget exclusion; `merge = true | "no-padding"`
joins a widget into the previous segment; `continueThemeAcrossLines` keeps
the cycle running from line to line; the FLEX sentinel `\x01FLEX_SEP\x01`
survives painting so flex spacing is resolved after colors. Enabling
Powerline in their TUI rewrites the config (`buildEnabledPowerlineSettings`,
`src/utils/powerline-settings.ts`): default padding becomes a space, manual
separators are stripped, and the default theme is `nord-aurora`; fonts are
detected by scanning font directories for names matching
`/powerline|nerd font|meslo.*lg|cascadia.*code.*pl|fira.*code.*nerd/i` and
by `fc-list | grep -i powerline`, with an offer to clone `powerline/fonts`
and run its `install.sh`. Gradients collapse to their first stop in this
mode. Precedent in the market: claude-powerline, coralline
(pill/lean/classic), claudebar, kcchien [C].

**Their painter algorithm** (`renderPowerlineStatusLine`). Inputs per
line: the rendered widgets (separators filtered out; flex points kept as
positions), the theme's `fg[]`/`bg[]` arrays for the current color level,
`separators[]` with a parallel `separatorInvertBackground[]`, `startCaps[]`,
`endCaps[]`, `autoAlign`, `continueThemeAcrossLines`, and three counters
carried across lines: separator index, theme color index, start-cap slot
index.

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
   as in § 6.1 step 5, the line is truncated to width, and a
   `chalk.reset('')` is appended.
6. **Cross-line state** after a line is printed: separator index advances
   by the separators actually emitted, the theme index by the number of
   color-consuming elements (only if `continueThemeAcrossLines`), the cap
   slot index by the number of segments on the line.

**Their theme table** (truecolor level; MIT, ccstatusline
`src/utils/colors.ts`). Each theme is five `(fg, bg)` pairs cycled per
segment; 16- and 256-color variants are hand-picked in the source. garnish
can derive the 256 level with the mapper in § 7.2 and keep only a
hand-picked 16-color row.

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

**garnish shape.** `frame.style = "powerline"` is a frame (caps on a
rule), not segments. Proposal: a new `[segments]` table, orthogonal to
`frame`:

```toml
[segments]
enabled = true
theme   = "nord-aurora"            # or a list of {fg, bg} pairs under [[segments.palette]]
separators = ["", ""]            # cycled per segment; "" alone is the classic look
caps = { start = [""], end = [""] }
merge = []                         # module ids merged into their predecessor
continue_across_lines = true
```

```rust
/// Painted per line after modules rendered; pure function of its inputs.
struct SegmentPlan<'a> {
    parts: Vec<Vec<Seg<'a>>>,      // split at flex points
    palette: &'a [(Rgb, Rgb)],     // (fg, bg) cycled per color-consuming segment
    seps: &'a [SepGlyph],          // { glyph: &str, invert: bool }
    caps: Caps<'a>,                // start/end arrays cycled per part
    carry: &'a mut Carry,          // sep_idx, theme_idx, cap_idx across lines
}
fn separator(prev: &Seg, next: &Seg, g: &SepGlyph) -> Ansi { /* step 4 above */ }
```

Behaviour: each rendered module gets the next palette pair; separators are
painted fg = previous bg, bg = next bg; `align` already gives equal columns,
so `autoAlign` needs no separate key; `hide_when_empty` removes the segment
and its separator together. Frame and segments compose (a rule can still
fill the gap). Differences worth making: (a) alignment reuses garnish's
`align` columns rather than a second `autoAlign`; (b) the "same background"
rule should pick a contrasting fg from the theme (their `fg(this)`), keep
it; (c) garnish knows the exact width, so the flex split happens before
painting and the sentinel trick is unnecessary; (d) `hide_when_empty`
removes the segment *and* its palette slot, so colors do not shift when a
module hides (theirs advances the theme index only for rendered elements,
which is the same behaviour); (e) `color = never`/`mono` renders segments
as plain text with the frame separator, never as unreadable same-color
blocks. Output cost: two SGR sequences and two resets per segment, about
40 bytes; a 4-line, 16-module layout adds ~1 KB per tick. Within budget,
but pin it in `bench/`. Why B rather than A: it touches the painter, the
frame, the alignment pass and the goldens together, and `theme.rs` needs a
background role set that does not exist today.

### 6.5 One-shot notices and the config-error badge — Tier A

- **One-shot notices.** ccstatusline's `settings.updatemessage = {message,
  remaining}` prints a line under the status line for `remaining` renders
  (decrementing each time), used after installs. garnish equivalent: a
  `<cache>/notice` file `{text, ticks_left}` written by `install`, `config
  migrate` or a version change, printed as a dim extra row and decremented
  per tick (a worker-owned file the tick reads; the decrement is the one
  write, so it is a tick-side write and shares the § 0 decision unless the
  notice expires by time instead of by tick count, which needs no write).
- **Config-error badge.** On an unparseable settings file their line
  renders from in-memory defaults with a red `⚠ invalid config` prefix, and
  the file is never overwritten. garnish already has the `⚠ garnish:` row;
  § 12.1 adds the "never overwrite" sentence to `SPEC.md` § 5.
- **Needs-based work.** Their `renderMultipleLines` computes transcript
  analysis, usage prefetch and status prefetch only when a configured
  widget needs them. garnish's worker model already has this shape; every
  worker in § 9 must spawn only when its module is configured on some line.

## 7. Color and text presentation

### 7.1 A1. Reset the harness's dim at the start of every row — Tier A, verify first

The 2.1.261 binary renders each status line row as `<Text dimColor
wrap="truncate">` (§ 4.3), so the whole row is wrapped in SGR 2.
ccstatusline prefixes every rendered line with `\x1b[0m` for exactly this
reason (`renderMultipleLines`, comment "override Claude Code's dim").
garnish emits resets only *after* painted segments (`src/ansi.rs`), so its
first segment, plain separators and frame glyphs render dim in the real
harness while `preview` shows them at full intensity. Proposal: prefix each
output row with `\x1b[0m` when color is on; add the fact to `CLAUDE.md`
§ Claude Code facts once confirmed on screen, and a golden that pins the
prefix. Cost: a few bytes per row.

### 7.2 A3. Color gradients — Tier A, payload-only (measure)

ccstatusline accepts `gradient:<preset>`, `gradient:RRGGBB-RRGGBB` or
`gradient:hex:a,b,c` as any color value, interpolates in OKLab, applies per
widget or across the whole line, degrades to the nearest ansi16 color, and
collapses to the first stop in Powerline mode (`src/utils/gradient.ts`, 13
named presets ported from gradient-string with attribution).

**Grammar.** `gradient:<name>` | `gradient:<stop>-<stop>[-…]` |
`gradient:<stop>,<stop>[,…]` where a stop is `RRGGBB`, `#RRGGBB` or
`hex:RRGGBB`; the delimiter is `,` if the body contains one, else `-`;
fewer than two valid stops → not a gradient.

**Presets** (from gradient-string, MIT, re-expressed as explicit stops
because interpolation is OKLab rather than an HSV hue spin):

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

**Sampling and application.** sRGB → linear (`c ≤ 0.04045 ? c/12.92 :
((c+0.055)/1.055)^2.4`) → OKLab (Ottosson's matrices, Appendix C) →
interpolate `L, a, b` linearly between the two bracketing stops: `scaled =
t·(n−1)`, `lower = min(n−2, ⌊scaled⌋)`, `frac = scaled − lower` → back to
sRGB, clamped. Per widget: one SGR per **non-whitespace** code point;
whitespace passes through and does not consume a step; CSI/OSC sequences
pass through untouched; the sweep restarts at `t = 0` per widget. Per line:
the sweep spans the visible cells of the whole line (walking display
clusters), and is applied after truncation. ansi16 → no gradient (first
stop or plain). Powerline mode → background collapses to the first stop; a
foreground gradient may still sweep across all non-color-preserving
segments (`powerlineGradientWidth`). Their known limitation: the per-widget
path walks code points, so a ZWJ emoji gets several steps and inert codes
on zero-width joiners.

**xterm-256 mapping** (the standard 6×6×6 cube plus the 24-step gray ramp
[P]):

```text
gray (r == g == b):  r < 8 → 16;  r > 248 → 231;  else 232 + round((r − 8) / 247 · 24)
else:                16 + 36·round(r/255·5) + 6·round(g/255·5) + round(b/255·5)
```

**garnish shape.** A `gradient` value accepted wherever a color is,
computed in `theme.rs` with an OKLab helper (pure arithmetic, about forty
lines, no crate; the `palette` crate is not needed):

```toml
[colors]
accent = "gradient:#89b4fa-#f5c2e7"          # two stops
[modules.context]
bar_gradient = "gradient:ok-warn-danger"     # role names as stops
```

`theme.rs` gains `enum Paint { Solid(Rgb), Gradient(Vec<Rgb>) }` wherever a
role or module color is resolved; `ansi.rs` gains `paint_gradient(text,
stops, level)`. Rules: per-segment gradient by default, `[line].gradient =
"<spec>"` to sweep the visible text of the line; `color = 256` maps each
sample with the table above; `mono` and ansi16 fall back to the role's
solid color. Do better than their limitation without a new crate: step the
gradient per **cell**, using `unicode-width`; a zero-width code point (ZWJ,
variation selector, combining mark) gets no SGR and no step, a wide glyph
consumes two steps, so the sweep is uniform in terminal cells. Docs list
the presets from a `GRADIENT_PRESETS` table so `garnish docs` stays the
source of truth. Cost is one interpolation per cell on painted text, so
keep it out of the fill and frame glyphs by default and measure with
`bench/run.sh`.

### 7.3 A6. Number formats and dim-parens — Tier A, payload-only

ccstatusline's global override `numberFormat` picks a style per kind
(`tokens | speed | percent | memory | cost`) with `precise | compact |
whole` and an optional `decimals` (`src/utils/number-format.ts`).
Resolution order: global per-kind → per-widget → the widget's own baseline
(percent 1 decimal, context bar 0, cost 2). `compact` trims trailing zeros
(`512.0 → 512`, `5.2 → 5.2`); `whole` forces 0 decimals. The TUI cycles
precise → compact → whole with `.`. `dim = "parens"` dims every `(...)`
span: `\x1b[2m(...)\x1b[22m`, re-asserting bold with `\x1b[22;1m` when the
surrounding text is bold, because SGR 22 clears both.

garnish has `durations`. Proposal: a sibling key with the same shape and
per-module override:

```toml
[format]
tokens  = "compact"   # 128k | precise 128,400 | whole 128400
percent = "whole"     # 42% | precise 42.3%
cost    = "cents"     # $1.23 | whole $1
```

`num.rs` gets `struct NumFormat { style, decimals }`; module schemas
declare which kind each number is so docs can say what `format.tokens`
affects. `dim = "parens"` is a segment-level flag: garnish already knows
which segments are "detail" (the full-preset extras), so dim those
segments rather than regex over the rendered text.

### 7.4 A4. Hide conditions as a list — Tier A, payload-only

ccstatusline has a unified `metadata.hide = "no-git,zero,..."` per widget
with per-widget defaults and a TUI checklist (`getHideableStates` on the
Widget interface; `src/utils/migrations.ts` v3→v4 folded older
per-widget booleans such as `hideNoGit` into it). Storage: absent means
"widget defaults"; the editor writes the key only when the enabled set
differs from the defaults, so untouched configs stay minimal. Observed
vocabulary (`src/widgets/shared/hideable.ts` and the widgets):

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

garnish has `hide_when_empty` only. Proposal: keep it as the shorthand and
add a per-module list whose vocabulary comes from the schema:

```toml
[modules.sync]
hide = ["no_upstream", "zero"]      # schema-declared states; docs generated
[modules.limit7d]
hide = ["below:10"]                 # value-threshold states where the module has a percentage
```

`ModuleSchema.hide_states: &[HideState { key, doc, default }]` per module;
`config check` rejects unknown ones; `garnish docs` lists them;
`hide_when_empty` stays as the alias for a module's `empty` state;
`text.<name>.hide_with = "<module>"` is the `merge-target-hidden`
equivalent (§ 6.2).

### 7.5 A7. Path display; A8. Links — Tier A, payload-only

`CurrentWorkingDir` offers fish-style abbreviation (`~/r/g/src`), a segment
count, and `~` collapsing. garnish's `path` has three presets. Proposal:
`style = "full" | "fish" | "tail:N"` on `path`, applied to the base part
(the subpath stays dim as today).

Three things from `src/utils/hyperlink.ts` and the widgets: `GitBranch`
links to `<repo url>/tree/<branch>`; `GitRootDir` links to
`vscode://file/<path>` or `cursor://file/<path>`; the `Link` widget is an
arbitrary OSC 8 anchor. garnish already emits OSC 8 for `pr`. Proposal:
`link = true` on `branch` (uses `workspace.repo.{host,owner,name}` from the
payload, so no git call), `link = "vscode" | "cursor" | "none"` on `path`,
and a `url` key on `text.<name>` modules (static text stays static; the URL
is a string in the config, so § 3.7 holds). Two cautions from the docs:
`footerLinksRegexes` already badges ids the user configures, so do not
duplicate them; `FORCE_HYPERLINK=1` overrides link detection [D].

### 7.6 Per-module option vocabulary (from the editor keybinds)

Every option ccstatusline exposes per widget, mapped to a garnish key. Most
exist already; the rest are the Tier A keys above.

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
| `v` view last/count/list, `t` tokens reclaimed, `s` split by trigger | compaction counter views | N4 / C1 |
| `z` zero conflicts display | show `0` or hide | `hide = ["zero"]` (A4) |
| `e` edit text / edit cmd, `t` timeout, `p` preserve colors | custom text / command | `text.<name>` (have); command: out |
| `r` raw value, `m` merge, `x` exclude align, `h` hide… | item-level | `label = ""`, `glue`, `align_stop`, `hide` |

## 8. Payload-only and settings-derived modules

### 8.1 A9. Settings-derived identity modules — Tier A, cached file read (30 s)

ccstatusline reads Claude's settings in layer order project-local >
project > user-local > user (`src/utils/claude-settings.ts`; the user dir
honours `CLAUDE_CONFIG_DIR`); the first layer that defines the key wins;
"no file exists at all" → `null` (hide), "files exist but no key" →
`false`. Keys: `sandbox.enabled`, `voice.enabled`. Remote control: scan
`<config>/sessions/*.json` for the file whose `sessionId` equals the
payload's `session_id`; enabled iff `bridgeSessionId` is a non-empty
string. Account: `~/.claude.json` (or `<CLAUDE_CONFIG_DIR>/.claude.json`)
→ `oauthAccount.emailAddress`. The official precedence adds managed and
`--settings` layers above these four files [D]; for files, their order is
right.

garnish already reads settings for the autocompact override, cached 30 s.
Proposal: small modules on the same cached read: `sandbox` (glyph when
sandboxing is on), `voice` (glyph when voice mode is on; the harness hides
its own voice hint when a custom status line exists [D]), `account`
(email, hidden by default; the field name is community-documented only
[C]), and `remote` (glyph when the session is bridged; matched by
`session_id`, which garnish has, and confirmed locally by the registry
keys in § 4.8 [L, B]). `remote` is a duplicate of the harness's own `/rc
active` footer indicator [D], so it is the lowest-value of the four. This
raises the fixed module count from 21; the schema, docs and goldens absorb
it.

### 8.2 A10. Elapsed view and absolute times on the usage modules — Tier A, payload-only

ccstatusline's BlockTimer infers the current 5-hour block by scanning
every transcript under `~/.claude/projects` for the oldest timestamp
within a progressive 10/20/48-hour lookback, flooring to the hour, and
caching the result (`src/widgets/BlockTimer.ts`). garnish has
`rate_limits.five_hour.resets_at`, so the same view is arithmetic:
elapsed = 5 h − (resets_at − now). Proposal: `mode = "elapsed" |
"remaining"` on `limit5h` plus an optional time-cursor glyph on its mini
bar (the bar shows usage; the cursor shows how far through the window we
are, so "60 % used at 20 % elapsed" is visible at a glance). Absolute
reset times (`resets 14:30`) with the local zone via jiff are the natural
companion; claude-hud offers `relative`, `absolute`, `both`, `elapsed`,
`elapsedAndAbsolute` plus an `hourCycle`, which shows the demand [C]. The
harness re-runs the line at each `resets_at` (§ 4.2), so a countdown is
never stale at the boundary.

### 8.3 A11. `context` percentage of the usable window — Tier A, payload-only

`ContextPercentageUsable` shows usage as a share of the autocompact
threshold instead of the raw window, and the transcript-derived context
length resets to zero after a `compact_boundary` row. garnish already
knows the threshold (`compact_buffer_tokens`, `SPEC.md` § 2.3). Proposal:
`scale = "window" | "usable"` on `context`; the bar's 100 % becomes the
autocompact point. Correction from the docs: `used_percentage` is always
measured against the full window, and once any autocompact window
override is set it "no longer indicates when compaction will run" [D];
`SPEC.md` § 2.3 already models that chain, and the buffer constant is
verify item 2 in § 4.9.

### 8.4 A12. `version`; N6. Reduced motion; `provider` — Tier A

- **A12.** The `version` payload field exists and has no module; a dim
  `v2.1.263` costs nothing and helps bug reports [D].
- **N6.** When the settings read finds `prefersReducedMotion: true`, treat
  `animate` as off unless the garnish config sets it explicitly [D, B].
- **`provider`** (from ccsidekick): a badge for bedrock / vertex / foundry
  / proxy / api from the environment (`CLAUDE_CODE_USE_BEDROCK` etc.,
  `ANTHROPIC_BASE_URL`), hidden for a subscription; payload/env only.

### 8.5 N11. Burn rate and time-to-limit; pace — Tier A, payload-only

codachi shows a pace delta `⇡5%`/`⇣2%` (`used% − elapsed%` of the window,
from `resets_at` and the window length, 300 or 10 080 min; positive red,
negative green); ccsidekick colors by a pace band `r = used_fraction /
max(elapsed_fraction, 0.01)` (`r ≤ 1` nominal, `≤ 1.5` caution, else
critical; below 20 % used the pace is ignored, above 80 % always
critical); coralline's `burn` and ccusage's `$/hr` are the market form
[C]. Proposal on `limit5h`/`limit7d`: `pace = true` shows the delta,
`pace_colors = true` colors by the band, and `eta = true` shows the
projected time until the binding window reaches 100 % from
`used_percentage` and `resets_at` alone.

### 8.6 N2 / N3. A `today` view — Tier B

- **N2, from `stats-cache.json`.** A worker reads the harness-maintained
  file (no credentials, no transcript) and caches `dailyModelTokens` for
  today; render tokens by model or a total. Cost needs a pricing table,
  which garnish does not have (`modelPricing` in settings is a
  user-supplied override, not a table [D]); show tokens only. Verify how
  often the harness rewrites the file (§ 4.9 item 8).
- **N3, from the payload.** claude-hud folds `cost.total_cost_usd` into a
  per-day ledger keyed by `session_id` on every render (baseline on first
  sight, midnight reset, drop entries unseen for 24 h) [C]. That needs a
  tick-side write, which `SPEC.md` forbids; it shares the § 0 decision
  with the companion memory. With the garlic heartbeat worker of § 21.3 in
  place, the same ledger can be written by that worker every 30 s instead,
  which removes the tick-side write and makes N3 a Tier B worker feature.

### 8.7 N9. Usage snapshot interop — Tier B

claude-hud reads and writes an "external usage snapshot" JSON
(`updated_at`, `five_hour`, `seven_day`, `balance_label`, `model_scoped[]`
with `display_name`, `utilization`, ISO `resets_at`); coralline's
`VL_LIMIT_SYNC` solves the same idle-session problem with a file store
[C]. garnish could read one as a fallback when the payload lacks
`rate_limits` and, through the heartbeat worker, write one so idle
sessions and other tools see fresh windows.

## 9. Workers

Everything here runs off the tick. Each subsection records the exact
commands, file formats and fallbacks ccstatusline uses, then the worker
shape for garnish.

### 9.1 B2. Git details in the existing worker — Tier B, cached worker

Their commands (`src/utils/git.ts`), one per widget, each cached
separately: `status --porcelain -z` (flags for staged / unstaged /
untracked, and conflicts when a line starts with `DD|AU|UD|UA|DU|AA|UU`),
`diff --shortstat`, `rev-list --left-right --count HEAD...@{upstream}`,
`ls-files --unmerged`, `rev-parse --short HEAD`, `symbolic-ref --short
HEAD` (null when detached), `stash list`, `remote get-url origin`,
`rev-parse --git-dir` (worktree detection: `.git/worktrees/<name>`).
Cache: in-memory key `command|cwd`; on disk one file per repository
`~/.cache/ccstatusline/git-cache/git-<sha256(gitdir)>.json` keyed by
command, entries `{output | null, createdAt, headMtimeMs, indexMtimeMs}`;
fresh iff both mtimes are unchanged **and** (`ttl == 0` or age ≤ ttl, ttl
0–60 s, default 5); failures cached as `null`; written via one stable
`.tmp` path and `rename`. `GIT_OPTIONAL_LOCKS=0` in the environment
(documented as equivalent to `--no-optional-locks` [P]; garnish already
sets it). coralline does one `git status --porcelain=v2 --branch` per
render [C].

garnish shape: the `branch` worker already runs `status --porcelain=v2
--branch`, which yields branch, upstream, ahead/behind and every file flag
in one call; add `stash list --porcelain` to the same run, and take the
**invalidation lesson**: store `head_mtime`/`index_mtime` in the entry so
the existing validator treats a commit or a `git add` between ticks as a
miss even inside the TTL (garnish already invalidates on `head`/`upstream`;
the index mtime covers the dirty flag). Expose the extra data as options
of `branch` (`show = ["sha", "dirty", "counts", "stash", "conflicts"]`),
not modules. Display vocabulary worth copying: `+staged *unstaged
?untracked !conflicts` as single glyphs with counts. From ccsidekick: the
in-progress operation glyph (`rebase 2/7`, `merge`, `cherry-pick`,
`revert`) from the existence of `.git/rebase-merge|rebase-apply|MERGE_HEAD|
CHERRY_PICK_HEAD|REVERT_HEAD`, no process; `tag` via `describe --tags
--exact-match` in the worker; `GIT_DIR`-style location variables stripped
so a hook environment cannot redirect the reads.

### 9.2 B3. CI status on `pr` — Tier B (B3a) or a network decision (B3b)

Their flow (`src/utils/git-review-cache.ts`, `git-remote.ts`,
`src/widgets/GitCiStatus.ts`): cache file
`git-review-<sha256(cwd ‖ "\0" ‖ ref)[..16]>.json` where `ref` is
`branch:<name>` or `head:<short sha>`; TTL 30 s; the entry records whether
checks were queried so enabling the CI widget forces one refresh. Miss or
stale → spawn self detached with `--internal-refresh-git-review-cache <cwd>
<metadata|checks> <lockPath>` after taking a lock next to the cache file
(stale after 30 s); the tick returns the stale data meanwhile; the refresh
mode reads no stdin, prints nothing, and only unlinks the lock path it can
derive itself. Provider: origin URL → ssh alias resolved with `ssh -G
<host>` → github.com ⇒ `gh`, gitlab.com ⇒ `glab`, anything else ⇒ probe
`gh auth status --hostname` and `glab auth status --hostname`; CLI timeout
5 s shared across the attempts. Fetch: `gh pr view --json
url,number,title,state,reviewDecision[,statusCheckRollup]`; if the checks
field errors (token lacks scope), retry without it; if nothing resolves,
retry `gh pr view <branch> --repo <origin ref>` (forks). Label: MERGED,
CLOSED, APPROVED, CHANGES_REQ, OPEN. CI rollup: CheckRun rows use
`status`+`conclusion`, StatusContext rows use `state`; NEUTRAL/SKIPPED
ignored; glyphs `✓ ✗ ●`, `-` when no checks.

garnish gets the PR number, URL, review state and `pr.kind` from the
payload and makes no network calls; the harness shows its own PR / MR
badge [D]. Only checks need a worker, two options:

- **B3a (preferred on this machine).** Daniel's `shuck` daemon already
  follows the working tree's PR and its CI. If it persists state on disk,
  a `ci` option on `pr` can read that file with no process at all. Check
  what `shuck monitor` writes before choosing.
- **B3b.** A `garnish refresh --module pr` worker running `gh pr view
  --json statusCheckRollup` with the existing lock and TTL machinery, 60 s
  TTL, failure cached like any other entry, the "checks unavailable →
  metadata only" fallback, `head` in the validator. This lifts "no network
  calls" indirectly, so it is a § 0 decision even though the tick stays
  payload-plus-cache.

Rendering: `✓ 12` / `● 3` / `✗ 1` after the state glyph, colored ok /
warn / danger.

### 9.3 C2. Usage API for per-model weekly limits and extra usage — Tier C, network worker

Their flow (`src/utils/usage-fetch.ts`, `usage-prefetch.ts`,
`usage-windows.ts`): in-memory cache (180 s; error entries 30 s) → disk
cache `~/.cache/ccstatusline/usage.json` (180 s by mtime, only if the
stored `tokenHash` matches the current token's fingerprint and the fields
the configured widgets need are present) → no token ⇒ `no-credentials` →
active lock (`usage.lock` JSON `{blockedUntil, error}`, capped at 24 h) ⇒
serve stale → write lock `now + 30 s` → HTTPS GET of the endpoint in § 4.7
with `Authorization: Bearer <token>` and `anthropic-beta: oauth-2025-04-20`,
via `HTTPS_PROXY` if set, 5 s timeout → 429 ⇒ lock `now + Retry-After`
(default 300 s) → parse → write cache with token hash. Token:
`~/.claude/.credentials.json` `.claudeAiOauth.accessToken`, or on macOS the
keychain item "Claude Code-credentials" (newest of several candidates by
modification date). Response: `five_hour`/`seven_day` `{utilization,
resets_at}` (a null bucket means 0 % on Enterprise, their issue #343),
`limits[]` of `{kind: session | weekly_all | weekly_scoped, utilization,
resets_at, scope.model.display_name}` (newer accounts, their issue #503),
`extra_usage {is_enabled, monthly_limit, used_credits, utilization,
currency}`. Per-model registry (`WEEKLY_MODEL_USAGE_BUCKETS`): Sonnet ↔
`seven_day_sonnet`, Opus ↔ `seven_day_opus`, Fable ↔ `weekly_scoped` only.
Stdin `rate_limits` wins over the API for the fields it carries; only
missing fields trigger a fetch.

Cost for garnish: `reqwest` (the crate map's chosen HTTP crate), reading
the user's OAuth token, and lifting "no network calls". Would be opt-in
(`[usage] api = true`), network-worker only, cache served stale on failure,
token fingerprint in the entry, the 24 h lock horizon. Modules: `limit7d`
gains `model = "all" | "sonnet" | "opus" | "<display name>"`
(server-supplied names, so no hard-coded model list) and a new `extra`
module with `show_limit`. **The case is weaker than it looks** (§ 4.7):
the endpoint is undocumented, its rate limiting has been closed as not
planned, most of the market refuses to call it, and the payload schema
suggests the harness is moving these fields toward the payload. If a later
version adds `model_scoped` or `extra_usage` to the payload, C2 becomes a
Tier A payload-only module and should be re-tiered before anything
network-related is built.

### 9.4 Alternatives to C2 (evaluate before it)

| option | tier | shape | cost |
|---|---|---|---|
| N10 `get_usage` control request | C | worker runs `claude -p --input-format stream-json --output-format stream-json`, sends `{"type":"control_request","request_id":…,"request":{"subtype":"get_usage","skip_behaviors":true}}`, parses `rate_limits` incl. `model_scoped` | official protocol, no token handling; a Claude Code process start per refresh (verify item 7) and an undocumented response shape |
| N2 `stats-cache.json` | B | § 8.6 | no quota percentage; tokens per day only |
| N9 usage snapshot | B | § 8.7 | interop, not new data |

### 9.5 C3. Claude service status; C5. Update check — Tier C, network

`ClaudeStatus` polls `status.claude.com/api/v2/status.json` (→
`status.indicator`: none / minor / major / critical / maintenance) and
`incidents.json` only when a widget enables history, every 5 min, backs
off 30 s on failure, serves stale, and renders a 48-hour strip of eight
six-hour buckets (`▮`) colored by the worst overlapping incident impact
(`src/utils/claude-service-status.ts`, `src/widgets/ClaudeStatus.ts`).
Both endpoints return Statuspage JSON and `status.anthropic.com` serves
the same page id [P]. Same cost class as C2 (network, `reqwest`), no
credentials. Decision: bundle with C2's network decision or skip.

C5: ccstatusline queries the npm registry from the TUI and offers the
install command (`src/utils/update-checker.ts`). For garnish this would be
a crates.io or GitHub Releases request from `doctor` or the setup TUI,
never from the tick. Low value while installs are `cargo install
--locked`.

### 9.6 C1. Transcript-derived metrics — Tier C, cached worker (transcript non-goal)

Row shapes that matter (`src/utils/jsonl-*.ts`, `compaction.ts`,
`speed-metrics.ts`, `speed-window.ts`): `{type: "user" | "assistant" |
"system", subtype?, isSidechain?, isApiErrorMessage?, timestamp, message:
{usage: {input_tokens, output_tokens, cache_read_input_tokens,
cache_creation_input_tokens}, stop_reason?, content}}`; compaction rows
are `type: "system", subtype: "compact_boundary", compactMetadata:
{trigger: "auto" | "manual", preTokens, postTokens}`; subagent transcripts
live in `subagents/agent-<id>.jsonl` beside the main file. The row shape
is confirmed by community tooling only; a collaborator comment on #16944
confirms that subagent compaction exists and follows the main mechanism,
not the row shape [C, M]. Derivations: token totals (sidechain and
API-error rows excluded; when `stop_reason` exists only rows with a stop
reason count, to avoid double-counting streamed partials); context length
= usage of the newest main-chain row after the last boundary, else
`postTokens`, else 0; compaction stats = count, split by trigger, Σ max(0,
pre − post) reclaimed; speed = per assistant row an interval from the
previous user row's timestamp, intervals merged, tokens ÷ merged duration,
optionally over a rolling window of 0–120 s; session duration = first to
last timestamp when the payload lacks `total_duration_ms`. Reading: a
streaming line iterator, a reverse iterator for tail lookups, and for the
cache timer a 32 KB tail read that grows backwards until a full row is
found.

**What still needs the transcript after § 10.3:** `speed` (tokens per
second) and the exact reclaimed-tokens figure. The compaction count and
its auto/manual split come from the `PostCompact` hook (N4); an
approximate reclaimed figure comes from the payload, since the tick sees
`context_window.total_input_tokens` before and after the boundary the hook
marks. garnish shape if approved: a `refresh --module speed` worker that
stores `{offset, size, inode}` in its cache entry and reads only bytes
appended since the last run (O(new rows) per refresh), TTL 2–5 s, a ring
of the last N `(user_ts, assistant_ts, output_tokens)` triples in the
entry; the tick reads the cache. `SPEC.md` § 2.2 marks `transcript_path`
"not used"; lifting it is the § 0 "Transcript" decision, now only for
these two values.

### 9.7 Timers recorded for completeness — Tier D

- Cache timer: newest main-chain row is a `user` row ⇒ HOT (Claude is
  working); else countdown `ttl − 5 s − (now − last assistant row with
  cache activity)`, TTL 5 m or 1 h; glyphs 🟢 > 50 %, 🟡 > 20 %, 🔴, ❄️
  COLD. garnish's `cache` reads `prompt_cache.expires_at`; only the glyph
  ladder is worth borrowing as `cache.glyphs = true` mapped to
  ok/warn/danger.
- Block timer: glob `~/.claude/projects/**/*.jsonl`, newest mtime first,
  progressive lookback 10 → 20 → 48 h, collect every row timestamp, walk
  from the newest until a gap ≥ 5 h, floor the block start to the hour,
  cache until the window ends, cache an empty result for 1 min. garnish:
  `resets_at` arithmetic (A10).

## 10. Hooks

Everything hook-fed in garnish shares one hidden subcommand, one file
class and one settings edit. This section fixes the shared contract, then
lists the modules that ride on it.

### 10.1 The shared contract

- **Subcommand.** `garnish hook` (hidden) reads the hook JSON from stdin,
  classifies in-process, appends one line to
  `<cache>/<session>/events.jsonl` (bounded to 200 lines), rewrites a
  small `<cache>/<session>/events.json` summary, and exits 0 with empty
  stdout. It never prints: an async hook's stdout is delivered into the
  conversation on the next turn (§ 4.4), and a sync hook's stdout would
  be shown or injected depending on the event.
- **Registration.** Every entry carries `async: true` so it can never
  delay a tool or a prompt, and a `_tag: "garnish-managed"` marker so
  `install --hooks` / uninstall can find its own entries without
  matching on the command string (garlic's prefix matching is a
  cautionary example, § 22). ccstatusline uses the same tag pattern
  (`_tag: ccstatusline-managed`, re-synced on every save, legacy
  untagged entries matching the command pattern removed).
- **Two install paths.** `garnish install --hooks` writes the entries
  into the user's `settings.json`, preserving key order as `install`
  already does; a garnish **plugin** (`.claude-plugin/plugin.json`,
  `hooks/hooks.json`, `skills/setup/SKILL.md`, `bin/garnish` on the Bash
  PATH, cache under `${CLAUDE_PLUGIN_DATA}`; `claude plugin init`
  scaffolds it, `claude plugin validate --strict` checks it in CI)
  installs the same hooks without editing user settings [D]. The plugin
  cannot carry `statusLine`, so its setup skill still writes that key
  (§ 4.4). Distribution needs a marketplace repository; Daniel already
  publishes one for garlic (`justanotherspy/claude-plugins`). Which
  events the plugin registers is decided by what the user configures; a
  hook that runs on every tool call for a module nobody enabled is
  waste, so `install --hooks` derives the needed set from the config,
  as ccstatusline does.
- **Event log rows** are `{ts, kind, ...}` with no free text copied from
  the hook input except tool names and a sanitised skill name, so
  nothing a tool printed can reach the status line unescaped.
- **`gc`** sweeps the log with the session directory (24 h idle).

### 10.2 B4. Active skill — Tier B, cached file read

ccstatusline registers a `PreToolUse` hook with matcher `Skill` and a
`UserPromptSubmit` hook that call it in `--hook` mode; the handler
appends `{timestamp, session_id, skill, source}` rows to
`~/.cache/ccstatusline/skills/skills-<session>.jsonl` (`tool_input.skill`
for the tool, `/^\/([a-zA-Z0-9_:-]+)/` on the prompt), and the widget
shows the last skill until the next prompt clears it (`src/utils/hooks.ts`,
`hook-handler.ts`, `src/widgets/Skills.ts`). The matcher and the
`UserPromptSubmit` shape are confirmed [D]. garnish: a `skill` module
with `show = "last" | "count" | "list"` and `hide = ["empty"]`, reading
the summary's last-skill field.

### 10.3 N4. `compactions` via `PostCompact` — Tier B, cached file read

The `PostCompact` hook fires with matcher `manual`/`auto` and carries
`trigger` and `compact_summary`, no token counts (§ 4.4). Entry:
`{"PostCompact": [{"matcher": "auto|manual", "hooks": [{"type":
"command", "command": "garnish hook", "async": true}]}]}` appends
`{ts, trigger}`; the module renders `⟲ 3` with `(2 auto, 1 manual)` in
the full preset. Reclaimed tokens: the exact figure needs the transcript
(C1); an approximation is the drop in `context_window.total_input_tokens`
across the boundary the hook marks, which the tick can compute from two
consecutive payloads only if something remembers the previous one, so
the approximation belongs in the heartbeat worker's samples (§ 21.3),
not on the tick. claudia-statusline shows a `Compacting… ⠋` then `✓`
state from the same hooks [C]; `PreCompact` gives garnish the same
"compacting" moment.

### 10.4 The companion's events (§ 17.2) and garlic's liveness hook (§ 21.4)

Both ride the same subcommand: `PostToolUse` / `PostToolUseFailure` /
`PreToolUse(Skill)` / `UserPromptSubmit` for the companion's outcome
categories, and `Notification` with matcher `idle_prompt` for the one
presence-shaped signal the harness exposes. Neither needs `PostToolBatch`
(it co-fires and would double count; ccsidekick deliberately leaves it
unwired). Other events worth a row for the companion's voice, all
documented: `StopFailure` (`rate_limit`, `overloaded`,
`authentication_failed`, `billing_error`), `SubagentStart`/`SubagentStop`,
`PostModelSwitch`, `WorktreeCreate`/`WorktreeRemove` [D].

## 11. Subagent rows — N1, Tier B

`subagentStatusLine` is a second render surface that none of the mined
projects cover: one command run per refresh receives every visible
subagent row as one JSON object, `{…base fields, columns, tasks: [{id,
name, type, status, description, label, startTime, model, effort,
contextWindowSize, tokenCount, tokenSamples, cwd}]}`, and writes back one
`{"id", "content"}` line per task; it is trust-gated like `statusLine`
(`Skipping subagentStatusLine execution - workspace trust not accepted`),
per-task `model` and `contextWindowSize` arrived in 2.1.205 and `effort`
in 2.1.214 [D, B]. coralline already themes those rows [C].

Proposal: `garnish subagents` reuses the module renderer with `columns`
in place of `COLUMNS`: `name`, `model`, `effort`, a context gauge from
`tokenCount / contextWindowSize`, elapsed from `startTime`, `status`
coloring; `[subagents]` in the TOML picks the modules and the preset.
Tick class: payload-only (one process per refresh, no cache). The plugin's
`settings.json` may ship it as a default (§ 4.4), which is the one status
line surface a plugin can install by itself.

## 12. Config lifecycle, sharing and rotation

### 12.1 Lifecycle (from ccstatusline's schema versions)

Their settings carry `version: 4`; migrations v1→v2 (add version), v2→v3,
v3→v4 (per-widget `hideNoGit`-style booleans → `metadata.hide`);
`detectVersion` treats a missing field as v1. Load: parse JSON → if
unversioned, validate against the v1 schema then migrate → else migrate
if older → validate current schema → **persist the migrated file only if
validation passed** → run. Any failure: log, set `lastLoadError`, run on
in-memory defaults, render the red badge, and never write the file. The
TUI asks for confirmation before `Save` would replace an invalid file.
Writes are atomic through the symlink target: resolve the link, write
`<name>.<pid>.<ts>.tmp` in the target's directory, `rename`. Import:
refuses a newer `version`, migrates an older one, validates; preview lists
the keys that would change; `replace` keeps `installation`, `merge`
overlays only the keys present in the import; `installation`, `version`,
`updatemessage`, `exportedBy` are never imported.

garnish shape: TOML plus per-key fallback already survives unknown keys,
so no version field is needed until a key is *renamed*; when that day
comes, `garnish config migrate` (rename in place, `.bak` first, refuse on
parse error) is the whole feature. Add to `SPEC.md` § 5: **"a config that
fails to parse is never rewritten by any command"**; garlic's `load_config`
does the opposite (§ 22) and shows why the sentence is needed. `config
export` is `cat`; a `config import <file> [--merge]` that validates then
writes is cheap and gives a TUI its import screen. Settings writes take
ccsidekick's contract: write temp, rename, re-read, parse, restore the
previous text on failure; keep the oldest and newest backups only.

### 12.2 Sharing (from ccstatusline-editor) — Tier A

The editor's whole loop is "a config file you can hand to someone,
preview without installing, and apply with one command" (`POST /api/share`
into Workers KV → `?s=<id>`; apply with `npx -y @refinist/ccsa@latest
'<json>'`, which backs up the current settings to a timestamped copy under
`~/.config/ccsa/`; `ccsa export` pulls the live config back). garnish's
TOML already is that file:

- `garnish config share` prints the config as a single line; `garnish
  config apply '<toml>'` on the other end validates before writing and
  backs up like `install` does. No server: the payload is the TOML
  itself, base64 if it must survive a chat client.
- `garnish preview --config <file|->` renders a foreign config against the
  bundled fixtures at a chosen width, so a shared config can be seen
  before it is applied. `preview` already has `--width`.
- `garnish preview --html` emits a self-contained HTML page (spans with
  inline colors, monospace, fixed width) that a browser or
  `html-to-image` can capture; zero dependencies, and it doubles as the
  README gallery generator (gallery samples must fit GitHub's width).
  `--png`/`--svg` are not crate-free and are not proposed.
- claude-powerline's Powerline Studio (web configurator, paste-to-edit,
  copy JSON) and coralline's importable `~/.p10k.zsh` values are the
  market's versions of the same loop [C].

### 12.3 Rotation — Tier A

The editor's rotation page builds a pool of themes, a period (hourly /
daily / weekly / custom) and a strategy (cycle / shuffle), or "one look
per weekday"; its CLI picks `slotIndex(date, period) mod themeCount`
(rotating the Sunday-first card order into epoch order because epoch day
0 was a Thursday). For garnish a rotation is a stateless function of the
clock it already has:

```toml
theme = ["nord", "dracula", "catppuccin-mocha"]
[rotation]
period   = "day"          # hour | day | week
strategy = "cycle"        # cycle | shuffle (FNV of the slot)
weekdays = { mon = "nord", tue = "dracula" }   # optional pin
```

Picked as `floor(now / period) mod n`; no CLI toggle, no bundle file, no
schedule registration: the config re-reads every tick. Same for `preset`
and `icons` if wanted.

### 12.4 Templates, and a web preview later — Tier C

Templates are the presets gallery (`SPEC.md` § 12): "apply a template
then tweak" is `garnish config init --preset <name>` followed by editing;
a setup TUI should open on the gallery, not on an empty line. The honest
web preview for garnish is a static page that runs the *real* renderer
compiled to WebAssembly (`wasm32-unknown-unknown`; the render path has no
I/O once payload and config are strings), fed by a TOML textarea and a
fixture picker; it would never drift from the terminal output the way the
editor's hand-ported preview can. Cost: a `wasm-bindgen` build target and
a static site; value: a gallery and share links (`?toml=<base64>`) with no
backend. Not a recommendation.

## 13. Setup, install and doctor

### 13.1 What ccstatusline does

Running the binary on a TTY (no piped stdin) opens the TUI
(`src/tui/App.tsx`): a **live preview** at the top of every screen at the
current terminal width with a truncation warning; a **main menu** (Edit
Lines, Edit Colors, Powerline Setup, Terminal Options, Global Overrides,
Configure Status Line with refresh interval 1–60 s and git cache TTL,
Export / Import with a replace-or-merge preview, Install / Manage
Installation, Check for Updates, Star on GitHub, Save & Exit, Exit without
saving; `Ctrl+S` saves anywhere; an unsaved-changes dialog guards exit);
a **line editor** (move / add / remove / edit; a widget picker with fuzzy
and initialism search, `gab` → GitAheadBehind; a footer listing only the
keys that apply to the highlighted widget; a per-widget editor for raw
value, colors, number format, hide states, symbols); **colors** (theme
selection with "customize" copying the theme onto each widget; changing
the color level sanitises colors the new level cannot show); **Powerline
setup** (enable, theme, separators, caps, font detection, an offer to
install the fonts); the **install flow** ("Pinned global install" at the
exact version versus "Auto-update" via `npx -y ccstatusline@latest`; a
confirmation dialog listing every side effect before anything is written;
backups `.orig` (first ever) and `.bak` (latest); a warning on an existing
`statusLine`; uninstall of the line, the hooks, or both); **manage
installation** (current command, install style, registry check). Install
detail (`installStatusLine`): write `statusLine = {type: "command",
command, padding: 0}`; keep an existing `refreshInterval`, else set 10
when `claude --version` supports it; save `{method, packageManager,
installedVersion}`; sync hooks. Twenty-four screens in all; the items
editor keys are ↑↓, Enter (move mode), `a`dd / `i`nsert, `k` clone,
`d`elete, `c`lear, `r`aw, `m`erge, e`x`clude from align, `.` precision,
`h`ide; the color keys `f`/`h`ex/`a`nsi256/`g`radient/`b`old/`d`im/`r`eset/
`c`lear/`s`how separators, with a footer warning that VS Code's
"Terminal › Integrated: Minimum Contrast Ratio" alters colors.

### 13.2 What garnish can do better

- garnish re-reads its TOML every tick, so a TUI that edits the live file
  is reflected in Claude Code within a second: no save-to-apply step and
  no "unsaved changes" dialog, only an undo (keep a `.bak`).
- The preview can be exact: garnish knows the box width and has
  `preview` fixtures; theirs is terminal width minus a guess.
- Every option, its type, default and doc string already lives in
  `ModuleSchema`; a per-module editor can be generated from it, as the
  docs are, so a new option never needs TUI code.
- The presets gallery gives the picker thirteen complete starting points
  instead of an empty line.

### 13.3 Options (Daniel's choice)

| option | dependencies | what it gives | cost |
|---|---|---|---|
| **7.3a Skill only** (already planned, `SPEC.md` § 13, Phase 18) | none | Claude edits the config in conversation, with `preview` for the check; the market floor (§ 2.3), and the harness's own `/statusline` competes here | no visual picker; not what Daniel said he liked |
| **7.3b Prompt wizard** `garnish setup` | `inquire` 0.9.4 (or none, with numbered menus) | preset, icons, theme, frame, lines; writes the config and runs `install`; prints a preview after each step | no live preview pane; ~500 lines |
| **7.3c Full TUI** `garnish setup` | `ratatui` 0.30.2 + `ratatui-crossterm` 0.1.2 + `crossterm` 0.29.0 | the ccstatusline experience: preview pane, line editor, schema-driven module editor, theme/frame pickers, install and doctor screens, import/export; three Rust precedents ship it (§ 2.3) | new crates (a crate-map decision), binary size and compile time, a large test surface (snapshot tests via ratatui's `TestBackend`), ~3–4 k lines |

Recommendation if 7.3c is chosen: keep the TUI in its own module tree
behind a cargo feature (`--features setup`) so the tick path and
`bench/run.sh` are unaffected, enter it only when `garnish` runs on a TTY
with no stdin (so `garnish` alone opens setup and `garnish < payload.json`
renders), and generate every editor screen from `ModuleSchema`. The
install screen should mirror their confirmation dialog: list the settings
path, the exact `statusLine.command`, the backup name and whether hooks
will be added, then ask once. The setup skill's contract comes from
ccsidekick: run `list` first, map plain-English intent onto flags, never
pass a value that did not validate; `garnish setup --preset … --theme …`
is the non-interactive twin.

### 13.4 N5. `doctor` and `install` checks — Tier A

Report workspace trust, `disableAllHooks`, `allowManagedHooksOnly`; set
`statusLine.hideVimModeIndicator: true` (inside the `statusLine` object)
when the `vim` module is on; suggest
`refreshInterval` when `clock`, `limit*` countdowns or animation are
configured (the docs recommend it for time-based segments); detect
harness runs by `CLAUDE_CODE_CHILD_SESSION=1` / `CLAUDECODE=1` so
`preview` can tell a manual run from a harness run; warn when a
`garlic hook prompt` entry appears both in settings and in an enabled
plugin (double counting, § 22); and, learning from garlic's installer,
refuse to touch a `settings.json` that does not parse. Recommend
`install --absolute` as the default: the pinned-vs-latest lesson
(200 ms vs 1.1 s startup) translates directly.

## 14. Deliberately not ported

- **CustomCommand** (arbitrary shell): `SPEC.md` § 3.7 rules it out; its
  existence is why they need a per-widget timeout and a "preserve rendered
  colors" flag.
- **Windows** support and the `docs/WINDOWS.md` caveats: non-goal.
- **jj widgets** (N12): claude-hud supports Jujutsu opt-in with one
  read-only subprocess; out of scope unless Daniel uses jj.
- **CacheTimer heuristics**: `prompt_cache.expires_at` is exact.
- **BlockTimer transcript inference**: `resets_at` is exact (A10).
- **`flexMode = full-minus-40`**: a heuristic for the autocompact notice
  wrapping the line; garnish's width is exact. Replaced by verify item 1
  in § 4.9 (the notification row squeeze).
- **NBSP substitution**: they replace spaces with U+00A0 so VS Code's
  terminal does not trim trailing padding. It would break garnish's width
  math if adopted blindly (NBSP is a real cell, but trailing NBSP changes
  copy/paste and the harness's `trim()` treats it as non-whitespace, so a
  spacer row would suddenly render). Verify item 4 in § 4.9.
- **FreeMemory / memory usage**: host telemetry, not session state.
- **npm-style auto-update**: see `install --absolute` above.
- **Binary patching** (N13): CCometixLine's `--patch` and tweakcc rewrite
  `cli.js` for context-low suppression, verbose mode, spinner styles and
  themes; never for garnish.
- **LLM-driven pets and blocking hooks** (claude-code-tamagotchi): network,
  an LLM and a blocking `PreToolUse`; none of the three.
- **Pure-Rust git** (`gix` 0.87.1, used by claude-powerline-rust): would
  replace `git::run_program`, which the crate map rules out; recorded only.

## 15. Implementation lessons and tests

Cheap habits worth copying regardless of which features land.

1. **Settings recovery contract.** Migrate, validate, persist only on
   success; never overwrite a file that could not be parsed; run on
   defaults with a visible badge. garnish's `config check` and the
   `⚠ garnish:` line match this; § 12.1 adds the sentence to `SPEC.md`.
2. **Cache poison horizon.** A lock older than 24 h is treated as
   abandoned even if the pid looks alive. garnish uses `/proc/<pid>`
   liveness and a grace window; a horizon is a one-line safety net
   against pid reuse.
3. **Stale-serve on fetch failure** and **failure cached for a full TTL**:
   garnish already does the latter; the former is what any network worker
   must do.
4. **Situation-keyed cache entries.** garnish's `head`/`upstream`
   validator is the same idea as their token hash; extend it with the
   `.git/index` mtime (B2).
5. **`GIT_OPTIONAL_LOCKS=0`**: already set; keep it.
6. **Widget interface as a checklist.** Their `Widget` interface names
   every capability a widget may opt into (`supportsRawValue`,
   `supportsColors`, `supportsNumberFormat`, `getHideableStates`,
   `preservesRenderedColors`). `ModuleSchema` is the equivalent; A4 and
   A6 add `hide_states` and `number_kinds` to it so docs and a TUI come
   from one place.
7. **Fuzzy picker with initialism matching** (`src/utils/fuzzy.ts`): copy
   into any picker garnish ships.
8. **Config templates as files in the repo** (`configTemplates/`): the
   presets gallery is the same idea and should be what setup offers first.
9. **Publish workflow**: a release job attaching Linux and macOS binaries
   to the `v*` tag so `install` can point at a URL for people without a
   Rust toolchain. Not urgent; note for v0.2.0.
10. **Sanitisation invariant** (ccsidekick): every externally sourced
    string (cwd, branch, session name, agent name, pack text) is stripped
    of C0/C1 and ESC before painting; add a golden with a hostile branch
    name.
11. **Tests.** Their ~170 `bun test` files include renderer tests per
    feature, one test per widget plus shared-behaviour suites that run
    the same assertions over every widget of a family, TUI component
    tests, migration tests per version step, and a schema/registry parity
    test. Worth adding to garnish: a **module-matrix test generated from
    `ModuleSchema`** that renders every module × every hide state ×
    `max_width` × icon set and asserts the invariants (never wider than
    `max_width`, hidden states render nothing, OSC 8 wrappers balanced),
    so a new module gets the shared behaviour for free.

---

# Part III — A companion character (ccsidekick, codachi)

Daniel likes the idea of a sidekick or tamagotchi-style pet in the status
line. Two MIT projects do it well in different ways; this part records how
each is built and then designs a garnish companion that keeps garnish's
invariants (no child process on a warm tick, payload first, clock-driven
animation, a fixed module set) and improves on both. The market confirms
the demand (eight pet projects, § 2.2); the mascot project's nine
hook-driven states and heat-map color shift, and Claude-Code-Personalities'
kaomoji-by-activity, are the closest to the design below [C].

## 16. The two projects

### 16.1 ccsidekick (krayong, MIT, v1.8.0, Bun/TypeScript workspace)

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
  and would double count). The documented `PostToolUseFailure` input
  (`error`, `is_interrupt`, `duration_ms`; Bash `tool_response` with
  `stdout`, `stderr`, `interrupted`, `isImage`) matches what the classifier
  reads [D].
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
  uses (tweakcc patches the binary for the same effect, Tier D).
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

### 16.2 codachi (vincent-k2026, MIT, v0.3.0, zero-dependency TypeScript)

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
  entries, optimistic generation counter). That event name is absent from
  the 33 documented hook events [D]; the garnish design uses
  `PostToolUse` + `PostToolUseFailure`. ~40 categories including
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

### 16.3 What each got right

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

## 17. garnish companion design

Everything below is payload-plus-cache on the tick; the only new process is
the hook, which runs in Claude Code's hook slot, not on the tick.

### 17.1 Modules

- **`pet`** — the figure. `species = "cat" | "penguin" | "owl" | "octopus"
  | "bunny" | "sprig"` (a garnish-original default, so the bundled set has
  no trademark exposure), `size = "auto" | "tiny" | … | "thicc"` (`auto`
  follows the context bucket), `rows = 1 | 3` (a one-row pet `(o w o)~`
  fits today's four-line layouts; three rows use the gutter in § 17.5),
  `name = "Mochi"`, `animate = true` (default off when
  `prefersReducedMotion` is set, N6), `mood_colors = true`.
- **`say`** — the one-line voice, usually alone on its own line or right
  after `pet`; `max_width` (default 66), `tone` filter if a pack declares
  tones, `hide = ["idle"]` to speak only when something happened.
- **`tip`** — the helpful line, `min_severity = "medium"`, `show_for = 300`,
  `cooldown = 600`, `hide_when_empty` default true.
- **`provider`** — badge for bedrock / vertex / foundry / proxy / api from
  the environment (`CLAUDE_CODE_USE_BEDROCK` etc., `ANTHROPIC_BASE_URL`),
  hidden for a subscription; small, payload/env only, and ccsidekick users
  asked for it.

### 17.2 Events: the hook

`garnish hook` (§ 10.1) is registered for `PostToolUse`,
`PostToolUseFailure`, `PreToolUse` (matcher `Skill`) and `UserPromptSubmit`,
async, tagged, silent. It classifies in-process, appends one line to
`<cache>/<session>/events.jsonl` (bounded 200), rewrites the
`events.json` summary (`freshest`, `consecutive_failures`, `edits_60s`,
counts by category, last skill), and exits 0 without output. The tick
reads the summary only.

```rust
/// Written to events.jsonl by `garnish hook`; the tick never parses Bash text.
struct Event { ts: i64, kind: EventKind, stack: Option<Stack>, detail: Option<String> }

enum EventKind { TestPass, TestFail, BuildPass, BuildFail, TypecheckPass, TypecheckFail,
    Lint, Format, Install, GitCommit, GitPush, GitPull, GitMerge, GitRebase, GitBranch,
    GitTag, GitStash, ForcePush, Dangerous, FileEdit(FileClass), FileCreate, FileRead,
    Search, WebFetch, TodoUpdate, AgentSpawn, SkillRun, Docker, K8s, Deploy, DbMigrate,
    ServerStart, Compaction(Trigger), Idle, StopFailure(Reason) }
enum FileClass { Test, Docs, Style, Config, Code }
```

Classification is table-driven (tool-name map, then Bash rules
most-specific first, wrappers `npx bunx sudo time env command` skipped) with
the pass/fail suffix taken from the hook event plus the soft-fail check.
The crate map bans `regex`, so the failure markers are a list of literal
substrings and two tiny hand-written matchers (`[1-9]\d* failed`, `error
TS\d`); `tests/hook.rs` pins them against real tool outputs (cargo, pytest,
go test, jest, tsc). Rapid editing = ≥ 5 `FileEdit` in 60 s, computed by the
hook into the summary. `Compaction`, `Idle` and `StopFailure` rows come
from the `PostCompact`, `Notification(idle_prompt)` and `StopFailure`
entries when those are installed (§ 10.3, § 21.4).

### 17.3 Mood and pressure

```rust
enum Mood { Idle, Sleep, Busy, Happy, Struggling, Recovery }
enum Pressure { CompactHint, BlockLimit, WeeklyLimit }

fn mood(summary: &Summary, now: i64) -> Mood            // ccsidekick's rule, § 16.1
fn pressure(payload: &Payload, cfg: &Config) -> Option<Pressure>
```

Pressure uses garnish's exact autocompact threshold (`context ≥ window −
compact_buffer_tokens − margin` → `CompactHint`), then `limit5h`/`limit7d`
by the pace band (§ 8.5) with the 80 % floor. `Sleep` = idle and context <
10 % (codachi). Freshness (`hot < 15 s`, `warm < 60 s`, `cold < 5 min`)
weights the message choice, not the mood.

### 17.4 Frames, animation and packs

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
pools of § 17.6. Pure data, validated by `config check`, never executed;
mood is color-only for static art (ccsidekick's rule), which keeps a
sourced figure from strobing. The mascot project's pack search order
(project `.claude/mascot-packs/`, user plugin dir, bundled), validator and
storybook CLI are the same shape [C].

### 17.5 Gutter layout (three-row pet)

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
without a second element). The mascot documents the failure mode a gutter
must avoid: before 2.1.141 one over-wide row dropped every row below it;
garnish's exact-width frame already prevents this.

### 17.6 Voice

Pools per slot, tier-nested where ccsidekick nests them: `first_contact`,
`greeting.{morning,day,evening,night,weekend}`, `mood.<mood>`,
`event.<reaction>` (the 18-cell reaction set: three fail kinds, lint,
format, install, git, file_edit, search, web_fetch, todo_update,
agent_spawn, skill_run, docker, k8s, deploy, db_migrate, dangerous),
`pressure.<kind>`, `milestone.{tier_up,comeback,streak,anniversary}`,
`positive_git.{clean_tree,op_cleared,branch_created,tag_pushed}`, `egg`,
`date_egg`, `stack.<stack>.{slow,fail}`, `file.<class>` (codachi's
file-type lines). Selection is ccsidekick's chain (§ 16.1) with codachi's
freshness gating on the event slot. Deterministic pick: `fnv1a(seed) mod
len` where the seed is `(slot, tier, mood, bucket, session_id, 10-second
tick)`; a ten-line FNV-1a in `num.rs` avoids a hash crate. Templates may
name the file or branch (`{file}`, `{branch}`), sanitised first.

The bundled voice lives in `packs/sprig.toml` and is loaded with
`include_str!`; a unit test lints it the way `pack:lint` does (pool counts,
≤ 66 columns, no near-duplicates by token-set Jaccard ≥ 0.8, no control
characters). Further packs are user data under the config dir.

### 17.7 Memory, tiers, stats

`<cache>/companion/memory.json` (`schema_version`, `first_met`, `sessions`,
`uptime_s`, `last_seen`, `projects_seen`, `streak_days`, `last_day`) is
updated when a new `session_id` is first seen. Two ways to write it: the
tick writes it once per session (a tick-side write, § 0), or the heartbeat
worker of § 21.3 writes it on its first run for the session, which needs no
exception. Tiers at 3 / 15 / 50 / 100 sessions; milestone and pressure
lines latch once per session in `<cache>/<session>/state.json`, merged as
sets so overlapping ticks never drop a latch. `garnish stats` prints the
dashboard: first met, sessions, uptime, tier progress bar, streak, last-24 h
events from `events.jsonl` (tests pass/fail, commits, edits), this
session's duration; and, since the harness already keeps `dailyActivity`,
`dailyModelTokens`, `totalSessions` and `hourCounts` in
`~/.claude/stats-cache.json` [D, L], the activity heatmap and per-model
bars can read the harness's own aggregates instead of a parallel ledger.
No network, no telemetry.

### 17.8 Tips

Port ccsidekick's catalog as a `&[Tip]` table: `id`, `severity`,
`momentary`, `test: fn(&Derived) -> Option<String>`. Inputs garnish already
has: payload (context, limits, effort, `pr`), the `branch` worker (dirty
flags, upstream, ahead/behind, untracked paths, in-progress operation
from `.git/*` file existence, stash count, tag), events (dangerous, force
push, compaction count from N4), settings (API key vs subscription from
`rate_limits` presence). Secret detection matches untracked names against
`.env*`, `*.pem`, `*.key`, `id_rsa*`, `*credentials*`, `*.p12`. Kube and
terraform prod contexts come from `~/.kube/config` `current-context` and
`.terraform/environment` files, read on the settings cache cadence. Show
window and cooldown are latched in `state.json`. N7: `garnish tips export`
writes the catalog as tip objects with ids and cooldowns so the user can
point `spinnerTipsOverride.tipsFile` at it and the harness rotates them in
its own spinner [D]; a complement to the `tip` module, not a replacement.

### 17.9 Spinner verbs

`install --spinner-verbs` writes `spinnerVerbs` from the active pack
(≥ 25 verbs) into Claude settings, preserving key order; `install
--no-spinner-verbs`/uninstall removes it. The settings schema has
`spinnerVerbs: {mode: "append" | "replace", verbs: string[]}` (append adds
to the ~200 built-in verbs, replace uses only yours; replace with an empty
list keeps the built-ins) and the sibling `spinnerTipsOverride` (§ 4.5)
[D, B]; the spinner store reads them live. `mode = "append"` is the polite
default.

### 17.10 Small ports from these two (Tier A unless noted)

- `limit5h`/`limit7d`: `pace`, `pace_colors`, `eta` (§ 8.5).
- `branch`: in-progress operation glyph, `tag`, submodule branches
  optional (§ 9.1).
- `context`: `eta = true` prints `~15m` from a velocity ring; codachi
  samples on the tick. In garnish the ring can live in the heartbeat
  worker's samples (§ 21.3) at 30 s resolution, or on the tick at 1 s
  resolution as a tick-side write (§ 0).
- `todo` module (in-progress task name) — C1, transcript.
- `cost`: `burn = true` (`$/h` over the 5-hour window) — needs the
  transcript (C1) or a per-session cost ring from the heartbeat worker.
- `provider` module — Tier A.
- `preview --live` (codachi's `demo`): loop a fixture through the renderer
  on a TTY with the clock running, for screenshots and for checking
  animation without Claude Code.

---

# Part IV — garlic: session time and attention

Daniel's question, from PR #40 and the review request: garnish runs once a
second in every open Claude Code session with that session's numbers on
stdin. Can that tick track how long the user has actually been working,
notice which session has the user's attention, hand garlic a better signal
than its hook-based estimate, and show garlic's day on the status line?
And, looking at garlic's own source, how should garlic itself improve?

The short answer, after reading both codebases and the harness: **yes to
the display, the liveness and the facets; no to moving the engine.** The
tick cannot see focus (nothing can, § 19), and it cannot improve on the
hook timestamps for the boundaries garlic already sees. What it adds is a
live figure garlic cannot show today (its total only moves when a cursor
closes), a liveness signal that closes the cursors of dead sessions, a
suspend detector, the pre-first-prompt interval for every start source,
per-request API-time attribution, and a deterministic delivery channel for
the nudge that does not depend on the model relaying it. All of that fits
the existing worker machinery without the tick-side write PR #40 asked
for. Three independent audits of garlic (time model, state and sync,
interfaces) and one adversarial verification of PR #40 fed this part;
every garlic claim below is cited to v0.3.5 source lines [S].

## 18. garlic today

### 18.1 Shape

garlic (`garlic-ward` on crates.io, v0.3.5, MIT) is a single Rust binary
that is also a library crate (`src/lib.rs:3-16`); dependencies `clap`,
`serde`, `serde_json`, `toml`, `chrono`, `rand`, `ureq`, `tempfile`,
`dirs`. It installs four Claude Code hooks (`src/setup.rs:15-20`; the
plugin ships the identical four in `plugins/garlic/hooks/hooks.json:3-35`):

| event | matcher | command |
|---|---|---|
| `SessionStart` | `startup` | `garlic hook session-start` |
| `UserPromptSubmit` | `""` | `garlic hook prompt` |
| `Stop` | `""` | `garlic hook stop` |
| `SessionEnd` | `""` | `garlic hook session-end` |

Each hook reads the harness JSON from stdin and uses **only `session_id`**,
falling back to `"default"` when it is missing, empty or unparsable
(`src/cli.rs:193-197, 221-232`); the event name comes from the
subcommand, and the timestamp is `SystemTime::now()` at hook run time
(`src/cli.rs:96-102`), since the hook JSON carries none (§ 4.4).

### 18.2 The time model

Per session, one cursor at a time (`src/engine.rs:23-30`: `open_cursor`
replaces any existing cursor for the session):

- `SessionStart` opens a **User** cursor (`engine.rs:138-140`).
- `UserPromptSubmit` closes whatever cursor is open, by its stored kind,
  and opens an **Agent** cursor (`engine.rs:103-111`).
- `Stop` closes the cursor and opens a **User** cursor (`engine.rs:124-127`).
- `SessionEnd` closes and removes the cursor (`engine.rs:146-154`).

Closing rules (`engine.rs:37-57`): a User cursor longer than
`max_prompt_gap_minutes` (default 40, `src/config.rs:35-37`) is **dropped
entirely**; an Agent cursor is **clamped** to `max_generation_minutes`
(default 120, `config.rs:38-40`). Closed intervals of the same session and
kind that abut within 1 s are coalesced (`src/intervals.rs:55-77`), so the
list stays O(turns). The daily total is the **union** (a sweep-line) of
all closed intervals across sessions (`intervals.rs:86-137`); `agent` and
`user` are independent lenses of the same sweep and `overlap` is
wall-clock time with two or more sessions active, never folded into the
total (`intervals.rs:40-53`). The total is recomputed **only when a cursor
closes** (`engine.rs:92-97`, called from the three handlers); open cursors
contribute nothing until then.

Thresholds (default `[30, 60, 90, 120, 150, 180, 210, 240]`,
`config.rs:44-46`) are checked **only in `handle_prompt`**
(`engine.rs:110-119, 173-187`): every threshold the total has crossed is
marked given at once and the highest is nudged. The bedtime check is the
hour before `reset_hour` (`engine.rs:193-207`; `bedtime_hour = (reset_hour
− 1).rem_euclid(24)`), once per night, also on the prompt hook only. The
nudge is delivered as `UserPromptSubmit` `additionalContext` inside a fixed
relay frame (`src/hooks.rs:58-79`: "[garlic] Break reminder for the user.
Relay the message below to the user at your next natural opportunity …");
message pools are `const` arrays with a `{time}` placeholder
(`src/nudges.rs:7-85`); only `nudge_pending = true` is persisted, not the
text or the time (`hooks.rs:204-211`).

### 18.3 State and config

State lives at `$GARLIC_DIR` or `~/.garlic/` (`src/paths.rs:17-46`) as
`state.toml`, `config.toml` and `version_cache.toml`; **per user**, not
per machine. `state.toml` fields (`src/state.rs:20-52`,
`intervals.rs:13-38`), every one `#[serde(default)]`: `date`,
`accumulated_minutes` (f64), `last_event_time`, `nudges_given: Vec<i64>`,
`ignored`, `bedtime_nudge_given`, `history` (≤ 30 `{date, minutes}`,
`HISTORY_MAX` at `state.rs:12`), `intervals` (`{session_id, kind:
"agent"|"user", start, end}` in Unix seconds as f64), `open`
(`{session_id, kind, start}`), `reset_pending`, `nudge_pending`.
`config.toml` has five flat keys (`config.rs:13-19, 35-49`):
`max_prompt_gap_minutes = 40`, `max_generation_minutes = 120`, `reset_hour
= 2`, `nudge_thresholds_minutes = [...]`, `nudge_style = "gentle"` (valid:
`gentle`, `firm`, `spicy`; unknown falls back to gentle,
`nudges.rs:88-94`).

Persistence (`state.rs:99-158`): `load_state` does `create_dir_all`, opens
the file, takes a **shared** `flock`, reads, unlocks, parses; a parse
failure returns a fresh state for today (`state.rs:107-110`); a date
change archives the old total to `history` only if `accumulated_minutes >
0` and returns a fresh state with **no intervals and no open cursors**
(`state.rs:112-128`); "today" is the local wall clock shifted back a day
before `reset_hour` (`state.rs:74-86`). `save_state` opens a second
handle, takes an **exclusive** `flock`, `set_len(0)`, writes, flushes; no
temp file, no rename, no fsync (`state.rs:143-158`). `load_config`
overwrites a missing *or unparsable* `config.toml` with defaults
(`config.rs:100-121`). The only concurrency test asserts that the file is
valid TOML after concurrent writes, not that updates survive
(`state.rs:385-406`).

### 18.4 Outputs

- `garlic status --json` emits nine keys in this order
  (`src/commands.rs:238-248`): `accumulated_minutes`, `agent_minutes`,
  `user_minutes`, `overlap_minutes`, `thresholds` (sorted),
  `nudges_given`, `next_threshold` (the first sorted threshold *not in
  `nudges_given`*, not the first above the total, `commands.rs:232-235`),
  `ignored`, `date` (reset-hour shifted). Not exposed: `open`,
  `intervals`, `nudge_pending`, `reset_hour`, the caps, any per-session
  view, any schema version. With `GARLIC_URL`/`GARLIC_TOKEN` set, `status`
  first flushes to the backend (up to 5 s, `src/remote.rs:22`) and may
  write state to clear `reset_pending` (`commands.rs:206-220`,
  `src/sync.rs:37-41`); without one it still creates `~/.garlic/` and
  rewrites a bad `config.toml`. The README promises `status --json` "for
  scripting and statusline integrations" (`README.md:160-161`).
- `garlic statusline` (`commands.rs:345-398`) reads no stdin, prints one
  line `<icon> <time> / <max> [· agent X · user Y] [⇄] [(paused)]` with
  the garlic glyph (U+1F9C4) while `nudge_pending` and the vampire
  (U+1F9DB) otherwise, always exits 0, and **writes `state.toml` to clear
  `nudge_pending`** after rendering (a re-read narrows the race window
  without closing it, `commands.rs:349-360`).
- `garlic version` performs a crates.io request (3 s timeout, cached
  24 h) and may prompt on a Homebrew install (`commands.rs:73-77, 123`;
  `src/version.rs:9, 38-81`); `garlic --version` is clap's, side-effect
  free. The plugin manifest says version `0.1.0` while the crate is
  `0.3.5` (`plugins/garlic/.claude-plugin/plugin.json:3`, `Cargo.toml:3`).

### 18.5 Backend and sync

Opt-in via `GARLIC_URL` + `GARLIC_TOKEN` (`remote.rs:46-68`), bearer auth,
namespace = SHA-256 of the token; endpoints `GET /health`, `GET
/version`, `GET /v1/state`, `POST /v1/events/{session-start,prompt,stop,
session-end}`, `POST /v1/intervals`, `POST /v1/ignore`, `POST /v1/reset`,
each returning `{state, crossed_threshold, bedtime}` (`backend/README.md`).
The client pushes `state.intervals` and never `open` (`sync.rs:42`,
`remote.rs:106-117`); the backend replaces stored intervals whose
`session_id` appears in the push, keeps the rest, and recomputes the union
(`backend/src/engine.rs:351-367`) under a per-namespace Redis lock (`SET
NX PX 10000`, retried ~5 s, `backend/src/store.rs:21-23, 61-74`). Default
hooks never touch the network; `GARLIC_SYNC=blocking|sync|1` makes them
flush inline (`sync.rs:51-56`, `hooks.rs:104-119`). The server computes
the day boundary in its own zone (`backend/src/engine.rs:199-205`), the
client in local time (`state.rs:74-76`); `week`/`month` views are local
and text only (`commands.rs:186-199`).

### 18.6 Installer and plugin

`garlic setup` writes the hooks into `~/.claude/settings.json`: it detects
its own entries by the command string starting with `garlic hook`
(`setup.rs:31-48`), deletes those *entries* (the whole `{matcher, hooks}`
object) and re-adds its own (`setup.rs:95-99`). It parses the file with
`serde_json::from_str(&text).unwrap_or_else(|_| json!({}))`
(`setup.rs:71-79`) and then writes the result atomically
(`setup.rs:50-62, 103`): **an unparsable `settings.json` is silently
replaced by `{"hooks": …}`**, dropping `statusLine`, `permissions`,
`env` and everything else. The README warns that enabling the plugin and
running `garlic setup` double-counts every event (`README.md:123-127`);
the code does not de-duplicate. The `/garlic` slash command runs `garlic
$ARGUMENTS` with `allowed-tools: Bash(garlic:*)`.

## 19. Attention: what the harness knows, and what reaches a tool

§ 4.6 has the internals. The consequences for time tracking:

1. **Focus and interaction never leave the harness.** No payload field,
   no hook event. The presence rule exists and is one function, so the
   upstream ask (G4) is small: a `presence: {terminal_focus,
   last_interaction_ms}` object in the status line payload. Worth an
   issue on `anthropics/claude-code`; until then "focus" means
   "activity" in both tools.
2. **Querying the OS for the active window** is rejected, as PR #40
   rejected it: it differs between X11, every Wayland compositor and
   macOS, needs a new dependency or a child process on the warm tick, and
   still has to map a window to a pid to an ancestor of the status line
   process.
3. **`idle_prompt` is a hook-visible presence signal** (§ 4.6): fires
   about 60 s after Claude finishes if the user has not typed since,
   whether or not the terminal is focused (verify item 6 confirms it on
   screen). It means "no keystroke for 60 s", which includes reading a
   long answer, so it can bound a user interval, never close it.
4. **Liveness is free.** The session registry (§ 4.8) gives a pid per
   session and `/proc/<pid>` answers "alive"; garnish's worker already
   uses that check for its own locks. A heartbeat adds *what the session
   was doing*, not whether it exists.
5. **The tick has no focus gate** (§ 4.2), so a session left open in a
   background window keeps ticking; a heartbeat therefore proves the TUI
   is open, not that anyone is looking at it. Verify item 5 confirms it
   on screen.
6. **"The in-focus session" as a cross-session view** is therefore
   defined as activity ordering: the session whose `prompt_id` changed
   most recently is the one the user last spoke to; among the others, the
   one whose API ledger stepped most recently is the one being waited on.
   It cannot tell "reading session B's answer" from "went for coffee";
   nothing without focus can.

## 20. Review of PR #40 (`GARLIC-INTEGRATION.md`)

The proposal's structure is sound and its recommendation order (a display
module first, the sensor second, engine ownership only if garlic is
retired) survives. Its facts about the harness were mostly right; its
facts about garlic and about the cost ledger need correction, and it
missed four signals. Verdicts:

| PR #40 claim | verdict | note |
|---|---|---|
| § 1 hooks `SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd` | corrected | `SessionStart` matcher is `startup` only; resume/clear/compact/fork never reach garlic (§ 18.1) |
| § 1 "hook JSON: `session_id`, event, timestamp" | corrected | no timestamp in hook JSON; garlic stamps its own run time (§ 4.4, § 18.1) |
| § 1 `~/.garlic/state.toml` under `flock`, "one file for the whole machine" | corrected | per user (`$HOME` or `$GARLIC_DIR`); locking confirmed but not held across read-modify-write (§ 22.1) |
| § 1 state contents; Prompt→Stop agent capped, Stop→Prompt user dropped, union across sessions | confirmed | `engine.rs:37-57, 92-97` |
| § 1 outputs (`additionalContext`, `status`, `statusline`, backend) | confirmed | |
| § 1 garlic covers `claude -p` and SDK sessions; garnish only TUI sessions | half | hooks run in `-p` unless `--bare` [D]; whether the status line runs in `-p` is not documented and was not traced: plausible, not verified |
| § 1 a killed session's cursor is lost at rollover, not inflated | confirmed | `state.rs:112-128` drops `open` |
| § 1 the cap "protects only against a hung generation that does eventually stop" | corrected | the cap applies at every close (next Stop, next prompt, SessionEnd); still per session, never at rollover |
| § 1 Prompt→Stop counts tool runs and permission waits as agent time | confirmed | no tool or permission hooks registered |
| § 3.2 `total_api_duration_ms` "grows only while a request is in flight", so its delta is model time | **corrected** | it is a ledger stepped once per completed request (§ 4.4); the delta attributes API time per request after the fact and cannot show "generating now" |
| § 4 focus tracking, presence rule, push-only consumer, no payload field, `remote.session_id` | confirmed, with detail | § 4.6 adds the input-inferred focus, the extra consumers of raw focus state, and the `?1004` mode table |
| § 4 hook list (19 events) | corrected | 33 events in both the docs and the binary; `PostToolUseFailure`, `PostToolBatch`, `PermissionDenied`, `PostCompact`, `PreModelSwitch`, `PostModelSwitch`, `MessageDisplay`, `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `DirectoryAdded`, `FileChanged`, `Elicitation`, `ElicitationResult` were missing; "no focus event" holds |
| § 5 timer `max(1, refreshInterval) × 1000`, no focus gate, eight triggers | confirmed, extended | a new assistant message id also triggers; `command` change bypasses the debounce; scheduled re-renders fire 1 s after the instant; an invalid `refreshInterval` drops the timer (§ 4.2) |
| § 6.2 garlic config keys `thresholds`, `style`, `max_prompt_gap_minutes`, `reset_hour`; bedtime a key | corrected | five keys; `max_generation_minutes` omitted; bedtime is derived (§ 18.2) |
| § 6.3 worker stores total, highest threshold, split, `ignored`, `nudge_pending` from `status --json` | corrected | no `nudge_pending`, no highest-threshold field; `nudge_pending` is observable only through `statusline`, which clears it as a side effect; `status` itself has side effects (§ 18.4) |
| § 6.3 `status --json` is the promised integration surface | confirmed | `README.md:160-161`; its numbers exclude open cursors, so a 30 s worker shows a figure that jumps at each event (§ 21.2) |
| § 6.1 a 30 s heartbeat needs one exception to "the tick never writes" | **not needed** | the worker machinery already does it (§ 21.3) |
| § 7 contract files, § 8 security | kept | § 21.6, § 23 |

Missed signals: `Notification(idle_prompt)` as a hook-exposed presence
cap (§ 19 item 3); registry liveness (§ 19 item 4); the open cursors persisted in
`state.toml` that garnish can project to a live figure (§ 21.2); and the
harness facts that break garlic's model regardless of garnish (`Stop` not
firing on interrupt, `UserPromptSubmit.source`, `-p` sessions, suspend;
§ 22).

Disposition: this Part supersedes the PR's document. The PR is Daniel's
to close or merge; nothing here edits its branch.

## 21. Integration design

### 21.1 Ownership (G6)

garlic stays the engine (intervals, union, caps, thresholds, bedtime,
`ignore`, `reset`, history, week/month, backend) and the relay (its
`UserPromptSubmit` hook is the only path into Claude's context). garnish
becomes the **sensor and the display**: it reads garlic's state, writes
per-session activity garlic can consume, and shows the day. PR #40's 6.2
(garnish owns the tracking) duplicates garlic's engine and config, orphans
`status --week/--month`, `sync` and the backend, keeps garlic's hook
anyway, and puts a machine-wide locked state file on the render path,
which is exactly what `SPEC.md` § 6 designed the cache to avoid. It makes
sense only if garlic's engine is being retired; it is not recommended.

### 21.2 G1. The `garlic` module

**What it shows.** `⏳ 2h10m / 4h` colored by the band the other
percentage modules use against the next threshold; the garlic glyph for
N seconds after a nudge; `(paused)` when ignored; in the `full` preset the
agent/user split, `⇄` when another session overlaps, the bedtime window,
`resets in 1h12m`, and this session's own lane (`this: agent 12m · user
4m · generating 0:42`). Nothing renders when garlic is not installed.

**Where the numbers come from.** Two options were weighed:

| | A. cached file read of `state.toml` + `config.toml` (recommended) | B. worker runs `garlic status --json` (PR #40's 6.3) |
|---|---|---|
| liveness of the figure | open cursors are projected to `now` on every tick, so the counter ticks | moves only when a cursor closes; up to `max_generation_minutes` stale (`engine.rs:92-97`) |
| side effects | none; read-only | `status` flushes to a backend (≤ 5 s), rewrites a bad `config.toml`, may write state (§ 18.4) |
| processes | none (the refresh worker is garnish itself) | one `garlic` process per refresh |
| coupling | to the file format, five config keys and the cursor rules; both repositories are Daniel's; add a `version` key to `state.toml` (§ 22) | to the JSON, which today lacks `open`, the caps, `reset_hour`, `nudge_pending` and a schema version |
| per-session lane | yes (`session_id` on every interval and cursor) | no |
| nudge flash | reads `nudge_pending` without clearing it; garlic's own flash protocol is unchanged | consuming the flag through `statusline` would steal garlic's flash |

A is recommended. B becomes viable once garlic exposes `status --json
--local` with `open`, the config, `last_nudge` and a schema version
(§ 22 item 17), at the cost of a process per refresh; the module's schema does
not change between them.

**Shape.** A cached module with `refresh = 10` (seconds). The worker,
`garnish refresh --module garlic`, is the ordinary detached worker
(`SPEC.md` § 6): it opens `state.toml` with a **non-blocking shared
`flock`** (garlic's writer holds the exclusive lock for microseconds; on
`EWOULDBLOCK` or a zero-length file it keeps the previous entry, since
`save_state` truncates in place and a reader can see the empty window),
parses it and `config.toml` (applying garlic's defaults and clamping the
same ranges, because garlic's loader validates nothing, `config.rs:
112-120`), and stores in the cache entry: `date`, `closed_total`,
`agent`/`user`/`overlap`, the `open` cursors, the closed intervals that
overlap `[min(open.start), now]` (the "tail", usually a handful),
`thresholds`, `nudges_given`, `ignored`, `nudge_pending`, the five config
values, `state_mtime`, and the resolved reset instant. The entry's
validator is `state_mtime`, so a garlic write between ticks is a miss and
the worker re-reads early; otherwise every 10 s.

On the tick, pure arithmetic: each open cursor becomes a projected
interval `[start, now]` under garlic's own closing rules (User: counts
only while `now − start ≤ max_prompt_gap_minutes`, else 0; Agent:
`min(now − start, max_generation_minutes)`), the projections are unioned
with the tail by the same sweep-line, and the display total is
`closed_total + (union(tail ∪ projected) − union(tail))`. The projection
converges exactly to what garlic will record at the next close, so the
figure never goes backwards except when a user gap is dropped, which is
garlic's own behaviour. Only cursors whose session is alive (registry pid,
§ 4.8, or a fresh heartbeat, § 21.3) are projected, so a leaked cursor
from a dead session does not inflate the display. Threshold coloring uses
the projected total against `thresholds`, and shows "crossed 60m, nudge
on next prompt" when the projected total is past a threshold garlic has
not marked, which garlic cannot show (`engine.rs:110-119`). Day bucketing
uses garlic's rule (local time shifted by `reset_hour`), never UTC, so the
projected total resets at the same instant garlic's does.

Cost: the tick reads one cache entry (as every cached module does) and
runs a sweep over a few intervals; the worker parses two small TOML files
every 10 s. No `garlic` process, ever, on the tick or in the worker.
Spec impact: one more id in the fixed set; the first module whose worker
reads another tool's files, which `SPEC.md` § 6 should say explicitly.

### 21.3 G2. Heartbeat and facets through the worker machinery

PR #40 asked whether a 30 s heartbeat write from the render tick is an
acceptable exception to "the tick never writes". It is not needed. The
`garlic` module's worker already runs every 10 s and on every change the
validator notices; make the **payload sample part of the worker's
arguments**, as the situation (`head`, `upstream`) already is for the git
workers, and have the worker append it:

- The tick passes `session_id`, `prompt_id`, `total_api_duration_ms`,
  `total_duration_ms`, the token counts and `cwd` to `refresh --module
  garlic` when it spawns it. The entry's validator also includes
  `prompt_id`, so a prompt boundary makes the entry a miss on the tick
  that sees it and the worker runs at once: boundaries are recorded at
  one-second resolution. The API ledger is *not* in the validator: it
  steps once per completed request (§ 4.4) and is after-the-fact anyway,
  so sampling it at the TTL loses nothing and avoids a worker spawn per
  request in a tool-heavy turn. Heartbeats run at the TTL when nothing
  changes.
- The activity file is written only while the `garlic` module is
  configured on some line (the needs-based rule of § 6.5). If garlic
  should get liveness from sessions that do not display it, a
  `[garlic] heartbeat = true` switch runs the same worker with the module
  hidden; say which in the § 0 decision for G2.
- The worker appends one line to
  `<cache>/sessions/<session_id>/activity` (§ 21.6), creates it `0600`,
  writes temp-and-rename when it compacts the file, and never touches
  `state.toml`.
- The same worker can carry every other "tick-side write" the earlier
  documents wanted: the companion memory's first-seen record (§ 17.7),
  the per-day cost ledger (N3), the context-ETA ring (§ 17.10) at 10 s
  resolution, and the usage snapshot (N9). One worker, one file class,
  no exception to `SPEC.md` § 6.

What garlic (or `garnish activity`) derives from the activity files:

1. **Liveness.** A session whose file's mtime is older than `2 × TTL +
   5 s` is gone, whether or not `SessionEnd` fired; garlic can close its
   cursor at the last sample time instead of losing the span at rollover
   (§ 22 item 15). The registry pid is the cheaper first check; the heartbeat
   adds the last-activity time.
2. **Suspend.** A gap between consecutive samples far larger than the TTL
   is a sleep window; garlic counts wall-clock sleep as agent time up to
   the cap today (§ 22 item 12), and `total_duration_ms` is wall clock too
   (§ 4.4), so tick continuity is the only suspend detector available.
3. **The pre-first-prompt interval** for every start source: the first
   sample has no `prompt_id` (absent until the first input [D]), so
   "first tick → first `prompt_id`" is the user interval garlic's
   `startup`-only matcher misses on resume/clear/fork (§ 22 item 10).
4. **API-time attribution.** Each step of the ledger is one completed
   request's duration (§ 4.4). Summed per turn and set against the turn's
   wall-clock span, it splits a Prompt→Stop interval into "model time"
   and "tool runs, permission waits and idle", a new facet for `garlic
   status` and for the module's full preset. It is after-the-fact, per
   request; it does not say "generating now".
5. **Interrupted turns.** A `prompt_id` that never sees a further ledger
   step, followed by a long flat stretch, is the signature of an
   interrupt or a `StopFailure`, which garlic books as agent time up to
   the cap (§ 22 item 6). garlic can cap such a cursor at the last ledger step
   instead.
6. **Token facets** per session and per day, which garlic has no window
   on.

What the heartbeat does *not* do: it does not replace garlic's hook
timestamps for the prompt and stop boundaries (a 10 s worker sees them
one tick late at best), it does not see focus, and it does not run in
sessions without a status line (`claude -p` with `--bare`, SDK sessions).
garnish cannot be the only sensor.

### 21.4 G3. The `idle_prompt` hook

`garnish install --hooks` (or the plugin) adds `{"Notification":
[{"matcher": "idle_prompt", "hooks": [{"type": "command", "command":
"garnish hook", "async": true}]}]}`. The handler appends an `Idle{ts}` row
to the session event log (§ 10.1) and the activity file. Semantics: at
`ts`, the user had not typed for about 60 s after Claude finished
(§ 4.6). Uses: the companion's `sleep` mood; the `garlic` module's
per-session lane ("idle 3m"); and for garlic, a bound on the current User
cursor: a user gap that contains an `Idle` row started at least 60 s
before it, so the "cliff" rule (§ 22 item 13) has a better anchor than the next
prompt. It is never proof of absence.

### 21.5 The cross-session view

`garnish activity --json` (a subcommand, never the tick) reads the
registry and the activity files of every live session and prints them
(id, name from the registry, last sample age, prompt count, API minutes,
idle since). The `garlic` module's worker does the same read to fill the
"N live · this one active 2m ago" segment and to decide which cursors to
project. Real focus needs G4.

### 21.6 Contracts

Line-oriented text, versioned on the first line, written as
`<file>.tmp.<pid>` then renamed, read with the rule "a malformed line is
skipped, an unknown version is ignored".

**Activity file** (garnish writes, garlic reads):
`<garnish cache root>/sessions/<session_id>/activity`, in the directory
garnish already sanitises and garbage-collects after 24 h idle. The cache
root resolves as `SPEC.md` § 6 says (`GARNISH_CACHE_DIR`, then
`$XDG_RUNTIME_DIR/garnish`, and so on); `garnish doctor` prints it and
`garnish activity --path` is the scriptable form.

```
v1 <session_id> <first_seen_ms> <refresh_s> <cwd>
<ts_ms> <prompt_id or -> <api_ms> <duration_ms> <input_tokens> <output_tokens> [I]
```

Line 1 identifies the file and states the worker cadence so a reader can
compute liveness without guessing. Samples are newest last, appended on a
change of `prompt_id` or `api_ms`, else every `refresh_s`; a trailing `I`
marks an `idle_prompt` row. The file is rewritten to its last 200 samples
when it passes 1000 lines. The mtime is the heartbeat. `cwd` is the only
string besides ids and is written plain (`SPEC.md` § 5 sanitising applies
to everything garnish writes).

**garlic's `state.toml`** (garlic writes, garnish reads): the format in
§ 18.3 plus a `version = 1` key (§ 22 item 19); garnish keeps the last good
parse on any read failure and renders `n/a`, never `0m`, when it has none.

**No nudge signal file.** PR #40's 7.3 existed for its 6.2 shape; with
garlic keeping the engine it is dropped. The nudge reaches the status line
by reading garlic's state (`nudge_pending` today, `last_nudge` after
§ 22 item 5), and reaches Claude by garlic's own relay.

## 22. Improving garlic

Ordered by how much they change what garlic records, each cited to
v0.3.5 [S]. The first two are recommended regardless of anything garnish
does. "garnish" notes say what garnish assumes meanwhile.

### 22.1 Persistence (fix first)

1. **Hold one exclusive lock across the read-modify-write.** Every hook
   does `load_state → mutate → save_state` with the shared lock released
   after the read and the exclusive lock taken only for the write
   (`state.rs:134-140, 143-158`; `hooks.rs:169-176, 224-226, 241-243`).
   Two sessions' hooks a few milliseconds apart (session A's `Stop`,
   session B's prompt, which is the parallel-agent case garlic exists
   for; or the same event twice when both the plugin and `setup` hooks
   are installed) interleave as read/read/write/write and the second
   write silently discards the first's interval or cursor. The harness
   runs matching hooks in parallel (§ 4.4). The lock only prevents torn
   files, which is all `concurrent_saves_dont_corrupt` tests
   (`state.rs:385-406`). Fix: open once, `flock(LOCK_EX)`, read, mutate,
   write, unlock; keep the lock scope inside one function so no caller
   can get it wrong. *garnish: never writes `state.toml`, so it cannot
   join the race; its per-session activity file has no shared writer.*
2. **Write temp-and-rename.** `save_state` does `set_len(0)` then
   `write_all` (`state.rs:151-155`); a kill between the two (the harness
   cancels `SessionEnd` hooks at 1.5 s, and cancels a running
   `statusline` on any new trigger) leaves an empty or partial file, which
   parses as `State::default()` with an empty `date` and is treated as a
   first run: today's intervals, `nudges_given` and the whole 30-day
   `history` are gone without a message (`state.rs:107-128`). garlic
   already has the pattern for `settings.json` (`setup.rs:50-62`,
   `NamedTempFile` + `persist`). *garnish: keeps the last good parse and
   treats an empty file as unknown.*
3. **Never overwrite what could not be parsed; keep a backup.** A corrupt
   `state.toml` resets the day (`state.rs:107-110`); a corrupt
   `config.toml` is overwritten with defaults (`config.rs:112-120`); the
   backend does the same (`backend/src/store.rs:140-145`). Rename the bad
   file to `.corrupt-<ts>` and say so on stderr.
4. **`garlic setup` must refuse an unparsable `settings.json`**
   (`setup.rs:71-79`) instead of replacing it with `{}`, and back the
   file up before writing. This is the one finding that damages the user
   outside garlic: it removes the `statusLine` that points Claude Code at
   garnish. *garnish `doctor` warns when `statusLine` is missing after a
   `garlic setup` and tells users to run `garlic setup` before configuring
   the status line.*
5. **`garlic statusline` must not write.** It clears `nudge_pending` on
   the render path (`commands.rs:345-362`), which makes a status line
   command a state writer and couples the one-shot flash to whoever
   renders. Replace the bool with `last_nudge = {at, threshold, style}`
   persisted by the prompt hook (`hooks.rs:204-211`); any renderer then
   flashes for N seconds from its own clock with no write.

### 22.2 The time model against the harness as it is

6. **`Stop` does not fire on a user interrupt; API errors fire
   `StopFailure`** [D]. garlic registers only `Stop`, so after an Esc the
   Agent cursor stays open and the next prompt closes it as agent time up
   to the 120-minute cap (`engine.rs:37-57, 110`): prompt at t₀, Esc at
   t₀+2 m, three hours away, next prompt books 120 minutes of agent time
   while nothing ran; the same break after a clean `Stop` is dropped
   entirely by the gap rule. Register `StopFailure` (close the cursor as
   agent time at the failure) and, for interrupts, either accept the
   heartbeat's last-ledger-step time (§ 21.3) or close an Agent cursor
   met by a *prompt* at the lesser of the cap and the gap rule. No test
   covers the interrupt case.
7. **Honour `UserPromptSubmit.source`** (§ 4.4). A `/loop` or scheduled
   wakeup, an auto-continuation or a channel message fires `garlic hook
   prompt` like a human; the User cursor closes as thinking time while
   the user may be asleep and the day's total grows for as long as the
   loop runs (`cli.rs:223-232` reads only `session_id`). Treat
   `loop_wakeup`, `schedule_wakeup`, `system` and `sdk` prompts as
   machine turns: close the Agent cursor if one is open, open no User
   cursor, and make the set configurable. The field is optional while it
   rolls out, so absent means `user`.
8. **`claude -p` sessions run the same hooks unless `--bare`** [D], so a
   CI wrapper or a cron that runs Claude goes through the full cycle and
   can receive a nudge in headless output (`hooks.rs:70-79`). Item 7's
   `sdk` rule covers it.
9. **Stop-hook continuation.** With any blocking `Stop` hook in the
   user's config, `Stop` fires twice with no prompt between
   (`stop_hook_active` [D]); the first opens a User cursor and the second
   closes it as user time or drops it (`engine.rs:124-127`), so agent
   continuation is mis-kinded. Read `stop_hook_active` and keep the Agent
   cursor open when it is true.
10. **Read `source` on `SessionStart` instead of widening the matcher.**
    The `startup`-only matcher (`setup.rs:16`) means resume, clear and
    fork sessions never open the pre-first-prompt User cursor; but a bare
    matcher would be worse, because `SessionStart(compact)` fires
    mid-turn and `open_cursor` would replace the running Agent cursor
    with a User one (`engine.rs:23-30`). Match all five sources and
    branch on `source` in the hook: open a User cursor for `startup`,
    `resume`, `clear`, `fork`; do nothing for `compact`.
11. **Rollover loses the straddling turn and the split.** A prompt at
    01:55 and a `Stop` at 02:05 (default `reset_hour` 2) records nothing:
    the load at 02:05 returns a fresh state with no cursor
    (`state.rs:112-128`), and the archived `history` entry keeps only a
    scalar (`state.rs:14-18`). Carry `open` across the rollover, split
    the closing interval at the boundary, and archive agent/user/overlap
    with the total.
12. **Suspend counts as agent time.** Cursors are wall-clock
    (`cli.rs:96-102`); a lid closed during a generation books the sleep
    up to the cap. garlic has no suspend detector; the heartbeat gap
    (§ 21.3) is the only one available, and garlic can subtract a
    reported sleep window from any cursor that spans it.
13. **The gap rule is a cliff.** 39 minutes of reading counts 39, 41
    counts 0 (`engine.rs:42-51`), while the agent side clamps
    (`engine.rs:55`). Documented (`README.md:23`), but asymmetric, and
    with item 6 the same absence scores 0 or 120 depending on whether
    `Stop` fired. Clamping the user gap to the cap, or to the last
    `idle_prompt` row plus 60 s (§ 21.4), is the smaller surprise.
14. **Thresholds and bedtime only on the prompt hook.** A two-hour agent
    run crosses 60/90/120 silently and the user learns at the next
    prompt, when all crossed thresholds are marked at once
    (`engine.rs:110-119, 173-187`); a user idling in the bedtime hour
    without prompting never sees the bedtime nudge. Evaluate on `Stop`
    too, and record `crossed_at` per threshold so a display can say when.
15. **Liveness.** Nothing closes the cursor of a session that died
    without `SessionEnd` (`engine.rs:14-20, 146-162`). With the registry
    pid (§ 4.8) or the activity file's mtime (§ 21.3), `load_state` can
    close orphans at their last-seen time; without either, at least stop
    projecting them.
16. **Subagents are invisible, correctly**: the harness dispatches
    `SubagentStop`, not `Stop`, inside a subagent [D, B], so parallel
    subagents neither double count nor split the interval. Background
    subagents that outlive the main `Stop` run in what garlic books as
    user time; their wakeup arrives as a `system`-sourced prompt (item 7).

### 22.3 Interfaces

17. **Expose what a display needs in `status --json`**: `open` cursors,
    `projected_minutes` (or accept `--now`), the five config values and
    the reset instant, `crossed` (thresholds ≤ total regardless of
    nudging; today `next_threshold` means "not yet nudged",
    `commands.rs:232-235`), `last_nudge`, a per-session map (`sessions:
    {<id>: {agent, user, open_kind, open_since}}`), and a `schema_version`.
    Add `--local` (no flush, no write) and `--session <id>`. Make
    `statusline` read `session_id` from stdin so it can render a
    per-session lane (`cli.rs:57, 166`).
18. **`status --json` should be side-effect free by default**: no backend
    flush (§ 18.4), no config rewrite, no `create_dir_all` on a read.
19. **Add `version = 1` to `state.toml`** and treat the file format as a
    contract, since garnish reads it (§ 21.2).
20. **Hook ownership by tag, not prefix.** `is_garlic_entry` matches any
    command starting with `garlic hook` and deletes the whole entry
    (`setup.rs:31-48, 95-99`), so another tool's hook grouped into the
    same entry is removed. Use a `_tag`/`id` field, and de-duplicate
    against an enabled plugin instead of warning in the README.
21. **Timeouts.** With `GARLIC_SYNC=blocking` the `SessionEnd` flush
    (5 s client timeout, `remote.rs:22`) exceeds the harness's 1.5 s
    budget and the prompt flush sits inside the 30 s hook that blocks the
    model (`hooks.rs:178-195, 243-244`). Set a per-hook `timeout` on
    `SessionEnd`, and never flush inline on the prompt path; flush on
    `Stop` or from `sync`.
22. **Backend keying.** The merge is keyed by harness `session_id` with no
    machine component (`backend/src/engine.rs:351-367`), so two hosts
    that both fall back to `"default"` overwrite each other; `open` is
    never sent, so the shared total is always "as of the last closed
    interval". Add a machine id to the key and a `live` list to
    `POST /v1/intervals`. Client and server also bucket the day on
    different clocks (§ 18.5); intervals are absolute, so only "which
    day" differs.
23. **Small things.** `last_event_time` is written but never read
    (`state.rs:27`; `commands.rs:836`) and the README's claim that
    `SessionEnd` clears it (`README.md:325`) describes a mechanism that
    does not exist; the plugin manifest says `0.1.0` against crate
    `0.3.5`; `garlic version` does network I/O and may prompt, so probes
    should use `--version`; `format_duration` floors (`src/format.rs:
    7-15`), which garnish matches so the two never disagree by a minute;
    `status --json` has no schema version; hook cost is one process
    start, a config read (possibly a write), `create_dir_all`, two
    `flock`ed opens and a full parse/serialise per event, growing with the
    day's interval count, and there is no benchmark.

### 22.4 What garnish gives garlic in return

The nudge shown on the status line every second regardless of whether
the model relays it; a live total between events; the per-session lane;
liveness, suspend and interrupt signals; the pre-first-prompt interval for
every start source; API-time and token facets. None of it requires garlic
to change first; items 5, 17 and 19 make it cleaner.

## 23. Security and privacy

- **Injection path.** A hook's `additionalContext` lands in the model's
  context. Nothing read from a file may be relayed verbatim; garlic's
  relay wrapper stays fixed text and its message pools stay constants.
  garnish's event log and activity file hold numbers, ids and enums only;
  the one string (`cwd`) is never relayed anywhere.
- **File placement.** Runtime directories are per-user and 0700 by
  convention; garnish's cache root falls back to `~/.cache/garnish`, which
  is 0755 on many systems, so the activity files are created 0600.
- **Symlinks.** Both writers refuse to follow a symlink at the target path
  (garnish's `skills install` already does this; the same check applies
  to the activity file and to garlic's state and settings writes).
- **Contents.** The activity file holds timestamps, ids, token counts and
  the working directory. No prompt text, no model output, no transcript
  path. `garnish doctor`'s `~` collapsing applies if it is ever printed.
- **Session ids** are opaque tokens from the harness and are already
  sanitised before becoming a path segment.
- **Reading garlic's state** is read-only under a shared lock; garnish
  never writes `state.toml`, `config.toml` or `settings.json` on garlic's
  behalf.

---

# Part V — Spec impact and phase order

## 24. Spec impact

### 24.1 Non-goals touched

| current non-goal | proposals that touch it |
|---|---|
| no network calls | B3b (`gh` subprocess), C2, C3, C5, N10 (a `claude` subprocess): all worker-only, never on the tick, opt-in |
| `transcript_path` not used | C1, now only for `speed` and the exact reclaimed-tokens figure: a bounded tail read in a worker |
| fixed module set | A9 (`sandbox`, `voice`, `remote`, `account`), A12 (`version`), B4 (`skill`), N4 (`compactions`), N2/N3 (`today`), `provider`, the companion (`pet`, `say`, `tip`), G1 (`garlic`): still fixed, just larger |
| the tick never writes | untouched if the heartbeat worker (§ 21.3) carries the companion memory, the cost ledger and the ETA ring; touched only if 1 s resolution for the ETA ring is wanted |
| workers read only the payload, git and garnish's own cache | G1 reads garlic's `state.toml`/`config.toml`; N2 reads `stats-cache.json`; A9 reads Claude settings and the session registry; all read-only |
| no daemon | untouched; every proposal is a one-shot worker or a file read |
| no Windows | untouched |

### 24.2 Crate map candidates

`reqwest` for C2/C3/C5 (already named as the chosen HTTP crate);
`ratatui` + `ratatui-crossterm` + `crossterm` for 7.3c, or `inquire` for
7.3b. Gradients (A3) need no crate: OKLab is ~40 lines of arithmetic.
Recorded and rejected: `gix`, `git2`, `palette`, `memmap2`, `simd-json`,
`sysinfo`, `ureq`.

### 24.3 `SPEC.md` notes for the Phase 12–18 stack

Recorded here, not edited there: `remote.session_id` in the payload
(§ 4.1); "a config that fails to parse is never rewritten by any command"
(§ 12.1); the scheduled re-render one second after `resets_at`/
`expires_at` and the dropped window (§ 4.2); the `total_api_duration_ms`
ledger semantics (§ 4.4); the 33 hook events and the async-output rule
(§ 4.4). If a later version adds `model_scoped` or `extra_usage` to the
payload, C2 becomes a Tier A payload-only module and should be re-tiered
before anything network-related is built.

## 25. Suggested phase order after Phase 18

Grouped so each phase is one PR-sized concern and the cheap,
invariant-safe work lands first; network and TUI phases wait on § 0.

| phase | concern | contents |
|---|---|---|
| 19 | harness fidelity | verify items 1–4 and 10 of § 4.9; A1 dim reset (then golden); A12 `version`; N6 reduced motion; N5 `doctor` checks incl. the `garlic setup` warning |
| 20 | per-module presentation | A4 `hide = [...]`, A5 `max_width`, A6 `[format]` + `dim = "parens"`, A7 path styles, A13 separator color |
| 21 | links and identity | A8 links, A9 `sandbox` / `voice` / `account` (`remote` only if wanted), `provider` |
| 22 | usage views | A10 `limit5h` elapsed + absolute reset times, A11 `context` usable scale, N11 pace / burn / eta |
| 23 | garlic display | G1 `garlic` module: worker reads `state.toml`/`config.toml`, tick projects open cursors; per-session lane; verify items 5, 9 and 10; § 22 items 1–5 and 19 land in garlic alongside |
| 24 | heartbeat and facets | G2 activity file through the `garlic` worker; `garnish activity`; registry liveness; suspend and interrupt signals consumed by garlic (§ 22 items 6, 12, 15); N3 ledger and the companion memory ride the same worker |
| 25 | hooks | the shared `garnish hook`, `install --hooks` with tags, B4 `skill`, N4 `compactions`, G3 `idle_prompt`; plugin packaging (N8) if chosen |
| 26 | color | A3 gradients (OKLab, presets with attribution, 256/mono degrade) |
| 27 | layout | A2 center group; B1 Powerline segments if approved |
| 28 | git worker | B2 counts/stash/conflicts/in-progress op on `branch`, index-mtime invalidation; B3a `ci` from shuck state if available |
| 29 | subagent rows | N1 `garnish subagents` |
| 30 | transcript worker | C1 `speed` and reclaimed tokens (if approved) |
| 31 | network workers | C2 (after N10 is measured), C3, B3b (if approved; adds `reqwest`) |
| 32 | setup | 7.3b or 7.3c `garnish setup` (if approved), import/export, doctor screen |
| 33 | sharing and rotation | `config share` / `config apply` / `preview --config` / `preview --html`; rotation keys |
| 34 | companion core | `pet` one-row with the `sprig` voice, `say`, mood and pressure, `preview --live` (§ 17.1–17.6) |
| 35 | companion memory and tips | memory via the worker, tiers, `garnish stats` over `stats-cache.json`, `tip` catalog, N7 tips export (§ 17.7–17.8) |
| 36 | companion gutter and packs | `[gutter]`, `pack.toml` static art, spinner verbs (§ 17.4–17.5, 17.9) |

Each phase follows the protocol in `CLAUDE.md`: `SPEC.md` first, then
code, then the adversarial review. When a phase starts, move its text out
of this file into `SPEC.md` and delete it here, so this document shrinks
to what is still undecided.

---

# Appendices

## Appendix A — Verdicts on the earlier proposals, keyed to this document

The validation pass checked every claim of the first `FUTURE-SPEC.md`
against the official docs, the binaries, the issue tracker and the
ecosystem; the garlic pass checked PR #40 (§ 20). The table is kept so a
reader of the earlier PR bodies can see what changed and why.

| section (new) | claim or design | verdict | evidence |
|---|---|---|---|
| § 2 | market table (four repos) | **extended**: 31 more projects surveyed | C |
| § 3 | `COLUMNS`/`LINES` are the width source | **confirmed**: added in 2.1.153; `tput cols` cannot read the size in the piped child | D |
| § 3, § 4.2 | 300 ms debounce, in-flight cancel, `refreshInterval` ≥ 1 | **confirmed**; plus re-runs one second after a `resets_at`/`expires_at`, a dropped window, the `lastAssistantMessageId` trigger, and the `.catch(void 0)` schema rule | D, B |
| § 3 | payload-first, no transcript | **strengthened**: the transcript is written asynchronously and may lag | D |
| § 4.1, § 5 | payload table matches SPEC § 2.2 | **confirmed** with version notes (Appendix B); `remote.session_id` added from the binary | D, B |
| § 7.1 (A1) | rows render as `<Text dimColor wrap="truncate">` | **confirmed in 2.1.261 strings**, not re-located in 2.1.263; on-screen effect still to verify; #28750 fixed 2.1.141 | B, D, C |
| § 7.5 (A8) | OSC 8 links | **confirmed**; `FORCE_HYPERLINK=1`; `footerLinksRegexes` (2.1.176, at most five badges, scheme allowlist) badges ids alongside the line | D |
| § 8.1 (A9) | `remote` from `sessions/*.json` | **confirmed locally**; duplicates the harness's `/rc active` indicator | L, B, D |
| § 8.1 (A9) | `voice` from settings | **confirmed**: `voice.enabled`; `voiceEnabled` deprecated since 2.1.92 | D |
| § 8.1 (A9) | `account` from `~/.claude.json` | **plausible**: the file is documented; the field name is not | D, C |
| § 8.2 (A10) | elapsed / absolute reset times | **confirmed valuable** (claude-hud's five formats); the harness re-runs at `resets_at` | C, D |
| § 8.3 (A11) | usable-context scale | **corrected in part**: `used_percentage` always measures the full window (the sentence is on the env-vars page) | D |
| § 8.4 (A12) | `version` module | **confirmed** | D |
| § 6.4 (B1) | Powerline segments | **precedent**: claude-powerline, coralline, claudebar, kcchien | C |
| § 9.1 (B2) | git counts, stash, conflicts | **confirmed approach**: `GIT_OPTIONAL_LOCKS=0` documented as equivalent to `--no-optional-locks` | P, C |
| § 9.2 (B3) | PR/CI | **note**: the harness's own PR / MR badge; `pr.kind` | D |
| § 10.2 (B4) | `skill` via hooks | **confirmed**: `PreToolUse` matcher `Skill`; `UserPromptSubmit` has no matcher; hooks merge; `async: true`; `PostToolUse` inside subagents | D |
| § 9.6, § 10.3 (C1) | transcript metrics | **re-tiered**: count and trigger from `PostCompact`; `speed` and reclaimed tokens stay | D |
| § 9.3 (C2) | usage API | **weakened**; § 4.7 | C, D, B |
| § 9.5 (C3) | service status | **confirmed**: Statuspage JSON at both hosts | P |
| § 13 | installer | **extended**: `/statusline` auto-configures; trust gate; `disableAllHooks`; `allowManagedHooksOnly`; verbose debug log | D, B |
| § 4.5, § 17.9 | `spinnerVerbs`, `spinnerTipsOverride` | **reconfirmed**, with the 2.1.247 tip objects and `tipsFile` | D, B |
| § 6.1 | flex points, separators | no official source; claude-powerline's grid engine is the precedent | C |
| § 7.2 | OKLab | **confirmed** (Appendix C) | P |
| § 9.6 | transcript rows | `compact_boundary` shape from community tooling; subagent compaction mechanism from a collaborator on #16944 | C, M |
| § 4.4 | hook payloads | **refined**: `PostToolUseFailure` fields; `PermissionDenied` auto-mode only; `tool_response` shapes | D |
| § 8.1 | settings layers | **corrected order**: managed > `--settings` > project local > shared project > user | D |
| § 16.2 | codachi's `PostToolExecution` hook | **absent** from the 33 documented events | D |
| § 10.4, § 17.2 | companion event sources | **extended**: `Notification` types, `StopFailure` matchers, `PostCompact`, `SubagentStart/Stop`, `PostToolBatch`, `PostModelSwitch`; async hooks must stay silent | D |
| § 17.4 | animation | **extended**: `prefersReducedMotion` | D, B |
| § 17.8 | tips | **alternative**: export as `spinnerTipsOverride.tipsFile` | D |
| § 12.2 | sharing | **precedent**: Powerline Studio; coralline's p10k import | C |
| § 4.4 | `total_api_duration_ms` grows during a request | **corrected**: a ledger stepped once per completed request | B, D |
| § 4.4 | 32 hook events | **corrected**: 33 in both the docs and the binary | D, B |
| § 4.3 | footer width | **confirmed for the status line**; a second width exists for the hint line only | B |
| § 20 | PR #40's claims | per-row verdicts in § 20 | S, D, B |

## Appendix B — Official facts with the version that introduced them

From the changelog page (`<Update label>` headings) and the "Requires
Claude Code vX" notes on the reference pages [D]:

| fact | version |
|---|---|
| `/statusline` command and custom status line | 1.0.71 |
| `exceeds_200k_tokens` | 1.0.88 |
| `context_window.current_usage` | 2.0.70 |
| `spinnerVerbs` | 2.1.23 |
| `spinnerTipsOverride` | 2.1.45 |
| `workspace.added_dirs` | 2.1.47 |
| `worktree` object | 2.1.69 |
| status line blank without workspace trust (fix) | 2.1.79 |
| `rate_limits` (5-hour and 7-day) | 2.1.80 |
| `refreshInterval`; `workspace.git_worktree` (listed again under 2.1.98) | 2.1.97 |
| `effort.level`, `thinking.enabled` | 2.1.119 |
| stale remote-control status lines after resume (fix) | 2.1.128 |
| `context_window` counts are current context, not cumulative (fix) | 2.1.132 |
| multi-line output no longer drops rows when a line is over-wide (fix) | 2.1.141 |
| `workspace.repo` and `pr` ("GitHub repo and PR information"; the field names come from the status line page) | 2.1.145 |
| `COLUMNS` and `LINES` in the environment | 2.1.153 |
| footer hints restored for custom status line users (fix); the current docs say a custom status line hides most hints again | 2.1.169 |
| `footerLinksRegexes` | 2.1.176 |
| `CLAUDE_CLIENT_PRESENCE_FILE` | 2.1.181 |
| `prompt_id` in the status line payload | 2.1.196 |
| `subagentStatusLine` per-task `model` and `contextWindowSize` | 2.1.205 |
| `subagentStatusLine` per-task `effort` | 2.1.214 |
| status line running twice on resume (fix) | 2.1.216 |
| GitLab MR badge, `pr.kind`; `quota_auto_resume_*` notification types | 2.1.234 |
| `modelPricing`; pre-reset `rate_limits` percentage after idle (fix) | 2.1.243 |
| `spinnerTipsOverride` tip objects, `tipsFile`, `label` | 2.1.247 |
| `agent_needs_input` for a teammate's terminal setup question | 2.1.248 |
| `rate_limits.spend_limit`; `prompt_cache` | 2.1.251 |
| `prompt_cache.last_miss_cause`, `miss_causes` (the entry says "a likely cause for prompt-cache misses"; the field names come from the status line page) | 2.1.260 |
| `keybindingFlavor` deprecated (the docs describe ≥ this version) | 2.1.261 |
| "Bug fixes and reliability improvements"; no 2.1.262 entry | 2.1.263 |

Other documented facts used above [D]: the status line runs once at
session start and on resume, then on the triggers in § 4.2;
`hideVimModeIndicator` suppresses the built-in `-- INSERT --` row; the
line hides during autocomplete, help and permission prompts;
`CLAUDE_CODE_SHELL_PREFIX` wraps status line commands;
`CLAUDE_CODE_SAFE_MODE=1` skips a non-managed status line; the
`rate_limits` object appears only for Pro/Max subscribers or behind a
gateway with spend limits, after the first API response; `-p` sessions
run the same hooks as interactive ones unless `--bare`; on SIGTERM in
`-p` mode Claude Code runs `SessionEnd` hooks and exits; `/clear` starts a
new session and resets cost to $0.

## Appendix C — OKLab matrices (Björn Ottosson, bottosson.github.io/posts/oklab) [P]

Linear sRGB → LMS:

```text
l = 0.4122214708·r + 0.5363325363·g + 0.0514459929·b
m = 0.2119034982·r + 0.6806995451·g + 0.1073969566·b
s = 0.0883024619·r + 0.2817188376·g + 0.6299787005·b
```

then `l' = ∛l`, `m' = ∛m`, `s' = ∛s`, then LMS' → Lab:

```text
L = 0.2104542553·l' + 0.7936177850·m' − 0.0040720468·s'
a = 1.9779984951·l' − 2.4285922050·m' + 0.4505937099·s'
b = 0.0259040371·l' + 0.7827717662·m' − 0.8086757660·s'
```

The inverse cubes the intermediate values and applies the inverse
matrices (the `4.0767416621 …` matrix in ccstatusline's source is the
LMS → linear sRGB inverse). sRGB companding and the xterm-256 mapping are
in § 7.2.

## Appendix D — Theme and gradient tables

The ten Powerline themes are in § 6.4 and the thirteen gradient presets
in § 7.2, next to the algorithms that consume them; they are not repeated
here.

## Appendix E — Crate facts (crates.io, 2026-09-06) [P]

| crate | version | note |
|---|---|---|
| ratatui | 0.30.2 | backends split out; `ratatui-crossterm` 0.1.2 supplies `CrosstermBackend`; `termina` 0.4.0 is a newer backend |
| crossterm | 0.29.0 | last release 2025-04-05 |
| inquire | 0.9.4 | the prompt-wizard crate for 7.3b; used by Claude-Code-Personalities and ccusage-statusline-rs |
| dialoguer | 0.12.0 | alternative to inquire; used by claude-code-statusline-pro |
| gix | 0.87.1 | pure-Rust git used by claude-powerline-rust; would replace `git::run_program`, which the crate map rules out; recorded only |
| ureq | — | the blocking HTTP crate cship, best-claude-hud and garlic use; garnish's named choice stays `reqwest` |
| palette | 0.7.7 | not needed; OKLab is ~40 lines (Appendix C) |
| memmap2, simd-json | 0.9.11, 0.18.1 | claude-powerline-rust's transcript-scan tricks; irrelevant without a scan |

## Appendix F — Where to look in the mined repositories

### ccstatusline (commit 016be1f)

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

### garlic (v0.3.5)

| topic | files |
|---|---|
| hook dispatch, session id, timestamps | `src/cli.rs` (`parse_session_id` 221-232, `unix_now` 96-102), `src/hooks.rs` |
| cursor model, closing rules, thresholds, bedtime | `src/engine.rs` (14-30, 37-57, 92-97, 103-162, 173-207) |
| union, coalescing, breakdown | `src/intervals.rs` (40-53, 55-77, 86-137) |
| state file, locking, rollover, corruption | `src/state.rs` (12, 20-52, 74-86, 99-158, 385-406) |
| config defaults and loading | `src/config.rs` (13-19, 35-49, 100-121), `src/commands.rs` 658-727 (`set` validation) |
| `status --json`, `statusline` | `src/commands.rs` (206-248, 345-398) |
| nudges and the relay frame | `src/nudges.rs` (7-85, 88-94, 109-113), `src/hooks.rs` (58-79, 204-211) |
| installer and plugin | `src/setup.rs` (15-48, 50-62, 65-103), `plugins/garlic/hooks/hooks.json`, `plugins/garlic/.claude-plugin/plugin.json`, `plugins/garlic/commands/garlic.md` |
| backend and sync | `src/remote.rs` (22, 46-78, 99-117), `src/sync.rs` (37-56, 104-119), `backend/src/engine.rs` (30-78, 199-237, 336-381), `backend/src/store.rs` (21-74, 140-181), `backend/README.md` |
| version check | `src/version.rs` (9, 38-81), `src/commands.rs` (73-77, 123) |

## Appendix G — Old section numbers → this document

For references in the bodies of PR #27 (earlier revisions) and PR #40.

| old (`FUTURE-SPEC.md`, first revision) | new |
|---|---|
| § 1 Method | § 1 |
| § 2 Market | § 2.1 |
| § 3 Where garnish is ahead | § 3 |
| § 4 Widget inventory | § 5 |
| § 5.1 Tier A: A1 | § 7.1; A2 § 6.2; A3 § 7.2; A4 § 7.4; A5, A13 § 6.3; A6 § 7.3; A7, A8 § 7.5; A9 § 8.1; A10 § 8.2; A11 § 8.3; A12 § 8.4 |
| § 5.2 Tier B: B1 | § 6.4; B2 § 9.1; B3 § 9.2; B4 § 10.2 |
| § 5.3 Tier C: C1 | § 9.6 and § 10.3; C2 § 9.3–9.4; C3, C5 § 9.5; C4 § 13 |
| § 5.4 Tier D | § 14 |
| § 6 Lessons | § 15 |
| § 7 Installer | § 13 |
| § 8 Spec changes; § 8.3 verified facts | § 24; § 4 |
| § 9 Decisions | § 0 |
| § 10 Phase order | § 25 |
| § 11 Line composition | § 6.1–6.2 |
| § 12 Powerline painter and theme table | § 6.4 |
| § 13 Gradients | § 7.2 |
| § 14 Number formats | § 7.3 |
| § 15 Hide states | § 7.4 |
| § 16 Option vocabulary | § 7.6 |
| § 17 Output and lifecycle tricks | § 6.5, § 7.1, § 14 |
| § 18.1 git | § 9.1; § 18.2 PR/CI § 9.2; § 18.3 usage § 9.3; § 18.4 transcript § 9.6; § 18.5 hooks § 10; § 18.6 settings § 8.1; § 18.7 timers § 9.7; § 18.8 status § 9.5 |
| § 19 Config lifecycle | § 12.1 |
| § 20 Installer mechanics | § 13.1 |
| § 21 Tests | § 15 item 11 |
| § 22 Payload comparison | § 5 (end) |
| § 23 The two companion projects | § 16 |
| § 24 Companion design; § 24.11 decisions | § 17; § 0 |
| § 25–26 Editor, sharing; § 26.1 decisions | § 12.2–12.4; § 0 |
| Appendix (file maps) | Appendix F |

| old (`FUTURE-SPEC-RESEARCH.md`) | new |
|---|---|
| § 1 decision-changing findings | § 0, § 4, § 9.3–9.4, § 10, § 11 |
| § 2 verdict table | Appendix A |
| § 3 decision impacts | § 0 and the sections named there |
| § 4 new surfaces N1–N13 | N1 § 11; N2/N3 § 8.6; N4 § 10.3; N5 § 13.4; N6 § 8.4; N7 § 17.8; N8 § 10.1; N9 § 8.7; N10 § 9.4; N11 § 8.5; N12, N13 § 14 |
| § 5 ecosystem | § 2.2–2.3 |
| § 6 verify-first | § 4.9 |
| § 7 crates | Appendix E |
| Appendix A OKLab; B versions; C sources | Appendix C; Appendix B; Appendix H |

| old (PR #40 `GARLIC-INTEGRATION.md`) | new |
|---|---|
| § 1 what each tool sees | § 18, § 4 |
| § 2–3 what the sampler adds | § 21.3 |
| § 4 focus | § 4.6, § 19 |
| § 5 tick cadence | § 4.2 |
| § 6.1–6.3 shapes | § 21.1–21.3 |
| § 7 contract | § 21.6 |
| § 8 security | § 23 |
| § 9 open questions | § 0 (G1–G6) |
| § 10 re-verify | § 4.9 |

## Appendix H — Sources

**Official** (fetched 2026-09-06 as Markdown from
`https://code.claude.com/docs/en/…`): `statusline`, `hooks`, `hooks-guide`,
`settings`, `settings-reference`, `plugins-reference`, `plugins`,
`fullscreen`, `accessibility`, `remote-control`, `voice-dictation`,
`costs`, `context-window`, `model-config`, `env-vars`, `claude-directory`,
`sessions`, `monitoring-usage`, `commands`, `headless`, `cli-reference`,
`interactive-mode`, `changelog`, `agent-sdk/typescript`,
`whats-new/2026-w24..w34`.

**Binaries**: `strings` of Claude Code 2.1.261 (2026-09-05) and 2.1.263
(2026-09-06, `BUILD_TIME 2026-09-06T01:08:56Z`). Anchors used: the payload
builder (`exceeds_200k_tokens`, `spend_limit`, `remote:{session_id`), the
status line controller (`refreshInterval`, the eight-name trigger array,
`Plo=300`, `Mlo=1000`), the footer (`flexWrap:"wrap"` with `paddingX`/
`columnGap` from a `var X=2,Y=1` pair near `FooterHintLine`),
`statuslineIssueCount` (2.1.261 row wrapper), `terminalFocus`,
`terminalFocusGainedAt`, `lastInteractionTime`, `FOCUS_EVENTS:1004`, the
presence function with `60000`, `DISABLE_NOTIFICATION_PRESENCE_CHECK`,
`user_present`, `messageIdleNotifThresholdMs`, `sendIdleNotification`,
`idle_prompt`, the notification-type array, the 33-name hook-event array,
the `UserPromptSubmit` schema with `source`, `SubagentStop` conversion,
`recordApiDuration`, `get_usage`, `skip_behaviors`, `hideVimModeIndicator`,
`subagentStatusLine`, `prefersReducedMotion`, `bridgeSessionId`,
`abandoned-stale`, `weekly_scoped`, `model_scoped`, `tipsFile`, `Status
line command skipped`, `stats-cache.json`.

**Issue tracker** (`anthropics/claude-code`, state on 2026-09-06): #28750
(closed, not planned; community root cause `wrap: "truncate"`), #27305
(closed; notification banners compress the line), #27864 (closed, stale),
#22115 (closed, completed: `COLUMNS`), #13585 (open: quota access), #16944
(closed, completed by a collaborator: subagent compaction docs), #27916
(closed: subagent count in the status line), #31021 and #31637 (closed,
not planned: usage endpoint 429), #26096 (closed, completed: `added_dirs`).

**garlic**: `~/repos/garlic` at v0.3.5 (`Cargo.toml:3`), files in
Appendix F; `README.md`, `backend/README.md`, `plugins/garlic/*`.

**Primary**: git manual (`GIT_OPTIONAL_LOCKS`, `--no-optional-locks`);
Ottosson, *A perceptual color space for image processing* (OKLab);
crates.io API; live probes of `status.claude.com/api/v2/{status,incidents}.json`.

**Community**: READMEs and metadata of the 31 repositories in § 2.2 via
the GitHub API; ccusage.com/guide/statusline.

**Local checks** (keys and process state only): `~/.claude/sessions/*.json`,
`~/.claude/stats-cache.json`, `/proc/<pid>` for registry pids.
