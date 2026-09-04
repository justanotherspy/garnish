# Configuration reference

garnish reads `--config`, else `$GARNISH_CONFIG`, else `$XDG_CONFIG_HOME/garnish/garnish.toml` (`~/.config/garnish/garnish.toml`), else `~/.garnish.toml`. Without a file the built-in `default` preset is used. `garnish config init` writes an annotated file; `garnish config check` validates it; `garnish config show` prints the fully resolved result.

An invalid file never blanks the status line: garnish renders the defaults and appends a dim `⚠ config: <file>:<line> <message>` line.

## Top-level keys

| key | values | default | meaning |
|---|---|---|---|
| `preset` | `default` \| `minimal` \| `full` \| `compact` | `default` | Which lines exist and which module preset they imply, when `[[line]]` is absent. |
| `icons` | `nerd` \| `unicode` \| `emoji` \| `ascii` | `nerd` | Glyph set. `nerd` needs a Nerd Font. |
| `theme` | `garnish` \| `catppuccin-mocha` \| `nord` \| `dracula` \| `tokyonight` \| `mono` | `garnish` | Color palette (see below). |
| `color` | `auto` \| `always` \| `never` \| `256` \| `truecolor` | `auto` | Escape-code output. `auto` is truecolor unless `NO_COLOR` is set. |
| `truncate` | bool | `true` | Truncate the left group when a line overflows `$COLUMNS`; the right group is never cut. |
| `stale_style` | `dim` \| `hide` \| `plain` | `dim` | How cached values past their TTL are shown while a worker refreshes them. |
| `padding` | integer | `0` | Cells subtracted from the width; mirror `statusLine.padding`. |

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
| `fill` | `true` | Extend the rule between the left and right groups to `$COLUMNS` and close with the right cap. With `false`, lines are left-packed. |
| `separator` | style-dependent | Default separator between modules. |
| `first` `middle` `last` `single` | style-dependent | Line prefixes (`single` when there is one line). |
| `right_first` `right_middle` `right_last` `right_single` | style-dependent | Right caps. |
| `fill_char` | style-dependent | The rule character (must be one cell wide). |
| `pad` | style-dependent | Text between prefix/content and content/rule. |

### Frame styles

`none`

```text
◆ Opus  ◫ ████████▍░░░░░░░░░░▏ 42%                            ⠋ 16:00:00
⧗ 24% ⏱ 2h13m  ▦ 41% ⏱ 3d4h                               ⛁ 91% 1h ● 47m
```

`rounded`

```text
╭─ ◆ Opus │ ◫ ████████▍░░░░░░░░░░▏ 42% ─────────────────── ⠋ 16:00:00 ─╮
╰─ ⧗ 24% ⏱ 2h13m │ ▦ 41% ⏱ 3d4h ────────────────────── ⛁ 91% 1h ● 47m ─╯
```

`square`

```text
┌─ ◆ Opus │ ◫ ████████▍░░░░░░░░░░▏ 42% ─────────────────── ⠋ 16:00:00 ─┐
└─ ⧗ 24% ⏱ 2h13m │ ▦ 41% ⏱ 3d4h ────────────────────── ⛁ 91% 1h ● 47m ─┘
```

`double`

```text
╔═ ◆ Opus │ ◫ ████████▍░░░░░░░░░░▏ 42% ═══════════════════ ⠋ 16:00:00 ═╗
╚═ ⧗ 24% ⏱ 2h13m │ ▦ 41% ⏱ 3d4h ══════════════════════ ⛁ 91% 1h ● 47m ═╝
```

`heavy`

```text
┏━ ◆ Opus │ ◫ ████████▍░░░░░░░░░░▏ 42% ━━━━━━━━━━━━━━━━━━━ ⠋ 16:00:00 ━┓
┗━ ⧗ 24% ⏱ 2h13m │ ▦ 41% ⏱ 3d4h ━━━━━━━━━━━━━━━━━━━━━━ ⛁ 91% 1h ● 47m ━┛
```

`powerline`

```text
◆ Opus  ◫ ████████▍░░░░░░░░░░▏ 42%                         ⠋ 16:00:00
⧗ 24% ⏱ 2h13m  ▦ 41% ⏱ 3d4h                            ⛁ 91% 1h ● 47m
```

## `[[line]]`

Each entry is one output row. `modules` are left-aligned, `right` are right-aligned, `separator` overrides the frame separator for that line. Any module id may appear on any line, in any order; a module that has nothing to show is skipped.

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

```text
╭─ ▣ ~/projects/garnish │ ⇄ #42 ○ ───────────────────────────────────────────────── ♯ garnish-dev ─╮
├─ ◆ Opus │ ◔ ▁▃▅▇█ │ ◫ ████████▍░░░░░░░░░░▏ 42% ──────────────────────────────────────────────────┤
├─ ⧗ 24% ⏱ 2h13m │ ▦ 41% ⏱ 3d4h ────────────────────────────────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m │ ⇄ 8m20s │ ⛁ 91% 1h ● 47m ──────────────────────────────────────────────── ⠋ 16:00:00 ─╯
```

### `minimal`

Module preset `minimal`. Lines:

- `path branch context limit5h cost` ⟶ clock

```text
~/garnish  42%  24%                                                                            16:00
```

### `full`

Module preset `full`. Lines:

- `path branch sync worktree pr` ⟶ session_name agent
- `model effort context style` ⟶ vim
- `limit5h limit7d spend cost` ⟶ lines
- `session api cache` ⟶ clock

```text
╭─ ▣ ~/projects/garnish │ ⇄ #42 ○ pending ──────────────────────────────── ♯ garnish-dev sess-000 ─╮
├─ ◆ Opus ⋯ claude-opus-5 │ ◔ ▁▃▅▇█ high │ ◫ ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ │ ✎… ──┤
├─ ⧗ █▉░░░░░░ 24% ⏱ 2h13m │ ▦ ███▎░░░░ 41% ⏱ 3d4h ───────────────────────────── Δ +156 −23 (+133) ─┤
╰─ ⏱ 1h12m since 14:48 │ ⇄ 8m20s (12%) │ ⛁ 91% 1h ● 47m 2 misses … ─ ⠋ 16:00:00 Sat 01 Feb +00:00 ─╯
```

### `compact`

Module preset `default`. Lines:

- `path branch sync pr` ⟶ clock
- `model effort context limit5h cost` ⟶ cache

```text
╭─ ▣ ~/projects/garnish │ ⇄ #42 ○ ──────────────────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─ ◆ Opus │ ◔ ▁▃▅▇█ │ ◫ ████████▍░░░░░░░░░░▏ 42% │ ⧗ 24% ⏱ 2h13m ───────────────── ⛁ 91% 1h ● 47m ─╯
```

## `[modules.<id>]`

Every module accepts `enabled`, `preset`, `refresh`, `label`, `prefix`, `suffix`, `hide_when_empty`, an `icons` table and a `colors` table, plus its own options. Resolution order: built-in default → icon set → module preset → top-level preset → explicit key. See the per-module pages in [modules/](modules/).

## Environment

| variable | effect |
|---|---|
| `COLUMNS` | Width of the status line (set by Claude Code). `GARNISH_COLUMNS` is the fallback; 120 when neither is set. |
| `NO_COLOR` | Disables escape codes under `color = "auto"`. |
| `GARNISH_CONFIG` | Config file path. |
| `GARNISH_CACHE_DIR` | Cache root (default `$XDG_RUNTIME_DIR/garnish`, `$XDG_CACHE_HOME/garnish`, `~/.cache/garnish`). |
| `GARNISH_NOW` | Freeze the clock (epoch seconds or RFC 3339) for reproducible renders. |
| `GARNISH_NO_SPAWN` | Log intended background refreshes to `<cache>/spawns.log` instead of spawning them (tests). |
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, `DISABLE_AUTO_COMPACT` | Read to place the `context` compaction marker exactly where Claude Code will compact. |
