# `text.<name>`

Static text in a box of fixed width; define any number as `[modules.text.<name>]`.

A fixed string in a box, placed on a line as `text.<name>`. `width = 0` makes the box as wide as the text; otherwise the box is `width` cells with `pad` blank cells on each side, `justify` places shorter text in it, and `overflow` decides what happens to longer text: `clip` cuts it with an ellipsis, `scroll` slides a window over it and restarts after the end has passed, `scroll-wrap` is a ticker that flows continuously with `gap` between the end and the start. Scrolling is a pure function of the clock (`floor(now × step) mod period`), so nothing is stored between ticks and `GARNISH_ANIMATE=0` freezes it. The text is plain: escape sequences and control characters are stripped. Text modules have no `preset` and no `refresh`.

**Sources:** the config file only. **Refresh:** every tick; nothing to cache.

## Example

```toml
[[line]]
modules = ["text.motd", "text.clip", "text.tag"]
[modules.text.motd]
text = "ship it before lunch, then write the docs"
width = 12
overflow = "scroll-wrap"
gap = " · "
[modules.text.clip]
text = "a rather long note"
width = 8
overflow = "clip"
[modules.text.tag]
text = "v0.2"
width = 8
justify = "right"
pad = 1
color = "muted"
```

renders (frame 0; the first box scrolls in a live session) as

```text
ship it befo  a rathe…       v0.2
```

## Options

`[modules.text.<name>]`

| key | type | default | description |
|---|---|---|---|
| `enabled` | bool | `true` | Render this module. |
| `label` | string ≤ 4096 chars | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `text` | string ≤ 4096 chars | `""` | The text. ANSI/OSC sequences and control characters are stripped. |
| `width` | integer ≤ 1024 | `0` | Box width in cells; 0 = the text's own width. |
| `pad` | integer ≤ 1024 | `0` | Blank cells added on each side of the box. |
| `justify` | `left` \| `right` \| `center` | `"left"` | Where text narrower than the box sits. |
| `overflow` | `clip` \| `scroll` \| `scroll-wrap` | `"scroll"` | Text wider than the box: `clip` cuts with an ellipsis, `scroll` slides a window and restarts after the end, `scroll-wrap` flows continuously with `gap` between end and start. |
| `step` | number | `1` | Cells scrolled per tick (0.001–1000; 0.5 = every second tick). |
| `gap` | string ≤ 4096 chars | `"   "` | `scroll-wrap` only: text between the end and the start. |
| `url` | string ≤ 4096 chars | `""` | Wrap the box in a clickable OSC 8 link to this `http(s)://` URL (printable ASCII only; anything else is reported and dropped). |

No `preset`, no `refresh` and no `max_width` (the box is sized by `width`): a text module renders every tick as configured.

## Colors

`[modules.text.<name>.colors]`, or the shorthand `color = …` on the module (an explicit `colors.text` wins over the shorthand). A module name is letters, digits, `_` and `-` only, so `text.<name>` reads the same on a line and in `config show`.

| key | default | description |
|---|---|---|
| `text` | `accent` | The text. |
