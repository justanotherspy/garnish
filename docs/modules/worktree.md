# `worktree`

Git worktree name.

Shown when the current directory is inside a linked git worktree (`workspace.git_worktree`) or the session entered a Claude Code worktree (`worktree.name`). The `full` preset adds the original branch.

**Sources:** `workspace.git_worktree`, `worktree.name`, `worktree.branch`, `worktree.original_branch`

**Refresh:** every tick, nothing cached

## Presets

| preset | render |
|---|---|
| `minimal` | `feature-x` |
| `default` | `⑂ feature-x` |
| `full` | `⑂ feature-x main ➔ worktree-feature-x` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` feature-x` |
| `unicode` | `⑂ feature-x` |
| `emoji` | `🌳 feature-x` |
| `ascii` | `wt: feature-x` |

## Options

`[modules.worktree]`

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
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_original` | bool | `false` | `false` | `true` | Show `original → branch`. |

## Icons

`[modules.worktree.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `worktree` | `U+F126` | `⑂` | `🌳` | `wt:` | Worktree icon. |
| `arrow` | `U+F178` | `➔` | `➡` | `->` | Original → branch arrow. |

Also try (`worktree`: `U+F0E8` `U+F1BB` `U+F402` `⌂`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.worktree.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | Worktree name. |
| `original` | `muted` | Original branch. |
