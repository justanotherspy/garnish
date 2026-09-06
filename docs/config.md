# Configuration reference

garnish reads `--config`, else `$GARNISH_CONFIG`, else `$XDG_CONFIG_HOME/garnish/garnish.toml` (`~/.config/garnish/garnish.toml`), else `~/.garnish.toml`. Without a file the built-in `default` preset is used. `garnish config init` writes an annotated file; `garnish config check` validates it; `garnish config show` prints the fully resolved result.

A bad key never blanks the status line: every valid key stays in effect, the built-in default stands in for the bad one, and a dim `⚠ config: <file> <path>: <message>` line is appended; only a file that does not parse as TOML falls back to the defaults wholesale, with the line of the syntax error.

## Top-level keys

| key | values | default | meaning |
|---|---|---|---|
| `preset` | `default` \| `minimal` \| `full` \| `compact` | `default` | Which lines exist and which module preset they imply, when `[[line]]` is absent. |
| `icons` | `nerd` \| `unicode` \| `emoji` \| `ascii` | `nerd` | Glyph set. `nerd` needs a Nerd Font. |
| `theme` | `garnish` \| `catppuccin-mocha` \| `nord` \| `dracula` \| `tokyonight` \| `mono` | `garnish` | Color palette (see below). |
| `color` | `auto` \| `always` \| `never` \| `256` \| `truecolor` | `auto` | Escape-code output. `auto` is truecolor unless `NO_COLOR` is set. |
| `truncate` | bool | `true` | Truncate the left group when a line overflows the width (`$COLUMNS − 4 − padding`); the right group is never cut. |
| `stale_style` | `dim` \| `hide` \| `plain` | `dim` | How overdue cached values are shown. |
| `stale_after` | integer ≥ 1 | `5` | TTL periods a cached value may be overdue before it is styled stale; until then the last value shows unchanged while a worker refreshes it. |
| `padding` | integer | `0` | Extra cells subtracted from the width, on top of the 4 Claude Code's box always takes; set `2 × statusLine.padding` when that setting is non-zero. |
| `align` | bool | `false` | Pad each module column to the widest module in it across lines, so the separators stack vertically (see [Aligned columns](#aligned-columns)). |
| `right_justify` | `end` \| `start` | `end` | Where a padded right-group module's text sits: `end` pads on the left so the text hugs the cap, `start` pads on the right so the text follows the separator. Only matters with `align = true` and a filled rule. |
| `hide_empty_lines` | bool | `true` | Drop a line whose modules all rendered nothing (outside a repository, a line of `branch sync pr` is empty); the frame's caps follow the surviving lines. A line configured as `modules = []` with no `right` is an intentional spacer and is always kept. With `stale_style = "hide"` a line of only cached modules can disappear while its values are overdue and return after the refresh; `hide_when_empty = false` on one module pins the row. |
| `overflow` | `truncate` \| `ticker` | `truncate` | A left group wider than its budget is cut with `…` (`truncate`) or scrolled (`ticker`): a window onto the group advances `ticker_step` cells per tick and wraps around with `ticker_gap` between the end and the start. The offset comes from the tick's clock, so it needs no state and `GARNISH_NOW` freezes it; it moves as often as Claude Code ticks (`refreshInterval`, at least 1 s). The right group is never scrolled or cut. |
| `ticker_step` | number | `1` | Cells the ticker advances per tick (must be > 0; `0.5` = every second tick). |
| `ticker_gap` | string | `"   "` | Text between the end of a scrolled group and its wrapped-around start. |
| `animate` | bool | `true` | Master switch for every animation (the clock spinner, scrolling text modules, the ticker, and the animated frame parts of § 4.2): `false` freezes them all at frame 0. `GARNISH_ANIMATE=0` does the same for one session; recommended for screen readers and recordings. |
| `durations` | `compact` \| `fixed` | `compact` | How elapsed times and countdowns print: `compact` drops a zero second unit (`8m20s`, `9m`, `2h`); `fixed` always shows two units with the small one two digits wide (`8m20s`, `9m00s`, `2h00m`), so timers keep their width. |

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
| `first` `middle` `last` `single` | style-dependent | Line prefixes (`single` when there is one line). |
| `right_first` `right_middle` `right_last` `right_single` | style-dependent | Right caps. |
| `fill_char` | style-dependent | The rule character (must be one cell wide). |
| `pad` | style-dependent | Text between prefix/content and content/rule. |
| `fill_pattern` | `""` | One-cell glyphs repeated across the rule instead of `fill_char`; each tick the pattern shifts `fill_step` cells in `fill_direction`, so dots appear to travel along the rule. The rule's width never changes, only which glyph lands in each cell. Empty keeps the static rule. |
| `fill_step` | `1` | Cells the pattern shifts per tick (0.5 = every second tick). |
| `fill_direction` | `right` | `left` \| `right`: which way the pattern travels. |
| `separator_frames` | `[]` | Separator strings cycled one per tick; every frame must have the same width (validation rejects a mismatch so columns cannot jitter). A per-line `separator` wins over the frames. Empty keeps the static `separator`. |
| `separator_step` | `1` | Frames the separator advances per tick. |

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

## `[[line]]`

Each entry is one output row. `modules` are left-aligned, `right` are right-aligned, `separator` overrides the frame separator for that line. Any module id may appear on any line, in any order; a module that has nothing to show is skipped, and a line whose modules all have nothing to show is dropped (`hide_empty_lines`). `modules = []` with no `right` is a spacer: an empty framed row that always stays. A spacer needs a visible frame: with `style = "none"` it is whitespace only, and Claude Code drops whitespace-only rows from the script's output (`preview` still shows it).

```toml
[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]
separator = "  "
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

Every module accepts `enabled`, `preset`, `refresh`, `label`, `prefix`, `suffix`, `hide_when_empty`, an `icons` table and a `colors` table, plus its own options. Resolution order: built-in default → icon set → module preset → top-level preset → explicit key. See the per-module pages in [modules/](modules/). `[modules.text.<name>]` defines a text box of your own, placed as `text.<name>`; see [text](modules/text.md).

## Environment

| variable | effect |
|---|---|
| `COLUMNS` | Terminal width (set by Claude Code). `GARNISH_COLUMNS` is the fallback; 120 when neither is set. The lines are rendered 4 cells narrower, plus `padding`: the width of Claude Code's status line box. |
| `NO_COLOR` | Disables escape codes under `color = "auto"`. |
| `GARNISH_CONFIG` | Config file path. |
| `GARNISH_CACHE_DIR` | Cache root (default `$XDG_RUNTIME_DIR/garnish`, `$XDG_CACHE_HOME/garnish`, `~/.cache/garnish`). |
| `GARNISH_NOW` | Freeze the clock (epoch seconds or RFC 3339) for reproducible renders. |
| `GARNISH_NO_SPAWN` | Log intended background refreshes to `<cache>/spawns.log` instead of spawning them (tests). |
| `GARNISH_ANIMATE` | `0` freezes every animation (spinner, scrolling text, ticker, patterned rule) at frame 0 for the session; for screen readers and recordings. |
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, `DISABLE_AUTO_COMPACT` | Read to place the `context` compaction marker exactly where Claude Code will compact. |
