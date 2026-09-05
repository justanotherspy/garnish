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
free: it parses the session JSON, renders **21 small modules** (plus any
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

A config written by `garnish config init` (or `garnish install`) spells out
every `[[line]]` and the `[frame]`; those explicit blocks win over the
preset, so changing `preset` in such a file only changes what each module
shows, not the lines. To switch presets outright, delete the `[[line]]` and
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
  want clickable pull-request numbers. Everything else works anywhere ANSI
  colors do.

## Install

```sh
git clone https://github.com/justanotherspy/garnish.git && cd garnish
make install            # cargo install --path . --locked  →  ~/.cargo/bin/garnish
garnish install         # writes statusLine into ~/.claude/settings.json (backup kept)
garnish config init     # writes ~/.config/garnish/garnish.toml with the defaults
```

`garnish install` merges a `statusLine` block into your settings file and
keeps a backup next to it; add `--dry-run` to see the change first and
`--absolute` if `~/.cargo/bin` is not on the PATH Claude Code sees. The
equivalent by hand:

```json
{ "statusLine": { "type": "command", "command": "garnish", "refreshInterval": 1 } }
```

## Compose your line

```toml
preset = "default"          # default | minimal | full | compact
icons  = "nerd"             # nerd | unicode | emoji | ascii
theme  = "catppuccin-mocha" # garnish | catppuccin-mocha | nord | dracula | tokyonight | mono

[frame]
style = "rounded"           # none | rounded | square | double | heavy | powerline | custom

[[line]]
modules = ["path", "branch", "sync", "pr"]
right   = ["clock"]

[[line]]
modules = ["model", "effort", "context"]
right   = ["limit5h", "cost"]

[modules.context]
preset = "full"
width  = 30
```

Every module has `minimal` / `default` / `full` presets plus its own icons,
colors and refresh interval. Put any module on any line, left or right.

A few top-level keys keep a multi-line layout tidy: `align = true` pads every
module column to the widest module in it, so the `│` separators stack
vertically instead of drifting with each line's content (`right_justify =
"start"` keeps a padded right-side module next to its separator instead of
the cap); `durations = "fixed"` prints timers as `9m00s` / `1h05m` instead of
`9m` / `1h5m`, so they keep their width as they tick; and `hide_empty_lines`
(on by default) drops a line whose modules all have nothing to show, while
`modules = []` makes a spacer row that always stays.

```text
╭─ ❖ Opus         │ ⊞ ████████▍░░░░░░░░░░▏ 42% ─────────────── ⠋ 16:00:00 ─╮
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d04h ──────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m        │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m00s ───────────────────────────╯
```

| group | modules |
|---|---|
| repo | `path` `branch` `sync` `worktree` `pr` |
| model | `model` `effort` `context` `style` |
| usage | `limit5h` `limit7d` `spend` `cost` |
| session | `session` `api` `cache` `clock` |
| identity | `session_name` `vim` `agent` `lines` |

Start with the [guide](docs/guide.md), then the
[configuration reference](docs/config.md) and the per-module pages under
[docs/modules/](docs/README.md). The reference pages are generated from the
module definitions in the code, so they always match the binary you built.

## Try it without a session

```sh
garnish preview tests/fixtures/payloads --preset compact --icons unicode --theme nord
garnish config init && garnish config check && garnish config show
garnish doctor          # versions, settings, config, cache, failed refreshes, glyph test
```

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
