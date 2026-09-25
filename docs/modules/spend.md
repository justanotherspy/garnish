# `spend`

Spend-limit usage behind a Claude apps gateway.

Percentage of the applicable spend limit consumed (can exceed 100%) and a countdown to the period reset. Hidden unless a gateway reports it.

**Sources:** `rate_limits.spend_limit.used_percentage`, `rate_limits.spend_limit.resets_at`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `112%` |
| `default` | `$ 112% ⏱ 27d8h` |
| `full` | `$ ████████ 112% ⏱ 27d8h` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` 112%  27d8h` |
| `unicode` | `$ 112% ⏱ 27d8h` |
| `emoji` | `💳 112% ⏰ 27d8h` |
| `ascii` | `spend 112% reset 27d8h` |

## Options

`[modules.spend]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | `0` | `0` | `0` | `0` | This module renders from the payload every tick; any value but 0 is reported. |
| `hide` | list of `empty`, `below:N`, `above:N` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine; `below:N` and `above:N` compare the percentage the row prints. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the window icon. |
| `show_reset` | bool | `false` | `true` | `true` | Show when the window resets, in the form `reset` picks. |
| `reset` | `countdown` \| `absolute` \| `both` | `"countdown"` | `"countdown"` | `"countdown"` | How the reset shows: `countdown` (`⏱27d8h`), `absolute` the local date the window resets on, since it is weeks away and a clock time alone would read as tonight (`⏱Mar 1`), or `both` (`27d8h (Mar 1)`); `show_reset = false` hides every form. |
| `bar_width` | integer ≤ 1024 | `0` | `0` | `8` | Mini bar width in cells; 0 hides it. |
| `bar` | `blocks` \| `line` | `"blocks"` | `"blocks"` | `"blocks"` | Bar glyphs: `blocks` (the icon set's `█`/`░`, fractional cells) or `line` (`━`/`─`, `=`/`-` in the ascii set; whole cells, so no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win. |
| `thresholds` | list of numbers | `[50, 75, 90]` | `[50, 75, 90]` | `[50, 75, 90]` | Ascending percentages where the band color changes. |
| `durations` | `inherit` \| `compact` \| `fixed` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's timers and countdowns print: `inherit` follows the top-level `durations`; `compact` or `fixed` pins this module. |
| `percent` | `inherit` \| `whole` \| `precise` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's percentages print: `inherit` follows `[format] percent`; `whole` (42%) or `precise` (42.3%) pins this module. |
| `band_colors` | list of colors | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | One color per band (roles or literal colors). |

## Icons

`[modules.spend.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `window` | `U+F0D6` | `$` | `💳` | `spend` | Window icon. |
| `reset` | `U+F017` | `⏱` | `⏰` | `reset` | The reset's glyph, in every `reset` form. |
| `fill` | `█` | `█` | `█` | `#` | Filled bar cell. |
| `empty` | `░` | `░` | `░` | `-` | Empty bar cell. |

Also try (`window`: `U+F250` `U+F133` `⏳` `≣` `⌛`; `fill`: `█` `━` `▓` `#` `=`; `empty`: `░` `─` `▒` `.` `-`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.spend.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `reset` | `muted` | The reset, in every `reset` form. |
| `empty` | `muted` | Empty part of the bar. |
