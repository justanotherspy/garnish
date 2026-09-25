# `sync`

Commits ahead/behind the upstream branch.

Ahead/behind counts against `@{upstream}` using the remote-tracking refs already on disk (no network). The `full` preset names the upstream and hints how long ago the last fetch happened; `fetch_interval` opts into a background `git fetch`.

**Sources:** `git rev-list --left-right --count (worker)`, `.git/FETCH_HEAD age`

**Refresh:** cached, refreshed in the background every 5 s

## Presets

| preset | render |
|---|---|
| `minimal` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |
| `default` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |
| `full` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |
| `unicode` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |
| `emoji` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |
| `ascii` | `(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)` |

## Options

`[modules.sync]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer ≥ 1 | `5` | `5` | `5` | Seconds a cached value lives before a background worker refreshes it. |
| `hide` | list of `empty`, `zero` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine; `zero` when the count is zero. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`, `-` in the ascii set). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `show_zero` | bool | `false` | `false` | `false` | Show `0` counts instead of hiding them. |
| `show_upstream` | bool | `false` | `false` | `true` | Show the upstream name. |
| `fetch_age` | bool | `false` | `true` | `true` | Hint when the last fetch is older than `fetch_stale_minutes`. |
| `fetch_stale_minutes` | integer | `30` | `30` | `30` | Age after which the fetch hint appears. |
| `fetch_interval` | integer | `0` | `0` | `0` | Run `git fetch` in the background every N seconds (0 = never). |
| `durations` | `inherit` \| `compact` \| `fixed` | `"inherit"` | `"inherit"` | `"inherit"` | How this module's timers and countdowns print: `inherit` follows the top-level `durations`; `compact` or `fixed` pins this module. |

## Icons

`[modules.sync.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `ahead` | `⇡` | `⇡` | `🔼` | `^` | Ahead glyph. |
| `behind` | `⇣` | `⇣` | `🔽` | `v` | Behind glyph. |
| `stale` | `U+F017` | `↻` | `⌛` | `?` | Stale-fetch glyph. |
| `no_upstream` | `U+F127` | `⊘` | `🚫` | `-` | No-upstream glyph. |

Also try (`ahead`: `⇡` `⇈` `^`; `behind`: `⇣` `⇊` `v`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.sync.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `ahead` | `ok` | Ahead count. |
| `behind` | `warn` | Behind count. |
| `upstream` | `muted` | Upstream name. |
| `stale` | `muted` | Fetch-age hint. |
