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
2026-09-19 (the table below). The release pipeline with the Homebrew tap
waits for its first tag, which will be `v0.3.0`; `CHANGELOG.md`
§ Unreleased is its section.

**There is no open phase.** The drift between `SPEC.md` and the code is
the short list under *When asked* in the backlog (a keyboard path to a
separator or a cap, a cargo feature); everything else in the spec
is implemented, and where Phase 22 was built differently from its design,
SPEC § 14 says so and why (its 2026-09-20 refinements, undo among them,
are the paragraph after that). Phase 23 (usage views and formats) is on the
pull request from `claude/docs-workflow-consolidation-vepxgw`, waiting
for Daniel's merge; its decisions are in `WORKLOG.md` (2026-09-19) and
its design in SPEC § 3, § 3.3, § 3.8 and § 4.

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
- [ ] How far to go in refusing a hostile `.git/config`: `core.fsmonitor`
  and `remote.<name>.uploadpack` are overridden on every call;
  `core.sshCommand`, `core.gitProxy` and an `ext::` remote URL are not,
  each a setting a user may want honoured, all three reachable only with
  the opt-in `fetch_interval > 0`. Clear them too, refuse to fetch in a
  repository the user does not own (git's `safe.directory` answer), or
  leave them and say so in the `fetch_interval` docs
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
- [ ] The Claude settings chain (SPEC § 2.3) ignores `CLAUDE_CONFIG_DIR`,
  which moves the user file; only the `account` worker honours it for
  `.claude.json` (SPEC § 3.8). Honouring it in the chain means one more
  path rule in `claude_settings::settings_files` and a doctor line
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

**Parked designs** (decided, not to be reopened without a reason)

- [ ] A `branch.forge = "auto" | "github" | "gitlab"` key, if a self-hosted
  GitLab with no open merge request ever turns up in practice. Decided
  2026-09-16 with Daniel: documented as a known limitation instead (SPEC
  § 3.1, the `link` option text and `examples/garnish.toml`)
- [ ] `parse_settings_json` accepts files Claude Code rejects: its schema is
  strict for every key, and a user, project or local file that fails it is
  skipped entirely, so `doctor` can show `ok` and resolve keys from a file
  Claude Code never reads. The general check would mean carrying Claude
  Code's schema, which is a spec decision
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
