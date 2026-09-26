# PLAN.md — the drift between SPEC and the code

`SPEC.md` is the target design. This file holds what the code still lacks
of it (the open phase, if any), what has landed, and the backlog. How the
project got here is `WORKLOG.md`; how to work here is `CLAUDE.md`.

## Where things stand

**There is no open phase.** Everything in `SPEC.md` is implemented except
the *When asked* items below; where Phase 22 was built differently from its
design, SPEC § 14 says so.

`v0.2.0` (2026-09-06) shipped Phases 0–18. Phases 19–23, the setup
refinements and the fixes from the 2026-09-25 whole-codebase review (12
area reviewers, ~270 findings; #85, #86) are on `main`, unreleased. The
first release through the Homebrew pipeline will be `v0.3.0`;
`CHANGELOG.md` § Unreleased is its section.

## Done

| phase | landed | when |
|---|---|---|
| 0–9 | scaffold and strict lints; payload, time, ANSI; schema-driven config with presets, themes, icon sets, frames; 21 modules; cache and detached workers; direct `.git` reads; generated docs; `install`, `doctor`, `gc`; benches; hardening; `v0.1.0` | 09-04 |
| 10–15 | CI on Linux and macOS with SHA-pinned actions; host detection and `setup.sh`; `align`, `durations`; line keys, spacers, bars; per-key config fallback; the ticker and `text.<name>` modules | 09-05 |
| 16–18 | animation; the presets gallery; three bundled skills; `v0.2.0` | 09-06 |
| Release pipeline | tag → verify → pre-release → binaries → cask → approval → tap → promote | 09-11 |
| 19 Harness fidelity | `prefersReducedMotion`, `install::replace_file`, doctor's settings chain, faint `preview`, the three-renderer height rule | 09-13 |
| 20 Presentation | `COMMON_OPTS` with `max_width`, the module matrix test, `path.style = "fish"`, links, `context.scale`, `reset` modes | 09-13 |
| Audit | code read against its documents: escapes, unbounded reads, wrong renders; 194 → 227 tests | 09-16 |
| 21 Layout | `[[row]]`, `[[row.col]]`, stacks, titles, `[box.<name>]`; `layout.rs` | 09-17 |
| 22 Interactive setup | `garnish setup`: preset picker, builder, editors, pickers, install screen; ratatui | 09-19 |
| Consolidation | 28 gallery presets; the review workflow's final shape; documents brought to the code | 09-19 |
| 23 Usage views | `hide` rules, `[format]`, pace/eta on the limit windows, `separator_color`, `version`/`sandbox`/`voice`/`account` (25 modules), 32 presets | 09-19 |
| Setup refinements | undo/redo, clickable hints, six form bugs, column and box keys | 09-20 |
| Review 2026-09-25 | hardened `.git` reads and git calls, private cache root, config location rules, `config/` and `setup/app` split, ~80 setup and layout fixes, workflow hardening; 319 → 555 tests | 09-25 |

## Backlog

Open items only; closed ones are in `WORKLOG.md`.

**Waiting on Daniel**

- [ ] First release (`v0.3.0`): create the `release` environment (required
  reviewer Daniel) and merge `garnish.sts.yaml` in the tap. Afterwards drop
  the "once the first release is tagged" note from README and guide § 1
  and the tap's README.
- [ ] Watch a nine-row status line at 24 and 50 rows in the fullscreen and
  classic renderers (`/tui`) to confirm `⌊LINES / 2⌋ − 5`; then drop "read,
  not watched" from SPEC § 2.1.
- [ ] Should `preview <dir>`'s heading honour `--color never`? (SPEC § 7 is
  silent.)
- [ ] The `fetch` path with a hostile `.git/config`: the tick and default
  workers run nothing a repository names, but with the opt-in
  `fetch_interval > 0` git can still reach `core.sshCommand`,
  `core.gitProxy`, `ext::` URLs, hooks, `credential.helper`, `core.askPass`
  and `core.alternateRefsCommand`. Clear them, refuse to fetch in a
  repository the user does not own, or document it.
- [ ] A gateway session with only a spend limit hides `cost` (`rate_limits`
  present ⇒ subscription). Alternative: `spend` in `minimal` and `compact`,
  after a spend-only fixture.
- [ ] `valign = "bottom"` in a `fill = true` row leaves blank padding lines,
  not rule (`sidebar-panels`). Should bare columns carry the rule?
- [ ] Config location: a config a checkout's own `.claude/` settings name is
  refused by every hand command. Alternatives: follow it for readers only
  (tried; made `config path` wrong for one side) or trust projects Claude
  Code trusts (`hasTrustDialogAccepted`, undocumented). Related, all safe
  today: a shell-exported `GARNISH_CONFIG` is refused inside a session;
  `~/` in a settings `env` value is refused, not expanded; the tick reads
  no `managed-settings.d` drop-in; a hand command in a subdirectory skips
  the checkout's files; `install --settings <link>` replaces the link.
- [ ] serde_json without `float_roundtrip` reads a few floats slightly off
  (`6e26`), and `install` writes them back.
- [ ] Layout edge cases: a `width = 0` unboxed stack keeps its height; a
  last column cut to nothing still counts as content; freed rule in a
  `fill = true` box lands after the last column; unsettled row heights use
  the narrowest caps; pads are judged by cells given, not drawn.
- [ ] The ascii `pending` PR glyph `..` is also the ascii ellipsis.

**When asked** (small; none blocks anything)

- [ ] `preview` reads git and the settings chain like the tick; the setup
  pane does not. Should the CLI preview skip them too?
- [ ] A `setup` cargo feature if binary size matters (2.8 → 3.4 MB with
  ratatui; tick time unchanged).
- [ ] The glyph picker shows cell counts (`|1`, `|2`) rather than the
  doctor's `|` grid the spec describes.
- [ ] Keyboard selection of separators, caps and rules in the preview;
  clicks on titles and box edges inside a row of columns land on the outer
  row.
- [ ] Snapshot tests see symbols only: a `cell_modifiers` helper would pin
  the inverse video on selections.
- [ ] `tests/presets.rs` reads `ticker_step` as an integer.
- [ ] Undo: `w` and `f` are not edits; history is dropped when a preset is
  applied. Untested: a two-cell glyph under the cursor, undo after reload.
- [ ] Git and cache: fetch-age hint with no success yet; `LockGuard::adopt`
  renames over any lock; on Linux a tick that cannot lock only logs it;
  `upstream()` assumes the default refspec; `doctor` and the logs create
  the cache root with default permissions; `diff-files` can hash an
  exactly-matching racily-clean entry; the section skipper is slower than a
  plain parse on one crafted 1 MiB shape (11 ms vs 7).
- [ ] Setup: one `post_edit` step; SIGTERM leaves the terminal broken
  (needs a signal crate); some pickers offer values refused in context.
- [ ] Config: an emptied single-row stack round-trips as a bare column; a
  config whose rows all vanish gets the default rows back; one nested-box
  mistake is reported twice; a key named `--config` loses its file on the
  `⚠` row.
- [ ] Layout: a boxed stack as the last column gets a cap pad; the prefix
  pad is always drawn (`── ────`); a visible `[frame] pad` shows on padding
  lines; `custom` caps wider than `COLUMNS=10` are recut to `…`; a column
  facing a gap under a rule reserves a fill cell.
- [ ] Install: `--dry-run` says "would write" for up-to-date skills;
  `Steps::apply` does not re-read the file; a reinstall without
  `--absolute` writes the bare `garnish`; an explicit `--config` reinstall
  drops the old prefix; `exec garnish`, `env -i garnish` and similar read
  as not garnish.
- [ ] A worker given a missing `--config` runs on the defaults silently;
  `head_from_git` cuts names past 4096 chars where `head()` refuses.
- [ ] Absurd payload numbers print long durations and token counts.
- [ ] The module key scan cannot find a key declared but never read.
- [ ] `docs/config.md`'s Environment table is hand-written; `docs.rs` has
  two module writers that could share one.
- [ ] A `Quiet` error could skip capturing a backtrace (0.7 s under
  `RUST_BACKTRACE=1` in a debug build).
- [ ] `animated-dots` could use Nerd moon glyphs once checked.

**Parked designs** (decided; reopen only with a reason)

- [ ] `branch.forge = "auto" | "github" | "gitlab"`, if a self-hosted GitLab
  without an open MR turns up (documented as a limitation, SPEC § 3.1).
- [ ] Checking settings files against Claude Code's full schema (only the
  `tui` case is handled).
- [ ] Caching the resolved config and settings reads, only if the tick
  budget is threatened.
- [ ] `FUTURE-SPEC.md` Tier A leftovers (theme rotation, `config
  share`/`apply`, `preview --config`/`--html`, gradients, Powerline
  segments, `provider`, `remote`); Tier B/C untouched.
