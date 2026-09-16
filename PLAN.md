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
tap landed on 2026-09-11 and waits for its first tag.

Phases 19 (harness fidelity) and 20 (per-module presentation) landed on
2026-09-13 and 2026-09-16, and an audit of everything through Phase 20
followed the same day: the code was read against the documents, the
defects it found were fixed with a test each, and the rules that had been
written out more than once were given one home.

**The drift between SPEC and the code is now Phases 21–22 below, and
nothing else.** `main` is ready for Phase 21.

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
| 19 Harness fidelity | `animate` as `Option<bool>` following `prefersReducedMotion` over the settings chain, never rewriting an unparsable `settings.json`/`garnish.toml` (`install::replace_file`, backups for `config init --force`), the doctor's settings-chain report with suggestions, the `# color:` golden mode (`colour-on`), `$ROOT` in `# env:`; the dim reset dropped as impossible (SPEC § 2.1), the 13 000 constant re-read in 2.1.270; then (09-13) `preview` drawing its rows faint, `GARNISH_MANAGED_SETTINGS` (SPEC § 9), the three-renderer height rule (SPEC § 2.1) and the `tui` row in `doctor` | 09-12/13 |
| 20 Presentation | `COMMON_OPTS` (the common keys as bounded specs) with `max_width` cutting the decorated module before alignment, the schema-generated module matrix test, `path.style = "fish"`, `branch.link` and `text.url` through a hand-written percent-encoder with GitLab's `/-/tree/`, `context.scale = "usable"`, `reset = absolute \| both` on the limit modules; seven config goldens | 09-13 |
| Audit through 20 | the code read against the documents: two path/argument escapes out of the repository, four unbounded things, nine silent or wrong renders, one rule per thing in place of the copies, five blind spots in the tests, shellcheck and least-privilege in CI, the documents' drift; then an adversarial review of the audit itself, which found four regressions it had introduced and five fixes that had stopped at the example; 194 → 225 tests | 09-16 |

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
**Order: 19 → 20 → 21 → 22; 19 and 20 are done (the table above).**
Phase 19 went first because it was meant to change every colour-on render
(the dim reset, which the harness binary then ruled out, see the work log)
and because it brought the colour-on golden mode the later phases use
(`# color:` in `tests/config_golden.rs`, `colour-on`); Phase 20's
`max_width` and the schema-generated matrix test are what the builder's
module editor is built on; Phase 21's layout model is what the builder
must draw; Phase 22 is the one that adds crates. Each phase is its own
`gh stack` chain of `phase-N/<concern>` layers, as before (Phases 19 and
20 each landed as one branch of small commits, one per layer, their
sessions having no `gh stack`); one release per phase is fine, `v0.3.0`
being whichever lands first through the pipeline. Nothing here lifts a
non-goal: no network, no transcript, no tick-side write, the module set
stays at 21. The code maps below were read on 2026-09-12, before Phase 19
moved things; re-derive their line pointers against `main` when the phase
starts (Phase 20's had drifted: `render_group` sat elsewhere and two of
the seams it named were the wrong constructs, so the session navigated by
symbol instead).

(A 24 h lock horizon from FUTURE-SPEC § 15 was on the Phase 19 list and
was dropped in the spec review: `LOCK_STALE_MS` already bounds a lock's
life. Phase 19's own layers are in the done table; the two things it
could not settle, the height rule and the dim `preview` question, were
decided on 2026-09-13 (work log).)

### Phase 21 — Layout: lines, columns and boxes (SPEC § 4.3)

Daniel's ideas, 2026-09-12, consolidated the same day into one model: a
**row** (`[[row]]`, the addressable unit, one or more terminal **lines**
tall) is columns side by side (a plain row is one column, today's flex
rule inside it), a column holds its own modules or a stack of rows,
columns share the width by `width` (`fr`, `auto`, cells) and place a lone
group by `justify`; titles decorate rules and boxes decorate rows and
columns; two levels, never deeper. `[[line]]` and `hide_empty_lines`
stay as permanent aliases. Supersedes FUTURE-SPEC A2. The layers build
the model inside out so each one ships with goldens and a byte-identical
default render. In the code map below the names are today's (`LineCfg`,
`resolve_lines`, `render_lines_at`); the rename layer gives them their
row names.

Code map (2026-09-12): `LineCfg` (`src/config/mod.rs:48-62`) is flat;
`RawConfig::from_table` handles an array of tables only for `line`
(`:414-427`) and `RawLine::from_table` (`:556-585`) matches keys by name
with a literal unknown-key list, so `[[row.col]]` and
`[[row.col.row]]` need hand-written recursion with the `field()`
discipline (`:590-609`: report the path, keep the rest; a serde-derived
column would discard a whole row on one bad key); `resolve_lines`
(`:861`) is a 1:1 map with the spacer rule and the `bad_list` guard
(`:549`); `frame::FrameChars` (`src/frame.rs:66-89`) has caps only,
`ends(index, count)` (`:160`) assumes one terminal line per config entry, `Rule::paint`
(`:202`) returns a `String` with no cell ranges, and `compose_line`
(`:246-358`) owns the left budget, the truncate-or-scroll choice and the
final row cut; `render_lines_at` (`src/render.rs:154-259`) applies
`align_columns` (`:336`) and the `fill = false` splice (`:214-233`) over
`lefts`/`rights`, then `hide_empty_lines`, `keep_blank` and the cap index
per composed line; `docs::config_toml` writes `[[line]]` at
`src/docs.rs:167-185`; `gallery::FILES` is a fixed-size sorted array
(`src/gallery.rs:14`); config goldens are keyed by `(name, now)` with one
`# columns:` per file and orphans deleted (`tests/config_golden.rs:225`).

- [ ] `phase-21/row-rename`: `[[row]]` accepted beside `[[line]]` in `RawConfig::from_table` (both present in one file is reported and the file's `[[line]]` array ignored, since the two arrays cannot be ordered against each other), `hide_empty_rows` beside `hide_empty_lines`; `config show` writes the new names; `LineCfg`/`RawLine`/`resolve_lines` become `RowCfg`/`RawRow`/`resolve_rows` and `render_lines_at` keeps its name (it returns terminal lines); every preset, `examples/garnish.toml`, `docs/`, README, guide and the three skills say `[[row]]`, while two config-golden fixtures keep `[[line]]` to pin the alias; goldens byte-identical (`UPDATE_DOCS=1` for the regenerated files only)
- [ ] `phase-21/layout-types`: `ColCfg { width: Width::{Fr(n), Auto, Cells(n)}, modules, right, justify, valign, box, rows: Vec<RowCfg> }`, `RowCfg` gains `cols`, `gap`, the four `title*` keys and `box`, `Config` gains `boxes`; `RawRow::from_table` gains `col` (and inner `row`) with hand-written per-key fallback and paths `row[i].col[j].row[k]`, the unknown-key list extended; `[box.<name>]` parsed like `[modules.text.<name>]` with bare-key names; validation as SPEC § 4.3 lists (words, the `width` grammar as integer-or-string, `gap`, the caps on columns, inner rows and `title_pad`, unknown or unjoined box names, both forms on a row or a column, nesting, non-adjacent reuse, a title on a boxed row), each with its TOML path; the `bad_list` rule holds for `[[row.col]]` too
- [ ] `phase-21/layout-normalise`: `resolve_rows` turns a row without columns into one `1fr` column carrying its `modules`/`right` so the resolved tree is always the same shape, with `justify` defaulted by position and all-empty columns making a spacer; `docs::config_toml` writes `[[row.col]]`, `[[row.col.row]]` and `[box.<name>]` back and keeps skipping emptied non-spacer rows; a test over every fixture and preset shows the resolved config and the render byte-identical; `config show` round-trips a fixture config that uses every form
- [ ] `phase-21/lines-per-row`: `render_lines_at` returns terminal lines grouped per configured row (a `Vec<Line>` per row, each line a segment list with the element kinds cap, rule, gap, module, separator, title, box edge, which is also what Phase 22's placement map reads), `ends()` takes a block index and count instead of a line index, `keep_blank`, `hide_empty_rows` and the cap choice move to the grouped form, and `Rule::paint` produces segments with known cell ranges; `render_loaded`, `render_plain_at` and `benches/tick.rs` keep their outputs byte-identical (goldens and docs unchanged)
- [ ] `phase-21/layout-columns`: `compose_line` split into lay out columns (`auto` and cell widths first, the free width shared by `fr` with the remainder to the first columns; `checked_div`/`checked_rem` and the `num.rs` helpers, no `as`), compose each column (the flex rule with `right`, `justify` for a lone group, `…` cut or a per-column ticker window: the § 4.1 rule inside a flex column, a lone group as the window, `auto` never scrolling), then join with `gap` and caps with the fill glyph or pattern in every empty cell of a one-line row; left-to-right clamping when the width runs out (gap then column; a column whose gap plus one cell does not fit renders nothing with everything to its right, `GARNISH_DEBUG` logs it), `truncate = false` letting only the last column run past the box, no `fr` column meaning a rule after the last, the fill pattern phased over the whole line; `align_columns` and the `fill = false` splice ported to columns in the same commit (`align = true` per module position, counted from the left in left- and centre-justified columns and from the right in right-justified columns and `right` groups, only between rows with the same column count; `right_justify` a per-column property), so there is never a second alignment implementation; the two-group path deleted with a one-column fast path kept (goldens byte-identical; unit test that shares add up to the width at every width from 10 to 400 and differ by at most one cell); config goldens `columns-one` (each `justify`), `columns-three`, `columns-six`, `columns-widths` (`auto`, cells and `fr` mixed), `columns-ticker` at two instants, each at 80 and 160 columns (one file per width, both files present since orphans are deleted)
- [ ] `phase-21/titles`: the `title*` keys reduced to plain text and capped like `label`; the rule segments from `lines-per-row` carry the title after the left cap, before the right cap, or centred in the widest empty gap; plain text at the same place under `fill = false` or `style = "none"`; the first line of a multi-line row; the `title*` keys of a `box = true` row titling that box; cut with `…` when wider than its space, never widening the line; a title-only row is a titled spacer; config goldens `title-rows` (each `title_justify` on a spacer and on a module row, at 60 and 120 columns)
- [ ] `phase-21/boxes`: `[box.<name>]` (`title*`, `style` and `color` inheriting from `[frame]`, `fill` defaulting to `false`, an unstyled box `rounded` when the frame has no box shape, an unjoined box reported); adjacent rows with one name form a box, `box = true` boxes a row alone, a column-level `box` (name or `true`) spans the outer row's height, a boxed one-line column is a three-line box; drawn as a corner-capped top rule with the title, side glyphs at both ends of each line in place of the frame's caps, and a bottom rule (two extra lines, so a box is at least three lines); `FrameChars` gains corners and `side` per built-in style in `for_style` (`none` invisible, `powerline` reported and drawn rounded), `RawFrame::from_table` and `FRAME_KEYS` the five `custom` keys, reduced to plain text and one-cell-checked like the caps (`src/config/mod.rs:520`); the frame's first/last caps treat a multi-line row as one block; nesting reported in both directions; `hide_empty_rows` drops a box whose rows all went; config goldens `boxes-two` (two titled boxes around unboxed rows), `box-columns` (a three-column row inside a box) and `box-column` (a boxed column beside a bare one) at two widths; a unit test that every box line is exactly the box width
- [ ] `phase-21/stacks`: `[[row.col.row]]` laid out to the column's width (inner `justify` overriding the column's), the outer row's height as the tallest column (a row's height its content's lines, a column's the sum of its rows'), `valign` padding, spaces in gap cells and padding lines of a multi-line row, `blank` on the outer row keeping every line, the outer caps on every non-box line; `hide_empty_rows` per inner row and for the whole row, an emptied column keeping its share beside a sibling that stayed; unit test that every line is exactly the box width over widths 10–400 with stacks of unequal height; config goldens `dashboard` (the SPEC sample: a full-height double box, a bare centred column, three stacked boxes) at 60 and 120 columns, `stack-valign` and `stack-hidden` (an inner row that renders nothing)
- [ ] Presets `grid-three`, `grid-six`, `boxed-panels` and `dashboard-panels` (declared widths chosen from the rendered fixture so every column holds its widest render without `…` at three instants, none of them scrolling, since `tests/presets.rs` fails on any `…` and its ticker-advance check assumes one window; `gallery::FILES` grows to 19 in alphabetical order, its unit test compares it with the directory); `docs/config.md` gains a `[[row.col]]` section (widths, `justify`, stacks), `title` rows and a `[box.<name>]` section, each with a sample at two widths; README layout paragraph and guide § 5 rewritten around "a row is columns, one or more lines tall"; the `garnish-statusline` skill's question table and examples updated for columns, `width`, `justify`, titles and boxes (it hand-writes the TOML, so nothing else would catch it going stale); CHANGELOG
- [ ] Bench: `tick_in_process_columns` (six columns), `tick_in_process_boxes` (two boxes) and `tick_in_process_dashboard`; layout is arithmetic over the rendered segments, so the warm default tick is untouched (a criterion comparison of the in-process tick before and after, where a 0.05 ms difference is measurable; hyperfine stays the budget gate); adversarial review (a column narrower than a module, `fr` totals and cell widths overflowing the box, zero free width, `auto` columns wider than the box, a row under `hide_empty_rows` and `stale_style = "hide"`, a title wider than the line, a box at the minimum width of 10, a box whose title is a module id, `blank` on a titled spacer, a multi-line row of bare columns with colour off, a boxed column with no rows, a stack that scrolls under `overflow = "ticker"`, a file mixing `[[line]]` and `[[row]]`); work log

### Phase 22 — Interactive setup (SPEC § 14)

Decided 2026-09-12 with Daniel: a full-screen `garnish setup` in the
terminal, ccstatusline's shape with garnish's exact preview and
schema-generated editors. `ratatui` + `crossterm` are the one new
dependency pair (crate map row in `CLAUDE.md` when the first layer lands).

Code map (2026-09-12): `cli::Command` (`src/cli.rs:90-171`) with
`Preview`/`RenderArgs` (`:29-101`, `preview()` at `:534`); `Render` reads
stdin unconditionally (`:252`) and `tests/cli.rs::run()` never gives a tty;
`cli::install` (`:411-470`) writes settings, config and skills while
printing to a locked stdout; `docs::config_toml` (`src/docs.rs:90`)
serialises a resolved `Config`, so a saved draft loses a hand-written
file's comments; payload fixtures are embedded only in `docs::sample_fixture`
(`src/docs.rs:354`) and `benches/tick.rs:13`, everything else reads
`tests/fixtures/payloads/` from disk; `Cargo.toml` has no `[features]` and
`panic = "abort"` in release (`:71`), so a panic hook runs but nothing
unwinds; `clippy::exit` and `clippy::panic` are denied and `missing_docs`
gates rustdoc.

- [ ] `phase-22/fixtures`: the embedded fixture table moves out of `docs.rs` into a `pub` `fixtures.rs` (name, `include_str!`) shared by the docs samples, `benches/tick.rs` and the preview pane, listing the fixtures SPEC § 14 names; a unit test that every embedded file equals the one on disk
- [ ] `phase-22/setup-shell`: the crates, `src/setup/` module tree (every `pub` item documented), `garnish setup` opens a home screen (*Pick a preset*, *Build a custom layout*, *Install*, *Quit*) and quits cleanly through `Quiet`/`ExitCode`, never `process::exit`; a `Drop` guard restores the terminal on the normal path and a panic hook chained ahead of color-eyre's does it on a panic (it runs before the release profile's abort; the unwinding test is dev-profile only); `setup` without `--preset` and without a tty on stdout exits 1 with one line; `bench/run.sh` unchanged (note the cold-start delta in the commit; a `setup` cargo feature is the fallback)
- [ ] `phase-22/tty-pointer`: the bare `garnish` checks `std::io::IsTerminal` on stdin and prints the one-line pointer, exit 0; the explicit `render` always reads stdin; a `GARNISH_STDIN_TTY` test hook (documented in SPEC § 9) forces the decision so `tests/cli.rs` can cover both paths without a pty
- [ ] `phase-22/setup-preview`: a second painter target in `ansi.rs` turning segments into ratatui spans (ratatui interprets no escape bytes; no new crate), with a unit test that the span text and styles agree with `Painter::paint`'s output; the preview pane over the Phase 21 `lines-per-row` output at `w − 4 − padding` with the live clock and the embedded fixtures (`f` cycles, `w` sets a terminal width, `padding` edits re-shrink), honouring the config's `color` and `NO_COLOR` with the "colours off" status line, a minimum size message below 60 × 12 and a redraw on resize; the snapshot harness over ratatui's `TestBackend` with goldens under `tests/golden/setup/` at 80×24 and 140×40 (`UPDATE_GOLDEN=1`, `GARNISH_NOW` frozen and `TZ=UTC` pinned, the row-start guards stripping escape prefixes), key sequences driven through the event loop
- [ ] `phase-22/setup-gallery`: the preset picker (built-ins plus `gallery::PRESETS`) with summary, declared width, `needs`, the narrower-than-declared warning and the taller-than-the-fullscreen-budget warning (`⌊LINES / 2⌋ − 5` rows, SPEC § 2.1, with a unit test at the threshold: 7 at 24 rows, 20 at 50; the row count stated either way); `Enter` writes with `install`'s never-clobbered backup and offers install; `e` opens the builder; `setup --preset <name> [--install]` never opens the screen, and `setup` without `--preset` and without a tty on stdout exits 1 with one line (`tests/cli.rs`)
- [ ] `phase-22/setup-builder`: the line list (add, insert, delete, clone, move, spacer), a line shown as its columns side by side (SPEC § 4.3): *Add a column* with its `width` and `justify`, *Stack* to turn a column into lines, *Add a title*, *Wrap in a box* over a selected run and *Box the column* (titles and box edges in the placement map), moves within and between columns, the module picker with fuzzy and initialism search over the 21 ids, the existing `text.<name>` tables and *New text module…* (a name checked by the § 3.7 rule, the table created with schema defaults; removing a last placement asks whether to drop the table; unit tests on the matcher), `Esc` closing the innermost layer only, the top-level and `[colors]` screens; the draft is a resolved `Config` and `s` saves it through `docs::config_toml` with `install`'s backup (the status bar says a hand-written file's comments live on in the backup only), `q` asks once on a dirty draft, a changed `(mtime, len)` on disk (or a file absent at open) asks overwrite-or-reload, a failed write shows the OS error and keeps the draft, an unparsable file is never overwritten
- [ ] `phase-22/setup-selection`: the placement map computed from the Phase 21 `lines-per-row` output (each segment already carries its element kind; this layer adds the module id and cell ranges: several per module across a ticker wrap, the `…` cell owned by the cut module, an empty module owning none; measured through `Segment::width()`, never byte offsets; unit test that the ranges tile each row and match the painted widths for flex, multi-column, stacked and ticker lines); crossterm mouse capture on entry and off on exit (also on panic), click and wheel handling, `Tab`/`Shift-Tab`/arrows as the keyboard twins; the selection highlighted in the preview (inverse video) and on the chip; snapshot tests driving synthetic mouse events through `TestBackend`
- [ ] `phase-22/setup-module-editor`: the overlay form generated from `ModuleSchema` (checkboxes for booleans, radio lists for `preset` and enums, steppers with `max`, colour swatches plus a validated custom entry, text-module schema on the same screen), re-rendering the preview on every change, a dot on chips that carry overrides; the unit test that every `OptSpec` kind and every top-level key has a form row
- [ ] `phase-22/setup-pickers`: the string picker seeded from the distinct values in the built-in presets, the frame tables and `gallery::PRESETS` (deduplicated at start-up, each drawn as it renders) plus *custom…* through the config parser's plain-text and width checks; the glyph picker with one row per icon set, the schema's `IconSpec.suggestions` per key, `doctor`'s two-cell `|` marker on every candidate and *custom…*, writing per-key `[modules.<id>.icons]` overrides; `suggestions` added to the schemas for every icon key (a few per set) and covered by the existing glyph guard test; `garnish docs` lists them as *also try* on each module page (`make docs`, `docs_sync`)
- [ ] `phase-22/setup-install`: the plan-and-apply core of `cli::install` extracted from its stdout reporting (a `Plan` the CLI prints and the screen lists) so the install screen runs the same code (`--dry-run` summary, one confirmation) and `setup --preset … --install` needs no shell-out; the doctor's settings report in the status bar; `garnish-statusline` skill names `setup`; README "Set up" section and guide § 2 rewritten around it; CHANGELOG
- [ ] Adversarial review of the phase (terminal left raw, a draft that `config check` would reject, a preset applied at a width where the terminal cuts it, `Ctrl+C` mid-save), tests for every bug found; work log

## Backlog

Open items only; closed ones are in the work log.

**Waiting on Daniel**

- [ ] First release through the pipeline (`v0.3.0`): needs the `release` environment (required reviewer Daniel) on the repo and the merged `garnish.sts.yaml` in the tap; afterwards drop the "lands with the first release" note from the tap's README, and the "from the first tagged release" qualifier from README § Install and guide § 1
- [ ] Watch a nine-line status line at 24 and 50 rows in Claude Code's fullscreen and classic renderers (`/tui`) to confirm the § 2.1 arithmetic (`⌊LINES / 2⌋ − 5` rows whole with an empty prompt; the classic frame scrolling), then drop "read, not watched" from SPEC § 2.1 and `CLAUDE.md`
- [ ] Whether `preview <dir>`'s heading should honour `--color never` (SPEC § 7 does not say; the test compares the plain heading)
- [ ] How far to go in refusing a hostile `.git/config`. The audit's review found that refusing a `-` remote closed one door and left others: the same file sets `core.fsmonitor` (a command `git status` runs) and `remote.<name>.uploadpack` (a command a fetch runs), and both are now overridden on the command line, which costs nothing real. Three remain, and each is a setting a user may genuinely want honoured in their own repositories: `core.sshCommand`, `core.gitProxy`, and an `ext::` remote URL. All three need `fetch_interval > 0`, which is opt-in and defaults to 0, so nothing reaches them by default. The options are to clear them too (safe against an unpacked archive, breaks a custom ssh command or proxy), to refuse to fetch at all when the repository is not owned by the user (git's own `safe.directory` answer), or to leave them and say so in the `fetch_interval` docs

**Parked designs** (decided, not to be reopened without a reason)

- [ ] A `branch.forge = "auto" | "github" | "gitlab"` key, if a self-hosted GitLab with no open merge request ever turns up in practice. Decided 2026-09-16 with Daniel: documented as a known limitation instead (SPEC § 3.1, the `link` option text and `examples/garnish.toml`)
- [ ] `parse_settings_json` accepts files Claude Code rejects: its schema is strict for every key (an enum's spelling, a number's type), and a user, project or local file that fails it is skipped entirely, so `doctor` can show `ok` and resolve keys from a file Claude Code never reads. The `tui` row names its own case; the general check would mean carrying Claude Code's schema, which is a spec decision
- [ ] Optional headroom (Phase 8 analysis): cache the resolved config keyed by mtime, cache the settings-chain reads for 30 s, but only if the tick budget is ever threatened. The chain is already read at most once per tick and only on demand, so this is about the config parse
- [ ] From `FUTURE-SPEC.md` (PR #27, reviewed 2026-09-12): the Tier A ideas not taken into Phases 19–20 stay in that document until asked for (A2, the `center` group, is answered by the Phase 21 layout): `hide = [...]` lists (A4), `[format]` number styles and `dim = "parens"` (A6), separator colour inheritance (A13), a `version` module (A12; it would grow the fixed set), settings-derived `sandbox`/`voice`/`account` modules (A9), pace and burn on the limits (N11), theme rotation (§ 12.3), `config share`/`apply` and `preview --html` (§ 12.2), gradients (A3) and Powerline segments (B1). Everything Tier B/C (workers, hooks, network, transcript, the companion, garlic) is a § 0 decision there, untouched. Once this plan's phases start, FUTURE-SPEC should lose the sections they adopted (§ 6.2, § 12.4, § 13), per its own rule

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
  the work below; every defect got a test, and the suite went 194 → 225.

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

  **Nine silent or wrong renders.** A failed or overdue module built wholly
  from its cache entry lost its `✗`/`⟳` mark, so a broken git read as an
  empty row; `pr` underlined its number whenever `link = true`, even with
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
  `branch` appeared in none of the 472 goldens, in no matrix case and in no
  benchmark (`render_module/sync` was timing an early return). The git
  helpers ran under the developer's `~/.gitconfig`, which on this project
  means `commit.gpgsign`. The payload goldens and the generated docs had no
  orphan check. The "unwritable cache" case was a no-op as root. And
  `Cache::from_env`'s precedence chain had no test: the one named for it
  asserted `key_hash` and `sanitize`.

  **CI** gained the shellcheck gate `CLAUDE.md` has always required (about
  1,100 lines of shell, none of it checked), `permissions: contents: read`
  on `ci.yml`, and an explicit `ref:` on the review workflow's checkout,
  whose three comment triggers were reading `main`'s tree.

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
  frozen). 223 → 225 tests.
