# `worktree`

Git worktree name.

Shown when the current directory is inside a linked git worktree (`workspace.git_worktree`) or the session entered a Claude Code worktree (`worktree.name`). The `full` preset adds the original branch.

**Sources:** `workspace.git_worktree`, `worktree.name`, `worktree.branch`, `worktree.original_branch`

**Refresh:** every tick (payload only)

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
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `show_original` | bool | `false` | `false` | `true` | Show `original → branch`. |

## Icons

`[modules.worktree.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `worktree` | `U+F126` | `⑂` | `🌳` | `wt:` | Worktree icon. |
| `arrow` | `→` | `➔` | `➡` | `->` | Original → branch arrow. |

## Colors

`[modules.worktree.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | Worktree name. |
| `original` | `muted` | Original branch. |
