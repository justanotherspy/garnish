# `agent`

Agent name.

`agent.name` when Claude Code runs with `--agent` or agent settings. Hidden otherwise. The `full` preset adds a glyph when extended thinking is enabled.

**Sources:** `agent.name`, `thinking.enabled`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `security-reviewer` |
| `default` | `✪ security-reviewer` |
| `full` | `✪ security-reviewer ⋯` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` security-reviewer` |
| `unicode` | `✪ security-reviewer` |
| `emoji` | `👤 security-reviewer` |
| `ascii` | `agent: security-reviewer` |

## Options

`[modules.agent]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_thinking` | bool | `false` | `false` | `true` | Show the thinking glyph. |

## Icons

`[modules.agent.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `agent` | `U+F21B` | `✪` | `👤` | `agent:` | Agent icon. |
| `thinking` | `U+F0EB` | `⋯` | `💭` | `~` | Thinking glyph. |

## Colors

`[modules.agent.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | Agent name. |
| `thinking` | `accent2` | Thinking glyph. |
