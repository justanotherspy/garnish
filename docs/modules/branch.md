# `branch`

Checked-out branch (or detached HEAD).

The current branch read from the repository without spawning git (in a reftable repository, whose refs are not files, the background worker asks git instead); a detached HEAD shows the short commit. The `full` preset adds the short SHA and a dirty marker (computed by the background worker).

**Sources:** `worktree.branch`, `.git/HEAD`, `git diff-index and diff-files (worker)`

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
| `hide` | list of `empty` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the branch icon. |
| `show_sha` | bool | `false` | `false` | `true` | Append the short commit SHA. |
| `dirty` | bool | `false` | `false` | `true` | Show a marker when tracked files have staged or unstaged changes (untracked files do not count; a file touched without changing counts until git next refreshes its index, since garnish never reads file contents). |
| `max_length` | integer | `40` | `40` | `40` | Cut the name itself to this many characters with `…` (`..` in the ascii set; 0 = no limit); the common `max_width` caps the whole module in cells instead. |
| `link` | bool | `false` | `false` | `false` | Link the name to the branch on the forge (`https://<host>/<owner>/<name>/tree/<branch>`, `/-/tree/` on GitLab), built from `workspace.repo` in the payload; nothing is linked without it or on a detached HEAD. GitLab is recognised by a host named after it or an open merge request, so a self-hosted GitLab on an unrelated host name links to `/tree/` until one is open. |

## Icons

`[modules.branch.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `branch` | `U+E725` | `⎇` | `🌿` | `on` | Branch icon. |
| `detached` | `U+F0C1` | `➦` | `📌` | `@` | Detached HEAD icon. |
| `dirty` | `U+F111` | `✱` | `✨` | `*` | Dirty marker. |

Also try (`branch`: `U+F126` `U+F418` `U+E702` `⎇` `⌥` `⑂`; `dirty`: `✱` `*` `+` `~`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.branch.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `name` | `text` | Branch name. |
| `sha` | `muted` | Short SHA. |
| `dirty` | `warn` | Dirty marker. |
