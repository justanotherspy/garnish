<h1 align="center">🌿 garnish</h1>

<p align="center">
  <em>A fast, cached, beautifully themed status line for Claude Code.</em><br>
  <sub>Rust · nightly · no network calls of its own · &lt; 3 ms per tick</sub><br>
  <a href="https://github.com/justanotherspy/garnish/actions/workflows/ci.yml"><img src="https://github.com/justanotherspy/garnish/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ───────────────────────── ❯ garnish-dev ─╮
├─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% ──────────────────────────┤
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ───────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m ──────────────────────── ⠋ 16:00:00 ─╯
```

<sub>The default preset with unicode icons in an 80-column terminal, rendered from a saved payload (inside a repository the first line also carries the branch, ahead/behind and worktree). The other presets are below; every frame style is rendered in [docs/config.md](docs/config.md).</sub>

## Why

Claude Code re-runs your status line command every second. garnish makes that
free: it parses the session JSON, renders **25 small modules** (plus any
number of your own fixed-width text boxes) from a TOML config, and keeps
anything slow (git, worktrees) in a detached background
worker so a tick never waits. Dozens of sessions on one machine, no
contention.

## Presets

Four top-level presets pick the lines and how much each module says. Set
`preset = "…"` in the config; the samples use unicode icons, rendered at a
terminal width where nothing is cut.

`default`, four lines, at 80 columns:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ───────────────────────── ❯ garnish-dev ─╮
├─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% ──────────────────────────┤
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ───────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m ──────────────────────── ⠋ 16:00:00 ─╯
```

`compact`, two lines, at 90 columns:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% │ ⏳ 24% ⏱ 2h13m ── ⛁ 91% 1h ✦ 47m ─╯
```

`minimal`, one line and no frame, at 80 columns:

```text
~/garnish  42%  24%                                                    16:00
```

`full`, every module at full verbosity; it wants a wide terminal, here 120
columns (this block is the one that may scroll on a narrow screen):

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ pending ──────────────────────────────────────────────── ❯ garnish-dev sess-000 ─╮
├─ ❖ Opus ⋯ claude-opus-5 │ ⚙ ▁▃▅▇█ high │ ⊞ ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ │ ✎ default ───────────┤
├─ ⏳ █▉░░░░░░ 24% ⏱ 2h13m │ ≣ ███▎░░░░ 41% ⏱ 3d4h ──────────────────────────────────────────── Δ +156 −23 (+133) ─┤
╰─ ⏱ 1h12m since 14:48 │ ⇄ 8m20s (12%) │ ⛁ 91% 1h ✦ 47m 2 misses 352kw ───────────── ⠋ 16:00:00 Sat 01 Feb +00:00 ─╯
```

Beyond the four built-ins, the [presets gallery](docs/presets.md) has 32
complete configs (titled sections, boxed panels, a sidebar, a grid, links,
a compaction-aware bar, pace against the rate limits, precise numbers,
modules that hide when idle, narrow and ASCII-only terminals, tickers and
animation, …) rendered at their own widths; `garnish setup` previews each
at yours, `garnish presets` lists them and `garnish config init --preset
<name>` writes one.

A config written by `garnish config init` (or `garnish install`) spells out
every `[[row]]` and the `[frame]`; those explicit blocks win over the
preset, so changing `preset` in such a file only changes what each module
shows, not the rows. To switch presets outright, delete the `[[row]]` and
`[frame]` blocks (or start from a file that holds only `preset`, `icons` and
`theme`). Each preset's module list is in
[docs/config.md](docs/config.md#top-level-presets).

## Requirements

- Linux or macOS. (Windows is not supported.)
- [rustup](https://rustup.rs). The repository pins a nightly toolchain in
  `rust-toolchain.toml`; rustup installs it on the first build.
- Claude Code 2.1.251 or newer (the version that added the `prompt_cache`
  and `effort` payload fields).
- A [Nerd Font](https://www.nerdfonts.com) for the default glyphs. Without
  one, set `icons = "unicode"` (or `emoji` / `ascii`).
- A terminal with OSC 8 support (iTerm2, Kitty, WezTerm, Ghostty…) if you
  want clickable pull-request numbers, branch names (`link = true`) and
  text boxes (`url`). Everything else works anywhere ANSI colors do.

## Install

With [Homebrew](https://brew.sh) (macOS and Linux, prebuilt binary, from
the first tagged release; until then, build from source):

```sh
brew install --cask justanotherspy/tap/garnish
```

From source:

```sh
git clone https://github.com/justanotherspy/garnish.git && cd garnish
make install            # cargo install --path . --locked  →  ~/.cargo/bin/garnish
```

Then set it up:

```sh
garnish setup           # pick a preset or build a layout, previewed live, then hook it into Claude Code
```

`setup` is a full-screen picker and builder: every preset rendered at
your terminal's real width, rows, columns, titles and boxes from single
keys, an editor for every option, a preview you can click, undo, and an
install step that merges the `statusLine` block into
`~/.claude/settings.json` with a backup kept. Run it again any time to
edit the config in place.

Without the screen, `garnish setup --preset compact --install` writes a
preset and hooks it up, and `garnish install` alone does the settings
(`--dry-run` shows the change; `--absolute` if `~/.cargo/bin` is not on the
PATH Claude Code sees; `garnish --config FILE install` writes a command that
reads FILE, and a later `install` keeps the arguments and any
`NAME=value` prefix a garnish command already has; a `--config FILE` kept
that way is the file every other command then uses: `config path`,
`config check`, `config show`, `preview`, `doctor`, `config init`, `setup`
and `install`; inside a project whose own `.claude/` settings name
another config, by the command's `--config` or by `GARNISH_CONFIG` in
their `env` block, garnish follows that file for none of them, since a
cloned repository must not choose a file garnish reads or writes: each
refuses on one line, and `garnish --config FILE …` says which you mean;
inside a Claude Code session a `GARNISH_CONFIG` counts only when your own
`~/.claude/settings.json` sets it, and anywhere only as an absolute path,
since Claude Code does not expand `~` in a settings value).
The equivalent by hand:

```json
{ "statusLine": { "type": "command", "command": "garnish", "refreshInterval": 1 } }
```

## Compose your line

`garnish setup` edits everything below in place with a live preview; this
is what it writes.

```toml
preset = "default"          # default | minimal | full | compact
icons  = "nerd"             # nerd | unicode | emoji | ascii
theme  = "catppuccin-mocha" # garnish | catppuccin-mocha | nord | dracula | tokyonight | mono

[frame]
style = "rounded"           # none | rounded | square | double | heavy | powerline | custom

[[row]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]

[[row]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "cost"]

[modules.context]
preset = "full"
width  = 30
```

Every module has `minimal` / `default` / `full` presets plus its own icons,
colors and refresh interval, and the common `label`, `prefix`, `suffix`,
`hide_when_empty` and `max_width` keys (`max_width` cuts a module to that
many cells with `…`, so one long value cannot push the rest of the row
off). Put any module on any row, left or right.

A few top-level keys keep a multi-line layout tidy: `align = true` pads every
module column to the widest module in it, so the `│` separators stack
vertically instead of drifting with each line's content (`right_justify =
"start"` keeps a padded right-side module next to its separator instead of
the cap); `durations = "fixed"` prints timers as `9m00s` / `1h05m` instead of
`9m` / `1h5m`, so they keep their width as they tick; and `hide_empty_rows`
(on by default) drops a row whose modules all have nothing to show, while
`modules = []` makes a spacer row that always stays. (`[[line]]` and
`hide_empty_lines` are permanent aliases of `[[row]]` and `hide_empty_rows`,
so a config written before rows existed keeps working.)

A module can also leave the row while there is nothing worth reading:
`hide = ["zero"]` on `lines` or `sync`, `hide = ["below:10"]` on `context`
(each module page lists the states it takes). `[format]` decides how every
number prints (`tokens = "precise"` for `128,400`, `percent = "precise"`
for `42.3%`, `cost = "whole"`, `parens = "dim"` for muted details), and a
module that prints a number can pin its own style. The two rate-limit
windows can show their pace against the clock (`pace = true` prints
`⇡14%` ahead or `⇣32%` behind, `eta = true` the time until the window is
spent, `reset = "elapsed"` how much of it has passed, `elapsed_marker` a
cursor on the bar).

```text
╭─ ❖ Opus         │ ⊞ ████████▍░░░░░░░░░░▏ 42% ─────────────── ⠋ 16:00:00 ─╮
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d04h ──────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m        │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m00s ───────────────────────────╯
```

A row can also be **columns side by side**, and a column can hold a stack of
rows of its own. `width` is `"1fr"` (a share of what is left), `"auto"` (the
column's content) or a number of cells; `justify` places a column's modules
and follows the column's position when you leave it out, so three bare
columns read left / centre / right. `title` sets text into a row's rule, and
`[box.<name>]` frames a run of rows — or a whole column — with its own
corners and sides:

```text
╔═ Repository ═════════════════════════════════════════╗
║ ❒ ~/projects/garnish │ ❖ Opus             ⠋ 16:00:00 ║
║ ⊞ ████████▍░░░░░░░░░░▏ 42%                           ║
╚══════════════════════════════════════════════════════╝
```

The `grid-three`, `grid-six`, `boxed-panels` and `dashboard-panels` presets
are working examples; [docs/config.md](docs/config.md#rowcol) has the keys.

| group | modules |
|---|---|
| repo | `path` `branch` `sync` `worktree` `pr` |
| model | `model` `effort` `context` `style` |
| usage | `limit5h` `limit7d` `spend` `cost` |
| session | `session` `api` `cache` `clock` |
| identity | `session_name` `vim` `agent` `lines` |
| harness | `version` `sandbox` `voice` `account` |

Start with the [guide](docs/guide.md), then the
[configuration reference](docs/config.md) and the per-module pages under
[docs/modules/](docs/README.md). The reference pages are generated from the
module definitions in the code, so they always match the binary you built.

## Try it without a session

```sh
garnish config init && garnish config check && garnish config show
garnish doctor          # versions, settings, config, cache, failed refreshes, glyph test
garnish setup           # pick, build and preview a layout full-screen
```

`garnish setup` previews every change live on the bundled sample payloads.
From a checkout of this repository, `preview` renders those payloads
directly (`tests/fixtures/payloads` is not installed with the binary):

```sh
garnish preview tests/fixtures/payloads --preset compact --icons unicode --theme nord
```

## Skills

Three Claude Code skills ship with garnish under `skills/` and are written to
`~/.claude/skills/` by `garnish install` (or `garnish skills install`); with
`CLAUDE_CONFIG_DIR` set, the skills and the settings file go under it, where
Claude Code reads them:

- **garnish-statusline** offers `garnish setup` first, or builds the
  config from a conversation (terminal, font, width, what matters, rows or
  panels, colours, frame, motion, links), previews a draft with `garnish
  preview`, validates it with `config check` and writes it once you
  approve.
- **garnish-feedback** files an issue on this repository with `gh`, carrying
  the environment, `config show`, `doctor` (glyph grid included) and the
  rendered line, and asks for a screenshot.
- **garnish-submit-preset** turns the current config into a gallery preset
  proposal: header, validated file, rendered sample, an issue labelled
  `preset`.

None of them needs network access from garnish itself; they drive the
`garnish` and `gh` CLIs.

## Troubleshooting

- **Boxes instead of icons**: your font lacks Nerd Font glyphs; set
  `icons = "unicode"`.
- **The right edge wanders by a cell on some lines**: your terminal draws a
  glyph wider (or narrower) than garnish counts. `garnish doctor` ends with a
  glyph grid in which every `|` should line up; the one pushed out of its
  column names the glyph. Override it under `[modules.<id>.icons]`, and paste
  the grid into an issue so the built-in set can be fixed.
- **Hairline gaps between the blocks of a bar**: the font draws `█` a shade
  narrower than a cell; set `bar = "line"` on the module for a `━`/`─` bar.
- **A value with `⟳` or `✗` after it**: the background refresh is overdue or
  failed; `garnish doctor` shows the last error.
- **Nothing changes after editing the config**: `garnish config path` shows
  which file is read, `garnish config check` reports problems with their
  TOML path. A bad key never blanks the line: every valid key stays in
  effect, the built-in default stands in for the bad one, and a dim
  `⚠ config: …` note is appended; only a file that does not parse falls back
  to the defaults wholesale.
- **The right edge is cut with `…`**: Claude Code truncates rows wider than
  its own box, which is 4 cells narrower than the terminal plus 2 cells per
  unit of `statusLine.padding`. garnish subtracts the 4 by itself; if you set
  `statusLine.padding` in `settings.json`, set `padding` in the config to
  twice that value.
- **The line looks faint**: Claude Code draws every status line row dim,
  and nothing a command prints can undo it; `garnish preview` draws its
  rows the same way, so pick brighter roles under `[colors]` if it reads
  too faint there.
- **Nothing moves**: `garnish doctor` reports whether `refreshInterval` is
  set and whether Claude Code's *Reduce motion* setting is freezing the
  animations (`animate = true` in the config overrides it).
- **The bottom rows are missing**: Claude Code's fullscreen renderer
  (`/tui`) gives the prompt box and the status line together at most half
  the terminal's rows and cuts a taller status line from the bottom; keep
  the line count at most `LINES / 2 − 5`, rounding down (7 rows on a
  24-line terminal), or use the classic renderer.
- **The terminal is garbled after `garnish setup` was killed**: `setup`
  puts the terminal back when it exits, on `Ctrl+C` and on a crash, but a
  `kill` gives it no chance to; typing `reset` (even unseen) and Enter
  restores echo, the main screen and the mouse.

More in the guide's [troubleshooting section](docs/guide.md#7-troubleshooting).

## Contributing

garnish is written by Claude Code, session by session, under the rules in
[`CLAUDE.md`](CLAUDE.md). [`SPEC.md`](SPEC.md) is the target design and
[`PLAN.md`](PLAN.md) the progress and backlog.

```sh
make setup    # rustup nightly + components + cargo-nextest (ARGS=--bench adds hyperfine, jq)
make check    # fmt + clippy (pedantic, nursery, no panics) + nextest + doctests
make docs     # regenerate docs/ and examples/ from the module schemas
make bench    # hyperfine gate: warm tick mean < 3 ms, p99 < 8 ms
```

Pull requests run the same setup and checks on Linux and macOS.
