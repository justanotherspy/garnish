# `version`

The Claude Code version.

The payload's `version`, printed dim as `v2.1.270`: what a bug report needs and what shows an upgrade. Nothing shows when the payload carries no version, so `hide_when_empty = false` prints `–` as it does for any absent field.

**Sources:** `version`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `v2.1.260` |
| `default` | `v2.1.260` |
| `full` | `⊛ v2.1.260` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | `v2.1.260` |
| `unicode` | `v2.1.260` |
| `emoji` | `v2.1.260` |
| `ascii` | `v2.1.260` |

## Options

`[modules.version]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | `0` | `0` | `0` | `0` | This module renders from the payload every tick; any value but 0 is reported. |
| `hide` | list of `empty` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `false` | `true` | Show the icon. |

## Icons

`[modules.version.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `version` | `U+F02C` | `⊛` | `📦` | `` | Version icon. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.version.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `version` | `muted` | The version. |
