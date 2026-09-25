# `limit5h`

Five-hour rate limit usage and time until reset.

Percentage of the rolling five-hour window consumed and a countdown to `resets_at`. Only present for Claude.ai Pro/Max subscriptions; hidden otherwise.

**Sources:** `rate_limits.five_hour.used_percentage`, `rate_limits.five_hour.resets_at`

**Refresh:** every tick, nothing cached

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
| `refresh` | `0` | `0` | `0` | `0` | This module renders from the payload every tick; any value but 0 is reported. |
| `hide` | list of `empty`, `below:N`, `above:N` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine; `below:N` and `above:N` compare the percentage the row prints. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`, `-` in the ascii set). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the window icon. |
| `show_reset` | bool | `false` | `true` | `true` | Show when the window resets, in the form `reset` picks. |
| `reset` | `countdown` \| `absolute` \| `both` \| `elapsed` | `"countdown"` | `"countdown"` | `"countdown"` | How the reset shows: `countdown` (`⏱2h13m`), `absolute` the local wall-clock time (`⏱14:30`, no weekday or date, since this window resets within the day, so the width stays steady), `both` (`2h13m (14:30)`), or `elapsed` the time into the window over its length (`⏱2h46m/5h`); `show_reset = false` hides every form. |
| `bar_width` | integer ≤ 1024 | `0` | `0` | `8` | Mini bar width in cells; 0 hides it. |
| `bar` | `blocks` \| `line` | `"blocks"` | `"blocks"` | `"blocks"` | Bar glyphs: `blocks` (the icon set's `█`/`░`, fractional cells) or `line` (`━`/`─`, `=`/`-` in the ascii set; whole cells, so no hairline gaps where the font draws `█` narrow). Explicit `icons.fill`/`icons.empty` win. |
| `thresholds` | list of numbers | `[50, 75, 90]` | `[50, 75, 90]` | `[50, 75, 90]` | Ascending percentages where the band color changes. |
| `durations` | `inherit` \| `compact` \| `fixed` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's timers and countdowns print: `inherit` follows the top-level `durations`; `compact` or `fixed` pins this module. |
| `percent` | `inherit` \| `whole` \| `precise` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's percentages print: `inherit` follows `[format] percent`; `whole` (42%) or `precise` (42.3%) pins this module. |
| `band_colors` | list of colors | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | `["band1", "band2", "band3", "band4"]` | One color per band (roles or literal colors). |
| `pace` | bool | `false` | `false` | `false` | Print the difference between the share used and the share of the window elapsed: `⇡14%` ahead of pace in `colors.ahead`, `⇣32%` behind in `colors.behind`, a zero difference bare. |
| `pace_colors` | bool | `false` | `false` | `false` | Colour the percentage by the pace band instead of `thresholds`: used ÷ elapsed at most 1 is nominal, at most 1.5 caution, above that critical; ignored under 20 % used, always critical above 80 %. |
| `eta` | bool | `false` | `false` | `false` | Print the time until the window reaches 100 % at the current rate (`⇥ 1h37m`), only when that lands before the reset. |
| `elapsed_marker` | bool | `false` | `false` | `false` | Draw the `marker` glyph on the mini bar at the share of the window elapsed, so usage and time read together; needs `bar_width`. |

## Icons

`[modules.limit5h.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `window` | `U+F252` | `⏳` | `⏳` | `5h` | Window icon. |
| `reset` | `U+F017` | `⏱` | `⏰` | `reset` | The reset's glyph, in every `reset` form. |
| `fill` | `█` | `█` | `█` | `#` | Filled bar cell. |
| `empty` | `░` | `░` | `░` | `-` | Empty bar cell. |
| `ahead` | `⇡` | `⇡` | `🔼` | `^` | Ahead-of-pace glyph. |
| `behind` | `⇣` | `⇣` | `🔽` | `v` | Behind-pace glyph. |
| `eta` | `U+F04E` | `⇥` | `⏩` | `eta` | Eta glyph. |
| `marker` | `▏` | `▏` | `▏` | `\|` | Elapsed marker on the bar. |

Also try (`window`: `U+F250` `U+F133` `⏳` `≣` `⌛`; `fill`: `█` `━` `▓` `#` `=`; `empty`: `░` `─` `▒` `.` `-`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.limit5h.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `reset` | `muted` | The reset, in every `reset` form. |
| `empty` | `muted` | Empty part of the bar. |
| `ahead` | `hot` | Pace delta, ahead. |
| `behind` | `ok` | Pace delta, behind. |
| `eta` | `hot` | Eta. |
| `marker` | `muted` | Elapsed marker. |
| `pace_nominal` | `ok` | Percentage, nominal pace. |
| `pace_caution` | `warn` | Percentage, caution. |
| `pace_critical` | `danger` | Percentage, critical. |
