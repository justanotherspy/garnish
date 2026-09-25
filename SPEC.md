# garnish — Product Requirements & Technical Specification

Status: approved 2026-09-04 (`v0.1.0` the same day, `v0.2.0` on
2026-09-06); revised 2026-09-12 with the layout model (§ 4.3), the
interactive setup (§ 14) and the Phase 19–20 keys, and the same day with
what Phase 19 found in the harness (§ 2.1). The Phase 20 keys (§ 3)
shipped on 2026-09-13, the layout model on 2026-09-17 and the setup on
2026-09-19; revised the same day with the Phase 23 keys (`hide` in § 3,
pace in § 3.3, the four modules of § 3.8, `[format]` and
`separator_color` in § 4), which `PLAN.md` Phase 23 is building. Owner:
Daniel Schwartz. Builder: Claude. This document is the target design of
the whole system; when the design changes, it changes here first, with
the reason (`CLAUDE.md` § Phase protocol). Everything in it is
implemented or named as the open phase in `PLAN.md`; where something was
built differently from its first design, the section says so and why.
Progress lives in `PLAN.md`, the dated log in `WORKLOG.md`.

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
- **Composable**: 25 granular modules plus static text modules, any of them
  on any line, each with `minimal` / `default` / `full` presets; lines are
  columns of modules or stacks, with titles and boxes (§ 4.3).
- **Beautiful**: Nerd Font glyphs, smooth gradient bars, framed lines, named
  color themes, OSC 8 links.
- **Documented from code**: module docs are generated from each module's
  option schema, so they cannot drift.
- **Tested exhaustively**: real-binary integration tests over payload fixtures,
  temp git repos, PATH shims, a frozen clock, and a hyperfine latency gate.

### Non-goals

- No generic/plugin modules; the module set is fixed (text modules are
  static strings, never commands or files, § 3.7).
- No network calls (PR state comes from the harness payload).
- No Windows support. Linux and macOS only.
- No daemon. Workers are one-shot detached processes.
- The tick never writes anything but its own cache and debug log; nothing
  reads the transcript. (FUTURE-SPEC lists the proposals that would lift
  these; each is a decision for Daniel, none is taken.)

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

**Every row is drawn dim by the harness, and nothing in the output can
undo it** (read in the 2.1.261 and 2.1.270 binaries on 2026-09-12, PLAN
Phase 19). The status line component renders each row as `<Text dimColor
wrap="truncate">` around a child that parses the row's escape sequences
into per-piece style props (colour, bold, dim, italic, underline,
strikethrough, inverse, an OSC 8 link) and re-emits them through Ink; the
parent's `dim` is merged into every piece's styles, and a piece can add a
style but never clear one (a reset, `ESC[0m`, only clears the styles the
parser tracks for the text after it). So the whole row, garnish's colours
included, shows at reduced intensity on screen. **`preview` and the
`setup` pane (§ 14) draw their rows the same way** (decided 2026-09-13):
the painter folds SGR 2 into every segment it paints (`Painter.dim`; the
pane's ratatui twin sets the `DIM` modifier on every span), so what you
see there is what the screen shows, colour for colour, and a theme is
judged at the intensity it will have. `--color never` stays plain, and
the tick never adds the dim itself: the harness does, and the bytes of a
tick are what the goldens pin. FUTURE-SPEC § 7.1's A1 (a leading
`ESC[0m` on every row) assumed the raw bytes reached the terminal inside
SGR 2 and was dropped when Phase 19 read the component: the prefix would
be parsed away in every supported version. What remains is the fact, in
`CLAUDE.md` with how to re-verify it and in the guide's troubleshooting.
The harness's trim keeps every row that carries a non-whitespace
byte, so with colour on the painter's escape sequences keep a filled
spacer, and `blank` (§ 4.1) matters only with colour off or for a row
that is empty in both modes.

**Height.** `LINES` in the script's environment is the whole terminal's
row count (2.1.270: the hook runner copies `process.stdout.rows` next to
`columns`; the renderer reads the same number, falling back to 24 and
clamping at 2048). The status line component draws every row of the
output, one `<Text>` per row in a column with no cap of its own, and
nothing between it and the screen bounds its height either: the footer
(a column of the status line and the hint line, beside the mode block),
the composer (the prompt box and its notices above the footer, the
artifact panel and, outside fullscreen, the suggestions below it) and
the REPL slot carry no height, `maxHeight` or `overflow`. What a tall
status line does is decided by which of Claude Code's three renderers is
active (read in the 2.1.270 binary on 2026-09-13, `WORKLOG.md`; read,
not watched on a screen). The choice is made in this order: screen-reader
mode, tmux's `-CC` mode, Windows over SSH, `CLAUDE_CODE_NO_FLICKER=0`,
`CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN` and a crash auto-off force
*classic*; a background session and `CLAUDE_CODE_NO_FLICKER=1` force
*fullscreen*; then the `tui` key of the settings chain decides
(`"default"` or `"fullscreen"`, which `/tui` shows and sets); with it
unset, a new install starts in fullscreen for its first sessions and
server-side gates that default to off decide after that, classic
otherwise. The *split* renderer needs `CLAUDE_CODE_DECSTBM` or a server
flag of its own and yields to fullscreen.

- *classic*: nothing is cut. The frame is laid out with a width
  constraint only, so more rows make it taller; once it is taller than
  the terminal its top scrolls into the scrollback and the bottom
  `LINES − 1` rows stay in view, the status line last and the prompt box
  above it as long as the two fit in those rows (a taller block scrolls
  the prompt box's top away too, and a row that has scrolled off is never
  redrawn). Renders are cell diffs; the visible rows are erased and
  redrawn on a resize, a forced reset, or a frame that shrinks back into
  the screen, by more than a screen, or with a change in a row that has
  scrolled off; the scrollback is never touched. Every status line row
  costs a row of transcript.
- *fullscreen* (the alternate screen): the bottom block (everything under
  the transcript: the prompt box with its notices and permission wait,
  the status line, the hint line and, when shown, the inline panes, the
  artifact panel and the background-sessions line; the suggestions float
  above it) sits in a box of `maxHeight = ⌊LINES / 2⌋` (`LINES − 2` while
  a history search or an elicitation overlay is open) inside a root
  `LINES` tall, and the alternate-screen buffer drops anything laid out
  below the root. The box has no `overflow` of its own and its children
  are top-aligned (Yoga's default `justifyContent`), so its last rows go
  first: the hint line, then the status line from the bottom up. With an
  empty or one-line prompt and no notice, the prompt box's three rows,
  the composer's margin and the hint line take five, and the status line
  keeps `⌊LINES / 2⌋ − 5` rows whole (7 at 24 rows, 20 at 50); that is
  the most it ever keeps. The prompt box is not fixed: in fullscreen its
  text area shows up to `max(3, ⌊LINES / 2⌋ − 5)` lines of the draft, so
  a longer draft grows the box a row per line and takes the hint line
  and then the status line's last rows until the draft is sent, and a
  notice or the permission wait above the prompt takes its own height
  the same way.
- *split* (DECSTBM scroll regions): the bottom block is bounded to
  `LINES − 2` rows, the transcript keeps two, and overflow is again cut
  from the bottom.

The script is told nothing about the renderer (the `tui` key is in the
settings chain; the environment switches and the gates are not), so
garnish caps nothing on the tick: every configured row is printed, and
the fullscreen budget an empty prompt leaves is the rule the tools apply.
`doctor` prints the `tui` setting with the other keys (§ 7), the § 14
picker warns when a preset's row count exceeds `⌊LINES / 2⌋ − 5` for the
terminal it runs in, and a multi-line row (§ 4.3) gets no height cap of
its own: the harness cuts a too-tall block from the bottom in fullscreen
and cuts nothing in classic. Claude Code's schema takes only the two
names for `tui`: another value in the managed file is dropped on its own,
in any other file it has Claude Code reject the whole file, and `doctor`
says which (§ 7).

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

Auth-mode rule: `rate_limits` present ⇒ subscription (show limits); absent ⇒
API key/gateway (show `cost`).

### 2.3 Autocompact threshold (approximation)

Not in the payload. From the 2.1.260 binary (unchanged in 2.1.261 and
2.1.270, re-read 2026-09-12: `window − 13000`, lowered by the percentage
override): `threshold = effective_window − 13_000`, or
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
`prefix`, `suffix`, `hide_when_empty`, `hide` (a list of states, below),
`max_width`. Option resolution: built-in default →
icon-set default → module preset → top-level preset → explicit key.

`max_width` (PLAN Phase 20; from FUTURE-SPEC § 6.3, A5) caps one module's
rendered width: `0` (default) is unlimited, otherwise the decorated module
(`label`, `prefix` and `suffix` included, the stale marker too) is cut to
that many cells with `…` through `ansi::truncate` (grapheme-aware; a cut
link is still opened and closed around what is left of its text when
painted) *before* alignment and before any column or line cut (§ 4.3), so
a long branch name or session title cannot push the rest of the line off
without the whole left group being cut. Capped at 1024 like every cell
count (§ 5); `0` skips the measurement, so the default tick pays nothing.
Text modules (§ 3.7) have `width` for the same purpose and no `max_width`;
`config check` reports one and names `width`. The common options (this
one, `label`, `prefix`, `suffix`, `hide_when_empty`) are specs in
`config::schema::COMMON_OPTS`, bounded and documented like a module's own.

`hide = [...]` (PLAN Phase 23; from FUTURE-SPEC § 7.4, A4) lists the
states in which the module leaves its row, each named by the schema:
`empty` (nothing to show, what `hide_when_empty` hides), `zero` (a count
or an amount that is zero: `cost` at `$0.00`, `lines` at `+0 −0`, `sync`
at `⇡0 ⇣0`), and `below:N` / `above:N` for a module whose value is a
percentage (`context`, `limit5h`, `limit7d`, `spend`, `cache`'s hit
ratio, `api`'s share of the session, whether or not `show_share` prints
it), compared with the number the row prints, rounded as its `percent`
style rounds it (§ 4); `zero` reads an amount the same way, as printed,
so `$0.00` under two decimals and `$0` under `cost = "whole"` are zero
and `$0.004` under three decimals is not. Which states a module accepts follows from the *measure*
its schema declares (a count, an amount, a percentage, or none): every
module takes `empty`, a text module (§ 3.7) nothing else, and `config
check` names the accepted states when it refuses one. The list and
`hide_when_empty` are a union (`hide = ["zero"]` on `cost` still hides
an absent cost), so `hide_when_empty` is the older spelling of `empty`,
as `lines.hide_zero` is of `zero`, and both stay. A hidden module is a
module that rendered nothing (§ 4: not a column, its row dropped under
`hide_empty_rows`), never a `–`. The default is `[]`, so nothing on disk
changes. The rule is applied once, in the render loop, from the measure
a module attaches to its output; no module reads the list itself, and
the parser, the reference and the `setup` form all take the vocabulary
from the schema's measure.

### 3.1 Repo group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `path` | base dir (git toplevel, else `project_dir`) + cwd subpath | base name | `~/parent/base` + dim `/sub` | full tilde path + subpath + `added_dirs` count | 0 (toplevel cached) |
| `branch` | branch or detached HEAD | name | icon + name | + short SHA, dirty `✱` | 5 |
| `sync` | ahead/behind vs `@{upstream}` | `⇡2 ⇣1` when non-zero | colored counts, the `no_upstream` glyph (`⊘`) when the branch has none, the fetch-age hint (`stale` glyph, a space, the age: `↻ 12m`, § 4.1) | + upstream name (`origin/main`, or the branch name alone when the upstream is a local branch) | 5 (+ opt-in `fetch_interval`) |
| `worktree` | `workspace.git_worktree` / `worktree.name` | name | icon + name | + `original_branch → branch` | 0 |
| `pr` | open PR/MR | `#123` linked | icon + `#123` linked + state glyph | + state word | 0 |

GitLab merge requests render as `!7` (GitLab's own notation) with the `mr`
icon; GitHub pull requests as `#42`.

`sync` treats a **gone** upstream as no upstream (decided 2026-09-25 with
Daniel): when the config names one but its remote-tracking ref no longer
exists (the branch merged and deleted on the forge, then pruned), the
worker records an `ok` entry marked `gone` without counting, and the row
shows the `no_upstream` glyph, no `✗` and no new icon. (It used to run
`rev-list` against the missing ref, fail, and show `✗` for good, with a
failing worker every TTL, in the most ordinary state after a pull request.)
The fetch-age hint counts from the last fetch that *worked*: a
`FETCH_HEAD` with something in it, or the worker's own record of its last
good fetch (`fetch_ok_at`), whichever is newer, across the worktree's and
the common git dir's `FETCH_HEAD` (the tracking refs are shared). git
truncates `FETCH_HEAD` before it contacts the remote, so a failing fetch
used to read as one that had just happened and the hint never appeared; a
stamp from the future counts as no age at all.

Two payload-only additions (PLAN Phase 20; from FUTURE-SPEC § 7.5, A7 and
A8):

- `path` has `style = "full" | "fish"`. `fish` abbreviates every
  directory of the base part but the last to its first character
  (`~/r/g/src`), the way the fish shell prompts: a leading `~` is not a
  segment and stays whole, the last segment is never abbreviated, a
  dot-directory keeps its dot and first letter (`.config` → `.c`, as fish
  does), the first character is a terminal cluster, the same unit every cut
  works in (a combining mark, a skin tone or the second half of a flag
  stays with what it belongs to: cutting inside one changes the glyph
  rather than shortening it), a segment whose abbreviation would read as
  `.`, `..` or nothing at all is kept whole instead (`...` must not show
  the path as its own parent), and a root or one-segment path is
  untouched. The
  existing `depth` (last `N` segments, `0` = all; per-preset defaults 1, 2
  and 0) applies before the abbreviation and keeps the `~` as it always
  has, so `depth = 2` with `fish` on `~/repos/garnish/src` gives `~/g/src`
  (corrected 2026-09-13 from `g/src`: `shorten` never drops the `~`). The
  subpath stays dim and untouched.
- `branch` has `link = false`: `true` wraps the name in an OSC 8 link to
  the branch on the forge, built from `workspace.repo.{host,owner,name}`
  in the payload (`https://<host>/<owner>/<name>/tree/<branch>`; `/-/tree/`
  when the host is named after GitLab or the payload's open request is a
  merge request, `pr.kind = "mr"`, a host naming GitHub winning over that
  signal since `/-/tree/` on github.com is a 404), no git call; nothing is
  linked when the payload has no `repo`, an empty branch name (which would
  link to the repository root, not the page the row names), an incomplete
  `repo` or a detached head, and the name is underlined
  only when it is linked, as `pr` does. Each path part is percent-encoded
  into the URL (RFC 3986 unreserved characters and `/` kept, everything
  else `%XX` of its UTF-8 bytes), so `feature/#12` and a non-ASCII name
  link correctly and the painter's rule (§ 5: `http(s)://`, printable
  ASCII) is met; the URL carries the whole name even when `max_length`
  cut the one on screen. The host is an authority, not a path part, so it
  is used verbatim when it is one (letters, digits, `-`, `.`, and an
  optional `:port`) and drops the link when it is not (decided
  2026-09-16: percent-encoding it turned a self-hosted
  `gitlab.example.com:8443` into `…com%3A8443`, and a host holding a
  slash or userinfo would have aimed the link elsewhere). A `.` or `..`
  path segment has its dots encoded, so a payload-supplied owner or name
  cannot walk the URL up to another page once a browser normalises it.

  **Known limitation** (decided 2026-09-16, with Daniel: documented rather
  than given a key): a self-hosted GitLab whose host is not named after it
  and which has no open merge request is indistinguishable in the payload
  from a GitHub-shaped forge, so it gets `/tree/` and the link 404s. The
  two signals cover github.com, gitlab.com, any host naming either, and
  any host at all while an MR is open. A `branch.forge` override is in
  PLAN's backlog if the case ever turns up in practice.

PR state glyphs/colors: approved `✓` ok, pending `❍` warn, changes_requested
`✗` danger, draft `❏` muted (the unicode set; nerd uses nf-fa glyphs, see
the generated `docs/modules/pr.md`). The number is linked with OSC 8 to
`pr.url` when there is one the painter will emit (§ 5), and underlined only
then: a payload may carry no URL, or an `ssh://` one, and an underline
with no link reads as clickable.

(Corrected in PLAN Phase 12: `○` and `◌`, and with them `branch`'s dirty
`●` and `cache`'s `●`/`○`, are Geometric Shapes or East Asian Ambiguous and
drew two cells in COSMIC Terminal. The shipped glyphs are `❍`, `❏`, `✱` and
`✦`/`✧`; the unit test `built_in_glyphs_have_one_width_in_every_terminal`
rejects the whole block.)

### 3.2 Model group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `model` | `display_name`, `⚡` when fast | name | icon + name (+⚡) | + `model.id`, thinking glyph | 0 |
| `effort` | `effort.level` | word | icon + scale `▁▃▅▇█` | scale + word | 0 |
| `context` | bar (100% = window) + % + compaction marker | `42%` | bar(20) + `42%` | bar(30) + `42%` + marker label + window tag + `exceeds_200k` | 0 (the § 2.3 settings chain is read at most once per tick, and only when the marker, its label or `scale = "usable"` needs it) |
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

`scale = "window" | "usable"` (PLAN Phase 20; from FUTURE-SPEC § 8.3,
A11): with `usable` the bar and the percentage are measured against the
autocompact threshold of § 2.3 instead of the whole window, so 100 % is
the point where compaction runs (`used_percentage × window ÷ threshold`,
capped at 100). The compaction marker then sits at the bar's end and is
not drawn, and neither is its `⤓` percentage (`show_compaction_percent`):
on that scale it would read a constant `⤓100%` (decided 2026-09-13). The
window tag of the `full` preset still names the real window. `window`
(default) is today's behaviour, byte for byte. When compaction is
disabled (§ 2.3: `autoCompactEnabled = false`, `DISABLE_AUTO_COMPACT`,
`DISABLE_COMPACT`) or the threshold is below a tenth of the window (a
large `compact_buffer_tokens` or a tiny percentage override), `usable`
falls back to `window`, and `config check` says nothing either way, since
the settings can change under a running session; `compaction_marker`
governs drawing alone and never the scale. `thresholds` and `warn_at`
follow the percentage on display, whichever scale it is.

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
`spend` (PLAN Phase 20; from FUTURE-SPEC § 8.2, A10): `absolute` prints
when the window resets, with the module's existing spacing, in the form
that identifies the instant at the distance that window sits. The further
off, the coarser: `limit5h` resets within the day, so it prints the time
alone (`⏱14:30`) and its width is as steady as `durations = "fixed"`
promises on a ticker line; `limit7d` is days away, so it adds the weekday
(`⏱Tue 14:30`); `spend` is weeks away, so it prints the date instead
(`⏱Mar 1`, no zero padding). A clock time alone on `spend` read as
tonight when the reset was 27 days out (corrected 2026-09-16: the Phase
20 review found the committed golden saying `⏱00:00` for 1 March).
`both` prints the countdown followed by that form in parentheses
(`2h13m (14:30)`, `27d8h (Mar 1)`, the countdown in the module's
`durations` style);
`countdown` (default) is today's behaviour; `show_reset = false` hides
every form, and so does an instant that has passed, as the countdown
always did. The time is formatted with jiff in the tick's local zone,
the one the `clock` module uses unless it sets its own `tz`, so the two
agree. The harness re-runs the line at each `resets_at`, so neither form
is stale at the boundary.

Pace, eta and the elapsed view (PLAN Phase 23; from FUTURE-SPEC § 8.2
and § 8.5, A10 and N11) on `limit5h` and `limit7d`, whose windows have a
known length (5 h, 7 d); `spend` has none, so it takes none of these
keys. The window's *elapsed* share is `1 − (resets_at − now) ÷ length`,
clamped to the window, from the payload's `resets_at` alone.

- `pace = true` prints the difference between the share used and the
  share elapsed after the percentage, as `⇡14%` (ahead of pace, in
  `colors.ahead`, `hot` by default: at this rate the window runs out
  before it resets) or `⇣32%` (behind, `colors.behind`, `ok`); a zero
  difference prints `0%` in the `behind` colour. The arrows are `sync`'s,
  overridable as `icons.ahead` / `icons.behind`.
- `pace_colors = true` colours the percentage by the pace band instead of
  the `thresholds` bands: the ratio `used ÷ max(elapsed, 1 %)` is
  *nominal* at or below 1 (`colors.pace_nominal`, `ok`), *caution* at or
  below 1.5 (`pace_caution`, `warn`), *critical* above (`pace_critical`,
  `danger`); below 20 % used the ratio is noise and the thresholds bands
  stand, above 80 % used the band is critical whatever the ratio.
- `eta = true` prints, after the pace, the time until the window reaches
  100 % at the current rate (`⇥ 1h37m`: `icons.eta`, `colors.eta`, in the
  module's `durations` style): `elapsed × (100 − used) ÷ used`, and only
  when that lands before the reset; a window that resets first shows
  nothing, since nothing runs out.
- `reset = "elapsed"` is a fourth form of the reset (the three above
  stay): the time into the window over its length, `⏱2h46m/5h`,
  `⏱3d20h/7d`, for people who think in blocks rather than deadlines. It
  sits behind `show_reset` and follows `durations` like the countdown.
- `elapsed_marker = true` draws the module's `marker` glyph (`▏`; one
  cell, may be blank, the same key and rule as `context`'s compaction
  marker) on the mini bar at the elapsed share, so "60 % used at 20 %
  elapsed" is visible at a glance; it needs `bar_width > 0`.

All five are off by default (`countdown` for `reset`), and the `full`
preset leaves them off, so every existing render is byte-identical. The
harness re-runs the line at `resets_at`, and a window whose reset has
passed prints none of them.

### 3.4 Session group

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `session` | `total_duration_ms` | `1h12m` | icon + `1h12m` | + start time | 0 |
| `api` | `total_api_duration_ms` | `8m20s` | icon + `8m20s` | + `(11%)` of session | 0 |
| `cache` | prompt cache | `91%` | icon + `91%` + TTL badge + `✦ 47m`/`✧` warm countdown | + misses, writes | 0 |
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

### 3.7 Text modules (PLAN Phase 15, shipped in v0.2.0)

The 25 built-in modules (§ 3.1–3.5 and § 3.8) stay the only ones that
read the payload, a settings file or the cache, or run anything. **Text
modules** are the one user-defined kind: a fixed string in
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
- **Links.** `url = "https://…"` (PLAN Phase 20; from FUTURE-SPEC § 7.5,
  A8) wraps the box in an OSC 8 link: every segment of the finished box
  (a scrolled window's cut cells, a clipped box's ellipsis, the `justify`
  fill) carries it, the `pad` cells around it never. The URL is a string
  in the config, so the module stays static; the painter's rule (§ 5:
  `http(s)://`, printable ASCII) is checked at config time, and anything
  else is reported by `config check` and dropped rather than vanishing on
  screen.
- **Docs.** `garnish modules` lists `text.<name>` as a family; the generated
  reference gets one page for it; `config check` validates `justify`,
  `overflow`, `step` (> 0) and that every `text.<name>` on a line has a
  table.

### 3.8 Harness identity and settings badges (PLAN Phase 23)

Decided 2026-09-19 with Daniel, from FUTURE-SPEC § 8.1 and § 8.4 (A9,
A12): four more ids, the module set staying fixed at 25 (§ 0 of that
document decides the count for these four alone).

| id | shows | minimal | default | full | refresh |
|---|---|---|---|---|---|
| `version` | the payload's `version` | dim `v2.1.270` | dim `v2.1.270` | icon + dim `v2.1.270` | 0 |
| `sandbox` | `sandbox.enabled` in the settings chain | glyph | glyph | glyph + `sandbox` | 0 (the § 2.3 chain, read at most once per tick) |
| `voice` | `voice.enabled` in the settings chain | glyph | glyph | glyph + `voice` | 0 (the same read) |
| `account` | `oauthAccount.emailAddress` from `~/.claude.json` | the part before `@` | icon + the email | icon + the email | 600 (a worker; the tick reads its cache entry) |

- `version` renders nothing when the payload carries no version (so the
  default hides it and `hide_when_empty = false` shows `–`, like every
  payload module); a leading `v` in the payload is not doubled.
  `show_icon` is off except in `full`: the value is the badge.
- `sandbox` and `voice` are glyph badges: nothing unless the first file
  of the § 2.3 chain that sets the key sets it to `true`, resolved as
  `prefersReducedMotion` is (§ 4.2); `style = "glyph" | "word"` adds the
  word. They share the one settings read a tick makes, so a config
  without them reads nothing new, and `doctor` lists both keys with the
  others (§ 7). The harness hides its own voice hint when a custom
  status line is set, which is why `voice` exists.
- `account` is the one cached module outside the repo group: its worker
  (`garnish refresh --module account`, § 6; a session-scoped entry with
  a 600 s TTL) reads `$CLAUDE_CONFIG_DIR/.claude.json` when that variable
  is set and non-empty (the § 5 rule for path variables), else
  `~/.claude.json`, up to 8 MiB, and stores the email; the tick reads
  the entry as it reads `sync`'s. The file can be hundreds of KB (the
  harness keeps its own state in it), which is why it is never parsed on
  the tick. An absent file is an `ok` entry with no email (an API-key
  user has no account: nothing to show, never `✗`); an unreadable,
  unparsable or oversized one is a failed entry (`✗`, retried once per
  TTL). The field name is what the community documents for the file
  (FUTURE-SPEC grades it C), so a file without it shows nothing rather
  than guessing. `style = "email" | "user"` picks the whole address or
  the part before `@`. The settings chain of § 2.3 keeps ignoring
  `CLAUDE_CONFIG_DIR` (PLAN's backlog).
- None of the four is in a built-in preset's rows: they are added by
  hand or through the `session-badges` gallery preset (§ 12). The
  generated pages show `sandbox` and `voice` on, from keys the pinned
  clock seeds in-process (§ 9), and `account` with a note that the
  worker fills it in.

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
# animate = true          # master switch for every animation; false freezes them at frame 0 and cuts a ticker line with …; unset, follows Claude Code's prefersReducedMotion (§ 4.2)
durations = "compact"     # compact (8m20s, 9m, 2h) | fixed (8m20s, 9m00s, 2h00m): how elapsed times and countdowns print; fixed by default with overflow = "ticker", and each timer module has its own (§ 4.1)

[format]                  # number styles (Number formats, below); each module that prints a kind has the same key with `inherit`
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

Top-level presets: `default` (four lines above), `minimal` (one line, frame
`none`, all modules minimal: `path branch context limit5h cost` / right
`clock`), `full` (four lines, every module full), `compact` (two lines:
`path branch sync pr` / right `clock`; `model effort context limit5h cost` /
right `cache`).

Layout rules: left group joined by `separator`; right group likewise; the frame
rule fills the gap to the width `$COLUMNS − 4 − padding` (§ 2.1; never below
10, a floor on the box before any § 4.3 gaps); right cap after. (This
two-group line is the one-column case of the layout model in § 4.3, where
the config unit is a **row** (`[[row]]`; `[[line]]` stays as its alias)
that may hold several columns, each its own modules or a stack of rows,
with titles and boxes, one or more terminal lines tall; the rules below
then apply inside each column, and `[[row.col]]` and `[box.<name>]` are
listed there. A "column" in the aligned-columns paragraph is a module's
position within its group, not a layout column. `hide_empty_lines`
likewise becomes `hide_empty_rows` with the old name as an alias.) Overflow: drop the fill, then truncate the **left** group
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

Number formats (`[format]`, PLAN Phase 23; from FUTURE-SPEC § 7.3, A6):
one style per kind of number, each with today's rendering as its
default. `tokens`: `compact` (`12k`, `128k`, `1.0M`), `precise`
(`128,400`, thousands separated), `whole` (`128400`). `percent`: `whole`
(`42%`) or `precise` (`42.3%`, one decimal). `cost`: `precise` (`$1.23`,
`cost.decimals` places) or `whole` (`$1`), either printing `$1.2k` from a
thousand up. A
module that prints a kind carries the same-named option with `inherit`
as its default (`context` and `cache` print tokens; the limits,
`context`, `cache` and `api` print percentages; `cost` prints money), so
one module can be pinned while the rest follow the table, exactly as
`durations` works; the module pages say which kinds each prints, and a
style on a module that prints no such number is an unknown key. `parens
= "dim"` draws every parenthesised detail (`api`'s share of the session,
`lines`' net, the `both` reset form's absolute time) in the muted role,
the way a `label` is drawn: the harness already dims every row (§ 2.1),
so a bare SGR 2 would not show, and the muted role is what "dim" visibly
means. A detail is one segment when `plain` and two when `dim`, decided
in one helper, so the colour-on renders of today's configs are
byte-identical. Bands and thresholds compare the number the row prints,
whichever style.

### 4.1 Layout keys decided on 2026-09-05 (PLAN Phases 13 and 15, shipped in v0.2.0)

These came out of the live config walkthrough with Daniel.

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
  it (an unframed spacer with `fill = false` is an empty row in both
  modes and needs `blank`). `blank = true` on the spacer (decided 2026-09-06; off by default so
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
  `separator_color` (PLAN Phase 23; from FUTURE-SPEC § 6.3, A13) colours
  every separator: `muted` (default, today's role), a role or a literal,
  or `inherit`, which paints each separator in the colour of the first
  coloured, undimmed segment of the module before it (an icon or a
  value, never a `label` or an `align` pad), falling back to muted, so a
  separator reads as part of the module it follows.
- **`sync`.** Zero counts shown by `show_zero` use the muted role; only
  non-zero counts carry the ahead/behind colours. The fetch-age hint has a
  space between its glyph and the age like every other module.

### 4.2 Animation (PLAN Phase 16, shipped in v0.2.0)

Every animation in garnish is a pure function of the tick's clock: frame
index or scroll offset = `floor(now_secs × step) mod period`. No state is
kept between ticks, so a cancelled tick loses nothing, every session on the
machine animates in step, and `GARNISH_NOW` freezes everything for goldens.
The cadence is whatever the harness ticks at (`refreshInterval`, minimum
1 s); `step` below 1 slows an animation down (0.5 = every second tick).

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
  **Reduced motion** (PLAN Phase 19; from FUTURE-SPEC § 8.4, N6): when
  the Claude settings chain of § 2.3 (the same files, in the same order,
  the first file that sets the key winning) resolves `prefersReducedMotion`
  to `true`, garnish behaves as if `animate = false` unless the config
  sets `animate` explicitly; the harness honours the same key for its own
  spinners, so the two stay in step. Precedence, strongest first:
  `GARNISH_ANIMATE=0` (always off), an explicit `animate` in the config,
  `prefersReducedMotion` in the settings, the default (`true`). The chain
  is read only when the answer depends on it (no explicit key, the
  session switch on), once per tick (the context module shares the read),
  and never under the pinned clock of the docs and the in-process tests;
  the goldens run the binary and so read the chain like a real tick, with
  `GARNISH_MANAGED_SETTINGS` (§ 9) set to nothing so that no machine's
  managed file reaches them, exactly as the autocompact keys are read. A
  settings file is read up to 1 MiB and skipped past
  that, like one that does not parse (§ 5). `config show` prints the value
  the file or the settings decide for the current directory (the session
  switch is not part of a config and stays out of it); `config init`
  writes the key as a comment, like `durations`, so the setting keeps
  deciding after `init`.

### 4.3 Layout: rows, columns and boxes (PLAN Phase 21, shipped 2026-09-17)

Decided 2026-09-12 with Daniel, consolidating three ideas from that day
(grid columns, titled rules and boxes, panels of stacked boxes) into one
model. Today's line is kept as the base case, so every existing config is
already a valid instance of it and nothing changes until someone adds a
column.

**Two words, kept apart** (Daniel, 2026-09-12). A **line** is one terminal
line, the thing the harness counts and the thing `preview` prints. A
**row** is the addressable unit of the config, `[[row]]`, one or more
lines tall: a bare row is one line, a boxed row is three (a frame line,
its content, a frame line), and a row whose content is itself several
lines tall (a module drawing art or an animation frame, a direction left
open and not designed here) would follow the same rules. Frame lines are
drawn, never configured: nothing in a config addresses the top of a box.
`[[line]]` stays accepted as an alias of `[[row]]` for every config
written before this model, and `hide_empty_lines` as an alias of
`hide_empty_rows`; `config show` writes the new names, `config check`
says nothing about the old ones, and no config ever breaks over the
rename.

**The model in five sentences.** A config is a list of **rows**. A row is
made of **columns** side by side; a row written with `modules`/`right` and
no `[[row.col]]` is one column that fills the width, which is exactly
today's line. A column holds either its own modules or a **stack** of
rows, and columns share the width by `width`. Inside a column, `modules`
with `right` is the flex form (the groups anchor to the column's edges and
the rule fills between; today's rule), while `modules` alone sits where
`justify` says. **Titles** decorate rules and **boxes** decorate rows and
columns; the tree is two levels deep and never deeper, so `setup` (§ 14)
can always draw it.

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

- **Columns and width.** The row's width is the box of § 2.1 minus
  `gap` cells per boundary. A column with `width = 24` takes 24 cells and
  `"auto"` takes its content's width (its modules joined by the
  separator; `max_width` applies); what is left is the free width, shared
  by the `fr` columns as `floor(free × n ÷ Σ fr)` each, the leftover
  cells going one each to the first of them, so shares differ by at most
  one cell and always add up. Defaults: `"1fr"`, so three bare columns
  are thirds and six are sixths. Content wider than its column is cut
  with `…` (`overflow = "truncate"`) or scrolled inside the column
  (`overflow = "ticker"`) and never spills into a neighbour, which is
  what keeps a layout's shape as the terminal is resized. Scrolling per
  column follows § 4.1: in a flex column the left group is the window and
  the `right` group is never scrolled; a lone group is the window; each
  inner row of a stack is its own window; an `auto` column always fits
  its content and so never scrolls. `truncate = false` means the last
  (or only) column's content is not cut and may run past the box, exactly
  § 4.1's rule for a plain row; every other column still never spills.
  An `auto` column is re-measured every tick and moves its neighbours as
  its content changes width, so it is for values that hold still (a
  clock under `durations = "fixed"`, a module with `max_width`), not for
  branch names; an `auto` flex column joins its two groups with the
  separator and draws no rule between them, and an `auto` stack is as
  wide as its widest inner row. With no `fr` column at all, the free
  width is a rule after the last column. When the width runs out, the
  row is laid out left to right, gap then column: a fixed or `auto`
  column takes at most what remains, and a column whose gap plus one
  cell does not fit renders nothing, as does everything to its right
  (the row width is `max(box − gaps, 0)`; the case only arises below
  the § 4 minimum of 10 cells or with fixed widths that exceed the box,
  and `GARNISH_DEBUG` logs it). (Decided with Daniel 2026-09-12: one
  `width` key taking `"<n>fr"`, `"auto"` or an integer, in preference to
  three single-typed keys, and `gap = 1` by default so adjacent columns
  never touch without tuning; `config check` names the three accepted
  forms when a `width` is a quoted number or anything else.)
- **Inside a column.** `modules` with `right`: the flex line of § 4, laid
  out to the column's width (left group anchored left, right group
  anchored right, the fill between, the left group cut first). `modules`
  alone: one group placed by `justify`, whose default follows the
  column's position (the first column left, the last right, any middle
  column centre; a lone column left), so a three-column row reads
  left / centre / right without writing it. `align = true` (§ 4) pads
  the module at position *k* of column *c* to the widest module at
  position *k* of column *c* across the rows with the same column count
  (a position is a module's place within its group, the "column" of § 4;
  rows with different column counts never align with each other, and
  inner rows of a stack align only with inner rows at the same column
  position), so separators stack; *k* counts from the left in a left- or
  centre-justified column and from the right end in a right-justified
  column or a `right` group, as § 4 does today; `right_justify` picks the
  pad side for those.
- **Stacks and height.** `[[row.col.row]]` entries make the column a
  stack of rows, each laid out to the column's width with the rules above
  (an inner row's `justify` overrides the column's). A row's height in
  lines is its content's: a bare row is one line, a boxed row its lines
  plus two; a column's height is the sum of its rows'; the outer row is
  as tall as its tallest column. A shorter stack is padded with empty
  lines placed by `valign` (which has no effect when every column is one
  line tall). Inner rows take no `[[row.col]]` and no `gap`; a column
  with both `modules` and inner rows is reported and the stack wins.
- **Frame and fill.** The `[frame]` caps sit at both ends of every
  terminal line that is not a box line: `first`, `middle` and `last` are
  decided over those lines in order, so a tall row's own lines carry
  `middle` between the first and the last, and a config whose only lines
  are inside boxes shows no cap at all (the box draws its own ends). A box
  line carries the box's corners and sides at its ends instead, as the
  samples show. (Decided while building Phase 21: an earlier wording made
  a multi-line row one block for this, which would have repeated `╭─` on
  every line of a tall row.) On a one-line row, `fill` draws the rule glyph (or the
  animated `fill_pattern`) in every empty cell inside the caps, gaps
  included, so a centred module floats on one continuous rule:
  `╭─ path ─── ⏱ 2h13m ─── 12:00:00 ─╮`; the pattern's phase is
  `(rule-cell index + frame) mod period`, the line's rule cells numbered
  together and the cells a module occupies taking no pattern cell with
  them, and the shorter-than-one-period fallback of § 4.2 counts those
  same cells, so the dots travel across column boundaries instead of
  restarting at each (a one-column row is unchanged). On a multi-line
  row it draws only inside each inner row's own cells; gap cells and
  padding lines are spaces, since a rule running past a box's side would
  look wrong. With colour off the harness drops a line that is spaces
  only (§ 2.1); `blank` on the outer `[[row]]` of a multi-line row keeps
  every one of its lines (the braille cell on any line that would be
  whitespace only, padding lines included), while on an inner row it
  follows the § 4.1 rule.
- **Titles.** `title` is plain text (reduced like every config string,
  § 5) set into the row's rule in the frame colour with `title_pad`
  spaces on each side; `title_color` picks another role or literal.
  `title_justify` puts it right after the left cap, centred, or right
  before the right cap. A line that carries modules has several runs of
  empty cells, one per column and one per `gap`: a centred title goes in
  the widest of them, a left one in the first run wide enough to hold it
  and a right one in the last, and either falls back to the widest run
  when no run at its end can hold it (the alternative is a title cut to
  its ellipsis in a one-cell gap). A title needs no rule: with `fill = false` or `style = "none"`
  it is the same text at the same place with `title_pad` spaces around
  it. On a multi-line row the title goes into the first line. On a
  `box = true` row the `title*` keys title that anonymous box (the one
  way to title a one-row box); a row inside a named box gets no title
  of its own (the box has one) and the key is reported and ignored. A
  title wider than its space is cut with `…` and never widens
  the line. A `[[row]]` with only a `title` is a titled spacer
  (`├─ Repository ────┤`), always kept (§ 4.1).
- **Boxes.** `[box.<name>]` (a bare key, as for text modules) carries a
  title (the four `title*` keys), `style`, `fill` and `color` (the box's
  glyphs; a role or literal, like a text module's `color`).
  `style` and the colour inherit from `[frame]` when absent, so a
  `double` box can sit in a `rounded` frame; when the frame's style has
  no box shape (`none`, `powerline`) an unstyled box is `rounded`. `fill`
  defaults to `false` inside a box (a clean interior is what a box is
  for; `fill = true` draws the rule between a row's groups as § 4 does
  outside one). A box is drawn as a corner-capped top rule with the
  title, the style's side glyphs at both ends of each line inside (the
  row laid out to the width between them), and a bottom rule: two extra
  lines, so a box is at least three lines tall. Three ways to join one:
  adjacent rows with the same `box = "<name>"` form one box spanning
  them; `box = "<name>"` or `box = true` on a column makes the whole
  column one box the outer row's full height, its padding lines drawn as
  empty interior lines, so a one-box column matches a three-box
  neighbour (a boxed one-line column is a three-line box); `box = true`
  on a row boxes that row alone with no title, so three adjacent
  `box = true` rows are three boxes. Boxes never nest: a row inside a
  boxed column may not carry `box`, and a column may not carry `box` on
  a row that has one; both are reported and the inner box ignored. The
  three ways read the same wherever the rows are: adjacent rows of a
  stack naming one box form one box in that column, as adjacent
  `[[row]]`s do. A name reused for a non-adjacent run is reported and
  the second run unboxed, and the run is one run in the whole config,
  not one per list: a stack is a run of its own, so a name inside one is
  never adjacent to a name outside it, and a name a column's own `box`
  has taken is taken. Lines outside every box keep the frame's caps as today. The
  built-in styles gain their corners and side: `rounded` `╭ ╮ ╰ ╯ │`,
  `square` `┌ ┐ └ ┘ │`, `double` `╔ ╗ ╚ ╝ ║`, `heavy` `┏ ┓ ┗ ┛ ┃`, with
  `fill_char` as the horizontal; `none` draws an invisible box (lines
  indented by the pad); `powerline` has no box shape and is reported and
  drawn rounded. A `custom` frame adds `top_left`, `top_right`,
  `bottom_left`, `bottom_right` and `side`, one cell each (reported
  otherwise, the style's glyph stays); every glyph passes the § 4.1
  width guard.
- **Hiding.** A module hidden by `stale_style = "hide"` or
  `hide_when_empty` leaves its row (§ 3.6, § 4.1; under the default
  `stale_style = "dim"` a stale value stays, dimmed); under
  `hide_empty_rows` an inner row whose modules all rendered nothing is
  dropped (its stack shortens and the outer row's height follows the
  tallest column that remains), a box whose rows all went is dropped
  with its frame lines (a title alone keeps nothing), and a row is
  dropped when every column is empty. A column that emptied while a
  sibling did not keeps its share and renders empty lines, so the layout
  does not reflow when a value comes and goes (`stale_style = "hide"`
  would otherwise move columns every `stale_after` TTLs). A column with
  no `modules` and no inner rows, or with `modules = []`, is an empty
  column that keeps its share; a `[[row]]` is a spacer only when every
  column is empty.
- **Edge cases, stated so nobody guesses.** One column has no boundary,
  so `gap` does nothing there and is not reported. An inner row's own
  `separator` wins over the outer row's, as a row's wins over the
  frame's. A column that scrolls carries its `align` pads inside the
  window (§ 4.1), so a scrolling column's separators do not stack with
  its neighbours'. `hide_when_empty = false` on a module pins its inner
  row, as it pins a top-level row. Text modules sit in columns like any
  module; a text module with `overflow = "scroll"` inside an `auto`
  column has a fixed box width, so the column holds still. A `[[row]]`
  with `[[row.col]]` entries and also `modules` or `right` at the row
  level is the reported case above; a row with `right` and no columns is
  the one-column row `[[line]]` has always been, and is not reported. `padding` (§ 4) shrinks the box before columns are
  shared. A `[box.<name>]` nobody joins is reported as unused. `config
  init` writes no columns or boxes into the annotated default file; they
  appear as a commented example, as text modules do.
- **Pads, decided while building Phase 21.** A rule never runs into a
  module's text: a column keeps one `pad` cell on each end its *content*
  reaches (a flex column's two groups, a left- or right-justified lone
  group, and both ends of an `auto` column, whose declared width is its
  content plus those cells), and none where the rule already surrounds the
  group or where the frame's cap or the box's side has padded it. A box's
  interior pad is the frame's `pad`, or one cell when the frame has none,
  so a box never has its content against its side and a `style = "none"`
  box indents by it. A title right after a cap drops its own leading pad
  for the same reason (`├─ Repository ──┤`, not `├─  Repository`), and the
  cell goes back to the rule. A box's own top and bottom rules are static:
  `fill_pattern` belongs to the frame, and a travelling box edge would
  read as an error.
- **Validation.** `config check` reports: `justify`/`valign` outside
  their words; a `width` that is not `"<n>fr"` (1–64), `"auto"` or a cell
  count (≤ 1024); `gap` above 16; more than 16 columns on a row or 16
  inner rows in a column; `title_pad` above 64; `box` naming no
  `[box.<name>]`; a row with both `modules` and `[[row.col]]` (the
  columns win); nesting in either direction; a non-adjacent reuse; a
  title on a row inside a named box; `[[line]]` and `[[row]]` both
  present in one file (the arrays cannot be ordered against each other;
  the file must use one name). `config show` writes a one-column row in
  the plain `[[row]]` form, writes `[[row.col]]`, `[[row.col.row]]` and
  `[box.<name>]` only where they are configured, rewrites the aliases,
  and drops every reported key as it drops unknown ids today, so its
  output is a fixed point that `config check` calls `ok`.
- **Setup.** The builder (§ 14) shows a row as its columns side by side:
  *Add a column*, its `width` and `justify`, *Stack* to turn a column
  into rows, *Add a title*, *Wrap in a box* over a selected run of rows
  and *Box the column*; the placement map lists every inner row, title
  and box edge so a click lands on the right thing.
- **Cost.** Layout is arithmetic over the segment lists the modules
  already render; nothing new is read or spawned. Presets `grid-three`,
  `grid-six`, `boxed-panels` and `dashboard-panels` pin the shares, the
  titles and the stacks at two widths each.

Two samples, each drawn at its box width (§ 2.1: a 40-cell box is a
44-column terminal). A titled box around two rows, in a 40-cell box; four
lines on screen for two rows of config:

```toml
[box.repo]
title = "Repository"
[[row]]
box = "repo"
modules = ["path", "branch", "sync"]
right   = ["pr"]
[[row]]
box = "repo"
modules = ["worktree"]
```

```text
╭─ Repository ─────────────────────────╮
│ ~/p/garnish  main ⇡2             #42 │
│ wt/review                            │
╰──────────────────────────────────────╯
```

A dashboard row of three columns in a 60-cell box with `[frame]
style = "none"` (so the unstyled `box = true` boxes are `rounded`): a
double box the full height, a bare centred column, three stacked boxes;
one row of config, nine lines on screen:

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
- **A file that fails to parse is never rewritten by any command** (PLAN
  Phase 19 for `install` and `config init --force`, Phase 22 for `setup`;
  from FUTURE-SPEC § 12.1 and § 13.4). `install`, `config init --force`
  and `setup` (§ 14) refuse to touch a `settings.json` that is not a JSON
  object or a `garnish.toml` with a TOML syntax error (a file with bad
  values parses and is replaced), name the problem and the file on one
  stderr line, and exit 1 quietly, a dry run included; the only way past
  is fixing or moving the file by hand. A file that parses is replaced
  through one function (`install::replace_file`): through a symlink,
  keeping the old file's permissions, after a timestamped backup next to
  it that is never overwritten, via a temp file in the same directory and
  a `rename`; `config init` names the backup it kept.
- **Nothing but text reaches a row.** Every string that becomes part of a
  row is reduced to plain text: escape sequences (CSI, OSC, and the string
  sequences DCS/SOS/PM/APC with their payloads), control characters and the
  bidi and zero-width format characters (bidi marks, embeddings and
  isolates, zero-width space and non-joiner, word joiner, the BOM; ZWJ and
  the emoji variation selector stay) are removed.
  The config's own strings (`label`, `prefix`, `suffix`, icon overrides,
  frame and box glyphs, separators, `text`, `gap`, `ticker_gap`, the
  row and box `title`) are reduced at
  config time, so width arithmetic sees the real cells; everything else (the
  payload's names and paths, git output, cache entries, the `⚠` line) is
  reduced by the `Segment` constructors, the one way onto a row. Colour and
  OSC 8 links are added by the painter alone, and a link is emitted only for
  an `http(s)://` URL of printable ASCII. (Whole-stack review, 2026-09-06:
  a `\n` in a session name added a row, an escape passed `--color never`,
  and a cut could split the sequence.)
- **Sizes are bounded.** A module cell count (`width`, `pad`, `bar_width`, `max_width`) above 1024, a
  row string (`text`, `gap`, `ticker_gap`, `label`, `prefix`, `suffix`, `title`) or a text module's `url` above 4096 characters or
  `cost.decimals` above 8 (the money formatter allocates one byte per place)
  is reported like any bad value and the default stands in; the renderers
  clamp again, and the effective width never exceeds 4096 cells whatever
  `COLUMNS` says. Each cap is the option's `max` in its module schema, so
  the generated reference prints it in the type column (`integer ≤ 1024`,
  `string ≤ 4096 chars`); the common options (`label`/`prefix`/`suffix`,
  `hide_when_empty`, `max_width`) are specs in `COMMON_OPTS` and go
  through the same code path, and `ticker_gap` (top-level) is checked by
  hand against the same constant. A Claude settings file of the § 2.3 chain (which a cloned
  repository can contribute to) is read up to 1 MiB and skipped past
  that; `doctor` shows a `statusLine.command` from any of them as plain
  text, cut to 200 characters. The `account` worker reads `~/.claude.json`
  up to 8 MiB and records a failed entry past that (§ 3.8); a `below:N`
  or `above:N` hide state takes `N` up to 1000 (§ 3). Without a home directory (`HOME` unset, no `XDG_CONFIG_HOME`) there
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
  > `~/.cache/garnish` (macOS `~/Library/Caches/garnish`) > the temp
  directory's `garnish-<uid>`, created `0700` and **refused** (no cache: no
  entry read or written, no lock, no worker, and `doctor` says why) unless
  it is a real directory this user owns that nobody else can write to
  (review 2026-09-25, decided with Daniel: the old `/tmp/garnish` was shared
  by every user, so another could read and plant entries or aim the sweep
  and the temp-file writes through a link; the uid comes from a file the
  process creates, there being no `libc`). Directories garnish creates are
  `0700` and its files `0600`: an entry may carry the account's email.
- `<root>/sessions/<session_id>/<module>.cache`; git data in
  `<root>/repos/<hash(git common dir + per-worktree git dir)>/<module>.cache`
  so sessions in one worktree share it. Never keyed on `transcript_path`.
- Entry: line 1 `v1 <computed_at_ms> <ttl_ms> ok|err`; then `key=value` lines
  or the error text. Malformed = miss, and so is anything that is not a
  regular file of at most 64 KiB (a FIFO would block the tick in `open`;
  locks are read the same way). The error text, and `fetch_error`, are
  plain text of at most 500 characters: control characters and escape
  sequences are dropped, since `doctor` prints them to a terminal. Written
  as `.<module>.tmp.<pid>` in the entry's directory + rename; every
  temporary name is unlinked first and created exclusively, so a link
  planted at one is never followed. `ttl_ms` is informational: freshness is
  always the reader's TTL. `account` (§ 3.8) keeps
  `<root>/sessions/<session_id>/account.cache` with an `email` line; an
  absent `.claude.json` is an `ok` entry without the line.
- Tick: fresh → render; past TTL → spawn worker unless `<module>.lock` is
  live, rendering the last value unchanged; older than `stale_after` TTLs
  (or computed for another head/upstream) → dim `⟳`; `err` → dim `✗`. A failed entry is fresh for its TTL like any
  other (a broken git is retried once per TTL, never once per tick). Entries
  record what they were computed for (`branch`'s `head`, `sync`'s `branch`
  and `upstream`: branches that share an upstream must not share counts);
  a render whose situation differs treats the entry as stale.
- Lock = file `pid epoch_ms`, created by `hard_link` from a pre-written temp
  file and re-stamped by `rename` (never truncated in place). Live when
  younger than 2 s (hand-over window), else while the pid exists (Linux,
  `/proc`) and it is younger than 60 s (30 s where pids cannot be checked:
  longer than a `sync` worker's 20 s fetch plus 2 s count, which a
  compile-time assertion keeps true, or the next tick reclaimed a lock
  mid-fetch and a second fetch started). A stale lock is reclaimed by an
  atomic rename, and the moved file is read back: one that is not the lock
  judged dead was another process's fresh lock and is linked back, so at
  most one process wins each reclaim. A guard only unlinks a lock that
  still carries its own pid. A root where no lock can be taken (a
  filesystem without hard links) is logged by the tick (`GARNISH_DEBUG`),
  recorded by the worker as a failed entry (a rename still works, so the
  row shows `✗` and the TTL spaces the retries), and named by `doctor`'s
  probe. (FUTURE-SPEC § 15 item 2 proposed a 24 h horizon against pid
  reuse; the 60 s / 30 s age limit above already bounds a lock's life
  whatever its pid, so nothing was added.)
- Worker: `garnish [--config C] refresh --module M --session S --cwd D`,
  null stdio, `process_group(0)`, spawned without wait. `--config` names the
  file the tick loaded (absolute), when it loaded one, so the worker reads
  the same options: a `--config` on the status line command is not in the
  environment the worker inherits, and it used to re-resolve the config
  and take `sync.fetch_interval` from another file (review 2026-09-25).
  On Linux the tick takes the lock and passes `--lock-held`; elsewhere the
  worker takes it itself. `GARNISH_NO_SPAWN=1` logs intended spawns to
  `<root>/spawns.log` instead.
- `refresh` must be ≥ 1 for cached modules (`config check` rejects 0).
- GC: bounded sweep when a worker writes a module's first entry in a scope,
  session or repo (session and repo dirs idle > 24 h by wall-clock mtime,
  ≤ 50 per sweep; temp/stale/adopt files older than 1 h), never on the
  tick; `garnish gc` for manual runs. (Corrected 2026-09-25: it used to
  wait for a new session *directory*, which the lock always created first,
  so the automatic sweep never ran for anyone.) It touches only what
  garnish would have made, since the root may be shared
  (`GARNISH_CACHE_DIR=~/.cache`): `sessions` and `repos` and each directory
  in them only as real directories (never through a link), a repo
  directory only when its name is a 16-digit hash and a session one only
  when it is a sanitised id, and either only when every file in it has one
  of garnish's own names.
- **No child process on a warm tick.** Branch/upstream/HEAD are read from
  `.git` files (loose refs, `packed-refs` scanned as bytes with early exit,
  worktree `gitdir`, symref chains capped at 5). Every such read is a
  bounded read of a regular file (a FIFO or a link to `/dev/zero` is
  refused, not opened: an archive can carry either and the tick repeats
  the read every second), contained in the git directory, and a symbolic
  ref may only point under `refs/` or at a capitalised pseudo-ref, as git's
  own `refname_is_safe` has it; a `.git` file's `gitdir:` and a `commondir`
  count only when they name a git directory by git's test (a `HEAD`, an
  `objects/` and a `refs/`), since the containment is relative to them
  (review 2026-09-25: `commondir: ~/.ssh` rendered a key's first line as
  the SHA). The upstream comes from `.git/config`, read up to 1 MiB (a
  byte that is not UTF-8 costs only what it touches) and parsed as git
  parses it: quoted values, the escapes, `;`/`#` comments, section and key
  names in any case, the first `merge` and the last `remote` (git quotes a
  value holding `#`, so `fix/#12` used to read as a tracking ref with
  quotes in it and `sync` showed `✗`). Reftable repos (whose refs are not
  files) report no head to the tick and fall back to the workers (review
  2026-09-25, decided with Daniel: this sentence used to be all there was,
  and both modules rendered nothing): `branch`'s worker asks git
  (`symbolic-ref -q --short HEAD`, else `rev-parse --verify HEAD` for a
  detached one, plus the commit for `show_sha`) and records `branch` and
  `detached`; `sync`'s resolves the branch the same way, reads the upstream
  from the config and checks its ref with `show-ref --verify`, recording
  `no_upstream` or `detached` as values the tick shows rather than
  failures. Both entries carry `tables`, the mtimes of the worktree's and
  the common `reftable/tables.list` (taken before git is asked), and the
  tick treats an entry whose `tables` differs from what it stats as for
  another state of the refs. A `HEAD` the tick refuses in a files
  repository (a link out of the git directory) leaves `branch`'s entry
  without a `head` key, which any render accepts (an empty one matched
  nothing, so every tick spawned a worker). Ahead/behind, dirty, and fetch run in the
  worker only through `git::run_program` (pipes drained on threads with 1 MiB
  of stdout and 64 KiB of stderr kept and the rest discarded, kill on
  timeout: 2 s for local commands, 20 s for `fetch`). `git` is the first
  executable on an *absolute* `PATH` entry, looked up once (an empty or
  relative entry would find a `git` the checkout ships, since the child
  resolves the name after its `chdir`); every call clears `core.fsmonitor`,
  sets `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0` and
  `GIT_NO_LAZY_FETCH=1` (no lazy fetch in a partial clone), and removes
  `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and the other variables that
  point git elsewhere, so git finds the repository from the directory as the
  tick did.
- **The dirty check never reads a worktree file** (decided 2026-09-25 with
  Daniel): `git status` hashes every file whose stat data no longer
  matches the index, through the `clean`/`process` filter driver the
  repository's own `.git/config` defines, so in an unpacked archive it ran
  that command on every refresh. `dirty` is `git diff-index --cached --quiet
  HEAD` (anything in the index before the first commit) plus `git -c
  core.checkStat=default diff-files --quiet --ignore-submodules=dirty`,
  which compare stat data and stop at the first difference; the stat rule
  is pinned so a repository cannot relax it until its files look "racily
  clean" and get hashed, and a submodule's own dirtiness (a `git status`
  inside it) is not asked. The accepted cost: a file touched without
  changing reads as dirty until the user's own git refreshes the index.
  `status.showStash` no longer matters (porcelain printed `# stash N`).
- **Fetch** (opt-in, `fetch_interval`) passes `--no-auto-maintenance`,
  `--recurse-submodules=no`, `--upload-pack git-upload-pack` and the remote
  after `--` (a name starting with `-` is refused), and sets
  `SSH_ASKPASS_REQUIRE=force` with `SSH_ASKPASS` a program that fails
  (`false`, found as `git` is): the worker keeps Claude Code's controlling
  terminal, and ssh would otherwise draw a host-key or passphrase prompt on
  it (OpenSSH 8.4 and later honour it; nothing else in git's environment
  stops ssh reading `/dev/tty`). `core.sshCommand`, `core.gitProxy`, an
  `ext::` URL, hooks and credential helpers stay a backlog decision (PLAN).
  A failed fetch is recorded in the entry (`fetch_error`, `fetch_attempt`)
  without hiding the local counts and is not retried within `fetch_interval`;
  a fetch that works records `fetch_ok_at`. Between attempts all three are
  carried from the previous entry, so the error lasts until a fetch works
  (it used to vanish at the next refresh), and `doctor` lists every entry
  carrying one as `FETCH FAILED`. A fetch is due when both the entry's
  `fetch_attempt` and `FETCH_HEAD`'s mtime (the newest of the two
  worktree files) are at least `fetch_interval` old, a stamp from the
  future counting as due.

## 7. CLI

`--config FILE` is a global flag on every command and overrides
`GARNISH_CONFIG` and the default location.

| command | purpose |
|---|---|
| `garnish` (or `garnish render`) | render from stdin (the default; the explicit form is for a settings file that wants a subcommand). The bare `garnish` with a terminal on stdin prints a two-line pointer at `garnish setup` and exits 0 instead of waiting (§ 14; `GARNISH_STDIN_TTY` pins the check, § 9); the explicit `garnish render` always reads stdin |
| `garnish refresh --module M --session S --cwd D [--all] [--lock-held]` | worker entry point; hidden from `--help`; the tick passes its own `--config` ahead of it (§ 6) |
| `garnish install [--settings P] [--refresh-interval 1] [--padding N] [--absolute] [--no-config] [--no-skills] [--dry-run]` | merge `statusLine` into settings.json through symlinks, keeping permissions, with a never-clobbered backup; write the bundled skills (§ 13) next to it unless `--no-skills`; write default config if absent, seeded with `padding = 2N` when `--padding N` is given (N ≤ 32767; when a config already exists, a stderr note names the value to set); warn on stderr if not on PATH. `--absolute` writes `current_exe()` (a symlinked launcher resolves to its target). |
| `garnish doctor` | diagnostics; the glyph test is a grid with one row per icon set and module (plus `config` rows for the icons the loaded config resolves to, overrides included): every single-character icon is padded to two cells and followed by `\|` and the cell count garnish uses, so a glyph the terminal draws wider or narrower pushes its `\|` out of the column; multi-character icons (spinner frames, the effort scale, ASCII words) are left out. It also lists Claude Code's settings chain for the current directory (managed, local, project, user: whether each file is there and parses) and the keys that change what the line can show, each resolved as Claude Code resolves it (the first file that sets a key wins) with the file named: `statusLine.command`, `statusLine.refreshInterval` (suggesting `1` when the config shows a clock, an elapsed time, a countdown or an animation), `statusLine.hideVimModeIndicator` (suggesting `true` when the `vim` module is on, so the mode is not shown twice), `disableAllHooks` (which stops the status line command), `prefersReducedMotion` (with how the config's `animate` interacts), `sandbox.enabled` and `voice.enabled` (which the `sandbox` and `voice` modules show, § 3.8) and `tui` (which renderer the settings ask for and what it does with a tall status line, § 2.1; a value that is neither name is named as one Claude Code drops from the managed file or rejects any other file for, and the next file that sets the key is shown) (PLAN Phase 19; from FUTURE-SPEC § 13.4, N5) |
| `garnish setup [--preset P] [--install]` | the interactive setup (§ 14): a full-screen picker and builder with a live preview at the real box width; `--preset` never opens the screen and writes that preset with the § 5 backup (as `config init --preset P --force` then does) plus `install` when `--install` is given, for scripts and the skill; without `--preset` and without a terminal on stdout it exits 1 with one line |
| `garnish config init [--preset P] [--force] \| check \| path \| show` | config management; `init` refuses to overwrite without `--force` and accepts gallery preset names (§ 12) as well as the four built-ins; `--force` keeps the previous file under `install`'s backup rule and refuses one that does not parse (§ 5); `check` lists problems and exits 1 quietly; `show` prints the fully resolved config, the animation switch as the file or the current directory's settings decide it (§ 4.2) |
| `garnish skills install [--dir D] \| list` | copy the bundled skills (§ 13) into `~/.claude/skills/` (or `D`); `install` runs this too unless `--no-skills` |
| `garnish preview <file\|dir> [--preset P] [--icons S] [--theme T] [--color M] [--width N]` | render one fixture or every `*.json` in a directory, each under a dim `── <name>` heading; the rows are drawn faint, as Claude Code draws every status line row (§ 2.1), so the preview shows the intensity the screen will have (`--color never` is plain); a preview is not a tick, so it never reads the cache or spawns a worker (§ 14) |
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
| warm tick, full preset (the default rows, every option) | < 3 ms | < 8 ms |
| warm tick, one row of every module id (settings badges, `account`) | < 3 ms | < 8 ms |
| warm tick, default preset, `TZ` naming a zone | < 3 ms | < 8 ms |
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
  one-module-per-line (25 rows with `hide_empty_lines = false`) and
  all-on-one-line (one row, cut with `…`) → no panic, correct line count,
  width ≤ `COLUMNS − 4` (§ 2.1); golden files under `tests/golden/`
  (`UPDATE_GOLDEN=1` regenerates). The goldens render with `--color
  never` except where a config fixture's `# color:` header says
  otherwise: `colour-on` pins the painter's escape sequences (with the
  faint `preview` folds into every segment, § 2.1) and the OSC 8 link
  (Phase 20's link goldens and Phase 22's snapshots use the same mode),
  and the row-start guards of both suites look past escape sequences. A
  `# env:` value may name the repository root as `$ROOT`, which is how
  `reduced-motion` points `HOME` at a settings fixture. Every test that
  runs the binary sets `GARNISH_MANAGED_SETTINGS` to nothing, so a
  managed settings file on the machine running `cargo test` never reaches
  a golden; one CLI test points the hook at a fixture instead.
- **Docs sync**: `garnish docs` output must equal committed `docs/`, and
  `config init` output must equal `examples/garnish.toml`.
- **Pinned renders spawn nothing** (PLAN Phase 23): `Clock::fixed()`
  turns workers off as it turns git and the settings chain off, so a
  cached module outside the repo group (`account`, § 3.8) renders its
  empty state in the docs and the in-process matrices without a cache
  directory (`tests/docs_sync.rs` asserts none appears), and the
  settings badges render on from keys the clock seeds in-process rather
  than from any file.
- **Module matrix from the schema** (PLAN Phase 20; from FUTURE-SPEC § 15
  item 11): an in-crate rayon test generated from `ModuleSchema` renders
  every module × every preset × every icon set × `max_width ∈ {0, 1, 4,
  12}`, plus every switch a schema declares (both values of a `Bool`,
  every variant of an `Enum`, at one preset and icon set), alone on an
  unframed line, against every payload fixture, and asserts the shared
  invariants (never wider than `max_width`, a cut ending in the ellipsis
  and an uncut module byte-identical to its uncapped render, a hidden
  state dropping the line rather than leaving a blank row while
  `hide_when_empty = false` always shows the placeholder, no escape or
  control byte in `Segment::text`, no link the painter would refuse, OSC 8
  wrappers balanced in the painted output), so a new module or option gets
  the shared behaviour checked without a hand-written test. The switches
  are what make that true of an option and not only of a module (added
  2026-09-16: without them every module-specific key sat at its schema
  default, Phase 20's own five included).
- **Layout matrix** (PLAN Phase 21): the column shares add up to the line
  width and differ by at most one cell at every width from 10 to 400;
  every line of a multi-line row is exactly the box width with stacks of
  unequal height; the resolved tree of every fixture and preset renders
  byte-identically before and after the model (a plain line is one
  column) and under either name (`[[line]]`, `[[row]]`); and the
  lines-per-row output tiles each line exactly (the placement map of
  § 14 reads it). `tests/presets.rs` renders every preset without `…` at
  its declared width at three instants (a preset that promises motion
  must differ between two of them, and a line ticker must slide exactly
  `ticker_step` cells, so a scrolled row carries nothing that counts
  seconds).
- **Setup snapshots** (PLAN Phase 22): the `setup` screens are rendered
  into ratatui's `TestBackend` (80 × 24, 100 × 30 and 140 × 40) and
  compared with goldens under `tests/golden/setup/` (`UPDATE_GOLDEN=1`
  regenerates; the test lists every golden it writes, so a renamed screen
  cannot leave a stale file): the home menu, the picker, the builder, the
  module form, the module picker, the top-level form, the glyph picker,
  a confirm dialog, the install screen and the help page have goldens,
  and the other forms and pickers are asserted by content; key and mouse
  sequences are driven through the same input path the terminal feeds,
  so all of it is tested without a tty. The test app
  pins the clock in-process (`Clock::fixed()`), aims the install plan at a
  temporary home and shows paths under it as `~/…`, so no environment is
  needed; `tests/cli.rs` covers `setup --preset [--install]` and the
  stdin pointer end to end.

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
| `GARNISH_MANAGED_SETTINGS` | the managed settings file read first in Claude Code's chain (§ 2.3, § 4.2, `doctor`) instead of the platform's (`/etc/claude-code/managed-settings.json`; on macOS `/Library/Application Support/ClaudeCode/managed-settings.json`); empty means no managed file, which is what every test that runs the binary sets |
| `GARNISH_STDIN_TTY` | `1` or `0` overrides the "is stdin a terminal" check of the bare `garnish` (§ 7, § 14), so the pointer path is testable without a pty |

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
- `docs/config.md` has generated sections for `[[row.col]]`, `title` and
  `[box.<name>]` with samples (Phase 21), and each module page lists the
  glyph picker's alternatives as *also try* (Phase 22).

## 11. Assumptions

- Window size comes from `context_window_size`; 1M is assumed only when the
  field is absent or zero.
- The 13k compaction buffer mirrors Claude Code 2.1.260 internals and may
  drift; it is configurable and the marker can be disabled.
- Cache TTL display uses `prompt_cache` only; when absent the module shows `–`.
- Session duration is `cost.total_duration_ms` and resets on `/clear`.
- No GitHub network access; PR presence/state is whatever the harness reports.
- Four default lines cost four terminal rows; `compact`/`minimal` exist for
  small terminals. A multi-line row (§ 4.3) costs its height. The harness
  caps nothing in its classic renderer and gives the prompt box and the
  status line together at most half the terminal in fullscreen (§ 2.1),
  the budget the `setup` picker warns against.

## 12. Presets gallery (PLAN Phase 17, shipped in v0.2.0)

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
- **The set.** The configs of the 2026-09-05 walkthrough were the first
  entries; four came with the layout model, and nine added on 2026-09-19
  show the rest of the vocabulary (titles at every position, links, the
  compaction scale, cell and share widths, a boxed column, a narrow and
  an ASCII-only terminal, half-speed animation, a two-cell ticker,
  animation off), so every layout key and most module options appear in
  at least one preset; four more with PLAN Phase 23 show pace and eta,
  the number formats, hide lists and the badges of § 3.8: 32 in all, and
  the `setup` picker (§ 14) is how they are browsed.

## 13. Skills (PLAN Phase 18, shipped in v0.2.0)

Three Claude Code skills ship with garnish, live under `skills/<name>/SKILL.md`
in the repository, are embedded in the binary (`include_str!`) so a
`cargo install` has them, and are written to `~/.claude/skills/<name>/` by
`garnish install` (or `garnish skills install`). Each skill is plain
Markdown with frontmatter (`name`, `description`) and instructions; none of
them needs network access from garnish itself, they drive `gh` and the
`garnish` CLI.

- **`garnish-statusline`.** Conversational config builder. The hands-on
  one is `garnish setup` (§ 14); both write the same file, and the skill
  offers `setup` first and keeps the conversational path for a person
  who would rather describe what they want. It asks, with recommended
  defaults: terminal and font (Nerd Font? decides `icons`), usual
  terminal width (decides preset and row count), what matters most,
  rows or columns, titles and boxes, colours, frame, alignment, motion,
  caps and links, the context scale and the reset form; names gallery
  presets that show the answers; drafts into a temp file, shows a
  `garnish preview` of it, and only then copies it over the real one
  behind the same `.bak-<epoch>` backup garnish itself keeps (§ 5),
  validates with `config check`, and explains how to tweak it. It never
  edits `settings.json` beyond what `garnish install` does.
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
  replaces the home directory in every path with `~` (`doctor` already
  collapses it and `config show` prints no path at all, so this catches
  what the person pasted by hand), keeps only `GARNISH_*` lines of the
  doctor's environment section, prints the whole issue body, and asks the
  person explicitly before `gh issue create`. Nothing leaves the machine on
  an unanswered or negative question.

## 14. Interactive setup (PLAN Phase 22, shipped 2026-09-19)

Decided 2026-09-12 with Daniel, from FUTURE-SPEC § 13 (option 7.3c): a
full-screen `garnish setup` in the terminal, the way ccstatusline's TUI
works, with the two things garnish can do that it cannot: an **exact** live
preview (garnish knows the harness box width, § 2.1, and renders through
the same code as the tick) and editors **generated from the module
schemas** (every option's type, default, choices, cap and doc string is
already in `ModuleSchema`, as the docs are). It replaces the website idea
of the earlier § 12: a preset rendered at the person's own width is a
better sample than a screenshot at someone else's.

**Built as designed below, with these differences, each decided while
building (2026-09-19).** The draft is the config *file's* own table
(TOML with its order kept), not a resolved `Config`: a save writes only
the keys the file and the edits carry, in the file's order, so a
hand-written file keeps its unset keys unset and its ordering, and only
its comments live on in the backup alone (`config show` still prints the
resolved form; `setup` never does). The option editors are a list of
key / value / default rows with one-key actions (`Enter` picks or types,
`←`/`→` steps, `d` unsets) rather than checkbox and radio widgets: the
same information in less screen. The glyph suggestions are one table in
`icons.rs` keyed by module and icon key rather than a field on each
`IconSpec`, with the same guard test and the same *also try* list on the
module pages, and the glyph picker prints each candidate's cell count
(`|1`, `|2`) rather than the doctor's two-cell grid. The placement map is
`layout::Line::modules()` over `render::render_tree_at`, which returns
each row's lines as typed pieces. A click selects a module and a second
click, or `Enter`, edits it; a click on a cap or the rule opens the frame
form. A separator, a cap or the rule is reached by a click alone; keys
reach modules, rows and columns, and `2` opens the frame form (the
keyboard twin is in PLAN's backlog). The placement map names the outer
row of a line, so a click on a title or a box edge inside a row of
columns selects that row and names the list as the way to the column or
inner row it may belong to (the same backlog item). `Esc` closes the innermost layer
and, at the base of the builder or the picker, leaves it as `q` does. A
module's editor is generated from `ModuleSchema`; the top-level, frame,
row, column and box forms list their keys by hand, since those are not
schema options, and the unit test walks both. The snapshot tests pin the
clock in-process (`Clock::fixed()`) and need no `GARNISH_NOW` or `TZ`.
The terminal minimum is 60 × 12. A `setup` cargo feature was not added:
the release binary grew from 2.8 MB to 3.4 MB and the end-to-end cold
tick did not move.

**Refined on 2026-09-20, after Daniel's first use and a walk of every
preset's forms.** *Undo*: every key or click that changes the draft
leaves the table it replaced on a history of a hundred; `u` (or
`Ctrl+Z`) puts it back with the list cursor of the time, `U` (or
`Ctrl+R`) redoes, a new edit ends the redo chain, and the status line
names what was undone. The draft is dirty exactly while its table
differs from the file's (as read, or as last saved), so an edit undone
leaves nothing to save and `q` does not ask. *Buttons*: the hint bar at
the foot of every screen is a row of them, a click on a hint pressing
its key, which is how undo has a button. *Editing*: a picker opens on
the value in effect and its `custom…` line starts from it, so a label is
edited rather than retyped, and the input line has a cursor (`←`/`→`,
`Home`/`End`, `Delete`); a string keeps its spaces (a picked `  `
separator had arrived as `""`); the `[colors]` form offers literals only
(the theme's own, each noted with its role, then the named colours),
since a role there has no ground, while a module's `colors.*`, a title
and a box still take roles; a row's form lists only the keys the parser
would take for it (`blank` on a spacer or a row of columns, the title
keys outside a named box) plus any key the file already sets, so `d` can
unset one the parser reports; a value the parser takes but that leaves
another key reported (`fill = false` under a `fill_pattern`) is set and
the status names that key; a `box` unset or changed, from the form or
with `d`, drops a `[box.<name>]` nothing joins any more, as the
builder's `b` does; a module's `label` picker starts with the module's
own name, bare and capitalised; changing the top-level `preset` swaps
the rows for the new preset's when they were still exactly the old
preset's (the builder writes a preset's rows into the file so they can
be edited, which would otherwise pin them) and says which happened.
*Columns grow from the cursor*: `C` inserts a column after the selected
one (a plain row's groups become the first column) and selects the new
one, so `m` fills it; `]` and `[` past the last or first column make a
column for the module, a plain row splitting into columns from the
module that leaves it (a module alone in its column stays, since a new
column would only leave an empty one behind); `m` on a row of columns
lands in the last column, or the last inner row of a stack, and says so.
*Boxes*: `B` boxes the selected row together with the row above, joining
the named box that row is in, or asking for a name (the title either row
carried, as a bare key) and putting both rows in a new `[box.<name>]`
that takes that title; a third `B` joins them, so a run of rows becomes
one titled box a key at a time.

**Two ways in, one file out.** The home screen offers *Pick a preset* and
*Build a custom layout*, plus *Install* and *Quit*; when a config already
exists it opens on that config in the builder, previewed, so `setup` is
also the editor for an existing file. Whatever route, the result is the
ordinary `garnish.toml` of § 4, written the way `config show` writes it
(the round trip already exists), never anything the tick could not read.

- **Preset picker.** The four built-in presets and every gallery preset
  (§ 12) in a list; the highlighted one is rendered live above the list,
  at the real width (`COLUMNS − 4 − padding`), with its summary, declared
  width and `needs` line, and a warning when the terminal is narrower than
  the preset's declared width (the `…` cut is shown as it would be on
  screen, not hidden) or shorter than the fullscreen budget of § 2.1
  allows for the preset's row count (`⌊LINES / 2⌋ − 5` rows whole with an
  empty prompt; the pane states the line count either way). The warnings
  have lines of their own under the facts they qualify, never the end of
  a line that a narrow terminal cuts (found by the Phase 22 review).
  `Enter` applies it: the file is written with the previous one kept by
  `install`'s backup rule (§ 5), and the install screen follows if the
  settings file has no `statusLine` yet. `e` opens the highlighted preset
  in the builder instead of applying it.
- **Builder.** The preview pane stays at the top of every builder screen
  and re-renders on every change. Below it, the `[[row]]` list: each row
  shows its columns as chips (§ 4.3; a plain row is one column) and its
  height in lines; keys add, insert, delete, clone and move rows, add a
  column and set its `width` and `justify`, turn a column into a stack,
  move a module within a column or into the next one (a new one past the
  edge), mark a row as a spacer, give it a title, box a row, box it
  together with the row above (`B`, a run of rows becoming one titled
  box a key at a time) and box a whole column; `u` takes any of it back.
  Adding a module opens a **picker**
  with fuzzy and initialism search over the 25 ids, the config's existing
  `text.<name>` tables and *New text module…* (`sy` finds `sync`, `sn`
  finds `session_name`), each with its one-line summary from
  `garnish modules`; the new-text entry asks for a name checked by the
  § 3.7 rule, creates the table with the schema defaults and opens its
  editor, and removing a text module's last placement asks whether to
  drop the table. `Enter` on a module
  opens its **editor**: one row per schema option (`preset`, `refresh`,
  `hide`, `label`/`prefix`/`suffix`, `hide_when_empty`, `max_width`, then
  the module's own options, then `icons.*` for the active icon set and
  `colors.*`), showing the default, the current value and the doc string;
  enums cycle, booleans toggle, integers edit with their `max` shown,
  colours offer the theme's roles and accept a literal, icons accept any
  string and show the cell count `doctor` would. A text module's editor is
  the same screen over the text schema. Separate screens set the top-level
  keys (`preset`, `icons`, `theme`, `color`, `frame`
  style/fill/separator/`separator_color`, `align`, `durations`, the
  `[format]` styles, `right_justify`, `overflow`, `animate`, `padding`)
  and the `[colors]` role overrides, each with the same row shape. Nothing
  in the builder is hand-coded per option: a unit test walks every
  `OptSpec` kind and every top-level key and asserts an editor exists for
  it, so an option added to a schema appears in `setup` the next build.
- **Selecting in the preview.** The preview is not a picture: every
  module, separator, cap and rule in it can be selected, with the mouse
  or the keyboard, and the selection is highlighted in place (inverse
  video over the module's cells and a marker on its chip in the line
  list). The renderer makes this possible with a **placement map**: an
  optional output of `render_lines_at` that lists, for every row, the cell
  ranges each module, separator, title, box edge and frame element
  occupies, computed from the same segment lists the painter emits (a
  unit test checks the ranges tile each row exactly and match the painted
  widths, columns, stacks and the ticker included). A module may own
  several ranges (one straddling the ticker's wrap-around), a cut module
  owns its `…` cell, and a module that rendered nothing or lies wholly
  outside the ticker window owns no cells and is reached from its chip in
  the row list instead. A click (crossterm mouse capture, on while
  `setup` runs and off when it exits, on `Ctrl+C` and on a panic, through
  a hook chained ahead of color-eyre's so the report prints on a restored
  terminal; the wheel scrolls lists) or `Tab`/`Shift-Tab`/the
  arrows move the selection; `Enter` or a second click on the selected
  item opens its editor as an **overlay panel** over the screen; clicking
  the rule or a cap opens the frame form, a separator the same form on
  its `separator` key. Everything the mouse does has a key, since tmux
  and some SSH sessions swallow mouse events (a separator, a cap and the
  rule are reached through `2`, the frame form, see above).
- **Editing by ticking.** The overlay lists every option of the selected
  module as a form: booleans as checkboxes (`[x] hide_when_empty`),
  `preset` and every enum as a radio list, integers as a stepper showing
  the `max`, colours as a swatch list of the theme's roles plus *custom*
  (a hex or 256 index, validated as `config check` would), and strings
  and icons as the pickers below. Every change re-renders the preview at
  once; `Esc` closes the innermost layer (a picker over a panel over the
  builder) and, with none open, leaves the builder or the preset picker
  as `q` does, and the module's chip shows a dot while it carries
  overrides. The form is generated from `ModuleSchema`, so a new option
  is a new row.
- **Freeform values come with suggestions.** A string option (`label`,
  `prefix`, `suffix`, `text`, `gap`, a line's `separator`, `ticker_gap`,
  the frame's `fill_char` and caps) opens a picker whose first entries are
  the distinct values the frame styles and the gallery presets already
  use (gathered from the frame tables and `gallery::PRESETS` at start-up,
  deduplicated, each shown with its cell count), then *custom…*, which
  opens an input line that is reduced to plain text and width-checked the
  way the config parser does (§ 5). So a separator picker offers ` │ `,
  ` ┃ `, `  `, ` · `, the powerline glyphs, and whatever a preset author
  found, before asking anyone to type one.
- **Glyph picker.** An icon key opens a picker with one row per icon set
  (the nerd, unicode, emoji and ascii glyphs for that key) followed by
  the key's **suggested alternatives**, a short list per key declared in
  the schema (`IconSpec.suggestions`, a few per set: a robot, a brain and
  a sparkle for `model.icon`, three branch shapes for `branch.icon`, …),
  then *custom…*. Every candidate is drawn in the person's own terminal
  the way `doctor`'s glyph grid draws it, padded to two cells and followed
  by `|` and garnish's cell count, so a glyph the font draws wider or
  missing shows at once. Choosing one writes the per-key override
  (`[modules.<id>.icons] <key> = "…"`) that the config already supports,
  so sets mix freely: `icons = "nerd"` as the base with an emoji clock and
  a unicode branch is three lines of TOML, and `config show` round-trips
  it. The suggestions pass the same unit test as the sets (one or two
  cells by every table, no East Asian Ambiguous character, no variation
  selector), and the generated module pages list them under the icons
  table as *also try*.
- **Preview.** Rendered in-process through `render_lines_at` with a live
  clock (animations move; `GARNISH_ANIMATE=0` freezes them as everywhere)
  that, like `garnish preview`, never reads the cache or spawns a worker
  (a preview is not a tick: a cached module shows its not-yet-refreshed
  state) and, unlike it, does no git discovery and reads no settings,
  since the bundled fixtures name no real directory. `f` cycles every bundled fixture
  (`fixtures::FIXTURES`: subscription, API key, before the first
  response, no git, the PR states, 1M at 96 % and the rest of
  `tests/fixtures/payloads/`) so the person sees what an absent field does
  to their layout; `w` sets a terminal width other than the real one (the
  box is then `w − 4 − padding`, and a `padding` edit re-shrinks the box
  at once). ratatui does not interpret escape bytes, so the pane is drawn
  by `setup::paint`, a second painter target over
  `Painter::painted_style` that turns the same segments into ratatui spans
  (no new crate); a unit test paints the rows both ways and checks the
  cell text and the styles agree, which is the "what you see is what the
  status line prints" guarantee. The pane dims every row as the harness
  does (§ 2.1: the `DIM` modifier on every span, the twin of
  `Painter.dim`), so it also shows the intensity the screen will have.
- **Saving.** Edits live in memory as the file's own table (see the
  differences above), and `s` writes it back (with the § 5 backup), so a
  hand-written file's ordering survives a save and its comments do not;
  the status bar says so with the first save that keeps a backup, and the
  backup keeps the original. Because the tick re-reads the config every second, a saved
  change shows in a running Claude Code within a second, so there is no
  apply step. `q` on an unsaved draft asks once. A file that does not
  parse is never overwritten (§ 5): `setup` opens on the built-in defaults,
  says so in the status bar, and `s` refuses until the file is moved.
- **Install.** The install screen mirrors `install --dry-run`: it lists
  the settings path, the exact `statusLine` object it will merge, the
  backup rule, whether the skills will be written and the PATH warning if
  any, and asks once. It runs the same code as `garnish install`; nothing
  in `setup` writes to `settings.json` by another route.
- **Non-interactive twin.** `garnish setup --preset <name> [--install]`
  never opens the screen: it writes that preset and, with `--install`,
  hooks it up, for scripts and for the `garnish-statusline` skill (§ 13),
  which keeps its conversational path and names `setup` as the hands-on
  one; without `--preset`, `setup` needs a terminal on stdout and exits 1
  with one line otherwise. The bare `garnish` typed at a terminal (stdin
  is a tty, so no payload is coming) prints one line pointing at
  `garnish setup` (two lines) and exits 0 instead of waiting for JSON;
  the explicit `garnish render` always reads stdin, and the harness
  always pipes, so rendering is unchanged (§ 7).
- **Traps, decided.** `setup` honours the global `--config` flag and
  `GARNISH_CONFIG` like every command, so it edits the file the tick
  reads; without a home directory and without either it refuses with the
  § 5 one-liner. The preview fixtures are embedded in the binary
  (`include_str!` of the named files under `tests/fixtures/payloads/`, as
  the presets and skills are), so `setup` works from a `cargo install`
  with no repository at hand. The preview honours the config's `color`
  and `NO_COLOR` for the rendered rows while the screen's own chrome
  uses the terminal's default colours, so a `color = "never"` config
  previews plain; in that case the preview's header says "colours off:
  edits are saved, not previewed". A terminal smaller than 60 × 12 gets
  one line asking for more room instead of a broken layout, and a resize
  redraws everything at the new width (the preview's box width follows
  it). A config that parses with problems opens on the per-key fallbacks
  (§ 5) with the first problem in the status bar and a count of the
  rest; saving writes the file's keys as they are, the bad values
  included (the tick keeps reporting them until they are fixed, and `d`
  in a form unsets one), and the status bar says so on opening. If the
  file on disk changes while `setup` is open (another
  session, the skill, an editor), `s` notices (a best-effort compare of
  mtime and length; a file absent at open and present at save counts as
  changed) and asks whether to overwrite or reload; it never merges. A
  save or an install that fails (a read-only directory, an unwritable
  `settings.json`; a symlinked settings file is written through the link
  as `install` does) shows the OS error (a failed save in the status bar,
  a failed install in its own screen), keeps the draft and never exits. The picker and the builder never run a module's
  worker or git: repo modules render from the fixture's fields, as in
  `preview`.
- **Cost and shape.** The TUI lives in its own module tree (`src/setup/`)
  and is never entered on the render path, so the tick budget (§ 8) does
  not move; `bench/run.sh` is the check, and a cargo feature (`setup`, on
  by default) is the fallback if binary size ever shows in the cold
  start. Crates: `ratatui` with the `crossterm` backend, the one new
  dependency pair (`inquire`, a prompt wizard, was the alternative and has
  no live pane; a WebAssembly page was the other and is the website again
  by another name). `setup` reads the schemas, the presets, the bundled
  fixtures and the config; it runs no command and makes no network call.
  The screens have snapshot tests (§ 9).
