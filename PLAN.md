# PLAN.md — the drift between SPEC and the code

`SPEC.md` is the target design. This file is what the code still lacks of
it (an open phase, when there is one), a compact table of what has
landed, and the backlog.
The dated log of how the project got here is `WORKLOG.md`; the rules for
working here are `CLAUDE.md`. Host trouble does not belong in this file.

## Where things stand

`v0.2.0` (2026-09-06) shipped everything through Phase 18: the 21 modules,
the schema-driven config with presets, themes, icon sets and frames, the
cache and worker model, aligned columns and fixed durations, per-key config
fallback, the ticker, text modules and animations, the presets gallery and
the three bundled skills. Phases 19–23 landed between 2026-09-13 and
2026-09-20 (Phase 23 merged as #79 on 2026-09-20, the setup refinements
as #80 the same day; the table below). The release pipeline with the
Homebrew tap waits for its first tag, which will be `v0.3.0`;
`CHANGELOG.md` § Unreleased is its section.

**There is no open phase.** A whole-codebase review on 2026-09-25 (12 area
reviewers, each checked by an adversarial verifier: about 270 findings,
2 high, 34 medium) was fixed in one pull request per concern group
(justanotherspy/garnish#86 for the code, justanotherspy/garnish#85 for
the workflows); `WORKLOG.md` (2026-09-25) has what it found and decided,
and the backlog below holds what it left open. The drift between `SPEC.md`
and the code is the short list under *When asked*; everything else in the
spec is implemented, and where Phase 22 was built differently from its
design, SPEC § 14 says so and why.

## Done

| phase | landed | when |
|---|---|---|
| 0 Scaffold | nightly toolchain, strict lints, nextest, Makefile, `scripts/ci.sh`, the four documents | 09-04 |
| 1 Payload, time, ANSI | `payload.rs`, `time.rs` (`GARNISH_NOW`), `ansi.rs` (width, OSC 8, `…` truncation), `num.rs`, `preview`, the golden harness (`UPDATE_GOLDEN=1`), the payload fixtures | 09-04 |
| 2 Schema, config, frame | `ModuleSchema`, config model with TOML-path validation, four icon sets, six themes, seven frame styles, `config init/check/path/show`, four built-in presets, the render matrix and config-driven goldens | 09-04 |
| 3 Payload modules | 16 payload-only modules incl. `context` with the autocompact chain, `⚠` failure rows, `GARNISH_DEBUG` | 09-04 |
| 4 Cache & workers | `cache.rs` (atomic entries, TTL, GC), `spawn.rs` (detached workers, hard-linked locks, `GARNISH_NO_SPAWN`), `refresh`; serial tests incl. 32 ticks → one worker and the killed-tick test | 09-04 |
| 5 Repo modules | `git.rs` direct `.git` reads, worker ahead/behind/dirty/fetch, `path branch sync worktree pr`, temp-repo and PATH-shim tests | 09-04 |
| 6 Docs | `garnish docs` → `docs/`, hand-written guide, `garnish modules`, docs-sync test, `config show` fully resolved | 09-04 |
| 7 Install, doctor | `install` (merge, backup, `--absolute`, `--dry-run`), `doctor`, `gc`, `examples/garnish.toml`, stale styling | 09-04 |
| 8 Performance | criterion benches, hyperfine gate; warm tick 2.5 ms of which 1.3 ms is process start | 09-04 |
| 9 Hardening, v0.1.0 | hardening tests, two adversarial reviews, `command-run` dropped, macOS paths, tag | 09-04 |
| 10 CI, hosts | Actions on Linux/macOS with SHA-pinned actions and Renovate, `ci-annotate.sh`, `session-host.sh`, the SessionStart hook, `setup.sh`, documents split by role | 09-05 |
| 11 Align, durations | `align = true` column padding, `durations = compact \| fixed`, byte-identical default render | 09-05 |
| 12 Walkthrough fixes | wide-glyph guard and replacement sets, powerline pad, muted zero counts, line separator at the join, quiet `config check`, doctor glyph grid, config-golden harness | 09-05 |
| 13 Line keys | `right_justify`, `hide_empty_lines`, spacers, `bar = "blocks" \| "line"`, `blank` | 09-05 |
| 14 Per-key fallback | the file read as a table, each key converted alone, syntax errors the only wholesale fallback | 09-05 |
| 15 Ticker, text modules | `time::frame`, `ansi::scroll`, `overflow = "ticker"`, `[modules.text.<name>]`, ticker durations default `fixed` | 09-05 |
| 16 Animation | `animate`, `fill_pattern`, `separator_frames`, `<key>_frames`, frozen ticker cut with `…` | 09-06 |
| 17 Presets gallery | `presets/*.toml` embedded, `docs/presets.md`, `garnish presets`, `config init --preset <gallery>` | 09-06 |
| 18 Skills, v0.2.0 | three `skills/*/SKILL.md`, `garnish skills install \| list`, issue templates, CHANGELOG, tag | 09-06 |
| Release pipeline | `release.yml`: tag → verify → pre-release → binaries → cask → Daniel's approval → tap → promote (`CLAUDE.md` § Release process) | 09-11 |
| 19 Harness fidelity | `animate` following `prefersReducedMotion`, never rewriting an unparsable file (`install::replace_file`), the doctor's settings-chain report, `preview` drawn faint, `GARNISH_MANAGED_SETTINGS`, the three-renderer height rule (SPEC § 2.1), the `# color:` golden mode | 09-13 |
| 20 Presentation | `COMMON_OPTS` with `max_width`, the schema-generated module matrix test, `path.style = "fish"`, `branch.link` and `text.url`, `context.scale = "usable"`, `reset = absolute \| both` | 09-13 |
| Audit through 20 | the code read against its documents: two escapes out of the repository, four unbounded things, nine wrong renders, one rule per thing, five blind spots in the tests, shellcheck in CI; then a review of the audit itself; 194 → 227 tests | 09-16 |
| 21 Layout | `[[row]]` with `[[line]]` as its alias, `[[row.col]]` (`width`, `gap`, `justify`, `valign`), `[[row.col.row]]` stacks, `title*`, `[box.<name>]` and `box = true`; `src/layout.rs` in place of `frame::compose_line`; four presets, seventeen config goldens, two adversarial reviews | 09-17 |
| 22 Interactive setup | `garnish setup` (`src/setup/`): home menu, preset picker with a live preview at the real width, builder over the config file's own table with a placement map and click-to-edit preview, schema-generated editors, glyph and string pickers with suggestions, install screen over `install::Steps`; `setup --preset [--install]`; the bare `garnish` on a tty points at `setup`; `fixtures.rs`; snapshot goldens under `tests/golden/setup/`; ratatui + crossterm and toml `preserve_order` | 09-19 |
| Consolidation | nine gallery presets showing the rest of the vocabulary (28 in all); the review workflow collecting the pull request in job shell with a short prompt and `Bash` whole; the skills shortened and pointed at `setup`; *also try* glyphs on the module pages; `WORKLOG.md` split from this file; `PLAN.md`, `SPEC.md`, `CLAUDE.md`, `README.md`, the guide and `CHANGELOG.md` brought to the code | 09-19 |
| 23 Usage views and formats | `hide` lists derived from a module's measure (`MeasureKind`, `HideRule`, `Rendered.measure`, one check in `render_group`); the `[format]` table with per-module `tokens`/`percent`/`cost` overrides and `parens = "dim"` through one `detail()` helper; `pace`, `pace_colors`, `eta`, `reset = "elapsed"` and `elapsed_marker` on the two limit windows; `[frame] separator_color` with `inherit`; the `version`, `sandbox`, `voice` and `account` modules (25 ids; `account` is the first cached module outside the repo group, `Clock.workers` keeps pinned renders off the cache, `Clock.settings_keys` seeds the badges' docs samples); `doctor` rows for the two settings keys; four gallery presets (32 in all); seven config goldens; three adversarial reviews | 09-19 |
| Setup refinements | after Daniel's first use: undo/redo over a history of tables (`Draft.saved` makes dirty a comparison), the hint bar as clickable buttons, pickers opening on the value in effect with `custom…` starting from it, an input cursor; six form bugs found by a walk of every preset's forms (trimmed strings, `[colors]` roles, `blank` and title keys where illegal, orphaned `[box]` tables, silent breakage of another key); `C`/`]`/`[`/`m` growing columns from the cursor, `B` boxing a row with the one above, the module's name as its first `label` suggestion, `preset` swapping its own rows; one adversarial review | 09-20 |
| Review 2026-09-25 | a read-only review of the whole codebase by 12 area reviewers, each verified adversarially, then fixed batch by batch: bounded regular-file reads of every `.git` file, a plumbing dirty check that never runs filter drivers, `git` from absolute `PATH` only, a private temp cache root and a GC that runs; the reftable fallback, gone upstreams, honest fetch clocks, `--config` to the workers; `config/mod.rs` and `setup/app.rs` split by concern, one `Vocab` per enum, `*_KEYS` tripwires, `refresh` only for cached modules; per-field payload leniency, TZ read as one zone file; `write_target`, `CLAUDE_CONFIG_DIR`, `install` keeping `--config`, the `⚠ garnish:` row on a panic or a bad flag; the per-row trim held, every column exactly its share; about 80 setup and layout fixes; the review workflow's token and trigger, the release build without a cache or setup script; ten modules `pub(crate)`; link and module-name checks on the docs; then a final adversarial review of both pull requests and a verification of its fixes (the review's isolation switched on whole, every command following the config the status line command passes, the reftable and entry-belt spawn storms, a streamed `.git/config` parser with a `warm-bigconfig` bench); 319 → 538 tests | 09-25 |

## Backlog

Open items only; closed ones are in `WORKLOG.md`.

**Waiting on Daniel**

- [ ] First release through the pipeline (`v0.3.0`): needs the `release`
  environment (required reviewer Daniel) on the repo and the merged
  `garnish.sts.yaml` in the tap; afterwards drop the "lands with the first
  release" note from the tap's README, and the "from the first tagged
  release" qualifier from README § Install and guide § 1
- [ ] Watch a nine-line status line at 24 and 50 rows in Claude Code's
  fullscreen and classic renderers (`/tui`) to confirm the § 2.1
  arithmetic (`⌊LINES / 2⌋ − 5` rows whole with an empty prompt), then
  drop "read, not watched" from SPEC § 2.1 and `CLAUDE.md`
- [ ] Whether `preview <dir>`'s heading should honour `--color never`
  (SPEC § 7 does not say; the test compares the plain heading)
- [ ] How far to go in refusing a hostile `.git/config` on the *fetch*
  path. Decided 2026-09-25: the tick and the default workers reach no
  program a repository names (plumbing dirty check, `GIT_NO_LAZY_FETCH`,
  `git` from absolute `PATH` only; `CLAUDE.md` § The repository is not
  the user's file). Still open, all reachable only with the opt-in
  `fetch_interval > 0`: `core.sshCommand`, `core.gitProxy`, an `ext::`
  URL, hooks (`core.hooksPath`, `.git/hooks`), `credential.helper`,
  `core.askPass` and `core.alternateRefsCommand`. Clear them, refuse to
  fetch in a repository the user does not own, or say so in the
  `fetch_interval` docs. (`GIT_NO_LAZY_FETCH` needs git 2.44 or later; a
  setsid'd worker would also keep other programs off `/dev/tty`.)
- [ ] Whether a gateway session with only a spend limit should show `cost`:
  `rate_limits` present means subscription (SPEC § 2.2), so today it hides
  `cost` by default (review finding mod-07; SPEC's wording now matches the
  rule, the behaviour is unchanged). The alternative is `spend` in the
  usage rows of `minimal` and `compact`, which today show neither for
  such a session; a spend-only payload fixture would come first
- [ ] `sidebar-panels` shows what `valign = "bottom"` does to a column
  shorter than its row: the empty lines above it are blank, not rule.
  Decide whether a padding line in a `fill = true` row should carry the
  rule (SPEC § 4.3 says spaces, "since a rule running past a box's side
  would look wrong", which is about boxes, not bare columns)

**When asked** (small; none blocks anything)

- [ ] `garnish preview` discovers git and reads the settings chain as the
  tick does; only the cache and the workers are off (SPEC § 14, Phase 23's
  review). The setup pane skips git and settings too, since the bundled
  fixtures name no real directory; whether the CLI preview should is
  Daniel's call (a captured payload from a real repository would then
  lose its repo group)
- [ ] A `setup` cargo feature, if the binary size ever matters: the
  release binary grew from 2.8 MB to 3.4 MB with ratatui and crossterm,
  the end-to-end cold tick did not move (about 2 ms either way, 200 runs
  in the session's container), and the render path never touches them
- [ ] The glyph picker draws each candidate with its cell count (`|1`,
  `|2`) rather than the doctor's two-cell `|` grid the spec describes;
  the count answers the same question for a glyph the font draws wide
- [ ] Keyboard selection of a separator, a cap or a rule in the preview
  (a click reaches them; keys reach modules, rows and columns); `2`
  opens the frame form meanwhile. In the same vein, the placement map
  names only the outer row of a line, so a click on a title or a box
  edge inside a row of columns selects that row and points at the list;
  carrying the inner `RowAt` and the column's box name in `Elem::Title`
  and `Elem::BoxEdge` would let the click land on the inner row
- [ ] The snapshot tests see symbols only, so nothing pins the inverse
  video on the selected module or the highlighted line and field; a
  `cell_modifiers` helper over `TestBackend` next to `snapshot` would
- [ ] `tests/presets.rs` reads `ticker_step` as an integer for its slide
  check, so a fractional step would be read as 1; no preset uses one
- [ ] The undo history holds the draft alone: a preview width (`w`) or a
  fixture (`f`) is not an edit and is not undone, and the history is
  dropped when a preset is applied from the picker (`Enter`), since that
  writes the file rather than editing the draft
- [ ] Two setup behaviours still unpinned by a test (WORKLOG 2026-09-20): a
  two-cell glyph under the input cursor, and undo after a reload
- [ ] Left open by the 2026-09-25 review, each small:
  - git and cache: the fetch-age hint with no success recorded yet, or a
    failed-fetch glyph (a schema icon); `LockGuard::adopt` renames over
    any lock, so a worker that starts late can take a re-taken one; on
    Linux a tick that cannot lock only logs it (spawning without the lock
    would let the worker record `✗`); `upstream()` assumes the default
    fetch refspec; `doctor`, the debug log and the spawn log still create
    the cache root with default permissions (the scope directories are
    0700); `diff-files` still hashes a racily-clean entry whose stat data
    matches exactly, which an unpacked archive cannot arrange under the
    pinned `checkStat`
  - setup: one `post_edit` step (pruning and the preset-row swap happen
    per route today); SIGTERM leaves the terminal
    broken (documented; a fix needs a signal crate)
  - config: an emptied single-row stack round-trips through `config show`
    as a bare column, which draws rule where the stack drew spaces; a
    config whose rows all vanish gets the default preset's rows back
  - layout: a boxed stack as a row's last column still gets a cap pad
    (`│ ─┤`, visible in the `stack-boxes` and `columns-auto-box` goldens);
    the prefix pad is always drawn, so a row whose first column starts
    with rule reads `── ────`
  - install: `install --dry-run`'s skills line says "would write" when they
    are up to date; `Steps::apply` does not re-read the settings file (the
    setup screen re-plans before applying)
  - the final review's leftovers (2026-09-25): the cap pad is decided per
    row, so a visible `[frame] pad` shows on the padding lines of a
    multi-line row beside nothing (the cap-side twin of the prefix-pad
    item above); some setup pickers offer entries the parser then refuses
    in context (the `box` picker on an inner row of a boxed column,
    `fill_pattern` under `fill = false`, an empty `custom…`), each refused
    cleanly; at `COLUMNS=10` a `custom` frame whose caps and a wide `pad`
    exceed the box is recut to `…` (2 of 20 000 fuzz seeds, from 17 before
    the layout follow-up); absurd payload numbers still print
    long durations, countdowns and token counts (percent and cost are
    bounded); without `--absolute` a reinstall replaces an absolute
    program word with the bare `garnish`, and `setup --preset P --install`
    always writes the bare word; an explicit `--config` reinstall drops the
    old command's prefix and arguments; `exec garnish …`, `env -i
    garnish …`, `env A=1 env garnish …`, `export GARNISH_CONFIG=…;
    garnish` and bash's `GARNISH_CONFIG+=…` read as not garnish or as
    passing no config (safe in a checkout: the lookup is the person's);
    one
    nested-box mistake is reported twice (the nesting and "no row joins
    this box"); a worker given a `--config` that names a missing file runs
    on the defaults without saying so (it could record a failure);
    `head_from_git` (the reftable fallback) cuts a name past 4096
    characters where `head()` refuses one; the `.git/config` section
    skipper is slower than a plain parse on a crafted 1 MiB file of short
    backslash-continued lines (11 ms against 7, bounded by the cap; every
    other hostile shape measured got faster)
  - layout, Daniel's call each (from the layout follow-up): a `width = 0`
    unboxed stack column keeps its stack's height (collapsing it would
    collapse an emptied `auto` stack too, which SPEC § 4.3 keeps so the
    layout does not reflow); a last column whose right-justified text is
    cut to nothing (`width = 1`) still counts as ending in content
    (`⏱ 1h12m ────────── ──`), and deciding that from the drawn cells would
    change the pad beside a boxed last column; inside a `fill = true` box a
    row whose `fr` columns are all squeezed out puts the freed rule right
    after the last column's text; under uneven `custom` caps, heights use
    the narrowest cap pair, so a box that fits only on the wider lines is
    drawn as empty cells
  - config location, Daniel's call (verification of 2026-09-26): a config
    that a checkout's own `.claude/` settings name (a local or project
    file outside the person's settings directory: the command's
    `--config`, a `GARNISH_CONFIG` its `env` block sets, a managed-settings
    hook it points at itself) is followed by no command run by hand,
    reading or writing; each refuses on one line and `--config FILE` names
    it. Alternatives: follow it for the readers only (tried: `config
    path` then answered for one side, and the `garnish-statusline` skill
    writes through it), or trust a project Claude Code itself trusts
    (`hasTrustDialogAccepted` in `~/.claude.json`, an undocumented key).
    Not guarded: a checkout's `env` block can also set `GARNISH_CACHE_DIR`,
    `GARNISH_DEBUG` and the other hooks for the tick, whose files are
    garnish-named (the tick reading a checkout's config is Claude Code's
    workspace-trust matter). Also open: `$HOME$HOME`, `~/$HOME`,
    `${HOME-x}` and a `~` after `GARNISH_CONFIG=` name one file to `sh`
    and are refused as naming none
  - modules: the key scan finds a key read but not declared; the reverse,
    a key declared that nothing reads (how `colors.percent` went unread),
    would need the scan to track a key's kind (icon, colour, option)
  - layout: under a rule (`Fill::Rule`) a column facing a gap of 1 or
    more still reserves a fill cell beside the gap's rule cells
    (cosmetic; without a rule the reservation is gone)
  - gallery and docs: the ascii `pending` PR glyph `..` is the ascii
    ellipsis too, so a cut and a pending check read alike (Daniel's call);
    `animated-dots` could use Nerd moon glyphs once their code points are
    checked in a Nerd Font; `docs/config.md`'s Environment table is still
    written by hand (the hook tripwire guards it); `docs.rs` keeps a
    module writer and a text-module writer that could share one, a few
    unreachable fallbacks and `is_tooling_line`
  - With `RUST_BACKTRACE=1` in the environment every error report captures
    and symbolises a backtrace, so a quiet refusal (`config check` on a bad
    file) takes 0.7 s in a debug build, 5 ms without; a `Quiet` error never
    prints its report, so it could be built without one

**Parked designs** (decided, not to be reopened without a reason)

- [ ] A `branch.forge = "auto" | "github" | "gitlab"` key, if a self-hosted
  GitLab with no open merge request ever turns up in practice. Decided
  2026-09-16 with Daniel: documented as a known limitation instead (SPEC
  § 3.1, the `link` option text and `examples/garnish.toml`)
- [ ] `parse_settings_json` accepts files Claude Code rejects: its schema is
  strict for every key, and a user, project or local file that fails it is
  skipped entirely, so `doctor` can show `ok` and resolve keys from a file
  Claude Code never reads. The `tui` case is handled (2026-09-25: a file
  Claude Code rejects for it contributes no keys and `doctor` marks it);
  the general check would mean carrying Claude Code's schema, which is a
  spec decision
- [ ] Optional headroom: cache the resolved config keyed by mtime, cache the
  settings-chain reads for 30 s, but only if the tick budget is ever
  threatened
- [ ] From `FUTURE-SPEC.md` (PR #27, reviewed 2026-09-12; Phase 23 took
  A4, A6, A9, A10, A12, A13 and N11 on 2026-09-19): the Tier A ideas
  still in that document, until asked for: theme rotation (§ 12.3),
  `config share`/`apply`, `preview --config` and `preview --html`
  (§ 12.2), gradients (A3), Powerline segments (B1), the `provider`
  badge (§ 8.4) and the `remote` module (A9's fourth, a duplicate of the
  harness's own indicator). Everything Tier B/C (workers, hooks, network,
  transcript, the companion, garlic) is a § 0 decision there, untouched
