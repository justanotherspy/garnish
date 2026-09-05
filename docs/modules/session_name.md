# `session_name`

Session name.

The name set with `--name` or `/rename`, or the AI-generated title. Hidden when the session only has its default name. The `full` preset appends the short session id.

**Sources:** `session_name`, `session_id`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `garnish-dev` |
| `default` | `❯ garnish-dev` |
| `full` | `❯ garnish-dev sess-000` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` garnish-dev` |
| `unicode` | `❯ garnish-dev` |
| `emoji` | `🔖 garnish-dev` |
| `ascii` | `garnish-dev` |

## Options

`[modules.session_name]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_id` | bool | `false` | `false` | `true` | Append the first 8 characters of the session id. |
| `max_length` | integer | `32` | `32` | `32` | Truncate longer names (0 = no limit). |

## Icons

`[modules.session_name.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `name` | `U+F02B` | `❯` | `🔖` | `` | Name icon. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.session_name.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | Name. |
| `id` | `muted` | Session id. |
