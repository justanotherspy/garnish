# WORKLOG.md — what was built, found and decided, by date

The dated log of the codebase, moved out of `PLAN.md` on 2026-09-19. One
entry per date: what landed, what the reviews found (by class), what was
decided and why, and the lessons behind the rules in `CLAUDE.md`. Not how
a session went; host trouble belongs in the host's notes. Oldest first.
Compacted on 2026-09-12 and again on 2026-09-26.

- **2026-09-04** — Research (statusline contract, autocompact internals,
  the namtao toolkit); spec and plan approved. Phases 0–9 in one day:
  scaffold, payload/time/ansi/num, schema-driven config, all 21 modules,
  cache and workers, git reader, docs generator, install/doctor, benches,
  hardening. Three adversarial reviews found hangs and loops (a
  symref-cycle stack overflow, a pipe-buffer deadlock in the worker, an
  untimed fetch), cache churn (fetch failures poisoning `sync`, failed
  entries respawning every tick, a handed-over lock that looked dead the
  moment the tick exited: fixed with a grace window and re-stamping) and
  `install` damage (widened permissions, colliding backups, replaced
  symlinks). Decided: `command-run` dropped, it has no kill-on-timeout;
  role overrides live under `[colors]`, not `[theme.colors]`. `v0.1.0`
  tagged; CI on Linux and macOS (macOS found the Linux-only lock hand-over
  asserted in tests and `/var` vs `/private/var`). Daniel's first
  feedback: a Nerd Fonts v3 glyph drew as a box (BMP private use only
  since), the fetch age dimmed every fifth tick (new `stale_after`), the
  right edge was cut.
- **2026-09-05** — CI green on both platforms; documents split by role
  (`CLAUDE.md` host-neutral, `SPRITE.md`, `PLAN.md` codebase-only); Phase
  10 host setup and the SessionStart hook. Right-edge root cause read from
  the 2.1.261 binary: the box is `COLUMNS − 4 − 2 × statusLine.padding`,
  so `Config::width` subtracts 4 and `install --padding` seeds
  `padding = 2N`. Phase 11 (`align`, `durations = fixed`), byte-identical
  by default. A live walkthrough with Daniel of every preset, theme,
  frame, icon set and option found eleven bugs (wide COSMIC glyphs, a bad
  colour discarding the whole config, powerline caps, separators, empty
  rows) and produced SPEC § 4.1, § 3.7, § 4.2, § 12, § 13, planned as
  Phases 12–18 in one `gh stack`. Phases 12, 14, 13, 15 landed (glyph
  guard, per-key fallback, line keys, `time::frame`, `ansi::scroll`, the
  ticker, text modules); the Phase 15 review found unsanitised gaps and
  text-module names breaking the `config show` round trip.
- **2026-09-06** — Phases 16–18: animation framework, the presets gallery
  (`include_str!`, generated page), the three skills and `skills install`.
  Reviews: multi-character spinner frames split, a short rule blinked,
  skill frontmatter not YAML, `skills::install` followed symlinks. A
  three-reviewer whole-stack pass hardened every row (`Segment::plain`/
  `styled` reduce to plain text, sizes bounded after `width = i64::MAX`
  aborted a tick, OSC 8 only for `http(s)://`) and made `config show`
  round-trip. Decided with Daniel: ticker durations default to `fixed`, a
  frozen ticker is cut with `…`, `blank = true` keeps an unframed spacer
  (only colour off loses it: the harness trims raw bytes). Stack #13–#42
  merged; `v0.2.0` tagged. Warm default tick 0.87 ms.
- **2026-09-11** — Release pipeline with the Homebrew tap: tag → verify →
  pre-release → four archives → cask rendered and `brew fetch`-checked →
  Daniel's approval in the `release` environment → tap push → promote. Its
  review fixed a `sha256 ""` from a failed substitution under `set -e`, a
  per-tag concurrency group and an auto-created unprotected environment,
  and moved the approval after the cask exists.
- **2026-09-12** — *Code*: every open plan item closed: the killed-tick
  test (dash's builtin `kill` takes neither `--` nor a negative pid, so
  the `kill` binary), behind/diverged/`fetch_interval` tests against a
  second clone, `Segment.text` private, `OptSpec::max` replacing a
  key-name match (it caught `cost.decimals`, a 4 GB allocation per tick);
  SPEC audited against the code and three § 9 promises given tests.
  *Documents (PR #48)*: at Daniel's request the website was dropped and
  SPEC § 14 became `garnish setup` (ratatui, preview through the real
  render, editors from `ModuleSchema`, install through `install`);
  FUTURE-SPEC's low-impact ideas chosen by "Tier A, no crate, no non-goal,
  no tick-side write, module set unchanged" became Phases 19–20. Daniel's
  layout ideas became one model (SPEC § 4.3): a row is columns, a column
  is modules or a stack of rows, `1fr | auto | cells`, titles and boxes
  as decorations, two levels deep; a *line* is a terminal line and a
  *row* the addressable unit, so `[[row]]` with `[[line]]` and
  `hide_empty_lines` as permanent aliases, and a plain row is one `1fr`
  column so the default render stays byte-identical. Two spec reviews
  returned 25 findings each, all taken. *Phase 19* (harness fidelity):
  read from the 2.1.270 binary, the 13 000 buffer is unchanged, `COLUMNS`/
  `LINES` are the full terminal, and the harness merges `dim` into every
  piece, so FUTURE-SPEC A1's dim reset could never work and was not built
  (SPEC § 2.1 records how to re-verify). Built: `animate` following
  `prefersReducedMotion`, `install::replace_file` (never rewrite an
  unparsable file), the doctor's settings rows, `# color:` goldens. A
  five-lens review found symlink, size-bound and double-parse bugs in the
  settings reads and test leaks (the checkout's `.claude/`,
  `GARNISH_ANIMATE`, the machine's managed settings), all fixed.
- **2026-09-13** — *Backlog decisions* (PR #50): `preview` draws every row
  faint (the tick's bytes unchanged); `GARNISH_MANAGED_SETTINGS` names the
  managed file or, empty, none, and every binary-run test sets it empty.
  The height rule read from the 2.1.270 binary: three renderers; classic
  cuts nothing, fullscreen gives the bottom block `⌊LINES / 2⌋`, the
  DECSTBM split `LINES − 2`. Decided: the tick caps nothing, the § 14
  picker warns against the fullscreen budget, `doctor` prints the `tui`
  setting. *Phase 20* (per-module presentation): a four-lens trap-finding
  pass over the plan settled the corners first (fish keeps `~`, GitLab by
  host or `pr.kind = "mr"`, the ASCII pending glyph is `..` so the matrix
  asserts cut ⇒ ellipsis). Layers: `COMMON_OPTS`, `max_width` (skipped at
  0 so the default tick pays nothing), the schema matrix (about 100 000
  renders, 0.6 s under rayon), `path.style = "fish"`, `branch.link` and
  `text.url`, `context.scale = "usable"`, `reset` on the limit modules.
  Lesson: a trap-finder agent ran `git stash` in the shared checkout while
  the reset layer was half-written and three files silently reverted
  (found in `git stash list`, re-applied by hand); since then every
  subagent gets its own worktree and a brief forbidding git commands that
  touch the tree.
- **2026-09-14** — First macOS host: `make setup` died with `rustc: command
  not found` because Homebrew's keg-only rustup keeps its proxies off
  PATH. Fixed on the host; `scripts/setup.sh` now finds the proxies and
  stops with a PATH note. `make install` built 0.2.0 unchanged.
- **2026-09-16** — *Phase 20 review*: PR #51 replayed on `main` and given
  the review it had skipped (three lenses, own worktrees, no-git briefs).
  Lint policy clean; a mutation pass proved all eight new goldens real.
  Found: the settings chain read on every `context` render, `branch.link`
  building bad URLs (empty branch, an encoded port, `.`/`..` segments
  walking the path up), `ansi::clusters` splitting flags and skin tones.
  The schema matrix now sweeps every `Bool` and `Enum` option, not only
  modules. Decided: `spend`'s reset takes the date form (a reset weeks out
  read as tonight); a self-hosted GitLab without an MR is a documented
  limitation, not a `branch.forge` key. *Height-rule review* of #50 (four
  lenses; refuters died when credits ran out, judged by hand): `tui`
  values Claude Code rejects are shown as written, the renderer is "asked
  for" not asserted, `⌊LINES / 2⌋ − 5` is the ceiling for an empty prompt
  rather than the rule, and `CLAUDE.md`'s re-verify anchors became quoted
  strings, not minified names. Recorded: a failed, timed-out (600 s) or
  empty run clears the status line. #50 and #51 merged.
  *Audit through Phase 20* (seven read-only lenses, 194 → 227 tests):
  two ways out of the repository (a `ref: ../../../secret` HEAD read any
  file; a `.git/config` remote named `--upload-pack=<cmd>` ran it); four
  unbounded things (a pipe read outliving the timeout behind an ssh
  `ControlPersist` master, future stamps making locks and entries live for
  ever, whole stderr cached, temp files left behind); empty env vars not
  treated as unset (`XDG_CONFIG_HOME=` made a checkout's file the config);
  nine wrong renders (a failed module losing its `✗`, `rgb_to_256` ignoring
  xterm's cube levels, and so on); rules spelled per module consolidated
  into one helper each (`lead`, `badge`, `glyph_prefix`, `cut_name`,
  `IconSet::ellipsis`); five test blind spots (`Clock::fixed()` hides the
  repo group, so `sync` was in no golden or bench; git helpers ran under
  the developer's `gpgsign`). CI gained the shellcheck gate and `contents:
  read`. The review of the audit found four regressions it had introduced,
  the key one: consolidating the frame-list rule rejected
  `separator_frames = []`, the line every `garnish config init` has ever
  written, so every existing config would have printed `⚠ config:` on
  every tick, and the docs-sync test had been "fixed" by changing the
  generator to hide it. Also a bounded pipe read returning `Ok("")` that
  `is_dirty` read as clean, and five fixes that stopped at the example
  (`joinable_ref` is a name rule where the threat is a symlinked path;
  `fetch_error` and `fetch_attempt` missed the bound and the future-stamp
  rule). Lessons into `CLAUDE.md`: fix the shape, not the example; check
  what is on disk before making a rule stricter. A fourth pass over the
  branch's claims found `packed-refs` outside the symlink rule and a
  working fetch recorded as failed.
- **2026-09-17** — *The review workflow*: PR #66 merged; its own review
  fanned out to four subagents, all refused, and ended green after 44 of
  50 turns having posted nothing. Decided then (reversed on 09-19): allow
  `Task`, cap turns at 100 since subagents spend from the same budget (an
  earlier cap of 15 left a two-file review unwritten). Workflow fixes go
  in alone because the action refused to run on a branch whose workflow
  differed from `main`'s. *Phase 21* (the layout model): `[[row]]` with
  the aliases kept (old fixtures stayed on the old names to pin them), the
  config model three levels deep, and `src/layout.rs` replacing
  `frame::compose_line` outright; all 227 goldens came out byte-identical
  on the first green run, which is the proof. Building found six bugs
  (column pads, emptied stacks, `truncate = false` on every column, a
  one-cell text module losing its link twice) and two copies worth
  removing (in-process tick 85 → 71 µs). Decided into SPEC § 4.3: caps
  chosen over the lines that carry them, a box's pad is the frame's or one
  cell, a box's edges do not animate. Reviews: a `debug_assert!` on the
  render path (a wide title exited 101 and cleared the line), six wrong
  renders, the `fr` remainder handed out per weight not per column; a
  mutation pass found ten rules with no test (now six unit tests, six
  config goldens) and two tests too weak to see their rule. Box runs now
  join anywhere in the tree; a tall row takes the room of its widest cap
  pair. 227 → 248 tests.
- **2026-09-18** — A nightly roll turned every branch red: clippy's
  `map_unwrap_or` widened to `map(_).unwrap_or_default()`. Fixed rather
  than pinned (mechanical, and a pin needs lifting again). Lesson: CI
  installs a fresh nightly while a container's is whatever its image was
  built with (four days behind), so `make check` passed on code CI
  rejected, and a grep missed a third site in `#[cfg(test)]` that only the
  lib-test target compiles: `rustup update nightly` first, then one
  `cargo clippy --all-targets`. The tell of a roll is failing lines
  byte-identical on `main`.
- **2026-09-19** — *The review workflow, seven runs to the first that
  worked* (about $10, four fixes, three of four written explanations
  falsified by the next run). A review "succeeded" with 10 denials and no
  output; `scripts/review-denials.sh` now reads the action's execution
  file and fails on a refusal or on a run with no summary, printing verbs
  only (a public log), from the base branch's copy since the checkout is
  untrusted; the checkout at `fetch-depth: 1` had no merge base. The next
  runs showed an allowlist of verbs cannot work: it matches on a prefix
  (`git --no-pager diff` is not `git diff`) and refuses a compound command
  even when every part is allowed (`git → wc`). Five runs had died
  obtaining or slicing a diff through `Bash`, so the job now writes the
  diff to files in plain shell and the prompt points `Read`/`Grep` there;
  denials fell to three. The sixth still posted nothing: the parent
  stopped with a `Task` subagent mid-flight, six for six, despite three
  prompt rules against it, so `Task` was removed (an instruction the model
  does not follow is not a control). The seventh worked: 35 turns, $1.37,
  zero denials, and still went red because the check missed a summary
  written into the tracking comment (now: the last tracking write with no
  `- [ ]`). The `concurrency` group belongs to the job: a workflow-level
  one is claimed when a run is created, so a run the review's own comment
  triggered cancelled the review.
  *Phase 22 and the consolidation (PR #78)*: `garnish setup` built over
  `src/setup/` (5 600 lines). Decided (SPEC § 14): the draft is the file's
  `toml::Table` with its order kept, never a resolved `Config`, so a save
  writes only what the file and the edits carry; glyph suggestions are one
  table; `Line::modules()` is the placement map; `install::Steps` is shared
  with the CLI; `setup --preset` is the scriptable twin; no `setup` cargo
  feature (binary 2.8 → 3.4 MB, cold tick unchanged). The icon suggestions
  had shipped empty strings where Nerd Font glyphs belonged (an editor
  dropped the raw private-use characters, and the guard skipped them); now
  `\u{…}` escapes and a test refuses an empty one. Setup goldens show the
  temporary home as `~/…`. Nine gallery presets (28 in all); the slide
  check now applies to the line ticker alone. The review workflow collects
  the PR in job shell and allows the file tools, `Bash` whole and the
  GitHub MCP tools; `WORKLOG.md` split out of `PLAN.md`. CI's newer
  nightly flagged `map_unwrap_or` again; reproduced with a dated nightly
  beside the pinned one. Three reviews (correctness, documents, 39
  mutations) found edit-safety bugs (a preset load over an unparsable file
  then saved over it, a loaded preset counted as clean, builder edits
  bypassing the parser, orphaned `[box.<name>]`), hit-testing against a
  model rather than the drawn line, terminal-guard leaks and narrow-screen
  cuts; all fixed with tests. 275 → 284. The PR's review check went red in
  13 s: the action skipped a PR that edits its workflow and `main`'s script
  took the empty argument as a usage error (now reported, exit 0).
  *Phase 23, usage views and formats* (after #78 merged): A4 hide lists,
  A6 number formats, pace and elapsed, separator colour, and four module
  ids (`version`, `sandbox`, `voice`, `account`; 21 → 25), all payload or
  settings reads, no crate, no existing golden moved. Decided: `hide` is
  hand-parsed with a vocabulary from the schema's `measure`, applied once
  in `render_group`; `[format]` with per-module `inherit`; pace is
  arithmetic over `resets_at` (`spend` has no window, so no pace);
  `separator_color = "inherit"` takes the first coloured segment;
  `account` is the first cached module outside the repo group, so
  `Clock.workers` gates `Ctx::cached`; `sandbox.enabled`/`voice.enabled`
  verified in the Claude Code docs; `itertools` dropped with its last use.
  Reviews: `precise` percentages compared a different rounding than they
  printed (one `shown` rounding now feeds both), pace kept rendering past
  a reset, `preview` spawned workers per fixture, a FIFO settings file
  blocked the tick (`open_regular`); 55 of 64 mutants killed, survivors
  given tests. 284 → 312 tests.
- **2026-09-20** — Setup refinements after Daniel's first use. A throwaway
  harness walked every preset and form field with `→`, `←`, `Enter` and
  found six bugs the snapshots missed (values trimmed so a picked
  separator lost its spaces, role names offered where the parser takes
  literals, keys offered that the parser refuses, orphaned boxes, a value
  that left another key reported shown as plain "set", `custom…` opening
  empty); the harness was not kept, its findings became tests. Built:
  undo/redo over a history of draft tables taken around every input, so
  new edit paths are undoable by construction; dirty is a comparison with
  the saved table, not a flag; clickable hint bar; column and box
  shortcuts (`C`, `]`/`[`, `B`). One review (own worktree, no git) found
  `B` refused on titled rows, orphaned boxes (`prune_orphan_boxes`, one
  place) and forms editing phantoms after undo. CI's nightly flagged
  `map_unwrap_or` again, fixed beside a dated toolchain. 312 → 319 tests.
- **2026-09-23** — Dependency sweep, no code change: `renovate.json`
  extends the shared `local>justanotherspy/renovate` preset, `cargo
  update` refreshed eleven transitive crates (every direct one already
  newest), `claude-code-action` 1.0.231 → 1.0.233.
- **2026-09-25** — Whole-codebase review at Daniel's request: 12 area
  reviewers, each followed by an adversarial verifier; 287 findings, 250
  confirmed (2 high, 34 medium, 158 low, 76 nit). Classes: git reads
  unguarded (a FIFO `HEAD` hung every tick, `git status` ran filter
  drivers, `commondir`/`gitdir:` escaped containment, `git` looked up
  after the chdir); promises never implemented (GC sweep, `fetch_error`,
  the reftable fallback); the wrong config file written (worker without
  `--config`, `CLAUDE_CONFIG_DIR` ignored); one wrong-typed payload field
  blanking every row; the harness trimming every row so leading spaces
  slid left; setup edits refused or lost; the review workflow putting a
  `contents: write` App token in reach of the model's Bash with any
  commenter's text in the prompt. Decided with Daniel: the dirty check is
  plumbing (`diff-index --cached` + `diff-files`, `checkStat` pinned),
  accepting a touched-but-unchanged file as dirty; rows starting with
  whitespace are held against the trim (empty SGR or U+2800); a non-zero
  `refresh` on a payload-only module is a problem; reftable repositories
  fall back to the worker; `context.colors.percent` paints the
  percentage; a `[frame] pad` string is drawn as its text; an all-hidden
  render still clears the line (documented); a middle column in a box
  drops its fill-cell reservation when `gap` ≥ 1 (one golden moved,
  `box-columns` shows `42%` for `4…`); the `account` worker retries once
  after a parse failure, since Claude Code can truncate and rewrite
  `.claude.json` in place (read from the 2.1.282 binary); workflow fixes
  in their own PR (#85) so the
  code PR (#86) is reviewed by an unchanged workflow; three tightened rules
  an old garnish file can trip stay problems, knowingly against the
  tightening rule, with an upgrade note in the CHANGELOG. Built as one
  batch per concern, each a subagent in its own worktree. Lessons: a test
  that read the clock before the event it measured failed one run in six;
  `make check` skips rustdoc `-D warnings`; the `config show` round trip
  crossed nextest's 60 s on macOS (now parallel); the gallery test missed
  four blank frames (each motion promise now checked alone and proved able
  to fail), and the fixing agent's own Edit tool turned `\uXXXX` into the
  glyph; a test build panicking on undeclared key reads caught a live case
  the source scan missed (`spend` reading pace switches it does not
  declare); with `RUST_BACKTRACE=1` a quiet refusal spends 0.7 s on an
  unprinted backtrace (backlog). Merge conflicts between batches were
  resolved by hand and the generated docs regenerated. 319 → 501 tests;
  `make bench` warm mean 2.5–2.6 ms, cold 4.6 ms. The final review (four
  reviewers) found the environment scrub set alone stopped the CLI
  starting (isolation now switched on whole), a failed reftable worker
  respawning every tick, a big `.git/config` costing 5 ms a tick, commands
  ignoring the `--config` the status line passes, `"rate_limits": []`
  read as a subscription, and a stderr nobody reads blanking the line;
  on #85 the token still reached `.git/config`, the report step ran git in
  the model's checkout, and the edit tools were only left off the
  allowlist. Kept on purpose, in PLAN's backlog: the three tightened
  rules, absurd durations, the per-row cap pad, context-refused picker
  entries.
  Verifying the fixes (a verifier per area, two skeptics per claim, 18
  confirmed, 3 refuted): pinning `core.trustctime=true` showed a clean
  tree dirty for good (reverted); `Task` left off the allowlist still ran;
  the report step's fallback turned API failures green; `warm-bigconfig`
  added to the bench. Verifiers' scratch builds (2.7 GB each) filled the
  disk twice. 501 → 538 tests.
- **2026-09-26** — A second verification pass over the first's fixes (one
  verifier per group, two skeptics per claim). `main` (Renovate #87,
  claude-code-action v1.0.235) merged into #85 and #86. #85: every earlier
  fix held (reproduced through the action's own parser, the pinned SDK
  and CLI, a mock API); the action drops a `#` line in `claude_args`
  (CLAUDE.md had said otherwise). `Skill` still forked a
  subagent and `Workflow`, `CronCreate`, `ScheduleWakeup` were still
  offered, because a tool left off the allowlist is not removed; now
  `--disallowedTools`, held by `scripts/test-scripts.sh`. Config location,
  eleven verifier rounds: the writers had started following a checkout's
  own `.claude/` settings, so a cloned repository chose where `config
  init` and `setup` wrote. Decided (conservative, Daniel's to revisit): a
  `--config` from a settings file that is not the person's own is followed
  by no command and refused on one line; every command names the same
  file (splitting readers from writers made `config path` wrong for one
  side). Rounds then closed a checkout's `env` block setting
  `GARNISH_CONFIG` or the managed hook (guessing which file set it failed
  three ways, so inside a session the variable counts only when the
  person's own settings set it), a relative `CLAUDE_CONFIG_DIR` or
  `--config`/`GARNISH_CONFIG` (ignored by the tick), `install` reading the
  managed layer past the demotion, `shell_words` splitting at Unicode
  whitespace, and `managed-settings.d` drop-ins. Letting the hook's file
  bring drop-ins was tried and undone (a hook file in `/tmp` took anyone's
  drop-ins; guarding the directory refused a group-writable platform one);
  the chain's decisions became pure functions tested without the platform
  directory. Rounds 9–11 found nothing reachable from a session and the
  last fix removed code, so the rounds stopped. Accepted as nits (PLAN):
  a halfway number, serde_json's inexact floats and `1e999`, bash's
  `$'…'`. Mutations only a platform managed file could show survived the
  whole suite until that logic took its paths as parameters and was
  unit-tested pure (`demote_hooked`, `layer_of`); one platform-guard test
  was vacuous because its case named another file. `make bench` at
  9417c5d within budget (warm 2.13–2.37 ms). Lesson: two macOS runs
  failed on tests asserting text that named a temp file (a macOS temp path
  is about 60 characters to Linux's 15); the suite now passes under a
  136-character `TMPDIR`. Layout (fuzz of 20 000 seeds × 211 widths,
  custom frames included): under uneven `custom` caps a row's height was
  measured on the frame's narrowest cap pair and laid out on its own caps,
  so a box was drawn and then cut (19 seeds); `frame_plan` now measures
  each row on the caps it lands on until nothing moves, and a box is
  drawn only whole (dashboard tick 70 → 64 µs). `make bench` at 5e95caf:
  warm means 2.57–2.89 ms (p99 3.2–4.4 ms), cold 4.5 ms, refresh-sync
  15.2 ms. *Documents slimmed*: `CLAUDE.md` cut to its rules (the stories
  stay here), `SPEC.md` to the design without provenance, `PLAN.md` to
  terse backlog lines, this log compacted, and `README.md` rewritten as the
  happy path (install, `garnish setup`, start Claude Code) linking to the
  guide and reference. Decided: section numbers in `SPEC.md` and the
  `CLAUDE.md` headings code cites stay fixed.
