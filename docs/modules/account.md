# `account`

The claude.ai account the session is signed in with.

The `oauthAccount.emailAddress` of `~/.claude.json` (`$CLAUDE_CONFIG_DIR/.claude.json` when that is set), the file Claude Code keeps for itself. It can be hundreds of KB, so a background worker reads it every `refresh` seconds and the tick shows the cached value: the first tick after a session starts shows nothing. A file without the field (an API-key session) shows nothing either; a file that cannot be read or parsed is a failed entry, marked `✗`.

**Sources:** `~/.claude.json oauthAccount.emailAddress (worker)`

**Refresh:** cached, refreshed in the background every 600 s

## Presets

| preset | render |
|---|---|
| `minimal` | — |
| `default` | — |
| `full` | — |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | — |
| `unicode` | — |
| `emoji` | — |
| `ascii` | — |

Nothing to show above (—): the worker reads `~/.claude.json`, which a sample never touches. Once it has, the module prints the sign-in's email address, or with `style = "user"` (the `minimal` preset) the part before the `@`.

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
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_icon` | bool | `false` | `true` | `true` | Show the icon. |
| `style` | `email` \| `user` | `"user"` | `"email"` | `"email"` | `email` shows the whole address; `user` the part before `@`. |

## Icons

`[modules.account.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `account` | `U+F007` | `@` | `👤` | `@` | Account icon. |

Also try (`account`: `U+F007` `U+F2BD` `U+F2C0` `@` `&`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.account.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `icon` | `accent2` | Icon. |
| `name` | `text` | The address or user. |
