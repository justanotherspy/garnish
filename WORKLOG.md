# WORKLOG.md — what was built, found and decided, by date

The dated log of the codebase, moved out of `PLAN.md` on 2026-09-19 so the
plan stays a short statement of drift and backlog. One compact entry per
date: what was built, what the reviews found, what was decided, never how
the session went. Host trouble does not belong here (`CLAUDE.md` § Hosts).
Newest entries at the end.

Compacted on 2026-09-12 from the full session log; each entry keeps what
was built, what the reviews found and what was decided, not how.

- **2026-09-04** — Research (statusline contract, autocompact internals,
  the namtao toolkit), spec and plan approved. Phases 0–9 in one day:
  scaffold, payload/time/ansi/num, schema-driven config, all 21 modules,
  cache and workers, git reader, docs generator, install/doctor, benches,
  hardening. Three adversarial reviews fixed a symref-cycle stack overflow,
  a pipe-buffer deadlock in the worker, an untimed fetch, fetch failures
  poisoning `sync`, failed entries respawning every tick, a lock handed to
  a worker that looked dead the moment the tick exited (grace window plus
  re-stamping), and `install` widening permissions, colliding backups and
  replacing symlinks; `command-run` was dropped for want of a timeout.
  Deviation: role overrides live under `[colors]`, not `[theme.colors]`.
  `v0.1.0` tagged, pushed, CI added on Linux and macOS (the macOS run found
  the Linux-only lock hand-over asserted in tests and `/var` vs
  `/private/var`). Daniel's first feedback: a Nerd Fonts v3 glyph drew as
  a box (replaced), the fetch age dimmed every fifth tick (new
  `stale_after`), the right edge was cut (root cause found the next day).
- **2026-09-05** — CI green on both platforms; documents split by role
  (`CLAUDE.md` host-neutral, `SPRITE.md`, `PLAN.md` codebase-only); Phase
  10 host setup and the SessionStart hook. Right-edge root cause read from
  the 2.1.261 binary: the box is `COLUMNS − 4 − 2 × statusLine.padding`,
  so `Config::width` subtracts 4 (goldens regenerated, `install --padding`
  seeds `padding = 2N`). Phase 11 (`align`, `durations = fixed`) for
  Daniel, byte-identical by default; its review dropped empty renders from
  the column count. A live walkthrough of every preset, theme, frame, icon
  set and option with Daniel found eleven bugs (COSMIC's wide glyphs, a
  bad colour discarding the whole config, an error report from
  `config check`, unpadded powerline caps, the wrong separator at the
  unfilled join, coloured zero counts, empty rows) and produced SPEC § 4.1,
  § 3.7, § 4.2, § 12 and § 13, planned as Phases 12–18 in the order
  12 → 14 → 13 → 15 → 16 → 17 → 18 as one `gh stack`. Phases 12, 14, 13 and
  15 landed that day (glyph guard and replacement sets, per-key fallback,
  the line keys, `time::frame`, `ansi::scroll`, the ticker, text modules);
  the Phase 15 review found unsanitised `gap`/`ticker_gap` and text-module
  names that broke the `config show` round trip.
- **2026-09-06** — Phases 16, 17 and 18 (animation framework; presets
  gallery with `include_str!` embedding and a generated page; the three
  skills, `skills install`, issue templates). Reviews: a multi-character
  spinner frame split into characters, a rule shorter than one period
  blinked, the submit-preset skill's frontmatter was not YAML, the
  statusline skill wrote before previewing, `skills::install` followed
  symlinks. A whole-stack review by three reviewers then hardened every
  row (`Segment::plain`/`styled` reduce everything to plain text, bounded
  sizes after `width = i64::MAX` aborted a tick, OSC 8 only for
  `http(s)://`, a `HOME` guard) and polished config/CLI (`config show`
  round-trips, a mistyped `modules` is not a spacer, doctor collapses
  `$HOME`). Daniel's three answers: ticker durations default to `fixed`
  (module-level `durations` override), a frozen ticker is cut with `…`,
  `blank = true` keeps an unframed spacer (premise narrowed on review: the
  harness trims raw bytes, so only colour off loses the row). Stack #13–#42
  merged bottom-up, `v0.2.0` tagged. Bench: warm default 0.87 ms.
- **2026-09-11** — Release pipeline with the Homebrew tap, modelled on
  garlic's workflow and the tap's octo-sts policies: tag → verify →
  pre-release → four archives (Linux arm64 native) → cask rendered and
  `brew fetch`-checked → Daniel's approval in the `release` environment →
  push to the tap → promote. Its review fixed a `sha256 ""` from a failed
  sed substitution under `set -e`, a per-tag concurrency group, an
  auto-created unprotected environment, and moved the approval after the
  cask exists; `CLAUDE.md` § Release process records the repository state
  the workflow relies on.
- **2026-09-12 (code)** — Every open plan item closed: the killed-tick test
  (the tick as a process-group leader; dash's builtin `kill` takes neither
  `--` nor a negative pid, which the first cut hid), behind/diverged/
  no-upstream and `fetch_interval` tests against a second clone,
  `Segment.text` private behind sanitising setters, `OptSpec::max`
  replacing the key-name match (catching `cost.decimals`, a 4 GB
  allocation per tick), the docs index wording. SPEC audited against the
  code and its drift fixed (visible `render`, `--width` on `preview`,
  `band_colors`, `exceeds_200k` as a flag, the `sync` glyphs,
  `DISABLE_COMPACT`, temp entry names, no settings cache); three § 9
  promises got tests (schema completeness by source scan, the frame-style
  matrix, `preview <dir>`). Review: the scan checked one literal per call,
  `push_str` allocated per bar cell, `label`/`prefix`/`suffix` had no cap;
  macOS counted `branch` spawns without the lock hand-over.
- **2026-09-12 (documents, PR #48)** — Daniel asked for FUTURE-SPEC's
  low-impact ideas in the spec and plan, the website dropped, and an
  interactive setup in its place. SPEC § 14 became `garnish setup`:
  FUTURE-SPEC § 13's `ratatui` option made a decision, with the exact
  preview through `render_lines_at`, editors generated from `ModuleSchema`,
  the gallery first, live save, install through `install`,
  `setup --preset` as the scriptable twin, and selection in the preview
  over a placement map. Chosen from the rest by "Tier A, no crate, no
  non-goal, no tick-side write, module set unchanged": the dim reset,
  reduced motion, the doctor's settings report, never rewriting an
  unparsable file, `max_width`, fish paths, branch and text links, the
  usable context scale, absolute reset times, the schema-generated matrix
  test: Phases 19 and 20.

  Daniel's layout ideas (grid columns, titled rules and boxes, panels of
  stacked boxes) became one model in SPEC § 4.3: a row is columns, a column
  is modules or a stack of rows, `width = "1fr" | "auto" | cells`,
  `justify`, titles and boxes as decorations, two levels deep, with a plain
  row as one `1fr` column so the default render is byte-identical. The name
  came from his distinction: a *line* is one terminal line, a *row* is the
  addressable unit, one or more lines tall, so the unit is `[[row]]` with
  `[[row.col]]` and `[[row.col.row]]` beneath, and `[[line]]` and
  `hide_empty_lines` stay as permanent aliases. Two adversarial reviews of
  the spec text returned 25 findings each, all taken (`51d4b66`, `cada6ef`):
  the samples contradicted the box rules, the lock horizon could never
  fire, `path.depth` already existed, and about two dozen corners were
  defined: the `truncate = false` rule, gap-then-column clamping, the fill
  pattern phased over the row, `config show` as a fixed point,
  both-direction nesting. A read-only code map at the top of each open
  phase re-cut its layers where the code's shape demanded. Open drafts #27
  and #40 stay parked.
- **2026-09-12 (Phase 19)** — Harness fidelity, on a branch as five
  commits. The verify items came first, read from the 2.1.270 binary and
  the 2.1.261 npm package: the 13 000 buffer is unchanged in both,
  `COLUMNS`/`LINES` are the full terminal size, and the status line
  component wraps each row in `<Text dimColor wrap="truncate">` around a
  child whose Ink fork merges the parent's `dim` into every piece, so a
  leading `ESC[0m` is parsed away and FUTURE-SPEC A1's premise never held.
  The dim-reset layer was therefore not built; SPEC § 2.1 records the
  mechanism and how to re-verify it.

  The layers that did land: `animate` as `Option<bool>` following
  `prefersReducedMotion` over the settings chain (with `Clock.settings`
  gating the read so a pinned clock never touches a settings file);
  never-rewrite through one `install::replace_file`; the doctor's settings
  rows as a pure function over a labelled chain; and the `# color:` golden
  mode. A five-lens adversarial review with three refuters per finding
  found all of these, all fixed: `replace_file` turned a dangling symlink
  into a regular file; the doctor suggested `refreshInterval = 1` where
  nothing was animated and printed a settings `command` raw; an empty
  `settings.json` was "invalid" to the doctor and `{}` to `install`; a
  settings file was read without a size bound; the settings chain was
  parsed twice per tick; the CLI tests could see the checkout's `.claude/`
  and the developer's `GARNISH_ANIMATE`; `config show` folded the session
  switch into a printed config; the unit tests could see the machine's
  managed settings file. Declined: a `GARNISH_MANAGED_SETTINGS` hook for
  the goldens (a spec decision, taken the next day).
- **2026-09-13 (backlog decisions)** — Daniel took the three Phase 19
  questions as suggested and PR #49 merged mid-way; the rest went on the
  same branch restarted from `main` (PR #50, merged 2026-09-14). `preview`
  draws every row faint (`Painter.dim` through
  `Request.dim`; only `preview` sets it, the tick's bytes and goldens are
  unchanged, the colour-on golden re-pinned) and SPEC § 14 has the pane
  do the same. `GARNISH_MANAGED_SETTINGS` (SPEC § 9) names the managed
  settings file or, empty, none: `managed_settings_path` returns an
  `Option`, every binary-run test and the bench set it empty, `doctor`
  lists it, one CLI test points it at a fixture (CI's one red was that
  test comparing a `/var` path with `doctor`'s `/private/var` cwd on
  macOS; canonicalised). The height rule came from the 2.1.270 binary in
  the session's container (a five-lens read with refuters, the renderer
  and component-tree lenses decisive): three renderers, nothing cut by
  the classic one (the frame scrolls, the bottom `LINES − 1` rows stay),
  `⌊LINES / 2⌋` for the whole bottom block in fullscreen (the status
  line's last rows go first; 7 rows whole at 24 lines, 20 at 50), and
  `LINES − 2` in the off-by-default DECSTBM split renderer. Decided:
  garnish caps nothing on the tick, the § 14 picker warns against the
  fullscreen budget, § 4.3 rows need no cap of their own, `doctor` prints
  the `tui` setting with what it means; SPEC § 2.1, § 7, § 11, § 14,
  `CLAUDE.md` (with the literals to grep after an upgrade), the guide and
  README carry it. Read, not watched: a nine-line status line at 24 rows
  on a real screen would confirm the arithmetic (backlog).
- **2026-09-13 (Phase 20)** — Per-module presentation, on a branch as one
  commit per layer (no `gh stack` in the session). A four-lens
  trap-finding workflow read the plan against the code before the layers
  and its findings were taken as they came: the `branch-link` golden went
  on `worktree-session` (the only fixture with both `workspace.repo` and
  a branch the module can read without a repository on disk;
  `git-worktree` renders no branch there), the config-golden harness takes
  one `# fixture:` per file so `context-usable` became two files, the
  SPEC's fish example `g/src` contradicted `shorten` (which keeps the `~`
  at every depth) and was corrected to `~/g/src`, the marker's `⤓`
  percentage under `usable` was decided (hidden with the marker, since it
  would read a constant 100 %), the fallback keys on § 2.3's enabled state
  while `compaction_marker` governs drawing alone, GitLab is the host's
  name or `pr.kind = "mr"`, the URL is built from the untruncated name,
  a text `url` is checked by the config against the painter's rule, the
  ASCII `pr` pending glyph is `..` so the matrix asserts an implication
  (cut ⇒ ellipsis; uncut ⇒ byte-identical) rather than an equivalence,
  the unknown-option message for a text module no longer recommends the
  keys it rejects, and a fish initial is a terminal cluster. The layers:
  `COMMON_OPTS` (the common keys as bounded specs, `COMMON_KEYS` nine
  wide, `config show` and the reference printing them from the table);
  `max_width` in `render_group` after `decorate` and before
  `align_columns`, skipped at 0 so the default tick pays nothing; the
  schema matrix (about 100 000 single-module renders, 0.6 s under rayon,
  registered for nextest's longer budget); `path.style = "fish"`;
  `branch.link` and `text.url`; `context.scale = "usable"` with the
  threshold factored out of the marker; `reset` on the limit modules with
  `time::wall_clock` and `Ctx::wall_clock`. The pre-Phase-19 code map's
  line pointers had drifted, so the layers navigated by symbol. One
  incident: a finder agent ran `git stash` in the checkout while the
  reset layer was half-written, so three files silently reverted; the
  stash was found, the edits re-applied and the lesson written into
  `CLAUDE.md` § Phase protocol (worktree isolation, no git commands that
  touch the tree).
- **2026-09-14 (first macOS host)** — `make setup` on Daniel's Mac died
  with `rustc: command not found` after a clean toolchain install: the
  rustup there is Homebrew's keg-only formula, whose `cargo`/`rustc`
  proxies sit in `$(brew --prefix rustup)/bin`, off PATH, while
  `~/.cargo/bin` held dead 2022 symlinks to a `rustup-init` the formula
  no longer ships. Fixed on the host (PATH); `scripts/setup.sh` now looks
  for the proxies in `$CARGO_HOME/bin` and rustup's own bin, uses them
  for its run and stops with a PATH note instead of a bare `command not
  found`. `make install` then built and installed `garnish` 0.2.0 with no
  change needed.
- **2026-09-16 (Phase 20 rebase and review)** — PR #51 had gone stale:
  cut from `85fe8b4`, twelve commits behind `main` and conflicting in this
  file. Its nine commits were replayed on `c62c03b` (one conflict, both
  sides kept, Phase 20's entries slotted in by date) and the phase
  protocol's step 4, which #51 never ran, was done here: three adversarial
  lenses (correctness and the lint policy, SPEC conformance, tests and
  performance), each in its own worktree with a no-git brief. The lint
  policy came out clean — no `unwrap`, indexing, `as` or unchecked
  arithmetic outside tests, and no `#[allow]` in the whole change set —
  and a mutation pass proved all eight new goldens real (revert a feature,
  exactly its golden fails). What they found, all fixed: the settings
  chain was read on every `context` render where the marker used to skip
  it; `branch.link` built a link to the repository root for an empty
  branch name, encoded the host so a self-hosted forge on a port became
  `…com%3A8443`, and let `pr.kind = "mr"` point github.com at a 404
  `/-/tree/`; `percent_encode` passed `.`/`..` segments through, so a
  payload-supplied owner could walk the URL up a level; `ansi::clusters`
  split flags and skin tones, so a cap cut half a flag off (and `fish`
  abbreviated `...` to `..`, showing a path as its own parent). The
  schema matrix covered a new module but not a new option — every
  module-specific key sat at its default, Phase 20's own five included —
  so it now sweeps every `Bool` and `Enum` a schema declares (still
  0.40 s) and asserts no segment carries a link the painter would refuse.
  Two invariants that were comments became tests (no schema redeclares a
  common key; every rejected text key is refused with its own message,
  from one table rather than two copies). The `spend` window took the date
  form (`⏱Mar 1`) rather than a bare clock time, since a reset weeks out
  read as tonight; a self-hosted GitLab with no open merge request was
  documented as a known limitation instead of given a `branch.forge` key
  (SPEC § 3.1).
- **2026-09-16 (height-rule review)** — The adversarial review of PR #50
  (four lenses; most of its refuters and the height read's own died when
  the account ran out of usage credits, so the findings were judged by
  hand) landed after the merge, as a follow-up PR. Behaviour: the `tui`
  row printed any other value as if Claude Code used it, while Claude
  Code's schema takes only the two names and, outside the managed file,
  rejects the whole file for one (now `Tui::Other` keeps the value as
  written, a non-string too; the row shows the next file that sets the
  key and names what was skipped, quoted and cut with `…` through one
  `line_of` helper shared with the command row); the rows asserted the
  renderer from the setting alone (now "asks for", with the environment
  override named, and `CLAUDE_CODE_NO_FLICKER`/`CLAUDE_CODE_DECSTBM` in
  the environment section); "fresh installs get fullscreen" was more
  than the binary says (a new install's first sessions, then gates that
  default to off). Spec: the `⌊LINES / 2⌋ − 5` budget was stated as the
  rule when it is the ceiling for an empty prompt (the prompt input's
  fullscreen viewport is `max(3, ⌊LINES / 2⌋ − 5)` draft lines, so a long
  draft or a notice takes the status line's last rows first); the
  renderer choice is made before the `tui` key (`CLAUDE_CODE_NO_FLICKER`,
  a background session, screen-reader mode, tmux `-CC`, Windows over SSH,
  a crash auto-off); the classic bullet's "only" hid two more full-reset
  triggers and its "the prompt box among them" holds only while the
  block fits; the suggestions float above the fullscreen block; the hint
  line sat in two parents; "top-aligned" now says why (Yoga's default
  `justifyContent`). Documents: the guide and README said "under" the
  count that fits (now "at most", rounding down); the plan's Phase 19
  note still sent both questions to the backlog, the done row lacked the
  09-13 items, the `setup-gallery` layer lacked the height warning, the
  on-screen check left the backlog with the rule still "read, not
  watched" (restored, narrowed); `CLAUDE.md`'s re-verify anchors were
  minified names (now quoted strings and shapes, and the 2.1.270 anchor
  for the stdout trim); the CHANGELOG led with the finding rather than
  the change. Recorded from the runner lens: a failed, timed-out (600 s)
  or empty run clears the status line, and an aborted run keeps the
  previous text. Merged with Phase 20 (#51) the same day; the only
  conflict was this file.
- **2026-09-16 (audit through Phase 20)** — Daniel asked for the whole
  project up to Phase 20 to be checked against its documents, its defects
  fixed, and its duplication consolidated, so `main` is ready for Phase 21.
  A seven-lens read-only audit (config, render, modules, systems,
  documents, tests, simplification), each lens in its own worktree, found
  the work below; every defect got a test, and the suite went 194 → 227.

  **Two ways out of the repository**, both reachable from a checkout the
  user did not create (an unpacked archive, a shared directory): a
  `.git/HEAD` saying `ref: ../../../secret` made `branch` render the first
  seven characters of that file as the short SHA, because a ref name was
  joined onto the git directory unchecked (`git::joinable_ref` is git's own
  `check-ref-format` rule now, applied to every `ref:` hop); and `git
  fetch` took the remote name from `.git/config` as a positional argument,
  where `--upload-pack=<cmd>` runs `<cmd>`.

  **Four things nothing bounded.** `run_program`'s timeout covered only the
  wait: joining the pipe readers blocked until every descendant closed the
  write end, so an ssh `ControlPersist` master outliving `git fetch` left
  the worker in `read_to_end` for ever with its lock held. A stamp in the
  future (a resumed VM, NTP correcting a bad RTC) made a lock live for ever
  and an entry fresh for ever, because the negative age passed the
  staleness check and then satisfied the grace window. The existing
  live-lock test only passed *because* of that, stamping from the wall
  clock while the tick ran on `GARNISH_NOW`. A failed entry stored a
  command's whole stderr, which every warm tick then read and parsed.
  `Cache::write` left its temp file behind on any failure.

  **Empty is unset.** `config::locate` read `GARNISH_CONFIG` and
  `XDG_CONFIG_HOME` without the empty guard its siblings have, so
  `GARNISH_CONFIG=` put `⚠ config: cannot read` on every tick and
  `XDG_CONFIG_HOME=` made the lookup relative to the current directory, so a
  checkout holding `garnish/garnish.toml` became the user's config.

  **Nine silent or wrong renders.** A *failed* module built wholly from its
  cache entry lost its `✗` mark, so a broken git read as an empty row (the
  first fix took the overdue case with it, which the review caught: an
  overdue module with no value still hides, or `sync` would flash `– ⟳`
  after every idle pause); `pr` underlined its number whenever `link = true`, even with
  no URL to link to; `sync` printed `refs/heads/main` where every other
  case reads `origin/main`; `spend` picked its band from a percentage
  clamped to 100 while printing the unclamped one; `context`'s
  `show_compaction_percent` did nothing unless the marker was also on;
  `branch.max_length` and `session_name.max_length` cut with `…` even in
  the ascii set, and by `char` rather than by cluster; a bar glyph override
  that was not one cell was swapped out in silence while `config check`
  said `ok`; `[frame] separator_frames = []` was accepted where an icon's
  was reported; a non-table item in an inline `line` array renumbered every
  later line, so an error named a `line[n]` that was not the user's; and
  `rgb_to_256` split the range evenly although xterm's cube levels are
  `0, 95, 135, 175, 215, 255`, which moved whole themes under
  `color = "256"` (`#6c7086` came out a light blue-grey). A clipped text
  box of wide glyphs could also come out narrower than its `width`, which
  the new sweep over that family found.

  **One home per rule.** `IconSet::ellipsis` and `IconSet::stale_glyphs`,
  `util::cut_name` and `util::short_sha`, `modules::lead` (the `show_icon`
  preamble seventeen renders opened with), `modules::badge` (a trailing
  glyph, four of whose nine sites had dropped the empty-glyph guard and
  left a stray cell) and `modules::glyph_prefix` (the same glyph built
  into a longer string, where the leftover was a double space),
  `config::env_path`, `equal_width_frames`,
  `refuse_unparsable`, one `comment()` in place of three identical closures,
  `common_keys()` derived from `COMMON_OPTS`, `name()` on `ColorChoice` and
  `StaleStyle` where `docs.rs` had carried stand-ins. `Freshness::Failed`
  held a message its own doc said `doctor` read (it does not, it re-reads
  the entries), so it was a `String` cloned per tick and dropped; five more
  public items had no caller at all.

  **The tests had five blind spots.** Every pinned render runs under
  `Clock::fixed()`, whose `git: false` makes the repo group render nothing,
  and every fixture's `cwd` does not exist either, so `sync` and half of
  `branch` appeared in no golden at all, in no matrix case and in no
  benchmark (`render_module/sync` was timing an early return). The git
  helpers ran under the developer's `~/.gitconfig`, which on this project
  means `commit.gpgsign`. The payload goldens and the generated docs had no
  orphan check. The "unwritable cache" case was a no-op as root. And
  `Cache::from_env`'s precedence chain had no test: the one named for it
  asserted `key_hash` and `sanitize`.

  **CI** gained the shellcheck gate `CLAUDE.md` has always required (about
  600 lines of shell, none of it checked) and `permissions: contents: read`
  on `ci.yml`. An explicit `ref:` on the review workflow's checkout, whose
  three comment triggers read `main`'s tree, was written and then taken back
  out: the action refuses to run on any branch whose copy of its workflow
  differs from `main`'s, so carrying the fix here cost this very branch its
  Claude review (a green twelve-second job saying `Workflow validation
  failed`). It goes in alone, first, and is in the backlog.

  **Documents.** SPEC said the settings chain is read every tick (it is
  read once, on demand), put `sync`'s fetch-age hint in the wrong preset
  column, still named `●`/`○` glyphs its own width rule bans, credited the
  statusline skill with `config init --force` semantics it did not have
  (the skill now takes the backup), and justified the feedback skill's
  redaction with a claim `doctor` stopped making in v0.2.0. The generated
  environment table left out `GARNISH_DEBUG` and `DISABLE_COMPACT`; the
  reference documented `ticker_step` as "> 0" while the parser takes
  0.001–1000. `GARNISH_DEBUG` itself wrote only on a failed spawn although
  SPEC promised per-tick diagnostics, so the tick writes one line and
  `debug.rs` has tests (it had none). `CLAUDE.md` restated SPEC § 2.1
  paragraph for paragraph and carried run IDs, dollar figures and
  Homebrew line numbers that cannot stay true; it keeps the conclusions and
  the grep anchors. This backlog lost two items that were already answered
  and gained the shape it has now: what waits on Daniel, and what is parked.

  **The review of the audit** (two adversarial agents over the branch's own
  diff, each in its own worktree) found nineteen things, and the useful half
  of that was what the audit had got *wrong*. Four were regressions it had
  introduced. Consolidating the frame-list rule made `separator_frames = []`
  a hard error, which is the line every `garnish config init` has ever
  written, so every config in the wild would have printed `⚠ config:` on
  every tick; worse, the docs-sync test had been satisfied by changing the
  generator to comment the key out, which hid the breakage instead of
  showing it. Bounding `run_program`'s pipe read returned `Ok("")` on
  giving up, and `is_dirty` reads no output as a clean tree, so the fix for
  a hang introduced a silent lie. Fixing `decorate` for a *failed* module
  also stopped `hide_when_empty` applying to an *overdue* one, which would
  have flashed `– ⟳` on `sync` after every idle pause. The one-cell bar rule
  refused `marker = ""`, the documented way to turn the marker off.

  Five more were fixes that had stopped at the example: `joinable_ref` is a
  rule about a name where the threat is a path, so a symlinked `HEAD`, ref
  or `refs/heads` still read any file on disk; refusing a `-` remote left
  `core.fsmonitor` and `remote.<name>.uploadpack`, which the same untrusted
  file sets and git runs; the one-cell rule was bypassed by `<key>_frames`;
  `MAX_ERROR_CHARS` missed `fetch_error`, the same stderr in a *successful*
  entry; and the future-stamp rule missed `fetch_attempt`, so a backwards
  clock froze auto-fetch. Two tests were weaker than their own doc comments
  claimed: the drain test ran a fake git that printed nothing, so it passed
  whether output was delivered or dropped, and the palette test sampled four
  roles of which two are the same colour in every palette.

  The lesson worth keeping is in `CLAUDE.md` now: fix the shape rather than
  the example, and check what is already on disk before making a config rule
  stricter. Both reproductions were run before and after the fix (a
  symlinked HEAD rendering `SECRETVALUE`, a future stamp leaving the fetch
  frozen).

  A fourth pass, over the claims the branch makes rather than its code,
  found two more of the same kind and a regression from the round above.
  `run_program`'s new "a read that gave up is an error" reached `fetch`,
  which runs `--quiet` and throws its stdout away and is the one caller
  whose pipes an ssh master holds open, so a fetch that worked was recorded
  as failed: the callers that read stdout are the ones that treat losing it
  as a failure now. The symlink rule had stopped at the three shapes it had
  tests for and left `packed-refs`, the fallback every absent loose ref
  takes; every ref read goes through one bounded, contained reader, and the
  bound is also why a hostile `.git/HEAD` can no longer make a branch name
  the size of the file. And the blank-`marker` exemption was in the static
  arm but not the frames arm of the same rule.

  It also caught four documents saying things that were no longer true: the
  `⟳` mark in three places after the overdue case was put back, a 256-colour
  error bound wrong by 28 (69, not 41, brute-forced to check), a shellcheck
  gate credited with 1,100 lines of shell where there are 600, and a
  `CLAUDE.md` tripwire that covered one of the four edits it claimed. The
  tripwire now writes every common option into a config and requires it
  back out, which fails if any of the four is missed. 194 → 227 tests.

- **2026-09-17 (the review workflow itself)** — PR #66 merged, and the two
  things that stopped its own Claude review working went in after it, alone,
  because the action refuses to run on a branch whose copy of the workflow
  differs from `main`'s. Asked for the label, the review fanned out to four
  subagents, had all four refused (`Task` was not in the allowlist), and
  finished reporting success after 44 of its 50 turns and $2.43 having
  posted nothing at all. Daniel chose to allow the fanout rather than forbid
  it, so `Task` is allowed and the cap is 100, since a subagent spends from
  the same budget; the prompt now tells the review to use one subagent per
  dimension on a large diff and never to end without its summary. The
  checkout also names the pull request's head, which it had to stop doing
  when the fix was pulled out of #66 to let that branch be reviewed at all.
- **2026-09-17 (Phase 21)** — The layout model, on one branch as a commit
  per layer. `[[row]]` first, with `[[line]]` and `hide_empty_lines` kept
  for ever: the existing config fixtures stayed on the old names, so every
  golden they pin covers the alias, and a unit test parses the same file
  under both names and compares the resolved configs. Then the config model
  (`ColCfg`, `Width`, `Justify`, `VAlign`, `TitleCfg`, `BoxRef`, `BoxCfg`)
  with hand-written per-key fallback three levels deep and each of SPEC
  § 4.3's validation rules under its own path, then `src/layout.rs`.

  The engine replaces `frame::compose_line` outright rather than growing a
  second composer beside it, and the proof is that all 227 goldens came out
  byte-identical on the first green run: a row of one `1fr` column *is* the
  flex line, and the composition tests that pinned `compose_line` moved over
  unchanged. `render_rows_at` returns each configured row's lines as typed
  pieces (cap, box edge, rule, gap, pad, module, separator, title), which is
  what Phase 22's placement map reads; `render_lines_at` kept its signature.

  Six bugs, each found by writing the thing that would show it. A golden
  for mixed widths showed a rule running into a module's text, because only
  a column's *interior* was padded: a column now keeps a pad on the ends its
  content reaches and nowhere else. A `hide_empty_rows` fixture showed an
  emptied stack drawing a rule on its first line and spaces below, because
  the column had become a flex column when its rows went. `truncate = false`
  let *every* column run past the box, where SPEC gives that to the last
  one alone. A text module one cell wide lost its link, twice: `paint` threw
  away a piece whose text was empty, and the packed-row trim then threw away
  a piece that was only pad. And the benches found two copies worth
  removing (a group flattened only to be measured, every line copied again
  on its way to the painter), which took the in-process default tick from
  85 µs to 71 µs against 49 µs before the phase — 0.08 ms end to end, inside
  the 0.2 ms a change has to justify.

  Decided while building, and written into SPEC § 4.3: the frame's caps are
  chosen over the lines that carry them (an earlier wording would have put
  `╭─` on every line of a tall row); the pad rule above; a box's interior
  pad is the frame's or one cell, so a box drawn inside a `style = "none"`
  frame still has room; a box's own edges do not animate.

  Two adversarial reviews then ran, one for correctness and one a mutation
  pass over the new tests. The correctness one found a `debug_assert!` in
  `place_title` — a panic path on the render path, so a title wider than
  its box exited 101 and cleared the status line — and six renders that
  went wrong rather than badly: a column's `right` group never cut to its
  column, a boxed column narrower than its own frame drawing past itself,
  `fill = false` padding lines emitting nothing so every later column
  shifted, a box joined only by inner rows reported as unused, a boxed
  column inside a boxed row nesting, and a left title taken literally into
  the one-cell gap between two columns and cut to its ellipsis. It also
  caught the `fr` remainder: `free % Σfr` hands out a cell per *weight*,
  not per column, so two `2fr` columns at 103 cells differed by two.

  The mutation pass broke one rule at a time and reported which test went
  red. Ten rules had none: the rule pattern's phase across a line, `align`
  with columns, the choice of run for a title, an over-wide title, an inner
  row's own `title` and `separator`, a box under a shapeless frame, a row
  with no `fr` column, packed columns, a box title against the right
  corner, and a `custom` frame's box glyphs. Six are now unit tests and six
  config goldens (`columns-pattern`, `columns-aligned`, `columns-packed`,
  `box-custom`, `stack-boxes`, plus rows added to `boxes-two`,
  `columns-widths` and `stack-valign`), each checked by re-applying the
  mutation and watching that fixture alone go red. It also found two of the
  phase's own tests too weak to see their rule: the share test did not pin
  *which* columns take the leftover, and `every_line_of_a_row_is_exactly_
  the_box_width` cannot see a boxed column one cell short, because the
  row's fill absorbs it — that one is pinned by the placement map instead,
  the `BoxEdge` spans having to stand in the same cells on every line.

  Two rules were only half implemented and are finished here. A box is one
  run of adjacent rows *wherever* they are: a stack's rows now join by name
  exactly as top-level rows do, and `check_box_runs` walks the whole tree,
  a stack being a run of its own and a column's own `box` taking the name.
  And a row's columns were sized from the caps of its first line, so a tall
  row under a `custom` frame whose `last` cap is wider lost that cap to the
  recut; the row now takes the room the widest pair leaves. Three SPEC
  wordings the build proved wrong were corrected with them: the pattern's
  phase is a rule-cell index, a title takes the first (or last) run that
  can *hold* it and otherwise the widest, and a row-level `right` is
  reported only beside `[[row.col]]`. 227 → 248 tests.
- **2026-09-18 (a nightly roll turned every branch red)** — clippy's
  `map_unwrap_or` widened to catch `map(_).unwrap_or_default()`, and with
  `-D warnings` that is an error: two of them in `tests/worker.rs`, one on a
  `Result` and one on an `Option`, on `main` and therefore on every branch
  off it. Fixed rather than pinned (§ Toolchain offers both): two mechanical
  lines that clippy itself wrote, against a pin that would need lifting
  again. The tell that it is a roll and not a branch's own fault is that the
  failing lines are byte-identical on `main`.

  The session cost more than the fix did, because it was diagnosed from CI
  one error at a time: a container's toolchain is whatever the image was
  built with (here 09-13, four days behind CI), so `make check` came back
  green on code CI rejected, and a grep for the single-line form missed a
  third site inside `#[cfg(test)]` in `src/layout.rs` that only the lib-test
  target compiles. `rustup update nightly` first, then reproduce: the whole
  thing is one `cargo clippy` once the toolchains match. `clippy.toml`
  relaxes unwrap and indexing in tests, not `map_unwrap_or`, so a test is
  just as red as `src/`.
- **2026-09-19 (the review failed a third time; stopped guessing)** — the
  first review to run against the widened allowlist died the same way as
  the two before it: `"subtype": "success"`, 10 turns, $0.95,
  `permission_denials_count: 10`, no summary, no inline comments. The
  difference was the diagnosis: the job log's `SDK options:` block showed
  the whole allowlist had applied, `Task` and read-only `Bash` included, so
  the previous fix was not the thing at fault and a fourth guess at the
  allowlist would have been a guess about nothing. The action prints only
  the denial *count*; it writes the full transcript to
  `$RUNNER_TEMP/claude-execution-output.json` and exposes it as the
  `execution_file` output, and the SDK's result message carries
  `permission_denials` as `{tool_name, tool_use_id, tool_input}`. So:
  `scripts/review-denials.sh` reads that file, prints each refused call as
  the `--allowedTools` entry that would have allowed it (the verb only,
  never arguments, so a public log cannot pick up a path or a token), and
  exits non-zero on a refusal *or* on a run that posted no summary — the
  first time either silent failure is a red check rather than a green one.
  Two real defects fell out of reading the action's source: the checkout
  was `fetch-depth: 1`, so the `git diff main...HEAD` the prompt points the
  review at had no merge base and could never have worked, and the new step
  had to run the base branch's copy of the script, since the checkout is
  the untrusted pull-request head and that step holds the job's token. The
  allowlist did grow again (`TodoWrite`, the read-only git verbs, the usual
  text tools), but that part is still inference and is labelled as such in
  the workflow; the script is what replaces inference next time.

  Next time was the same afternoon, and the inference was wrong. Run
  35452476655 failed the new step — a red check, which is the point — and
  named all twelve denials: every one a `git` call, five `Bash(git:*)`,
  four `Bash(git diff:*)`, three `Bash(git fetch origin:*)`. None of the
  guessed entries were ever reached. The four refused `git diff` calls with
  `Bash(git diff:*)` already allowed are the finding: **an allowlist of git
  subcommands cannot work**, because the list matches on a prefix and git's
  flags precede the verb, so `git --no-pager diff` is not `git diff`. `git`
  is allowed whole now (the job is `contents: read`, the checkout is
  disposable, and the action already allowed `git add|commit|rm`), and the
  script additionally prints each denied command's verbs, since a line
  refused for its shape — `git diff | less` dies on `less` — is invisible
  when the denials are grouped by verb.

  That verb printing paid off on the next run (35453133607), and corrected
  the guess inside the fix it had just shipped: five denials, all
  `git diff`, and the shape line read `git → wc`. **A compound command is
  refused even when every part of it is allowed** — `Bash(git:*)` and
  `Bash(wc:*)` were both on the list. So no allowlist can buy the review a
  pipeline, and widening one further was never going to work. The review's
  whole job is to read a diff, and five runs had now died obtaining or
  slicing one through `Bash`, so the diff is collected for it: a step
  before the review writes `$RUNNER_TEMP/pr.diff` and `pr.diffstat` in
  plain job shell, with no permission system in front of it, and the prompt
  sends `Read` and `Grep` there. Neither can be refused. The fan-out went
  from encouraged to discouraged in the same pass, because the parent
  stopping while subagents were still running is how every one of the five
  ended. The script also reports `$(…)`, backticks and redirects now, since
  those hide inside a command that looks single.

  The sixth run (35458733807) is the one that separated the two problems.
  Handing the review its diff worked: it read the diffstat from the file,
  reviewed `src/layout.rs` and the `frame.rs`/`render.rs` integration, and
  the denials fell from twelve to **three** — two `ls`, one `wc`, all of
  them incidental compound lines. And it still posted nothing, stopping at
  47 turns of its 100 and $2.77 with a subagent mid-flight. So the tool
  surface was never the whole story; the parent abandoning its delegates
  is a separate failure, and it is six for six. Three prompt rules had
  been written against exactly that (never end without the summary, never
  end with a subagent unfinished, prefer to review it yourself) and all
  three were ignored. **An instruction the model does not follow is not a
  control**, so `Task` came out of the allowlist: no delegate, nothing to
  abandon, and the diff is already on disk. Same move as the diff file,
  one level up. If a run without subagents still posts nothing, the thing
  to question is whether this review is worth its cost, not which rule to
  write next.

  It did not come to that. The seventh run (35459324425) is the first that
  worked: 35 turns, $1.37, **zero denials**, and a full review of the
  layout engine, the config model and the `frame.rs`/`render.rs`
  integration, with all seven review-fix claims in the PR body traced back
  to the tests that pin them, and no blocking findings. Seven runs and
  roughly $10 to get there, across four fixes — the denial reporter, git
  whole, the diff as a file, and `Task` removed — of which only the first
  was reasoned about correctly at the time. Three of the four explanations
  written down between the third run and the sixth were falsified by the
  next run. The execution file is what made each round tractable; the
  theories about it mostly were not.

  One immediate sting: **that first working run still went red**, because
  the check was wrong. The review writes its summary *into* the tracking
  comment (`update_claude_comment`, which is what `track_progress` does),
  and `review-denials.sh` counted only `gh pr comment` and inline
  comments. A false negative is the worse failure of the two — it marks a
  good review as failed and teaches you to stop reading the check. Fixed
  by testing the last tracking-comment write for an unchecked box, since
  every checklist tick is the same call and only a summary has no `- [ ]`
  left in it.
- **2026-09-19 (Phase 22 and the consolidation, PR #78)** — Daniel asked
  for one pull request: the setup phase built fully, the presets showing
  every feature, the review workflow made simple, the skills accurate,
  and the documents brought to the code with the work log split out.

  **Phase 22, `garnish setup`**, built in one session over `src/setup/`
  (draft, form, builder, pick, preview, paint, term, fuzzy, ui, app; 5 600
  lines with tests). Decided while building, and written into SPEC § 14:
  the draft is the config *file's* `toml::Table` with its order kept
  (`toml`'s `preserve_order`), so a save writes only what the file and the
  edits carry, rather than a resolved `Config` through `config show`'s
  writer, which would have rewritten every key of a hand-written file; the
  forms are key / value / default rows with one-key actions; the glyph
  suggestions are one table in `icons.rs`; the placement map is
  `Line::modules()` over `render_tree_at`, which keeps the row index per
  line; `Painter::painted_style` gives the tick's colour rules (dim,
  `Never`, the 256 quantisation) to the ratatui spans; `install::Steps` is
  the plan the CLI and the screen both apply. Under it, `fixtures.rs`
  embeds the sample payloads once (the docs table moved out of
  `docs.rs`), the bare `garnish` on a terminal prints a pointer
  (`GARNISH_STDIN_TTY` pins the check), and `setup --preset [--install]`
  is the scriptable twin. Tests: snapshot goldens under
  `tests/golden/setup/` at three sizes with the list of expected files
  guarded, key and mouse scripts, the CLI twin end to end.

  Three things the first green run hid. The setup goldens carried the
  temporary home's path, so they could never match twice: the app now
  shows paths under the home as `~/…` (a nicety on screen too) and
  `for_test` passes the temporary home. A title on a column wrote a key
  the parser rejects; `t` refuses on a column and names the row. And the
  icon suggestions had shipped with *empty strings* where the Nerd Font
  glyphs should have been (an editor dropped the raw private-use
  characters), which the guard test skipped as "private use" because
  `all` is true of nothing; the glyphs are `\u{…}` escapes now and the
  test refuses an empty suggestion. Measured against `main`'s release
  binary: 2.8 MB → 3.4 MB, and the end-to-end cold tick unchanged (about
  2 ms either way over 200 runs), so no `setup` cargo feature.

  **Nine gallery presets** (28 in all) show what none did: titles at
  every position and a titled spacer, links and fixed-width text buttons,
  the compaction scale with line bars and wall-clock resets, a 34-cell
  boxed column with a `2fr` stack of titled rows and a bottom-aligned
  column, a 72-column unicode layout with `max_width` caps, an ASCII-only
  one with a custom `+-|` frame and `color = "never"`, half-speed
  animation, a two-cell ticker, and `animate = false`. Writing them found
  the preset test's slide check applied to every changed row of a preset
  that carried an `overflow` key, so a text module's own scroll, a
  spinner or a countdown crossing a boundary read as a bad slide; the
  check is now for the line ticker alone, and a scrolled row carries
  nothing that counts seconds (the rule is in `CLAUDE.md`).

  **The review workflow** collects the pull request in job shell
  (`pr.md`, `commits.md`, `diffstat.txt`, `files.txt`, `diff.patch`,
  `checks.txt`) and hands the action a short prompt; the file tools,
  `Bash` whole and the GitHub MCP tools are allowed and `Task` is not,
  since an allowlist of verbs refused compound commands whatever it
  carried. The denial step stays.
  This pull request cannot be reviewed by it (the action refuses a
  modified workflow), which the backlog notes. **The skills** are shorter
  and point at `setup` first. **Documents**: `WORKLOG.md` holds this log,
  `PLAN.md` is the drift (none), the done table and the backlog,
  `CLAUDE.md` keeps every rule in fewer words and gained the setup and
  preset conventions, `SPEC.md` lost its "target state" marks and records
  the Phase 22 deviations, `README.md` and the guide are written around
  `setup`, and `CHANGELOG.md` § Unreleased is the `v0.3.0` section.

  The first push went red on CI for the reason `CLAUDE.md` § Toolchain
  already named: the container's nightly was five days behind, and the
  fresh one's `map_unwrap_or` catches `map(_).unwrap_or_default()` on an
  Option at three sites in `src/setup/`, one of them in `#[cfg(test)]`.
  Reproduced with a dated nightly installed beside the pinned one (so
  the review agents' worktrees kept theirs), fixed with `map_or_default`,
  green on both.

  **Three adversarial reviews** (correctness, document conformance, a
  mutation pass of 39 mutations over the new tests), each in its own
  worktree with a no-git brief. What they found, all fixed with a test:
  `p` and the picker's `e` replaced the draft over a file that does not
  parse and `s` then overwrote it (the preset load refuses now, as the
  picker's `Enter` did); a loaded preset counted as clean, so `q` quit
  without asking and `p` replaced unsaved edits unasked (it is an edit,
  and `p` over a dirty draft asks first); `try_set` let a bad value
  through when the file already had a problem at that path; `b`, unbox
  and every builder edit wrote straight into the draft, so a nested box
  passed and an orphaned `[box.<name>]` was saved with nothing said
  (every builder edit is tried and refused with the parser's message,
  unbox drops an orphan, and a new problem an edit introduces is named
  in the status bar); a file that only names a preset opened with an
  empty row list; row-list clicks were measured against a model of the
  line rather than the line drawn (the drawn chip ranges are recorded);
  deleting the last inner row or column left `row = []` / `col = []`
  that refused every edit; `t` with an empty input wrote `title = ""`;
  a text module whose placement was refused still got its table and its
  editor; the row form's "a new box name" entry was always refused; the
  terminal guard never showed the cursor again and left raw mode on when
  entering the alternate screen failed; the picker's width and height
  warnings and the "colours off" note were appended to lines a narrow
  terminal cuts (they have lines of their own now, facts first); the
  status message promised that a save drops bad keys; the help page cut
  its longest labels and its last rows at 80 × 24; an integer typed
  above its `max` was clamped in silence (refused with the bound now);
  the geometric dot marking an override could draw two cells. The
  document review corrected a dozen § 14 wordings (the preview above the
  list, `Esc`, the hand-written top-level forms, the suggestion sources,
  the snapshot coverage, the `slow-motion` summary, the skill's `gap`
  placement and `mktemp`). The mutation pass named 25 rules with no red
  test; the tests added here cover the ones that matter (validation,
  unset, the file path after `p`, `w`, list and preview clicks, the
  second placement of a text module, the builder's edge rules, a
  scrolled line's hit test), and PLAN's backlog carries the three that
  need a helper or a spec decision. 275 → 284 tests.

  Marked ready for review with the `claude-review` label, the pull
  request's `review` check went red in thirteen seconds: the action had
  skipped itself as `CLAUDE.md` says it does on a pull request that edits
  the workflow file, and the report step then ran `main`'s copy of
  `review-denials.sh` with an empty execution-file argument, which
  `${1:?}` treats as a usage error. The step now says so and exits 0
  before fetching the script, and the script takes an empty argument as
  "no execution file"; neither is a review, and the fix reaches the
  step's own path only once it is on `main`.

  **Phase 23, usage views and formats** (the same day, after PR #78
  merged). Decided with Daniel: the Tier A ideas left in
  `FUTURE-SPEC.md` (A4 hide lists, A6 number formats, N11 + A10 pace and
  elapsed, A13 separator colour) and four more module ids (A12 `version`;
  A9 `sandbox`, `voice`, `account`), the fixed set growing from 21 to 25;
  everything payload-only or a settings read, no new crate, no process
  on the tick, and every config on disk rendering byte for byte as before
  (the golden suite is the regression test: no existing golden moved).
  Documents first (SPEC § 3, § 3.3, the new § 3.8, § 4; the phase in
  PLAN), then small signed commits on one branch, one per layer, no
  `gh stack` in the container. Decisions: `hide` is a hand-parsed module
  key whose vocabulary derives from the schema's `measure` (`empty`
  everywhere, `zero` for a count or an amount, `below:N`/`above:N` for a
  percentage, `N` at most 1000), applied once in `render_group` from the
  `Measure` a module attaches with one call, in union with
  `hide_when_empty`; `[format]` is one table with a same-named per-module
  override taking `inherit`, and `parens = "dim"` goes through one
  `detail()` helper so `plain` stays one segment and today's bytes; pace
  is arithmetic over `resets_at` and the window length (5 h, 7 d; `spend`
  has no window, so no pace keys), eta shown only when it lands before
  the reset, the elapsed cursor reusing the `marker` icon key; `separator_color
  = "inherit"` takes the first coloured, non-dim segment of the module
  before the separator; `account` is the first cached module outside the
  repo group, so `Clock.workers` now gates `Ctx::cached` (a pinned
  render never touches a cache directory; `git: false` alone covered the
  repo modules) and `Clock.settings_keys` seeds the settings badges'
  docs samples in-process. `sandbox.enabled` and `voice.enabled` were
  verified on the Claude Code docs (the settings reference; the voice
  dictation page, which also says the harness drops its own `hold space
  to speak` hint under a custom status line), and `CLAUDE_CONFIG_DIR`
  moves `~/.claude.json` with every other `~/.claude` path. `itertools`
  left the dependency list with its last use. Four gallery presets (32
  in all), seven config goldens.

  **Three adversarial reviews** of the phase (correctness, SPEC
  conformance, a mutation pass of 64 mutants), each in its own worktree
  with a no-git brief; every finding fixed with a test. The two readers
  agreed on the first: under `percent = "precise"` the bands and the
  `below`/`above` rules compared the whole-number rounding while the row
  printed one decimal (`23.5%` coloured as over `thresholds = [23.7]`),
  and `zero` on an amount read a fixed half cent rather than the printed
  `$0.004` or `$0`; one `shown` rounding per style now feeds both the
  text and the compared number, and a tie rounds the same way in both.
  Also found: `pace`, `pace_colors` and the elapsed marker kept rendering
  after a window's reset had passed (the payload keeps the old
  `resets_at`, which read as 100 % elapsed); `garnish preview` rendered
  with the tick's clock, so a config placing `account` forked a worker
  per fixture and left lock files under the fixtures' session ids (a
  preview never reads the cache or spawns now, `Request.workers`; whether
  it should also skip git and the settings chain, as the setup pane does,
  is in the backlog); `read_account` and, on the tick path,
  `claude_settings::read_file` opened a FIFO and waited for a writer for
  ever (`open_regular` refuses anything but a regular file); `account`'s
  `minimal` preset kept the icon; `style = "user"` left a lone icon for
  `@host`; a negative zero printed its sign; a hide state was not
  trimmed; the SPEC's `⇥1h37m`, the `version` table row and the `$1.2k`
  rule for `whole`, and five stale counts. The mutation pass killed 55 of
  64; six survivors got their tests (a module hidden by its list never
  prints `–`, the marker needs its own switch, a zero delta prints bare,
  the ratio's floor, the blank clause of an inherited separator, the
  parse error's suffix, two formatter edges), one was equivalent (the
  remaining clamp) and one unreachable (the order of the hide check and
  the stale mapping, now said in a comment); three single-pin kills got
  a unit test beside their golden. 284 → 312 tests.
- **2026-09-20** — Setup refinements after Daniel's first use ("some
  settings don't work", a suggested label, undo, easier columns and
  boxes), on `claude/tui-presets-settings-bugs-ajz0wh`. A throwaway
  harness drove every preset (the four built-ins and the 32 gallery
  files) into the builder and walked every form and every field with
  `→`, `←` and `Enter`, logging each status line and every parser problem
  the draft gained; it found six bugs the snapshot tests had missed:
  `SlotKind::parse` trimmed every value, so a separator picked from the
  suggestions arrived as its glyph alone and `  ` as `""` (refused as
  "not one cell" for `fill_char`); the `[colors]` form offered role
  names, which `resolve_colors` refuses (a role defined by a role has no
  ground); `blank` was offered on plain rows and the title keys inside a
  named box, where the parser refuses any value; unsetting a row's `box`
  in its form (or with `d`) left an orphaned `[box.<name>]` reporting on
  every tick, and a name made for a box the parser then refused stayed
  behind the same way; `try_set` took a value that left another key
  reported (`fill = false` under a `fill_pattern`) with a plain "set",
  the problem hidden behind that status; `custom…` opened an empty line
  rather than the value in effect. Each fixed and pinned by a unit or
  snapshot test; the harness was not kept, its checks being the tests.
  Built: undo and redo (`u`/`U`, `Ctrl+Z`/`Ctrl+R`, in a form too) over
  a history of the draft's tables taken around every key and click that
  changes one, with the list cursor and the status of the time;
  `Draft.dirty` became a comparison with `saved` (the table as read or
  last saved), so an edit undone is not an edit, which also replaced
  `set_path`/`mark_dirty` with `replace_table`; the hint bar as buttons
  (`ui::hint_cells` measures what `hints` draws; the builder's bar cut
  to what 80 columns hold, `u undo` on it); an input cursor; pickers
  opening on the value in effect; `C` inserting after the selected
  column and selecting the new one, `]`/`[` past the edge making a column
  for the module (a plain row splitting from it; a module alone in its
  column stays), `m` on a row of columns landing in its last column; `B`
  boxing a row with the row above (joining its named box, or a new one
  named after the title either row carried); the module's name, bare and
  capitalised, as the first `label` suggestion; the top-level `preset`
  swapping the rows when they were still the old preset's, saying which
  happened. The help page holds 22 entries, exactly what 24 rows show.
  Presets: every file passes `config check` and renders uncut at its
  declared width; a read of all 32 renders found nothing beyond the
  `sidebar-panels` `valign` question already in the backlog. CI's
  nightly, a day newer than the container's, flagged two
  `map(_).unwrap_or_default()` chains under `map_unwrap_or`; reproduced
  and fixed with a dated nightly beside the pinned one (CLAUDE.md
  § Toolchain).

  **One adversarial review** in its own worktree (a no-git brief; the
  permission classifier refused it a copy of the uncommitted tree, so it
  read the checkout and mirrored the files into its scratchpad), against
  the state after the hint-click bug was caught in the self-review (a
  click on `u undo` ran the undo under `input()`'s snapshot, so the undo
  itself was recorded as an edit and cleared the redo chain). Its four
  findings, all taken with tests: `B` on a row carrying a title, joining
  the named box above, was refused by the parser (a row in a named box
  takes no title) and the whole edit reverted, so the "a run of rows a
  key at a time" promise held only for bare rows (the title goes, the
  status says so); `B` on a row leaving another box orphaned that box's
  table, refused the same way (`Draft::prune_orphan_boxes`, one place,
  also behind `b` and the forms); a row's, column's or box's form left
  open across an undo edited a phantom at the same path (`row_fields`
  and its kin return nothing for a missing table, the app closes such a
  form on undo, a module's is rebuilt); picking the preset already in
  effect claimed to replace the rows. Nits: `Target::BoxWith`'s payload
  unused (now checked against the selection), a doc comment stacked on
  the wrong function. Its missing-test list was taken in part (`]` from
  a right group, `[` from a stack's inner row, `HISTORY_LIMIT`, box use
  from a column or inner row, `hint_key`, `bare_key_of`); a two-cell
  glyph under the input cursor and undo after a reload stay unpinned.
  312 → 319 tests.
- **2026-09-23** — Dependency sweep, no code change. `renovate.json` now
  extends the shared `local>justanotherspy/renovate` preset in place of
  `config:best-practices` (which the preset carries), so this repository
  gets the same grouping, automerge and crate release-age rules as the
  others. `cargo update` refreshed eleven transitive crates (the pending
  lock file maintenance plus `instability`, `lru` and `thiserror`); every
  direct dependency already resolved to its newest release, and none has
  a newer major. `anthropics/claude-code-action` 1.0.231 → 1.0.233 (no
  input changes); every other action pin is the latest release. `make
  check` and `scripts/ci.sh` green on the 2026-09-22 nightly.
- **2026-09-25** — A whole-codebase review at Daniel's request, then its
  fixes. The review was read-only: 12 area reviewers, each followed by
  an adversarial verifier. 287 findings; 17 refuted or duplicates, 250
  confirmed, 8 plausible, and 12 found by the verifiers themselves (2
  high, 34 medium, 158 low, 76 nit).
  - *What it found, by theme:*
    - **git.** The tick read `.git` with no regular-file check or bound, so
      a FIFO `HEAD` hung every tick (git-01, high). `git status` ran a
      repository's filter drivers and a partial clone lazy-fetched.
      `commondir`/`gitdir:` could name any directory, so ref containment
      contained nothing. `git` was looked up on `PATH` after the chdir.
      Quoted `.git/config` values broke `sync`, and a pruned upstream
      showed `✗` for good.
    - **Never implemented.** The GC sweep never ran, `fetch_error` was
      never shown, the reftable fallback SPEC § 6 promised was missing,
      and so were `refresh` on payload-only modules, `colors.percent` and
      SPEC § 5's internal-error line.
    - **Wrong file written.** The worker never got `--config`. install,
      init and setup wrote the XDG file over a `~/.garnish.toml` in use,
      and ignored `CLAUDE_CONFIG_DIR`.
    - **Blanked status line.** One wrong-typed payload field blanked
      every row.
    - **Layout.** The harness trims every row, not only the blank ones, so
      rows starting with plain spaces slid left. Flex columns overflowed
      their share, and `auto` columns in boxes were cut.
    - **setup.** Edits were refused when a problem's row index shifted, `d`
      deleted box and text tables, and Esc on "changed on disk" dropped
      the edits.
    - **The review workflow.** It planted a `contents: write` App token
      in reach of the model's Bash, prompted with any commenter's text,
      and any user's `@claude` cancelled a paid run. The release build
      restored a cache a default-branch job could seed, and ran an
      unpinned nextest.
    - **Tests.** render.rs unit tests used the real clock, cache and
      workers. The criterion bench spawned itself as a worker.
  - *Decided with Daniel:*
    - The dirty check is plumbing (`diff-index --cached` + `diff-files`,
      `checkStat` pinned), accepting a touched-but-unchanged file as
      dirty until the user's git refreshes its index.
    - Rows starting with whitespace are held against the trim (an empty
      SGR with colour on, U+2800 with colour off).
    - `context.colors.percent` paints the percentage.
    - A non-zero `refresh` on a payload-only module is a config problem.
    - Reftable repositories fall back to the worker.
    - A `[frame] pad` string is drawn as its text.
    - An all-hidden render still clears the line (documented).
    - A middle column in a box drops its fill-cell reservation when
      `gap` ≥ 1.
    - The workflow fixes go in their own PR (#85), so the code PR (#86)
      can be reviewed by the unchanged workflow.
  - *How it was built:* one fix batch per concern, each a subagent in its
    own worktree working test-first under `make check`, merged into #86
    one batch at a time. `config/mod.rs` and `setup/app.rs` were split
    behaviour-free before their fixes.
  - *What the fixing found:*
    - A new git test read the clock before the fetch it measured, and
      failed one run in six. That became a testing rule in `CLAUDE.md`.
    - With `RUST_BACKTRACE=1` set, a quiet refusal spends 0.7 s capturing
      a backtrace it never prints (backlog).
    - `make check` does not run rustdoc `-D warnings`, so one pushed
      commit needed a follow-up; `CLAUDE.md` now says so.
  - *Conflicts on merge:* the CLI batch renamed `settings_files` to
    `settings_chain` under the render batch's new `render::context`, and
    two batches both reworded the `truncate`/`overflow` reference rows.
    Resolved by hand, and the generated docs regenerated.
  - What is left is in PLAN's backlog under *Left open by the 2026-09-25
    review*. 319 → 474+ tests.
