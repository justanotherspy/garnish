# FUTURE-SPEC-RESEARCH.md — online validation of FUTURE-SPEC.md

`FUTURE-SPEC.md` was written from four repositories read offline. This file
checks its claims against the official Claude Code documentation, the
2.1.261 binary, the Claude Code issue tracker, and the wider status line
ecosystem (31 more projects), and records what that changes. It does not
edit `FUTURE-SPEC.md`; each finding names the section it bears on so the
next planning session can fold it in. Research date: 2026-09-06.

Evidence grades used in the tables:

| grade | meaning |
|---|---|
| **D** | official docs at `code.claude.com/docs`, fetched 2026-09-06 (the pages describe ≥ 2.1.261; version notes quoted where the page gives them) |
| **B** | strings of the Claude Code 2.1.261 binary on this machine |
| **M** | a maintainer or collaborator comment on `anthropics/claude-code` |
| **C** | a community source: an issue comment by a non-member, a project README |
| **L** | a local check on this machine (file *keys* only, never values) |
| **P** | a primary technical source outside Claude Code (git manual, Ottosson's OKLab post, crates.io, a live HTTP probe) |

## 1. What changes a decision

Five findings alter a tier or a checkbox in § 9, § 24.11 or § 26.1. The
rest of this file is detail.

1. **`compactions` no longer needs the transcript (C1 splits).** Claude
   Code fires a `PostCompact` hook with a `manual` / `auto` matcher and a
   `PreCompact` hook before it [D]. A `garnish hook` entry on `PostCompact`
   can append to the same event log the companion and the `skill` module
   use (§ 18.5, § 24.2), so `compactions` becomes a hook-fed module in the
   B4 family. Only `speed` still needs a transcript read, so C1 shrinks to
   `speed` and the § 9 "Transcript" decision is about one module.
2. **C2 (usage API) is weaker than written, and has two alternatives.**
   Two issues about persistent HTTP 429 from `api.anthropic.com/api/oauth/usage`
   were closed *not planned*, one labelled `invalid` (#31021, #31637); the
   request for official quota access (#13585) is still open with 26
   comments; Claude Code's own `/usage` degrades to "last-known usage within
   60 minutes" when that endpoint rate-limits [D]. A community comment says
   the limit is per access token (about five requests) and suggests
   refreshing tokens to get a fresh window [C]; garnish must not port that,
   it evades a limit. Alternatives found: (a) `get_usage` is a real control
   request subtype of the stream-json SDK protocol in the binary
   (`get_usage is not supported in this context (onGetUsage callback not
   registered)`, with a `skip_behaviors` option) [B], so a worker could spawn
   `claude -p --input-format stream-json` and ask, with no credential
   handling, at the cost of a Claude Code process start; (b)
   `~/.claude/stats-cache.json` holds per-day `tokensByModel`, message,
   session and tool-call counts written by the harness for `/usage` [D][L].
   Neither gives a quota percentage; only the undocumented endpoint does.
3. **Hooks can ship as a plugin, but the status line cannot.** Plugin
   `hooks/hooks.json` is an official surface, and plugin `settings.json`
   may carry only `agent` and `subagentStatusLine`, explicitly not
   `statusLine` or `spinnerVerbs` [D]. Every top competitor (claude-hud
   27.8k stars, starship-claude, claude-powerline, the mascot) ships as a
   marketplace plugin plus a setup skill that writes `statusLine` [C]. The
   § 9 "Hooks" decision therefore has a third answer: publish garnish as a
   plugin whose `hooks.json` carries the tagged hooks, and keep
   `install --hooks` for non-plugin installs.
4. **`subagentStatusLine` is a second render surface FUTURE-SPEC does not
   cover.** One command run per refresh receives every visible subagent row
   as one JSON object (`columns` for the usable width, a `tasks` array with
   `id`, `name`, `type`, `status`, `description`, `label`, `startTime`,
   `model`, `effort`, `contextWindowSize`, `tokenCount`, `tokenSamples`,
   `cwd`) and writes back `{"id", "content"}` lines; it is trust-gated like
   `statusLine` [D][B]. coralline already themes those rows [C]. See § 4,
   N1.
5. **The width rule holds; the truncation history is now dated.** In the
   2.1.261 strings the footer is still a `flexWrap: "wrap"` box with
   `paddingX: Vne` (2) and `columnGap: Gne` (1), and the `isNarrow`
   half-width switch that the mascot README documents for 2.1.76 is gone
   [B]. The multi-line truncation bug behind A1's caution (#28750, `wrap:
   "truncate"` dropping rows) was fixed in 2.1.141 per the changelog [D].
   What remains for Phase 19: outside fullscreen rendering, notifications
   share the status line row and verbose mode adds a token counter there
   [D], so a full-width line can still be squeezed; the fullscreen renderer
   gives notifications their own row.

Two smaller items that belong in the same breath: the installer market norm
is *skill-driven* setup (coralline's `INSTALL.md` playbook, starship-claude's
`/starship`, claude-hud's `/claude-hud:setup`) and Claude Code's own
`/statusline` command auto-configures from the shell prompt [D][C], which
supports 7.3a as the floor; and Rust competitors already ship `ratatui`
configurators with live preview (claudebar, CCometixLine, best-claude-hud)
[C], which supports 7.3c as a proven shape rather than a novelty.

## 2. Verdicts by FUTURE-SPEC section

| § | claim or design | verdict | evidence |
|---|---|---|---|
| 2 | market table (four repos) | **extend**: 31 more projects surveyed; the largest are claude-hud (27,837 ★), ccusage (18,379 ★, now Rust) and tweakcc (2,483 ★); see § 5 | C |
| 3 | `COLUMNS`/`LINES` are the width source | **confirmed**: added in 2.1.153; docs say `tput cols` cannot work in the piped child; the `ps`+`stty` fallback competitors use is irrelevant for garnish (minimum 2.1.251) | D |
| 3 | 300 ms debounce, in-flight cancel, `refreshInterval` ≥ 1 | **confirmed**; also: re-runs when a `resets_at` or `expires_at` in the last payload passes, and a window is *dropped* once its `resets_at` passes | D |
| 3 | payload-first, no transcript | **strengthened**: hooks docs say the transcript is written asynchronously and may lag the in-memory conversation | D |
| 4 / 22 | payload table matches SPEC § 2.2 | **confirmed**: `workspace.repo` (2.1.145), `git_worktree` (2.1.97), `added_dirs` (2.1.47), `effort`/`thinking` (2.1.119), `prompt_cache` and `spend_limit` (2.1.251), `last_miss_cause`/`miss_causes` (2.1.260), `pr.kind = "mr"` (2.1.234); `used_percentage` may be `null` early, `current_usage` is `null` after `/compact` | D |
| 5 A1 | rows render as `<Text dimColor wrap="truncate">` | **confirmed in strings**; the on-screen effect is still to verify. Multi-line row loss (#28750) fixed 2.1.141 | B, D, C |
| 5 A8 | OSC 8 links | **confirmed**, with two additions: `FORCE_HYPERLINK=1` overrides detection; `footerLinksRegexes` (2.1.176) renders link badges *alongside* the status line, so garnish should not duplicate IDs the user already badges | D |
| 5 A9 | `remote` from `sessions/*.json` matched by `sessionId` | **confirmed locally**: the per-session file carries `sessionId`, `bridgeSessionId`, `name`, `nameSource`, `status`, `cwd`, `pid`, `version` [L]; `bridgeSessionId` present in the binary [B]. **Caveat**: the harness already shows a `/rc active` footer indicator, so the module is a duplicate at best | L, B, D |
| 5 A9 | `voice` from settings | **confirmed**: `voice.enabled` object; `voiceEnabled` deprecated since 2.1.92; the footer's own voice hint is hidden when a custom status line exists | D |
| 5 A9 | `account` from `~/.claude.json` | **plausible**: the file is documented as holding app state and the OAuth session; the field name is not documented | D, C |
| 5 A10 | elapsed / absolute reset times | **confirmed valuable**: claude-hud offers `relative`, `absolute`, `both`, `elapsed`, `elapsedAndAbsolute` plus an `hourCycle`; the harness re-runs the line at `resets_at` so a countdown is never stale at the boundary | C, D |
| 5 A11 | usable-context scale | **corrected in part**: `used_percentage` is always measured against the full window; with `autoCompactWindow` (setting), `--autocompact` (flag) or `CLAUDE_CODE_AUTO_COMPACT_WINDOW` (env, wins) set, the percentage "no longer indicates when compaction will run" [D]. garnish's SPEC § 2.3 already models that chain. See § 6 for the 13k buffer discrepancy | D |
| 5 A12 | `version` module | **confirmed** field | D |
| 5 B1 | Powerline segments | **precedent**: claude-powerline, coralline (pill/lean/classic), claudebar, kcchien; claude-powerline also has a boxed `tui` style with a grid engine (`areas`, `columns`, breakpoints, culling) | C |
| 5 B2 | git counts, stash, conflicts | **confirmed approach**: `GIT_OPTIONAL_LOCKS=0` documented (equivalent to `--no-optional-locks`); coralline does one `git status --porcelain=v2 --branch` per render; claude-hud adds opt-in Jujutsu (`jj`) support | P, C |
| 5 B3 | PR/CI | **note**: the harness's own footer shows the PR / `MR !N` badge (2.1.234); `pr.kind` distinguishes GitLab | D |
| 5 B4 | `skill` via hooks | **confirmed**: `PreToolUse` matcher `Skill`; `UserPromptSubmit` has no matcher; hooks merge across settings levels; command hooks block by default and take `async: true`; `PostToolUse` also fires inside subagents with `agent_id`/`agent_type` | D |
| 5 C1 | `compactions`, `speed` from the transcript | **re-tier** `compactions` (see § 1.1); `speed` stays | D |
| 5 C2 | usage API | **weakened**; see § 1.2 | M, C, D, B |
| 5 C3 | service status | **confirmed**: `status.claude.com/api/v2/status.json` and `incidents.json` return Statuspage JSON (`status.indicator`, `incidents[]`); `status.anthropic.com` serves the same page id | P |
| 6 | lessons | unchanged | — |
| 7 | installer | **extended**: `/statusline` auto-configures from the shell prompt; `statusLine` runs only after workspace trust is accepted (`claude --debug` logs `Status line command skipped: workspace trust not accepted`); `disableAllHooks` outside managed settings disables it; `allowManagedHooksOnly` hides it silently; `CLAUDE_CODE_DEBUG_LOG_LEVEL=verbose` logs full status line output | D, B |
| 8.3 | verified facts | **all reconfirmed**; `spinnerVerbs` `replace` with an empty list keeps the built-ins; `spinnerTipsOverride` accepts tip objects `{id, text, cooldownSessions, priority}`, `tipsFile` (absolute or `~/` path) and `label` since 2.1.247, plain strings only from project and local files; `weekly_scoped` and `model_scoped` in the binary | D, B |
| 11 | flex points, separators | no official source; claude-powerline's grid engine is the richest precedent (fr units, `auto`, spans, dividers, automatic culling of empty cells and rows) | C |
| 12 | Powerline painter | unchanged | — |
| 13 | OKLab | **confirmed**: Ottosson's matrices reproduced in Appendix A; the xterm-256 mapping is the standard 6×6×6 cube plus 24-step gray ramp | P |
| 14–17 | formats, hide states, options, lifecycle tricks | no external check needed | — |
| 18.1 | git cache | see B2 | P |
| 18.3 | usage flow | the endpoint is undocumented and community-discovered (ohugonnot README says so plainly); see § 1.2 | C, M |
| 18.4 | transcript rows | `compact_boundary` / `compactMetadata` confirmed by community tooling and by a collaborator comment that documented subagent compaction (#16944) | C, M |
| 18.5 | hook payloads | **refined**: `PostToolUseFailure` carries `error`, `is_interrupt`, `duration_ms`; for Bash the error starts with `Exit code N`; it does **not** fire for validation or permission rejections (`PermissionDenied` does); `tool_response` is an object whose shape depends on the tool (Bash: `stdout`, `stderr`, `interrupted`, `isImage`) | D |
| 18.6 | settings layers | **corrected order**: official precedence is managed > `--settings` > project local > shared project > user; ccstatusline reads only the four files, in that order, so the design is right for files; `CLAUDE_CONFIG_DIR` confirmed | D |
| 18.8 | status endpoint | confirmed (C3) | P |
| 20 | TUI crates | versions in § 7 | P |
| 23.2 | codachi's `PostToolExecution` hook | **confirmed absent** from the 32 documented events; the design's `PostToolUse` + `PostToolUseFailure` is right | D |
| 24.2 | companion event sources | **extend**: `Notification` types `idle_prompt` (about 60 s idle), `permission_prompt` (after about 6 s), `quota_auto_resume_*`; `StopFailure` matchers `rate_limit`, `overloaded`, `authentication_failed`, `billing_error`…; `PostCompact`, `SubagentStart`/`SubagentStop`, `PostToolBatch`, `PostModelSwitch`; give the garnish hook `async: true` so it can never delay a tool | D |
| 24.4 | animation | **extend**: `prefersReducedMotion` (settings, `/config` → Reduce motion) should default `animate` to off | D, B |
| 24.8 | tips | **alternative**: garnish's catalog could also be exported as a `spinnerTipsOverride.tipsFile` so the harness rotates it in its own spinner | D |
| 24.9 | spinner verbs | confirmed; tweakcc goes further by patching the binary (verbs, spinner styles, themes) — Tier D for garnish | D, C |
| 25–26 | editor, sharing | **precedent**: claude-powerline's Powerline Studio (web configurator, paste-to-edit, copy JSON) and coralline's importable `~/.p10k.zsh` values | C |

## 3. Decision impacts

### § 9

- **Network.** Unchanged in principle, but C2's case is weaker (§ 1.2). If
  Daniel wants any usage view beyond the payload, evaluate option (a)
  `get_usage` over stream-json in a worker before considering the OAuth
  endpoint; option (b) `stats-cache.json` needs no network at all and gives
  a `today` view (§ 4, N2). C3 stays as written (endpoint confirmed).
- **Transcript.** Now only `speed` needs it. `compactions` moves to the hook
  family.
- **Installer form.** 7.3a is the market floor and the harness has a
  built-in competitor (`/statusline`). 7.3c has three Rust precedents. No
  change to the choice, better information for it.
- **Hooks.** New option: ship a plugin (`hooks/hooks.json`, setup skill,
  `bin/`, `${CLAUDE_PLUGIN_DATA}` for the cache) via a marketplace, which
  installs hooks without editing user settings; the skill still writes
  `statusLine` because plugin settings cannot. `install --hooks` remains
  for `cargo install` users.
- **Module count.** Two candidates join the list: `compactions` (hook-fed)
  and `today` (stats cache); `remote` weakens (the harness shows its own
  indicator).
- **Segments.** Unchanged.
- **Verify items.** Reordered in § 6; the notification-row squeeze is new
  and first.

### § 24.11

- **Ship a companion.** Market confirms demand: eight pet projects found,
  the most-starred (claude-code-tamagotchi, 435 ★) pairs the pet with an
  LLM-driven "violation" blocker over Groq, which garnish must not copy
  (network, LLM, blocking hooks). The mascot project's 9 hook states and
  heat-map color shift, and Claude-Code-Personalities' kaomoji-by-activity,
  are the closest to the § 24 design.
- **Species / packs.** The mascot ships packs with a `pack.json`/`pack.yaml`
  search order (project `.claude/mascot-packs/`, user plugin dir, bundled),
  a validator and a storybook CLI, which matches § 24.5's pure-data packs
  and `preview --live`.
- **Tick-side writes.** Unchanged; note that claude-hud folds the payload's
  `cost.total_cost_usd` into a per-day ledger keyed by `session_id` on every
  render, so a "today's spend" from the payload alone is a precedent for
  the same trade-off.
- **Gutter.** The mascot documents the failure mode a gutter must avoid:
  before 2.1.141 one over-wide row dropped every row below it; it wraps its
  summary at `|` to keep each row under width. garnish's exact-width frame
  already prevents this.
- **Spinner verbs.** Confirmed; `replace` with an empty list is a no-op.
- **`garnish stats`.** `~/.claude/stats-cache.json` already holds
  `dailyActivity`, `dailyModelTokens`, `totalSessions`, `hourCounts`, so a
  `stats` subcommand can read the harness's own aggregates instead of
  keeping a parallel ledger.

### § 26.1

- `config share` / `config apply`: claude-powerline's web studio and
  ccstatusline-editor both prove the paste-a-config loop; no change.
- `preview --html`: no external input.
- Rotation keys: no external input.
- WASM preview: no change; Tier C.

## 4. New surfaces not in FUTURE-SPEC

| id | surface | tier | design note |
|---|---|---|---|
| N1 | `garnish subagents` for `subagentStatusLine` | B | One process per refresh reads `{…base hook fields, columns, tasks[]}` and prints one `{"id","content"}` line per task. Reuse the module renderer: `name`, `model`, `effort`, a context gauge from `tokenCount / contextWindowSize` (2.1.205+), elapsed from `startTime`, `status` coloring; `columns` replaces `COLUMNS`. Plugin `settings.json` may ship it as a default. Trust-gated like `statusLine`. |
| N2 | `today` module from `~/.claude/stats-cache.json` | B | Worker reads the file (harness-maintained, no credentials, no transcript), caches `dailyModelTokens` for today; render tokens by model or a total. Cost needs a pricing table, which garnish does not have; show tokens only unless the payload cost route (N3) is chosen. Verify first how often the harness rewrites the file. |
| N3 | `today` spend from payload cost, ledger per `session_id` | B | claude-hud's approach: fold `cost.total_cost_usd` into `<cache>/daily-cost.json` keyed by `session_id` (baseline on first sight, midnight reset, drop entries unseen for 24 h). Needs a tick-side write, which SPEC forbids today; the same decision as the companion memory (§ 24.11). |
| N4 | `compactions` via `PostCompact` | B | Hook entry `{"PostCompact": [{"matcher": "auto|manual", "hooks": [{"type": "command", "command": "garnish hook", "async": true}]}]}` appends to the session event log; module renders count and split. Replaces the C1 transcript design for this module. |
| N5 | installer / `doctor` checks | A | Report workspace trust, `disableAllHooks`, `allowManagedHooksOnly`; set `hideVimModeIndicator: true` when the `vim` module is on; suggest `refreshInterval` when `clock`, `limit*` countdowns or animation are configured (docs recommend it for time-based segments); detect harness runs by `CLAUDE_CODE_CHILD_SESSION=1` / `CLAUDECODE=1` so `preview` can tell a manual run from a harness run. |
| N6 | `prefersReducedMotion` | A | When the settings read (already cached for the autocompact override) finds it `true`, treat `animate` as off unless the garnish config sets it explicitly. |
| N7 | tips as `spinnerTipsOverride.tipsFile` | A | `garnish tips export` writes the catalog as tip objects with ids and cooldowns; the user points `tipsFile` at it (absolute or `~/` path; ignored from remote managed settings). Complements, not replaces, the `tip` module. |
| N8 | plugin packaging | B | `.claude-plugin/plugin.json`, `hooks/hooks.json`, `skills/setup/SKILL.md`, `bin/garnish` (added to the Bash PATH), cache under `${CLAUDE_PLUGIN_DATA}`; `claude plugin init` scaffolds it; `claude plugin validate --strict` in CI. Distribution needs a marketplace repository. |
| N9 | usage snapshot interop | B | claude-hud reads and writes an "external usage snapshot" JSON (`updated_at`, `five_hour`, `seven_day`, `balance_label`, `model_scoped[]` with `display_name`, `utilization`, ISO `resets_at`). garnish could read one as a fallback when the payload lacks `rate_limits` and, if tick-side writes are allowed, write one so idle sessions and other tools see fresh windows (coralline's `VL_LIMIT_SYNC` solves the same idle-session problem with a file store). |
| N10 | `get_usage` control request | C | Worker runs `claude -p --input-format stream-json --output-format stream-json`, sends `{"type":"control_request","request_id":…,"request":{"subtype":"get_usage","skip_behaviors":true}}`, parses `rate_limits` incl. `model_scoped`. Official protocol, no token handling, but a Claude Code process start per refresh and an undocumented response shape; measure before proposing. |
| N11 | burn-rate / time-to-limit | A | coralline's `burn` (projected time until the binding 5h or 7d window reaches 100 %) and ccusage's `$/hr` burn rate are the market form of § 24.10's pace delta; garnish can compute time-to-100 % from `used_percentage` and `resets_at` with no extra data. |
| N12 | Jujutsu (`jj`) status | D | claude-hud supports it opt-in, one subprocess, read-only flags. Out of scope unless Daniel uses jj. |
| N13 | binary patching (context-low suppression, verbose, spinner styles) | D | CCometixLine `--patch` and tweakcc rewrite `cli.js`; never for garnish. |

## 5. Ecosystem survey

Star counts, language and last push from the GitHub API on 2026-09-06.
"garnish ahead" lists what garnish already does that the project does not,
by its README.

| project | ★ | lang | pushed | license | what it does that garnish does not | garnish ahead |
|---|---|---|---|---|---|---|
| jarrodwatts/claude-hud | 27,837 | JS plugin | 2026-09-05 | MIT | tools/agents/todos lines from the transcript; `elementOrder`, `mergeGroups`, `rightAlign`; usage time formats; external usage snapshot; per-day cost ledger; provider label; jj; `CLAUDE_HUD_DISABLE`; per-`CLAUDE_CONFIG_DIR` overlay config; zh locales | exact width, no transcript on the tick, schema-generated docs, goldens, presets, animation |
| ccusage/ccusage | 18,379 | Rust (was TS) | 2026-09-05 | NOASSERTION | `statusline` subcommand: session/today/block cost, `$/hr` burn rate with color bands, 5-hour blocks, `--cost-source auto/cc/ccusage/both`, offline LiteLLM pricing, model label aliases; 18 agent CLIs | payload-first, no transcript scan, sub-3 ms |
| sirmalloc/ccstatusline | 12,782 | TS | 2026-09-03 | MIT | (mined in FUTURE-SPEC) | — |
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
| krayong/ccsidekick, vincent-k2026/codachi, refinist/ccstatusline-editor | 30 / 12 / 12 | TS | 2026-09-02 / 04-18 / 07-25 | MIT | (mined in FUTURE-SPEC) | — |
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

**Patterns across the market that FUTURE-SPEC should state as facts**

- Distribution: the plugin marketplace plus a setup skill is the norm for
  the top projects; single-binary Rust projects distribute through npm
  wrappers with platform binaries (CCometixLine, best-claude-hud), Nix
  flakes, or `curl | bash` installers with `--yes` for CI.
- Data: nobody but ccstatusline, ohugonnot and cship calls the usage
  endpoint; claude-hud and coralline explicitly refuse to ("never falls back
  to credential scraping or undocumented API calls") and solve idle-session
  staleness with local snapshot files instead.
- Performance claims cluster at "< 50 ms" (shell) and "≤ 10 ms" (cship);
  garnish's < 3 ms warm tick is the fastest stated budget found.
- Nothing found generates its docs from a schema, ships golden renders,
  or measures width exactly against the harness box.

## 6. Verify-first items, in order

1. **Notification row squeeze.** Classic renderer shares the row with
   notifications and the verbose token counter [D]. Render a full-width
   line with an active notification and note whether the harness truncates
   the status line or the notification; consider whether `padding` or a
   right-hand reserve is worth a config key.
2. **Autocompact position.** The docs say compaction runs at the model's
   context limit unless a window is set [D]; SPEC § 2.3 records a 13,000
   token buffer observed in the 2.1.260 binary. Re-check the constant in
   2.1.261+ and keep `compact_buffer_tokens` configurable either way.
3. **A1 dim reset on screen** (unchanged; strings re-confirmed).
4. **NBSP in VS Code** (unchanged); the mascot uses background-colored NBSP
   to stop "host trimming", which is a second data point that NBSP survives.
5. **`get_usage` cost**: time a `claude -p --input-format stream-json`
   round trip before proposing N10.
6. **`stats-cache.json` cadence**: when the harness rewrites it (startup,
   `/usage`, per turn) decides N2's TTL.
7. **`sessions/*.json` lifetime**: whether stale files linger after a crash
   (the harness has a `abandoned-stale` state [B]), which decides how
   `remote` and a session-name fallback (`name`, `nameSource`) validate.

## 7. Crate facts (crates.io, 2026-09-06)

| crate | version | note for FUTURE-SPEC § 20 / § 8.2 |
|---|---|---|
| ratatui | 0.30.2 | backends split out; `ratatui-crossterm` 0.1.2 supplies `CrosstermBackend`; `termina` 0.4.0 is a newer backend |
| crossterm | 0.29.0 | last release 2025-04-05 |
| inquire | 0.9.4 | the prompt-wizard crate for 7.3b; used by Claude-Code-Personalities and ccusage-statusline-rs |
| dialoguer | 0.12.0 | alternative to inquire; used by claude-code-statusline-pro |
| gix | 0.87.1 | pure-Rust git used by claude-powerline-rust; would replace `git::run_program`, which the crate map rules out, so record only |
| ureq | — | the blocking HTTP crate cship and best-claude-hud use; garnish's named choice stays `reqwest` |
| palette | 0.7.7 | not needed; OKLab is ~40 lines (Appendix A) |
| memmap2, simd-json | 0.9.11, 0.18.1 | claude-powerline-rust's transcript-scan tricks; irrelevant without a scan |

## Appendix A — OKLab matrices (Björn Ottosson, bottosson.github.io/posts/oklab)

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

The inverse cubes the intermediate values and applies the inverse matrices
(the `4.0767416621 …` matrix FUTURE-SPEC § 13.2 mentions is the LMS →
linear sRGB inverse). sRGB companding as in § 13.2. xterm-256 mapping in
§ 13.3 is the standard one: gray ramp `232 + round((v − 8) / 247 · 24)` for
`r = g = b` between 8 and 248, else `16 + 36·R + 6·G + B` with each channel
quantised to 0–5.

## Appendix B — Official facts with the version that introduced them

From the changelog page (`<Update label>` headings), for FUTURE-SPEC § 8.3
and SPEC § 2:

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
| `refreshInterval`; `workspace.git_worktree` | 2.1.97 |
| `effort.level`, `thinking.enabled` | 2.1.119 |
| stale remote-control status lines after resume (fix) | 2.1.128 |
| `context_window` counts are current context, not cumulative (fix) | 2.1.132 |
| multi-line output no longer drops rows when a line is over-wide (fix) | 2.1.141 |
| `workspace.repo` and `pr` | 2.1.145 |
| `COLUMNS` and `LINES` in the environment | 2.1.153 |
| footer hints restored for custom status line users (fix) | 2.1.169 |
| `footerLinksRegexes` | 2.1.176 |
| `subagentStatusLine` per-task `model` and `contextWindowSize` | 2.1.205 |
| `subagentStatusLine` per-task `effort` | 2.1.214 |
| status line running twice on resume (fix) | 2.1.216 |
| GitLab MR badge, `pr.kind` | 2.1.234 |
| `modelPricing`; pre-reset `rate_limits` percentage after idle (fix) | 2.1.243 |
| `spinnerTipsOverride` tip objects, `tipsFile`, `label` | 2.1.247 |
| `rate_limits.spend_limit`; `prompt_cache` | 2.1.251 |
| `prompt_cache.last_miss_cause`, `miss_causes` | 2.1.260 |
| `keybindingFlavor` deprecated (docs describe ≥ this version) | 2.1.261 |

Other documented facts used above: status line runs once at session start
and on resume, then on new assistant message, `/compact` finish, permission
mode change, vim toggle, `command` change (skips the debounce),
`refreshInterval`, a `resets_at` or `expires_at` passing; `hideVimModeIndicator`
suppresses the built-in `-- INSERT --` row; the line hides during
autocomplete, help and permission prompts; `CLAUDE_CODE_SHELL_PREFIX` wraps
status line commands; `CLAUDE_CODE_SAFE_MODE=1` skips a non-managed status
line; the `rate_limits` object appears only for Pro/Max subscribers or
behind a gateway with spend limits, after the first API response.

## Appendix C — Sources

Official (fetched 2026-09-06 from `https://code.claude.com/docs/en/…`):
`statusline`, `hooks`, `hooks-guide`, `settings`, `settings-reference`,
`plugins-reference`, `plugins`, `fullscreen`, `accessibility`,
`remote-control`, `voice-dictation`, `costs`, `context-window`,
`model-config`, `env-vars`, `claude-directory`, `sessions`,
`monitoring-usage`, `commands`, `changelog`, `agent-sdk/typescript`,
`whats-new/2026-w24..w34`.

Issue tracker (`anthropics/claude-code`, state on 2026-09-06): #28750
(closed, not planned; community root cause `wrap: "truncate"`), #27305
(closed; notification banners compress the line), #27864 (closed, stale),
#22115 (closed, completed: `COLUMNS`), #13585 (open: quota access), #16944
(closed, completed by a collaborator: subagent compaction docs), #27916
(closed: subagent count in the status line), #31021 and #31637 (closed, not
planned: usage endpoint 429), #26096 (closed, completed: `added_dirs`).

Binary: `strings` of Claude Code 2.1.261 (dump taken 2026-09-05), patterns
`get_usage`, `hideVimModeIndicator`, `subagentStatusLine`,
`prefersReducedMotion`, `bridgeSessionId`, `weekly_scoped`, `model_scoped`,
`tipsFile`, `Status line command skipped`, `var Vne=2,Gne=1`,
`flexWrap:"wrap"…paddingX`, `isNarrow` (absent).

Primary: git manual (`GIT_OPTIONAL_LOCKS`, `--no-optional-locks`);
Ottosson, *A perceptual color space for image processing* (OKLab);
crates.io API; live probes of `status.claude.com/api/v2/{status,incidents}.json`.

Community: READMEs and metadata of the 31 repositories in § 5 via the
GitHub API; ccusage.com/guide/statusline.
