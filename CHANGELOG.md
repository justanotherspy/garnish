# Changelog

User-visible changes per release. The tag message for a release is this
file's section for it. `PLAN.md` holds the session-by-session detail.

## 0.2.0 — unreleased (PLAN Phases 12–18)

**Fixes**

- The status line box is `COLUMNS − 4 − 2 × statusLine.padding` cells wide;
  garnish now fills exactly that, so the right edge no longer shows `…`
  (SPEC § 2.1). `garnish install --padding` seeds the matching `padding`.
- Every built-in glyph is one cell in every terminal: East Asian Ambiguous
  and emoji-presentation characters left the unicode and emoji sets, and
  `garnish doctor` prints a glyph grid to check your font (SPEC § 3.6).
- A bad value in the config no longer discards the whole file: each key
  falls back on its own and `config check` lists every problem (SPEC § 5).
- Powerline frames pad their segments; the spacer in `packed` lines is
  consistent; `config init` and `config check` exit quietly on a
  user error instead of printing a report.

**Layout** (SPEC § 4.1)

- `right_justify = "end" | "start"`, `hide_empty_lines`, `[[line]]
  spacer` and `style = "none"` spacers, `bar = "blocks" | "line"` on
  the bar modules.

**Text and motion** (SPEC § 3.7, 4.2)

- `text.<name>` modules: static text with `width`, `pad`, `justify`,
  `overflow = clip | scroll | scroll-wrap`, `step`, `gap`, `color`.
- Line ticker: `overflow = "ticker"` scrolls a line that does not fit,
  `ticker_step`, `ticker_gap`.
- Animation: `animate` (and `GARNISH_ANIMATE=0`), frame rule
  `fill_pattern`/`fill_step`/`fill_direction`, `separator_frames`/
  `separator_step`, `<icon>_frames` on any icon (`spinner_frames`,
  `model_frames`, …). Everything sits on frame 0 when animation is off.

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

## 0.1.0 — 2026-09-05

First release: the 21 modules, presets, themes, icon sets, frames, the
cache and detached workers, `install`, `doctor`, `preview`, `config`, the
generated docs and the golden suites.
