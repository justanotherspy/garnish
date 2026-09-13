# Changelog

User-visible changes per release. The tag message for a release is this
file's section for it. `PLAN.md` holds the session-by-session detail.

## Unreleased

**Harness fidelity** (PLAN Phase 19)

- `animate` left unset now follows Claude Code's *Reduce motion* setting
  (`prefersReducedMotion`, read from the same settings files as the
  autocompact keys); an explicit `animate` wins over it and
  `GARNISH_ANIMATE=0` over both. `config init` writes the key as a comment
  and `config show` prints the value in effect.
- `garnish install` and `config init --force` never rewrite a
  `settings.json` or `garnish.toml` that does not parse: one line names the
  file and the problem and nothing is written. `config init --force` keeps
  a backup of the file it replaces, as `install` does. Every rewrite goes
  through a symlink even before its target exists (a dotfiles link made
  ahead of the file stays a link), the new file is born with the old one's
  permissions, and no temp file survives a failure.
- Claude Code's settings files are read at most 1 MiB deep and an empty
  file counts as `{}` everywhere (`doctor` no longer calls it invalid).
- `garnish doctor` lists Claude Code's settings files for the current
  directory (managed, local, project, user) and whether each parses, then
  the keys that change what the line can show, resolved as Claude Code
  resolves them: it suggests `refreshInterval = 1` when the config shows a
  clock, a timer or a running animation and `hideVimModeIndicator = true`
  when the `vim` module is on, says when a `refreshInterval` below 1 is
  being ignored by Claude Code, and says when `disableAllHooks` or
  `prefersReducedMotion` is in effect. A `statusLine.command` from any of
  those files is shown as plain text, and the project's files relative to
  the project directory, so the report stays safe to paste into an issue.
- Verified against Claude Code 2.1.270 (and 2.1.261): every status line
  row is drawn dim by Claude Code and nothing the command prints can undo
  it, so the planned per-row reset was dropped. `garnish preview` now
  draws its rows faint the same way, so a theme is judged at the
  intensity the screen will give it (`--color never` stays plain; the
  status line itself is unchanged). The 13 000-token autocompact buffer
  is unchanged.
- `GARNISH_MANAGED_SETTINGS` names the managed settings file garnish reads
  first in Claude Code's chain, or, empty, says there is none; `doctor`
  lists it with the other hooks. The test suite sets it, so a managed
  file on the machine running `cargo test` no longer changes a golden.
- How Claude Code 2.1.270 lays out a tall status line, read from its
  renderers: the classic renderer cuts nothing and scrolls the frame, the
  fullscreen renderer (the `tui` setting) gives the prompt box and the
  status line together at most half the terminal and cuts a taller status
  line from the bottom. garnish caps nothing; `doctor` now prints the
  `tui` setting with what it means for the line, and the guide says how
  many rows fit.

**Fixes**

- `cost.decimals` is capped at 8: the money formatter allocated one byte
  per decimal place, so a huge value could exhaust memory on every tick.
- The size caps (`width`, `pad`, `bar_width` ≤ 1024 cells; `text`, `gap`
  ≤ 4096 characters; `decimals` ≤ 8) are part of each module's schema and
  the generated reference shows them in the type column.
- `docs/README.md` no longer says the whole `docs/` directory is generated:
  `docs/guide.md` is hand-written.

**Install**

- Prebuilt binaries for Linux and macOS (x86_64 and aarch64) on every
  release, and a Homebrew cask: `brew install --cask justanotherspy/tap/garnish`.
  The release workflow publishes the cask to the tap only after a manual
  approval (CLAUDE.md § Release process).

## 0.2.0 — 2026-09-06 (PLAN Phases 12–18)

**Fixes**

- The status line box is `COLUMNS − 4 − 2 × statusLine.padding` cells wide;
  garnish now fills exactly that, so the right edge no longer shows `…`
  (SPEC § 2.1). `garnish install --padding` seeds the matching `padding`.
- Every built-in glyph is one cell in every terminal: East Asian Ambiguous
  and emoji-presentation characters left the unicode and emoji sets, and
  `garnish doctor` prints a glyph grid to check your font (SPEC § 4.1).
- A bad value in the config no longer discards the whole file: each key
  falls back on its own and `config check` lists every problem (SPEC § 5).
- Powerline frames pad their segments; the unfilled join of a `packed`
  line uses the frame separator; `config init`, `config check` and
  `preview` exit quietly on a user error instead of printing a report.
- `config show` prints only what is in effect (the theme actually in use,
  line ids that render), so its output always passes `config check`.
- `garnish doctor` collapses the home directory to `~` in every path it
  prints, and its `config` glyph rows keep every field so an override that
  is not one glyph shows as `?` with its cell count.

**Hardening** (whole-stack review, SPEC § 5)

- Nothing but text reaches a row: escape sequences, control characters and
  bidi/format characters in the payload's names and paths, in git output,
  or in any config string are stripped before a cell is counted. A newline
  in a session name no longer adds a row, and `--color never` is plain.
- OSC 8 links are emitted only for `http(s)://` URLs of printable ASCII.
- A config integer can no longer size an allocation or a loop on every
  tick: `width`/`pad`/`bar_width` above 1024 cells, `text`/`gap`/`ticker_gap` above
  4096 characters, a `*_step` outside `0.001..=1000` and a `fill_char` that
  is not one cell are reported and defaulted; the renderers clamp again.
- A TOML syntax error keeps the command-line overrides (`preview --color
  never --icons ascii` of a broken file renders plain ascii), like an
  unreadable file already did.
- `install`, `config init`, `config path` and `skills install` refuse to
  guess a home directory when `HOME` is unset instead of writing into the
  current directory; `config init` and `config path` honour `--config` and
  `GARNISH_CONFIG` first.
- A one-cell ascii box still shows its clip mark (`.`).

**Layout** (SPEC § 4.1)

- `right_justify = "end" | "start"`, `hide_empty_lines`, spacer lines
  (`modules = []`, kept as an empty framed row), `bar = "blocks" | "line"`
  on the bar modules.
- `blank = true` on a spacer keeps it on screen without a frame: the row
  gets one invisible cell, since Claude Code drops whitespace-only rows
  (with colour off; the rule's colour codes keep it otherwise). Off by
  default.

**Text and motion** (SPEC § 3.7, 4.2)

- `text.<name>` modules: static text with `width`, `pad`, `justify`,
  `overflow = clip | scroll | scroll-wrap`, `step`, `gap`, `color`.
- Line ticker: `overflow = "ticker"` scrolls a line that does not fit,
  `ticker_step`, `ticker_gap`. Under a ticker the timers default to
  `durations = "fixed"`, so the window slides instead of jumping when a
  `compact` duration changes width; `durations = "compact"` at the top
  level opts back in, and every timer module (`session`, `api`, `cache`,
  `limit5h`, `limit7d`, `spend`, `sync`) has its own `durations` to pin one.
- Animation: `animate` (and `GARNISH_ANIMATE=0`), frame rule
  `fill_pattern`/`fill_step`/`fill_direction`, `separator_frames`/
  `separator_step`, `<icon>_frames` on any icon (`spinner_frames`,
  `model_frames`, …). Everything sits on frame 0 when animation is off,
  and a ticker line is then cut with `…` rather than frozen mid-scroll.

**Presets gallery** (SPEC § 12)

- `presets/*.toml` are embedded in the binary: `garnish presets` lists them,
  `garnish config init --preset <name>` writes one, `docs/presets.md`
  shows every preset rendered at its declared width.

**Skills** (SPEC § 13)

- Three Claude Code skills ship with garnish and are written to
  `~/.claude/skills/` by `garnish install` (or `garnish skills install`;
  `--no-skills` to skip): `garnish-statusline` builds a config
  interactively, `garnish-feedback` files an issue with everything a
  maintainer needs, `garnish-submit-preset` proposes a gallery preset.
  Matching issue templates live under `.github/ISSUE_TEMPLATE/`.

## 0.1.0 — 2026-09-05

First release: the 21 modules, presets, themes, icon sets, frames, the
cache and detached workers, `install`, `doctor`, `preview`, `config`, the
generated docs and the golden suites.
