# Changelog

User-visible changes per release. The tag message for a release is this
file's section for it. `WORKLOG.md` holds the day-by-day detail.

## Unreleased

**Usage views and formats** (PLAN Phase 23)

- `hide = ["zero", "below:10"]` on a module names the states in which it
  leaves its row (`empty`, `zero`, `below:N`, `above:N`; each module page
  lists the ones it takes); `hide_when_empty` stays as the alias of
  `empty`.
- `[format]` picks how numbers print (`tokens`, `percent`, `cost`), with
  the same-named option on each module that prints that kind, and
  `parens = "dim"` draws every parenthesised detail in the muted role.
- `limit5h` and `limit7d` gain `pace` (`⇡14%` ahead of the window, `⇣32%`
  behind), `pace_colors`, `eta` (`⇥ 1h37m` until the window is spent, only
  when that comes before the reset), `reset = "elapsed"` (`⏱2h46m/5h`)
  and `elapsed_marker` on the mini bar.
- `[frame] separator_color = "inherit"` paints each separator in the
  colour of the module before it; a role or a literal colour fixes it.
- Four modules: `version` (the Claude Code version, dim), `sandbox` and
  `voice` (badges while `sandbox.enabled` and `voice.enabled` are on in
  the settings chain; `style = "word"` adds the word), `account` (the
  email from `~/.claude.json`, or `$CLAUDE_CONFIG_DIR/.claude.json`, read
  by a worker every ten minutes; `style = "user"` for the part before
  `@`). `garnish doctor` lists the two settings keys with the others.
- Four gallery presets show them: `pace-and-eta`, `precise-numbers`,
  `quiet-when-idle`, `session-badges` (32 in all).

**`garnish setup`** (PLAN Phase 22)

- A full-screen setup in the terminal. The **preset picker** shows the
  four built-ins and the gallery, each rendered live at your terminal's
  real width, and warns when the terminal is narrower than the preset
  wants or shorter than Claude Code's fullscreen renderer allows; `Enter`
  writes it and offers the install step when Claude Code has no
  `statusLine` yet. The **builder** edits the config file in place: rows,
  columns, stacks, titles and boxes from single keys, a module picker with
  fuzzy and initialism search (`sn` finds `session_name`), an editor for
  every module and top-level key generated from the same schema the
  reference is, glyph and colour pickers with suggestions, and a preview
  you can click to select and edit a module. `s` saves only the keys you
  set, in the file's own order, keeping the previous file as a backup; a
  file that does not parse is never overwritten, and a file changed on
  disk since it was read asks before it is replaced.
- `garnish setup --preset <name> [--install]` writes a preset without
  opening the screen. A bare `garnish` typed at a terminal prints a
  pointer at `setup` instead of waiting for JSON (`garnish render` always
  reads stdin).
- Each module page lists alternative glyphs under *also try*: the same
  list the glyph picker offers.
- The builder has **undo**: `u` takes the last edit back and `U` puts it
  again (up to a hundred; `Ctrl+Z`/`Ctrl+R` too, inside a form as well),
  the key hints at the bottom of every screen are clickable buttons, and
  a draft whose edits are all undone is not asked about on quit.
- Columns and boxes from fewer keys: `C` leaves the cursor on the new
  column so `m` fills it, `]` past the last column (or on a plain row)
  makes a column for the module, `m` on a row of columns lands in its
  last column, and `B` boxes a row together with the row above (a run of
  rows becomes one titled box a key at a time).
- Fixes from a walk of every preset's forms: a picked separator kept
  only its glyph (its spaces were trimmed away, so `  ` became `""`);
  the `[colors]` form offered role names the parser refuses; `blank` and
  the title keys were offered on rows where no value is legal; unsetting
  a row's `box` in its form left an orphaned `[box.<name>]` reporting on
  every tick; a value that silenced another key (`fill = false` under a
  `fill_pattern`) was accepted without a word; `custom…` started from an
  empty line instead of the current value, and the input line had no
  cursor. The `label` picker now starts with the module's own name, and
  changing `preset` on the top-level form swaps in that preset's rows
  when the rows were still the old preset's.

**Layout: rows, columns, stacks, titles and boxes** (Phase 21)

- A config is a list of **rows** and a row can be several lines tall.
  `[[row]]` is the name; `[[line]]` and `hide_empty_lines` stay accepted
  for ever, so every config on disk renders byte for byte as before.
- **Columns**: `[[row.col]]` puts columns side by side, sharing the width
  by `width = "<n>fr" | "auto" | <cells>` with `gap` between them;
  `justify` defaults to the column's position, so three bare columns read
  left / centre / right. Content is cut to its own column and never spills.
- **Stacks**: `[[row.col.row]]` makes a column a stack of rows; `valign`
  places a shorter one.
- **Titles**: `title`, `title_justify`, `title_pad`, `title_color` set
  text into a row's rule; a row with only a title is a titled spacer.
- **Boxes**: `[box.<name>]` frames a run of adjacent rows, or a whole
  column, with its own corners and sides; `box = true` boxes one row. A
  box takes its style from `[frame]` unless it names one.

**Per-module presentation** (Phase 20)

- `max_width` on any module cuts the whole module to that many cells with
  `…` before the columns are aligned, so one long value cannot push the
  rest of the row off. The common keys (`label`, `prefix`, `suffix`,
  `hide_when_empty`, `max_width`) are on every module page with their caps.
- `path.style = "fish"` abbreviates every directory but the last
  (`~/p/garnish`). `branch.link = true` links the name to the branch on
  the forge; a text module's `url` links its box.
- `context.scale = "usable"` measures the bar against the auto-compaction
  threshold, so 100 % is where compaction runs.
- `reset = "absolute" | "both"` on `limit5h`, `limit7d` and `spend` prints
  when a window resets (`⏱14:30`, `⏱Tue 14:30`, `⏱Mar 1`), alone or after
  the countdown.

**Harness fidelity** (Phase 19)

- `animate` left unset follows Claude Code's *Reduce motion* setting; an
  explicit value wins, `GARNISH_ANIMATE=0` over both.
- `garnish install` and `config init --force` never rewrite a
  `settings.json` or `garnish.toml` that does not parse, keep a backup of
  what they replace, write through symlinks and keep permissions.
- `garnish doctor` lists Claude Code's settings files for the current
  directory and the keys that change what the line can show
  (`statusLine`, `refreshInterval`, `hideVimModeIndicator`,
  `disableAllHooks`, `prefersReducedMotion`, `tui`), resolved as Claude
  Code resolves them, and suggests `refreshInterval = 1` or
  `hideVimModeIndicator = true` when the config calls for them.
- `garnish preview` draws its rows faint, as Claude Code draws every
  status line row (verified in 2.1.270: nothing a command prints can undo
  it), so a theme is judged at the intensity the screen gives it.
- The guide says how many rows fit: Claude Code's fullscreen renderer
  gives the prompt box and the status line together at most half the
  terminal and cuts a taller status line from the bottom; the classic
  renderer scrolls instead.
- `GARNISH_MANAGED_SETTINGS` names the managed settings file, or, empty,
  says there is none.

**Gallery and skills**

- Nine more presets (32 in all with the four above): `titled-sections`, `links-and-shortcuts`,
  `compaction-watch`, `sidebar-panels`, `narrow-unicode`, `ascii-only`,
  `slow-motion`, `ticker-two-step` and `still-life` show titles, links,
  the compaction scale, cell and share widths, a boxed column, narrow and
  ASCII-only terminals, half-speed animation, a two-cell ticker and
  animation off. `docs/presets.md` renders every one at its width.
- The `garnish-statusline` skill offers `garnish setup` first and keeps
  the conversational path; all three skills are shorter.

**Fixes** (the audit through Phase 20; each with a test)

- A checkout you did not create could make garnish read files outside it
  or run commands: a `.git/HEAD` naming a ref outside the repository, a
  symlinked `HEAD`, ref or `refs/heads`, and three ways `.git/config` could
  run a command (a remote named `-…`, `core.fsmonitor`,
  `remote.<name>.uploadpack`). Every ref read now stays inside the git
  directory with a size cap, and the two settings are overridden on every
  call.
- A module whose last refresh failed lost its `✗` mark when it had no
  value of its own; a `git status` whose output could not be read reported
  a clean tree.
- `pr` underlined its number without a URL to link to; `sync` printed
  `refs/heads/main` for a local upstream; `spend` coloured its percentage
  from a value clamped to 100; `context.show_compaction_percent` needed
  the marker on; `branch.max_length` and `session_name.max_length` cut
  with `…` under `icons = "ascii"` and could split a flag or an accented
  letter.
- A bar glyph that was not one cell was silently replaced while
  `config check` said `ok`; it is reported now. Blanking a trailing glyph
  left a stray space.
- `GARNISH_CONFIG=` and `XDG_CONFIG_HOME=` (empty) misbehaved; an empty
  path variable means unset everywhere now.
- Under `color = "256"` every theme was shifted: the cube's levels are
  `0, 95, 135, 175, 215, 255`, not evenly spaced.
- A background worker could hang for ever holding its lock when something
  outlived `git fetch` (ssh's persistent connection does); a clock that
  stepped backwards froze a module's value and its automatic fetch.
- `cost.decimals` is capped at 8: the money formatter allocated one byte
  per decimal place. The size caps are part of each module's schema and
  the reference shows them.
- `GARNISH_DEBUG` writes a line per tick, as documented; the environment
  reference gained `GARNISH_DEBUG`, `DISABLE_COMPACT` and
  `GARNISH_MANAGED_SETTINGS`; `doctor` builds its environment list from
  the constants the code reads.

**Install**

- Prebuilt binaries for Linux and macOS (x86_64 and aarch64) on every
  release, and a Homebrew cask: `brew install --cask justanotherspy/tap/garnish`.
  The cask is published to the tap only after a manual approval.

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
