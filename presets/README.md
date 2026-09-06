# Presets gallery

Complete, named configs you can copy to `~/.config/garnish/garnish.toml` (or
point `GARNISH_CONFIG` at). Each file starts with a header the tooling reads
(SPEC § 12): `# name:` (matches the filename), `# summary:`, `# columns:` (the
terminal width it is designed for) and optionally `# needs:` (`nerd-font` or
`emoji`) and `# author:`. Every file passes `garnish config check`
(`tests/presets.rs` enforces it).

The seed set is the configurations walked through live on 2026-09-05. Renders
at each preset's declared width will be generated into `docs/presets.md` once
PLAN Phase 17 lands; until then, try one with:

```sh
garnish preview tests/fixtures/payloads/subscription-full.json --width 120 \
  --config presets/full-aligned.toml --color always
```

| name | summary | columns | needs |
|---|---|---|---|
| `minimal-clean` | one unframed line: path, context, limit, clock | 80 | nerd-font |
| `compact-aligned` | two rounded lines with stacked bars, Catppuccin Mocha | 100 | nerd-font |
| `full-aligned` | every module at full verbosity, columns aligned, fixed timers | 130 | nerd-font |
| `three-lines-double` | repo / model / timers in a double frame | 110 | nerd-font |
| `two-lines-powerline` | location and model only, powerline caps, no colour | 110 | nerd-font |
| `labels-and-placeholders` | labels, brackets, dim `–` for absent modules, UTC clock with date | 170 | nerd-font |
| `bars-and-limits` | 40-cell line-style context bar with window tag, mini bars on the limits | 130 | nerd-font |
| `session-detail` | session, api, cache and cost detail, plain stale style, 1 s git refresh | 130 | nerd-font |
| `packed-heavy` | custom heavy frame, left-packed lines, a separator per line | 130 | nerd-font |
| `dracula-256` | Dracula with role and per-module colour overrides in 256-colour mode | 130 | nerd-font |
| `emoji-overrides` | emoji icons with per-module glyph overrides and name limits | 130 | emoji |
| `single-line-full` | everything on one row, never truncated by garnish (ticker candidate) | 200 | nerd-font |
| `tall-eight-lines` | one module per row, eight rows, square frame | 100 | nerd-font |

Contributing one: the `garnish-submit-preset` skill (SPEC § 13) will do this
interactively; until it exists, open an issue with the file, its header and a
screenshot from your terminal.
