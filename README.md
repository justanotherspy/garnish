<h1 align="center">🌿 garnish</h1>

<p align="center">
  <em>A fast, cached, beautifully themed status line for Claude Code.</em><br>
  <sub>Rust · no network calls · &lt; 3 ms per tick</sub><br>
  <a href="https://github.com/justanotherspy/garnish/actions/workflows/ci.yml"><img src="https://github.com/justanotherspy/garnish/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ───────────────────────── ❯ garnish-dev ─╮
├─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% ──────────────────────────┤
├─ ⏳ 24% ⏱ 2h13m │ ≣ 41% ⏱ 3d4h ───────────────────────────── Δ +156 −23 ─┤
╰─ ⏱ 1h12m │ ⇄ 8m20s │ ⛁ 91% 1h ✦ 47m ──────────────────────── ⠋ 16:00:00 ─╯
```

Claude Code runs your status line command every second. garnish renders
the session's model, context window, rate limits, cost, git state and more
from a TOML config, and does anything slow (git) in a background worker so
a tick never waits.

## Get started

You need Linux or macOS and Claude Code 2.1.251 or newer.

**1. Install.** With Homebrew (once the first release is tagged):

```sh
brew install --cask justanotherspy/tap/garnish
```

Or from source, with [rustup](https://rustup.rs) installed:

```sh
git clone https://github.com/justanotherspy/garnish.git && cd garnish
make install            # → ~/.cargo/bin/garnish
```

**2. Set it up.**

```sh
garnish setup
```

Choose *Pick a preset*, move through the list (each preset is previewed at
your terminal's width), and press `Enter`. garnish writes your config and
then offers to install: accept, and it adds the `statusLine` block to
`~/.claude/settings.json` (keeping a backup) and installs its three skills.

Prefer no screen? This does the same with the `default` preset:

```sh
garnish setup --preset default --install
```

**3. Start Claude Code.** The status line is at the bottom. Run `garnish
setup` again whenever you want to change it.

If the icons show as boxes, your terminal font has no
[Nerd Font](https://www.nerdfonts.com) glyphs: pick the `unicode` icon set
in `setup` (or set `icons = "unicode"` in the config). If something else
looks wrong, `garnish doctor` checks the whole chain.

## Presets

The built-ins, rendered with unicode icons. `default` is above.

`compact`, at 90 columns:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ ────────────────────────────────────── ⠋ 16:00:00 ─╮
╰─ ❖ Opus │ ⚙ ▁▃▅▇█ │ ⊞ ████████▍░░░░░░░░░░▏ 42% │ ⏳ 24% ⏱ 2h13m ── ⛁ 91% 1h ✦ 47m ─╯
```

`minimal`, at 80 columns:

```text
~/garnish  42%  24%                                                    16:00
```

`full`, at 120 columns:

```text
╭─ ❒ ~/projects/garnish │ ⇄ #42 ❍ pending ──────────────────────────────────────────────── ❯ garnish-dev sess-000 ─╮
├─ ❖ Opus ⋯ claude-opus-5 │ ⚙ ▁▃▅▇█ high │ ⊞ ████████████▌░░░░░░░░░░░░░░░░▏ 42% ⤓99% 1.0M ‼ │ ✎ default ───────────┤
├─ ⏳ █▉░░░░░░ 24% ⏱ 2h13m │ ≣ ███▎░░░░ 41% ⏱ 3d4h ──────────────────────────────────────────── Δ +156 −23 (+133) ─┤
╰─ ⏱ 1h12m since 14:48 │ ⇄ 8m20s (12%) │ ⛁ 91% 1h ✦ 47m 2 misses 352kw ───────────── ⠋ 16:00:00 Sat 01 Feb +00:00 ─╯
```

The [gallery](docs/presets.md) has 32 more: panels, grids, tickers,
animation, ASCII-only terminals and more. `setup` previews them all.

## Modules

| group | modules |
|---|---|
| repo | `path` `branch` `sync` `worktree` `pr` |
| model | `model` `effort` `context` `style` |
| usage | `limit5h` `limit7d` `spend` `cost` |
| session | `session` `api` `cache` `clock` |
| identity | `session_name` `vim` `agent` `lines` |
| harness | `version` `sandbox` `voice` `account` |

Plus any number of `text.<name>` boxes of your own.

## Learn more

- [Guide](docs/guide.md): setup in depth, writing a config, rows, columns
  and boxes, themes, animation, troubleshooting.
- [Configuration reference](docs/config.md) and the
  [per-module pages](docs/README.md), generated from the code.
- [Presets gallery](docs/presets.md).
- [Troubleshooting](docs/guide.md#7-troubleshooting).
- [Changelog](CHANGELOG.md).

## Contributing

garnish is written by Claude Code, session by session.
[`CLAUDE.md`](CLAUDE.md) has the working rules, [`SPEC.md`](SPEC.md) the
design, [`PLAN.md`](PLAN.md) the status and backlog. `make setup` installs
the toolchain and `make check` runs what CI runs.
