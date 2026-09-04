# `lines`

Lines added and removed this session.

`cost.total_lines_added` and `cost.total_lines_removed`. The `full` preset adds the net delta.

**Sources:** `cost.total_lines_added`, `cost.total_lines_removed`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `+156 −23` |
| `default` | `Δ +156 −23` |
| `full` | `Δ +156 −23 (+133)` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` +156 −23` |
| `unicode` | `Δ +156 −23` |
| `emoji` | `📝 +156 −23` |
| `ascii` | `+156 -23` |

## Options

`[modules.lines]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_net` | bool | `false` | `false` | `true` | Append the net change. |
| `hide_zero` | bool | `true` | `true` | `true` | Hide when nothing changed. |

## Icons

`[modules.lines.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `lines` | `U+F440` | `Δ` | `📝` | `` | Diff icon. |
| `added` | `+` | `+` | `+` | `+` | Added glyph. |
| `removed` | `−` | `−` | `−` | `-` | Removed glyph. |

## Colors

`[modules.lines.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `added` | `ok` | Added count. |
| `removed` | `danger` | Removed count. |
| `net` | `muted` | Net delta. |
