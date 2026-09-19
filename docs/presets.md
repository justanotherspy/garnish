# Presets gallery

Complete configs from [`presets/`](../presets/). Copy one to `~/.config/garnish/garnish.toml`, point `GARNISH_CONFIG` at it, or write it with `garnish config init --preset <name>`; `garnish presets` lists them. Each sample is rendered at the preset's declared terminal width from the `subscription-full` payload with animations frozen at frame 0 (a ticker preset therefore shows its row cut with `…`, as it looks with animations off; in a live session it scrolls); presets that need a Nerd Font show their glyphs as boxes here unless your browser has one. The fit holds for the icon set the preset declares (`# needs:`); with `--icons emoji` some glyphs are two cells and a tight layout may need a wider terminal. A real-terminal capture may accompany a preset as `presets/screenshots/<name>.png`.

| name | summary | columns | needs |
|---|---|---|---|
| [`animated-dots`](#animated-dots) | dots travelling along the rule, a pulsing separator and a cycling model icon | 100 | nerd-font |
| [`ascii-only`](#ascii-only) | 7-bit ASCII throughout: ascii icons, a custom +-| frame, no colour codes at all | 100 | — |
| [`bars-and-limits`](#bars-and-limits) | 40-cell line-style context bar with window tag, mini bars on the limits | 130 | nerd-font |
| [`boxed-panels`](#boxed-panels) | two titled boxes, one around the repo rows and one around usage | 120 | nerd-font |
| [`compact-aligned`](#compact-aligned) | two rounded lines with stacked bars, Catppuccin Mocha | 110 | nerd-font |
| [`compaction-watch`](#compaction-watch) | the context bar measured to the compaction point, resets as clock times, line-style bars | 120 | nerd-font |
| [`dashboard-panels`](#dashboard-panels) | a full-height repo box, a centred column, and three stacked panels | 150 | nerd-font |
| [`dracula-256`](#dracula-256) | Dracula with role and per-module colour overrides in 256-colour mode | 130 | nerd-font |
| [`emoji-overrides`](#emoji-overrides) | emoji icons with per-module glyph overrides and name limits | 130 | emoji |
| [`full-aligned`](#full-aligned) | every module at full verbosity, columns aligned, fixed timers | 130 | nerd-font |
| [`grid-six`](#grid-six) | six equal columns, one module each, over a full-width flex row | 170 | nerd-font |
| [`grid-three`](#grid-three) | three columns side by side — repo, model, usage — reading left, centre, right | 140 | nerd-font |
| [`labels-and-placeholders`](#labels-and-placeholders) | labels, brackets, dim – for absent modules, UTC clock with date | 170 | nerd-font |
| [`links-and-shortcuts`](#links-and-shortcuts) | clickable branch and pull request, a fish-style path, two link buttons in fixed boxes | 110 | nerd-font |
| [`minimal-clean`](#minimal-clean) | one unframed line: path, context, limit, clock | 80 | nerd-font |
| [`motd-ticker`](#motd-ticker) | repo line plus a scrolling message of the day in a fixed 24-cell box | 100 | nerd-font |
| [`narrow-unicode`](#narrow-unicode) | three short unframed rows for a 72-column pane, no Nerd Font needed, capped modules | 72 | — |
| [`pace-and-eta`](#pace-and-eta) | the two rate-limit windows with pace against the clock, a projected time to 100 %, and the time cursor on their bars | 130 | nerd-font |
| [`packed-heavy`](#packed-heavy) | custom heavy frame, left-packed rows, a separator per row | 130 | nerd-font |
| [`precise-numbers`](#precise-numbers) | every number at full precision: token counts with thousands separators, percentages to a decimal, whole dollars, dimmed details | 120 | — |
| [`quiet-when-idle`](#quiet-when-idle) | modules that leave the row while there is nothing worth reading: zero counts, a near-empty context, an absent cost | 110 | — |
| [`session-badges`](#session-badges) | the harness itself beside the model: its version, a sandbox and a voice badge, the signed-in account, separators in the colour of what precedes them | 100 | nerd-font |
| [`session-detail`](#session-detail) | session, api, cache and cost detail, plain stale style, 1 s git refresh | 130 | nerd-font |
| [`sidebar-panels`](#sidebar-panels) | a 34-cell boxed sidebar, a two-share stack of titled rows, and a bottom-aligned column | 140 | nerd-font |
| [`single-line-full`](#single-line-full) | everything on one row, always scrolling as a ticker (200 columns is a comfortable window) | 200 | nerd-font |
| [`slow-motion`](#slow-motion) | half-speed rule, separator and note; a pulsing bar and an eight-dot spinner at full speed | 100 | nerd-font |
| [`still-life`](#still-life) | nothing moves: animation off, no spinner or seconds, fixed-width timers, plain stale values | 110 | nerd-font |
| [`tall-eight-lines`](#tall-eight-lines) | one module per row, eight rows, square frame | 100 | nerd-font |
| [`three-lines-double`](#three-lines-double) | repo / model / timers in a double frame | 130 | nerd-font |
| [`ticker-two-step`](#ticker-two-step) | a long first row scrolling two cells a tick behind a still clock, a second row that fits, 90 columns | 90 | nerd-font |
| [`titled-sections`](#titled-sections) | a title in every rule, left, centred and right, a titled spacer and one boxed row, Nord | 120 | nerd-font |
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

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]

[[row]]
modules = ["model", "effort", "context", "limit5h", "cost"]
right   = ["cache"]

[modules.model.icons]
model_frames = ["", "", "", ""]
```

</details>

## `ascii-only`

7-bit ASCII throughout: ascii icons, a custom +-| frame, no colour codes at all

At 100 columns:

```text
+- ~/projects/garnish | PR #42 .. ----------------------------------------------- garnish-dev -+
|- Opus               | .:=+# | ctx: ======---------| 42% ------------------------ | 16:00:00 -|
+- 5h 24% reset 2h13m | 7d 41% reset 3d04h ------------------------------ t: 1h12m | +156 -23 -+
```

<details><summary><code>presets/ascii-only.toml</code></summary>

```toml
# name: ascii-only
# summary: 7-bit ASCII throughout: ascii icons, a custom +-| frame, no colour codes at all
# columns: 100

# For a terminal or a log with no fonts and no colour: the ascii icon set,
# `color = "never"`, and a custom frame drawn from `+`, `-` and `|`. Every
# cut ends in `..` rather than `…`, so nothing outside ASCII ever reaches
# the row.

preset = "default"
icons  = "ascii"
theme  = "mono"
color  = "never"
align  = true
durations = "fixed"

[frame]
style        = "custom"
first        = "+-"
middle       = "|-"
last         = "+-"
single       = "--"
right_first  = "-+"
right_middle = "-|"
right_last   = "-+"
right_single = "--"
fill_char    = "-"
pad          = " "
separator    = " | "

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name"]

[[row]]
modules = ["model", "effort", "context"]
right   = ["clock"]

[[row]]
modules = ["limit5h", "limit7d"]
right   = ["session", "lines"]

[modules.context]
bar = "line"
width = 16
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

[[row]]
modules = ["path", "branch", "context"]
right   = ["clock"]

[[row]]
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

## `boxed-panels`

two titled boxes, one around the repo rows and one around usage

At 120 columns, needs nerd-font:

```text
╔═ Repository ═════════════════════════════════════════════════════════════════════════════════════════════════════╗
║  ~/projects/garnish │  #42                                                                       garnish-dev ║
║  Opus │  ▁▃▅▇█                                                                                                 ║
╚══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝
╭───────────────────────────────────────────────────── Usage ──────────────────────────────────────────────────────╮
│  ████████▍░░░░░░░░░░▏ 42% │  24%  2h13m │  41%  3d04h                                                       │
│                                                                                                        +156 −23 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
──  1h12m │  8m20s │  91% 1h  47m00s ───────────────────────────────────────────────────────────── ⠋ 16:00:00 ──
```

<details><summary><code>presets/boxed-panels.toml</code></summary>

```toml
# name: boxed-panels
# summary: two titled boxes, one around the repo rows and one around usage
# columns: 120
# needs: nerd-font

# Adjacent rows naming the same box form one box (SPEC § 4.3), drawn with its
# own corners and sides in place of the frame's caps. `fill` is off inside a
# box, so the interior is clean; the row outside them keeps the frame.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
durations = "fixed"

[frame]
style = "rounded"

[box.repo]
title = "Repository"
style = "double"

[box.usage]
title = "Usage"
title_justify = "center"

[[row]]
box = "repo"
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name"]

[[row]]
box = "repo"
modules = ["worktree", "model", "effort"]

[[row]]
box = "usage"
modules = ["context", "limit5h", "limit7d"]

[[row]]
box = "usage"
modules = ["spend", "cost"]
right   = ["lines"]

[[row]]
modules = ["session", "api", "cache"]
right   = ["clock"]

[modules.context]
width = 20
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

## `compaction-watch`

the context bar measured to the compaction point, resets as clock times, line-style bars

At 120 columns, needs nerd-font:

```text
╭─  ~/projects/garnish ───────────────────────────────────────────────────────────────────  Opus │  ▁▃▅▇█ high ─╮
├─  ━━━━━━━━━━━━────────────────── 43% 1.0M ‼  ────────────────────────────────────────────────── ⠋ 04:00:00 PM ─┤
╰─  ━─────── 24%  18:13 │  ━━━───── 41%  3d04h (Tue 20:00) ────────────────────────────────────────────────────╯
```

<details><summary><code>presets/compaction-watch.toml</code></summary>

```toml
# name: compaction-watch
# summary: the context bar measured to the compaction point, resets as clock times, line-style bars
# columns: 120
# needs: nerd-font

# `scale = "usable"` makes 100 % the auto-compaction threshold (SPEC § 3.2),
# so the bar and its percentage say how close compaction is; the window tag
# still names the real window, and a badge appears from `warn_at` up. The
# limit windows print when they reset as a wall-clock time rather than a
# countdown, and every bar is line-style, whole cells only.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true

[frame]
style = "rounded"

[[row]]
modules = ["path", "branch"]
right   = ["model", "effort"]

[[row]]
modules = ["context"]
right   = ["clock"]

[[row]]
modules = ["limit5h", "limit7d", "spend"]
right   = ["cost"]

[modules.context]
scale = "usable"
width = 30
bar = "line"
show_window = true
exceeds_200k = true
warn_at = 40
thresholds = [40, 60, 80]
band_colors = ["ok", "warn", "hot", "danger"]

[modules.limit5h]
reset = "absolute"        # ⏱ 14:30 instead of a countdown
bar_width = 8
bar = "line"

[modules.limit7d]
reset = "both"            # 3d4h (Tue 14:30)
bar_width = 8
bar = "line"
durations = "fixed"

[modules.spend]
reset = "absolute"        # ⏱ Mar 1: weeks away, so the date
bar_width = 8
bar = "line"

[modules.effort]
style = "both"

[modules.clock]
format = "12h"
```

</details>

## `dashboard-panels`

a full-height repo box, a centred column, and three stacked panels

At 150 columns, needs nerd-font:

```text
╔═ Repository ═════════════════════════════════════════════════╗                   ╭─────────────────────────────────────────────────────────────╮
║  ~/projects/garnish                                         ║                   │                  ████████▍░░░░░░░░░░▏ 42%                  │
║  #42                                                       ║                   ╰─────────────────────────────────────────────────────────────╯
║                                                              ║                   ╭─────────────────────────────────────────────────────────────╮
║                                                              ║   Opus   ▁▃▅▇█  │                         24%  2h13m                        │
║                                                              ║                   ╰─────────────────────────────────────────────────────────────╯
║                                                              ║                   ╭─────────────────────────────────────────────────────────────╮
║                                                              ║                   │                         ⠋ 16:00:00                          │
╚══════════════════════════════════════════════════════════════╝                   ╰─────────────────────────────────────────────────────────────╯
```

<details><summary><code>presets/dashboard-panels.toml</code></summary>

```toml
# name: dashboard-panels
# summary: a full-height repo box, a centred column, and three stacked panels
# columns: 150
# needs: nerd-font

# The SPEC § 4.3 dashboard: one row, three columns. The first is a box the
# row's full height over a stack of two rows, the second a bare centred
# column, the third a stack of three boxes. The frame has no box shape, so an
# unstyled box is drawn rounded.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
durations = "fixed"

[frame]
style = "none"

[box.repo]
title = "Repository"
style = "double"

[[row]]
gap = 2

[[row.col]]
box = "repo"
[[row.col.row]]
modules = ["path", "branch"]
[[row.col.row]]
modules = ["sync", "pr"]

[[row.col]]
width = "auto"
justify = "center"
valign = "center"
[[row.col.row]]
modules = ["model", "effort"]

[[row.col]]
justify = "center"
[[row.col.row]]
box = true
modules = ["context"]
[[row.col.row]]
box = true
modules = ["limit5h"]
[[row.col.row]]
box = true
modules = ["cost", "clock"]

[modules.context]
width = 20
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

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name", "clock"]

[[row]]
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

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name", "clock"]

[[row]]
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

## `grid-six`

six equal columns, one module each, over a full-width flex row

At 170 columns, needs nerd-font:

```text
╭─  ~/projects/garnish ──────────────────────────────────────────  Opus ──────────────  ████▏░░░░▏ 42% ──────────  24%  2h13m ───────────────────── ⠋ 16:00:00 ─╮
╰─  1h12m │  8m20s │  91% 1h  47m00s ───────────────────────────────────────────────────────────────────────────────────────────────────────────────  +156 −23 ─╯
```

<details><summary><code>presets/grid-six.toml</code></summary>

```toml
# name: grid-six
# summary: six equal columns, one module each, over a full-width flex row
# columns: 170
# needs: nerd-font

# Six `1fr` columns share the width equally (the leftover cells go one each to
# the first of them, so the shares differ by at most one), and each holds a
# single module. `durations = "fixed"` keeps the timers from changing width as
# they tick, which is what keeps a grid from shuffling.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
durations = "fixed"

[frame]
style = "rounded"

[[row]]
[[row.col]]
modules = ["path"]
[[row.col]]
modules = ["branch"]
[[row.col]]
modules = ["model"]
[[row.col]]
modules = ["context"]
[[row.col]]
modules = ["limit5h"]
[[row.col]]
modules = ["clock"]

[[row]]
modules = ["session", "api", "cache"]
right   = ["lines"]

[modules.path]
max_width = 20

[modules.context]
width = 10
```

</details>

## `grid-three`

three columns side by side — repo, model, usage — reading left, centre, right

At 140 columns, needs nerd-font:

```text
╭─  ~/projects/garnish ───────────────────────────────────  Opus │  ▁▃▅▇█ ─────────────────────────────  ████████▍░░░░░░░░░░▏ 42% ─╮
╰─  24%  2h13m │  41%  3d04h ───────────────────────────────────────────────────────────────────────────────────────── ⠋ 16:00:00 ─╯
```

<details><summary><code>presets/grid-three.toml</code></summary>

```toml
# name: grid-three
# summary: three columns side by side — repo, model, usage — reading left, centre, right
# columns: 140
# needs: nerd-font

# A row is columns side by side (SPEC § 4.3). With no `justify` anywhere, the
# first column reads left, the last right and the middle one centre, so the
# three groups sit where you would put them by hand. The second row is the
# plain form, which is one column filling the width.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
durations = "fixed"

[frame]
style = "rounded"

[[row]]
gap = 2
[[row.col]]
modules = ["path", "branch", "sync"]
[[row.col]]
modules = ["model", "effort"]
[[row.col]]
modules = ["context"]

[[row]]
modules = ["limit5h", "limit7d", "cost"]
right   = ["clock"]

[modules.context]
width = 20
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

[[row]]
modules = ["path", "branch", "sync", "worktree", "pr"]
right   = ["session_name", "clock"]

[[row]]
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

## `links-and-shortcuts`

clickable branch and pull request, a fish-style path, two link buttons in fixed boxes

At 110 columns, needs nerd-font:

```text
╭─  ~/p/garnish │  #42  pending ───────────────────────────────────────   docs │ issues │ ⠋ 16:00:00 ─╮
╰─  Opus │  ████████▍░░░░░░░░░░▏ 42% │  24%  2h13m ───────────────────────  1h12m │  91% 1h  47m ─╯
```

<details><summary><code>presets/links-and-shortcuts.toml</code></summary>

```toml
# name: links-and-shortcuts
# summary: clickable branch and pull request, a fish-style path, two link buttons in fixed boxes
# columns: 110
# needs: nerd-font

# OSC 8 links, in a terminal that draws them (SPEC § 4.1): `branch.link`
# opens the branch on the forge, the pull request number opens its page,
# and a text module with `url` is a button. The buttons are boxes of a
# fixed `width` with the text right-justified, so the row keeps its shape
# whatever they say; the path is abbreviated the way the fish shell
# prompts.

preset = "compact"
icons  = "nerd"
theme  = "garnish"
color  = "auto"

[frame]
style = "rounded"

[[row]]
modules = ["path", "branch", "pr"]
right   = ["text.docs", "text.issues", "clock"]

[[row]]
modules = ["model", "context", "limit5h"]
right   = ["session", "cache"]

[modules.path]
style = "fish"            # ~/p/garnish
depth = 3

[modules.branch]
link = true               # https://<host>/<owner>/<name>/tree/<branch>
max_length = 24

[modules.pr]
link = true
show_state_word = true

[modules.text.docs]
text    = "docs"
width   = 6
justify = "right"
url     = "https://github.com/justanotherspy/garnish/blob/main/docs/guide.md"
color   = "accent2"

[modules.text.issues]
text    = "issues"
width   = 6
justify = "right"
url     = "https://github.com/justanotherspy/garnish/issues"
color   = "accent2"
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

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["text.motd", "clock"]

[[row]]
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

## `narrow-unicode`

three short unframed rows for a 72-column pane, no Nerd Font needed, capped modules

At 72 columns:

```text
❒ ~/garnish  ⇄ #42 ❍                                           16:00
❖ Opus  ⊞ █████░░░░░░▏ 42%                                     1h12m
⏳ 24% ⏱ 2h13m  ≣ 41% ⏱ 3d4h                                     91%
```

<details><summary><code>presets/narrow-unicode.toml</code></summary>

```toml
# name: narrow-unicode
# summary: three short unframed rows for a 72-column pane, no Nerd Font needed, capped modules
# columns: 72

# Made for a narrow pane: the unicode set needs no patched font, the frame
# is off with two spaces between modules, every module that can grow is
# capped with `max_width`, and an overdue cached value hides rather than
# dims (`stale_style`) after two missed refreshes.

preset = "compact"
icons  = "unicode"
theme  = "garnish"
color  = "auto"
stale_style = "hide"
stale_after = 2

[frame]
style = "none"
separator = "  "

[[row]]
modules = ["path", "branch", "pr"]
right   = ["clock"]

[[row]]
modules = ["model", "context"]
right   = ["session"]

[[row]]
modules = ["limit5h", "limit7d"]
right   = ["cache"]

[modules.path]
depth = 1
max_width = 18

[modules.branch]
max_width = 16

[modules.model]
max_width = 12

[modules.context]
width = 12

[modules.clock]
preset = "minimal"        # 16:00, no spinner

[modules.session]
preset = "minimal"

[modules.cache]
preset = "minimal"
```

</details>

## `pace-and-eta`

the two rate-limit windows with pace against the clock, a projected time to 100 %, and the time cursor on their bars

At 130 columns, needs nerd-font:

```text
╭─  ~/projects/garnish            │  ████████▍░░░░░░░░░░▏ 42% ──────────────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─  ██▊░░░▏░░░░░ 24% ⇣32%  2h13m │  ████▉░▏░░░░░ 41% ⇣14%  3d04h ────────────────────────────────────────────────────────╯
```

<details><summary><code>presets/pace-and-eta.toml</code></summary>

```toml
# name: pace-and-eta
# summary: the two rate-limit windows with pace against the clock, a projected time to 100 %, and the time cursor on their bars
# columns: 130
# needs: nerd-font

# Every pace key of SPEC § 3.3 on both windows: `pace` prints how far the
# usage runs ahead of (⇡, hot) or behind (⇣, ok) the elapsed share of the
# window, `pace_colors` colours the percentage by that band instead of
# the thresholds, `eta` projects when 100 % lands (shown only while it
# lands before the reset, so a window behind pace prints none, as in the
# pinned sample), and `elapsed_marker` drops a cursor on the mini bar at
# the elapsed share. `spend` has no known window, so it keeps the plain
# countdown.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "rounded"

[[row]]
modules = ["path", "branch", "context"]
right   = ["clock"]

[[row]]
modules = ["limit5h", "limit7d"]
right   = ["spend", "cost"]

[modules.limit5h]
preset = "full"
bar_width = 12
pace = true
pace_colors = true
eta = true
elapsed_marker = true

[modules.limit7d]
preset = "full"
bar_width = 12
pace = true
pace_colors = true
eta = true
elapsed_marker = true

[modules.spend]
bar_width = 8
```

</details>

## `packed-heavy`

custom heavy frame, left-packed rows, a separator per row

At 130 columns, needs nerd-font:

```text
┏  ~/projects/garnish   garnish-dev  ⠋ 16:00:00
┃  Opus               ⋮  ▁▃▅▇█       ⋮  ████████▍░░░░░░░░░░▏ 42% ⋮  24%  2h13m ⋮  41%  3d04h
┗  1h12m              •  8m20s       •  91% 1h  47m00s          •  +156 −23
```

<details><summary><code>presets/packed-heavy.toml</code></summary>

```toml
# name: packed-heavy
# summary: custom heavy frame, left-packed rows, a separator per row
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

[[row]]
modules = ["path", "branch", "sync"]
right   = ["session_name", "clock"]
separator = "  "          # this line: two spaces, no bar

[[row]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "limit7d"]

[[row]]
modules = ["session", "api", "cache"]
right   = ["lines"]
separator = " • "
```

</details>

## `precise-numbers`

every number at full precision: token counts with thousands separators, percentages to a decimal, whole dollars, dimmed details

At 120 columns:

```text
╭─ ❖ Opus  │ ⚙ ▁▃▅▇█         │ ⊞ ████████▍░░░░░░░░░░▏ 42.0% 1.0M ───────────────────────────────────── ⠋ 16:00:00 ─╮
╰─ ⏱ 1h12m │ ⇄ 8m20s (11.6%) │ ⛁ 91.0% 1h ✦ 47m00s 352,000w ────────────────────────────── Δ +156 −23 (+133) │ $1 ─╯
```

<details><summary><code>presets/precise-numbers.toml</code></summary>

```toml
# name: precise-numbers
# summary: every number at full precision: token counts with thousands separators, percentages to a decimal, whole dollars, dimmed details
# columns: 120

# The `[format]` table of SPEC § 4 decides how every module prints a
# number: `tokens = "precise"` writes 128,400 where `compact` writes 128k,
# `percent = "precise"` keeps one decimal, `cost = "whole"` rounds to
# dollars, and `parens = "dim"` mutes the parenthesised details (the api
# share, the net lines, the cache writes) so the values in front of them
# stand out. A module can pin its own style with `tokens`, `percent` or
# `cost` under `[modules.<id>]`; here the context module keeps compact
# tokens for its window tag.

preset = "default"
icons  = "unicode"
theme  = "garnish"
color  = "auto"
align  = true
durations = "fixed"

[format]
tokens  = "precise"
percent = "precise"
cost    = "whole"
parens  = "dim"

[frame]
style = "rounded"

[[row]]
modules = ["model", "effort", "context"]
right   = ["clock"]

[[row]]
modules = ["session", "api", "cache"]
right   = ["lines", "cost"]

[modules.context]
show_window = true
tokens = "compact"          # the window tag stays 1M, the rest is precise

[modules.api]
show_share = true

[modules.cache]
show_writes = true

[modules.lines]
show_net = true

[modules.cost]
only_without_rate_limits = false
```

</details>

## `quiet-when-idle`

modules that leave the row while there is nothing worth reading: zero counts, a near-empty context, an absent cost

At 110 columns:

```text
╭─ ❒ ~/projects/garnish       │ ⇄ #42 ❍ ──────────────────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─ ⊞ ████████▍░░░░░░░░░░▏ 42% │ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d20h/7d ────────────────── Δ +156 −23 │ $1.23 ─╯
```

<details><summary><code>presets/quiet-when-idle.toml</code></summary>

```toml
# name: quiet-when-idle
# summary: modules that leave the row while there is nothing worth reading: zero counts, a near-empty context, an absent cost
# columns: 110

# The `hide` list of SPEC § 3 names the states in which a module leaves its
# row: `zero` for a count or an amount, `below:N` / `above:N` for a
# percentage, `empty` for nothing to show at all. Which states a module
# accepts follows from what it measures (its page lists them). So the
# lines module disappears until something changed, the context module
# until a tenth of the window is used, the sync counts while the branch is
# in step, and the cost while it is nil. `hide = ["empty"]` on `pr` is the
# same as the default `hide_when_empty = true`: the two combine, and
# neither switches the other off. The seven-day window prints how much of
# it has elapsed instead of a countdown.

preset = "default"
icons  = "unicode"
theme  = "garnish"
color  = "auto"
align  = true

[frame]
style = "rounded"

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]

[[row]]
modules = ["context", "limit5h", "limit7d"]
right   = ["lines", "cost"]

[modules.sync]
hide = ["zero"]

[modules.pr]
hide_when_empty = false     # would print `–` for no pull request…
hide = ["empty"]            # …but `empty` in the list hides it all the same

[modules.context]
hide = ["below:10"]

[modules.limit7d]
reset = "elapsed"           # 3d20h/7d rather than a countdown

[modules.lines]
hide = ["zero"]

[modules.cost]
only_without_rate_limits = false
hide = ["zero"]
```

</details>

## `session-badges`

the harness itself beside the model: its version, a sandbox and a voice badge, the signed-in account, separators in the colour of what precedes them

At 100 columns, needs nerd-font:

```text
╭─  Opus               │  ▁▃▅▇█ │  v2.1.260 ─────────────────────────────────── ⠋ 16:00:00 ─╮
╰─  ~/projects/garnish │  ████████▍░░░░░░░░░░▏ 42% ────────────────────────────────  1h12m ─╯
```

<details><summary><code>presets/session-badges.toml</code></summary>

```toml
# name: session-badges
# summary: the harness itself beside the model: its version, a sandbox and a voice badge, the signed-in account, separators in the colour of what precedes them
# columns: 100
# needs: nerd-font

# The four modules of SPEC § 3.8 on the first row. `version` is the Claude
# Code version from the payload; `sandbox` and `voice` show while
# `sandbox.enabled` and `voice.enabled` are on in the settings chain (the
# `full` preset adds the word to the glyph); `account` is the claude.ai
# email a background worker reads from `~/.claude.json`, so the first
# tick of a session shows nothing there. The row keeps its shape when a
# badge is absent, which is how the pinned sample looks: no settings
# file, no worker. `separator_color = "inherit"` paints each separator
# with the first colour of the module before it.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true

[frame]
style = "rounded"
separator_color = "inherit"

[[row]]
modules = ["model", "effort", "version", "sandbox", "voice", "account"]
right   = ["clock"]

[[row]]
modules = ["path", "branch", "context"]
right   = ["session"]

[modules.version]
show_icon = true

[modules.sandbox]
preset = "full"

[modules.voice]
preset = "full"

[modules.account]
style = "user"              # the part before `@`
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

[[row]]
modules = ["path", "branch", "sync"]
right   = ["clock"]

[[row]]
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

## `sidebar-panels`

a 34-cell boxed sidebar, a two-share stack of titled rows, and a bottom-aligned column

At 140 columns, needs nerd-font:

```text
╭─ ┏━━━━━━━━━━━━━ Repo ━━━━━━━━━━━━━┓   Model ───────────────  Opus │  ▁▃▅▇█ ─────────────────────                                  ─╮
├─ ┃  ~/projects/garnish ───────── ┃   ──────────────────  ██████▋░░░░░░░░▏ 42% ───────── Context                                   ─┤
├─ ┃  #42  ────────────────────── ┃   ──────────────  24%  2h13m │  41%  3d04h ──── Usage ────                                  ─┤
├─ ┃  garnish-dev ──────────────── ┃                                                                  ────────────  1h12m │  8m20s ─┤
╰─ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛                                                                  ─────────────────── ⠋ 16:00:00 ─╯
```

<details><summary><code>presets/sidebar-panels.toml</code></summary>

```toml
# name: sidebar-panels
# summary: a 34-cell boxed sidebar, a two-share stack of titled rows, and a bottom-aligned column
# columns: 140
# needs: nerd-font

# Column widths in cells and in shares (SPEC § 4.3): the sidebar is exactly
# 34 cells and boxed by name, with the box's own style, colour and a filled
# rule inside; the middle column takes two shares of what is left and
# stacks three titled rows; the last column sits at the bottom of the row.

preset = "default"
icons  = "nerd"
theme  = "catppuccin-mocha"
color  = "auto"
durations = "fixed"

[frame]
style = "rounded"

[box.side]
title = "Repo"
title_justify = "center"
style = "heavy"
fill  = true
color = "accent2"

[[row]]
gap = 3

[[row.col]]
width = 34
box = "side"
[[row.col.row]]
modules = ["path"]
[[row.col.row]]
modules = ["branch", "pr"]
[[row.col.row]]
modules = ["sync", "worktree", "session_name"]

[[row.col]]
width = "2fr"
[[row.col.row]]
title = "Model"
modules = ["model", "effort"]
right   = ["vim"]
[[row.col.row]]
title = "Context"
title_justify = "right"
modules = ["context"]
[[row.col.row]]
title = "Usage"
title_justify = "center"
modules = ["limit5h", "limit7d"]
right   = ["cost"]

[[row.col]]
width = "1fr"
valign = "bottom"
justify = "right"
[[row.col.row]]
modules = ["session", "api"]
[[row.col.row]]
modules = ["clock"]

[modules.context]
width = 16
```

</details>

## `single-line-full`

everything on one row, always scrolling as a ticker (200 columns is a comfortable window)

At 200 columns, needs nerd-font:

```text
──  ~/projects/garnish │  #42  pending │  Opus  claude-opus-5 │  ▁▃▅▇█ high │  ████████████▌░░░░░░░░░░░░░░░░▏… ─  garnish-dev sess-000 │  +156 −23 (+133) │ ⠋ 16:00:00 Sat 01 Feb +00:00 ──
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

[[row]]
modules = ["path", "branch", "sync", "worktree", "pr", "model", "effort", "context", "style", "limit5h", "limit7d", "spend", "cost", "session", "api", "cache"]
right   = ["session_name", "agent", "vim", "lines", "clock"]
```

</details>

## `slow-motion`

half-speed rule, separator and note; a pulsing bar and an eight-dot spinner at full speed

At 100 columns, needs nerd-font:

```text
╭─  ~/projects/garnish │  #42  ─ ─ ╌ ─ ─ ╌ ─ ─ ╌ ─ ─  half speed: this note   │ ⠁ 16:00:00 ─╮
╰─  Opus │  ████████▍░░░░░░░░░░▏ 42% │  24%  2h13m ─ ─ ╌ ─ ─ ╌ ─ ─ ╌ ─ ─ ╌  91% 1h  47m ─╯
```

<details><summary><code>presets/slow-motion.toml</code></summary>

```toml
# name: slow-motion
# summary: half-speed rule, separator and note; a pulsing bar and an eight-dot spinner at full speed
# columns: 100
# needs: nerd-font

# The step keys (SPEC § 4.2) slow an animation down: 0.5 advances every
# second tick. The rule pattern travels left and the separator alternates
# at half speed, and a note scrolls through its box at half speed and
# restarts after the end. Icon frames take no step, so the context bar's
# filled cells pulse through two glyphs and the clock spins on eight
# braille dots once a tick.

preset = "compact"
icons  = "nerd"
theme  = "dracula"

[frame]
style            = "rounded"
fill_pattern     = "─ ─ ╌ "
fill_direction   = "left"
fill_step        = 0.5
separator_frames = [" │ ", " ┆ "]
separator_step   = 0.5

[[row]]
modules = ["path", "branch", "pr"]
right   = ["text.note", "clock"]

[[row]]
modules = ["model", "context", "limit5h"]
right   = ["cache"]

[modules.context.icons]
fill_frames = ["█", "▓"]

[modules.clock.icons]
spinner_frames = ["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"]

[modules.text.note]
text     = "half speed: this note moves every second tick"
width    = 22
pad      = 1
overflow = "scroll"
step     = 0.5
```

</details>

## `still-life`

nothing moves: animation off, no spinner or seconds, fixed-width timers, plain stale values

At 110 columns, needs nerd-font:

```text
╔═  ~/projects/garnish │  #42  ═══════════════════════════════════════════════════════  garnish-dev ═╗
╠═  Opus               │  ▁▃▅▇█ │  ████████▍░░░░░░░░░░▏ 42% ════════════════════════  1h12m │ 16:00 ═╣
╚═  24%  2h13m        │  41%  3d04h ══════════════════════════════════════════════════════  91% 1h ═╝
```

<details><summary><code>presets/still-life.toml</code></summary>

```toml
# name: still-life
# summary: nothing moves: animation off, no spinner or seconds, fixed-width timers, plain stale values
# columns: 110
# needs: nerd-font

# For a recording, a screen reader, or a status line that should not draw
# the eye: `animate = false` freezes every animation at frame 0 (SPEC
# § 4.2), the clock shows neither spinner nor seconds so it changes once a
# minute, timers keep their width, and an overdue cached value stays as it
# was instead of dimming.

preset = "default"
icons  = "nerd"
theme  = "garnish"
color  = "auto"
align  = true
animate = false
durations = "fixed"
stale_style = "plain"

[frame]
style = "double"

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name"]

[[row]]
modules = ["model", "effort", "context"]
right   = ["session", "clock"]

[[row]]
modules = ["limit5h", "limit7d"]
right   = ["cache"]

[modules.clock]
seconds = false
spinner = false

[modules.context]
width = 20

[modules.cache]
show_countdown = false    # the warm countdown is the one value that ticks
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

[[row]]
modules = ["path"]
right   = ["clock"]
[[row]]
modules = ["branch"]
[[row]]
modules = ["sync"]
[[row]]
modules = ["pr"]
right   = ["session_name"]
[[row]]
modules = ["model"]
[[row]]
modules = ["context"]
right   = ["limit5h"]
[[row]]
modules = ["session"]
[[row]]
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

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name"]

[[row]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "limit7d"]

[[row]]
modules = ["session", "api", "cache"]
right   = ["lines", "clock"]

[modules.context]
preset = "full"           # bar plus token counts and the compaction marker
width  = 30
```

</details>

## `ticker-two-step`

a long first row scrolling two cells a tick behind a still clock, a second row that fits, 90 columns

At 90 columns, needs nerd-font:

```text
┌─  ~/projects/garnish │  #42  │  garnish-dev │  Opus │  ▁▃▅▇█ │  █… ─ 16:00 ─┐
└─  24%  2h13m │  +156 −23 ───────────────────────────────────  1h12m │  8m20s ─┘
```

<details><summary><code>presets/ticker-two-step.toml</code></summary>

```toml
# name: ticker-two-step
# summary: a long first row scrolling two cells a tick behind a still clock, a second row that fits, 90 columns
# columns: 90
# needs: nerd-font

# `overflow = "ticker"` scrolls a left group wider than its budget instead
# of cutting it (SPEC § 4.1): `ticker_step = 2` moves it two cells a tick
# and `ticker_gap` separates the end from the wrapped-around start. The
# right group holds still, and so does the second row, which fits. Timers
# default to `durations = "fixed"` under a ticker, so the window slides
# rather than jumps; the scrolled row carries nothing that counts seconds,
# so the only movement in it is the slide.

preset = "default"
icons  = "nerd"
theme  = "tokyonight"
overflow    = "ticker"
ticker_step = 2
ticker_gap  = "  ⋯  "

[frame]
style = "square"

[[row]]
modules = ["path", "branch", "sync", "pr", "session_name", "model", "effort", "context"]
right   = ["clock"]

[[row]]
modules = ["limit5h", "lines"]
right   = ["session", "api"]

[modules.clock]
seconds = false
spinner = false
```

</details>

## `titled-sections`

a title in every rule, left, centred and right, a titled spacer and one boxed row, Nord

At 120 columns, needs nerd-font:

```text
╭─  ~/projects/garnish │  #42   Repository ─────────────────────────────────────────────────────  garnish-dev ─╮
├─  Opus               │  ▁▃▅▇█ │  ██████▋░░░░░░░░▏ 42% ───────────────────────  Model  ────────────────────────┤
├─  24%  2h13m        │  41%  3d04h ─────────────────────────────────────────────────────── Usage   +156 −23 ─┤
╰─ ──────────────────────────────────────────────────── · · · ─────────────────────────────────────────────────────╯
╭─ Session ────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│  1h12m              │  8m20s │  91% 1h  47m00s                                                    ⠋ 16:00:00 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

<details><summary><code>presets/titled-sections.toml</code></summary>

```toml
# name: titled-sections
# summary: a title in every rule, left, centred and right, a titled spacer and one boxed row, Nord
# columns: 120
# needs: nerd-font

# A title is text set into a row's rule (SPEC § 4.3): in the first empty
# run of the line (after the modules, or after the cap on a spacer), the
# widest, or the last one before the right group, with `title_pad` spaces
# each side and `title_color` for a role or a literal. A row with only a
# title is a titled spacer, and `box = true` boxes one row alone with its
# own `title*` keys on that box's top rule.

preset = "default"
icons  = "nerd"
theme  = "nord"
color  = "auto"
align  = true
durations = "fixed"

[frame]
style = "rounded"

[[row]]
title = "Repository"
title_color = "accent"
modules = ["path", "branch", "sync", "pr"]
right   = ["session_name"]

[[row]]
title = "Model"
title_justify = "center"
title_pad = 2
modules = ["model", "effort", "context"]
right   = ["vim"]

[[row]]
title = "Usage"
title_justify = "right"
title_color = "#88c0d0"
modules = ["limit5h", "limit7d", "spend"]
right   = ["lines"]

[[row]]
title = "· · ·"
title_justify = "center"
modules = []

[[row]]
box = true
title = "Session"
modules = ["session", "api", "cache"]
right   = ["clock"]

[modules.context]
width = 16
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

[[row]]
modules = ["path", "branch", "sync", "worktree", "pr"]
right   = ["session_name", "clock"]

[[row]]
modules = ["model", "context", "limit5h"]
right   = ["session", "cost"]

[modules.context]
preset = "full"
width  = 30
```

</details>
