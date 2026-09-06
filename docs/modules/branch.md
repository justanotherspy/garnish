# `branch`

Checked-out branch (or detached HEAD).

The current branch read from the repository without spawning git; a detached HEAD shows the short commit. The `full` preset adds the short SHA and a dirty marker (computed by the background worker).

**Sources:** `worktree.branch`, `.git/HEAD`, `git status (worker)`

**Refresh:** cached, refreshed in the background every 5 s

## Presets

| preset | render |
|---|---|
| `minimal` | `worktree-feature-x` |
| `default` | `⎇ worktree-feature-x` |
| `full` | `⎇ worktree-feature-x` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` worktree-feature-x` |
| `unicode` | `⎇ worktree-feature-x` |
| `emoji` | `🌿 worktree-feature-x` |
| `ascii` | `on worktree-feature-x` |

## Options

`[modules.branch]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `5` | `5` | `5` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the branch icon. |
| `show_sha` | bool | `false` | `false` | `true` | Append the short commit SHA. |
| `dirty` | bool | `false` | `false` | `true` | Show a marker when the tree has changes. |
| `max_length` | integer | `40` | `40` | `40` | Truncate longer names (0 = no limit). |

## Icons

`[modules.branch.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `branch` | `U+E725` | `⎇` | `🌿` | `on` | Branch icon. |
| `detached` | `U+F0C1` | `➦` | `📌` | `@` | Detached HEAD icon. |
| `dirty` | `U+F111` | `✱` | `✨` | `*` | Dirty marker. |

## Colors

`[modules.branch.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `name` | `text` | Branch name. |
| `sha` | `muted` | Short SHA. |
| `dirty` | `warn` | Dirty marker. |
