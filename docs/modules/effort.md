# `effort`

Reasoning effort level as a five-step scale and/or word.

Shows `effort.level` (`low`, `medium`, `high`, `xhigh`, `max`). Hidden when the model does not support effort. The scale lights one step per level.

**Sources:** `effort.level`

**Refresh:** every tick, nothing cached

## Presets

| preset | render |
|---|---|
| `minimal` | `high` |
| `default` | `⚙ ▁▃▅▇█` |
| `full` | `⚙ ▁▃▅▇█ high` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` ▁▃▅▇█` |
| `unicode` | `⚙ ▁▃▅▇█` |
| `emoji` | `🎯 ▁▃▅▇█` |
| `ascii` | `.:=+#` |

## Options

`[modules.effort]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | `0` | `0` | `0` | `0` | This module renders from the payload every tick; any value but 0 is reported. |
| `hide` | list of `empty` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`, `-` in the ascii set). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `style` | `scale` \| `word` \| `both` | `"word"` | `"scale"` | `"both"` | How to show the level. |
| `show_icon` | bool | `false` | `true` | `true` | Show the effort icon. |

## Icons

`[modules.effort.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `effort` | `U+F0E4` | `⚙` | `🎯` | — | Effort icon. |
| `scale` | `▁▃▅▇█` | `▁▃▅▇█` | `▁▃▅▇█` | `.:=+#` | Five glyphs, one per level, lowest first. |

Also try (`effort`: `U+F012` `U+F080` `⚙` `✱`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.effort.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `active` | `accent2` | Lit scale steps. |
| `inactive` | `muted` | Unlit scale steps. |
| `word` | `text` | Level word. |
