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
open code item was closed and the spec was audited against the code, and
Phases 19–22 were written that day from the FUTURE-SPEC review and
Daniel's layout and setup ideas. Phase 19 (harness fidelity) landed on
2026-09-13 (PR #49): five of its six layers as designed; the sixth, the
per-row dim reset, was dropped after the harness binary showed it cannot
work, and became a spec correction (SPEC § 2.1) plus an open question for
Daniel (§ Backlog). Phase 20 (per-module presentation) was built the same
day on a branch (PR pending): all seven layers as designed, with the four
corners the code settled recorded in SPEC (`max_width` cuts the decorated
module, fish paths keep the `~` that `depth` keeps, the usable scale hides
the marker's percentage too, GitLab is the host's name or an open merge
request). **The drift between SPEC and the code is now Phases 21–22
below.**

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
| 19 Harness fidelity | `animate` as `Option<bool>` following `prefersReducedMotion` over the settings chain, never rewriting an unparsable `settings.json`/`garnish.toml` (`install::replace_file`, backups for `config init --force`), the doctor's settings-chain report with suggestions, the `# color:` golden mode (`colour-on`), `$ROOT` in `# env:`; the dim reset dropped as impossible (SPEC § 2.1), the 13 000 constant re-read in 2.1.270 | 09-12 |
| 20 Presentation | `COMMON_OPTS` (the common keys as bounded specs) with `max_width` cutting the decorated module before alignment, the schema-generated module matrix test, `path.style = "fish"`, `branch.link` and `text.url` through a hand-written percent-encoder with GitLab's `/-/tree/`, `context.scale = "usable"`, `reset = absolute \| both` on the limit modules; seven config goldens | 09-13 |

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
life. Phase 19's own layers are in the done table; what it could not
settle, the on-screen height rule and the dim `preview` question, is in
the backlog.)

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
- [ ] `phase-22/setup-gallery`: the preset picker (built-ins plus `gallery::PRESETS`) with summary, declared width, `needs` and the narrower-than-declared warning; `Enter` writes with `install`'s never-clobbered backup and offers install; `e` opens the builder; `setup --preset <name> [--install]` never opens the screen, and `setup` without `--preset` and without a tty on stdout exits 1 with one line (`tests/cli.rs`)
- [ ] `phase-22/setup-builder`: the line list (add, insert, delete, clone, move, spacer), a line shown as its columns side by side (SPEC § 4.3): *Add a column* with its `width` and `justify`, *Stack* to turn a column into lines, *Add a title*, *Wrap in a box* over a selected run and *Box the column* (titles and box edges in the placement map), moves within and between columns, the module picker with fuzzy and initialism search over the 21 ids, the existing `text.<name>` tables and *New text module…* (a name checked by the § 3.7 rule, the table created with schema defaults; removing a last placement asks whether to drop the table; unit tests on the matcher), `Esc` closing the innermost layer only, the top-level and `[colors]` screens; the draft is a resolved `Config` and `s` saves it through `docs::config_toml` with `install`'s backup (the status bar says a hand-written file's comments live on in the backup only), `q` asks once on a dirty draft, a changed `(mtime, len)` on disk (or a file absent at open) asks overwrite-or-reload, a failed write shows the OS error and keeps the draft, an unparsable file is never overwritten
- [ ] `phase-22/setup-selection`: the placement map computed from the Phase 21 `lines-per-row` output (each segment already carries its element kind; this layer adds the module id and cell ranges: several per module across a ticker wrap, the `…` cell owned by the cut module, an empty module owning none; measured through `Segment::width()`, never byte offsets; unit test that the ranges tile each row and match the painted widths for flex, multi-column, stacked and ticker lines); crossterm mouse capture on entry and off on exit (also on panic), click and wheel handling, `Tab`/`Shift-Tab`/arrows as the keyboard twins; the selection highlighted in the preview (inverse video) and on the chip; snapshot tests driving synthetic mouse events through `TestBackend`
- [ ] `phase-22/setup-module-editor`: the overlay form generated from `ModuleSchema` (checkboxes for booleans, radio lists for `preset` and enums, steppers with `max`, colour swatches plus a validated custom entry, text-module schema on the same screen), re-rendering the preview on every change, a dot on chips that carry overrides; the unit test that every `OptSpec` kind and every top-level key has a form row
- [ ] `phase-22/setup-pickers`: the string picker seeded from the distinct values in the built-in presets, the frame tables and `gallery::PRESETS` (deduplicated at start-up, each drawn as it renders) plus *custom…* through the config parser's plain-text and width checks; the glyph picker with one row per icon set, the schema's `IconSpec.suggestions` per key, `doctor`'s two-cell `|` marker on every candidate and *custom…*, writing per-key `[modules.<id>.icons]` overrides; `suggestions` added to the schemas for every icon key (a few per set) and covered by the existing glyph guard test; `garnish docs` lists them as *also try* on each module page (`make docs`, `docs_sync`)
- [ ] `phase-22/setup-install`: the plan-and-apply core of `cli::install` extracted from its stdout reporting (a `Plan` the CLI prints and the screen lists) so the install screen runs the same code (`--dry-run` summary, one confirmation) and `setup --preset … --install` needs no shell-out; the doctor's settings report in the status bar; `garnish-statusline` skill names `setup`; README "Set up" section and guide § 2 rewritten around it; CHANGELOG
- [ ] Adversarial review of the phase (terminal left raw, a draft that `config check` would reject, a preset applied at a width where the terminal cuts it, `Ctrl+C` mid-save), tests for every bug found; work log

## Backlog

Open items only; closed ones are in the work log.

- [ ] Optional headroom (Phase 8 analysis): cache the resolved config keyed by mtime, cache the settings.json reads for 30 s — only if the tick budget is ever threatened (SPEC § 3.2 says the settings chain is read every tick)
- [ ] First release through the pipeline (`v0.3.0`): needs the `release` environment (required reviewer Daniel) on the repo and the merged `garnish.sts.yaml` in the tap; afterwards drop the "lands with the first release" note from the tap's README
- [ ] Parked from `FUTURE-SPEC.md` (PR #27, reviewed 2026-09-12): the Tier A ideas not taken into Phases 19–20 stay in that document until asked for (A2, the `center` group, is answered by the Phase 21 layout): `hide = [...]` lists (A4), `[format]` number styles and `dim = "parens"` (A6), separator colour inheritance (A13), a `version` module (A12; it would grow the fixed set), settings-derived `sandbox`/`voice`/`account` modules (A9), pace and burn on the limits (N11), theme rotation (§ 12.3), `config share`/`apply` and `preview --html` (§ 12.2), gradients (A3) and Powerline segments (B1). Everything Tier B/C (workers, hooks, network, transcript, the companion, garlic) is a § 0 decision there, untouched. Once this plan's phases start, FUTURE-SPEC should lose the sections they adopted (§ 6.2, § 12.4, § 13), per its own rule
- [ ] Open question from the 2026-09-12 code session: whether `preview <dir>`'s heading should honour `--color never` (SPEC § 7 does not say; the test compares the plain heading)
- [ ] On-screen check of the status line's height (Phase 19 could only read the binary: `LINES` is the terminal height and the component draws every row with no cap of its own): render nine lines at 24 and 50 terminal lines in a real session and record the rule in SPEC § 2.1 and `CLAUDE.md`; it decides whether the § 14 picker warns on height as it does on width, and whether a § 4.3 multi-line row needs a cap of its own

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
  decorations, two levels deep, with a plain line as one `1fr` column so
  the default render is byte-identical. (I first kept the name `[[line]]`
  to avoid a migration; Daniel then drew the distinction that settled it:
  a *line* is one terminal line, a *row* is the addressable unit, one or
  more lines tall, a box three of them, a future art or animation row
  more. So the unit is `[[row]]`, with `[[row.col]]` and
  `[[row.col.row]]` beneath, `[[line]]` and `hide_empty_lines` kept as
  permanent aliases so nothing breaks, and a rename layer opens
  Phase 21.)
  Decided with Daniel: the one `width` key and `gap = 1`. An adversarial
  review of the day's spec text returned 25 findings, all taken (the
  samples contradicted the box rules; the lock horizon could never fire;
  `path.depth` already existed; a dozen corners defined, listed in the
  commit `51d4b66`). Then this plan was compacted to the drift between
  SPEC and the code plus this log. Daniel asked for the spec's
  ambiguities, traps and stale text to be fleshed out, a second review,
  and the phases checked against the code: the "target state" marks came
  off the shipped sections, § 4.3 and § 14 gained edge-case and trap
  lists, a read-only code map (recorded at the top of each open phase)
  re-cut the layers where the code's shape demanded (a colour-on golden
  mode, `animate` as `Option<bool>`, a `COMMON_OPTS` table, a `rows`
  layer with block caps and rule segments that the placement map reads,
  an embedded `fixtures.rs`, the install core apart from its printing,
  a `GARNISH_STDIN_TTY` hook), and a second adversarial review returned
  25 more findings, all taken (commit `cada6ef`: one `truncate = false`
  rule, gap-then-column clamping, `auto` and no-`fr` cases, the fill
  pattern phased over the row, a segment-to-span painter for the preview
  pane, `config show` as a fixed point, both-direction nesting, backups
  on `config init --force` and `setup --preset`). Open drafts #27 and #40
  stay parked.
- **2026-09-12 (Phase 19)** — Harness fidelity, on a branch as five
  commits (no `gh stack` in that session). The verify items came first,
  read from the 2.1.270 binary on the host and the 2.1.261 native npm
  package fetched for comparison: the 13 000 buffer is unchanged in both;
  `COLUMNS`/`LINES` are the full terminal size; and the status line
  component wraps each row in `<Text dimColor wrap="truncate">` around a
  child that parses the row's escapes into per-piece styles, with the Ink
  fork merging the parent's `dim` into every piece, so a leading `ESC[0m`
  is parsed away and the FUTURE-SPEC A1 premise never held in any
  supported version. Decided (documents first): the dim-reset layer was
  not built; SPEC § 2.1 records the mechanism and how to re-verify it,
  the guide's troubleshooting explains the difference from `preview`, and
  whether `preview` should dim its own rows is Daniel's question in the
  backlog. The height rule could be read only as far as the binary goes
  (no cap in the component, `flexShrink: 1` on the footer column); the
  on-screen check stays in the backlog. Then the layers: `animate` as
  `Option<bool>` (`config init` comments the key out like `durations`,
  `show` prints the value in effect, the three round-trip tests compare
  with the switch pinned); `prefersReducedMotion` on the settings chain
  with `Clock.settings` gating the read so `Clock::fixed()` never touches
  a settings file, unit tests per precedence pair and two config goldens
  reaching a settings fixture through `$ROOT` in `# env:`; never-rewrite
  through one `install::replace_file` (backup, temp, rename) with
  `config::syntax_error` for the TOML probe and one-line `Quiet` refusals
  in `install` and `config init --force`; the doctor's settings rows as a
  pure function over a labelled, read chain (`claude_settings::FileState`,
  `FileKeys` grown by the doctor's keys); and the `# color:` golden mode
  with `colour-on` pinning the painter's escapes, the OSC 8 link and the
  colour codes that keep an unframed spacer. An adversarial review of
  the code ran as a five-lens workflow (behaviour, spec, tests,
  hardening, lint) with three refuters per finding; every finding it
  raised was taken, in one fix commit: `replace_file` turned a dangling
  symlink into a regular file (it now follows the link chain, refuses a
  loop, is born with the old file's mode and removes its temp file on any
  failure); the doctor suggested `refreshInterval = 1` for animations
  that were frozen, for limits without `show_reset` and for text that fit
  its box, and stayed silent for a value below 1 (which Claude Code
  drops); the doctor printed a `statusLine.command` from any chain file
  raw (now plain text, 200 characters) and the current directory in full
  (now the heading, the project files relative to it); an empty
  `settings.json` was "invalid" to the doctor and `{}` to `install` (now
  `{}` to both); a settings file was read without a size bound (1 MiB);
  the settings chain was parsed twice per tick with `animate` unset (once
  now, through `Ctx::settings()`, shared with the context module); the
  CLI tests ran in the checkout (the checkout's `.claude/` could leak in)
  and kept the developer's `GARNISH_ANIMATE`; `config show` folded the
  session switch into a printed config (it prints the file-or-settings
  value now); the unit tests could see the machine's managed settings
  file (`Clock.managed`, `settings_chain(managed, …)`); plus the wording
  fixes (`--force` help, the `install` message, the vim suggestion, the
  statusline skill's claim that `--force` keeps no backup, the SPEC
  listing's `animate` comment, this log). Declined: a
  `GARNISH_MANAGED_SETTINGS` test hook for the goldens (a spec decision,
  in the backlog).
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
