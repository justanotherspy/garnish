# `cache`

Prompt cache hit ratio, TTL and warmth.

Hit ratio from `prompt_cache.hit_ratio` (falls back to the last request's cache-read share), the cache lifetime badge (`5m` or `1h`), and a live countdown until the cached prefix goes cold. Shows `–` (`-` in the ascii set) before the first API response.

**Sources:** `prompt_cache.*`, `context_window.current_usage`

**Refresh:** every tick, nothing cached

## Presets

| preset | render |
|---|---|
| `minimal` | `91%` |
| `default` | `⛁ 91% 1h ✦ 47m` |
| `full` | `⛁ 91% 1h ✦ 47m 2 misses 352kw` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` 91% 1h  47m` |
| `unicode` | `⛁ 91% 1h ✦ 47m` |
| `emoji` | `💾 91% 1h 🔥 47m` |
| `ascii` | `cache: 91% 1h warm 47m` |

## Options

`[modules.cache]`

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
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_ttl` | bool | `false` | `true` | `true` | Show the TTL badge. |
| `show_countdown` | bool | `false` | `true` | `true` | Show the warm countdown / cold state. |
| `show_misses` | bool | `false` | `false` | `true` | Show the miss count. |
| `show_writes` | bool | `false` | `false` | `true` | Show tokens written to the cache. |
| `durations` | `inherit` \| `compact` \| `fixed` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's timers and countdowns print: `inherit` follows the top-level `durations`; `compact` or `fixed` pins this module. |
| `tokens` | `inherit` \| `compact` \| `precise` \| `whole` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's token counts print: `inherit` follows `[format] tokens`; `compact` (128k, 1.0M), `precise` (128,400) or `whole` (128400) pins this module. |
| `percent` | `inherit` \| `whole` \| `precise` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's percentages print: `inherit` follows `[format] percent`; `whole` (42%) or `precise` (42.3%) pins this module. |

## Icons

`[modules.cache.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `cache` | `U+F1C0` | `⛁` | `💾` | `cache:` | Cache icon. |
| `warm` | `U+F06D` | `✦` | `🔥` | `warm` | Warm glyph. |
| `cold` | `U+F2DC` | `✧` | `🧊` | `cold` | Cold glyph. |

Also try (`cache`: `U+F0A0` `U+F187` `⛁` `⛃`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.cache.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `percent` | `text` | Hit ratio. |
| `ttl` | `muted` | TTL badge. |
| `warm` | `ok` | Warm countdown. |
| `cold` | `danger` | Cold state. |
| `detail` | `muted` | Misses and writes. |
