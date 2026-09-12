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

Code map (2026-09-12): rows become escape text only in `render_loaded`
(`src/render.rs:53-61`, `painter.paint(line)` per row); `Painter::PLAIN`
serves the docs; both golden suites run `--color never`
(`tests/golden.rs:27`, `tests/config_golden.rs:152`) and their `⚠ garnish:`
guards match row starts; `config.animate` is a plain `bool`
(`src/config/mod.rs:278`, `:967`) combined with `Clock.animate` once at
`src/render.rs:175`; `claude_settings::settings_files`/`resolve`
(`src/claude_settings.rs:84-130`) are shaped for the autocompact keys only;
`doctor::settings_section` (`src/doctor.rs:46`) reads the user settings file
alone and `report_with` returns one `String`; `install::merge`
(`src/install.rs:60`) already rejects bad JSON but `src/cli.rs:445` turns it
into an eyre report; `config init --force` (`src/cli.rs:597-635`) never
reads the target; `IsTerminal` is used nowhere.

- [ ] `phase-19/dim-reset`: confirm on screen that the harness wraps each row in SGR 2 (2.1.261 `<Text dimColor wrap="truncate">`; re-locate it in the current binary); prefix each row in `render_loaded` when `mode != ColorMode::Never` (not in `Painter::paint`, which the plain docs path shares, and not as a `Segment`, since `keep_blank` decides "whitespace only" from `Segment::text()`); a unit test that `--color never` emits no prefix and one that paints an unframed `fill = false` spacer and finds it; the fact in `CLAUDE.md` with how to re-verify; SPEC § 4.1's `blank` wording and the guide note
- [ ] `phase-19/colour-goldens`: the goldens cannot pin the prefix while they render with `--color never`: add a `# color:` header key to `tests/config_golden.rs` (`HEADER_KEYS`, `Header::parse`, `render()`), one colour-on golden (`dim-reset`), and make the `⚠ garnish:` row-start guards in both suites strip a leading `ESC[0m` first; Phase 20's link goldens and Phase 22's snapshots need the same mode
- [ ] `phase-19/animate-option`: `RawConfig.animate` becomes `Option<bool>` plumbed through `Config` so "set explicitly" is knowable; `config show` (`src/docs.rs:69-71`) prints the effective value; `examples/garnish.toml` and `tests/docs_sync.rs::example_config_matches_config_init` follow; goldens byte-identical
- [ ] `phase-19/reduced-motion`: a `prefersReducedMotion` reader on the settings chain next to the autocompact keys (`claude_settings` gains a second resolved key, same file order); the combination at `src/render.rs:175` becomes `clock.animate && config.animate.unwrap_or(!reduced)` (env, config, settings, default); `Clock::fixed()` must not touch the filesystem (docs and goldens); unit tests for each precedence pair, config golden `reduced-motion` with `# env:`
- [ ] `phase-19/never-rewrite`: `cli::install` maps `merge`'s parse error to `Quiet` after one stderr line instead of an eyre report; `config init --force` gains a read-and-`toml::from_str` probe before writing; `tests/cli.rs` covers both; the temp-and-rename write is already asserted (`src/install.rs:217`)
- [ ] `phase-19/doctor-settings`: extract the settings section into a testable function returning rows (the `glyph_rows` precedent, `src/doctor.rs:231`), give `report_with` the project dir so it can walk `settings_files` (caller `src/cli.rs:290`), then report `statusLine.refreshInterval` (suggest `1` when `clock`, a countdown or an animation is configured), `statusLine.hideVimModeIndicator` (suggest `true` when `vim` is on), `disableAllHooks`, `prefersReducedMotion`, and whether each file parses; unit tests over a settings fixture directory
- [ ] Verify item 2 of FUTURE-SPEC § 4.9: re-check the 13 000 autocompact constant in the current binary; keep `compact_buffer_tokens` either way; note the version in SPEC § 2.3 and `CLAUDE.md`
- [ ] Verify the status line's height: how many rows the harness shows before it caps, scrolls or squeezes the transcript (the footer's Ink box, `LINES` in the script's environment, a nine-row render on screen at a 24-row and a 50-row terminal); record the rule in SPEC § 2.1 and `CLAUDE.md`; it decides whether the § 14 picker warns on height as it does on width, and whether a multi-row line (§ 4.3) needs a cap of its own
- [ ] Docs (`make docs`, README/guide troubleshooting: "the line looks dimmer than `preview`" goes away), CHANGELOG `## Unreleased`, adversarial review, work log

(A 24 h lock horizon from FUTURE-SPEC § 15 was on this list and was dropped
in the spec review: `LOCK_STALE_MS` already bounds a lock's life.)

### Phase 20 — Per-module presentation (SPEC § 3, § 3.7, § 9)

Code map (2026-09-12): the common options are parsed by hand
(`src/config/mod.rs:1239-1267`, `COMMON_KEYS` is `[&str; 8]` at
`src/config/schema.rs:311`) and applied by `modules::decorate`
(`src/modules/mod.rs:379-431`) from `render_group` (`src/render.rs:364`);
`ansi::truncate` (`src/ansi.rs:320`) keeps `link` per segment and
`Painter::paint` opens and closes OSC 8 per segment, and `Painter.links`
is off under `--color never` (`src/render.rs:48`), so no golden can hold a
link today; `repo.rs` has `shorten` (`:48`), `depth` (`:95`) and a `branch`
`max_length` character cap (`:386`), and nothing reads `workspace.repo`;
`context::compaction_percent` (`src/modules/context.rs:200`) returns a
window percentage and early-returns when the marker is off; the three usage
modules come from one schema builder (`src/modules/usage.rs:44-62`) with
`show_reset` at `:75` and the countdown at `:162`; `Ctx.tz` exists
(`src/render.rs:169`) but only durations are formatted.

- [ ] `phase-20/common-opts`: a `COMMON_OPTS` table of `OptSpec`s (`label`, `prefix`, `suffix`, `hide_when_empty`, `max_width`) replaces the hand-parse so caps go through `over_max` and the reference's common-key table prints them; `COMMON_KEYS` grows to nine; `write_modules` (`src/docs.rs:241`) and the docs page follow; `max_width` rejected on text modules next to the text-name check (`src/config/mod.rs:997`) with a message naming `width`
- [ ] `phase-20/max-width`: applied after `decorate` and before `align_columns` (between `src/render.rs:207` and `:210`) through `ansi::truncate`; balanced OSC 8 asserted on painted output, not on segments; `branch.max_length` stays the per-module character cap and is documented as such; unit tests on a linked `pr` and a wide branch name; config golden `max-width`
- [ ] `phase-20/schema-matrix` (FUTURE-SPEC § 15 item 11): an in-crate test (it inspects `Segment::text`, private outside the crate) generated from `ModuleSchema` over module × preset × icon set × `max_width ∈ {0, 1, 4, 12}` × fixture asserting width ≤ `max_width`, nothing rendered for a hidden state, balanced OSC 8 on painted output, no escape bytes in text; rayon like `src/render.rs:1013`, under the longer nextest budget
- [ ] `phase-20/path-style`: `style = "full" | "fish"` (A7) declared in `repo.rs`'s schema (the source scan checks keys against the schemas in the same file) and applied after `shorten`; unit tests on `~`, a root path, a one-segment base and `depth = 2` with `fish`; config golden `path-fish`
- [ ] `phase-20/links`: `payload.rs` gains `workspace.repo.{host,owner,name}` if it lacks it; `branch` `link = true` (A8) builds the URL with a hand-written percent-encoder (unreserved and `/` kept; no indexing, no `as`) and GitLab `/-/tree/`, nothing when `repo` is absent or the head is detached; `text.<name>` `url` through the painter's `http(s)://` rule with `config check` reporting anything else; unit tests including `feature/#12` and a non-ASCII name; goldens `branch-link` (on `git-worktree`, which carries `repo`) and `text-link` under the Phase 19 `# color:` mode, since `--color never` paints no links
- [ ] `phase-20/context-scale`: factor the threshold out of `compaction_percent` so it is available with the marker off, then `scale = "usable"` (A11): percentage and bar against the § 2.3 threshold, marker hidden, bands on the displayed percentage, falls back to `window` when compaction is disabled or the threshold is under a tenth of the window; unit tests at the threshold edges and the two fallbacks, config golden `context-usable` at 80 % and 96 % of 1M
- [ ] `phase-20/reset-absolute`: a jiff wall-clock formatter in `time.rs` over `Clock.tz`, reached through a new `Ctx` method beside `countdown`; `reset = "countdown" | "absolute" | "both"` (A10) on the shared usage schema with the weekday rule as `limit7d`'s per-id branch in the builder (never on `limit5h` or `spend`); `show_reset = false` hides every form; unit tests at two instants and two zones, config golden `reset-absolute` pinned at two `# now:` values
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

Code map (2026-09-12): `LineCfg` (`src/config/mod.rs:48-62`) is flat;
`RawConfig::from_table` handles an array of tables only for `line`
(`:414-427`) and `RawLine::from_table` (`:556-585`) matches keys by name
with a literal unknown-key list, so `[[line.col]]` and
`[[line.col.line]]` need hand-written recursion with the `field()`
discipline (`:590-609`: report the path, keep the rest; a serde-derived
column would discard a whole line on one bad key); `resolve_lines`
(`:861`) is a 1:1 map with the spacer rule and the `bad_list` guard
(`:549`); `frame::FrameChars` (`src/frame.rs:66-89`) has caps only,
`ends(index, count)` (`:160`) assumes one row per line, `Rule::paint`
(`:202`) returns a `String` with no cell ranges, and `compose_line`
(`:246-358`) owns the left budget, the truncate-or-scroll choice and the
final row cut; `render_lines_at` (`src/render.rs:154-259`) applies
`align_columns` (`:336`) and the `fill = false` splice (`:214-233`) over
`lefts`/`rights`, then `hide_empty_lines`, `keep_blank` and the cap index
per composed row; `docs::config_toml` writes `[[line]]` at
`src/docs.rs:167-185`; `gallery::FILES` is a fixed-size sorted array
(`src/gallery.rs:14`); config goldens are keyed by `(name, now)` with one
`# columns:` per file and orphans deleted (`tests/config_golden.rs:225`).

- [ ] `phase-21/layout-types`: `ColCfg { width: Width::{Fr(n), Auto, Cells(n)}, modules, right, justify, valign, box, lines: Vec<LineCfg> }`, `LineCfg` gains `cols`, `gap`, the four `title*` keys and `box`, `Config` gains `boxes`; `RawLine::from_table` gains `col` (and inner `line`) with hand-written per-key fallback and paths `line[i].col[j].line[k]`, the unknown-key list extended; `[box.<name>]` parsed like `[modules.text.<name>]` with bare-key names; validation as SPEC § 4.3 lists (words, the `width` grammar as integer-or-string, `gap`, the caps on columns, inner lines and `title_pad`, unknown or unjoined box names, both forms on a line or a column, nesting, non-adjacent reuse, a title on a boxed line), each with its TOML path; the `bad_list` rule holds for `[[line.col]]` too
- [ ] `phase-21/layout-normalise`: `resolve_lines` turns a line without columns into one `1fr` column carrying its `modules`/`right` so the resolved tree is always the same shape, with `justify` defaulted by position and all-empty columns making a spacer; `docs::config_toml` writes `[[line.col]]`, `[[line.col.line]]` and `[box.<name>]` back and keeps skipping emptied non-spacer lines; a test over every fixture and preset shows the resolved config and the render byte-identical; `config show` round-trips a fixture config that uses every form
- [ ] `phase-21/rows`: `render_lines_at` returns rows grouped per configured line (a `Vec<Row>` per line, each row a segment list with the element kinds cap, rule, gap, module, separator, title, box edge, which is also what Phase 22's placement map reads), `ends()` takes a block index and count instead of a row index, `keep_blank`, `hide_empty_lines` and the cap choice move to the grouped form, and `Rule::paint` produces segments with known cell ranges; `render_loaded`, `render_plain_at` and `benches/tick.rs` keep their outputs byte-identical (goldens and docs unchanged)
- [ ] `phase-21/layout-columns`: `compose_line` split into lay out columns (`auto` and cell widths first, the free width shared by `fr` with the remainder to the first columns; `checked_div`/`checked_rem` and the `num.rs` helpers, no `as`), compose each column (the flex rule with `right`, `justify` for a lone group, `…` cut or a per-column ticker window: the § 4.1 rule inside a flex column, a lone group as the window, `auto` never scrolling), then join with `gap` and caps with the fill glyph or pattern in every empty cell of a single-row line; left-to-right clamping when the width runs out (each `fr` column at least one cell, a column with nothing left renders nothing and `GARNISH_DEBUG` logs it); `align_columns` and the `fill = false` splice ported to columns in the same commit (`align = true` per column index counted from the justified end and only between lines with the same column count; `right_justify` a per-column property), so there is never a second alignment implementation; the two-group path deleted with a one-column fast path kept (goldens byte-identical; unit test that shares add up to the width at every width from 10 to 400 and differ by at most one cell); config goldens `columns-one` (each `justify`), `columns-three`, `columns-six`, `columns-widths` (`auto`, cells and `fr` mixed), `columns-ticker` at two instants, each at 80 and 160 columns (one file per width, both files present since orphans are deleted)
- [ ] `phase-21/titles`: the `title*` keys reduced to plain text and capped like `label`; the rule segments from `rows` carry the title after the left cap, before the right cap, or centred in the widest empty gap; plain text at the same place under `fill = false` or `style = "none"`; the first row of a multi-row line; cut with `…` when wider than its space, never widening the row; a title-only line is a titled spacer; config goldens `title-rows` (each `title_justify` on a spacer and on a module row, at 60 and 120 columns)
- [ ] `phase-21/boxes`: `[box.<name>]` (`title*`, `style` and `color` inheriting from `[frame]`, `fill` defaulting to `false`, an unstyled box `rounded` when the frame has no box shape, an unjoined box reported); adjacent lines with one name form a box, `box = true` boxes a line alone, a column-level `box` (name or `true`) spans the line's height, a boxed one-row column is a three-row box; drawn as a corner-capped top rule with the title, side glyphs at both ends of each row in place of the frame's caps, and a bottom rule; `FrameChars` gains corners and `side` per built-in style in `for_style` (`none` invisible, `powerline` reported and drawn rounded), `RawFrame::from_table` and `FRAME_KEYS` the five `custom` keys, reduced to plain text and one-cell-checked like the caps (`src/config/mod.rs:520`); the frame's first/last caps treat a multi-row line as one block; `hide_empty_lines` drops a box whose lines all went; config goldens `boxes-two` (two titled boxes around unboxed lines), `box-columns` (a three-column line inside a box) and `box-column` (a boxed column beside a bare one) at two widths; a unit test that every box row is exactly the box width
- [ ] `phase-21/stacks`: `[[line.col.line]]` laid out to the column's width (inner `justify` overriding the column's), the line's height as the tallest column, `valign` padding, spaces in gap cells and padding rows of a multi-row line, `blank` on the outer line keeping every row, the outer caps on every non-box row; `hide_empty_lines` per inner line and for the whole line, an emptied column keeping its share beside a sibling that stayed; unit test that every row is exactly the box width over widths 10–400 with stacks of unequal height; config goldens `dashboard` (the SPEC sample: a full-height double box, a bare centred column, three stacked boxes) at 60 and 120 columns, `stack-valign` and `stack-hidden` (an inner line that renders nothing)
- [ ] Presets `grid-three`, `grid-six`, `boxed-panels` and `dashboard-panels` (declared widths in `60..=400`, `tests/presets.rs`; `gallery::FILES` grows to 19 in alphabetical order, its unit test compares it with the directory); `docs/config.md` gains a `[[line.col]]` section (widths, `justify`, stacks), `title` rows and a `[box.<name>]` section, each with a sample at two widths; README layout paragraph and guide § 5 rewritten around "a line is columns"; the `garnish-statusline` skill's question table and examples updated for columns, `width`, `justify`, titles and boxes (it hand-writes the TOML, so nothing else would catch it going stale); CHANGELOG
- [ ] Bench: `tick_in_process_columns` (six columns), `tick_in_process_boxes` (two boxes) and `tick_in_process_dashboard`; layout is arithmetic over the rendered segments, so the warm default tick is untouched (the one-column path must cost what the two-group path cost: assert within 0.05 ms); adversarial review (a column narrower than a module, `fr` totals and cell widths overflowing the box, zero free width, `auto` columns wider than the box, a line under `hide_empty_lines` and `stale_style = "hide"`, a title wider than the row, a box at the minimum width of 10, a box whose title is a module id, `blank` on a titled spacer, a multi-row line of bare columns with colour off, a boxed column with no lines, a stack that scrolls under `overflow = "ticker"`); work log

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
- [ ] `phase-22/setup-preview`: the preview pane over the Phase 21 `rows` output at `COLUMNS − 4 − padding` with the live clock and the embedded fixtures (`f` cycles, `w` sets a width), honouring the config's `color` and `NO_COLOR`, a minimum size message below 60 × 12 and a redraw on resize; the snapshot harness over ratatui's `TestBackend` with goldens under `tests/golden/setup/` at 80×24 and 140×40 (`UPDATE_GOLDEN=1`, `GARNISH_NOW` frozen and `TZ=UTC` pinned, the row-start guards stripping escape prefixes), key sequences driven through the event loop
- [ ] `phase-22/setup-gallery`: the preset picker (built-ins plus `gallery::PRESETS`) with summary, declared width, `needs` and the narrower-than-declared warning; `Enter` writes with `install`'s never-clobbered backup and offers install; `e` opens the builder; `setup --preset <name> [--install]` never opens the screen, and `setup` without `--preset` and without a tty on stdout exits 1 with one line (`tests/cli.rs`)
- [ ] `phase-22/setup-builder`: the line list (add, insert, delete, clone, move, spacer), a line shown as its columns side by side (SPEC § 4.3): *Add a column* with its `width` and `justify`, *Stack* to turn a column into lines, *Add a title*, *Wrap in a box* over a selected run and *Box the column* (titles and box edges in the placement map), moves within and between columns, the module picker with fuzzy and initialism search over the 21 ids, the existing `text.<name>` tables and *new text module…* (unit tests on the matcher), the top-level and `[colors]` screens; the draft is a resolved `Config` and `s` saves it through `docs::config_toml` with `install`'s backup (the status bar says a hand-written file's comments live on in the backup only), `q` asks once on a dirty draft, an mtime change on disk asks overwrite-or-reload, an unparsable file is never overwritten
- [ ] `phase-22/setup-selection`: the placement map computed from the Phase 21 `rows` output (each segment already carries its element kind; this layer adds the module id and cell ranges: several per module across a ticker wrap, the `…` cell owned by the cut module, an empty module owning none; measured through `Segment::width()`, never byte offsets; unit test that the ranges tile each row and match the painted widths for flex, multi-column, stacked and ticker lines); crossterm mouse capture on entry and off on exit (also on panic), click and wheel handling, `Tab`/`Shift-Tab`/arrows as the keyboard twins; the selection highlighted in the preview (inverse video) and on the chip; snapshot tests driving synthetic mouse events through `TestBackend`
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
