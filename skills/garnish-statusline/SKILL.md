---
name: garnish-statusline
description: Build or rework a garnish status line config interactively (terminal, font, width, what matters, colours, frame, alignment), preview it, validate it and write it. Use when someone wants to set up, redesign or tweak their Claude Code status line with garnish.
---

# garnish-statusline

You are configuring [garnish](https://github.com/justanotherspy/garnish), the
Claude Code status line. The config is one TOML file; `garnish` renders it
every second from the session payload. Every option is documented in
`docs/config.md` and `garnish modules`; complete examples are in
`garnish presets`. Work only through the `garnish` CLI and the config file:
never edit `~/.claude/settings.json` yourself (`garnish install` owns it).

## 1. Check the tools

```sh
garnish --version && garnish doctor | head -12
```

If `garnish` is missing, stop and point at the install section of the README.
If `doctor` says the status line is not configured, offer `garnish install`
after the config is written.

## 2. Ask, with recommended defaults

Ask these in one or two rounds (AskUserQuestion when available); accept a
free-text "describe what you want" at any point and map it onto the keys.

| question | decides | default |
|---|---|---|
| Terminal and font: is a Nerd Font installed? | `icons = "nerd"`, else `"unicode"` (or `"emoji"`, `"ascii"`) | nerd if they are unsure but use Ghostty/Kitty/WezTerm/iTerm2 with a patched font |
| Usual terminal width (columns)? | preset and line count: < 90 → `minimal` or `compact`; 90–130 → `compact` or `default`; wider → `default` or `full` | `compact` |
| What matters most: repo state, model/context, usage limits, timers? | which modules go on which line; `preset = "full"` on the modules they care about | the preset's lines |
| Colour: a named theme or match the terminal? | `theme = garnish \| catppuccin-mocha \| nord \| dracula \| tokyonight \| mono`; `color = "256"` for terminals without truecolor | `garnish` |
| Frame taste: rounded, square, double, heavy, powerline, none? | `[frame] style` | `rounded` |
| Should columns line up across lines? | `align = true`, `durations = "fixed"`; `right_justify` | `align = true` when there are 2+ lines |
| Anything that scrolls or moves? | `overflow = "ticker"`, a `[modules.text.<name>]` box, `[frame] fill_pattern`, `separator_frames`, `<key>_frames`; `animate = false` for screen readers | nothing animated |

If a gallery preset matches the answers (`garnish presets`), start from it:
`garnish config init --preset <name> --force` writes the whole file; then
adjust keys. Otherwise start from a built-in preset and add `[[line]]` blocks.

## 3. Write, preview, validate

Write the file to `garnish config path` (use the Write tool, or
`garnish config init --preset <p> --force` followed by edits). Then:

```sh
garnish config check                                  # every problem with its TOML path
garnish preview "$PAYLOAD" --width "$COLUMNS" --color always
```

`garnish preview` needs a payload file. If the repository is at hand use
`tests/fixtures/payloads/subscription-full.json`; otherwise write this sample
to a temp file and preview with it:

```json
{"session_id":"sess-000","cwd":"/home/dev/projects/demo","workspace":{"current_dir":"/home/dev/projects/demo","project_dir":"/home/dev/projects/demo"},"version":"2.1.261","model":{"id":"claude-opus-5","display_name":"Opus"},"output_style":{"name":"default"},"cost":{"total_cost_usd":1.23,"total_duration_ms":4320000,"total_api_duration_ms":500000,"total_lines_added":156,"total_lines_removed":23},"context_window":{"context_window_size":1000000,"used_percentage":42,"remaining_percentage":58,"total_input_tokens":400000,"total_output_tokens":20000},"effort":{"level":"high"},"rate_limits":{"five_hour":{"used_percentage":24,"resets_at":1738433620},"seven_day":{"used_percentage":41,"resets_at":1738699200}},"prompt_cache":{"warm":true,"ttl":"1h","hit_ratio":0.91,"requests":22,"misses":2}}
```

Show the preview, ask whether it reads right, iterate. Explain each key you
set in one line so the person can tweak it later (point at `docs/config.md`
and the module pages). Finish with `garnish config check` reporting `ok`, and
remind them that a saved-payload preview shows frame 0 of any animation.

## 4. Hook it up (only if asked or not yet done)

```sh
garnish install            # merges statusLine into ~/.claude/settings.json, keeps a backup
```

Never write `settings.json` by hand.
