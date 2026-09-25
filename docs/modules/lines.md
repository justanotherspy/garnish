# `lines`

Lines added and removed this session.

`cost.total_lines_added` and `cost.total_lines_removed`. The `full` preset adds the net delta.

**Sources:** `cost.total_lines_added`, `cost.total_lines_removed`

**Refresh:** every tick, nothing cached

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
| `refresh` | `0` | `0` | `0` | `0` | This module renders from the payload every tick; any value but 0 is reported. |
| `hide` | list of `empty`, `zero` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine; `zero` when the count is zero. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_net` | bool | `false` | `false` | `true` | Append the net change. |
| `hide_zero` | bool | `true` | `true` | `true` | Hide when nothing changed. |

## Icons

`[modules.lines.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `lines` | `U+F440` | `Δ` | `📝` | — | Diff icon. |
| `added` | `+` | `+` | `+` | `+` | Added glyph. |
| `removed` | `−` | `−` | `−` | `-` | Removed glyph. |

Also try (`lines`: `U+F457` `U+F0CB` `Δ` `∆`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.lines.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `added` | `ok` | Added count. |
| `removed` | `danger` | Removed count. |
| `net` | `muted` | Net delta. |
