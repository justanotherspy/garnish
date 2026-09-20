# `style`

Output style name.

Shows `output_style.name`. By default the `default` style is hidden; the `full` preset always shows it.

**Sources:** `output_style.name`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `concise` |
| `default` | `✎ concise` |
| `full` | `✎ concise` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` concise` |
| `unicode` | `✎ concise` |
| `emoji` | `🎨 concise` |
| `ascii` | `style: concise` |

## Options

`[modules.style]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `hide` | list of `empty` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `hide_default` | bool | `true` | `true` | `false` | Hide when the style is `default`. |
| `show_icon` | bool | `false` | `true` | `true` | Show the style icon. |

## Icons

`[modules.style.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `style` | `U+F1FC` | `✎` | `🎨` | `style:` | Style icon. |

Also try (`style`: `U+F040` `U+F031` `✎` `✏`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.style.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | Style name. |
