# Changelog

User-visible changes per release. The tag message for a release is this
file's section for it. `PLAN.md` holds the session-by-session detail.

## Unreleased

**Audit through Phase 20** — the code read against its documents

Fixed, each with a test:

- A checkout you did not create (an unpacked archive, a shared directory)
  could make garnish read files outside it or run commands. A `.git/HEAD`
  naming a ref outside the repository (`ref: ../../../x`), or a symlinked
  `HEAD`, ref or `refs/heads` directory, made `branch` show the first
  seven characters of any file as its short SHA: every ref file garnish
  opens (`HEAD`, a loose ref, `packed-refs`, `config`) must now resolve to
  a path inside the git directory, and is read with a size cap. And
  `.git/config` could
  run a command three ways: a remote name starting with `-` (git reads it
  as an option), `core.fsmonitor` (`git status` runs it) and
  `remote.<name>.uploadpack` (a fetch runs it). The name is refused and
  the other two are overridden on every call.
- A module whose last background refresh *failed* lost its `✗` mark when
  it had no value of its own, so `sync` with a broken git looked like an
  empty row instead of a failure. A `git status` whose output could not be read
  before the timeout also reported a *clean* tree rather than a failure,
  so the dirty marker went missing with nothing to show why.
- `pr` underlined its number whenever `link = true`, even when the payload
  carried no URL to link to, so it looked clickable and was not.
- `sync` printed `refs/heads/main` as the upstream of a branch tracking a
  local branch, where every other case reads `origin/main`.
- `spend` coloured its percentage from a value clamped to 100 while
  printing the real one, so a threshold above 100 could never be reached.
- `context.show_compaction_percent` printed nothing unless
  `compaction_marker` was also on, which nothing documented.
- `branch.max_length` and `session_name.max_length` cut with `…` even
  under `icons = "ascii"`, and could split a flag or an accented letter in
  half.
- A bar glyph (`fill` or `empty`) that was not exactly one cell was
  silently replaced while `config check` said `ok`; it is reported now,
  like `frame.fill_char`, and the same goes for an animated one
  (`fill_frames`). `marker` is exempt when it is blank, which is how the
  marker is turned off. `[frame] separator_frames = []` keeps meaning "no
  animation" and is still accepted.
- Blanking a trailing glyph (`icons.dirty = ""`) left a stray space that
  widened the module and shifted any aligned column beside it. The
  `cache` countdown left two.
- `GARNISH_CONFIG=` (empty) put `⚠ config: cannot read` on every tick, and
  `XDG_CONFIG_HOME=` made the config lookup relative to the current
  directory, so a checkout holding `garnish/garnish.toml` became your
  config. An empty path variable means unset everywhere now.
- Under `color = "256"` every theme was shifted: the 6×6×6 cube's levels
  are `0, 95, 135, 175, 215, 255`, not evenly spaced, so a channel could
  be moved by up to 69 and could land 95 away from the nearest level.
- A clipped text box of wide glyphs (CJK, emoji) could come out narrower
  than its `width`, shifting an aligned column.
- A background worker could hang for ever holding its module's lock when
  something outlived `git fetch` (ssh's persistent connection does), and a
  clock that stepped backwards froze a module's value, and its automatic
  fetch, until the wall clock caught up.
- An error in an inline `line = [...]` array named the wrong line number.
- `GARNISH_DEBUG` now writes a line per tick, as the reference has always
  said; it only ever logged a failed worker start.

Also: the generated reference gained `GARNISH_DEBUG`, `DISABLE_COMPACT`
and `GARNISH_MANAGED_SETTINGS` rows and the real range for the three
`*_step` keys (`0.001`–`1000`, documented as "> 0"); `garnish doctor`
builds its environment list from the same constants the code reads, so a
hook cannot go missing from a bug report.

**Per-module presentation** (PLAN Phase 20)

- `max_width` on any built-in module cuts the whole module (label, prefix
  and suffix included) to that many cells with `…`, before the columns
  are aligned and before the line is cut, so one long branch name or
  session title cannot push the rest of the line off. The common keys
  (`label`, `prefix`, `suffix`, `hide_when_empty`, `max_width`) are now
  listed on every module page and in `config init`'s file with their
  caps; a text module is told to use `width` instead.
- `path.style = "fish"` abbreviates every directory but the last to its
  first character, the way the fish shell prompts (`~/p/garnish`).
- `branch.link = true` links the name to the branch on the forge, built
  from the repository identity in Claude Code's payload (GitLab gets its
  `/-/tree/` form; nothing without a repository or on a detached HEAD),
  and a text module's `url` wraps its box in a link. `config check`
  rejects a URL the painter would not emit instead of letting the link
  vanish on screen.
- `context.scale = "usable"` measures the bar and the percentage against
  the auto-compaction threshold, so 100 % is the point compaction runs
  and the marker is implied; it falls back to the window scale when
  compaction is off or the threshold is under a tenth of the window.
- `reset = "absolute" | "both"` on `limit5h`, `limit7d` and `spend`
  prints when a window resets, alone or in parentheses after the
  countdown, in the form that identifies the instant at the distance that
  window sits: the time for the five-hour window (`⏱14:30`), the weekday
  and time for the seven-day one (`⏱Tue 14:30`), the date for the spend
  window, which is weeks out (`⏱Mar 1`).

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
- `garnish doctor` prints Claude Code's `tui` setting and what it means
  for the line, naming a value Claude Code drops or rejects the file for,
  and the guide's troubleshooting says how many rows fit: the fullscreen
  renderer gives the prompt box and the status line together at most half
  the terminal and cuts a taller status line from the bottom, the classic
  renderer cuts nothing and scrolls. garnish itself caps nothing (read
  from the 2.1.270 binary).

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
