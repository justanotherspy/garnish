# garnish — Product Requirements & Technical Specification

Owner: Daniel Schwartz. Builder: Claude. This is the target design of the
whole system and the Claude Code contract it depends on; when the design
changes, it changes here first, with the reason (`CLAUDE.md` § Phase
protocol). Everything here is implemented or named as the open phase in
`PLAN.md`. Progress lives in `PLAN.md`; dated decisions, reviews and
history in `WORKLOG.md`.

## 1. Purpose

`garnish` is the `statusLine.command` for Claude Code. Every second (and on
every harness trigger) Claude Code pipes a JSON snapshot of the session to
the command and displays whatever it prints. garnish turns that snapshot
into a small, information-dense dashboard of independent modules, laid out
by a TOML config, so cheaply that dozens of concurrent sessions on one host
do not notice it.

### Goals

- **Fast**: a warm tick averages < 3 ms (p99 < 8 ms) in release; cold < 30 ms.
- **Never blocks**: anything slow (git ahead/behind, dirty state, optional
  `git fetch`) runs in a detached worker; the tick renders cached data.
- **Composable**: 25 modules plus static text modules, any of them on any
  line, each with `minimal` / `default` / `full` presets; lines are columns
  of modules or stacks, with titles and boxes (§ 4.3).
- **Beautiful**: Nerd Font glyphs, smooth gradient bars, framed lines, named
  colour themes, OSC 8 links.
- **Documented from code**: module docs are generated from each module's
  option schema.
- **Tested exhaustively**: real-binary tests over payload fixtures, temp git
  repos, PATH shims, a frozen clock, and a hyperfine latency gate.

### Non-goals

- No generic/plugin modules; the set is fixed (text modules are static
  strings, never commands or files, § 3.7).
- No network calls (PR state comes from the payload).
- No Windows. Linux and macOS only.
- No daemon. Workers are one-shot detached processes.
- The tick writes nothing but its own cache and debug log; nothing reads
  the transcript. (FUTURE-SPEC lists proposals that would lift these; each
  is Daniel's decision, none is taken.)

## 2. Claude Code contract

Verified against docs (code.claude.com/docs/en/statusline) and v2.1.261;
each binary fact names the version it was read in. Minimum supported Claude
Code: **2.1.251** (adds `prompt_cache`, `effort`). `CLAUDE.md` says how to
re-verify each fact after an upgrade.

### 2.1 Settings

```json
{ "statusLine": { "type": "command", "command": "garnish", "refreshInterval": 1, "padding": 0 } }
```

`refreshInterval` minimum is 1 s. The harness also re-runs the command on
session start, assistant message, `/compact`, permission-mode change, vim
toggle, `command` change, a rate-limit `resets_at`, a prompt-cache
`expires_at`. Updates are debounced at 300 ms and **an in-flight script is
cancelled when a new trigger fires**, so anything slow must survive the
tick being killed. The script gets `COLUMNS`/`LINES`; output may have
multiple lines, ANSI colours and OSC 8 links.

**Width.** The status line sits in the footer box (2 cells of padding each
side), inside a box padded by `statusLine.padding` each side. Each row is an
Ink `<Text wrap="truncate">`, so a row wider than
`COLUMNS − 4 − 2 × statusLine.padding` is cut with `…` on the right (2.1.261:
footer `paddingX: 2`, status box `paddingX: padding`). garnish renders to
exactly that width: 4 is always subtracted, and the top-level `padding` key
supplies `2 × statusLine.padding`.

**Trim.** The harness trims stdout, then trims every row and drops rows left
empty (2.1.261: `stdout.trim().split("\n").flatMap(l => l.trim() || [])`),
drawing each row's *trimmed* text. The trim sees raw bytes, escapes
included (2.1.263), so a row is lost only when it is whitespace *after
painting*: an unframed spacer with colour off (`color = "never"`,
`NO_COLOR`) vanishes, while with colour on the rule's colour codes keep it.
`preview --color never` shows the row the screen drops; `blank = true`
(§ 4.1) keeps it.

A row that *starts* with whitespace would be drawn shifted left (a column's
padding line, a `style = "none"` box's pad, spaces placing a module under a
frame with no caps, with colour off the unstyled rule of `style = "none"`;
a plain segment carries no escape even with colour on). The tick holds
those cells, once, on the painted row, never in the layout: with colour on
the row starts with an empty SGR (`ESC[0m`), which the trim keeps and the
harness's escape parser drops; with colour off its first leading space
becomes the braille blank U+2800 (§ 4.1). A row that is whitespace
throughout is left to the spacer rule; trailing whitespace moves nothing.

**Every row is drawn dim by the harness, and nothing in the output can undo
it** (2.1.261, 2.1.270). Each row is `<Text dimColor wrap="truncate">`
around a child that parses the row's escapes into per-piece style props
(colour, bold, dim, italic, underline, strikethrough, inverse, OSC 8 link);
the parent's `dim` is merged into every piece, and a piece can add a style
but never clear one (`ESC[0m` clears only what the parser tracks after it).
**`preview` and the `setup` pane (§ 14) draw rows the same way**: the
painter folds SGR 2 into every segment (`Painter.dim`; the pane sets
ratatui's `DIM` on every span). `--color never` stays plain, and the tick
never adds the dim itself: the goldens pin a tick's bytes.

**Height.** `LINES` is the whole terminal's row count (2.1.270: copied from
`process.stdout.rows`; the renderer reads the same number, falling back to
24 and clamping at 2048). The status line draws every row, one `<Text>` per
row with no cap, and nothing between it and the screen (footer, composer,
REPL slot) has a height, `maxHeight` or `overflow`. What a tall
status line does depends on the renderer (read in the 2.1.270 binary, not
watched). Chosen in order: screen-reader mode, tmux `-CC`, Windows over
SSH, `CLAUDE_CODE_NO_FLICKER=0`, `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN` and a
crash auto-off force *classic*; a background session and
`CLAUDE_CODE_NO_FLICKER=1` force *fullscreen*; then the chain's `tui` key
(`"default"` or `"fullscreen"`, shown and set by `/tui`); unset, a new
install starts fullscreen for its first sessions and default-off
server-side gates decide after that, classic otherwise. *Split* needs
`CLAUDE_CODE_DECSTBM` or a server flag and yields to fullscreen.

- *classic*: nothing is cut. Laid out with a width constraint only; once
  taller than the terminal its top scrolls into the scrollback and the
  bottom `LINES − 1` rows stay in view, status line last, prompt box above
  it while both fit (a taller block scrolls the prompt box's top away; a
  row scrolled off is never redrawn). Renders are cell diffs; visible rows
  are erased and redrawn on a resize, a forced reset, or a frame that
  shrinks back into the screen, by more than a screen, or changes a row
  scrolled off; the scrollback is never touched. Every row costs a row of
  transcript.
- *fullscreen* (alternate screen): the bottom block (prompt box with notices
  and permission wait, status line, hint line and, when shown, inline
  panes, artifact panel, background-sessions line; suggestions float above)
  sits in a box of `maxHeight = ⌊LINES / 2⌋` (`LINES − 2` while a history
  search or elicitation overlay is open) in a root `LINES` tall; anything
  below the root is dropped. The box has no `overflow` and top-aligns its
  children (Yoga's default `justifyContent`), so its last rows go first:
  the hint line, then the status line from the bottom up. With an empty or
  one-line prompt and no notice, the prompt box's three rows, the
  composer's margin and the hint line take five, leaving the status line
  `⌊LINES / 2⌋ − 5` whole rows (7 at 24, 20 at 50), the most it ever keeps.
  The prompt shows up to `max(3, ⌊LINES / 2⌋ − 5)` draft lines, so a longer
  draft takes the hint line and then the status line's last rows until
  sent; a notice or the permission wait takes its height the same way.
- *split* (DECSTBM): the bottom block is bounded to `LINES − 2` rows, the
  transcript keeps two, overflow is cut from the bottom.

The script is told nothing about the renderer, so the tick caps nothing and
prints every configured row; the tools apply the fullscreen budget: `doctor`
prints `tui` (§ 7), the § 14 picker warns when a preset's rows exceed
`⌊LINES / 2⌋ − 5`, and a multi-line row (§ 4.3) has no height cap. Claude
Code's schema takes only the two `tui` names: another value is dropped from
the managed file alone and makes any other file rejected whole; `doctor`
says which.

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
| `version` | string | Claude Code version; the `version` module (§ 3.8) shows it |
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

Auth-mode rule (`Payload::is_subscription` = `rate_limits.is_some()`):
`rate_limits` present with any window (a gateway's `spend_limit` alone
included) ⇒ subscription (limit modules show, `cost` hides under its default
`only_without_rate_limits = true`); absent ⇒ API key, or a gateway with no
spend limit (show `cost`). Whether a spend-only gateway session should also
show `cost` is open in PLAN's backlog.

garnish does not model `prompt_id`, `transcript_path` or
`remaining_percentage` (= `100 − used_percentage`); other fields no module
reads yet are parsed and say so in `payload.rs`. An empty `cwd` or
`workspace.current_dir` is no directory: the other is used.

### 2.3 Autocompact threshold (approximation)

Not in the payload. From the 2.1.260 binary (unchanged in 2.1.261, 2.1.270):
`threshold = effective_window − 13_000`, or
`min(floor(window × pct / 100), window − 13_000)` when
`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` is set. `effective_window =
min(context_window_size, configured)`, `configured` from
`CLAUDE_CODE_AUTO_COMPACT_WINDOW` (env) > `autoCompactWindow` in settings
(managed > `.claude/settings.local.json` > `.claude/settings.json` >
`~/.claude/settings.json`, which `CLAUDE_CONFIG_DIR` moves to
`$CLAUDE_CONFIG_DIR/settings.json` as it moves every `~/.claude` path; empty
is unset, § 5) > model default (= window). `autoCompactEnabled = false`,
`DISABLE_AUTO_COMPACT=1` or `DISABLE_COMPACT=1` (no compaction at all)
disables the marker. The buffer is `modules.context.compact_buffer_tokens`.
This ordered list of files is "the settings chain" below.

## 3. Modules

Every module has `enabled` (bool), `preset` (`minimal|default|full`),
`refresh` (seconds a cached module's value lives before a worker refreshes
it, ≥ 1; a payload-only module takes only `0`, any other value reported as
having no effect), `icons.<key>`, `colors.<key>`, `label`, `prefix`,
`suffix`, `hide_when_empty`, `hide`, `max_width`. Resolution, weakest first:
built-in default → icon-set default → the module preset the top-level
`preset` implies → the module's own `preset` → explicit key.

`max_width`: `0` (default) is unlimited and skips the measurement; otherwise
the decorated module (`label`, `prefix`, `suffix`, stale marker) is cut to
that many cells with `…` via `ansi::truncate` (grapheme-aware; a cut link
still wraps what is left) *before* alignment and any column or line cut
(§ 4.3). Capped at 1024 like every cell count (§ 5). Text modules use
`width` instead; `config check` reports a `max_width` there and names
`width`. The common options (this, `label`, `prefix`, `suffix`,
`hide_when_empty`) are `config::schema::COMMON_OPTS` specs, bounded and
documented like a module's own.

`hide = [...]` (default `[]`) lists states in which the module leaves its
row: `empty` (what `hide_when_empty` hides), `zero` (a count or amount that
is zero: `cost` at `$0.00`, `lines` at `+0 −0`, `sync` at `⇡0 ⇣0`),
`below:N` / `above:N` (`N` ≤ 1000) for a percentage (`context`, `limit5h`,
`limit7d`, `spend`, `cache`'s hit ratio, `api`'s share whether or not
`show_share` prints it). Both compare the number as printed: a percentage
rounded by its `percent` style (§ 4), an amount as shown (`$0.00` at two
decimals and `$0` under `cost = "whole"` are zero, `$0.004` at three is
not). Accepted states follow the schema's *measure* (count, amount,
percentage, none): every module takes `empty`, a text module nothing else;
`config check` names the accepted states when refusing one. The list and
`hide_when_empty` are a union; `hide_when_empty` and `lines.hide_zero` stay
as older spellings of `empty` and `zero`. A hidden module rendered nothing
(§ 4: not a column, its row dropped under `hide_empty_rows`), never `–`.
The rule is applied once, in the render loop, from the measure a module
attaches to its output; parser, reference and `setup` form take the
vocabulary from the measure.

### 3.1 Repo group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `path` | base dir (git toplevel, else `project_dir`) + cwd subpath | base name | `~/parent/base` + dim `/sub` | full tilde path + subpath + `added_dirs` count | 0 (toplevel cached) |
| `branch` | branch or detached HEAD | name | icon + name | + short SHA, dirty `✱` | 5 |
| `sync` | ahead/behind vs `@{upstream}` | `⇡2 ⇣1` when non-zero | colored counts, the `no_upstream` glyph (`⊘`) when the branch has none, the fetch-age hint (`stale` glyph, a space, the age: `↻ 12m`, § 4.1) | + upstream name (`origin/main`, or the branch name alone when the upstream is a local branch) | 5 (+ opt-in `fetch_interval`) |
| `worktree` | `workspace.git_worktree` / `worktree.name` | name | icon + name | + `original_branch → branch` | 0 |
| `pr` | open PR/MR | `#123` linked | icon + `#123` linked + state glyph | + state word | 0 |

GitLab merge requests render as `!7` with the `mr` icon, GitHub PRs as `#42`.
PR state glyphs/colours: approved `✓` ok, pending `❍` warn,
changes_requested `✗` danger, draft `❏` muted (unicode set; nerd uses nf-fa,
see `docs/modules/pr.md`). The number is linked to `pr.url` and underlined
only when the painter will emit the link (§ 5).

`sync`: a **gone** upstream (configured, but its tracking ref no longer
exists) is no upstream: the worker records an `ok` entry marked `gone`
without counting, and the row shows the `no_upstream` glyph, never `✗`. The
fetch-age hint counts from the last fetch that *worked*: a non-empty
`FETCH_HEAD` or the worker's `fetch_ok_at`, whichever is newer, across the
worktree's and the common git dir's `FETCH_HEAD` (git truncates it before
contacting the remote); a future stamp has no age.

- `path`'s `style = "full" | "fish"`: `fish` abbreviates every base-part
  directory but the last to its first character (`~/r/g/src`). A leading
  `~` is not a segment; the last segment is never abbreviated; a
  dot-directory keeps dot and letter (`.config` → `.c`); the first character
  is a terminal cluster (a combining mark, skin tone or flag half stays
  with its base); a segment that would read `.`, `..` or empty stays whole;
  a root or one-segment path is untouched. `depth` (last `N` segments, `0`
  = all; preset defaults 1, 2, 0) applies first and keeps the `~`
  (`depth = 2`, `fish`, `~/repos/garnish/src` → `~/g/src`). The subpath
  stays dim and whole.
- `branch`'s `link = false`: `true` links the name to
  `https://<host>/<owner>/<name>/tree/<branch>` from
  `workspace.repo.{host,owner,name}` (`/-/tree/` when the host names GitLab
  or `pr.kind = "mr"`, a host naming GitHub winning), no git call; no link
  without `repo`, with an incomplete one, an empty branch or a detached
  head. Underlined only when linked. Path parts are percent-encoded (RFC
  3986 unreserved and `/` kept, else `%XX` of UTF-8 bytes; a `.`/`..`
  segment's dots encoded too), so § 5's rule holds; the URL carries the
  whole name even when `max_length` cut the visible one. The host is used
  verbatim when it is an authority (letters, digits, `-`, `.`, optional
  `:port`), else no link. **Known limitation**: a self-hosted GitLab not
  named after it with no open MR looks GitHub-shaped and gets `/tree/` (a
  404); a `branch.forge` override is in PLAN's backlog.

Built-in glyphs are one cell in every terminal: `○`, `◌`, `●` (Geometric
Shapes / East Asian Ambiguous, two cells in COSMIC Terminal) are excluded,
the shipped ones are `❍`, `❏`, `✱`, `✦`/`✧`, and
`built_in_glyphs_have_one_width_in_every_terminal` rejects the block.

### 3.2 Model group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `model` | `display_name`, `⚡` when fast | name | icon + name (+⚡) | + `model.id`, thinking glyph | 0 |
| `effort` | `effort.level` | word | icon + scale `▁▃▅▇█` | scale + word | 0 |
| `context` | bar (100% = window) + % + compaction marker | `42%` | bar(20) + `42%` | bar(30) + `42%` + marker label + window tag + `exceeds_200k` | 0 (the settings chain is read at most once per tick, and only when the marker, its label or `scale = "usable"` needs it) |
| `style` | `output_style.name` | name unless default | icon + name unless default | always | 0 |

Context bar: `█` with partial blocks for sub-cell precision, empty `░`; the
**filled part** takes the current band's colour (`thresholds = [50, 75,
90]`, `band_colors = ["band1", "band2", "band3", "band4"]`, the theme's four
band roles, overridable by role or literal); the band is the number of
thresholds reached. The percentage is drawn in `colors.percent` (`text`). A
`▏` marker sits at the autocompact position; `exceeds_200k = true` shows
`icons.exceeds` (`‼`) in `colors.exceeds` (`danger`) when the payload says
so; `warn_at` adds a badge threshold. No token counter. `used_percentage`
null → empty bar and the placeholder (§ 3.6). A `thresholds` list out of
ascending order (here and on usage modules) is reported and the default
stands in.

`scale = "window" | "usable"` (default `window`): `usable` measures bar and
percentage against the § 2.3 threshold (`used_percentage × window ÷
threshold`, capped at 100); the marker then sits at the bar's end and is not
drawn, nor its `⤓` percentage (`show_compaction_percent`); the `full`
window tag still names the real window. With compaction disabled (§ 2.3) or
a threshold below a tenth of the window, `usable` falls back to `window`
silently (settings change under a running session). `compaction_marker`
governs drawing only. `thresholds` and `warn_at` follow the displayed
percentage.

### 3.3 Usage group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `limit5h` | 5-hour % + reset countdown | `23%` | icon + `23%` + `⏱2h13m` | + mini bar | 0 |
| `limit7d` | 7-day % + reset countdown | `41%` | icon + `41%` + `⏱3d4h` | + mini bar | 0 |
| `spend` | spend-limit % | `62%` | icon + % + reset | + bar, danger > 100 | 0 |
| `cost` | `total_cost_usd` | `$1.23` | icon + `$1.23` | + `+156 −23` | 0 |

Limit modules render nothing when their window is absent. `cost` has
`only_without_rate_limits = true`, so one usage line serves both auth modes.

`reset` on the limit modules: `countdown` (default); `absolute`, the reset
instant, coarser the further off (`limit5h` `⏱14:30`, as steady as
`durations = "fixed"`; `limit7d` `⏱Tue 14:30`; `spend` `⏱Mar 1`, no zero
padding); `both`, countdown then that form in parentheses (`2h13m (14:30)`,
`27d8h (Mar 1)`, the countdown in the module's `durations`); `elapsed`
(below). `show_reset = false` hides every form, as does a passed instant.
Times use jiff in the tick's local zone (§ 3.4). The harness re-runs the
line at each `resets_at`, so no form goes stale at the boundary.

On `limit5h` and `limit7d` only (known lengths 5 h, 7 d; `spend` takes none
of these), the *elapsed* share is `1 − (resets_at − now) ÷ length`, clamped
to the window. All five keys are off by default (`full` included), and a
window whose reset has passed prints none of them.

- `pace = true`: used share minus elapsed share after the percentage, `⇡14%`
  (ahead, `colors.ahead`, `hot`) or `⇣32%` (behind, `colors.behind`, `ok`);
  zero prints `0%` in `behind`. Arrows are `sync`'s, overridable as
  `icons.ahead`/`icons.behind`.
- `pace_colors = true`: the percentage is coloured by `used ÷ max(elapsed, 1
  %)` instead of `thresholds`: ≤ 1 *nominal* (`colors.pace_nominal`, `ok`),
  ≤ 1.5 *caution* (`pace_caution`, `warn`), above *critical*
  (`pace_critical`, `danger`); below 20 % used the threshold bands stand,
  above 80 % used it is critical whatever the ratio.
- `eta = true`: after the pace, the time until 100 % at the current rate,
  `elapsed × (100 − used) ÷ used` (`⇥ 1h37m`, `icons.eta`, `colors.eta`, the
  module's `durations`), only when that lands before the reset.
- `reset = "elapsed"`: time into the window over its length (`⏱2h46m/5h`,
  `⏱3d20h/7d`), behind `show_reset`, following `durations`.
- `elapsed_marker = true`: the module's `marker` glyph (`▏`; one cell, may
  be blank, as `context`'s) on the mini bar at the elapsed share; needs
  `bar_width > 0`.

### 3.4 Session group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `session` | `total_duration_ms` | `1h12m` | icon + `1h12m` | + start time | 0 |
| `api` | `total_api_duration_ms` | `8m20s` | icon + `8m20s` | + `(11%)` of session | 0 |
| `cache` | prompt cache | `91%` | icon + `91%` + TTL badge + `✦ 47m`/`✧` warm countdown | + misses, writes | 0 |
| `clock` | local time + spinner | `HH:MM` | spinner + `HH:MM:SS` | + date, UTC offset | 0 |

`cache` hit % = `prompt_cache.hit_ratio`, else the last request's cache-read
share from `current_usage`; `prompt_cache` absent → placeholder (§ 3.6).
Spinner frame = `now_secs mod frames.len()`.

The tick's local zone is `TZ` when it names a zone, else `/etc/localtime`,
else UTC. `TZ` and `clock`'s `tz` are read as the C library reads `TZ`: a
POSIX rule (`JST-9`, `EST5EDT,M3.2.0,M11.1.0`) is that rule; anything else,
or after a leading `:`, is an absolute TZif path or a zone name read from
`TZDIR`, `/usr/share/zoneinfo`, `/usr/share/lib/zoneinfo` or
`/etc/zoneinfo`. A name with a `..` component or no file matches nothing; a
relative one is never read against the working directory. Only a name no
directory has goes to jiff's database (whose first use walks the zoneinfo
tree). A `TZ` naming nothing is reported once on stderr. `clock`'s `tz` is
resolved once, at config read; one naming nothing is reported by `config
check` under `modules.clock.tz` and the tick's zone stands in.

### 3.5 Session-identity group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `session_name` | `session_name` (absent → hidden) | name | icon + name | + short `session_id` | 0 |
| `vim` | `vim.mode` (absent → hidden) | `N`/`I`/`V`/`VL` | colored badge | + icon | 0 |
| `agent` | `agent.name` (absent → hidden) | name | icon + name | + thinking glyph | 0 |
| `lines` | lines added/removed | `+156 −23` | icon + colored `+156 −23` | + net delta | 0 |

### 3.6 Staleness

A cached module past its TTL spawns a worker and keeps rendering the last
value normally. Only once older than `stale_after` TTLs (default 5: 25 s for
a 5 s module) is it *overdue*, dimmed with a trailing `⟳`; an entry computed
for another situation (branch or upstream changed) is overdue at once. A
failed refresh renders dimmed with `✗`, the error kept in the cache file for
`garnish doctor`. A missing entry renders the placeholder.

The placeholder (nothing to show under `hide_when_empty = false`, a failed
module before its `✗`, `context` before the first response, `cache` without
a ratio) is `–`, `-` in the ascii set, whose marks are all 7-bit (ellipsis
`..`, overdue `~`, failed `x`). An ascii row's one non-7-bit character is the
braille blank U+2800 of § 2.1 and § 4.1, which is no mark (the space, the
only 7-bit blank, is trimmed away).

### 3.7 Text modules

The 25 built-in modules (§ 3.1–3.5, § 3.8) are the only ones that read the
payload, a settings file or the cache, or run anything. **Text modules** are
the one user-defined kind: a fixed string in a box of configurable width,
declared under `[modules.text.<name>]`, placed as `text.<name>`, any number
of them; they run nothing, read no file, touch no cache.

```toml
[[line]]
modules = ["path", "text.motd"]
right   = ["text.tag", "clock"]

[modules.text.motd]
text     = "ship it before lunch, then write the docs"
width    = 12             # cells; 0 = the text's own width
pad      = 1              # extra cells added on each side of the box
justify  = "left"         # left | right | center: where short text sits in the box
overflow = "scroll"       # clip | scroll | scroll-wrap: text wider than the box
step     = 1              # cells per tick (0.5 = every second tick)
gap      = "   "          # scroll-wrap only: text between the end and the start
color    = "accent"       # role or literal, shorthand for colors.text; label/prefix/suffix apply

[modules.text.tag]
text  = "v0.2"
color = "muted"
```

- **Names.** `<name>` is a bare key (letters, digits, `_`, `-`); anything
  else is rejected. No `icons` table, no `preset`, no `refresh` (rejected).
- **Box.** `width = 0` is exactly the text's width; otherwise `width` cells,
  plus `pad` blank cells each side; `justify` places narrower text. The
  rendered width is constant.
- **Overflow.** `clip` cuts with the ellipsis. `scroll` is a `width`-cell
  window moving `step` cells left per tick, restarting once the end has
  passed. `scroll-wrap` is the ticker: text, `gap`, text. Both stateless:
  offset `floor(now_secs × step) mod period`, the period the text width
  (`scroll`) or text plus gap (`scroll-wrap`), counted cluster by cluster as
  the scroller advances (`ansi::scroll_period`; a ligature one cell wide,
  Arabic `لا`, is two clusters), as the line ticker and the setup placement
  map count. One scroller in `ansi.rs` also drives the line ticker (§ 4.1).
- **Escapes.** `text` is plain: ANSI/OSC stripped, control characters removed.
- **Links.** `url = "https://…"` wraps the box in an OSC 8 link on every
  segment of the finished box (a scrolled window's cut cells, a clip's
  ellipsis, the `justify` fill), never on `pad` cells. § 5's rule is checked
  at config time; a failing URL is reported and dropped.
- **Docs and checks.** `garnish modules` lists `text.<name>` as a family
  with one reference page; `config check` validates `justify`, `overflow`,
  `step` (0.001–1000, like every `*_step`, § 5) and that every `text.<name>`
  on a line has a table.

### 3.8 Harness identity and settings badges

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `version` | the payload's `version` | dim `v2.1.270` | dim `v2.1.270` | icon + dim `v2.1.270` | 0 |
| `sandbox` | `sandbox.enabled` in the settings chain | glyph | glyph | glyph + `sandbox` | 0 (the settings chain, read at most once per tick) |
| `voice` | `voice.enabled` in the settings chain | glyph | glyph | glyph + `voice` | 0 (the same read) |
| `account` | `oauthAccount.emailAddress` from `~/.claude.json` | the part before `@` | icon + the email | icon + the email | 600 (a worker; the tick reads its cache entry) |

- `version`: nothing without a payload version (`hide_when_empty = false`
  shows `–`); a leading `v` is not doubled; `show_icon` off except `full`.
- `sandbox`, `voice`: glyph badges, shown only when the first chain file
  that sets the key sets `true` (resolved as `prefersReducedMotion`, § 4.2);
  `style = "glyph" | "word"` adds the word. They share the tick's one
  settings read; `doctor` lists both keys (§ 7). `voice` exists because the
  harness hides its own voice hint under a custom status line.
- `account`: the one cached module outside the repo group. Its worker
  (`garnish refresh --module account`, § 6; session-scoped, 600 s TTL)
  reads `$CLAUDE_CONFIG_DIR/.claude.json` when that is set and non-empty
  (§ 5), else `~/.claude.json`, up to 8 MiB, and stores the email; the file
  can be hundreds of KB, so the tick never parses it. Absent file → `ok`
  entry, no email (never `✗`); unreadable, unparsable or oversized →
  failed entry (`✗`, retried per TTL). An unparsable file is re-read once
  after 100 ms first (Claude Code 2.1.282 rewrites it in place when its
  rename fails, e.g. on a bind mount). The field name is
  community-documented; a file without it shows nothing. `style = "email" |
  "user"` picks the address or the part before `@`. The settings chain
  honours `CLAUDE_CONFIG_DIR` for its user file too.
- None of the four is in a built-in preset's rows; add them by hand or via
  the `session-badges` gallery preset (§ 12). Generated pages show
  `sandbox` and `voice` on from keys the pinned clock seeds (§ 9) and
  `account` with a note that the worker fills it in.

## 4. Configuration

Location: `--config` > `$GARNISH_CONFIG` > `$XDG_CONFIG_HOME/garnish/garnish.toml`
(default `~/.config/garnish/garnish.toml`) > `~/.garnish.toml` > built-in
defaults. Re-read every tick; no daemon.

- **Writing.** `config init`, `setup` and `install`'s default file write the
  file this order finds, the XDG path only when none exists, so a new file
  never hides a `~/.garnish.toml`.
- **Relative paths.** `GARNISH_CONFIG` counts only when absolute (Claude
  Code passes an `env` value unexpanded, so `"~/g.toml"` is relative): the
  tick ignores a relative one, a hand command refuses it. A relative
  `--config` on the tick is ignored the same way (`sh` leaves the `~` of
  `--config=~/g.toml`), with a `⚠ config:` row; hand commands resolve it in
  their own directory.
- **The status line command.** Without `--config` and `GARNISH_CONFIG`, the
  garnish `statusLine.command` names the file its ticks read: its
  `--config`, else a `GARNISH_CONFIG=` assignment before the program, else
  the `GARNISH_CONFIG` of a settings `env` block (the first the chain sets).
  The command is the one Claude Code runs from the current directory (the
  first chain file, § 2.3, that sets one, as `doctor` shows), for every
  hand command: `config path` prints the file, `config check`, `config
  show`, `preview` and `doctor` read it, `config init` and `setup` write it.
  When that command runs another program, the person's own garnish command
  (managed or user file's) names their config, with their own files' `env`
  alone.
- **Managed layer.** The managed file and, when it is the platform's, the
  `managed-settings.d/*.json` drop-ins beside it, applied above it (last by
  name first; a file or a link only; a hidden `.name.json` is off, as
  Claude Code skips it). The hook's file (§ 9) replaces the managed file
  alone, with no drop-ins (a hook file in a shared directory would take
  anyone's). `doctor` lists drop-ins as `drop-in` and says the keys a tick
  reads (§ 2.3) come from none of them (PLAN backlog).
- **`env` values** of any JSON type count, as JavaScript's `String()` spells
  them (`["/p"]` → `/p`, `5` and `5.0` → `5`, `null` → `null`); a number
  halfway between two shortest forms, or one serde_json reads a bit off,
  may differ in the last digit, and a file serde_json refuses (`1e999`) is
  skipped (PLAN backlog). An empty value names no file.
- **Whose file.** A settings file is the person's own when it is the
  managed file or lies in their settings directory (`CLAUDE_CONFIG_DIR`,
  else `~/.claude`, and `~/.claude` itself; by path or link target); any
  other is a checkout's. A config a checkout's file names (by `--config` or
  a `GARNISH_CONFIG=` prefix) is followed by no command, reading or
  writing: they refuse as for a value naming no one file (below), and
  `doctor` says so and shows what the lookup finds, never the refused file.
  A checkout command passing no config leaves the file to the lookup.
- **Inside a Claude Code session** (`CLAUDECODE` set, even empty) the
  environment carries every read settings file's `env`, a checkout's
  included, so `GARNISH_CONFIG` counts only when the person's own settings
  (user file, platform managed file, a drop-in) set that value, likewise
  `GARNISH_MANAGED_SETTINGS` (else the platform's managed file stands, in
  `doctor`'s chain too). Outside a session, a `GARNISH_CONFIG` that the
  current directory's checkout files set is refused too.
- **`install`** takes the command from the file it rewrites (`--settings`,
  else the user file) and its `env` value from the managed layer as the
  chain reads it (a managed layer a checkout pointed the hook at names
  nothing, whether that checkout is the current directory or the rewritten
  file with the other file of its `.claude` pair, found as written and once
  resolved), else from that file. It keeps that config, writing the default
  there when missing and checking its `padding` against it, but writes no
  config a non-own `--settings` file names (and says why). `setup
  --install` says when the command it keeps reads another file than the one
  it wrote.
- **Reading settings.** Hand commands read a settings file whole (within 64
  MiB), not under the tick's 1 MiB cap. The tick and workers never read
  settings for this: the harness passes `--config` and the tick passes it on.
- **Shell words.** The command is split as `sh` would: at space, tab or
  newline only; an unquoted leading `~`, and one `$HOME` or `${HOME}`
  anywhere, is the home directory with the rest glued on (`$HOME.x` is
  beside it, not in it); a program word the shell would split runs no
  garnish.
- **Names no one file.** A relative path, a second `$HOME`, an unquoted
  `$HOME` when the home holds a blank or glob character, a `~` in a
  `GARNISH_CONFIG=` value, arguments clap refuses (a second `--config`, one
  with no value, anything after `--`), or any other expansion is never
  guessed: `install` writes no default config and says why, `doctor` says so
  and shows the lookup's file, the others refuse with one line asking for
  `--config`, the value cut to 200 characters.
- **Path variables.** Empty is unset (§ 5). A relative `XDG_CONFIG_HOME`,
  `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR` (§ 6), `CLAUDE_CONFIG_DIR` or
  `GARNISH_MANAGED_SETTINGS` is ignored, as the XDG spec says: it would name
  a file in the session's repository.

```toml
preset = "default"        # default | minimal | full | compact
icons  = "nerd"           # nerd | unicode | emoji | ascii
theme  = "garnish"        # garnish | catppuccin-mocha | nord | dracula | tokyonight | mono
color  = "auto"           # auto | always | never | 256 | truecolor
truncate = true           # cut the left group when a line overflows; the right group only when it alone is wider than its column
stale_style = "dim"       # dim | hide | plain: how overdue cached values are shown
stale_after = 5           # TTL periods a value may be overdue before it is styled stale (≥ 1)
padding = 0               # extra cells subtracted from the width, on top of the harness's 4; set 2 × statusLine.padding
align = false             # pad each module column to the widest module in it across lines, so separators line up
right_justify = "end"     # end | start: where a padded right-group module's text sits (§ 4.1)
hide_empty_lines = true   # drop a line whose modules all rendered nothing; `modules = []` spacers stay (§ 4.1)
overflow = "truncate"     # truncate | ticker: cut or scroll a left group wider than the box (§ 4.1)
ticker_step = 1           # cells the ticker advances per tick (0.5 = every second tick)
ticker_gap = "   "        # text between the end and the wrapped-around start
# animate = true          # master switch for every animation (§ 4.2)
durations = "compact"     # compact (8m20s, 9m, 2h) | fixed (8m20s, 9m00s, 2h00m); fixed by default with overflow = "ticker" (§ 4.1)

[format]                  # number styles; each module that prints a kind has the same key with `inherit`
tokens  = "compact"       # compact (128k, 1.0M) | precise (128,400) | whole (128400)
percent = "whole"         # whole (42%) | precise (42.3%)
cost    = "precise"       # precise ($1.23, `cost.decimals` places; $1.2k from 1000) | whole ($1)
parens  = "plain"         # plain | dim: parenthesised details in the muted role

[colors]                  # role overrides: accent accent2 muted text ok warn hot danger frame band1..band4
accent = "#89b4fa"

[frame]
style = "rounded"         # none | rounded | square | double | heavy | powerline | custom
fill = true               # rule to the full width (§ 2.1) and close with the right cap
separator = " │ "
separator_color = "muted" # muted | inherit (the colour of the module before it) | a role or literal (§ 4.1)
# custom: first middle last single fill_char right_first right_middle right_last right_single separator pad
# boxes (§ 4.3): top_left top_right bottom_left bottom_right side
# animation (§ 4.2): fill_pattern fill_step fill_direction separator_frames separator_step

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

Top-level presets: `default` (the four lines above); `minimal` (one line,
frame `none`, all modules minimal: `path branch context limit5h cost` /
right `clock`); `full` (four lines, every module full); `compact` (two
lines: `path branch sync pr` / right `clock`; `model effort context limit5h
cost` / right `cache`).

**Layout rules.** Each group is joined by `separator`; the frame rule fills
the gap to `$COLUMNS − 4 − padding` (§ 2.1; never below 10, a floor on the
box before any § 4.3 gaps); the right cap follows. This two-group line is
the one-column case of § 4.3, whose unit is the **row** (`[[row]]`, alias
`[[line]]`; `hide_empty_rows`, alias `hide_empty_lines`); the rules here
apply inside each column. Overflow: drop the fill, then truncate the
**left** group (ANSI-aware, `…`); the right group is cut only when it alone
is wider than its column, after the left is gone. `preview --width` and
`GARNISH_COLUMNS` stand in for `$COLUMNS`, with the same subtraction.

**Aligned columns** (`align = true`; "column" here is a module's position in
its group, not a § 4.3 column): module *k* of a group, counted among modules
that rendered something (from the left in the left group, from the right
end in the right group), is padded with spaces to the widest module *k*
among lines that have a module after it: the left group pads on the right,
the right group on the left. A line's last module is never padded. With
`fill = false` the whole line is one sequence of columns aligned from the
left, and the last left module is padded when a right group follows.
Separators after column *k* then fall on the same cell in every line with
the same `separator`. Padding precedes truncation and fill; a value growing
by a cell moves the bars only if it was the column's widest.

**Durations**: `compact` prints at most two units, dropping a zero second
unit (`8m20s`, `9m`, `2h`, `3d4h`); `fixed` always prints two, the small
one zero-padded (`0m47s`, `9m00s`, `2h00m`, `3d04h`), so width changes only
when the large unit gains a digit or the pair changes (`59m59s` → `1h00m`).
Applies to `session`, `api`, the `cache` warm countdown, the limit resets
and the `sync` fetch age.

**Number formats** (`[format]`, defaults as in the example): `tokens`
`compact` (`12k`, `128k`, `1.0M`) | `precise` (`128,400`) | `whole`
(`128400`); `percent` `whole` (`42%`) | `precise` (`42.3%`); `cost`
`precise` (`$1.23`, `cost.decimals` places) | `whole` (`$1`), both `$1.2k`
from a thousand. A module printing a kind has the same key defaulting to
`inherit` (tokens: `context`, `cache`; percentages: the limits, `context`,
`cache`, `api`; money: `cost`), as `durations` works; module pages list the
kinds, and the key on a module printing no such number is unknown. `parens
= "dim"` draws parenthesised details (`api`'s share, `lines`' net, the
`both` reset's time) in the muted role, like a `label` (SGR 2 would not show
under the harness's dim, § 2.1); a detail is one segment when `plain`, two
when `dim`, decided in one helper. Bands and thresholds compare the printed
number.

### 4.1 Layout keys

```toml
[[line]]
modules = []              # an intentionally empty line: a blank framed row (spacer)
blank = false             # true keeps an unframed spacer on screen with one invisible cell
```

- **`right_justify`.** With `align`, a right-group module is padded to its
  column: `end` (default) pads on the left so text hugs the cap
  (`│          api  8m20s ─╯`), `start` on the right (`│ api  8m20s          ─╯`).
  The left group always pads on the right. Columns pair *positionally*, so
  a `–` under a wide bar gets a wide blank column; the guide says so.
- **Empty lines.** A line whose every module rendered nothing is dropped
  under `hide_empty_lines = true` (default); first/last caps follow the
  survivors; `false` keeps them. A line with `modules = []` and no `right`,
  or with no keys, is a *spacer*, always kept, drawn as an empty framed row
  (`├─ ────…────┤`). Under `style = "none"` (or a custom frame with empty
  caps) a spacer is whitespace and the § 2.1 trim applies (gone with colour
  off, kept by the rule's codes with colour on; unframed with `fill = false`
  it is empty either way). `blank = true` (default off) gives a row that
  would be whitespace one braille blank U+2800 (not whitespace to `trim`;
  drawn empty by a font with the spinner's braille) without changing its
  width (an empty row becomes that cell); a framed spacer gets none, and
  `blank` on a line with modules is reported. A non-list `modules`
  (`modules = "clock"`) is reported and the row is an ordinary empty line,
  never a spacer; so is a row whose `col`, or a column whose `row`, is a
  table instead of an array of tables (`[row.col]`; the row keeps its own
  `modules`). An unknown id is reported and removed, so `config show`
  writes only ids that render, leaves out a row or stack row its removal
  emptied wherever `hide_empty_rows` drops it, and any `[box.<name>]` none
  of its rows and columns names. Under `stale_style = "hide"` a line of
  cached modules can come and go; `hide_when_empty = false` on one pins it.
- **Ticker.** Under `overflow = "ticker"` a left group wider than its budget
  becomes a window advancing `ticker_step` cells left per tick and wrapping,
  `ticker_gap` (plain text, reduced at config time) between end and start;
  offset `floor(now_secs × ticker_step) mod (group width + gap width)`
  (§ 4.2). The right group never scrolls and is cut only when alone wider
  than its column. `truncate = false` hands the whole row over, ticker or
  not. With animations off (`animate = false`, `GARNISH_ANIMATE=0`) a ticker
  line is cut with `…`, not frozen at offset 0. It moves only as often as
  the harness ticks. The period is the group's *current* width, so a value
  changing width makes the window jump; hence under `overflow = "ticker"`
  the top-level `durations` defaults to `fixed` (by key, not motion: a
  frozen ticker still prints fixed timers); an explicit `compact` wins, and
  each timer module (`session`, `api`, `cache`, `limit5h`, `limit7d`,
  `spend`, `sync`) has its own `durations = "inherit" | "compact" |
  "fixed"`. `align` pads are inserted before the cut, so on a scrolling line
  they travel with the text and its columns do not stack with others.
- **Bars.** `util::bar` uses fractional-eighth blocks only with the `█`
  fill; other fills (`━`/`─`, `▰`/`▱`) have no partial cell, the workaround
  for fonts drawing `█` narrow; `bar = "blocks" | "line"` sets both glyphs.
- **Glyph sets.** Every `unicode` and `emoji` glyph must be one cell in the
  common terminals (COSMIC, Ghostty, Kitty, WezTerm, iTerm2, VS Code) or two
  by every table; sequences needing U+FE0F are banned from emoji (unit test).
- **Frames.** `powerline` pads its caps with one space by default.
- **Separators.** With `fill = false` the separator between groups is the
  line's own `separator`. `separator_color`: `muted` (default), a role or
  literal, or `inherit`: the colour of the first coloured, undimmed segment
  of the module before (an icon or value, never a `label` or `align` pad),
  else muted.
- **`sync`.** Zero counts shown by `show_zero` are muted; only non-zero
  counts carry ahead/behind colours. The fetch-age hint has a space between
  glyph and age.

### 4.2 Animation

Every animation is a pure function of the tick's clock: frame or offset =
`floor(now_secs × step) mod period`. No state, so a cancelled tick loses
nothing, sessions animate in step, and `GARNISH_NOW` freezes everything.
Cadence is the harness's tick; `step` below 1 slows it (0.5 = every second
tick).

```toml
# animate = true          # master switch; false freezes every animation at frame 0 (a ticker line is cut with … instead); unset, follows Claude Code's prefersReducedMotion

[frame]
fill_pattern   = "·  "    # repeated across the rule instead of fill_char
fill_step      = 1        # cells the pattern shifts per tick
fill_direction = "right"  # left | right
separator_frames = [" │ ", " ┃ ", " │ ", " ╎ "]   # cycle one frame per tick
separator_step   = 1

[modules.clock.icons]
spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]  # already a spinner; same rule

[modules.branch.icons]
branch_frames = ["⎇", "⑂"]  # any icon key accepts <key>_frames (one width); frame 0 when animations are off
```

- **Rule.** `fill_pattern` (one-cell glyphs) repeats across the gap and
  shifts `fill_step` cells in `fill_direction` per tick; the rule's width
  never changes (with `align` it starts at a fixed column). `fill_char` is
  the static case, used without a pattern and for a rule shorter than one
  period. `fill_pattern` with `fill = false` is reported as dead.
- **Separators.** `separator_frames` cycles one frame per tick; all frames
  must share a cell width (else rejected). `separator` is the static case;
  per-line `separator` overrides win over frames.
- **Glyphs.** Any icon key accepts `<key>_frames`, plain-text frames of one
  width (else rejected); frame `floor(now) mod n` replaces the icon. The
  `clock` spinner's built-in glyph is one-character frames cycled the same
  way; `spinner_frames` takes frames of any one width.
- **Scrollers.** The line ticker (§ 4.1) and text modules (§ 3.7) use a
  cell offset. Off, a text module sits at offset 0 in its box, while a
  ticker line is cut with `…`.
- **Off.** With animations off, patterns, separator and icon frames sit on
  frame 0.
- **Cost.** No I/O; pattern and separator frames are lookups; a module with
  icon frames costs one clone of its resolved config per tick (~0.5 µs;
  nothing without frames). Docs render under `Clock::fixed()`: frame 0.
- **Accessibility.** `animate = false` (or `GARNISH_ANIMATE=0` for a
  session) freezes everything at frame 0 and cuts a ticker line with `…`;
  the guide recommends it for screen readers and recordings. **Reduced
  motion**: when the settings chain (§ 2.3; first file setting the key
  wins) resolves `prefersReducedMotion` to `true`, garnish acts as if
  `animate = false` unless the config sets `animate`; the harness honours
  the same key. Precedence, strongest first: `GARNISH_ANIMATE=0`, explicit
  `animate`, `prefersReducedMotion`, default `true`. The chain is read only
  when the answer depends on it, once per tick (shared with `context`),
  never under the pinned clock; goldens run the binary and read it like a
  tick, with `GARNISH_MANAGED_SETTINGS` (§ 9) empty. A settings file over 1
  MiB is skipped, like one that does not parse (§ 5). `config show` prints
  the value the file or the current directory's settings decide (never the
  session switch); `config init` writes the key as a comment, like
  `durations`.

### 4.3 Layout: rows, columns and boxes

Today's line is the base case: every config is already valid, and nothing
changes until someone adds a column.

**Line and row.** A **line** is one terminal line (what the harness counts
and `preview` prints). A **row** is the config unit `[[row]]`, one or more
lines tall: bare one, boxed three (frame, content, frame); content several
lines tall would follow the same rules. Frame lines are drawn, never
configured. `[[line]]` and `hide_empty_lines` are aliases of `[[row]]` and
`hide_empty_rows`; `config show` writes the new names, `config check` is
silent about the old.

**Model.** A config is a list of **rows**; a row is **columns** side by
side (a row with `modules`/`right` and no `[[row.col]]` is one full-width
column). A column holds its own modules or a **stack** of rows; columns
share the width by `width`. In a column, `modules` with `right` is the flex
form (groups at the column's edges, the rule between); `modules` alone sits
where `justify` says. **Titles** decorate rules, **boxes** decorate rows and
columns. The tree is two levels deep, never deeper, so `setup` (§ 14) can
always draw it.

```toml
[[row]]                        # a row: one or more columns side by side; `[[line]]` is its alias
gap = 1                        # empty cells between columns (default 1)
separator = " · "              # joins the modules of every column (per-row override, § 4)
title = "Session"              # text set into the row's rule; see Titles
title_justify = "center"       # left | center | right (default left)
title_pad = 1                  # spaces on each side of the title
title_color = "accent"         # role or literal; default the frame colour
box = "repo"                   # this row is inside box `repo`; `true` boxes it alone; see Boxes
blank = false                  # § 4.1

[[row.col]]                    # a column; a row with no [[row.col]] is one "1fr" column
width = "1fr"                  # "<n>fr" share of the free width | "auto" its content | 24 cells
modules = ["path", "branch"]   # the column's own modules …
right   = ["pr"]               # … flex: `right` anchors to the column's right edge, the rule fills between
justify = "left"               # left | center | right: where `modules` sit when there is no `right`
valign  = "top"                # top | center | bottom: where a short stack sits in a taller row
box = "repo"                   # the whole column is one box, the row's full height
[[row.col.row]]                # … or a stack of rows (then no `modules` on the column)
modules = ["context"]          # an inner row takes every row key except `gap` and [[row.col]]
box = true

[box.repo]                     # a box: rows and columns join it by name
title = "Repository"
title_justify = "left"
style = "double"               # inherits [frame] style when absent (rounded if the frame has no box shape)
fill = false                   # default inside a box
color = "accent"               # role or literal for the box's glyphs; default the frame colour
```

- **Width.** Row width = the § 2.1 box minus `gap` per boundary. `24` takes
  24 cells; `"auto"` its content (modules joined by the separator,
  `max_width` applied, plus two sides and two pads when boxed). A column
  taking no cells (`width = 0`, an empty `auto`) takes no gap: gaps sit only
  between drawn columns. The rest is free width, shared by `fr` columns as
  `floor(free × n ÷ Σ fr)`, leftover cells one each to the first, so shares
  differ by at most one and add up. An `fr` column whose share is nothing is
  dropped (the last such first) with its gap and the row re-shared until
  every `fr` column has cells; with none left the freed cells are free width
  after the last column. Default `"1fr"` (three bare columns are thirds).
  Content wider than its column is cut with `…` or, under `overflow =
  "ticker"`, scrolled inside it, never spilling: in a flex column the left
  group is the window and `right` never scrolls, a lone group is the
  window, each inner row is its own window, an `auto` column always fits.
  `truncate = false` lets only the last (or only) column run past the box.
  An `auto` column is re-measured each tick and moves its neighbours, so it
  suits values that hold still (a `fixed` clock, a `max_width` module); an
  `auto` flex column joins its groups with the separator, no rule; an
  `auto` stack is its widest inner row. With no `fr` column the free width
  is a rule after the last column into the right cap. When width runs out
  the row is laid out left to right, gap then column: fixed and `auto`
  columns take at most what remains, and a column whose gap plus one cell
  does not fit renders nothing, nor does anything right of it (row width
  `max(box − gaps, 0)`; only below the § 4 minimum of 10 or with fixed
  widths over the box; `GARNISH_DEBUG` logs it). `config check` names the
  three `width` forms when one is a quoted number or anything else.
- **Inside a column.** With `right`: the § 4 flex line at the column's width
  (left group cut first). Alone: one group placed by `justify`, defaulting
  by position (first left, last right, middle centre, lone left). `align`
  (§ 4) pads position *k* of column *c* to the widest *k* of *c* across rows
  with the same column count (never across counts; stack rows only with
  stack rows at the same column position); *k* counts from the left in a
  left- or centre-justified column and in the left group of a column with
  `right` (whatever `justify` says), from the right in a right-justified
  column without `right` and in a `right` group, whose pad side
  `right_justify` picks.
- **Stacks and height.** `[[row.col.row]]` makes the column a stack, each
  inner row laid out to the column's width; an inner row has no `justify`
  (reported unknown) and follows the column's. A row's height is its
  content's (bare 1, boxed +2, a box too narrow to draw adds none); a
  column's is the sum of its rows'; the outer row is its tallest column. A
  shorter stack is padded with empty lines placed by `valign`. Inner rows
  take no `[[row.col]]` and no `gap`; a column with both `modules` and inner
  rows is reported and the stack wins.
- **Caps.** `[frame]` caps sit at both ends of every non-box line, `first`,
  `middle`, `last` decided over those lines in order (a tall row's inner
  lines are `middle`; a config of only boxed lines shows no cap); box lines
  carry the box's corners and sides. `custom` caps may differ in width: a
  tall row is laid out to the room its widest pair leaves, a narrower or
  empty cap's spare cells go to the rule (behind the cap's pad on a row
  ending in content), and a line with an empty cap keeps its pad when it
  has the cells. A row's height is measured on the room it is laid out to
  (the caps of the lines it lands on, depending on heights before it), so
  rows are re-measured until none moves; if they never settle (a box that
  fits under `single` caps but not the `first`/`last` its three lines
  would take), measure and layout use the narrowest pair.
- **Fill.** On a one-line row `fill` draws the rule (or `fill_pattern`) in
  every empty cell inside the caps, gaps included
  (`╭─ path ─── ⏱ 2h13m ─── 12:00:00 ─╮`); the pattern's phase is `(rule-cell
  index + frame) mod period`, rule cells numbered across the line (module
  cells take none), and § 4.2's short-rule fallback counts the same cells,
  so dots cross column boundaries. On a multi-line row it fills only each
  inner row's cells; gaps and padding lines are spaces. `blank` on the
  outer `[[row]]` of a multi-line row gives the braille cell to every line
  that would be whitespace (padding lines included); on an inner row it
  follows § 4.1, judged on the finished line.
- **Titles.** `title` is plain text (§ 5) set into the rule in the frame
  colour (`title_color` for another role or literal) with `title_pad`
  spaces each side; `title_justify` puts it after the left cap, centred, or
  before the right cap. A line with modules has several runs of empty cells
  (per column and per gap): a centred title takes the widest, a left one
  the first that fits, a right one the last, falling back to the widest.
  Cells around it stay what the run was (rule, or gap/padding spaces). With
  `fill = false` or `style = "none"` it is the same text in the same place
  with its pad spaces. On a multi-line row it goes in the first line. On a
  `box = true` row the `title*` keys title that anonymous box; a row in a
  named box gets no title (reported, ignored). Without `title` the other
  three keys are reported as having no effect. A too-wide title is cut with
  `…`, never widening the line. A row with only a `title` is a titled
  spacer (`├─ Repository ────┤`), always kept.
- **Boxes.** `[box.<name>]` (a bare key) takes the four `title*` keys,
  `style`, `fill`, `color` (role or literal for its glyphs). `style` and
  colour inherit from `[frame]`; a frame style with no box shape (`none`,
  `powerline`) gives an unstyled box `rounded`. `fill` defaults to `false`
  (`true` draws the rule between a row's groups). A box is a corner-capped
  top rule with the title, side glyphs at both ends of each inner line, and
  a bottom rule: at least three lines. Joining: adjacent rows (of the
  config or of one stack) with the same `box = "<name>"` form one box;
  `box = "<name>"` or `true` on a column makes the whole column one box the
  outer row's height, padding lines as empty interior lines (a boxed
  one-line column is three lines); `box = true` on a row boxes it alone,
  untitled (three adjacent ones are three boxes). Boxes never nest: `box`
  on a row in a boxed column, on a column of a boxed row, or on a row of
  that column's stack is reported and the inner box ignored. A name reused
  for a non-adjacent run is reported and the second run unboxed; runs count
  across the whole config (a stack is its own run; a name a column's `box`
  took is taken). Lines outside boxes keep the frame's caps. Corners and
  side: `rounded` `╭ ╮ ╰ ╯ │`, `square` `┌ ┐ └ ┘ │`, `double` `╔ ╗ ╚ ╝ ║`,
  `heavy` `┏ ┓ ┗ ┛ ┃`, `fill_char` horizontal; `none` an invisible box
  (lines indented by the pad); `powerline` is reported and drawn rounded. A
  `custom` frame adds `top_left`, `top_right`, `bottom_left`,
  `bottom_right`, `side`, one cell each (else reported, style's glyph kept;
  § 4.1 guard); unset is empty, an explicit empty string is refused. Corners
  are drawn whatever the side, so a box too narrow for its corners or sides
  renders nothing: empty cells (rule on a one-line row, spaces on a taller
  one), no added lines, and as a last column it does not end the row in
  content. A glyphless box (`style = "none"`) fits any one cell, and a
  `width = 0` column has none, so it adds no lines either.
- **Hiding.** A module hidden by `stale_style = "hide"` or
  `hide_when_empty` leaves its row (§ 3.6, § 4.1). Under `hide_empty_rows`
  an inner row with nothing rendered is dropped (the stack shortens, the
  outer row follows its tallest remaining column), a box whose rows all
  went goes with its frame lines (a title alone keeps nothing), and a row
  goes when every column is empty. A column emptied beside a non-empty
  sibling keeps its share and renders empty lines, so nothing reflows. A
  column with no `modules` and no inner rows, or `modules = []`, is an
  empty column keeping its share; a `[[row]]` is a spacer only when every
  column is empty.
- **Edge cases.** One column has no boundary, so `gap` does nothing (not
  reported). An inner row's `separator` beats the outer row's, which beats
  the frame's. A scrolling column carries its `align` pads in the window.
  `hide_when_empty = false` pins an inner row as it pins a row. A `scroll`
  text module in an `auto` column has a fixed box width, so the column
  holds still. A row with `[[row.col]]` and also row-level `modules` or
  `right` is reported (columns win); `right` without columns is the plain
  one-column row. `padding` (§ 4) shrinks the box before sharing. A
  `[box.<name>]` nobody joins is reported unused. `config init` writes no
  columns or boxes, only a commented example.
- **Pads.** A rule never touches a module's text: a column keeps one `pad`
  cell on each end its *content* reaches (a flex column's two groups, a
  left- or right-justified lone group, both ends of an `auto` column, whose
  width includes them), none where the rule surrounds the group or a cap or
  box side has padded it. A row whose content reaches the cap keeps the
  cap's pad against it, leftover cells going behind: content, pad, rule,
  cap. A last column that takes no cells, or whose box does not fit, draws
  nothing, so the column before ends the row and keeps its own pad and the
  cap takes none; likewise with no `fr` column the last column keeps its
  pad before the trailing rule and the cap takes none. The one exception: a
  column narrower than its text plus pads keeps what text fits (a one-cell
  column its `…`) and no pads, so there alone text meets the rule. A
  stack's column pads belong to the inner rows reaching its edges. A box's
  interior pad is the frame's `pad`, or one space when the frame has none,
  so content never touches a box side and a `style = "none"` box indents by
  it. Every pad is the `pad` string itself, unstyled. In a box, a lone
  group keeps no fill cell and no pad on a side facing the box's side;
  between two columns in a box without a rule both pads stay only at
  `gap = 0` (a gap's spaces separate them otherwise); under a rule both
  stay. A title right after a cap drops its leading pad (`├─ Repository ──┤`),
  the cell going to the rule. A box's top and bottom rules are static:
  `fill_pattern` belongs to the frame.
- **Validation.** `config check` reports `justify`/`valign` outside their
  words; a `width` not `"<n>fr"` (1–64), `"auto"` or a cell count (≤ 1024);
  a `gap` or `title_pad` outside 0–16 or 0–64, naming the range; over 16
  columns in a row or 16 inner rows in a column; `box` naming no
  `[box.<name>]`; `modules` beside `[[row.col]]`; nesting either way; a
  non-adjacent reuse; a title in a named box; `title_justify`, `title_pad`
  or `title_color` without `title`; `[[line]]` and `[[row]]` in one file
  (they cannot be ordered against each other). `config show` writes a
  one-column row as plain `[[row]]`, writes `[[row.col]]`,
  `[[row.col.row]]` and `[box.<name>]` only where configured, rewrites the
  aliases and drops every reported key, so its output is a fixed point
  `config check` calls `ok`.
- **Setup.** The builder (§ 14) shows a row's columns side by side: *Add a
  column*, `width`, `justify`, *Stack*, *Add a title*, *Wrap in a box* over
  a run of rows, *Box the column*; the placement map lists every inner row,
  title and box edge.
- **Cost.** Layout is arithmetic over segment lists the modules already
  render; nothing new is read or spawned. Presets `grid-three`, `grid-six`,
  `boxed-panels` and `dashboard-panels` pin shares, titles and stacks at two
  widths (`tests/presets.rs`: declared width and 40 wider, uncut, same
  lines, every line that fills the box filling the wider one).

A sample: three columns in a 60-cell box (§ 2.1: a 64-column terminal)
under `[frame] style = "none"` (so the `box = true` boxes are `rounded`): a
full-height double box, a bare centred column, three stacked boxes; one row
of config, nine lines on screen:

```toml
[box.repo]
style = "double"
title = "Repository"

[[row]]
gap = 2
[[row.col]]
box = "repo"
[[row.col.row]]
modules = ["path", "branch"]
[[row.col.row]]
modules = ["sync", "pr"]

[[row.col]]
justify = "center"
[[row.col.row]]
modules = ["model", "effort"]

[[row.col]]
justify = "center"
[[row.col.row]]
box = true
modules = ["context"]
[[row.col.row]]
box = true
modules = ["limit5h"]
[[row.col.row]]
box = true
modules = ["cost"]
```

```text
╔═ Repository ════╗      Opus  high       ╭────────────────╮
║ ~/garnish  main ║                       │ ████░░░░░░ 42% │
║ ⇡2 ⇣1  #42      ║                       ╰────────────────╯
║                 ║                       ╭────────────────╮
║                 ║                       │   23%  2h13m   │
║                 ║                       ╰────────────────╯
║                 ║                       ╭────────────────╮
║                 ║                       │     $1.23      │
╚═════════════════╝                       ╰────────────────╯
```

Validation (`garnish config check`): unknown keys, wrong types, unknown
module ids, unknown presets, bad colours, animation frames of unequal width,
all reported with TOML paths; on problems it lists them and exits 1 without
an error report.

## 5. Failure behaviour

`garnish` (render) always exits 0 and prints every row not hidden. If all
rows hid (§ 4.1) it prints one empty line, which Claude Code trims to
nothing, clearing the status line until a module has something (§ 2.1).
Otherwise:

- **Invalid config** → each invalid key falls back to its built-in default
  (the file is read as a TOML table, each key converted alone) and a dim
  `⚠ config: <path>:<line> <msg>` row is appended (value errors carry the
  TOML path, syntax errors the line); only a TOML syntax error falls back
  to the defaults wholesale.
- **Malformed stdin** or non-object JSON → `⚠ garnish: bad payload`, the
  parser's message (line, column, expectation) on stderr and in the
  `GARNISH_DEBUG` log; any JSON object renders. A known field of the wrong
  type is absent, alone (an array for an object too: `"rate_limits": []` is
  no rate limits), as is a non-string list entry and a non-finite numeric
  string (`inf`, `NaN`). Numbers too large to print are shown at a bound: a
  percentage past 100 (`spend`) and a cost stop at 99 999
  (`num::MAX_SHOWN`), and bands compare the bounded number.
- **A command line clap refuses on the render path** (no subcommand but
  `render`, stdin not a terminal: a `statusLine.command` typo, a flag an
  upgrade removed) → `⚠ garnish: <the error's first line>`, the whole error
  on stderr; a subcommand's bad flag, or one typed at a terminal, keeps
  clap's usage error and exit code.
- **A panic** → `⚠ garnish: internal error`: a render-path panic hook
  prints it and exits 0, ahead of release's `panic = "abort"`; a stack
  overflow or signal is beyond it. The hook writes the row before its
  stderr note, and no render-path stderr write can fail the tick
  (`debug::stderr_line`; `eprintln!` panics on an unread stderr).
- **A file that fails to parse is never rewritten.** `install`, `config
  init --force` and `setup` (§ 14) refuse a `settings.json` that is not a
  JSON object or a `garnish.toml` with a TOML syntax error (bad values parse
  and are replaced), name problem and file on one stderr line, and exit 1
  quietly, dry runs included; only fixing or moving it by hand gets past. A
  parsing file is replaced by `install::replace_file`: through a symlink,
  keeping permissions, after a timestamped never-overwritten backup beside
  it, via a same-directory temp file and `rename`; `config init` names the
  backup.
- **Nothing but text reaches a row.** Every string on a row is reduced to
  plain text, removing escape sequences (CSI, OSC, DCS/SOS/PM/APC with
  payloads, nF sequences such as `tput sgr0`'s `ESC ( B` with their final
  byte), control characters and bidi and zero-width format characters (bidi
  marks, embeddings, isolates, zero-width space and non-joiner, word joiner,
  BOM; ZWJ and the emoji variation selector stay). Config strings (`label`,
  `prefix`, `suffix`, icon overrides, frame and box glyphs, separators,
  `text`, `gap`, `ticker_gap`, row and box `title`) are reduced at config
  time, so width arithmetic sees real cells; everything else (payload names
  and paths, git output, cache entries, the `⚠` line) by the `Segment`
  constructors, the one way onto a row. A module that measures or cuts a
  string (`max_length`, fish initials, a short sha or session id) reduces
  it first. Colour and OSC 8 are added by the painter alone, and a link
  only for an `http(s)://` URL of printable ASCII.
- **Sizes are bounded.** A cell count (`width`, `pad`, `bar_width`,
  `max_width`) above 1024, a row string (`text`, `gap`, `ticker_gap`,
  `label`, `prefix`, `suffix`, `title`) or text `url` above 4096
  characters, or `cost.decimals` above 8 is reported and the default stands
  in; renderers clamp again, and the effective width never exceeds 4096
  whatever `COLUMNS` says. Each cap is the option's schema `max`, printed
  in the reference's type column (`integer ≤ 1024`, `string ≤ 4096
  chars`); common options go through `COMMON_OPTS` alike, and the top-level
  `ticker_gap` is checked by hand against the same constant. The tick reads
  a chain settings file (a cloned repository can supply one) up to 1 MiB
  and skips it past that; `doctor` shows any `statusLine.command` as plain
  text cut to 200 characters. `~/.claude.json` is read up to 8 MiB (§ 3.8);
  `below:N`/`above:N` take `N` ≤ 1000. A `*_step` must lie in
  `0.001..=1000` (below nothing moves; above, `now × step` saturates).
  `frame.fill_char` must be exactly one cell, else reported and the style's
  glyph kept.
- **No home directory** (`HOME` unset, no `XDG_CONFIG_HOME`): `install`,
  `config init`, `config path` and `skills install` refuse with one line
  naming the flag to pass, never writing into the current directory.
- **An empty path variable is unset** (`GARNISH_MANAGED_SETTINGS`
  excepted, § 9).

`GARNISH_DEBUG=1` appends per-tick diagnostics to `<cache>/debug.log` (1 MB
rotation); `garnish doctor` shows its tail plus toolchain, config
path/validity, cache dir, last worker errors and the glyph grid (§ 7).

## 6. Cache & workers

- **Root**: `$GARNISH_CACHE_DIR` > `$XDG_RUNTIME_DIR/garnish` >
  `$XDG_CACHE_HOME/garnish` > `~/.cache/garnish` (macOS
  `~/Library/Caches/garnish`) > the temp directory's `garnish-<uid>`,
  created `0700` and **refused** (no entry read or written, no lock, no
  worker; `doctor` says why) unless a real directory this user owns that
  nobody else can write (the uid comes from a file the process creates; no
  `libc`). Created directories are `0700`, files `0600` (an entry may hold
  the account's email).
- **Layout**: `<root>/sessions/<session_id>/<module>.cache`; git data in
  `<root>/repos/<hash(git common dir + per-worktree git dir)>/<module>.cache`,
  shared by sessions in one worktree. Never keyed on `transcript_path`.
  `account` keeps `sessions/<session_id>/account.cache` with an `email`
  line (absent `.claude.json`: `ok`, no line).
- **Entry**: line 1 `v1 <computed_at_ms> <ttl_ms> ok|err`, then `key=value`
  lines or the error text. Malformed, or not a regular file ≤ 64 KiB (a
  FIFO would block `open`; locks are read alike), is a miss. The error text
  and `fetch_error` are plain text ≤ 500 characters (`doctor` prints them).
  An entry that would not read back as itself (a value with a line break, a
  file past 64 KiB) is stored as a failed entry naming the value, or every
  tick would miss and spawn; the repository readers already refuse a
  branch, `remote` or `merge` with a control character or over 4096
  characters. Written as `.<module>.tmp.<pid>` + rename; every temporary
  name is unlinked first and created exclusively, so a planted link is
  never followed. `ttl_ms` is informational: freshness is the reader's TTL.
- **Tick**: fresh → render; past TTL → spawn a worker unless
  `<module>.lock` is live, rendering the last value unchanged; older than
  `stale_after` TTLs (or for another head/upstream) → dim `⟳`; `err` → dim
  `✗`. A failed entry is fresh for its TTL (retry once per TTL, never per
  tick). Entries record their situation (`branch`'s `head`; `sync`'s
  `branch` and `upstream`, since branches sharing an upstream must not
  share counts); a differing situation makes the entry stale.
- **Lock** = file `pid epoch_ms`, created by `hard_link` from a pre-written
  temp file, re-stamped by `rename`, never truncated in place. Live when
  younger than 2 s (hand-over), else while its pid exists (Linux, `/proc`)
  and it is under 60 s old (30 s where pids cannot be checked, which a
  compile-time assertion keeps above a `sync` worker's 20 s fetch plus 2 s
  count); the age limit bounds a lock whatever its pid. A stale lock is
  reclaimed by atomic rename and read back: if it is not the lock judged
  dead it was another's fresh lock and is linked back, so one process wins
  each reclaim. A guard unlinks only a lock still carrying its pid. A root
  where no lock can be taken (no hard links) is logged by the tick
  (`GARNISH_DEBUG`), recorded by the worker as a failed entry (`✗`, retries
  spaced by the TTL), and named by `doctor`'s probe.
- **Worker**: `garnish [--config C] refresh --module M --session S --cwd D`,
  null stdio, `process_group(0)`, not waited for. `--config` is the file the
  tick loaded, when it loaded one (absolute, passed as bytes so a
  non-UTF-8 path survives), since a status line `--config` is not in the
  inherited environment. On Linux the tick takes the lock and passes
  `--lock-held`; elsewhere the worker takes it. `GARNISH_NO_SPAWN=1` logs
  intended spawns to `<root>/spawns.log` instead.
- **`refresh`** is ≥ 1 for cached modules (0 rejected), 0 for payload-only
  ones (§ 3). `refresh --module` on a payload-only module is refused with
  one stderr line and exit 1, writing no entry.
- **GC**: a bounded sweep when a worker writes a module's first entry in a
  scope (session or repo): session and repo dirs idle > 24 h by wall-clock
  mtime, ≤ 50 per sweep; temp/stale/adopt files over 1 h; never on the
  tick; `garnish gc` by hand. The root may be shared
  (`GARNISH_CACHE_DIR=~/.cache`), so it touches `sessions`, `repos` and
  their directories only as real directories (never through a link), a repo
  directory only when named as a 16-digit hash, a session one only as a
  sanitised id, and either only when every file in it has a garnish name.
- **No child process on a warm tick.** Branch/upstream/HEAD come from `.git`
  files (loose refs, `packed-refs` scanned as bytes with early exit,
  worktree `gitdir`, symref chains capped at 5). Each is a bounded read of a
  regular file (FIFO or `/dev/zero` link refused; opened `O_NONBLOCK` and
  the handle re-checked, since one can be swapped in after the check),
  contained in the git directory. A symbolic ref may point only under
  `refs/` or at a capitalised pseudo-ref (git's `refname_is_safe`); a `.git`
  file's `gitdir:` and a `commondir` count only when they name a git
  directory by git's test (`HEAD`, `objects/`, `refs/`), as containment is
  relative to them. The upstream comes from `.git/config`, read up to 1 MiB
  (a non-UTF-8 byte costs only what it touches) and parsed as git does:
  quoted values, escapes, `;`/`#` comments, `\` line joins, any-case
  section and key names, git's four blanks, CRLF, a leading BOM, the first
  `merge` and last `remote`; a malformed line costs only itself. It is read
  64 KiB at a time; sections other than the branch's are skipped by a byte
  search for the next header, parsed only where a `\` may join lines.
- **Reftable** repos report no head to the tick and fall back to workers.
  `branch`'s asks git (`symbolic-ref -q HEAD` less `refs/heads/`, not
  `--short`, which spells a branch sharing a tag's name `heads/<name>`;
  else `rev-parse --verify HEAD` when detached, plus the commit for
  `show_sha`) and records `branch` and `detached`; `sync`'s resolves the
  branch alike, reads the upstream from the config, checks it with
  `show-ref --verify`, and records `no_upstream` or `detached` as values,
  not failures. Both entries carry `tables` (mtimes of the worktree's and
  common `reftable/tables.list`, taken before asking git); an entry whose
  `tables` differs from the tick's stat is for another ref state, except a
  failed entry (no values), fresh for its TTL whatever the stamp. A `HEAD`
  the tick refuses in a files repository (a link out of the git directory)
  leaves `branch`'s entry without `head`, which any render accepts.
- **Git in the worker.** Ahead/behind, dirty and fetch run only there,
  through `git::run_program` (pipes drained on threads, 1 MiB stdout and
  64 KiB stderr kept, the rest discarded; kill on timeout: 2 s local, 20 s
  `fetch`). `git` is the first executable on an *absolute* `PATH` entry,
  looked up once. Every call clears `core.fsmonitor`, sets
  `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0` and `GIT_NO_LAZY_FETCH=1`
  (no lazy fetch running the repository's `uploadpack`; honoured since the
  May 2024 security releases), and removes `GIT_DIR`, `GIT_WORK_TREE`,
  `GIT_INDEX_FILE` and the other redirecting variables. Every call but
  `fetch` sets `GIT_ALLOW_PROTOCOL` empty (older gits ignore
  `GIT_NO_LAZY_FETCH`); `fetch` keeps the user's.
- **Dirty never reads a worktree file** (`git status` would hash files
  through the repository's own `clean`/`process` filter): `git diff-index
  --cached --quiet HEAD` (anything in the index before the first commit)
  plus `git -c core.checkStat=default diff-files --quiet
  --ignore-submodules=dirty`, comparing stat data and stopping at the first
  difference; the stat rule is pinned so a repository cannot make files
  "racily clean" and hashed; a submodule's own dirtiness is not asked.
  `core.trustctime` stays the repository's (pinned `true`, a clean tree
  would read dirty for good wherever it is `false`; with `false` a crafted
  index must still match the inode). Accepted cost: a file touched without
  change reads dirty until the user's git refreshes the index.
  `status.showStash` is irrelevant.
- **Fetch** (opt-in, `fetch_interval`) passes `--no-auto-maintenance`,
  `--recurse-submodules=no`, `--upload-pack git-upload-pack` and the remote
  after `--` (a name starting with `-` is refused), and sets
  `SSH_ASKPASS_REQUIRE=force` with `SSH_ASKPASS` a failing program
  (`false`, found as `git` is), since the worker keeps Claude Code's
  controlling terminal (OpenSSH 8.4+). `core.sshCommand`, `core.gitProxy`,
  `ext::` URLs, hooks and credential helpers are a backlog decision (PLAN).
  A failed fetch records `fetch_error` and `fetch_attempt` without hiding
  local counts and is not retried within `fetch_interval`; a good one
  records `fetch_ok_at`. All three carry over between attempts, so the
  error lasts until a fetch works; `doctor` lists each such entry as `FETCH
  FAILED`. A fetch is due when both `fetch_attempt` and `FETCH_HEAD`'s mtime
  (newer of the two worktree files) are at least `fetch_interval` old, a
  future stamp counting as due.

## 7. CLI

`--config FILE` is a global flag overriding `GARNISH_CONFIG` and the default
location (§ 4).

| command | purpose |
|---|---|
| `garnish` (or `garnish render`) | render from stdin (the explicit form is for a settings file that wants a subcommand). The bare `garnish` with a terminal on stdin prints a two-line pointer at `garnish setup` and exits 0 (§ 14; `GARNISH_STDIN_TTY`, § 9); `garnish render` always reads stdin |
| `garnish refresh --module M --session S --cwd D [--all] [--lock-held]` | worker entry point, hidden from `--help`; the tick passes its `--config` ahead of it (§ 6) |
| `garnish install [--settings P] [--refresh-interval 1] [--padding N] [--absolute] [--no-config] [--no-skills] [--dry-run]` | merge `statusLine` into settings.json (`--settings`, else the § 2.3 user file) through symlinks, keeping permissions and key order, with a never-clobbered backup; write the skills (§ 13) unless `--no-skills`; write the default config if absent (§ 4's target), seeded with `padding = 2N` from `--padding N` or the file's `statusLine.padding` (N ≤ 32767; an existing config with another `padding` gets a stderr note naming the value); warn if not on PATH. The command is `garnish`, or with `--absolute` the first `garnish` on PATH, else the path it was run by, else `current_exe()` (a package manager's launcher, not its versioned target), shell-quoted where needed. With an explicit config (`--config`, else `GARNISH_CONFIG`) it is `<program> --config <absolute path>`; otherwise a command already running garnish keeps its arguments and only its program word changes, found as `sh` splits it past `NAME=value` words and a leading `env` with its assignments, that prefix kept. `--refresh-interval` ≥ 1; `--dry-run` says an up-to-date file is left alone |
| `garnish doctor` | diagnostics. Glyph grid: one row per icon set and module (plus `config` rows for the loaded config's resolved icons), each single-character icon padded to two cells then `\|` and garnish's cell count, so a mis-sized glyph pushes its `\|` out of line; multi-character icons omitted. The settings chain for the current directory (managed, with the platform file's `managed-settings.d` drop-ins above it, local, project, user): each file's presence and parse, read whole; one past the tick's 1 MiB says so and the tick's keys (`prefersReducedMotion`, `sandbox.enabled`, `voice.enabled`, the auto-compaction keys) skip it. Then, resolved as Claude Code does with the file named: `statusLine.command`; `statusLine.refreshInterval` (suggesting `1` for a clock, an elapsed time, a countdown (a limit reset in any form but `absolute`, or `eta`) or an animation); `statusLine.hideVimModeIndicator` (suggesting `true` with `vim` on); `statusLine.padding` (suggesting the config's `padding = 2N` when it differs); `disableAllHooks` (which stops the command); `prefersReducedMotion` (and its interplay with `animate`); `sandbox.enabled`, `voice.enabled` (§ 3.8); `tui` (§ 2.1; a value that is neither name is named as one Claude Code drops from the managed file or rejects any other file for, the next file setting the key is shown, the rejected file's row says so instead of `ok`, and neither `doctor` nor the tick takes any key from it). Project files are named relative to it, other paths with `~`; an unreadable config is named as such |
| `garnish setup [--preset P] [--install]` | the interactive setup (§ 14); `--preset` never opens the screen, writing that preset with the § 5 backup (as `config init --preset P --force`) plus `install` with `--install`; without `--preset` or a terminal on stdout it exits 1 with one line |
| `garnish config init [--preset P] [--force] \| check \| path \| show` | `init` refuses to overwrite without `--force`, accepts gallery names (§ 12) and the four built-ins, and under `--force` keeps a § 5 backup and refuses an unparsable file; `check` lists problems and exits 1 quietly; `show` prints the fully resolved config, the animation switch as the file or the current directory's settings decide (§ 4.2) |
| `garnish skills install [--dir D] \| list` | copy the skills (§ 13) into `~/.claude/skills/` (`$CLAUDE_CONFIG_DIR/skills/` when set; or `D`); `install` does this unless `--no-skills` |
| `garnish preview <file\|dir> [--preset P] [--icons S] [--theme T] [--color M] [--width N]` | render a fixture or every `*.json` in a directory under dim `── <name>` headings, rows faint as on screen (§ 2.1; `--color never` plain); never reads the cache or spawns; `--preset` replaces the file's rows, so their problems and whether a `[box.<name>]` is joined go unreported |
| `garnish docs --out DIR` | regenerate docs from schemas; a maintainer's tool, hidden from `--help`, no default directory (it replaces same-named files); `make docs` runs via the docs-sync test |
| `garnish modules` | list module ids + summaries |
| `garnish presets` | list the gallery presets (§ 12): name, summary, declared width, requirement |
| `garnish gc` | sweep stale cache dirs |

## 8. Performance budget

hyperfine (`bench/run.sh`, release, `-N`, warmup 20, 300 runs), gated by
`bench/check.sh` (jq over hyperfine's JSON):

| scenario | mean | p99 |
|---|---|---|
| warm tick, default preset | < 3 ms | < 8 ms |
| warm tick, full preset (the default rows, every option) | < 3 ms | < 8 ms |
| warm tick, one row of every module id (settings badges, `account`) | < 3 ms | < 8 ms |
| warm tick, default preset, `TZ` naming a zone | < 3 ms | < 8 ms |
| warm tick, default preset, a `.git/config` of 5000 branch sections | < 3 ms | < 8 ms |
| cold tick (empty cache, git repo) | < 30 ms | — |
| `refresh --module sync` worker (rev-list, no fetch) | < 50 ms | — |

Criterion benches in `benches/` track parse, config resolution and
per-module render cost.

## 9. Testing strategy

- **Unit**: each module × preset × icon set × `max_width` and each option
  switch, frozen clock (default theme; `theme-nord`'s config golden paints
  another palette over the same roles); absent/null fields; band edges;
  duration/countdown formatting; threshold math; ANSI width/truncation;
  frame assembly; preset resolution order; schema completeness (a scan of
  `src/modules/*.rs` requires every icon, colour and option key read by name
  to exist in a schema of a module that file defines; a sibling's key in
  the same file is not caught).
- **Integration** (real binary): the payload fixtures of
  `tests/fixtures/payloads/` (subscription, API key, pre-first-response
  nulls, no git, worktree session, git worktree, the PR states and an MR,
  spend_limit, fast_mode, 1M at 3/50/80/96 %, 200k, a cold cache, vim,
  agent, no session_name, an output style, absent effort, added dirs;
  autocompact via settings/env/disabled from the test's environment;
  `preview` of the directory renders each in name order); temp repos with a
  local bare origin (ahead, behind, diverged via a second pushing clone, no
  upstream, detached, dirty, worktree; `fetch_interval` fetches once per
  interval and sees the push); a slow/failing PATH-shim `git` proving ticks
  never block; TTL expiry; live lock; stale lock with dead pid;
  `.tmp`/truncated entries ignored; the tick's process group (a group
  leader, killed with the `kill` binary) killed after spawning a slow
  worker, which still writes its entry; 32 concurrent ticks → one worker;
  GC bounds.
- **Config matrix**: every preset × icon set × fixture, every frame style,
  one module per line (25 rows, `hide_empty_lines = false`) and all on one
  (cut with `…`) → no panic, right line count, width ≤ `COLUMNS − 4`;
  goldens under `tests/golden/` (`UPDATE_GOLDEN=1` regenerates), rendered
  `--color never` unless a config fixture's `# color:` says otherwise
  (`colour-on` pins the painter's escapes, with `preview`'s faint, and the
  OSC 8 link). Both suites' row-start guards look past escapes and fail on
  a row whose raw bytes start with whitespace without being all whitespace
  (§ 2.1). A `# env:` value may use `$ROOT` for the repository root
  (`reduced-motion` points `HOME` at a settings fixture). Every binary test
  sets `GARNISH_MANAGED_SETTINGS` empty; one CLI test points it at a
  fixture.
- **Docs sync**: `garnish docs` must equal `docs/`, `config init` must equal
  `examples/garnish.toml`.
- **Pinned renders spawn nothing**: `Clock::fixed()` turns off workers, git
  and the settings chain, so `account` renders its empty state with no
  cache directory (`tests/docs_sync.rs` asserts none appears) and the
  badges render on from seeded keys. Only `render::render` builds its clock
  from the environment; every other entry point takes a `Clock`.
- **Module matrix from the schema**: an in-crate rayon test renders every
  module × preset × icon set × `max_width ∈ {0, 1, 4, 12}`, plus every
  schema switch (both `Bool` values, every `Enum` variant, at one preset and
  set), alone on an unframed line, against every fixture, asserting: never
  wider than `max_width`; a cut ends in the ellipsis and an uncut module is
  byte-identical to its uncapped render; a hidden state drops the line
  rather than leaving a blank row, while `hide_when_empty = false` always
  shows the placeholder; no escape or control byte in `Segment::text`; no
  link the painter would refuse; balanced OSC 8 wrappers.
- **Layout matrix**: column shares add up to the line width and differ by
  at most one cell at every width 10–400; every line of a multi-line row is
  exactly the box with stacks of unequal height; every fixture and preset
  renders byte-identically as a plain line and one column, under either
  name; lines-per-row output tiles each line exactly (the § 14 placement
  map reads it). `tests/presets.rs` renders every preset uncut at its
  declared width at three instants: no `…`, no layout cut, same modules and
  titles as 200 columns wider (structure, not `…`, which the ascii set also
  draws). Each motion promise is checked alone from the parsed config: a
  rule pattern, separator frames, icon frames and a scrolling text module
  must move; a line ticker must slide exactly `ticker_step` cells (so a
  scrolled row holds nothing counting seconds); `animate = false` must
  render the same at two ticks a minute apart.
- **Setup snapshots**: `setup` screens are rendered into ratatui's
  `TestBackend` (80 × 24, 100 × 30, 140 × 40) against `tests/golden/setup/`
  (`UPDATE_GOLDEN=1`; the test lists every golden it writes): home, picker,
  builder, module form, module picker, top-level form, glyph picker, a
  confirm dialog, install screen and help have goldens, other forms and
  pickers are asserted by content; keys and mouse go through the terminal's
  input path, so no tty is needed. The test app pins the clock
  (`Clock::fixed()`), aims install at a temporary home and shows its paths
  as `~/…`; `tests/cli.rs` covers `setup --preset [--install]` and the
  stdin pointer end to end.

### Test hooks (environment)

| var | effect |
|---|---|
| `GARNISH_NOW` | freeze `time::now()` (epoch seconds or RFC 3339) |
| `GARNISH_CACHE_DIR` | cache root override |
| `GARNISH_CONFIG` | config path override, absolute only (§ 4) |
| `GARNISH_NO_SPAWN` | record intended worker spawns instead of spawning |
| `GARNISH_COLUMNS` | width override when `COLUMNS` is absent |
| `GARNISH_DEBUG` | write `<cache>/debug.log` |
| `GARNISH_ANIMATE` | `0` freezes every animation at frame 0 for the session and cuts a ticker line with `…` (§ 4.2) |
| `GARNISH_MANAGED_SETTINGS` | the managed settings file read first in the settings chain (§ 2.3, § 4.2, `doctor`) instead of the platform's (`/etc/claude-code/managed-settings.json`; on macOS `/Library/Application Support/ClaudeCode/managed-settings.json`), without the platform's `managed-settings.d` drop-ins and without any beside it (§ 4); empty means no managed file, which every test that runs the binary sets, and a relative path is ignored (§ 4) |
| `GARNISH_STDIN_TTY` | `1` or `0` overrides the "is stdin a terminal" check of the bare `garnish` (§ 7, § 14), so the pointer path is testable without a pty |
| `GARNISH_TEST_PANIC` | debug builds only: a tick panics before it renders, so the `⚠ garnish: internal error` row of § 5 is testable through the binary |

The boolean hooks (`GARNISH_NO_SPAWN`, `GARNISH_DEBUG`, `GARNISH_ANIMATE`,
`GARNISH_STDIN_TTY`, `GARNISH_TEST_PANIC`) follow Claude Code's rule: `1`,
`true`, `yes`, `on` on, `0`, `false`, `no`, `off` off, trimmed, any case;
anything else, empty included, is unset (`GARNISH_ANIMATE=false` freezes
like `0`). `NO_COLOR` follows no-color.org: any non-empty value turns colour
off.

## 10. Documentation

- `docs/README.md`, `docs/config.md` and `docs/modules/<id>.md` are
  generated by `garnish docs` from `ModuleSchema` and the preset, theme and
  icon tables, samples rendered under a pinned clock with no git or
  settings lookup; committed so they read on GitHub; `tests/docs_sync.rs`
  fails on drift. `docs/config.md` covers `[[row.col]]`, `title` and
  `[box.<name>]` with samples; module pages list the glyph picker's
  alternatives as *also try*.
- `docs/guide.md` is the one hand-written page under `docs/` (install,
  hook-up, first config, troubleshooting); `garnish docs` never writes it.
- `examples/garnish.toml` is what `config init` writes, kept in sync by the
  same test.
- `README.md` is for users and links to the guide and reference; `CLAUDE.md`,
  `PLAN.md` and `SPRITE.md` are for building the project.
- `presets/` (§ 12) holds named example configs; `docs/presets.md` is
  generated from them.

## 11. Assumptions

- Window size comes from `context_window_size`; 1M is assumed only when it
  is absent or zero.
- The 13k compaction buffer mirrors Claude Code 2.1.260 internals and may
  drift; it is configurable and the marker can be disabled.
- Cache TTL display uses `prompt_cache` only; absent, the placeholder (§ 3.6).
- Session duration is `cost.total_duration_ms` and resets on `/clear`.
- No GitHub network access; PR presence/state is whatever the harness reports.
- Four default lines cost four terminal rows; `compact`/`minimal` suit small
  terminals; a multi-line row (§ 4.3) costs its height. The classic renderer
  caps nothing; fullscreen gives prompt box and status line together at most
  half the terminal (§ 2.1), the budget the `setup` picker warns against.

## 12. Presets gallery

The four built-in top-level presets stay the only ones compiled into the
binary. Everything else is a **gallery preset**: a complete config file
`presets/<name>.toml`, chosen by name.

- **File contract.** A comment header the tooling parses: `# name:
  <kebab-case>`, `# summary: <one line>`, `# columns: <N>` (the sample's
  terminal width), `# needs: nerd-font` (optional; `nerd-font` | `emoji` |
  none), `# author: <github handle>` (optional). The rest is an ordinary
  config passing `config check`.
- **Gallery page.** `garnish docs` renders every preset (pinned clock,
  `subscription-full` payload, declared width) into `docs/presets.md`: name,
  summary, requirements, sample, and the file in a collapsed block.
  `tests/docs_sync.rs` keeps it in sync; `tests/presets.rs` checks each file
  validates, renders uncut, moves as promised (§ 9) and is named as its
  file; a unit test holds `# needs:` to the icon set
  (`nerd-font` for `nerd`, `emoji` for `emoji`, else nothing) and every icon
  frame list to two distinct, drawn frames.
- **Choosing one.** `garnish config init --preset <name>` writes the file
  (tooling header lines stripped); `garnish presets` lists them. The four
  built-in names keep working; a gallery preset may not reuse one (unit
  test).
- **Screenshots.** `presets/screenshots/<name>.png` are optional terminal
  captures contributed with a preset (the submit-preset skill, § 13). The
  gallery page and `garnish setup` (§ 14) are how presets are browsed; there
  is no website (the picker renders at the person's own width).
- **The set.** 32 presets, together showing every layout key and most
  module options: titles at every position, links, the compaction scale,
  cell and share widths, a boxed column, narrow and ASCII-only terminals,
  half-speed animation, a two-cell ticker, animation off, pace and eta,
  number formats, hide lists, the § 3.8 badges.

## 13. Skills

Three Claude Code skills live under `skills/<name>/SKILL.md`, are embedded
(`include_str!`) so a `cargo install` has them, and are written to
`~/.claude/skills/<name>/` (`$CLAUDE_CONFIG_DIR/skills/<name>/` when set; a
`SKILL.md` with other text is replaced behind the § 5 backup) by `garnish
install` or `garnish skills install`. Each is Markdown with frontmatter
(`name`, `description`); none needs network access from garnish; they drive
`gh` and the `garnish` CLI.

- **`garnish-statusline`.** Conversational config builder; it offers
  `garnish setup` (§ 14) first and keeps the conversation for a person who
  would rather describe. It asks, with recommended defaults: terminal and
  font (Nerd Font? → `icons`), usual width (→ preset and row count), what
  matters most, rows or columns, titles and boxes, colours, frame,
  alignment, motion, caps and links, the context scale, the reset form;
  names gallery presets showing the answers; drafts into a temp file, shows
  its `garnish preview`, then copies it over the real one behind garnish's
  `.bak-<epoch>` backup (§ 5), validates with `config check`, and explains
  tweaking. It never edits `settings.json` beyond `garnish install`.
- **`garnish-feedback`.** Files an issue on `justanotherspy/garnish` via
  `gh issue create` from a template: terminal and version, font, OS,
  `garnish --version`, `garnish config show`, `garnish doctor`, the
  rendered line (`garnish preview --color never` on a saved payload), and
  asks for a screenshot. Labels: `feedback`, plus `alignment` for width
  reports.
- **`garnish-submit-preset`.** Reads the current config; asks for a name,
  one-line summary, design width, font requirement and author handle;
  renders the sample, runs `config check`, and opens an issue labelled
  `preset` with the file (§ 12 header) and sample, asking for a screenshot.
  A maintainer turns accepted issues into `presets/<name>.toml` PRs.
- **Both reporting skills post publicly**, so each replaces the home
  directory with `~` in every path (catching what the person pasted;
  `doctor` already collapses it and `config show` prints none), keeps only
  the `GARNISH_*` lines of the doctor's environment section and the Claude
  Code renderer and compaction switches (`COLUMNS`, `LINES`, `TZ` are the
  Bash tool's, so they go), prints the whole body, and asks explicitly
  before `gh issue create`. Nothing leaves on an unanswered or negative
  answer.

## 14. Interactive setup

A full-screen `garnish setup` with an **exact** live preview (it knows the
§ 2.1 box width and renders through the tick's code) and editors
**generated from the module schemas** (type, default, choices, cap and doc
string are in `ModuleSchema`). The terminal minimum is 60 × 12.

**Where it was built differently from its first design** (each decided
while building): the draft is the config *file's* table, not a resolved
`Config` (Saving); editors are key / value / default rows with one-key
actions (`Enter` picks or types, `←`/`→` steps, `d` unsets), not checkbox,
radio and stepper widgets; glyph suggestions are one table in `icons.rs`
keyed by module and icon key, not a field on `IconSpec`; the glyph picker
prints cell counts (`|1`, `|2`), not the doctor's two-cell grid; chrome
marks are ASCII (a chip's and a set key's `*`, the preview's `>` row
marker), since some terminals draw the geometric dot and arrow two cells
wide; the placement map is `layout::Line::modules()` over
`render::render_tree_at` (each row's lines as typed pieces), not an output
of `render_lines_at`; no `setup` cargo feature was added (the release
binary grew from 2.8 MB to 3.4 MB, the cold tick did not move; the feature
stays the fallback).

**Two ways in, one file out.** Home offers *Pick a preset*, *Build a custom
layout*, *Install*, *Quit*; with a config present it opens that config in
the builder, so `setup` is also its editor. The result is always the
ordinary `garnish.toml` of § 4.

- **Preset picker.** Built-in and gallery presets (§ 12) in a list, the
  highlighted one rendered live at the real width (`COLUMNS − 4 −
  padding`) with summary, declared width and `needs`, plus a warning, on a
  line of its own under the facts it qualifies, when the terminal is
  narrower than the declared width (the `…` cut shown as on screen) or too
  short for the preset's rows in the § 2.1 fullscreen budget (`⌊LINES / 2⌋
  − 5`; the pane always states the line count). `Enter` applies it with the
  § 5 backup: a gallery preset as its file, comments included (as `setup
  --preset` writes it), a built-in as `preset = "<name>"` with its rows
  written out (where `setup --preset` writes `config init`'s annotated
  defaults); the install screen follows if settings have no `statusLine`.
  `e` opens it in the builder instead.
- **Builder.** The preview stays on top and re-renders on every change.
  Below, the `[[row]]` list shows each row's columns as chips (§ 4.3) and
  its height; keys add, insert, delete (not the last row: a file without
  `[[row]]` takes the preset's), clone and move rows, add a column and set
  `width`/`justify`, stack a column, move a module within a column or to
  the next (a new one past the edge), make a spacer, title, box a row, box
  it with the row above, box a column. `C` inserts a column after the
  selected one (a plain row's groups becoming the first) and selects it for
  `m`; `]`/`[` past the last/first column make one for the module, a plain
  row splitting from the module that leaves (a module alone in its column
  stays); `m` on a row of columns lands in the last column or last stack
  row and says so. `B` boxes the selected row with the row above, joining
  that row's named box or asking a name (the title either carried, as a
  bare key) for a new `[box.<name>]` taking that title; repeated `B` grows
  the run. Changing the top-level `preset` (or `d`, the default) swaps the
  rows for the new preset's when they were still the old preset's, and
  says which happened.
- **Undo.** Every change keeps the replaced table on a history of 100; `u`
  (`Ctrl+Z`) restores it with its list cursor, `U` (`Ctrl+R`) redoes, a new
  edit ends the redo chain, and the status line names what was undone. The
  draft is dirty exactly while its table differs from the file's (as read
  or last saved), so an undone edit leaves nothing to save and `q` does
  not ask. The hint bar is a row of buttons, a click pressing its key.
- **Module picker.** Fuzzy and initialism search (`sy` → `sync`, `sn` →
  `session_name`) over the 25 ids, the config's `text.<name>` tables and
  *New text module…*, each with its `garnish modules` summary; the new
  entry asks a name (§ 3.7 rule), creates the table with schema defaults
  and opens its form. Removing a text module's last placement (chip, line,
  or `space` making its row a spacer) asks whether to drop the table, once
  for every module so left.
- **Forms.** `Enter` on a module opens its form as an **overlay panel**:
  `preset`, `refresh` (cached module, or set in the file), `hide`,
  `label`/`prefix`/`suffix`, `hide_when_empty`, `max_width`, its own
  options, `icons.*` for the active set, `colors.*`, each with default,
  current value and doc string; enums cycle, booleans toggle, integers show
  their `max` in the input title (`max_width (0–1024)`), colours offer the
  theme's roles and a validated literal (hex or 256 index), icons open the
  glyph picker. A text module's form uses the text schema. Other forms:
  top-level keys (`preset`, `icons`, `theme`, `color`, `frame`
  style/fill/separator/`separator_color`, `align`, `durations`, `[format]`,
  `right_justify`, `overflow`, `animate`, `padding`), `[colors]`, and the
  frame, row, column and box tables. Module forms come from `ModuleSchema`,
  the rest are hand-listed; unit tests check every `OptSpec` kind and
  top-level key has an editor, the hand lists match the parser's key list
  for an unknown key, and every picker entry passes the parser. A form
  lists only keys the parser takes for that table (a row's `blank` on a
  spacer or a row of columns, its title keys outside a named box) plus any
  the file sets, so `d` can unset a reported one; a set key without a row
  of its own is listed after, showing its value, taking a TOML literal on
  `Enter`, gone on `d` (an icon's `<key>_frames`, typed as its array, and a
  text module's `color` shorthand get proper rows). A chip shows `*` while
  its module has overrides; forms mark set keys the same way.
- **Editing values.** A picker opens on the value in effect, its `custom…`
  line starting from it; the input has a cursor (`←`/`→`, `Home`/`End`,
  `Delete`). Strings keep their spaces, as do `separator_frames`, typed as
  its TOML array (`[" │ ", " ┃ "]`); `hide` and colour and number lists are
  comma-separated. `[colors]` offers literals only (the theme's, each noted
  with its role, then named colours: a role has no ground there); module
  `colors.*`, titles and boxes take roles. A `label` picker starts with the
  module's name, bare and capitalised. A value the parser takes that leaves
  another key reported (`fill = false` under `fill_pattern`) is set and the
  status names that key. A `box` unset or changed (form, `d`, or the
  builder's `b`, which reads `none`, `false` and nothing as unbox and
  `true` as a box of its own) drops a `[box.<name>]` nothing joins any
  more. `d` on the last key of a `[box.<name>]` or `[modules.text.<name>]`
  leaves the table, empty, as its presence defines it (an emptied module
  table is pruned, a box or text table is not).
- **Suggestions.** A string option (`label`, `prefix`, `suffix`, `text`,
  `gap`, a line's `separator`, `ticker_gap`, `fill_char`, caps) opens a
  picker of the distinct values the frame styles and gallery presets use
  (from the frame tables and `gallery::PRESETS` at start-up, deduplicated,
  each with its cell count), then *custom…*, reduced and width-checked as
  the parser does (§ 5).
- **Glyph picker.** One row per icon set (nerd, unicode, emoji, ascii) for
  the key, then its suggested alternatives (a robot, brain and sparkle for
  `model.icon`, three branch shapes for `branch.icon`, …), then *custom…*,
  each with garnish's cell count. A choice writes the per-key override
  (`[modules.<id>.icons] <key> = "…"`), so sets mix and `config show`
  round-trips. Suggestions pass the sets' unit test (one or two cells by
  every table, no East Asian Ambiguous character, no variation selector,
  none empty); module pages list them as *also try*.
- **Selecting in the preview.** Every module, separator, cap, rule, title
  and box edge is selectable and highlighted in place (inverse video; a
  marker on its chip). The **placement map** gives each row's cell ranges
  per element from the segment lists the painter emits (a unit test checks
  they tile each row and match painted widths, columns, stacks and ticker
  included). A module may own several ranges (one straddling the ticker
  wrap), a cut one owns its `…`, one that rendered nothing or lies outside
  the ticker window owns none and is reached by its chip. A click selects
  a module, a second click or `Enter` opens its form; the rule or a cap
  opens the frame form, a separator the frame form on `separator` or the
  row's form when the row sets one (the map names the outer row, so an
  inner row's separator opens the frame's); a title or box edge opens the
  named box's form, else the row's, and inside a row with columns selects
  that row and names the list as the way to the column or inner row. The
  wheel scrolls lists. **Every mouse action has a key** (tmux and some SSH
  sessions swallow mouse events): `Tab`/`Shift-Tab`/arrows move among
  modules, rows and columns; `2` opens the frame form (separators, caps,
  rule); `e` opens the `[box.<name>]` form of the selected line's box (its
  own, else for an inner row its column's, then its row's); the remaining
  keyboard twin is in PLAN's backlog. `Esc` closes the innermost layer and,
  at the base, leaves like `q`. Mouse capture is on while `setup` runs and
  off on exit, `Ctrl+C` and panic, through a hook chained ahead of
  color-eyre's (the report prints on a restored terminal) that acts only
  while the screen holds the terminal; a signal (`kill`, SIGTERM) runs
  neither (no signal hook in std, no crate added), so a killed `setup` can
  leave the terminal raw, alternate and mouse-reporting until `reset`; the
  README's troubleshooting says so.
- **Preview.** In-process with a live clock (`GARNISH_ANIMATE=0` freezes
  it), never reading the cache or spawning (a cached module shows its
  unrefreshed state), and, unlike `garnish preview`, with no git discovery
  and no settings read (the fixtures name no real directory); repo modules
  render from fixture fields. `f` cycles the bundled fixtures
  (`fixtures::FIXTURES`, the files of `tests/fixtures/payloads/`); `w` sets
  another terminal width (box `w − 4 − padding`; a `padding` edit
  re-shrinks it at once). ratatui ignores escape bytes, so `setup::paint`,
  a second painter target over `Painter::painted_style`, turns the same
  segments into spans; a unit test paints both ways and compares cell text
  and styles. Every row is dimmed as the harness does (§ 2.1). The preview
  honours the config's `color` and `NO_COLOR` while the chrome uses
  terminal defaults, so `color = "never"` previews plain (no bold, italic,
  underline; only the harness's faint) under a header "colours off: edits
  are saved, not previewed".
- **Saving.** `s` writes the draft with the § 5 backup, skipping a draft the
  file already holds. The draft is the file's own table (TOML, order
  kept): a save writes only the keys the file and edits carry, in the
  file's order, so unset keys stay unset, but comments survive only in the
  backup (`config show` prints the resolved form; `setup` never does). The
  status bar says so on opening a file with comments (a `#` outside every
  string) and again, before the backup path, on the save dropping them. The
  tick re-reads the config each second: no apply step. `q` on an unsaved
  draft asks once. An unparsable file is never overwritten (§ 5): `setup`
  opens on the defaults, says so, and `s` refuses until it is moved,
  re-checking at write time whatever the change check answered; the
  picker's `Enter` asks before replacing a file that appeared or changed
  since opening. A file parsing with problems opens on the per-key
  fallbacks (§ 5), the first problem and a count in the status bar, which
  also says a save writes the file's keys as they are, bad values included
  (`d` unsets one). A builder edit is refused only for a problem it adds
  (problems compared by message and index-free path, count for count: a row
  inserted above a bad one is kept, a bad row cloned is not). If the file
  changes on disk meanwhile, `s` notices (best-effort mtime and length; a
  file absent at open and present at save counts) and asks overwrite (`y`)
  or reload (`n`, undoable with `u`); `Esc`, and `Enter` on the default
  answer, do neither; it never merges. A failed save or install (read-only
  directory, unwritable `settings.json`; a symlinked settings file is
  written through the link) shows the OS error (save: status bar; install:
  its screen), keeps the draft and never exits.
- **Install.** The screen mirrors `install --dry-run` (settings path, the
  exact `statusLine` object, backup rule, skills, any PATH warning) and asks
  once; it runs `garnish install`'s code, the only route by which `setup`
  writes `settings.json`. Its plan has no config step (the draft is the
  config) and is remade when applied, so a key written meanwhile (`/voice`
  writes `voice.enabled`) is kept and the file then found is backed up.
  `setup --preset P --install` plans before writing the preset, so a
  refused settings file leaves the config untouched.
- **Non-interactive twin.** `setup --preset <name> [--install]` never opens
  the screen (§ 7), for scripts and the `garnish-statusline` skill (§ 13).
  The bare `garnish` at a terminal (stdin a tty, no payload coming) prints
  a two-line pointer at `garnish setup` and exits 0; `garnish render`
  always reads stdin and the harness always pipes, so rendering is
  unchanged.
- **Traps.** `setup` honours `--config`, `GARNISH_CONFIG` and, without
  either, the installed `statusLine.command`'s `--config` (§ 4), editing
  the file the tick reads; with no home directory and none of these, or a
  command `--config` naming no one file, it refuses with the § 5 one-liner.
  The preview fixtures are embedded (`include_str!` of named files under
  `tests/fixtures/payloads/`), so it works from a `cargo install`. Below
  60 × 12 it shows one line asking for room and takes no click and no key
  but `q`, `Esc`, `Ctrl+C`; a resize redraws everything at the new width.
- **Cost and shape.** The TUI lives in `src/setup/`, never entered on the
  render path, so the tick budget (§ 8) does not move (`bench/run.sh`
  checks). Crates: `ratatui` with the `crossterm` backend, the one new
  dependency pair. `setup` reads schemas, presets, bundled fixtures and the
  config; it runs no command and makes no network call. Screens have
  snapshot tests (§ 9).
