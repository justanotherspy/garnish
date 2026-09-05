# `vim`

Vim mode badge.

`vim.mode` when vim mode is enabled (`NORMAL`, `INSERT`, `VISUAL`, `VISUAL LINE`). Set `hideVimModeIndicator = true` in the `statusLine` settings so the mode is not shown twice.

**Sources:** `vim.mode`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `I` |
| `default` | `INSERT` |
| `full` | `INSERT` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | `INSERT` |
| `unicode` | `INSERT` |
| `emoji` | `INSERT` |
| `ascii` | `INSERT` |

## Options

`[modules.vim]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `style` | `badge` \| `short` | `"short"` | `"badge"` | `"badge"` | Full word or one letter. |
| `show_icon` | bool | `false` | `false` | `true` | Show the vim icon. |

## Icons

`[modules.vim.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `vim` | `U+E62B` | `` | `` | `` | Vim icon. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.vim.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `normal` | `accent` | NORMAL mode. |
| `insert` | `ok` | INSERT mode. |
| `visual` | `warn` | VISUAL modes. |
