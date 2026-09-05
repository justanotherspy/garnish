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
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the window icon. |
| `show_reset` | bool | `false` | `true` | `true` | Show the countdown to the reset. |
| `bar_width` | integer | `0` | `0` | `8` | Mini bar width in cells; 0 hides it. |
| `bar` | `blocks` \| `line` | `"blocks"` | `"blocks"` | `"blocks"` | Bar glyphs: `blocks` (the icon set's `█`/`░`, fractional cells) or `line` (`━`/`─`, `=`/`-` in the ascii set; whole cells, so no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win. |
| `thresholds` | list of numbers | `[50, 75, 90]` | `[50, 75, 90]` | `[50, 75, 90]` | Ascending percentages where the color changes. |
| `band_colors` | list of colors | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | One color per band. |

## Icons

`[modules.spend.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `window` | `U+F0D6` | `$` | `💳` | `spend` | Window icon. |
| `reset` | `U+F017` | `⏱` | `⏰` | `reset` | Countdown glyph. |
| `fill` | `█` | `█` | `█` | `#` | Bar filled cell. |
| `empty` | `░` | `░` | `░` | `-` | Bar empty cell. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.spend.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `reset` | `muted` | Countdown. |
| `empty` | `muted` | Bar empty part. |
