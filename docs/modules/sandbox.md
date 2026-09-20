# `sandbox`

A badge while Bash sandboxing is on.

Shows while `sandbox.enabled` is `true` in Claude Code's settings chain (the first file that sets it wins, as for `prefersReducedMotion`): Bash commands run isolated from the filesystem and the network. Nothing otherwise, so `hide_when_empty = false` prints `–` as it does for any absent value.

**Sources:** `.claude/settings.json sandbox.enabled (the settings chain)`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `⊡` |
| `default` | `⊡` |
| `full` | `⊡ sandbox` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | `` |
| `unicode` | `⊡` |
| `emoji` | `🔒` |
| `ascii` | `[]` |

## Options

`[modules.sandbox]`

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
| `show_icon` | bool | `true` | `true` | `true` | Show the icon. |
| `style` | `glyph` \| `word` | `"glyph"` | `"glyph"` | `"word"` | `glyph` shows the icon alone; `word` adds the module's name after it. |

## Icons

`[modules.sandbox.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `sandbox` | `U+F023` | `⊡` | `🔒` | `[]` | Sandbox glyph. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.sandbox.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `word` | `text` | The word, under `style = "word"`. |
