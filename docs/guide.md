# garnish guide

garnish is the `statusLine` command for Claude Code: every second Claude Code
pipes a JSON snapshot of the session to it, and garnish prints a few framed
lines built from small, independent modules. This guide gets you from install
to a status line you like. The [configuration reference](config.md) lists
every key, and each module has a page under [modules/](modules/). (Those
pages are generated from the code; this guide is the one hand-written page
here.)

## 1. Install

With [Homebrew](https://brew.sh) (macOS and Linux, prebuilt binary, from
the first tagged release; until then, build from source):

```sh
brew install --cask justanotherspy/tap/garnish
garnish --version
```

From source:

```sh
git clone https://github.com/justanotherspy/garnish.git && cd garnish
make install            # cargo install --path . --locked  →  ~/.cargo/bin/garnish
garnish --version
```

Requirements: Linux or macOS; for a source build, [rustup](https://rustup.rs)
(the repo pins a nightly toolchain, which rustup installs on first build); Claude Code
2.1.251 or newer; a terminal with ANSI colors; a
[Nerd Font](https://www.nerdfonts.com) for the default `nerd` icon set (or
set `icons = "unicode"` / `"emoji"` / `"ascii"`); OSC 8 hyperlink support
(iTerm2, Kitty, WezTerm, Ghostty…) for clickable pull-request numbers,
branch names (`branch.link = true`) and text boxes (`url`).

## 2. Set it up

```sh
garnish setup           # pick a preset or build a layout, previewed live, then hook it in
```

`setup` takes over the terminal while it runs. *Pick a preset* lists
the four built-ins and the gallery, each rendered at your terminal's real
width as you move through them (with a warning when the terminal is
narrower than a preset wants, or shorter than Claude Code's fullscreen
renderer shows whole); `Enter` writes it, and the install step follows
when Claude Code has no `statusLine` yet. *Build a custom layout* opens
the builder: the preview at the top, your rows below it, and single keys
to add a module (`m`, with search), a row (`a`), a column (`C`, which
leaves the cursor on the new column; `]` on a module past the last
column makes one for it), a stack (`S`), a title (`t`) or a box (`b`;
`B` boxes the row together with the row above, `e` edits the box the
selected line is in), to edit the selected
module or row (`Enter`; a click in the preview selects, a second click
edits), and to open the top-level keys (`1`), the frame (`2`) and the
colours (`3`). `u` undoes the last edit and `U` redoes it, `f` cycles
the sample payloads, `w` previews at another width, `s` saves (a backup
of the previous file is kept), `I` installs, `?` lists every key, and
the key hints along the bottom are clickable. When a config already
exists, `setup` opens straight into the builder on it.

Without the screen:

```sh
garnish setup --preset compact --install   # write a preset and hook it up
garnish install                             # the settings alone (backup kept; --dry-run to look first)
```

or by hand:

```json
{ "statusLine": { "type": "command", "command": "garnish", "refreshInterval": 1 } }
```

`refreshInterval: 1` makes the clock tick and the countdowns move; garnish
keeps that cheap by rendering payload data directly and everything slow
(git, worktrees) from a cache that a detached worker refreshes in the
background. A warm tick never waits on git and never runs a process; when a
cached value has expired the tick spawns one detached worker and moves on.
garnish makes no network calls of its own; only `[modules.sync]
fetch_interval` opts into a background `git fetch`. `garnish install` rewrites
`settings.json` in one read-modify-write with no lock, so run it while no
other tool is editing that file, and it refuses a `settings.json` that is
not valid JSON rather than rewrite it. `garnish doctor` lists the settings
files Claude Code reads for the current directory, which one sets
`statusLine`, and suggests `refreshInterval = 1` or
`hideVimModeIndicator = true` when your config calls for them.

## 3. Try it before you commit

```sh
garnish preview tests/fixtures/payloads/subscription-full.json
garnish preview tests/fixtures/payloads --preset compact --icons unicode --theme nord
COLUMNS=80 garnish preview tests/fixtures/payloads/api-key.json --width 80
```

`preview` renders a saved payload with any preset, icon set, theme and width,
so you can see a change without waiting for a real session. The rows come
out faint on purpose: Claude Code draws every status line row dim, so the
preview shows the intensity the screen will have.

## 4. Write a config

```sh
garnish config init     # ~/.config/garnish/garnish.toml, fully annotated
garnish config check    # every problem, with its TOML path
garnish config show     # the fully resolved result (what a tick actually uses)
garnish setup           # the same file, edited in place with a live preview
```

Start from a preset and override what you care about:

```toml
preset = "compact"          # default | minimal | full | compact
icons  = "nerd"
theme  = "catppuccin-mocha"

[frame]
style = "rounded"           # none | rounded | square | double | heavy | powerline | custom

[modules.context]
preset = "full"
width  = 30
```

A bad key never blanks the status line: every valid key stays in effect, the
built-in default stands in for the bad one, and a dim `⚠ config: <file>
<path>: <message>` line is appended. Only a file that does not parse as TOML
falls back to the defaults wholesale, with the line of the syntax error;
such a file is never overwritten either (`config init --force` refuses it
and keeps a backup of any file it does replace).

## 5. Compose your own rows

Every module is independent, so rows are just lists of module ids. `modules`
are left-aligned, `right` are right-aligned, and the frame rule fills the gap.

```toml
[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name", "clock"]

[[row]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "limit7d", "cost"]
```

| group | modules |
|---|---|
| repo | `path` `branch` `sync` `worktree` `pr` |
| model | `model` `effort` `context` `style` |
| usage | `limit5h` `limit7d` `spend` `cost` |
| session | `session` `api` `cache` `clock` |
| identity | `session_name` `vim` `agent` `lines` |
| yours | `text.<name>`: a fixed string in a box, any number of them |

A text module is the one thing you define yourself: plain text (escape
sequences are stripped) in a box of fixed width, so it doubles as a
fixed-width slot next to aligned columns. Longer text scrolls or is cut:

```toml
[[row]]
modules = ["path", "text.motd"]
right   = ["text.tag", "clock"]

[modules.text.motd]
text     = "ship it before lunch, then write the docs"
width    = 12             # cells; 0 = the text's own width
overflow = "scroll-wrap"  # clip | scroll | scroll-wrap
gap      = " · "

[modules.text.tag]
text  = "v0.2"
color = "muted"
```

Every module also takes `label` (dim text before the value), `prefix` and
`suffix` (text around it), `hide_when_empty` and `max_width`, which cuts
the whole module to that many cells with `…` before the columns are
aligned, so one long branch name or session title cannot push the rest of
the line off (`[modules.branch] max_width = 24`; text modules size their
box with `width` instead). A few modules have a presentation key of their
own: `path.style = "fish"` abbreviates the directories above the last one
(`~/p/garnish`), `branch.link = true` and a text module's `url` make them
clickable, `context.scale = "usable"` makes the bar say how close
auto-compaction is, and `reset = "absolute"` (or `"both"`) on the limit
modules prints the time a window resets at instead of, or after, the
countdown. On `limit5h` and `limit7d`, `reset = "elapsed"` prints how much
of the window has passed (`2h46m/5h`), `pace = true` how far the usage runs
ahead of (`⇡14%`) or behind (`⇣32%`) that elapsed share, `eta = true` when
100 % lands if it lands before the reset, `pace_colors = true` colours the
percentage by that band instead of the thresholds, and `elapsed_marker =
true` drops a cursor on the mini bar. Each module page under
[modules/](modules/) lists its keys.

Numbers print the way `[format]` says: `tokens = "precise"` writes
`128,400` where `compact` writes `128k`, `percent = "precise"` keeps a
decimal (`42.3%`), `cost = "whole"` rounds to dollars, and `parens = "dim"`
mutes every parenthesised detail (the api share, the net lines, a `both`
reset). A module that prints a number takes the same key
(`[modules.context] tokens = "whole"`) to pin its own style; `inherit`,
the default, follows the table.

Modules that have nothing to show are skipped: `limit5h` only appears on a
subscription, `cost` only with an API key, `pr` only while a pull request is
open, `vim` only with vim mode on. A module can leave the row in more
states than that: `hide = ["zero"]` on `lines`, `sync` or `cost` skips it
while the count or the amount is nil, `hide = ["below:10"]` (or `above:N`)
on a module that prints a percentage skips it under (or over) that value,
and `hide = ["empty"]` is the same as `hide_when_empty = true`, which stays
as the older spelling; the two combine. Each module page lists the states
its measure allows. A row whose modules all have nothing to
show is dropped too (outside a repository, a row of `branch sync pr` would
otherwise be an empty framed row); set `hide_empty_rows = false` to keep
such rows, or write `modules = []` for a spacer row that always stays.
Claude Code drops whitespace-only rows from the script's output, so with
`style = "none"` and colour off (`color = "never"`, `NO_COLOR`) a spacer
shows in `preview` only; add `blank = true` to that row to keep it on
screen whatever the colour setting (the row then carries one invisible
cell). Claude Code trims every row as well, so a row that starts with
spaces (a column's padding line, a module placed right or centre under
`style = "none"`) would slide left; garnish keeps those cells with an
invisible lead: an empty colour code with colour on, the same braille
blank with it off.
With `stale_style = "hide"`, a row made only of cached modules can vanish
while its values are overdue; `hide_when_empty = false` on one of them pins
the row.

With several lines, `align = true` (top level) pads every module column to
the widest module in it so the `│` separators stack vertically, and
`durations = "fixed"` prints timers as `9m00s` / `1h05m` so their width does
not change as they tick. Both are shown in
[config.md § Aligned columns](config.md#aligned-columns). Columns pair
*positionally*: the third module of every line is padded to the same width
whatever it shows, so a `–` placeholder sitting under a 24-cell context bar
gets a 24-cell blank column. Alignment ignores separators, so the remedy is
to put modules of similar width in the same column, or to move the odd
module to a line of its own. On the right side the pad goes before the text
by default so it hugs the cap; `right_justify = "start"` puts it after, so
the text stays next to the separator.

### Columns, titles and boxes

A row is the addressable unit of the config and can be more than one
terminal line tall. Everything above is the base case: a row with `modules`
and `right` is one column filling the width. Add `[[row.col]]` tables and
the row becomes columns side by side, sharing the width by `width` —
`"1fr"` (a share of what is left), `"auto"` (the column's own content) or a
number of cells — with `gap` empty cells between them:

```toml
[[row]]
gap = 2
[[row.col]]
modules = ["path", "branch"]
[[row.col]]
modules = ["model", "effort"]
[[row.col]]
modules = ["context"]
```

`justify` says where a column's modules sit when it has no `right` group,
and its default follows the column's position, so those three read left,
centre and right without being told. A column can hold a stack of rows of
its own (`[[row.col.row]]`) instead of modules; the row is then as tall as
its tallest column, and `valign` places a shorter one.

`title` sets plain text into a row's rule (`title_justify` left, centred or
right; a row with only a title is a titled spacer). `[box.<name>]` frames a
run of adjacent rows, or a whole column, with its own corners and sides in
place of the frame's caps — two extra lines, so a box is at least three
tall, and `box = true` boxes one row on its own. Rows join by name inside
a stack as they do at the top level, and a box is one run of adjacent rows
in the whole file: a name that comes back anywhere after it is reported.

```toml
[box.repo]
title = "Repository"
style = "double"      # inherits [frame] style when absent

[[row]]
box = "repo"
modules = ["path", "model"]
right   = ["clock"]
```

The keys are in [config.md § `[[row.col]]`](config.md#row-col) and
§ `[box.<name>]`; `grid-three`, `grid-six`, `boxed-panels` and
`dashboard-panels` in the gallery are working examples to copy from.

## 6. Presets, icons, colors

Each module has three presets: `minimal` (bare value), `default`, and `full`
(everything it knows). Set them per module (`[modules.context] preset =
"full"`) or all at once with the top-level `preset`.

Every glyph a module uses is an `icons` key with a value per icon set, and
every colored part is a `colors` key that defaults to a theme role:

```toml
[modules.branch.icons]
branch = ""            # any string
[modules.branch.colors]
name = "danger"          # a role…
icon = "#ff8800"         # …or a literal color
[colors]
accent = "bright-blue"   # restyle every module that uses the role
```

### Animation

Everything that moves in garnish (the clock spinner, a scrolling text
module, the line ticker, the animated frame parts) is a pure function of the
clock: frame = `floor(now × step) mod period`. Nothing is stored between
ticks, every session on the machine animates in step, and the cadence is
whatever Claude Code ticks at (`refreshInterval`, at least 1 s); a `step`
below 1 slows an animation down (0.5 = every second tick). `animate = false`
in the config, or `GARNISH_ANIMATE=0` in the environment, freezes every
animation at frame 0 and cuts a ticker line with `…` instead of leaving it
frozen mid-scroll; use it for screen readers and recordings. Claude Code's
own *Reduce motion* setting (`prefersReducedMotion` in its settings files)
freezes garnish the same way as long as the config leaves `animate` unset,
so the two stay in step; an explicit `animate` wins over the setting, and
`garnish config show` prints the value in effect.

## 7. Troubleshooting

- **Boxes or missing glyphs** → your font lacks Nerd Font icons; set
  `icons = "unicode"`.
- **Misaligned right edge on some lines** → the terminal draws a glyph wider
  or narrower than garnish counts. The built-in `unicode` and `emoji` sets
  avoid the characters terminals disagree on (East Asian Ambiguous widths,
  the Geometric Shapes block, emoji that need a variation selector), but an
  override under `[modules.<id>.icons]` can bring one back. `garnish doctor`
  ends with a glyph grid: every icon is followed by `|` and the cell count
  garnish uses, in fixed four-cell fields, so the `|` of a glyph your
  terminal draws differently is pushed out of its column. Override that glyph
  and paste the grid into an issue.
- **Hairline gaps between the blocks of a bar** → the font draws `█` a
  shade narrower than a cell. Set `bar = "line"` on `context`, `limit5h`,
  `limit7d` or `spend` for a `━`/`─` bar with whole cells (no fractional
  block either), or pick your own glyphs under `[modules.<id>.icons]`
  (`fill`, `empty`).
- **`⟳` next to a value** → the cached value has not been refreshed for
  `stale_after` TTLs (default 5) and a worker is on it; `✗` means the last
  refresh failed. `garnish doctor` shows the error.
- **`account` shows nothing** → a background worker reads the email from
  `~/.claude.json` (or `$CLAUDE_CONFIG_DIR/.claude.json`), so the first
  tick of a session shows nothing, and an API-key session has no account
  at all; `✗` means the file could not be read or parsed, and `garnish
  doctor` says why.
- **The `sandbox` or `voice` badge never appears** → they show only while
  `sandbox.enabled` or `voice.enabled` is `true` in Claude Code's settings
  files (`/voice` writes the second); `garnish doctor` prints both keys
  with the file each comes from.
- **Nothing changes** → check `garnish config path` and `garnish config check`.
- **The line looks faint** → Claude Code draws every status line row dim
  and folds that into every coloured piece of it; nothing a status line
  command prints can undo it, and `preview` draws its rows the same way so
  that it shows what the screen shows. If the line reads too faint, pick
  brighter roles under `[colors]` or a theme with more contrast.
- **Nothing moves** → `garnish doctor` says whether `refreshInterval` is
  set (Claude Code re-runs the line every second only with `refreshInterval:
  1`) and whether Claude Code's *Reduce motion* setting is freezing the
  animations; `animate = true` in the config overrides the setting.
- **The bottom rows are missing** → in Claude Code's fullscreen renderer
  (the `tui` setting, which `/tui` shows and sets; a new install starts
  with it) the prompt box and the status line share at most half the
  terminal's rows, and a taller status line loses its last rows. Keep the
  line count at most `LINES / 2 − 5`, rounding down (7 rows on a 24-line
  terminal, 20 on 50); that keeps the line whole while the prompt is
  empty, and a long prompt being typed takes rows from the bottom until
  it is sent. The classic renderer cuts nothing and scrolls instead.
  `garnish doctor` prints the `tui` setting in effect.
- **Right edge cut with `…`** → Claude Code's status line box is 4 cells
  narrower than the terminal, plus 2 cells per unit of `statusLine.padding`.
  garnish subtracts the 4 on its own; if `statusLine.padding` is set in
  `settings.json`, set `padding` in the config to twice that value.
- **Too much for one row** → `overflow = "ticker"` scrolls a left group that
  is wider than the box, one cell per tick (`ticker_step`), wrapping around
  after `ticker_gap`; the right group stays put. It moves as often as Claude
  Code ticks (`refreshInterval`, at least 1 s); with animations off the
  line is cut with `…` instead. Timers switch to `durations = "fixed"` on their own while the
  ticker is on, because a `compact` duration that changes width between
  ticks (`1h` → `59m59s`) changes the scroll period and the window jumps
  instead of sliding; `durations = "compact"` at the top level opts back in,
  and `durations = "compact"` under one `[modules.<id>]` pins just that
  module (a right-hand module never scrolls, so it can stay compact).
- **Reproduce a render** → `GARNISH_NOW=1738425600 COLUMNS=100 garnish < payload.json`
  (the lines come out 96 cells wide: what fits in Claude Code's box at that
  terminal width).
- **A garbled terminal after `garnish setup` was killed** → `setup` puts
  the terminal back when it exits, on `Ctrl+C` and on a crash, but a
  `kill` (or a supervisor's SIGTERM) gives it no chance to, and the shell
  is left without echo, on the alternate screen, with mouse reporting on.
  Type `reset` (even unseen) and Enter.

## 8. Under the hood

stdin JSON → `Payload` → `Config` (TOML + presets) → each `[[row]]` renders
its modules → frame joins left/right groups and fills to the width of Claude
Code's box (`$COLUMNS − 4 − padding`, § 7) → stdout.
Cached modules read one small file each; when it is past its TTL the tick
spawns `garnish refresh` in its own process group to recompute it and keeps
showing the last value, dimmed only once it is `stale_after` TTLs overdue.
Warm tick budget: under 3 ms.

## 9. Skills

garnish ships three Claude Code skills (`skills/<name>/SKILL.md`, embedded in
the binary and written to `~/.claude/skills/<name>/` by `garnish install` or
`garnish skills install [--dir D]`; `garnish skills list` shows them):

- `garnish-statusline` — offers `garnish setup` first, or builds the
  config from a conversation: it asks about your terminal and font, width,
  what matters most, rows or panels, colours, frame, motion and links,
  drafts the config in a temp file, previews and validates it, and writes
  it into place only once you approve. It never edits `settings.json`
  beyond what `garnish install` does.
- `garnish-feedback` — files a GitHub issue on `justanotherspy/garnish` with
  the environment, `garnish config show`, `garnish doctor` (glyph grid
  included) and the plain rendered line, labelled `feedback` (and
  `alignment` for width problems), and asks for a screenshot.
- `garnish-submit-preset` — proposes your config as a gallery preset: name,
  summary, designed width, requirement and author, the file with its gallery
  header, a rendered sample checked to fit, an issue labelled `preset`.

Invoke them from Claude Code by name once installed.
