# `account`

The claude.ai account the session is signed in with.

The `oauthAccount.emailAddress` of `~/.claude.json` (`$CLAUDE_CONFIG_DIR/.claude.json` when that is set), the file Claude Code keeps for itself. It can be hundreds of KB, so a background worker reads it every `refresh` seconds and the tick shows the cached value: the first tick after a session starts shows nothing. A file without the field (an API-key session) shows nothing either; a file that cannot be read or parsed is a failed entry, marked `✗`.

**Sources:** `~/.claude.json oauthAccount.emailAddress (worker)`

**Refresh:** cached, refreshed in the background every 600 s

## Presets

| preset | render |
|---|---|
| `minimal` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |
| `default` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |
| `full` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |
| `unicode` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |
| `emoji` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |
| `ascii` | `(shown once its worker has read ~/.claude.json, e.g. `@ dev@example.com`)` |

## Options

`[modules.account]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer ≥ 1 | `600` | `600` | `600` | Seconds a cached value lives before a background worker refreshes it. |
| `hide` | list of `empty` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`, `-` in the ascii set). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `style` | `email` \| `user` | `"user"` | `"email"` | `"email"` | `email` shows the whole address; `user` the part before `@`. |

## Icons

`[modules.account.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `account` | `U+F007` | `@` | `👤` | `@` | Account icon. |

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.account.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | The address or user. |
