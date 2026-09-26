# Configuration reference

garnish reads `--config`, else `$GARNISH_CONFIG`, else `$XDG_CONFIG_HOME/garnish/garnish.toml` (`~/.config/garnish/garnish.toml`), else `~/.garnish.toml`. Without a file the built-in `default` preset is used. `garnish config init`, `garnish setup` and `garnish install` write the file found this way, and the XDG one only when there is none. When the `statusLine.command` Claude Code runs passes its own `--config` (and neither `--config` nor `$GARNISH_CONFIG` names another), that file is the one every command but the status line itself uses: `config path`, `config check`, `config show`, `config init`, `preview`, `doctor`, `setup` and `install`. `garnish config init` writes an annotated file; `garnish config check` validates it; `garnish config show` prints the fully resolved result.

A bad key never blanks the status line: every valid key stays in effect, the built-in default stands in for the bad one, and a dim `⚠ config: <file> <path>: <message>` line is appended; only a file that does not parse as TOML falls back to the defaults wholesale, with the line of the syntax error.

## Top-level keys

| key | values | default | meaning |
|---|---|---|---|
| `preset` | `default` \| `minimal` \| `full` \| `compact` | `default` | Which rows exist and which module preset they imply, when `[[row]]` is absent. |
| `icons` | `nerd` \| `unicode` \| `emoji` \| `ascii` | `nerd` | Glyph set. `nerd` needs a Nerd Font. |
| `theme` | `garnish` \| `catppuccin-mocha` \| `nord` \| `dracula` \| `tokyonight` \| `mono` | `garnish` | Color palette (see below). |
| `color` | `auto` \| `always` \| `never` \| `256` \| `truecolor` | `auto` | Escape-code output. `auto` is truecolor unless `NO_COLOR` is set and not empty. |
| `truncate` | bool | `true` | Truncate the left group when a line overflows the width (`$COLUMNS − 4 − padding`); the right group is cut only when it alone is wider than its column. |
| `stale_style` | `dim` \| `hide` \| `plain` | `dim` | How overdue cached values are shown. |
| `stale_after` | integer ≥ 1 | `5` | TTL periods a cached value may be overdue before it is styled stale; until then the last value shows unchanged while a worker refreshes it. |
| `padding` | integer | `0` | Extra cells subtracted from the width, on top of the 4 Claude Code's box always takes; set `2 × statusLine.padding` when that setting is non-zero. |
| `align` | bool | `false` | Pad each module column to the widest module in it across lines, so the separators stack vertically (see [Aligned columns](#aligned-columns)). |
| `right_justify` | `end` \| `start` | `end` | Where a padded right-group module's text sits: `end` pads on the left so the text hugs the cap, `start` pads on the right so the text follows the separator. Only matters with `align = true` and a filled rule. |
| `hide_empty_rows` | bool | `true` | Drop a row whose modules all rendered nothing or were hidden by `hide_when_empty` or a `hide` list (outside a repository, a row of `branch sync pr` is empty); the frame's caps follow the surviving rows. A row configured as `modules = []` with no `right` is an intentional spacer and is always kept. With `stale_style = "hide"` a row of only cached modules can disappear while its values are overdue and return after the refresh; `hide_when_empty = false` on one module pins the row. `hide_empty_lines` is the permanent alias of this key. |
| `overflow` | `truncate` \| `ticker` | `truncate` | A left group wider than its budget is cut with `…` (`truncate`) or scrolled (`ticker`): a window onto the group advances `ticker_step` cells per tick and wraps around with `ticker_gap` between the end and the start. The offset comes from the tick's clock, so it needs no state and `GARNISH_NOW` freezes it; it moves as often as Claude Code ticks (`refreshInterval`, at least 1 s). The right group is never scrolled, and is cut only when it alone is wider than its column. With animations off the line is cut with `…` like `truncate`. |
| `ticker_step` | number | `1` | Cells the ticker advances per tick (0.001–1000; `0.5` = every second tick). |
| `ticker_gap` | string | `"   "` | Text between the end of a scrolled group and its wrapped-around start. |
| `animate` | bool | `true` | Master switch for every animation (the clock spinner, scrolling text modules, the ticker, and the animated frame parts of § 4.2): `false` freezes them all at frame 0 and cuts a ticker line with `…`. Unset, garnish follows Claude Code's `prefersReducedMotion` setting (the settings chain of the project directory and the home, the first file that sets it winning), so the two stay in step; an explicit value wins over the setting, and `GARNISH_ANIMATE=0` freezes one session whatever either says. `config show` prints the value in effect. Recommended off for screen readers and recordings. |
| `durations` | `compact` \| `fixed` | `compact` (`fixed` with a ticker) | How elapsed times and countdowns print: `compact` drops a zero second unit (`8m20s`, `9m`, `2h`); `fixed` always shows two units with the small one two digits wide (`8m20s`, `9m00s`, `2h00m`), so timers keep their width. Defaults to `fixed` when `overflow = "ticker"`, because a timer changing width inside the scrolled group makes the window jump; set it to `compact` to opt back in. Every module that prints a timer (`session`, `api`, `cache`, `limit5h`, `limit7d`, `spend`, `sync`) has its own `durations` (`inherit` \| `compact` \| `fixed`) to pin one module. |

## `[format]` — number styles

One style per kind of number, each defaulting to what garnish has always printed. Every module that prints a kind carries the same key with `inherit` as its default, to pin one module while the rest follow the table, the way `durations` works; a style on a module that prints no such number is an unknown key.

| key | values | default | meaning |
|---|---|---|---|
| `tokens` | `compact` \| `precise` \| `whole` | `compact` | Token counts: `128k` and `1.0M`; `128,400`; `128400`. Printed by `context`, `cache`. |
| `percent` | `whole` \| `precise` | `whole` | Percentages: `42%`; `42.3%`. Bands and thresholds compare the number printed, whichever style. Printed by `context`, `limit5h`, `limit7d`, `spend`, `api`, `cache`. |
| `cost` | `precise` \| `whole` | `precise` | Money: `$1.23` (`cost.decimals` places, `$1.2k` from a thousand up); `$1`. Printed by `cost`. |
| `parens` | `plain` \| `dim` | `plain` | The parenthesised details (`api`'s share of the session, `lines`' net, the `both` reset form's time): in the colour of the value they follow, or in the muted role the way a `label` is drawn (Claude Code already dims every row, so the muted colour is what "dim" visibly means). |

## `[colors]` — theme roles

Every module color defaults to a role; override a role here to restyle every module at once.

| role | garnish default | used for |
|---|---|---|
| `accent` | `#7dd3a0` | primary highlight: icons and names |
| `accent2` | `#89b4fa` | secondary highlight |
| `muted` | `#6c7086` | de-emphasised text, separators, stale values |
| `text` | `#cdd6f4` | ordinary text |
| `ok` | `#a6e3a1` | good / low usage |
| `warn` | `#f9e2af` | caution / medium usage |
| `hot` | `#fab387` | high usage |
| `danger` | `#f38ba8` | critical, errors, exceeded limits |
| `frame` | `#585b70` | frame lines and rules |
| `band1` | `#a6e3a1` | bar band 1 (lowest) |
| `band2` | `#f9e2af` | bar band 2 |
| `band3` | `#fab387` | bar band 3 |
| `band4` | `#f38ba8` | bar band 4 (highest) |

### Themes

| theme | description |
|---|---|
| `garnish` | The house palette: fresh greens with warm accents. |
| `catppuccin-mocha` | Catppuccin Mocha. |
| `nord` | Nord. |
| `dracula` | Dracula. |
| `tokyonight` | Tokyo Night. |
| `mono` | No color at all; relies on dim and bold only. |

## `[frame]`

| key | default | meaning |
|---|---|---|
| `style` | `rounded` (`none` for the `minimal` preset) | `none` \| `rounded` \| `square` \| `double` \| `heavy` \| `powerline` \| `custom` |
| `fill` | `true` | Extend the rule between the left and right groups to the full width and close with the right cap. With `false`, lines are left-packed. |
| `separator` | style-dependent | Default separator between modules. |
| `separator_color` | `muted` | Every separator's colour: a theme role or a literal, or `inherit`, which paints each separator in the colour of the first coloured, undimmed segment of the module before it (an icon or a value, never a `label` or an align pad), falling back to `muted`. |
| `first` `middle` `last` `single` | style-dependent | Line prefixes (`single` when there is one line). |
| `right_first` `right_middle` `right_last` `right_single` | style-dependent | Right caps. |
| `fill_char` | style-dependent | The rule character (must be one cell wide). |
| `pad` | style-dependent | Text between prefix/content and content/rule. |
| `top_left` `top_right` `bottom_left` `bottom_right` `side` | style-dependent (none for `none` and `powerline`) | A box's corners and side (`[box.<name>]` below), one cell each; a box without a `style` of its own draws with these. |
| `fill_pattern` | `""` | One-cell glyphs repeated across the rule instead of `fill_char`; each tick the pattern shifts `fill_step` cells in `fill_direction`, so dots appear to travel along the rule. The rule's width never changes, only which glyph lands in each cell. Empty keeps the static rule. |
| `fill_step` | `1` | Cells the pattern shifts per tick (0.001–1000; 0.5 = every second tick). |
| `fill_direction` | `right` | `left` \| `right`: which way the pattern travels. |
| `separator_frames` | `[]` | Separator strings cycled one per tick; every frame must have the same width (validation rejects a mismatch so columns cannot jitter). A per-line `separator` wins over the frames. Empty keeps the static `separator`. |
| `separator_step` | `1` | Frames the separator advances per tick (0.001–1000). |

Animations follow the clock rule of [Animation](guide.md#animation): frame = `floor(now × step) mod period`, so `animate = false` or `GARNISH_ANIMATE=0` freezes them at frame 0, which is also what these generated samples show.


### Frame styles

`none`

```text
❖ Opus  ⊞ ████████▍░░░░░░░░░░▏ 42%                        ⠋ 16:00:00
⏳ 24% ⏱ 2h13m  ≣ 41% ⏱ 3d4h                          ⛁ 91% 1h ✦ 47m
```

`rounded`

```text
╭─ ❖ Opus │ ⊞ ████████▍░░░░░░░░░░▏ 42% ─────────────── ⠋ 16:00:00 ─╮
╰─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ───────────────── ⛁ 91% 1h ✦ 47m ─╯
```

`square`

```text
┌─ ❖ Opus │ ⊞ ████████▍░░░░░░░░░░▏ 42% ─────────────── ⠋ 16:00:00 ─┐
└─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ───────────────── ⛁ 91% 1h ✦ 47m ─┘
```

`double`

```text
╔═ ❖ Opus │ ⊞ ████████▍░░░░░░░░░░▏ 42% ═══════════════ ⠋ 16:00:00 ═╗
╚═ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ═════════════════ ⛁ 91% 1h ✦ 47m ═╝
```

`heavy`

```text
┏━ ❖ Opus │ ⊞ ████████▍░░░░░░░░░░▏ 42% ━━━━━━━━━━━━━━━ ⠋ 16:00:00 ━┓
┗━ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ━━━━━━━━━━━━━━━━━ ⛁ 91% 1h ✦ 47m ━┛
```

`powerline`

```text
 ❖ Opus  ⊞ ████████▍░░░░░░░░░░▏ 42%                   ⠋ 16:00:00 
 ⏳ 24% ⏱ 2h13m  ≣ 41% ⏱ 3d4h                     ⛁ 91% 1h ✦ 47m 
```

### Aligned columns

With `align = true` every module column is padded to the widest module in it, so the separators fall on the same cell in every line (only between lines that share a `separator`). `durations = "fixed"` keeps timers from changing width as they tick. The same three lines, `align = false` then `align = true`:

```text
╭─ ❖ Opus │ ⊞ ████████▍░░░░░░░░░░▏ 42% ─────────────────────── ⠋ 16:00:00 ─╮
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d04h ──────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m00s ──────────────────────────────────╯
```

```text
╭─ ❖ Opus         │ ⊞ ████████▍░░░░░░░░░░▏ 42% ─────────────── ⠋ 16:00:00 ─╮
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d04h ──────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m        │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m00s ───────────────────────────╯
```

## `[[row]]`

Each entry is one row of the status line. `modules` are left-aligned, `right` are right-aligned, `separator` overrides the frame separator for that row. Any module id may appear on any row, in any order; a module that has nothing to show is skipped, and a row whose modules all have nothing to show is dropped (`hide_empty_rows`). `modules = []` with no `right` is a spacer: an empty framed row that always stays. With `style = "none"` a spacer is whitespace only, and Claude Code drops whitespace-only rows from the script's output when colour is off (`color = "never"`, `NO_COLOR`; with colour on the rule's colour codes keep the row; `preview --color never` shows what the screen drops). `blank = true` on the spacer keeps it on screen either way by giving the row one invisible cell (a braille blank, U+2800, which the harness does not trim and a font with the clock spinner's braille should draw empty). It is off by default, so the harness's own rule stands unless you opt in; on a row with modules it is reported.

`[[line]]` is the permanent alias of `[[row]]`: every config written before rows existed keeps working, and `config check` says nothing about it. A file uses one name or the other — carrying both arrays is reported and the `[[line]]` entries ignored, because TOML gives no order between two arrays of tables.

```toml
[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]
separator = "  "

[[row]]
modules = []          # a spacer
blank = true          # keep it on screen even without a frame
```

## `[[row.col]]`

A row is columns side by side; a row written with `modules`/`right` and no `[[row.col]]` is one column filling the width, which is what every config above is. Columns share the row's width by `width`:

| value | meaning |
|---|---|
| `"<n>fr"` | a share of the width left over once the others are placed (`"1fr"` by default, so three bare columns are thirds and six are sixths) |
| `"auto"` | exactly the column's content, re-measured every tick — for values that hold still (a clock under `durations = "fixed"`, a module with `max_width`), not for branch names |
| an integer | that many cells |

`gap` is the empty cells between columns (1 by default; on a one-line row the rule runs through them, so a centred module floats on one continuous rule). `justify` (`left` \| `center` \| `right`) places a column's `modules` when it has no `right` group; its default follows the column's position, so a three-column row reads left / centre / right without saying so. A column with both `modules` and `right` is the flex form of a plain row, laid out to the column's width. Content wider than its column is cut with `…` (or scrolled under `overflow = "ticker"`) and never spills into a neighbour, which is what keeps a layout's shape as the terminal is resized.

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

```text
── ❒ ~/projects/garnish ───────────────────────── ❖ Opus │ ⚙ ▁▃▅▇█ ─────────────────── ⊞ ████████▍░░░░░░░░░░▏ 42% ──
```

A column can hold a **stack** of rows instead of modules (`[[row.col.row]]`), and then the row is as tall as its tallest column; `valign` (`top` \| `center` \| `bottom`) places a stack shorter than its row. An inner row takes every row key but `gap` and `[[row.col]]`: the tree is two levels deep and never deeper.

## Titles

`title` is plain text set into a row's rule in the frame colour, with `title_pad` spaces on each side (1 by default) and `title_color` for another role or literal. `title_justify` puts it right after the left cap, centred in the widest empty gap of the line, or right before the right cap. A title wider than its space is cut with `…` and never widens the line, and a row with only a title is a titled spacer that is always kept.

```toml
[[row]]
title = "Session"
modules = []

[[row]]
title = "Usage"
title_justify = "right"
modules = ["limit5h", "limit7d"]
```

```text
╭─ Session ────────────────────────────────────────────────────────────────╮
╰─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ────────────────────────────────── Usage ─╯
```

## `[box.<name>]`

A box frames a run of rows, or a whole column, with its own corners and sides in place of the frame's caps: two extra lines, so a box is at least three lines tall. Three ways to join one: adjacent rows with the same `box = "<name>"` form one box, inside a stack as at the top level; `box = "<name>"` or `box = true` on a column makes the whole column one box the row's full height; `box = true` on a row boxes that row alone, and then the row's own `title*` keys title it. Boxes never nest. A box is one run of adjacent rows in the whole file, so a name that comes back anywhere after it is reported and that run left unboxed.

| key | default | meaning |
|---|---|---|
| `title` `title_justify` `title_pad` `title_color` | none | the title set into the box's top rule, as for a row |
| `style` | the `[frame]` style | `none` \| `rounded` \| `square` \| `double` \| `heavy` \| `custom`; when the frame's style has no box shape (`none`, `powerline`) an unstyled box is `rounded`, and a box that asks for `none` itself is invisible |
| `fill` | `false` | draw the rule between a row's groups inside the box; off by default, because a clean interior is what a box is for |
| `color` | the frame colour | role or literal for the box's glyphs |

```toml
[box.repo]
title = "Repository"
style = "double"

[[row]]
box = "repo"
modules = ["path", "model"]
right   = ["clock"]

[[row]]
box = "repo"
modules = ["context"]
```

```text
╔═ Repository ═════════════════════════════════════════╗
║ ❒ ~/projects/garnish │ ❖ Opus             ⠋ 16:00:00 ║
║ ⊞ ████████▍░░░░░░░░░░▏ 42%                           ║
╚══════════════════════════════════════════════════════╝
```

## Top-level presets

### `default`

Module preset `default`. Lines:

- `path branch sync worktree pr` ⟶ session_name agent
- `model effort context style` ⟶ vim
- `limit5h limit7d spend cost` ⟶ lines
- `session api cache` ⟶ clock

At 80 columns, unicode icons:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ───────────────────────── ❯ garnish-dev ─╮
├─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% ──────────────────────────┤
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ───────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m ──────────────────────── ⠋ 16:00:00 ─╯
```

### `minimal`

Module preset `minimal`. Lines:

- `path branch context limit5h cost` ⟶ clock

At 80 columns, unicode icons:

```text
~/garnish  42%  24%                                                    16:00
```

### `full`

Module preset `full`. Lines:

- `path branch sync worktree pr` ⟶ session_name agent
- `model effort context style` ⟶ vim
- `limit5h limit7d spend cost` ⟶ lines
- `session api cache` ⟶ clock

At 120 columns, unicode icons:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ pending ──────────────────────────────────────────────── ❯ garnish-dev sess-000 ─╮
├─ ❖ Opus ⋯ claude-opus-5 │ ⚙ ▁▃▅▇█ high │ ⊞ ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ │ ✎ default ───────────┤
├─ ⏳ █▉░░░░░░ 24% ⏱ 2h13m │ ≣ ███▎░░░░ 41% ⏱ 3d4h ──────────────────────────────────────────── Δ +156 −23 (+133) ─┤
╰─ ⏱ 1h12m since 14:48 │ ⇄ 8m20s (12%) │ ⛁ 91% 1h ✦ 47m 2 misses 352kw ───────────── ⠋ 16:00:00 Sat 01 Feb +00:00 ─╯
```

### `compact`

Module preset `default`. Lines:

- `path branch sync pr` ⟶ clock
- `model effort context limit5h cost` ⟶ cache

At 90 columns, unicode icons:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% │ ⏳ 24% ⏱ 2h13m ── ⛁ 91% 1h ✦ 47m ─╯
```

## `[modules.<id>]`

Every module accepts `enabled`, `preset`, `refresh` (the seconds a cached module's value lives before its worker refreshes it; a module that renders from the payload every tick takes only `0`), `hide` (a list of the states in which it leaves its row: `empty`, and `zero` or `below:N` / `above:N` where the module's page lists them; `hide_when_empty` is the older spelling of `empty`, and the two combine), `label`, `prefix`, `suffix`, `hide_when_empty`, `max_width` (cells the whole module is cut to with `…`, before alignment; 0 = unlimited), an `icons` table and a `colors` table, plus its own options. Resolution order: built-in default → icon set → the module preset the top-level `preset` implies → the module's own `preset` → explicit key. See the per-module pages in [modules/](modules/). `[modules.text.<name>]` defines a text box of your own, placed as `text.<name>`; see [text](modules/text.md).

## Environment

| variable | effect |
|---|---|
| `COLUMNS` | Terminal width (set by Claude Code). `GARNISH_COLUMNS` is the fallback; 120 when neither is set. The lines are rendered 4 cells narrower, plus `padding`: the width of Claude Code's status line box. |
| `NO_COLOR` | Disables escape codes under `color = "auto"` when set and not empty (no-color.org). |
| `GARNISH_CONFIG` | Config file path, absolute; a relative one is ignored. |
| `GARNISH_CACHE_DIR` | Cache root (default `$XDG_RUNTIME_DIR/garnish`, `$XDG_CACHE_HOME/garnish`, `~/.cache/garnish`). |
| `GARNISH_NOW` | Freeze the clock (epoch seconds or RFC 3339) for reproducible renders. |
| `GARNISH_NO_SPAWN` | Log intended background refreshes to `<cache>/spawns.log` instead of spawning them (tests). |
| `GARNISH_ANIMATE` | `0` (or `false`, `no`, `off`) freezes every animation (spinner, scrolling text, rule pattern, separator and icon frames) at frame 0 for the session and cuts a ticker line with `…`; for screen readers and recordings. |
| `GARNISH_DEBUG` | `1` appends a line per tick to `<cache>/debug.log`, rotated at 1 MiB; `garnish doctor` shows the tail. Nothing is written otherwise. |
| `GARNISH_MANAGED_SETTINGS` | The organisation settings file read first in Claude Code's chain, instead of the platform's and its `managed-settings.d` drop-ins; empty means there is none, and a relative path is ignored. |
| `GARNISH_STDIN_TTY` | `1` or `0` overrides the "is stdin a terminal" check of the bare `garnish`, which prints a pointer at `garnish setup` instead of waiting on a terminal (tests). |
| `GARNISH_TEST_PANIC` | Debug builds only: a tick panics before it renders, so the `⚠ garnish: internal error` row is testable (tests). |
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, `DISABLE_AUTO_COMPACT`, `DISABLE_COMPACT` | Read to place the `context` compaction marker exactly where Claude Code will compact; the last two turn compaction off, so the marker goes with it. |
| `CLAUDE_CONFIG_DIR` | Where the `account` worker reads `.claude.json` when it is set and non-empty, instead of the home directory (Claude Code keeps every `~/.claude` file there); the settings chain does not follow it yet. |
