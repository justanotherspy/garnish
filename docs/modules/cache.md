# `cache`

Prompt cache hit ratio, TTL and warmth.

Hit ratio from `prompt_cache.hit_ratio` (falls back to the last request's cache-read share), the cache lifetime badge (`5m` or `1h`), and a live countdown until the cached prefix goes cold. Shows `–` before the first API response.

**Sources:** `prompt_cache.*`, `context_window.current_usage`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `91%` |
| `default` | `⛁ 91% 1h ● 47m` |
| `full` | `⛁ 91% 1h ● 47m 2 misses 352kw` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` 91% 1h  47m` |
| `unicode` | `⛁ 91% 1h ● 47m` |
| `emoji` | `🗄️ 91% 1h 🔥 47m` |
| `ascii` | `cache: 91% 1h warm 47m` |

## Options

`[modules.cache]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_ttl` | bool | `false` | `true` | `true` | Show the TTL badge. |
| `show_countdown` | bool | `false` | `true` | `true` | Show the warm countdown / cold state. |
| `show_misses` | bool | `false` | `false` | `true` | Show the miss count. |
| `show_writes` | bool | `false` | `false` | `true` | Show tokens written to the cache. |

## Icons

`[modules.cache.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `cache` | `U+F1C0` | `⛁` | `🗄️` | `cache:` | Cache icon. |
| `warm` | `U+F06D` | `●` | `🔥` | `warm` | Warm glyph. |
| `cold` | `U+F2DC` | `○` | `❄️` | `cold` | Cold glyph. |

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
