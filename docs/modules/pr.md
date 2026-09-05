# `pr`

Open pull/merge request with review state, linked.

The open PR (or GitLab MR) Claude Code found for the current branch, as a clickable OSC 8 link with a glyph for the review state: approved, pending, changes requested, or draft. Hidden when there is none. No network calls: the harness supplies the data.

**Sources:** `pr.number`, `pr.url`, `pr.review_state`, `pr.kind`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `#42` |
| `default` | `⇄ #42 ✓` |
| `full` | `⇄ #42 ✓ approved` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | ` #42 ✓` |
| `unicode` | `⇄ #42 ✓` |
| `emoji` | `🔀 #42 ✅` |
| `ascii` | `PR #42 ok` |

## Options

`[modules.pr]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `label` | string | `""` | `""` | `""` | Dim text before the value. |
| `prefix` / `suffix` | string | `""` | `""` | `""` | Text around the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `show_icon` | bool | `false` | `true` | `true` | Show the PR icon. |
| `show_state` | bool | `false` | `true` | `true` | Show the review-state glyph. |
| `show_state_word` | bool | `false` | `false` | `true` | Show the review state as a word. |
| `link` | bool | `true` | `true` | `true` | Make the number a clickable link. |

## Icons

`[modules.pr.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `pr` | `U+F407` | `⇄` | `🔀` | `PR` | Pull request icon. |
| `mr` | `U+F407` | `⇄` | `🔀` | `MR` | Merge request icon. |
| `approved` | `✓` | `✓` | `✅` | `ok` | Approved. |
| `pending` | `U+F10C` | `❍` | `🕓` | `..` | Pending review. |
| `changes_requested` | `✗` | `✗` | `❌` | `xx` | Changes requested. |
| `draft` | `U+F192` | `❏` | `🚧` | `wip` | Draft. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.pr.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent` | Icon. |
| `number` | `text` | PR number. |
| `approved` | `ok` | Approved. |
| `pending` | `warn` | Pending. |
| `changes_requested` | `danger` | Changes requested. |
| `draft` | `muted` | Draft. |
