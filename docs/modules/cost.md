# `cost`

Estimated session cost in USD.

Shows `cost.total_cost_usd`. By default it is hidden for subscription sessions (those report `rate_limits`), so one usage line serves both auth modes; set `only_without_rate_limits = false` to always show it.

**Sources:** `cost.total_cost_usd`, `cost.total_lines_added`, `cost.total_lines_removed`, `rate_limits`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `$1.23` |
| `default` | `$1.23` |
| `full` | `$1.23 +156 −23` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` $1.23` |
| `unicode` | `$1.23` |
| `emoji` | `💵 $1.23` |
| `ascii` | `$1.23` |

## Options

`[modules.cost]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the cost icon. |
| `decimals` | integer | `2` | `2` | `2` | Decimal places. |
| `only_without_rate_limits` | bool | `true` | `true` | `true` | Hide when the harness reports subscription rate limits. |
| `show_lines` | bool | `false` | `false` | `true` | Append lines added/removed. |

## Icons

`[modules.cost.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `cost` | `U+F155` | `` | `💵` | `` | Cost icon. |
| `added` | `+` | `+` | `+` | `+` | Lines-added glyph. |
| `removed` | `−` | `−` | `−` | `-` | Lines-removed glyph. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.cost.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `ok` | Icon. |
| `amount` | `text` | Amount. |
| `added` | `ok` | Lines added. |
| `removed` | `danger` | Lines removed. |
