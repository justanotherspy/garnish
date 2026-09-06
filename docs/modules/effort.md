# `effort`

Reasoning effort level as a five-step scale and/or word.

Shows `effort.level` (`low`, `medium`, `high`, `xhigh`, `max`). Hidden when the model does not support effort. The scale lights one step per level.

**Sources:** `effort.level`

**Refresh:** every tick (payload only)

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
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `style` | `scale` \| `word` \| `both` | `"word"` | `"scale"` | `"both"` | How to show the level. |
| `show_icon` | bool | `false` | `true` | `true` | Show the effort icon. |

## Icons

`[modules.effort.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `effort` | `U+F0E4` | `⚙` | `🎯` | `` | Effort icon. |
| `scale` | `▁▃▅▇█` | `▁▃▅▇█` | `▁▃▅▇█` | `.:=+#` | Five glyphs, one per level, lowest first. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.effort.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `active` | `accent2` | Lit scale steps. |
| `inactive` | `muted` | Unlit scale steps. |
| `word` | `text` | Level word. |
