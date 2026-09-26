# Changelog

User-visible changes per release. The tag message for a release is this
file's section for it. `WORKLOG.md` holds the day-by-day detail.

## Unreleased

**Whole-codebase review** (2026-09-25; each fix with a test)

*A repository you did not create*

- A FIFO or an endless file under `.git` can no longer hang the status
  line: every file there is read only when it is a regular file, up to a
  size cap, even when one is swapped in after the check. A `gitdir:` or
  `commondir` counts only when it points at a real git directory.
- The dirty check never runs a repository's filter drivers: it asks git's
  plumbing, which compares stat data and never hashes file content. A file
  touched without changing now reads as dirty until your own git
  refreshes its index. A partial clone never fetches lazily, and no git
  call but `fetch` may use any transport, so a hostile partial clone
  cannot run its own `uploadpack` even on an older git.
- A branch or upstream name git cannot have written (a line break, or
  more than 4096 characters) reads as none, instead of a permanent `⟳`
  and a worker on every tick.
- `sync` no longer slows the tick for a large `.git/config`: about 2.3 ms
  instead of 5.3 ms with 5000 branch sections. Config parsing follows git
  more closely (`\` before CRLF continues a line, a byte-order mark is
  skipped, an unknown escape ends the line).
- `doctor`'s cache probe never writes through a planted link, and creates
  a missing cache root private to you.
- Workers take `git` only from absolute `PATH` entries (never a `git` the
  checkout ships), ignore an inherited `GIT_DIR` or `GIT_WORK_TREE`, and
  bound what they read from git. A background `fetch` starts no
  maintenance and no submodule recursion, and ssh never prompts on the
  terminal.
- The last-resort cache root under the temporary directory is per user
  and private; GC removes only what garnish created, never follows a
  link, and the automatic sweep now actually runs.

*Git and sync*

- A deleted and pruned upstream shows the no-upstream glyph instead of a
  permanent `✗`; upstreams whose names contain `#` or `;` count
  correctly; switching between branches that share an upstream no longer
  shows the previous branch's counts.
- The fetch-age hint counts from the last fetch that worked, and
  `garnish doctor` lists fetches that keep failing.
- `branch` and `sync` work in reftable repositories; where git fails
  there, a worker runs once per refresh instead of on every tick, and a
  branch that shares its name with a tag shows as `main`, not
  `heads/main`, with its upstream found.
- `--config` on the status line command now reaches the background
  workers, a path that is not UTF-8 included.

*Install, config location and the CLI*

- `install`, `config init` and `setup` write the config the status line
  reads: an existing `~/.garnish.toml` is no longer hidden behind a new
  XDG file. When `statusLine.command` passes its own `--config`, that
  file is the one every command uses (`config path`, `check`, `show` and
  `init`, `preview`, `doctor`, `setup` and `install`) instead of a
  default file the status line never reads; a `--config` that names no
  one file (a relative path) is refused on one line, never guessed.
  A config a project's own `.claude/` settings name (by the command's
  `--config`, or by `GARNISH_CONFIG` in their `env` block, which Claude
  Code copies into the session) is followed by none of them: a cloned
  repository never chooses a file garnish reads or writes, and each
  command says so on one line (`garnish --config FILE …` names the file
  yourself); `install --settings` on such a file writes no config it
  names. When the project runs another status line, your own garnish
  command still names your config, and `setup --install` says so when
  the command it keeps reads another file than the one it wrote. `$HOME`
  and a `GARNISH_CONFIG=` prefix in the command are read as `sh` reads
  them, a relative `CLAUDE_CONFIG_DIR` is ignored, and a settings file
  over 1 MiB no longer loses its command for the commands run by hand.
- `CLAUDE_CONFIG_DIR` is honoured for `settings.json`, the skills and the
  settings chain.
- `install` keeps a garnish command's arguments, writes `--config` when
  one is given (the flag or `GARNISH_CONFIG`), shell-quotes paths, no
  longer reorders `settings.json`'s keys, and seeds a new config's
  `padding` from `statusLine.padding`. `install --absolute` records the
  launcher on `PATH`, not a versioned Homebrew path that an upgrade
  deletes. `--dry-run` says "already up to date". A reinstall keeps an
  environment prefix (`NAME=value garnish …`, `env … garnish …`).
- Backups keep the original file's permissions and are synced to disk;
  an edited `SKILL.md` is backed up before it is replaced. A
  `settings.json` or config that is not UTF-8 is refused on one line.
- A typo'd flag in `statusLine.command` (`garnish render --bogus`
  included), or a panic, shows a `⚠ garnish:` row instead of blanking the
  status line, and a stderr nobody reads can no longer blank it either.
- An empty `NO_COLOR` leaves colour on; the boolean `GARNISH_*` hooks
  accept `true`/`false`/`yes`/`no`/`on`/`off`; a relative `XDG_*`
  directory is ignored.
- `garnish doctor` gains a `statusLine.padding` row, marks a settings
  file Claude Code rejects, and names a config it cannot read.
- `preview --theme` typos are refused on one line; `garnish … | head` no
  longer prints an error report; `garnish docs` is hidden and needs
  `--out`; `gc` says it removes idle session and repository directories.

*The payload*

- A payload field of an unexpected type drops only that field instead of
  blanking the whole status line (an array where an object belongs
  included: `"rate_limits": []` no longer hides the cost); an empty
  `workspace.current_dir` no longer hides `cwd`.
- Huge or non-finite numbers print bounded (at most `99999%` or
  `$100.0k`), never `$infk` or hundreds of digits.
- `max_length`, fish-path initials and short ids count the text the row
  shows, so a styled session name is no longer cut short or loses its
  `…`; `tput sgr0` in a name no longer leaves a stray `B`.
- `TZ` accepts POSIX rules (`JST-9`) and the `:` prefix; a zone name
  costs one file read instead of a walk of the whole zoneinfo tree each
  tick, and an unknown `TZ` is reported on stderr. `clock.tz` takes the
  same forms, and an unknown one is reported by `config check`.
- Under `color = "256"`, near-grey colours (every theme's frame) use the
  grey ramp instead of being lightened.

*Modules*

- `context` draws its percentage in `colors.percent` (default `text`) as
  documented; the band colour stays on the bar. `warn_at` compares the
  percentage the row prints, so `80%` with `warn_at = 80` warns.
- `context` and `effort` no longer print a leading or double space when
  an earlier part is switched off.
- With the ascii icon set, a module with no value shows `-` instead of
  `–`.
- The `lines` net delta takes the module's own `+`/`−` glyphs, and a zero
  net prints `+0`.
- The `account` worker reads a `.claude.json` caught mid-write once more
  before showing `✗`.

*Layout*

- Rows that start with spaces no longer slide left on screen (Claude Code
  trims every row it draws).
- `[frame] pad` is drawn as its text again (`pad = "·"`), boxes included.
- Every column is exactly its share: a flex column whose right group
  fills it no longer spills into its neighbour; an `auto` column in a
  box, or an `auto` stack of boxed rows, is no longer cut; an empty
  `auto` column leaves no stray rule cell; a row with no `fr` column has
  no hole before its cap; a module that exactly fits a box is no longer
  cut; a stacked row keeps its pad.
- A title on a multi-line row no longer draws rule across padding;
  `align = true` stacks right-justified columns correctly next to
  left-justified ones; a `blank` inner row adds no braille cell to a
  framed line; scrolling text and the ticker no longer skip a step on
  ligature scripts.
- Inside a box without a rule, two columns a `gap` apart keep their full
  width, so a module is no longer cut two cells early.
- A column that draws nothing (`width = 0`, or an `fr` column squeezed to
  no cells) takes no gap, so no stray rule cell follows the last column
  and no hole opens before the cap; `align = true` stacks the separators
  of a column that has a `right` group from its left end; a box too
  narrow for its corners or sides draws nothing and adds no lines to its
  row, instead of cutting the whole line to `…` or leaving two empty
  framed lines; a tall row under `custom` caps that leave a later line's
  right cap narrower or empty fills the box on every line, and its rule
  never runs into a right group's text.
- A bad payload says why on stderr and in the `GARNISH_DEBUG` log.

*Config*

- A `[row.col]` typo is reported and the row keeps its modules instead of
  becoming a blank spacer; a box on a stack inside a boxed row is
  reported (boxes never nest); `config show` writes an emptied row the
  way it renders, and leaves out a `[box.x]` no written row joins.
- Negative whole numbers in number options keep their sign, whole floats
  are written back as valid TOML, and `nan`/`inf` are refused.
- Reported now: `thresholds` out of order, `title_justify`/`title_pad`/
  `title_color` without a `title`, `refresh` on a module that renders
  every tick (and `garnish refresh --module` on one). `gap`, `title_pad`
  and count errors name their range; every colour key reports a bad
  colour with one message; `hide_empty_rows` next to `hide_empty_lines`
  no longer depends on key order; `preview --preset` no longer reports
  problems in the rows the preset replaced.
- **Upgrading:** a config an earlier `garnish setup` wrote can carry two of
  the keys now reported, and shows a `⚠ config:` row until you delete
  them: a `refresh` on a module that renders every tick (the old module
  form offered it), and `title_color`, `title_pad` or `title_justify`
  left behind when a title was removed. `garnish config check` names each
  one.

*`garnish setup`*

- `e` edits the box of the selected line from the keyboard; every form
  lists all the keys its table holds, so `d` removes an unknown or
  misplaced key, and `<key>_frames` and a text module's `color` are
  editable; frame lists are typed as TOML arrays and keep their spaces.
- The "changed on disk" question opens on *keep editing* (Esc and Enter
  no longer reload); the comment-loss warning shows when the file opens;
  `s` with nothing changed writes nothing; the picker writes gallery
  presets with their comments and asks before replacing a changed file;
  a save refuses a file that stopped parsing.
- `b` moves a box's last member out; deleting a box's last member drops
  the box; `d` keeps a box or text-module table; `x` on the last row is
  refused; builder edits are no longer refused over a problem the file
  already had; removing a row's or a box's title removes its
  `title_justify`, `title_pad` and `title_color` with it (one undo puts
  all four back).
- The home menu works at 60×12 and a too-small terminal ignores clicks
  and keys; clicks on titles, boxes, separators and the gutter open the
  right form; the preview marker is `>`, a selection inside a ticker
  line highlights only its module, and the colour-off preview draws
  plain; unset layout keys step from their value in effect, colours show
  as roles, integer inputs show their bounds, and colour pickers list
  each colour once; mouse moves no longer redraw.

*Reference and gallery*

- The `animated-dots` preset's model icon cycles through ◐ ◓ ◑ ◒ instead
  of four blank frames.
- The reference no longer breaks a table on a `|` glyph, and an empty
  glyph shows as `—`; it now documents the `[frame]` box glyph keys and
  the `GARNISH_STDIN_TTY` and `GARNISH_TEST_PANIC` hooks; the `sync` and
  `account` pages say why their samples are empty.
- *also try* glyphs and the setup picker's suggestions cover `version`,
  `sandbox`, `voice` and `account`.
- The `config init` comment on `preset` says what it still picks (each
  module's preset) and that the rows and frame written below it stay as
  written; `config show` writes custom frame glyph keys in the setup
  form's order.

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
- Four gallery presets show the layout: `grid-three`, `grid-six`,
  `boxed-panels` and `dashboard-panels`.

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
  it), so a theme is judged at the intensity the screen gives it. It no
  longer reads the cache or starts background workers, so previewing a
  payload from inside a real repository changes nothing on disk.
- A FIFO or other non-regular file in the settings chain no longer blocks
  the tick: settings files are read only when they are regular files, up
  to a size cap.
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

## 0.1.0 — 2026-09-04

First release: the 21 modules, presets, themes, icon sets, frames, the
cache and detached workers, `install`, `doctor`, `preview`, `config`, the
generated docs and the golden suites.
