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

Verified against docs (code.claude.com/docs/en/statusline) and v2.1.261.
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

**The status line box is narrower than `COLUMNS`.** The harness renders it
inside its footer box, which has a fixed horizontal padding of 2 cells on
each side, and then inside a box padded by `statusLine.padding` on each
side. Each output row is an Ink `<Text wrap="truncate">`, so a row wider than

    COLUMNS − 4 − 2 × statusLine.padding

is cut with `…` on the right. garnish renders to exactly that width: the
4-cell frame is always subtracted, and the top-level `padding` key adds
`2 × statusLine.padding` when that setting is non-zero (verified in the
2.1.261 binary: footer `paddingX: 2`, status box `paddingX: padding`).

**Whitespace-only rows are dropped.** The harness trims the script's stdout
and removes every row that is empty after trimming (2.1.261:
`stdout.trim().split("\n").flatMap(l => l.trim() || [])`). The trim runs on
the raw bytes, escape sequences included, so a row is lost only when it is
whitespace *after painting*: an unframed spacer with colour off
(`color = "never"`, `NO_COLOR`) vanishes, while with colour on the rule's
colour codes around the spaces keep it (verified in the 2.1.263 binary:
no ANSI strip before the trim). `preview --color never` shows the row the
screen drops; § 4.1 `blank = true` keeps it in both cases.

**Every row is drawn dim by the harness** (target state; PLAN Phase 19).
The 2.1.261 binary renders each status line row as `<Text dimColor
wrap="truncate">`, so the whole row sits inside SGR 2 and anything garnish
leaves unpainted (the first plain segment, separators, frame glyphs) shows
at reduced intensity on screen while `preview` shows it at full intensity.
garnish prefixes every row with `ESC[0m` when colour is on, so the row
renders as `preview` shows it; a golden pins the prefix. Phase 19 confirms
the wrapper on screen first and records the fact in `CLAUDE.md` (from
FUTURE-SPEC § 7.1, A1).

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
false`, `DISABLE_AUTO_COMPACT=1` or `DISABLE_COMPACT=1` (which turns off
compaction altogether) disables the marker. The buffer constant is
configurable (`modules.context.compact_buffer_tokens`).

## 3. Modules

Every module has: `enabled` (bool), `preset` (`minimal|default|full`),
`refresh` (seconds; `0` = payload-only, rendered every tick; `> 0` = cached with
that TTL and refreshed by a worker), `icons.<key>`, `colors.<key>`, `label`,
`prefix`, `suffix`, `hide_when_empty`, `max_width`. Option resolution: built-in default →
icon-set default → module preset → top-level preset → explicit key.

`max_width` (target state; PLAN Phase 20; from FUTURE-SPEC § 6.3, A5) caps
one module's rendered width: `0` (default) is unlimited, otherwise the
module's text is cut to that many cells with `…` through `ansi::truncate`
(grapheme-aware, an OSC 8 wrapper kept balanced) *before* alignment, so a
long branch name or session title cannot push the rest of the line off
without the whole left group being cut. Capped at 1024 like every cell
count (§ 5).

### 3.1 Repo group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `path` | base dir (git toplevel, else `project_dir`) + cwd subpath | base name | `~/parent/base` + dim `/sub` | full tilde path + subpath + `added_dirs` count | 0 (toplevel cached) |
| `branch` | branch or detached HEAD | name | icon + name | + short SHA, dirty `●` | 5 |
| `sync` | ahead/behind vs `@{upstream}` | `⇡2 ⇣1` when non-zero | colored counts, the `no_upstream` glyph (`⊘`) when the branch has none | + upstream name + fetch-age hint (`stale` glyph, a space, the age: `↻ 12m`, § 4.1) | 5 (+ opt-in `fetch_interval`) |
| `worktree` | `workspace.git_worktree` / `worktree.name` | name | icon + name | + `original_branch → branch` | 0 |
| `pr` | open PR/MR | `#123` linked | icon + `#123` linked + state glyph | + state word | 0 |

GitLab merge requests render as `!7` (GitLab's own notation) with the `mr`
icon; GitHub pull requests as `#42`.

Two payload-only additions (target state; PLAN Phase 20; from FUTURE-SPEC
§ 7.5, A7 and A8):

- `path` gets `style = "full" | "fish"` and `depth = 0`. `fish` abbreviates
  every directory of the base part but the last to its first character
  (`~/r/g/src`), the way the fish shell prompts; `depth = N` keeps only the
  last `N` segments of the base part (`0` = all). The subpath stays dim and
  untouched, and both keys apply to every preset.
- `branch` gets `link = false`: `true` wraps the name in an OSC 8 link to
  the branch on the forge, built from `workspace.repo.{host,owner,name}`
  in the payload (`https://<host>/<owner>/<name>/tree/<branch>`; GitLab
  hosts use `/-/tree/`), no git call; nothing is linked when the payload
  has no `repo` or the head is detached. Only `http(s)://` URLs of
  printable ASCII are ever emitted (§ 5), as for `pr`.

PR state glyphs/colors: approved `✓` ok, pending `❍` warn, changes_requested
`✗` danger, draft `❏` muted (the unicode set; nerd uses nf-fa glyphs, see
the generated `docs/modules/pr.md`). Link uses OSC 8 to `pr.url`. (Changed
in PLAN Phase 12: `○` and `◌` are East Asian Ambiguous and drew two cells
in COSMIC Terminal.)

### 3.2 Model group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `model` | `display_name`, `⚡` when fast | name | icon + name (+⚡) | + `model.id`, thinking glyph | 0 |
| `effort` | `effort.level` | word | icon + scale `▁▃▅▇█` | scale + word | 0 |
| `context` | bar (100% = window) + % + compaction marker | `42%` | bar(20) + `42%` | bar(30) + `42%` + marker label + window tag + `exceeds_200k` | 0 (the settings chain of § 2.3 is read every tick; caching it for 30 s is the backlog's optional headroom, not implemented) |
| `style` | `output_style.name` | name unless default | icon + name unless default | always | 0 |

Context bar: filled cells `█` with partial blocks for sub-cell precision,
empty `░`; the **filled part** takes the color of the current band
(`thresholds = [50, 75, 90]`, `band_colors = ["band1", "band2", "band3",
"band4"]`: the theme's four band roles, overridable with any role or literal
colour, as in the § 4 example); a `▏` marker at the autocompact position;
`exceeds_200k = true` shows the `icons.exceeds` glyph (`‼`) in
`colors.exceeds` (`danger`) when the payload says so (one flag plus the
module's ordinary icon and colour tables, not a nested table: every module's
glyphs and colours live in `icons`/`colors`); `warn_at` adds an extra badge
threshold. No token counter. `used_percentage` null → empty bar and `–`.

`scale = "window" | "usable"` (target state; PLAN Phase 20; from
FUTURE-SPEC § 8.3, A11): with `usable` the bar and the percentage are
measured against the autocompact threshold of § 2.3 instead of the whole
window, so 100 % is the point where compaction runs (`used_percentage ×
window ÷ threshold`, capped at 100). The compaction marker then sits at
the bar's end and is not drawn; the window tag of the `full` preset still
names the real window. `window` (default) is today's behaviour. When the
marker is disabled (§ 2.3: compaction off) `usable` falls back to `window`
and `config check` says nothing, since the setting can change under a
running session.

### 3.3 Usage group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `limit5h` | 5-hour % + reset countdown | `23%` | icon + `23%` + `⏱2h13m` | + mini bar | 0 |
| `limit7d` | 7-day % + reset countdown | `41%` | icon + `41%` + `⏱3d4h` | + mini bar | 0 |
| `spend` | spend-limit % | `62%` | icon + % + reset | + bar, danger > 100 | 0 |
| `cost` | `total_cost_usd` | `$1.23` | icon + `$1.23` | + `+156 −23` | 0 |

Limit modules render nothing when their window is absent. `cost` has
`only_without_rate_limits = true` so one usage line serves both auth modes.

`reset = "countdown" | "absolute" | "both"` on `limit5h`, `limit7d` and
`spend` (target state; PLAN Phase 20; from FUTURE-SPEC § 8.2, A10):
`absolute` prints the local wall-clock time the window resets at
(`⏱ 14:30`; `limit7d` and any reset more than a day away add the weekday,
`⏱ Tue 14:30`), `both` prints the countdown followed by the time in
parentheses, `countdown` (default) is today's behaviour. The time is
formatted with jiff in the zone the `clock` module uses, so the two agree.
The harness re-runs the line at each `resets_at`, so neither form is stale
at the boundary.

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

### 3.7 Text modules (target state; PLAN Phase 15)

The 21 built-in modules stay the only ones that read the payload or run
anything. **Text modules** are the one user-defined kind: a fixed string in
a box of configurable width, declared under `[modules.text.<name>]` and
placed on a line as `text.<name>`. Any number may exist. They never run a
command, read a file or touch the cache, so they cost nothing on the tick.

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

- **Names.** `<name>` is a bare key (letters, digits, `_`, `-`), so
  `text.<name>` is one token on a line and `config show` writes the table
  back verbatim; `config check` rejects anything else. Text modules have no
  `icons` table.
- **Box.** `width = 0` makes the box exactly as wide as the text; otherwise
  the box is `width` cells and `pad` blank cells are added on both sides.
  `justify` places text narrower than the box. The module's rendered width
  is constant, which is what makes it useful as a fixed-width slot next to
  aligned columns.
- **Overflow.** `clip` cuts with the ellipsis. `scroll` shows a `width`-cell
  window that moves `step` cells to the left each tick and, when the end of
  the text has scrolled past, restarts from the beginning (no wrap-around
  text). `scroll-wrap` is the ticker: the text is followed by `gap` and
  then itself, so it flows continuously. Both are stateless: the offset is
  `floor(now_secs × step) mod period`, where the period is the text width
  for `scroll` and text plus gap for `scroll-wrap`, so a frozen clock
  freezes the scroll and a cancelled tick loses nothing. Text modules have
  no `preset` and no `refresh`; `config check` rejects both.
- **Shared primitive.** The same scroller implements line-level
  `overflow = "ticker"` (§ 4.1); one function in `ansi.rs`, tested once.
- **Escapes.** `text` is plain text: ANSI and OSC sequences are stripped,
  control characters removed, so a config cannot break the row.
- **Links.** `url = "https://…"` (target state; PLAN Phase 20; from
  FUTURE-SPEC § 7.5, A8) wraps the box in an OSC 8 link. The URL is a
  string in the config, so the module stays static; the painter's rule
  (§ 5: `http(s)://`, printable ASCII) applies, and anything else is
  reported by `config check` and dropped.
- **Docs.** `garnish modules` lists `text.<name>` as a family; the generated
  reference gets one page for it; `config check` validates `justify`,
  `overflow`, `step` (> 0) and that every `text.<name>` on a line has a
  table.

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
padding = 0               # extra cells subtracted from the width, on top of the harness's 4; set 2 × statusLine.padding
align = false             # pad each module column to the widest module in it across lines, so separators line up
right_justify = "end"     # end | start: where a padded right-group module's text sits (§ 4.1)
hide_empty_lines = true   # drop a line whose modules all rendered nothing; `modules = []` spacers stay (§ 4.1)
overflow = "truncate"     # truncate | ticker: cut or scroll a left group wider than the box (§ 4.1)
ticker_step = 1           # cells the ticker advances per tick (0.5 = every second tick)
ticker_gap = "   "        # text between the end and the wrapped-around start
animate = true            # master switch for every animation; false freezes them at frame 0 and cuts a ticker line with … (§ 4.2)
durations = "compact"     # compact (8m20s, 9m, 2h) | fixed (8m20s, 9m00s, 2h00m): how elapsed times and countdowns print; fixed by default with overflow = "ticker", and each timer module has its own (§ 4.1)

[colors]                  # role overrides: accent accent2 muted text ok warn hot danger frame band1..band4
accent = "#89b4fa"

[frame]
style = "rounded"         # none | rounded | square | double | heavy | powerline | custom
fill = true               # rule to the full width (§ 2.1) and close with the right cap
separator = " │ "
# custom: first middle last single fill_char right_first right_middle right_last right_single separator pad
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

Top-level presets: `default` (four lines above), `minimal` (one line, frame
`none`, all modules minimal: `path branch context limit5h cost` / right
`clock`), `full` (four lines, every module full), `compact` (two lines:
`path branch sync pr` / right `clock`; `model effort context limit5h cost` /
right `cache`).

Layout rules: left group joined by `separator`; right group likewise; the frame
rule fills the gap to the width `$COLUMNS − 4 − padding` (§ 2.1; never below
10); right cap after. Overflow: drop the fill, then truncate the **left** group
(ANSI-aware, `…`); never the right group. `preview --width` and
`GARNISH_COLUMNS` stand in for `$COLUMNS` and get the same subtraction, so
`preview` shows what Claude Code would show at that terminal width.

Aligned columns (`align = true`): module *k* of a group, counted among the
modules that rendered something (from the left in the left group, from the
right end in the right group), is padded with spaces to the widest module *k*
among the lines that have a module after it: the left group pads on the
right, the right group (hanging off the right cap) on the left. A line's
last module is never padded. With `fill = false` the right group follows the
left one after a separator, so the whole line is one sequence of columns,
aligned from the left; the last left module is then padded when a right
group follows it.
The separators after column *k* then fall on the same cell in every line,
so `│` bars stack vertically; bars only line up between lines that use the
same `separator`. Padding happens before truncation and fill, so an
over-wide line still loses its left group first. A column's width is the
widest module in it, so a value growing by a cell only moves the bars when
that module was already the widest.

Durations (`durations`): `compact` prints at most two units and drops a zero
second unit (`8m20s`, `9m`, `2h`, `3d4h`). `fixed` always prints two units
with the small one zero-padded to two digits (`0m47s`, `9m00s`, `2h00m`,
`3d04h`), so the width of a timer only changes when the large unit gains a
digit or the unit pair changes (`59m59s` → `1h00m`). Applies to every
elapsed time and countdown: `session`, `api`, the `cache` warm countdown,
the `limit5h`/`limit7d`/`spend` resets and the `sync` fetch age.

### 4.1 Layout keys decided on 2026-09-05 (target state; PLAN Phase 13, ticker in Phase 15)

These came out of the live config walkthrough with Daniel. They are part of
the target design; the implementation status is in `PLAN.md`.

```toml
right_justify = "end"     # end | start: where a padded right-group module's text sits
hide_empty_lines = true   # drop a line whose modules all rendered nothing
overflow = "truncate"     # truncate | ticker: what happens to a left group wider than the box
ticker_step = 1           # cells the ticker advances per tick (0.5 = every second tick)
ticker_gap = "   "        # text inserted between the end and the wrapped-around start

[[line]]
modules = []              # an intentionally empty line: a blank framed row (spacer)
blank = false             # true keeps an unframed spacer on screen with one invisible cell (§ 4.1)
```

- **`right_justify`.** With `align = true` a right-group module is padded to
  its column width. `end` (default, today's behaviour) puts the pad on the
  left so the text hugs the cap: `│          api  8m20s ─╯`. `start` puts the
  pad on the right so the text follows the separator and the gap sits
  before the cap: `│ api  8m20s          ─╯`. The left group always pads on
  the right. Columns pair *positionally*: column 3 of every line is padded
  to the same width whatever module is in it, so a `–` placeholder under a
  wide bar gets a wide blank column; the guide says so.
- **Empty lines.** A line whose every module rendered nothing (outside a
  repository, `branch`, `sync` and `pr` are all empty) is dropped when
  `hide_empty_lines = true` (default); the frame's first/last caps follow the
  surviving lines. A line configured with `modules = []` and no `right` is an
  *intentional* spacer and is always kept, drawn as an empty framed row
  (`├─ ────…────┤`). With `style = "none"` (or a custom frame with empty
  caps) a spacer is whitespace only; with colour off (`color = "never"`,
  `NO_COLOR`) Claude Code strips it (§ 2.1), so it shows in `preview` but
  not in the status line, while with colour on the rule's colour codes keep
  it. `blank = true` on the spacer (decided 2026-09-06; off by default so
  the harness's own rule stands until the user opts in) keeps it on screen
  either way: a row that would be whitespace only gets one invisible cell,
  the braille blank U+2800, which is not whitespace to the harness's `trim`
  and which a font with the clock spinner's braille should draw empty. The
  width is unchanged (an empty row, `fill = false` with no frame, becomes
  that one cell), a framed spacer needs no cell and gets none, and `blank`
  on a line with modules is reported. Setting
  `hide_empty_lines = false` restores today's behaviour for the accidental
  case too. A `[[line]]` with no keys is a spacer as well; a `modules` that
  is not a list (`modules = "clock"`) is reported and the row is an
  ordinary empty line, dropped like any other, never a spacer. An unknown
  id on a line is reported and removed, so `config show` writes only ids
  that render. With `stale_style = "hide"` a line of only cached modules can
  come and go as its values fall overdue and refresh; `hide_when_empty =
  false` on one of them pins the row.
- **Ticker.** With `overflow = "ticker"` a left group wider than its budget is
  not cut with `…`; instead the line shows a window onto the group that
  advances `ticker_step` cells to the left on every tick and wraps around,
  with `ticker_gap` between the end and the start (a news ticker). The
  offset is the § 4.2 rule, `floor(now_secs × ticker_step) mod (group width
  + gap width)`, so it is stateless, deterministic under `GARNISH_NOW`, and
  survives the harness cancelling a tick. `ticker_gap` is plain text
  (escapes and control characters stripped at config time). The right group
  is never scrolled or cut. `truncate` (default) keeps the `…` behaviour;
  `truncate = false` hands the whole row over, ticker or not. With
  animations off (`animate = false`, `GARNISH_ANIMATE=0`) a ticker line is
  cut with `…` like `truncate`, not frozen at offset 0 (decided 2026-09-06:
  a silent cut hides what is missing from the readers the switch is for).
  The `durations` default above follows the key, not the motion, so a
  frozen ticker line still prints fixed timers.
  A ticker only moves as often as the harness ticks (`refreshInterval`,
  minimum 1 s), which is the documented limit of the effect. Two
  consequences of the stateless rule (whole-stack review, 2026-09-06): the
  period is the group's *current* width, so a value in the scrolled group
  that changes width between ticks (a `compact` duration passing from `1h`
  to `59m59s`) makes the window jump instead of slide. So with
  `overflow = "ticker"` the top-level `durations` defaults to `fixed`
  (decided 2026-09-06: the smooth case is the default and the jumpy one an
  opt-in); an explicit `durations = "compact"` still wins, and every module
  that prints a timer or countdown (`session`, `api`, `cache`, `limit5h`,
  `limit7d`, `spend`, `sync`) has its own `durations = "inherit" |
  "compact" | "fixed"` to pin one module while the rest follow the
  top-level key (a right-group module is never scrolled, so it may stay
  compact). And `align` pads are inserted before the window is cut,
  so on a scrolling line they travel with the text and that line's columns
  do not stack with the others.
- **Bars.** `util::bar` uses the fractional-eighth block glyphs only when
  the fill glyph is `█`; any other fill (`━`/`─`, `▰`/`▱`) gives a bar with
  no partial cell. Some terminal fonts draw `█` a hair narrower than a cell,
  which shows as hairline gaps between filled blocks; the line-style fill is
  the documented workaround, and a per-module `bar = "blocks" | "line"`
  shorthand sets the two glyphs at once.
- **Glyph sets.** Every glyph in the built-in `unicode` and `emoji` sets
  must be one cell wide in the common terminals (COSMIC, Ghostty, Kitty,
  WezTerm, iTerm2, VS Code) or two cells by every table. Emoji sequences
  that need a variation selector (U+FE0F) are banned from the emoji set
  because terminals disagree on their width; a unit test enforces it.
- **Frames.** The `powerline` style pads its caps with one space by default.
- **Separators.** With `fill = false`, the separator between the left and
  right groups is the line's own `separator`, not the frame default.
- **`sync`.** Zero counts shown by `show_zero` use the muted role; only
  non-zero counts carry the ahead/behind colours. The fetch-age hint has a
  space between its glyph and the age like every other module.

### 4.2 Animation (target state; PLAN Phase 16)

Every animation in garnish is a pure function of the tick's clock: frame
index or scroll offset = `floor(now_secs × step) mod period`. No state is
kept between ticks, so a cancelled tick loses nothing, every session on the
machine animates in step, and `GARNISH_NOW` freezes everything for goldens.
The cadence is whatever the harness ticks at (`refreshInterval`, minimum
1 s); `step` below 1 slows an animation down (0.5 = every second tick).

```toml
animate = true            # master switch; false freezes every animation at frame 0 (a ticker line is cut with … instead)

[frame]
fill_pattern   = "·  "    # repeated across the rule instead of fill_char
fill_step      = 1        # cells the pattern shifts per tick
fill_direction = "right"  # left | right
separator_frames = [" │ ", " ┃ ", " │ ", " ╎ "]   # cycle one frame per tick
separator_step   = 1

[modules.clock.icons]
spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]  # already a spinner; same rule

[modules.branch.icons]
branch_frames = ["", ""]  # any icon key accepts <key>_frames (one width); frame 0 when animations are off
```

- **Animated rule.** `fill_pattern` is a string of one-cell glyphs repeated
  across the gap between the left and right groups; each tick it shifts
  `fill_step` cells in `fill_direction`, so dots appear to travel along the
  rule. The rule's *width* never changes (it is computed from the groups
  as today), only which glyph lands in each cell; with `align` on, the
  rule still starts at a fixed column. `fill_char` remains the static
  single-glyph case, used when no pattern is set and for a rule shorter than
  one period (a lone pattern cell would blink); with `animate` off the
  pattern sits on frame 0. `fill_pattern` with `fill = false` is reported as
  a dead key.
- **Animated separators.** `separator_frames` cycles the separator
  string one frame per tick; every frame must have the same cell width
  (validation rejects mismatched widths so columns cannot jitter), and
  `separator` stays the static case used when no frames are set; with
  `animate` off the frames sit on frame 0. Per-line `separator` overrides
  win over the frames.
- **Animated glyphs.** Any icon key in `[modules.<id>.icons]` accepts a
  `<key>_frames` list of plain-text frames that all share one width
  (validation rejects a mismatch); frame `floor(now) mod n` replaces the
  icon while animations run and frame 0 when they are off, so the static
  `<key>` is what shows when no frames are configured.
  The `clock` spinner's built-in glyph is a string of one-character frames
  cycled the same way; `spinner_frames` is the general form and takes frames
  of any one width.
- **Scrollers.** The line ticker (§ 4.1) and text modules (§ 3.7) use the
  same clock rule with a cell offset instead of a frame index. Off, a text
  module sits at offset 0 inside its declared box, while a ticker line is
  cut with `…` (§ 4.1): the box is a chosen width, the cut is not.
- **Cost.** Animation adds no I/O; the pattern and separator frames are a
  lookup, and a module with icon frames costs one clone of its resolved
  config per tick (about half a microsecond; nothing when no module has
  frames). The tick budget (§ 8) is unchanged. Docs render with
  `Clock::fixed()`, so the generated samples show frame 0.
- **Accessibility.** `animate = false` (or `GARNISH_ANIMATE=0` for a
  session) freezes everything at frame 0 and cuts a ticker line with `…`;
  the guide recommends it for screen readers and for recordings.
  **Reduced motion** (target state; PLAN Phase 19; from FUTURE-SPEC § 8.4,
  N6): when the Claude settings chain of § 2.3 (already read every tick for
  the autocompact keys) resolves `prefersReducedMotion` to `true`, garnish
  behaves as if `animate = false` unless the config sets `animate`
  explicitly; the harness honours the same key for its own spinners, so
  the two stay in step. `config show` prints the effective value.

Validation (`garnish config check`): unknown keys, wrong types, unknown module
ids, unknown presets, bad colors, animation frames of unequal width, all
reported with TOML paths; on problems the command lists them and exits 1
without an error report.

## 5. Failure behaviour

`garnish` (render) always exits 0 and always prints something:

- invalid config → keep every valid key and substitute the built-in default
  for each invalid one (the resolver already does this per key), append dim
  `⚠ config: <path>:<line> <msg>`; only a TOML syntax error falls back to
  the built-in defaults wholesale. (Decided 2026-09-05: one bad colour used
  to discard the whole file, frame and lines included, which made a typo
  look like a different program. Implemented in PLAN Phase 14: the file is
  read as a plain TOML table and each key is converted on its own; value
  errors carry the TOML path, syntax errors the line.)
- malformed stdin → `⚠ garnish: bad payload`;
- internal error → `⚠ garnish: <msg>`.
- **A file that fails to parse is never rewritten by any command** (target
  state; PLAN Phase 19; from FUTURE-SPEC § 12.1 and § 13.4). `install`,
  `config init --force`, `setup` (§ 14) and `skills install` refuse to
  touch a `settings.json` or `garnish.toml` that does not parse, name the
  problem and the file, and exit 1 quietly; the only way past is fixing or
  moving the file by hand. A file that parses is edited through a temp
  file and `rename`, with the backup rule `install` already has.
- **Nothing but text reaches a row.** Every string that becomes part of a
  row is reduced to plain text: escape sequences (CSI, OSC, and the string
  sequences DCS/SOS/PM/APC with their payloads), control characters and the
  bidi and zero-width format characters (bidi marks, embeddings and
  isolates, zero-width space and non-joiner, word joiner, the BOM; ZWJ and
  the emoji variation selector stay) are removed.
  The config's own strings (`label`, `prefix`, `suffix`, icon overrides,
  frame glyphs, separators, `text`, `gap`, `ticker_gap`) are reduced at
  config time, so width arithmetic sees the real cells; everything else (the
  payload's names and paths, git output, cache entries, the `⚠` line) is
  reduced by the `Segment` constructors, the one way onto a row. Colour and
  OSC 8 links are added by the painter alone, and a link is emitted only for
  an `http(s)://` URL of printable ASCII. (Whole-stack review, 2026-09-06:
  a `\n` in a session name added a row, an escape passed `--color never`,
  and a cut could split the sequence.)
- **Sizes are bounded.** A module cell count (`width`, `pad`, `bar_width`) above 1024, a
  row string (`text`, `gap`, `ticker_gap`, `label`, `prefix`, `suffix`) above 4096 characters or
  `cost.decimals` above 8 (the money formatter allocates one byte per place)
  is reported like any bad value and the default stands in; the renderers
  clamp again, and the effective width never exceeds 4096 cells whatever
  `COLUMNS` says. Each cap is the option's `max` in its module schema, so
  the generated reference prints it in the type column (`integer ≤ 1024`,
  `string ≤ 4096 chars`); `ticker_gap` (top-level) and `label`/`prefix`/
  `suffix` (common to every module) are checked by hand against the same
  constant. Without a home directory (`HOME` unset, no `XDG_CONFIG_HOME`) there
  is no default config or settings location: `install`, `config init`,
  `config path` and `skills install` refuse with a one-line note naming the
  flag to pass, rather than writing into the current directory. A `*_step` must lie in `0.001..=1000`: below, nothing ever moves;
  above, `now × step` saturates to a constant frame. `frame.fill_char` must
  be exactly one cell, else it is reported and the style's glyph stays.
  (Same review: `width = 9223372036854775807` aborted the tick with an
  allocation failure and a giant bar spun forever, with `config check`
  saying `ok`.)

`GARNISH_DEBUG=1` appends per-tick diagnostics to `<cache>/debug.log` (1 MB
rotation); `garnish doctor` shows the tail plus toolchain, config path/validity,
cache dir, last worker errors, and the glyph test grid (§ 7).

## 6. Cache & workers

- Root: `$GARNISH_CACHE_DIR` > `$XDG_RUNTIME_DIR/garnish` > `$XDG_CACHE_HOME/garnish`
  > `~/.cache/garnish` (macOS `~/Library/Caches/garnish`).
- `<root>/sessions/<session_id>/<module>.cache`; git data in
  `<root>/repos/<hash(git-common-dir + worktree path)>/<module>.cache` so
  sessions in one worktree share it. Never keyed on `transcript_path`.
- Entry: line 1 `v1 <computed_at_ms> <ttl_ms> ok|err`; then `key=value` lines
  or the error text. Malformed = miss. Written as `.<module>.tmp.<pid>` in
  the entry's directory + rename.
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
  A lock older than 24 h is abandoned whatever its pid says (target state;
  PLAN Phase 19; from FUTURE-SPEC § 15 item 2): a one-line safety net
  against pid reuse, which the liveness check alone cannot see.
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
| `garnish` (or `garnish render`) | render from stdin (the default; the explicit form is for a settings file that wants a subcommand) |
| `garnish refresh --module M --session S --cwd D [--all] [--lock-held]` | worker entry point; hidden from `--help` |
| `garnish install [--settings P] [--refresh-interval 1] [--padding N] [--absolute] [--no-config] [--no-skills] [--dry-run]` | merge `statusLine` into settings.json through symlinks, keeping permissions, with a never-clobbered backup; write the bundled skills (§ 13) next to it unless `--no-skills`; write default config if absent, seeded with `padding = 2N` when `--padding N` is given (N ≤ 32767; when a config already exists, a stderr note names the value to set); warn on stderr if not on PATH. `--absolute` writes `current_exe()` (a symlinked launcher resolves to its target). |
| `garnish doctor` | diagnostics; the glyph test is a grid with one row per icon set and module (plus `config` rows for the icons the loaded config resolves to, overrides included): every single-character icon is padded to two cells and followed by `\|` and the cell count garnish uses, so a glyph the terminal draws wider or narrower pushes its `\|` out of the column; multi-character icons (spinner frames, the effort scale, ASCII words) are left out. Target state (PLAN Phase 19; from FUTURE-SPEC § 13.4, N5): it also reports the settings keys that change what the line can show (`statusLine.refreshInterval`, suggesting `1` when `clock`, a countdown or an animation is configured; `statusLine.hideVimModeIndicator`, suggesting `true` when the `vim` module is on so the mode is not shown twice; `disableAllHooks`; `prefersReducedMotion`) and whether the settings file parses |
| `garnish setup [--preset P] [--install]` | the interactive setup (§ 14): a full-screen picker and builder with a live preview at the real box width; with `--preset` and no terminal it writes that preset non-interactively (the same as `config init --preset P --force` plus `install` when `--install` is given), for scripts and the skill |
| `garnish config init [--preset P] [--force] \| check \| path \| show` | config management; `init` refuses to overwrite without `--force` and accepts gallery preset names (§ 12) as well as the four built-ins; `check` lists problems and exits 1 quietly; `show` prints the fully resolved config |
| `garnish skills install [--dir D] \| list` | copy the bundled skills (§ 13) into `~/.claude/skills/` (or `D`); `install` runs this too unless `--no-skills` |
| `garnish preview <file\|dir> [--preset P] [--icons S] [--theme T] [--color M] [--width N]` | render one fixture or every `*.json` in a directory |
| `garnish docs [--out DIR]` | regenerate docs from schemas |
| `garnish modules` | list module ids + summaries |
| `garnish presets` | list the gallery presets (§ 12): name, summary, declared width, requirement |
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
  schema completeness (a scan of `src/modules/*.rs` checks that every
  icon, colour and option key the render code reads by name exists in a
  schema of a module that file defines, since `ModuleCfg` answers an
  unknown key silently; files hold several modules, so a key of one read
  by a sibling in the same file is not caught).
- **Integration** (real binary): payload fixtures (the files under
  `tests/fixtures/payloads/`: subscription, API key, pre-first-response
  nulls, no git, worktree session, git worktree, the PR states and an MR,
  spend_limit, fast_mode, 1M at 3/50/80/96 %, 200k, a cold cache, vim,
  agent, no session_name, an output style, absent effort, added dirs;
  autocompact via settings/env/disabled is driven by the environment of the
  test, and `preview` on the whole directory renders each in name order);
  temp git repos with a local bare origin (ahead, behind, diverged, no
  upstream, detached, dirty, worktree; behind and diverged use a second
  clone that pushes, `fetch_interval` end to end fetches once per interval
  and sees that push); PATH shim `git` (slow/failing) proving ticks never
  block; cache TTL expiry; live lock; stale lock with dead pid;
  `.tmp`/truncated entries ignored; the tick's whole process group killed
  after it spawned a worker whose git is slow, and the worker still writes
  the entry (the tick runs as a process-group leader and the group is
  killed with the `kill` binary, so the test fails if the worker is not in
  a group of its own); 32 concurrent ticks → exactly one worker; GC bounds.
- **Config matrix**: every preset × icon set × fixture, every frame style,
  one-module-per-line (21 rows with `hide_empty_lines = false`) and
  all-on-one-line (one row, cut with `…`) → no panic, correct line count,
  width ≤ `COLUMNS − 4` (§ 2.1); golden files under `tests/golden/`
  (`UPDATE_GOLDEN=1` regenerates).
- **Docs sync**: `garnish docs` output must equal committed `docs/`, and
  `config init` output must equal `examples/garnish.toml`.
- **Module matrix from the schema** (target state; PLAN Phase 20; from
  FUTURE-SPEC § 15 item 11): a test generated from `ModuleSchema` renders
  every module × every preset × every icon set × a few `max_width` values
  against every fixture and asserts the shared invariants (never wider
  than `max_width`, nothing rendered for a hidden state, OSC 8 wrappers
  balanced, no escape or control byte in `Segment::text`), so a new module
  or option gets the shared behaviour checked without a hand-written test.
- **Setup snapshots** (target state; PLAN Phase 21): every `setup` screen is
  rendered into ratatui's `TestBackend` at two terminal sizes and compared
  with goldens under `tests/golden/setup/` (`UPDATE_GOLDEN=1` regenerates);
  key sequences are driven through the same event loop the terminal feeds,
  so the picker, the builder and the install dialog are tested without a
  tty.

### Test hooks (environment)

| var | effect |
|---|---|
| `GARNISH_NOW` | freeze `time::now()` (epoch seconds or RFC 3339) |
| `GARNISH_CACHE_DIR` | cache root override |
| `GARNISH_CONFIG` | config path override |
| `GARNISH_NO_SPAWN` | record intended worker spawns instead of spawning |
| `GARNISH_COLUMNS` | width override when `COLUMNS` is absent |
| `GARNISH_DEBUG` | write `<cache>/debug.log` |
| `GARNISH_ANIMATE` | `0` freezes every animation at frame 0 for the session and cuts a ticker line with `…` (§ 4.2) |

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
- `presets/` (§ 12) holds complete, named example configs; `docs/presets.md`
  is generated from them.

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

## 12. Presets gallery (target state; PLAN Phase 17)

The four built-in top-level presets stay the only ones compiled into the
binary. Everything else is a **gallery preset**: a complete config file under
`presets/<name>.toml`, chosen by name.

- **File contract.** Each file starts with a comment header the tooling
  parses: `# name: <kebab-case>`, `# summary: <one line>`, `# columns: <N>`
  (the terminal width the sample is rendered at), `# needs: nerd-font`
  (optional, `nerd-font` | `emoji` | none), `# author: <github handle>`
  (optional). The rest is an ordinary config that passes `config check`.
- **Gallery page.** `garnish docs` renders every preset with the pinned clock
  and the `subscription-full` payload at its declared width into
  `docs/presets.md`: name, summary, requirements, the sample, and the file's
  contents in a collapsed block. `tests/docs_sync.rs` keeps it in sync;
  `tests/presets.rs` checks that every file validates, renders without `…`
  at its declared width, and has a unique name matching its filename.
- **Choosing one.** `garnish config init --preset <gallery name>` writes the
  file (with the header stripped of tooling lines); `garnish presets`
  lists names and summaries. The four built-in names keep working (and a
  gallery preset may not reuse one; a unit test guards it).
- **Screenshots.** `presets/screenshots/<name>.png` are optional
  real-terminal captures contributed with a preset (the submit-preset skill
  in § 13 tells people how). The gallery page and `garnish setup` (§ 14)
  are how people browse presets; a website built from them was in the
  target design until 2026-09-12 and was dropped in favour of the
  interactive setup (the picker shows a preset rendered at the person's
  own width, which no screenshot can).
- **Seed set.** The configs exercised in the 2026-09-05 walkthrough
  (`presets/` in this repository) are the first entries.

## 13. Skills (target state; PLAN Phase 18)

Three Claude Code skills ship with garnish, live under `skills/<name>/SKILL.md`
in the repository, are embedded in the binary (`include_str!`) so a
`cargo install` has them, and are written to `~/.claude/skills/<name>/` by
`garnish install` (or `garnish skills install`). Each skill is plain
Markdown with frontmatter (`name`, `description`) and instructions; none of
them needs network access from garnish itself, they drive `gh` and the
`garnish` CLI.

- **`garnish-statusline`.** Conversational config builder (the hands-on
  one is `garnish setup`, § 14; both write the same file, and the skill
  points at `setup` when the person would rather see the choices than
  answer questions). Asks, with
  recommended defaults: terminal and font (Nerd Font? decides `icons`),
  usual terminal width (decides preset and line count), what matters most
  (repo, model/context, usage limits, timers), colour preference (theme,
  or match the terminal), frame taste (rounded / powerline / none), whether
  columns should line up (`align`, `durations`), and offers a free-text
  "describe what you want" step. It writes the config with
  `garnish config init --force` semantics after showing a `garnish preview`
  of it, validates with `config check`, and explains how to tweak it. It
  never edits `settings.json` beyond what `garnish install` does.
- **`garnish-feedback`.** Files a GitHub issue on `justanotherspy/garnish`
  with `gh issue create` using a template: terminal application and
  version, font, OS, `garnish --version`, the config (`garnish config
  show`), `garnish doctor` output, the rendered line (`garnish preview` on
  a saved payload with `--color never`), and asks the person to take a
  screenshot and attach it to the issue. Labels: `feedback`, plus
  `alignment` when the report is about widths.
- **`garnish-submit-preset`.** Reads the current config, asks for a name,
  a one-line summary, the terminal width it was designed for, the font
  requirement and an author handle, renders the sample, checks it with
  `config check`, and opens a GitHub issue labelled `preset` containing the
  file with its § 12 header and the sample, asking for a screenshot. A
  maintainer turns accepted issues into `presets/<name>.toml` PRs.
- **Both reporting skills post to a public repository**, so each one first
  replaces the home directory in every path with `~` (doctor and `config
  show` print absolute paths), keeps only `GARNISH_*` lines of the doctor's
  environment section, prints the whole issue body, and asks the person
  explicitly before `gh issue create`. Nothing leaves the machine on an
  unanswered or negative question.

## 14. Interactive setup (target state; PLAN Phase 21)

Decided 2026-09-12 with Daniel, from FUTURE-SPEC § 13 (option 7.3c): a
full-screen `garnish setup` in the terminal, the way ccstatusline's TUI
works, with the two things garnish can do that it cannot: an **exact** live
preview (garnish knows the harness box width, § 2.1, and renders through
the same code as the tick) and editors **generated from the module
schemas** (every option's type, default, choices, cap and doc string is
already in `ModuleSchema`, as the docs are). It replaces the website idea
of the earlier § 12: a preset rendered at the person's own width is a
better sample than a screenshot at someone else's.

**Two ways in, one file out.** The home screen offers *Pick a preset* and
*Build a custom layout*, plus *Install* and *Quit*; when a config already
exists it opens on that config in the builder, previewed, so `setup` is
also the editor for an existing file. Whatever route, the result is the
ordinary `garnish.toml` of § 4, written the way `config show` writes it
(the round trip already exists), never anything the tick could not read.

- **Preset picker.** The four built-in presets and every gallery preset
  (§ 12) in a list; the highlighted one is rendered live on the right, at
  the real width (`COLUMNS − 4 − padding`), with its summary, declared
  width and `needs` line, and a warning when the terminal is narrower than
  the preset's declared width (the `…` cut is shown as it would be on
  screen, not hidden). `Enter` applies it: the file is written with the
  previous one kept as `garnish.toml.bak`, and the install screen follows
  if the settings file has no `statusLine` yet. `e` opens the highlighted
  preset in the builder instead of applying it.
- **Builder.** The preview pane stays at the top of every builder screen
  and re-renders on every change. Below it, the `[[line]]` list: each line
  shows its left and right groups as chips; keys add, insert, delete, clone
  and move lines, move a module within its group or between `modules` and
  `right`, and mark a line as a spacer. Adding a module opens a **picker**
  with fuzzy and initialism search over the 21 ids and the `text.<name>`
  family (`sy` finds `sync`, `sn` finds `session_name`), each with its
  one-line summary from `garnish modules`. `Enter` on a module
  opens its **editor**: one row per schema option (`preset`, `refresh`,
  `label`/`prefix`/`suffix`, `hide_when_empty`, `max_width`, then the
  module's own options, then `icons.*` for the active icon set and
  `colors.*`), showing the default, the current value and the doc string;
  enums cycle, booleans toggle, integers edit with their `max` shown,
  colours offer the theme's roles and accept a literal, icons accept any
  string and show the cell count `doctor` would. A text module's editor is
  the same screen over the text schema. Separate screens set the top-level
  keys (`preset`, `icons`, `theme`, `color`, `frame` style/fill/separator,
  `align`, `durations`, `right_justify`, `overflow`, `animate`, `padding`)
  and the `[colors]` role overrides, each with the same row shape. Nothing
  in the builder is hand-coded per option: a unit test walks every
  `OptSpec` kind and every top-level key and asserts an editor exists for
  it, so an option added to a schema appears in `setup` the next build.
- **Preview.** Rendered in-process through `render_lines_at`, exactly as
  `garnish preview` renders a fixture: the same clock (live, so animations
  move; `GARNISH_ANIMATE=0` freezes them as everywhere), no git discovery,
  no cache, no settings. `f` cycles the bundled fixtures (subscription,
  API key, before the first response, no git, the PR states, 1M at 96 %)
  so the person sees what an absent field does to their layout; `w` sets
  a width other than the terminal's to check a narrower box. The pane is
  a `Paragraph` of the rendered rows, so what it shows is byte for byte
  what the status line prints at that width, with the § 2.1 dim reset.
- **Saving.** Edits live in memory until `s` writes the file (with the
  `.bak`); because the tick re-reads the config every second, a saved
  change shows in a running Claude Code within a second, so there is no
  apply step. `q` on an unsaved draft asks once. A file that does not
  parse is never overwritten (§ 5): `setup` opens on the built-in defaults,
  says so in the status bar, and `s` refuses until the file is moved.
- **Install.** The install screen mirrors `install --dry-run`: it lists
  the settings path, the exact `statusLine` object it will merge, the
  backup name, whether the skills will be written and the PATH warning if
  any, and asks once. It runs the same code as `garnish install`; nothing
  in `setup` writes to `settings.json` by another route.
- **Non-interactive twin.** `garnish setup --preset <name> [--install]`
  without a terminal on stdout writes that preset and, with `--install`,
  hooks it up, for scripts and for the `garnish-statusline` skill (§ 13),
  which keeps its conversational path and names `setup` as the hands-on
  one. `garnish` typed at a terminal (stdin is a tty, so no payload is
  coming) prints one line pointing at `garnish setup` and exits 0 instead
  of waiting for JSON; the harness always pipes, so `render` is unchanged.
- **Cost and shape.** The TUI lives in its own module tree (`src/setup/`)
  and is never entered on the render path, so the tick budget (§ 8) does
  not move; `bench/run.sh` is the check, and a cargo feature (`setup`, on
  by default) is the fallback if binary size ever shows in the cold
  start. Crates: `ratatui` with the `crossterm` backend, the one new
  dependency pair (`inquire`, a prompt wizard, was the alternative and has
  no live pane; a WebAssembly page was the other and is the website again
  by another name). `setup` reads the schemas, the presets, the bundled
  fixtures and the config; it runs no command and makes no network call.
  Every screen has a snapshot test (§ 9).
