<h1 align="center">🌿 garnish</h1>

<p align="center">
  <em>A fast, cached, beautifully themed status line for Claude Code.</em><br>
  <sub>Rust · nightly · zero network · &lt; 3 ms per tick</sub>
</p>

```text
╭─  ~/repo/garnish   main ⇡2   #42 ○ ──────────────────────  garnish-dev ─╮
├─  Opus ▅▇█ high  ████████████░░░░░░░░▏░ 58%  concise ───────────── NORMAL ─┤
├─  5h 23% ⏱2h13m   7d 41% ⏱3d4h ────────────────────────────── +156 −23 ─┤
╰─  1h12m   8m20s   91% 1h warm 47m ─────────────────────── ⠹ 14:02:33 ─╯
```

## Why

Claude Code re-runs your status line command every second. garnish makes that
free: it parses the session JSON, renders **21 small modules** from a TOML
config, and keeps anything slow (git, worktrees) in a detached background
worker so a tick never waits. Dozens of sessions on one machine, no
contention.

## Install

```sh
git clone <this repo> && cd garnish
make install            # cargo install --path . --locked  →  ~/.cargo/bin/garnish
garnish install         # writes statusLine into ~/.claude/settings.json (backup kept)
garnish config init     # writes ~/.config/garnish/garnish.toml with the defaults
```

Requires a Nerd Font for the default glyphs (`icons = "unicode"` otherwise) and
a terminal with OSC 8 support for clickable PR links.

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

| group | modules |
|---|---|
| repo | `path` `branch` `sync` `worktree` `pr` |
| model | `model` `effort` `context` `style` |
| usage | `limit5h` `limit7d` `spend` `cost` |
| session | `session` `api` `cache` `clock` |
| identity | `session_name` `vim` `agent` `lines` |

Full reference: [`docs/`](docs/README.md) (generated from the module schemas)
and the [guide](docs/guide.md).

## Try it without a session

```sh
garnish preview tests/fixtures/payloads --preset compact --icons unicode --theme nord
garnish config init && garnish config check && garnish config show
```

## Develop

```sh
make check    # fmt + clippy (pedantic, nursery, no panics) + nextest + doctests
make watch    # watchexec: lint + test on every save
make docs     # regenerate docs/ from the module schemas
make bench    # hyperfine gate: warm tick mean < 3 ms, p99 < 8 ms
```

See [`CLAUDE.md`](CLAUDE.md) for the working rules, [`SPEC.md`](SPEC.md) for
the contract, and [`PLAN.md`](PLAN.md) for progress.
