# `path`

Working directory, based on the repository root.

The base directory is the git top level when inside a repository, otherwise `workspace.project_dir`. When the current directory is deeper than the base, the extra path is shown dimmed. The `full` preset shows the whole tilde-collapsed path and the number of `/add-dir` directories.

**Sources:** `workspace.project_dir`, `workspace.current_dir`, `workspace.added_dirs`, `git top level`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `~/garnish` |
| `default` | `❒ ~/projects/garnish` |
| `full` | `❒ ~/projects/garnish` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` ~/projects/garnish` |
| `unicode` | `❒ ~/projects/garnish` |
| `emoji` | `📁 ~/projects/garnish` |
| `ascii` | `~/projects/garnish` |

## Options

`[modules.path]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the folder icon. |
| `depth` | integer | `1` | `2` | `0` | Path components of the base to keep (0 = all). |
| `style` | `full` \| `fish` | `"full"` | `"full"` | `"full"` | How the base prints: `full` as is; `fish` abbreviates every directory but the last to its first character (`~/p/garnish`; a dot-directory to `.c`), as the fish shell prompts. `depth` applies first; the subpath is untouched. |
| `show_subpath` | bool | `false` | `true` | `true` | Show the path below the base. |
| `show_added` | bool | `false` | `false` | `true` | Show the count of added directories. |

## Icons

`[modules.path.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `folder` | `U+F07B` | `❒` | `📁` | `` | Folder icon. |
| `added` | `U+F067` | `+` | `➕` | `+` | Added-directories glyph. |

Also try (`folder`: `U+F07C` `U+F413` `U+F015` `❒` `❏`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.path.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `base` | `text` | Base directory. |
| `subpath` | `muted` | Path below the base. |
| `added` | `muted` | Added directories. |
