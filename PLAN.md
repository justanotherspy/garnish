# PLAN.md — implementation plan and progress

This file is the gap between `SPEC.md` (the target design) and the code as
it is, plus a compact log of how the project got here. Check items off as
they land; keep the **Work log** current; anything still open after a phase
closes goes in the **Backlog**. Rules for working here: `CLAUDE.md`. Host
trouble does not belong in this file.

## Where things stand

`v0.2.0` (2026-09-06) shipped everything through Phase 18: the 21 modules,
the schema-driven config with presets, themes, icon sets and frames, the
cache and worker model, aligned columns and fixed durations, per-key config
fallback, the clock-driven ticker, text modules and animations, the presets
gallery and the three bundled skills. The release pipeline with the Homebrew
tap landed on 2026-09-11 and waits for its first tag. On 2026-09-12 every
open code item was closed and the spec was audited against the code, so
**the only drift between SPEC and the code is Phases 19–22 below**, written
that day from the FUTURE-SPEC review and Daniel's layout and setup ideas.

## Done — Phases 0–18, compacted

| phase | landed | when |
|---|---|---|
| 0 Scaffold | nightly toolchain, strict lints, nextest, Makefile, `scripts/ci.sh`, the four documents | 09-04 |
| 1 Payload, time, ANSI | `payload.rs`, `time.rs` (`GARNISH_NOW`), `ansi.rs` (width, OSC 8, `…` truncation), `num.rs`, `preview`, the golden harness (`UPDATE_GOLDEN=1`), the payload fixtures | 09-04 |
| 2 Schema, config, frame | `ModuleSchema`, config model with TOML-path validation, four icon sets, six themes, seven frame styles, `config init/check/path/show`, four built-in presets, the render matrix and (09-05) config-driven goldens | 09-04 |
| 3 Payload modules | 16 payload-only modules incl. `context` with the autocompact chain, `⚠` failure rows, `GARNISH_DEBUG` | 09-04 |
| 4 Cache & workers | `cache.rs` (atomic entries, TTL, GC), `spawn.rs` (detached workers, hard-linked locks, `GARNISH_NO_SPAWN`), `refresh`; serial tests incl. 32 ticks → one worker and (09-12) the killed-tick test | 09-04 |
| 5 Repo modules | `git.rs` direct `.git` reads, worker ahead/behind/dirty/fetch, `path branch sync worktree pr`, temp-repo and PATH-shim tests (behind/diverged/fetch end to end on 09-12) | 09-04 |
| 6 Docs | `garnish docs` → `docs/`, hand-written guide, `garnish modules`, docs-sync test, `config show` fully resolved | 09-04 |
| 7 Install, doctor | `install` (merge, backup, `--absolute`, `--dry-run`), `doctor`, `gc`, `examples/garnish.toml`, stale styling | 09-04 |
| 8 Performance | criterion benches, hyperfine gate; warm tick 2.5 ms of which 1.3 ms is process start | 09-04 |
| 9 Hardening, v0.1.0 | hardening tests, two adversarial reviews (symref cycle, worker deadlock, fetch poisoning, install permissions and backups), `command-run` dropped, macOS paths, tag | 09-04 |
| 10 CI, hosts | Actions on Linux/macOS with SHA-pinned actions and Renovate, `ci-annotate.sh`, `session-host.sh`, the SessionStart hook, `setup.sh`, documents split by role | 09-04/05 |
| 11 Align, durations | `align = true` column padding, `durations = compact \| fixed`, byte-identical default render | 09-05 |
| 12 Walkthrough fixes | wide-glyph guard and replacement sets (unicode, emoji, nerd borrowings), powerline pad, muted zero counts, line separator at the join, quiet `config check`, fetch-age spacing, doctor glyph grid, config-golden harness | 09-05 |
| 13 Line keys | `right_justify`, `hide_empty_lines`, spacers, `bar = "blocks" \| "line"`, `blank` (09-06) | 09-05 |
| 14 Per-key fallback | the file read as a table, each key converted alone, syntax errors the only wholesale fallback | 09-05 |
| 15 Ticker, text modules | `time::frame`, `ansi::scroll`, `overflow = "ticker"`, `[modules.text.<name>]`, ticker durations default `fixed` (09-06) | 09-05 |
| 16 Animation | `animate`, `fill_pattern`, `separator_frames`, `<key>_frames`, frozen ticker cut with `…` (09-06) | 09-06 |
| 17 Presets gallery | `presets/*.toml` embedded, `docs/presets.md`, `garnish presets`, `config init --preset <gallery>`; website dropped 09-12 for the setup | 09-06 |
| 18 Skills, v0.2.0 | three `skills/*/SKILL.md`, `garnish skills install \| list`, issue templates, CHANGELOG, tag | 09-06 |

Between 17 and 18 a whole-stack review added row hardening (every string
reduced to plain text by the `Segment` constructors, bounded sizes, OSC 8
only for `http(s)://`) and config/CLI polish. On 2026-09-11 the release
pipeline (`release.yml`, cask template, `changelog-section.sh`,
`render-cask.sh`) and its review landed. On 2026-09-12 the last open items
closed (`Segment.text` private, `OptSpec::max`, the killed-tick and fetch
tests, a source scan for schema completeness) and the spec's drift from
the code was fixed.

## Open — the drift between SPEC and the code

Phases 19–22 are the 2026-09-12 review of `FUTURE-SPEC.md` (PR #27) with
Daniel: the cheap, invariant-safe ideas moved into `SPEC.md` (each
paragraph there names its FUTURE-SPEC section and proposal id), his layout
model, and the interactive setup he chose in place of the website.
**Order: 19 → 20 → 21 → 22.** Phase 19 first because the dim reset changes
what every golden and every `setup` preview shows; Phase 20's `max_width`
and the schema-generated matrix test are what the builder's module editor
is built on; Phase 21's layout model is what the builder must draw;
Phase 22 is the one that adds crates. Each phase is its own `gh stack`
chain of `phase-N/<concern>` layers, as before; one release per phase is
fine, `v0.3.0` being whichever lands first through the pipeline. Nothing
here lifts a non-goal: no network, no transcript, no tick-side write, the
module set stays at 21.

### Phase 19 — Harness fidelity (SPEC § 2.1, § 4.2, § 5, § 7)

- [ ] Dim reset (A1): confirm on screen that the harness wraps each row in SGR 2 (2.1.261 `<Text dimColor wrap="truncate">`; re-locate it in the current binary), then prefix every row with `ESC[0m` when colour is on; regenerate goldens; the fact in `CLAUDE.md` with how to re-verify; a unit test that `--color never` emits no prefix; SPEC § 4.1's `blank` wording and the guide note that with colour on every configured row now survives the harness's trim (a test paints an unframed `fill = false` spacer and checks the prefix)
- [ ] Reduced motion (N6): `claude_settings` resolves `prefersReducedMotion` on the same chain as the autocompact keys; effective `animate` is `clock.animate && config.animate.unwrap_or(!reduced)` (env, then config, then settings, then the default); `config show` prints the effective value; unit tests for each precedence pair, config golden `reduced-motion` with `# env:`
- [ ] Never rewrite an unparsable file (SPEC § 5): `install` and `config init --force` refuse on a settings or config file that does not parse (name the problem, exit 1 quietly); `tests/cli.rs` covers each; `install` already writes through a temp file and `rename`, assert it
- [ ] Doctor checks (N5): report `statusLine.refreshInterval` (suggest `1` when `clock`, a countdown or an animation is configured), `statusLine.hideVimModeIndicator` (suggest `true` when `vim` is on), `disableAllHooks`, `prefersReducedMotion`, and whether the settings file parses; unit tests on the doctor's report over a settings fixture
- [ ] Verify item 2 of FUTURE-SPEC § 4.9: re-check the 13 000 autocompact constant in the current binary; keep `compact_buffer_tokens` either way; note the version in SPEC § 2.3 and `CLAUDE.md`
- [ ] Verify the status line's height: how many rows the harness shows before it caps, scrolls or squeezes the transcript (the footer's Ink box, `LINES` in the script's environment, a nine-row render on screen at a 24-row and a 50-row terminal); record the rule in SPEC § 2.1 and `CLAUDE.md`; it decides whether the § 14 picker warns on height as it does on width, and whether a multi-row line (§ 4.3) needs a cap of its own
- [ ] Docs (`make docs`, README/guide troubleshooting: "the line looks dimmer than `preview`" goes away), CHANGELOG `## Unreleased`, adversarial review, work log

(A 24 h lock horizon from FUTURE-SPEC § 15 was on this list and was dropped
in the spec review: `LOCK_STALE_MS` already bounds a lock's life.)

### Phase 20 — Per-module presentation (SPEC § 3, § 3.7, § 9)

- [ ] `max_width` (A5) as a common option next to `label`/`prefix`/`suffix` (`OptSpec` with `.max(1024)`), applied in `render_group` through `ansi::truncate` before alignment, OSC 8 wrapper kept balanced; rejected on text modules with a message naming `width`; unit tests on a linked `pr` and a wide branch name; config golden `max-width`
- [ ] Module matrix from the schema (FUTURE-SPEC § 15 item 11): a test generated from `ModuleSchema` over module × preset × icon set × `max_width ∈ {0, 1, 4, 12}` × fixture asserting width ≤ `max_width`, nothing rendered for a hidden state, balanced OSC 8, no escape bytes in `Segment::text`; rayon like the other matrices, with the longer nextest budget
- [ ] `path`: `style = "full" | "fish"` (A7) applied after the existing `depth`; unit tests on `~`, a root path, a one-segment base and `depth = 2` with `fish`; config golden `path-fish`
- [ ] `branch`: `link = true` from `workspace.repo` (A8), the branch percent-encoded (unreserved and `/` kept), GitLab `/-/tree/`, nothing when `repo` is absent or the head is detached; unit tests including `feature/#12` and a non-ASCII name, a golden on the `git-worktree` fixture (it carries `repo`)
- [ ] `text.<name>`: `url` (A8) through the painter's `http(s)://` rule; `config check` reports anything else; unit test, golden `text-link`
- [ ] `context`: `scale = "usable"` (A11): percentage and bar against the § 2.3 threshold, marker hidden, bands on the displayed percentage, falls back to `window` when compaction is disabled or the threshold is under a tenth of the window; unit tests at the threshold edges and the two fallbacks, config golden `context-usable` at 80 % and 96 % of 1M
- [ ] `limit5h`/`limit7d`/`spend`: `reset = "countdown" | "absolute" | "both"` (A10) formatted with jiff in the `clock` zone, weekday always on `limit7d` and never elsewhere, `show_reset = false` hides every form; unit tests at two instants, config golden `reset-absolute` pinned at two `# now:` values
- [ ] Schema → render → `make docs` → `UPDATE_GOLDEN=1`; guide § 5 gains the three presentation keys; CHANGELOG; adversarial review; work log

### Phase 21 — Layout: lines, columns and boxes (SPEC § 4.3)

Daniel's ideas, 2026-09-12, consolidated the same day into one model: a
line is columns side by side (a plain line is one column, today's flex
rule inside it), a column holds one row of modules or a stack of lines,
columns share the width by `width` (`fr`, `auto`, cells) and place a lone
group by `justify`; titles decorate rules and boxes decorate lines and
columns; two levels, never deeper. Supersedes FUTURE-SPEC A2. The layers
build the model inside out so each one ships with goldens and a
byte-identical default render.

- [ ] `phase-21/layout-model`: `LineCfg` gains `cols: Vec<ColCfg>`, `gap`, the four `title*` keys and `box`; `ColCfg { width: Width::{Fr(n), Auto, Cells(n)}, modules, right, justify, valign, box, lines: Vec<LineCfg> }`, parsed from `[[line.col]]` and `[[line.col.line]]` with the per-key fallback; a line without columns becomes one `1fr` column carrying its `modules`/`right` (so the resolved tree is always the same shape; a test over every fixture and preset shows the resolved config and the render byte-identical); `justify` defaults by position; validation as SPEC § 4.3 lists (words, the `width` grammar, `gap`, the caps on columns, inner lines and `title_pad`, unknown box names, both forms on a line or a column, nesting, non-adjacent reuse, a title on a boxed line), each with its TOML path; `config show` round-trips every form (test over a fixture config that uses all of them); all-empty columns make a spacer
- [ ] `phase-21/layout-columns`: `render::compose_line` generalised to columns: `auto` and cell widths taken first, the free width shared by `fr` with the remainder to the first columns (unit test that shares add up to the width at every width from 10 to 400 and differ by at most one cell), the flex rule inside a column with `right`, `justify` placement for a lone group, `gap` boundaries, the fill glyph or pattern in every empty cell of a single-row line, `…` cut or a per-column ticker window for over-wide content (the § 4.1 rule inside a flex column, a lone group as the window, `auto` never scrolling), left-to-right clamping when the width runs out (each `fr` column at least one cell, a column with nothing left renders nothing and `GARNISH_DEBUG` logs it), `align = true` per column index counted from the justified end and only between lines with the same column count, `right_justify` on right groups and right-justified columns; the two-group path deleted, since a plain line is one column (goldens byte-identical); config goldens `columns-one` (each `justify`), `columns-three`, `columns-six`, `columns-widths` (`auto`, cells and `fr` mixed), `columns-ticker` at two instants, each at 80 and 160 columns (one file per width, the header takes one)
- [ ] `phase-21/titles`: the `title*` keys reduced to plain text and capped like `label`; `frame::Rule::paint` sets the title after the left cap, before the right cap, or centred in the widest empty gap; plain text at the same place under `fill = false` or `style = "none"`; the first row of a multi-row line; cut with `…` when wider than its space, never widening the row; a title-only line is a titled spacer; config goldens `title-rows` (each `title_justify` on a spacer and on a module row, at 60 and 120 columns)
- [ ] `phase-21/boxes`: `[box.<name>]` (`title*`, `style` and `color` inheriting from `[frame]`, `fill` defaulting to `false`, an unstyled box `rounded` when the frame has no box shape, an unjoined box reported); adjacent lines with one name form a box, `box = true` boxes a line alone, a column-level `box` (name or `true`) spans the line's height, a boxed one-row column is a three-row box; drawn as a corner-capped top rule with the title, side glyphs at both ends of each row in place of the frame's caps, and a bottom rule; corners and `side` on the built-in styles (`none` invisible, `powerline` reported and drawn rounded) and the five `custom` keys with the one-cell check; the frame's first/last caps treat a multi-row line as one block; `hide_empty_lines` drops a box whose lines all went; config goldens `boxes-two` (two titled boxes around unboxed lines), `box-columns` (a three-column line inside a box) and `box-column` (a boxed column beside a bare one) at two widths; a unit test that every box row is exactly the box width
- [ ] `phase-21/stacks`: `[[line.col.line]]` laid out to the column's width (inner `justify` overriding the column's), the line's height as the tallest column, `valign` padding, spaces in gap cells and padding rows of a multi-row line, `blank` on the outer line keeping every row, the outer caps on every non-box row; `hide_empty_lines` per inner line and for the whole line, an emptied column keeping its share beside a sibling that stayed; unit test that every row is exactly the box width over widths 10–400 with stacks of unequal height; config goldens `dashboard` (the SPEC sample: a full-height double box, a bare centred column, three stacked boxes) at 60 and 120 columns, `stack-valign` and `stack-hidden` (an inner line that renders nothing)
- [ ] Presets `grid-three`, `grid-six`, `boxed-panels` and `dashboard-panels` (declared widths, `tests/presets.rs`); `docs/config.md` gains a `[[line.col]]` section (widths, `justify`, stacks), `title` rows and a `[box.<name>]` section, each with a sample at two widths; README layout paragraph and guide § 5 rewritten around "a line is columns"; the `garnish-statusline` skill's question table and examples updated for columns, `width`, `justify`, titles and boxes (it hand-writes the TOML, so nothing else would catch it going stale); CHANGELOG
- [ ] Bench: `tick_in_process_columns` (six columns), `tick_in_process_boxes` (two boxes) and `tick_in_process_dashboard`; layout is arithmetic over the rendered segments, so the warm default tick is untouched (the one-column path must cost what the two-group path cost: assert within 0.05 ms); adversarial review (a column narrower than a module, `fr` totals and cell widths overflowing the box, zero free width, `auto` columns wider than the box, a line under `hide_empty_lines` and `stale_style = "hide"`, a title wider than the row, a box at the minimum width of 10, a box whose title is a module id, `blank` on a titled spacer, a multi-row line of bare columns with colour off, a boxed column with no lines, a stack that scrolls under `overflow = "ticker"`); work log

### Phase 22 — Interactive setup (SPEC § 14)

Decided 2026-09-12 with Daniel: a full-screen `garnish setup` in the
terminal, ccstatusline's shape with garnish's exact preview and
schema-generated editors. `ratatui` + `crossterm` are the one new
dependency pair (crate map row in `CLAUDE.md` when the first layer lands).

- [ ] `phase-22/setup-shell`: the crates, `src/setup/` module tree, `garnish setup` opens a home screen (*Pick a preset*, *Build a custom layout*, *Install*, *Quit*) and quits cleanly, terminal restored on panic and on `Ctrl+C`; `garnish` with a tty on stdin prints the one-line pointer and exits 0 (`tests/cli.rs`); `bench/run.sh` unchanged (note the cold-start delta in the commit)
- [ ] `phase-22/setup-preview`: the preview pane through `render_lines_at` at `COLUMNS − 4 − padding` with the live clock and the bundled fixtures (`f` cycles, `w` sets a width); the snapshot harness over ratatui's `TestBackend` with goldens under `tests/golden/setup/` at 80×24 and 140×40 (`UPDATE_GOLDEN=1`, `GARNISH_NOW` frozen and `TZ=UTC` pinned), key sequences driven through the event loop; the terminal-restore hook chained ahead of color-eyre's panic hook (a test panics inside the loop and checks the restore ran)
- [ ] `phase-22/setup-gallery`: the preset picker (built-ins plus `gallery::PRESETS`) with summary, declared width, `needs` and the narrower-than-declared warning; `Enter` writes with `install`'s never-clobbered backup and offers install; `e` opens the builder; `setup --preset <name> [--install]` never opens the screen, and `setup` without `--preset` and without a tty on stdout exits 1 with one line (`tests/cli.rs`)
- [ ] `phase-22/setup-builder`: the line list (add, insert, delete, clone, move, spacer), a line shown as its columns side by side (SPEC § 4.3): *Add a column* with its `width` and `justify`, *Stack* to turn a column into lines, *Add a title*, *Wrap in a box* over a selected run and *Box the column* (titles and box edges in the placement map), moves within and between columns, the module picker with fuzzy and initialism search over the 21 ids and `text.<name>` (unit tests on the matcher), the top-level and `[colors]` screens; `s` saves through the `config show` writer with the same backup, `q` asks once on a dirty draft, an unparsable file is never overwritten
- [ ] `phase-22/setup-selection`: the placement map from `render_lines_at` (per row, the cell ranges of every module, separator, cap, rule, title and box edge, from the same segment lists the painter emits; several ranges per module across a ticker wrap, the `…` cell owned by the cut module, an empty module owning none; unit test that the ranges tile each row and match the painted widths for flex, multi-column, stacked and ticker lines); crossterm mouse capture on entry and off on exit (also on panic), click and wheel handling, `Tab`/`Shift-Tab`/arrows as the keyboard twins; the selection highlighted in the preview (inverse video) and on the chip; snapshot tests driving synthetic mouse events through `TestBackend`
- [ ] `phase-22/setup-module-editor`: the overlay form generated from `ModuleSchema` (checkboxes for booleans, radio lists for `preset` and enums, steppers with `max`, colour swatches plus a validated custom entry, text-module schema on the same screen), re-rendering the preview on every change, a dot on chips that carry overrides; the unit test that every `OptSpec` kind and every top-level key has a form row
- [ ] `phase-22/setup-pickers`: the string picker seeded from the distinct values in the built-in presets, the frame tables and `gallery::PRESETS` (deduplicated at start-up, each drawn as it renders) plus *custom…* through the config parser's plain-text and width checks; the glyph picker with one row per icon set, the schema's `IconSpec.suggestions` per key, `doctor`'s two-cell `|` marker on every candidate and *custom…*, writing per-key `[modules.<id>.icons]` overrides; `suggestions` added to the schemas for every icon key (a few per set) and covered by the existing glyph guard test; `garnish docs` lists them as *also try* on each module page (`make docs`, `docs_sync`)
- [ ] `phase-22/setup-install`: the install screen through `install`'s own code (`--dry-run` summary, one confirmation), the doctor's settings report in the status bar; `garnish-statusline` skill names `setup`; README "Set up" section and guide § 2 rewritten around it; CHANGELOG
- [ ] Adversarial review of the phase (terminal left raw, a draft that `config check` would reject, a preset applied at a width where the terminal cuts it, `Ctrl+C` mid-save), tests for every bug found; work log

## Backlog

Open items only; closed ones are in the work log.

- [ ] Optional headroom (Phase 8 analysis): cache the resolved config keyed by mtime, cache the settings.json reads for 30 s — only if the tick budget is ever threatened (SPEC § 3.2 says the settings chain is read every tick)
- [ ] First release through the pipeline (`v0.3.0`): needs the `release` environment (required reviewer Daniel) on the repo and the merged `garnish.sts.yaml` in the tap; afterwards drop the "lands with the first release" note from the tap's README
- [ ] Parked from `FUTURE-SPEC.md` (PR #27, reviewed 2026-09-12): the Tier A ideas not taken into Phases 19–20 stay in that document until asked for (A2, the `center` group, is answered by the Phase 21 layout): `hide = [...]` lists (A4), `[format]` number styles and `dim = "parens"` (A6), separator colour inheritance (A13), a `version` module (A12; it would grow the fixed set), settings-derived `sandbox`/`voice`/`account` modules (A9), pace and burn on the limits (N11), theme rotation (§ 12.3), `config share`/`apply` and `preview --html` (§ 12.2), gradients (A3) and Powerline segments (B1). Everything Tier B/C (workers, hooks, network, transcript, the companion, garlic) is a § 0 decision there, untouched. Once this plan's phases start, FUTURE-SPEC should lose the sections they adopted (§ 6.2, § 12.4, § 13), per its own rule
- [ ] Open question from the 2026-09-12 code session: whether `preview <dir>`'s heading should honour `--color never` (SPEC § 7 does not say; the test compares the plain heading)

## Work log

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
  interactive setup in its place. SPEC § 14: `garnish setup` as FUTURE-SPEC
  § 13's `ratatui` option made a decision (exact preview through
  `render_lines_at`, editors generated from `ModuleSchema`, the gallery
  first, live save, install through `install`, `setup --preset` as the
  scriptable twin, a pointer when `garnish` is typed at a terminal), then
  selection in the preview by mouse or keyboard over a placement map, an
  overlay form of checkboxes and pickers, string pickers seeded from the
  presets, a glyph picker with schema suggestions drawn with the doctor's
  width marker. Chosen from the rest by "Tier A, no crate, no non-goal, no
  tick-side write, module set unchanged": the dim reset, reduced motion,
  the doctor's settings report, never rewriting an unparsable file,
  `max_width`, fish paths, branch and text links, the usable context
  scale, absolute reset times, the schema-generated matrix test (Phases 19
  and 20). Daniel's layout ideas arrived one at a time (grid columns,
  titled rules and boxes, panels of stacked boxes); asked whether they
  still read as one thing, I flagged the `align` collision and the three
  sections, he left the consolidation to me, and SPEC § 4.3 became one
  model: a line is columns, a column is a row of modules or a stack,
  `width = "1fr" | "auto" | cells`, `justify`, titles and boxes as
  decorations, two levels deep, `[[line]]` kept (no migration) with a
  plain line as one `1fr` column so the default render is byte-identical.
  Decided with Daniel: the one `width` key and `gap = 1`. An adversarial
  review of the day's spec text returned 25 findings, all taken (the
  samples contradicted the box rules; the lock horizon could never fire;
  `path.depth` already existed; a dozen corners defined, listed in the
  commit `51d4b66`). Then this plan was compacted to the drift between
  SPEC and the code plus this log. Open drafts #27 and #40 stay parked.
