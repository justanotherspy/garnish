# `session`

Session duration.

Wall-clock time since the session started (`cost.total_duration_ms`; resets on `/clear`). The `full` preset adds the start time.

**Sources:** `cost.total_duration_ms`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `1h12m` |
| `default` | `⏱ 1h12m` |
| `full` | `⏱ 1h12m since 14:48` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` 1h12m` |
| `unicode` | `⏱ 1h12m` |
| `emoji` | `⌚ 1h12m` |
| `ascii` | `t: 1h12m` |

## Options

`[modules.session]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_start` | bool | `false` | `false` | `true` | Append the start time (HH:MM). |
| `durations` | `inherit` \| `compact` \| `fixed` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's timers and countdowns print: `inherit` follows the top-level `durations`; `compact` or `fixed` pins this module. |

## Icons

`[modules.session.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `session` | `U+F017` | `⏱` | `⌚` | `t:` | Session icon. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.session.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `value` | `text` | Duration. |
| `start` | `muted` | Start time. |
