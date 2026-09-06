# garnish guide

garnish is the `statusLine` command for Claude Code: every second Claude Code
pipes a JSON snapshot of the session to it, and garnish prints a few framed
lines built from small, independent modules. This guide gets you from install
to a status line you like. The [configuration reference](config.md) lists
every key, and each module has a page under [modules/](modules/). (Those
pages are generated from the code; this guide is the one hand-written page
here.)

## 1. Install

```sh
git clone https://github.com/justanotherspy/garnish.git && cd garnish
make install            # cargo install --path . --locked  →  ~/.cargo/bin/garnish
garnish --version
```

Requirements: Linux or macOS; [rustup](https://rustup.rs) (the repo pins a
nightly toolchain, which rustup installs on first build); Claude Code
2.1.251 or newer; a terminal with ANSI colors; a
[Nerd Font](https://www.nerdfonts.com) for the default `nerd` icon set (or
set `icons = "unicode"` / `"emoji"` / `"ascii"`); OSC 8 hyperlink support
(iTerm2, Kitty, WezTerm, Ghostty…) for clickable pull-request numbers.

## 2. Hook it into Claude Code

```sh
garnish install         # merges the statusLine block into ~/.claude/settings.json (backup kept)
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
other tool is editing that file.

## 3. Try it before you commit

```sh
garnish preview tests/fixtures/payloads/subscription-full.json
garnish preview tests/fixtures/payloads --preset compact --icons unicode --theme nord
COLUMNS=80 garnish preview tests/fixtures/payloads/api-key.json --width 80
```

`preview` renders a saved payload with any preset, icon set, theme and width,
so you can see a change without waiting for a real session.

## 4. Write a config

```sh
garnish config init     # ~/.config/garnish/garnish.toml, fully annotated
garnish config check    # every problem, with its TOML path
garnish config show     # the fully resolved result (what a tick actually uses)
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
falls back to the defaults wholesale, with the line of the syntax error.

## 5. Compose your own lines

Every module is independent, so lines are just lists of module ids. `modules`
are left-aligned, `right` are right-aligned, and the frame rule fills the gap.

```toml
[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name", "clock"]

[[line]]
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
[[line]]
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

Modules that have nothing to show are skipped: `limit5h` only appears on a
subscription, `cost` only with an API key, `pr` only while a pull request is
open, `vim` only with vim mode on. A line whose modules all have nothing to
show is dropped too (outside a repository, a line of `branch sync pr` would
otherwise be an empty framed row); set `hide_empty_lines = false` to keep
such rows, or write `modules = []` for a spacer row that always stays. A
spacer needs a visible frame: Claude Code drops whitespace-only rows from
the script's output, so with `style = "none"` it shows in `preview` only.
With `stale_style = "hide"`, a line made only of cached modules can vanish
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
animation at frame 0; use it for screen readers and recordings.

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
- **Nothing changes** → check `garnish config path` and `garnish config check`.
- **Right edge cut with `…`** → Claude Code's status line box is 4 cells
  narrower than the terminal, plus 2 cells per unit of `statusLine.padding`.
  garnish subtracts the 4 on its own; if `statusLine.padding` is set in
  `settings.json`, set `padding` in the config to twice that value.
- **Too much for one row** → `overflow = "ticker"` scrolls a left group that
  is wider than the box, one cell per tick (`ticker_step`), wrapping around
  after `ticker_gap`; the right group stays put. It moves as often as Claude
  Code ticks (`refreshInterval`, at least 1 s), and `GARNISH_ANIMATE=0`
  freezes it.
- **Reproduce a render** → `GARNISH_NOW=1738425600 COLUMNS=100 garnish < payload.json`
  (the lines come out 96 cells wide: what fits in Claude Code's box at that
  terminal width).

## 8. Under the hood

stdin JSON → `Payload` → `Config` (TOML + presets) → each `[[line]]` renders
its modules → frame joins left/right groups and fills to the width of Claude
Code's box (`$COLUMNS − 4 − padding`, § 7) → stdout.
Cached modules read one small file each; when it is past its TTL the tick
spawns `garnish refresh` in its own process group to recompute it and keeps
showing the last value, dimmed only once it is `stale_after` TTLs overdue.
Warm tick budget: under 3 ms.
