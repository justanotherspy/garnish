# `model`

Model name, with fast-mode and thinking indicators.

Shows `model.display_name`. A bolt appears when fast mode is on; the `full` preset adds the raw model id and a thinking glyph when extended thinking is enabled.

**Sources:** `model.display_name`, `model.id`, `fast_mode`, `thinking.enabled`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `Opus` |
| `default` | `❖ Opus` |
| `full` | `❖ Opus ⋯ claude-opus-5` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` Opus` |
| `unicode` | `❖ Opus` |
| `emoji` | `🤖 Opus` |
| `ascii` | `Opus` |

## Options

`[modules.model]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the model icon. |
| `show_id` | bool | `false` | `false` | `true` | Append the raw model id. |
| `show_fast` | bool | `true` | `true` | `true` | Show a bolt when fast mode is on. |
| `show_thinking` | bool | `false` | `false` | `true` | Show a glyph when extended thinking is enabled. |

## Icons

`[modules.model.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `model` | `U+EB08` | `❖` | `🤖` | `` | Model icon. |
| `fast` | `U+F0E7` | `⚡` | `⚡` | `!` | Fast mode. |
| `thinking` | `U+F0EB` | `⋯` | `💭` | `~` | Extended thinking. |

## Colors

`[modules.model.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `name` | `text` | Model name. |
| `id` | `muted` | Model id. |
| `fast` | `warn` | Fast-mode bolt. |
| `thinking` | `accent2` | Thinking glyph. |
