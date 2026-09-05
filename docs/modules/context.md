# `context`

Context window usage bar with color bands and the auto-compaction marker.

A smooth bar spanning the full context window (`context_window.context_window_size`, 1M when absent). The filled part takes the color of the current band; a marker shows where Claude Code will auto-compact (`autoCompactWindow` / `CLAUDE_CODE_AUTO_COMPACT_WINDOW` minus the summary buffer). No token counter: the bar and the percentage are the story.

**Sources:** `context_window.used_percentage`, `context_window.context_window_size`, `exceeds_200k_tokens`, `~/.claude/settings.json autoCompactWindow/autoCompactEnabled`, `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `42%` |
| `default` | `⊞ ████████▍░░░░░░░░░░▏ 42%` |
| `full` | `⊞ ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` ████████▍░░░░░░░░░░▏ 42%` |
| `unicode` | `⊞ ████████▍░░░░░░░░░░▏ 42%` |
| `emoji` | `🧠 ████████▍░░░░░░░░░░▏ 42%` |
| `ascii` | `ctx: ########-----------\| 42%` |

## Options

`[modules.context]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `width` | integer | `0` | `20` | `30` | Bar width in cells; 0 hides the bar. |
| `bar` | `blocks` \| `line` | `"blocks"` | `"blocks"` | `"blocks"` | Bar glyphs: `blocks` (the icon set's `█`/`░`, fractional cells) or `line` (`━`/`─`, `=`/`-` in the ascii set; whole cells, so no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win. |
| `show_icon` | bool | `false` | `true` | `true` | Show the context icon. |
| `show_percent` | bool | `true` | `true` | `true` | Show the percentage after the bar. |
| `thresholds` | list of numbers | `[50, 75, 90]` | `[50, 75, 90]` | `[50, 75, 90]` | Ascending percentages where the band color changes. |
| `band_colors` | list of colors | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | One color per band (roles or literal colors). |
| `compaction_marker` | bool | `true` | `true` | `true` | Mark the auto-compaction threshold on the bar. |
| `compact_buffer_tokens` | integer | `13000` | `13000` | `13000` | Tokens Claude Code reserves below the window for the compaction summary. |
| `show_compaction_percent` | bool | `false` | `false` | `true` | Also print the compaction threshold as a percentage. |
| `show_window` | bool | `false` | `false` | `true` | Show the window size tag (`1M`, `200k`). |
| `exceeds_200k` | bool | `false` | `false` | `true` | Show an indicator when the last response exceeded 200k tokens. |
| `warn_at` | number | `0` | `0` | `0` | Extra warning badge at or above this percentage; 0 disables. |

## Icons

`[modules.context.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `context` | `U+F2DB` | `⊞` | `🧠` | `ctx:` | Context icon. |
| `fill` | `█` | `█` | `█` | `#` | Filled cell. |
| `empty` | `░` | `░` | `░` | `-` | Empty cell. |
| `marker` | `▏` | `▏` | `▏` | `|` | Compaction marker. |
| `compact` | `⤓` | `⤓` | `⤓` | `compact@` | Compaction label glyph. |
| `exceeds` | `‼` | `‼` | `‼` | `!!` | Exceeds-200k indicator. |
| `warn` | `U+F071` | `⚠` | `⚠` | `!` | Warning badge. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.context.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `percent` | `text` | Percentage text. |
| `empty` | `muted` | Empty part of the bar. |
| `marker` | `warn` | Compaction marker. |
| `exceeds` | `danger` | Exceeds-200k indicator. |
| `window` | `muted` | Window size tag. |
| `warn` | `danger` | Warning badge. |
