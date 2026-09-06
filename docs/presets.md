# Presets gallery

Complete configs from [`presets/`](../presets/). Copy one to `~/.config/garnish/garnish.toml`, point `GARNISH_CONFIG` at it, or write it with `garnish config init --preset <name>`; `garnish presets` lists them. Each sample is rendered at the preset's declared terminal width from the `subscription-full` payload with animations frozen at frame 0 (a ticker preset therefore shows the start of its scrolling window, not a cut); presets that need a Nerd Font show their glyphs as boxes here unless your browser has one. The fit holds for the icon set the preset declares (`# needs:`); with `--icons emoji` some glyphs are two cells and a tight layout may need a wider terminal. A real-terminal capture may accompany a preset as `presets/screenshots/<name>.png`.

| name | summary | columns | needs |
|---|---|---|---|
| [`animated-dots`](#animated-dots) | dots travelling along the rule, a pulsing separator and a cycling model icon | 100 | nerd-font |
| [`bars-and-limits`](#bars-and-limits) | 40-cell line-style context bar with window tag, mini bars on the limits | 130 | nerd-font |
| [`compact-aligned`](#compact-aligned) | two rounded lines with stacked bars, Catppuccin Mocha | 110 | nerd-font |
| [`dracula-256`](#dracula-256) | Dracula with role and per-module colour overrides in 256-colour mode | 130 | nerd-font |
| [`emoji-overrides`](#emoji-overrides) | emoji icons with per-module glyph overrides and name limits | 130 | emoji |
| [`full-aligned`](#full-aligned) | every module at full verbosity, columns aligned, fixed timers | 130 | nerd-font |
| [`labels-and-placeholders`](#labels-and-placeholders) | labels, brackets, dim – for absent modules, UTC clock with date | 170 | nerd-font |
| [`minimal-clean`](#minimal-clean) | one unframed line: path, context, limit, clock | 80 | nerd-font |
| [`motd-ticker`](#motd-ticker) | repo line plus a scrolling message of the day in a fixed 24-cell box | 100 | nerd-font |
| [`packed-heavy`](#packed-heavy) | custom heavy frame, left-packed lines, a separator per line | 130 | nerd-font |
| [`session-detail`](#session-detail) | session, api, cache and cost detail, plain stale style, 1 s git refresh | 130 | nerd-font |
| [`single-line-full`](#single-line-full) | everything on one row, always scrolling as a ticker (200 columns is a comfortable window) | 200 | nerd-font |
| [`tall-eight-lines`](#tall-eight-lines) | one module per row, eight rows, square frame | 100 | nerd-font |
| [`three-lines-double`](#three-lines-double) | repo / model / timers in a double frame | 130 | nerd-font |
| [`two-lines-powerline`](#two-lines-powerline) | location and model only, powerline caps, no colour | 110 | nerd-font |

## `animated-dots`

dots travelling along the rule, a pulsing separator and a cycling model icon

At 100 columns, needs nerd-font:

```text
╭─  ~/projects/garnish │  #42  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·   ⠋ 16:00:00 ─╮
╰─ Opus │  ▁▃▅▇█ │  ████████▍░░░░░░░░░░▏ 42% │  24%  2h13m ·  ·  ·  ·  ·    91% 1h  47m ─╯
```

<details><summary><code>presets/animated-dots.toml</code></summary>

```toml
# name: animated-dots
# summary: dots travelling along the rule, a pulsing separator and a cycling model icon
# columns: 100
# needs: nerd-font

# Everything that moves here is a pure function of the clock (SPEC § 4.2):
# the `·  ` pattern drifts toward the right cap one cell per second, the
# separator pulses through three weights, and the model icon cycles through
# four moon phases. Set `animate = false` (or `GARNISH_ANIMATE=0`) to freeze
# it all at frame 0.

preset = "compact"
icons  = "nerd"
theme  = "tokyonight"

[frame]
style            = "rounded"
fill_pattern     = "·  "
separator_frames = [" │ ", " ┃ ", " ╎ "]

[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]

[[line]]
modules = ["model", "effort", "context", "limit5h", "cost"]
right   = ["cache"]

[modules.model.icons]
model_frames = ["", "", "", ""]
```

</details>

## `bars-and-limits`

40-cell line-style context bar with window tag, mini bars on the limits

At 130 columns, needs nerd-font:

```text
╭─  ~/projects/garnish       │  ━━━━━━━━━━━━━━━━───────────────────────┃ 42% ⤓99% 1.0M ─────────────────────── ⠋ 16:00:00 ─╮
╰─  ██▊░░░░░░░░░ 24%  2h13m │  ████▉░░░░░░░ 41% ──────────────────────────────────────────────────────  +156 −23 (+133) ─╯
```

<details><summary><code>presets/bars-and-limits.toml</code></summary>

```toml
# name: bars-and-limits
# summary: 40-cell line-style context bar with window tag, mini bars on the limits
# columns: 130
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "rounded"

[[line]]
modules = ["path", "branch", "context"]
right   = ["clock"]

[[line]]
modules = ["limit5h", "limit7d", "spend", "cost"]
right   = ["lines"]

[modules.context]
width = 40                     # wide bar
show_window = true             # 1M / 200k tag
show_compaction_percent = true # print the threshold as a percentage too
thresholds = [40, 60, 80]      # colour bands move earlier
[modules.context.icons]
fill  = "━"                    # line-style bar: no fractional blocks, no font gaps
empty = "─"
marker = "┃"

[modules.limit5h]
preset = "full"
bar_width = 12

[modules.limit7d]
preset = "full"
bar_width = 12
show_reset = false

[modules.lines]
show_net = true
hide_zero = false
```

</details>

## `compact-aligned`

two rounded lines with stacked bars, Catppuccin Mocha

At 110 columns, needs nerd-font:

```text
╭─  ~/projects/garnish │  #42  ────────────────────────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─  Opus               │  ▁▃▅▇█ │  ████████▍░░░░░░░░░░▏ 42% │  24%  2h13m ──────  91% 1h  47m00s ─╯
```

<details><summary><code>presets/compact-aligned.toml</code></summary>

```toml
# name: compact-aligned
# summary: two rounded lines with stacked bars, Catppuccin Mocha
# columns: 110
# needs: nerd-font

preset = "compact"
icons  = "nerd"
theme  = "catppuccin-mocha"
color  = "auto"
align  = true
durations = "fixed"
```

</details>

## `dracula-256`

Dracula with role and per-module colour overrides in 256-colour mode

At 130 columns, needs nerd-font:

```text
┏━  ~/projects/garnish │  #42  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  garnish-dev │        ⠋ 16:00:00 ━┓
┗━  Opus               │  ▁▃▅▇█ │  ████████▍░░░░░░░░░░▏ 42% ━━━━━━━━━━  24%  2h13m │  41%  3d04h │  91% 1h  47m00s ━┛
```

<details><summary><code>presets/dracula-256.toml</code></summary>

```toml
# name: dracula-256
# summary: Dracula with role and per-module colour overrides in 256-colour mode
# columns: 130
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "dracula"
color  = "256"            # quantise every colour to the 256-colour cube
align  = true
durations = "fixed"

[colors]                  # role overrides: every module that uses the role follows
accent = "#ff79c6"        # dracula pink for all icons
frame  = "#6272a4"        # comment blue for the box
muted  = "gray"           # named colour
warn   = "yellow"         # named colour for the middle band / behind counts

[frame]
style = "heavy"

[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name", "clock"]

[[line]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "limit7d", "cache"]

[modules.path.colors]
base    = "#8be9fd"       # cyan base directory
subpath = "#bd93f9"       # purple below it

[modules.branch.colors]
icon = "#50fa7b"          # this icon ignores the accent override

[modules.context]
band_colors = ["#50fa7b", "#f1fa8c", "#ffb86c", "#ff5555"]
[modules.context.colors]
percent = "#f8f8f2"

[modules.clock.colors]
time = "214"              # 256-colour index (orange)
```

</details>

## `emoji-overrides`

emoji icons with per-module glyph overrides and name limits

At 130 columns, needs emoji:

```text
╭─ 📁 ~/projects/garnish    │ 🔀 #42 🕓 pending ───────────────────────────── 🔖 garnish-dev sess-000 │            16:00:00 ─╮
╰─ 🤖 Opus 💭 claude-opus-5 │ 🎯 ▁▃▅▇█ │ 🧠 ▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▏ 42% ─────────────────────── ⌚ 1h12m │ 💾 91% 1h 🔥 47m00s ─╯
```

<details><summary><code>presets/emoji-overrides.toml</code></summary>

```toml
# name: emoji-overrides
# summary: emoji icons with per-module glyph overrides and name limits
# columns: 130
# needs: emoji

preset = "default"
icons  = "emoji"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "rounded"

[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name", "clock"]

[[line]]
modules = ["model", "effort", "context"]
right   = ["session", "cache"]

[modules.branch]
max_length = 6            # a long feature branch gets cut with …

[modules.session_name]
max_length = 12
show_id = true

[modules.model]
show_id = true            # append the raw model id
show_thinking = true

[modules.pr]
show_state_word = true

[modules.branch.icons]
branch = "⎇"              # override just this glyph back to a text symbol

[modules.context.icons]
fill  = "▰"
empty = "▱"
marker = "▏"

[modules.clock.icons]
spinner = ""              # no spinner glyph at all
```

</details>

## `full-aligned`

every module at full verbosity, columns aligned, fixed timers

At 130 columns, needs nerd-font:

```text
╭─  ~/projects/garnish   │  #42  pending ────────────────────────────────────────────────────────  garnish-dev sess-000 ─╮
├─  Opus  claude-opus-5 │  ▁▃▅▇█ high  │  ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ │  default ────────────────────┤
├─  █▉░░░░░░ 24%  2h13m │  ███▎░░░░ 41%  3d04h ──────────────────────────────────────────────────────  +156 −23 (+133) ─┤
╰─  1h12m since 14:48    │  8m20s (12%) │  91% 1h  47m00s 2 misses 352kw ───────────────── ⠋ 16:00:00 Sat 01 Feb +00:00 ─╯
```

<details><summary><code>presets/full-aligned.toml</code></summary>

```toml
# name: full-aligned
# summary: every module at full verbosity, columns aligned, fixed timers
# columns: 130
# needs: nerd-font

preset = "full"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true             # pad each module column to its widest module so the │ bars stack
durations = "fixed"       # 9m00s / 1h05m instead of 9m / 1h5m, so timers keep their width
```

</details>

## `labels-and-placeholders`

labels, brackets, dim – for absent modules, UTC clock with date

At 170 columns, needs nerd-font:

```text
╭─ in  ~/projects/garnish │ –       │ pr  #42  ──────────────────────────────────────────────────────────────────────  garnish-dev │ 16:00:00 Sat 01 Feb +00:00 ─╮
╰─  Opus                  │  ▁▃▅▇█ │  ████████▍░░░░░░░░░░▏ 42% │ vim – ───────────────────────────────────── up  1h12m since 14:48 │          api  8m20s (12%) ─╯
```

<details><summary><code>presets/labels-and-placeholders.toml</code></summary>

```toml
# name: labels-and-placeholders
# summary: labels, brackets, dim – for absent modules, UTC clock with date
# columns: 170
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "rounded"

[[line]]
modules = ["path", "branch", "sync", "worktree", "pr"]
right   = ["session_name", "clock"]

[[line]]
modules = ["model", "effort", "context", "vim"]
right   = ["session", "api"]

[modules.path]
label = "in"              # label goes before the value
depth = 0                 # keep every path component
show_added = true

[modules.branch]
show_sha = true
dirty = true
prefix = "["
suffix = "]"

[modules.worktree]
hide_when_empty = false   # show a dim – instead of vanishing outside a worktree

[modules.pr]
hide_when_empty = false
label = "pr"

[modules.vim]
hide_when_empty = false
label = "vim"

[modules.clock]
tz = "UTC"
date = true
utc_offset = true
spinner = false

[modules.session]
label = "up"
show_start = true

[modules.api]
label = "api"
show_share = true
```

</details>

## `minimal-clean`

one unframed line: path, context, limit, clock

At 80 columns, needs nerd-font:

```text
~/garnish  42%  24%                                                    16:00
```

<details><summary><code>presets/minimal-clean.toml</code></summary>

```toml
# name: minimal-clean
# summary: one unframed line: path, context, limit, clock
# columns: 80
# needs: nerd-font

preset = "minimal"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
```

</details>

## `motd-ticker`

repo line plus a scrolling message of the day in a fixed 24-cell box

At 100 columns, needs nerd-font:

```text
╭─  ~/projects/garnish │  #42  ───────────────────── ship it before lunch, th │ ⠋ 16:00:00 ─╮
╰─  Opus │  ▁▃▅▇█ │  ████████▍░░░░░░░░░░▏ 42% │  24%  2h13m ─────────────  91% 1h  47m ─╯
```

<details><summary><code>presets/motd-ticker.toml</code></summary>

```toml
# name: motd-ticker
# summary: repo line plus a scrolling message of the day in a fixed 24-cell box
# columns: 100
# needs: nerd-font

# A text module (SPEC § 3.7) as a ticker: the message flows through a 24-cell
# box one cell per second and wraps around after ` · `; the second line is the
# usual model/context/limit row. Change `text` to whatever you want to keep in
# view.

preset = "compact"
icons  = "nerd"
theme  = "garnish"

[frame]
style = "rounded"

[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["text.motd", "clock"]

[[line]]
modules = ["model", "effort", "context", "limit5h", "cost"]
right   = ["cache"]

[modules.text.motd]
text     = "ship it before lunch, then write the docs"
width    = 24
overflow = "scroll-wrap"
gap      = " · "
color    = "accent2"
```

</details>

## `packed-heavy`

custom heavy frame, left-packed lines, a separator per line

At 130 columns, needs nerd-font:

```text
┏  ~/projects/garnish   garnish-dev  ⠋ 16:00:00
┃  Opus               ⋮  ▁▃▅▇█       ⋮  ████████▍░░░░░░░░░░▏ 42% ⋮  24%  2h13m ⋮  41%  3d04h
┗  1h12m              •  8m20s       •  91% 1h  47m00s          •  +156 −23
```

<details><summary><code>presets/packed-heavy.toml</code></summary>

```toml
# name: packed-heavy
# summary: custom heavy frame, left-packed lines, a separator per line
# columns: 130
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "custom"
fill  = false             # no rule to the right edge; lines are left-packed
first        = "┏"
middle       = "┃"
last         = "┗"
single       = "━"
fill_char    = " "
right_first  = ""
right_middle = ""
right_last   = ""
right_single = ""
pad          = " "
separator    = " ⋮ "        # default separator for lines that do not set one

[[line]]
modules = ["path", "branch", "sync"]
right   = ["session_name", "clock"]
separator = "  "          # this line: two spaces, no bar

[[line]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "limit7d"]

[[line]]
modules = ["session", "api", "cache"]
right   = ["lines"]
separator = " • "
```

</details>

## `session-detail`

session, api, cache and cost detail, plain stale style, 1 s git refresh

At 130 columns, needs nerd-font:

```text
╭─  ~/projects/garnish ─────────────────────────────────────────────────────────────────────────────────────────── ⠋ 16:00 ─╮
╰─  1h12m since 14:48 │  8m20s (12%) │  91% 1h  47m00s 2 misses 352kw ──────────────────  $1.234 +156 −23 │  +156 −23 ─╯
```

<details><summary><code>presets/session-detail.toml</code></summary>

```toml
# name: session-detail
# summary: session, api, cache and cost detail, plain stale style, 1 s git refresh
# columns: 130
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"
stale_style = "plain"     # dim | hide | plain: overdue values show unchanged
stale_after = 1           # style stale after one missed TTL (default 5)

[frame]
style = "rounded"

[[line]]
modules = ["path", "branch", "sync"]
right   = ["clock"]

[[line]]
modules = ["session", "api", "cache"]
right   = ["cost", "lines"]

[modules.branch]
refresh = 1               # worker every second (about 13 ms of CPU each)
show_sha = true

[modules.sync]
refresh = 1
show_zero = true          # show ⇡0 ⇣0 instead of hiding
show_upstream = true

[modules.session]
show_start = true

[modules.api]
show_share = true

[modules.cache]
show_ttl = true
show_countdown = true
show_misses = true
show_writes = true

[modules.cost]
only_without_rate_limits = false  # show even on a subscription
decimals = 3
show_lines = true

[modules.clock]
seconds = false
```

</details>

## `single-line-full`

everything on one row, always scrolling as a ticker (200 columns is a comfortable window)

At 200 columns, needs nerd-font:

```text
──  ~/projects/garnish │  #42  pending │  Opus  claude-opus-5 │  ▁▃▅▇█ high │  ████████████▌░░░░░░░░░░░░░░░░▏  ─  garnish-dev sess-000 │  +156 −23 (+133) │ ⠋ 16:00:00 Sat 01 Feb +00:00 ──
```

<details><summary><code>presets/single-line-full.toml</code></summary>

```toml
# name: single-line-full
# summary: everything on one row, always scrolling as a ticker (200 columns is a comfortable window)
# columns: 200
# needs: nerd-font

# The full-preset row is about 340 cells wide, more than any terminal, so
# instead of a cut the left group scrolls one cell per second and wraps around
# (SPEC § 4.1 `overflow = "ticker"`); the clock and the other right-hand
# modules stay put. The declared width is the window the sample is shown in,
# not a width at which the row fits. The ticker makes `durations` default to
# `fixed`, so the timers hold their width and the window slides.

preset = "full"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
overflow  = "ticker"

[frame]
style = "rounded"

[[line]]
modules = ["path", "branch", "sync", "worktree", "pr", "model", "effort", "context", "style", "limit5h", "limit7d", "spend", "cost", "session", "api", "cache"]
right   = ["session_name", "agent", "vim", "lines", "clock"]
```

</details>

## `tall-eight-lines`

one module per row, eight rows, square frame

At 100 columns, needs nerd-font:

```text
┌─  ~/projects/garnish ──────────────────────────────────────── ⠋ 16:00:00 Sat 01 Feb +00:00 ─┐
├─  #42  pending ───────────────────────────────────────────────────  garnish-dev sess-000 ─┤
├─  Opus  claude-opus-5 ─────────────────────────────────────────────────────────────────────┤
├─  ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ ──────────────────  █▉░░░░░░ 24%  2h13m ─┤
├─  1h12m since 14:48 ────────────────────────────────────────────────────────────────────────┤
└─  91% 1h  47m00s 2 misses 352kw ───────────────────────────────────────  +156 −23 (+133) ─┘
```

<details><summary><code>presets/tall-eight-lines.toml</code></summary>

```toml
# name: tall-eight-lines
# summary: one module per row, eight rows, square frame
# columns: 100
# needs: nerd-font

preset = "full"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"
truncate = true

[frame]
style = "square"

[[line]]
modules = ["path"]
right   = ["clock"]
[[line]]
modules = ["branch"]
[[line]]
modules = ["sync"]
[[line]]
modules = ["pr"]
right   = ["session_name"]
[[line]]
modules = ["model"]
[[line]]
modules = ["context"]
right   = ["limit5h"]
[[line]]
modules = ["session"]
[[line]]
modules = ["cache"]
right   = ["lines"]
```

</details>

## `three-lines-double`

repo / model / timers in a double frame

At 130 columns, needs nerd-font:

```text
╔═  ~/projects/garnish │  #42  ═══════════════════════════════════════════════════════════════════════════  garnish-dev ═╗
╠═  Opus               │  ▁▃▅▇█ │  ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ ════════  24%  2h13m │  41%  3d04h ═╣
╚═  1h12m              │  8m20s │  91% 1h  47m00s ══════════════════════════════════════════  +156 −23 │    ⠋ 16:00:00 ═╝
```

<details><summary><code>presets/three-lines-double.toml</code></summary>

```toml
# name: three-lines-double
# summary: repo / model / timers in a double frame
# columns: 130
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "catppuccin-mocha"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "double"

[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name"]

[[line]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "limit7d"]

[[line]]
modules = ["session", "api", "cache"]
right   = ["lines", "clock"]

[modules.context]
preset = "full"           # bar plus token counts and the compaction marker
width  = 30
```

</details>

## `two-lines-powerline`

location and model only, powerline caps, no colour

At 110 columns, needs nerd-font:

```text
  ~/projects/garnish   #42                                                garnish-dev  ⠋ 16:00:00 
  Opus                 ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼   24%  2h13m         1h12m 
```

<details><summary><code>presets/two-lines-powerline.toml</code></summary>

```toml
# name: two-lines-powerline
# summary: location and model only, powerline caps, no colour
# columns: 110
# needs: nerd-font

preset = "default"
icons  = "nerd"
theme  = "mono"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "powerline"
pad   = " "               # powerline ships with no space between the caps and the text

[[line]]
modules = ["path", "branch", "sync", "worktree", "pr"]
right   = ["session_name", "clock"]

[[line]]
modules = ["model", "context", "limit5h"]
right   = ["session", "cost"]

[modules.context]
preset = "full"
width  = 30
```

</details>
