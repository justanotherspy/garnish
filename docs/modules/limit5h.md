# `limit5h`

Five-hour rate limit usage and time until reset.

Percentage of the rolling five-hour window consumed and a countdown to `resets_at`. Only present for Claude.ai Pro/Max subscriptions; hidden otherwise.

**Sources:** `rate_limits.five_hour.used_percentage`, `rate_limits.five_hour.resets_at`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `24%` |
| `default` | `⏳ 24% ⏱ 2h13m` |
| `full` | `⏳ █▉░░░░░░ 24% ⏱ 2h13m` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` 24%  2h13m` |
| `unicode` | `⏳ 24% ⏱ 2h13m` |
| `emoji` | `⏳ 24% ⏰ 2h13m` |
| `ascii` | `5h 24% reset 2h13m` |

## Options

`[modules.limit5h]`

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
| `bar` | `blocks` \| `line` | `"blocks"` | `"blocks"` | `"blocks"` | Bar glyphs: `blocks` (`█`/`░`, fractional cells) or `line` (`━`/`─`, whole cells; no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win. |
| `thresholds` | list of numbers | `[50, 75, 90]` | `[50, 75, 90]` | `[50, 75, 90]` | Ascending percentages where the color changes. |
| `band_colors` | list of colors | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | One color per band. |

## Icons

`[modules.limit5h.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `window` | `U+F252` | `⏳` | `⏳` | `5h` | Window icon. |
| `reset` | `U+F017` | `⏱` | `⏰` | `reset` | Countdown glyph. |
| `fill` | `█` | `█` | `█` | `#` | Bar filled cell. |
| `empty` | `░` | `░` | `░` | `-` | Bar empty cell. |

## Colors

`[modules.limit5h.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `reset` | `muted` | Countdown. |
| `empty` | `muted` | Bar empty part. |
