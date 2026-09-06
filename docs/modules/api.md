# `api`

Time spent waiting for API responses.

`cost.total_api_duration_ms`, a subset of the session duration. The `full` preset adds its share of the session.

**Sources:** `cost.total_api_duration_ms`, `cost.total_duration_ms`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `8m20s` |
| `default` | `⇄ 8m20s` |
| `full` | `⇄ 8m20s (12%)` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` 8m20s` |
| `unicode` | `⇄ 8m20s` |
| `emoji` | `📡 8m20s` |
| `ascii` | `api: 8m20s` |

## Options

`[modules.api]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_share` | bool | `false` | `false` | `true` | Append the share of the session. |

## Icons

`[modules.api.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `api` | `U+F0EC` | `⇄` | `📡` | `api:` | API icon. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.api.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `value` | `text` | Duration. |
| `share` | `muted` | Share of session. |
