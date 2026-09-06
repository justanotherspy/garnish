---
name: garnish-statusline
description: "Build or rework a garnish status line config interactively (terminal, font, width, what matters, colours, frame, alignment), preview it, validate it and write it. Use when someone wants to set up, redesign or tweak their Claude Code status line with garnish."
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
If the doctor's `statusLine` line is anything but `command=…` (not
configured, settings missing or not valid JSON), offer `garnish install`
after the config is written.

## 2. Ask, with recommended defaults

Ask these in one or two rounds (AskUserQuestion when available); accept a
free-text "describe what you want" at any point and map it onto the keys.

| question | decides | default |
|---|---|---|
| Terminal and font: is a Nerd Font installed? | `icons = "nerd"`, else `"unicode"` (or `"emoji"`, `"ascii"`) | nerd if they are unsure but use Ghostty/Kitty/WezTerm/iTerm2 with a patched font |
| Usual terminal width (columns; `echo $COLUMNS` in their own terminal, a shell without a tty does not know)? | preset and line count: < 90 → `minimal` or `compact`; 90–130 → `compact` or `default`; wider → `default` or `full` | `compact` |
| What matters most: repo state, model/context, usage limits, timers? | which modules go on which line; `preset = "full"` on the modules they care about | the preset's lines |
| Colour: a named theme or match the terminal? | `theme = garnish \| catppuccin-mocha \| nord \| dracula \| tokyonight \| mono`; `color = "256"` for terminals without truecolor | `garnish` |
| Frame taste: rounded, square, double, heavy, powerline, none? | `[frame] style` | `rounded` |
| Should columns line up across lines? | `align = true`, `durations = "fixed"`; `right_justify` | `align = true` when there are 2+ lines |
| Anything that scrolls or moves? | `overflow = "ticker"`, a `[modules.text.<name>]` box, `[frame] fill_pattern`, `separator_frames`, `<key>_frames`; `animate = false` for screen readers | nothing animated |

If a gallery preset matches the answers (`garnish presets`), start from it;
otherwise start from a built-in preset and add `[[line]]` blocks.

## 3. Draft, preview, validate, then write

Never overwrite the config before the person has seen the result: `config
init --force` keeps no backup, and a hand-tuned file is easy to lose. Draft
into a temp file first (`--config` selects it for any subcommand):

```sh
DRAFT=$(mktemp -t garnish.XXXXXX)
garnish --config "$DRAFT" config init --preset <name> --force   # gallery or built-in preset as the base
# … edit $DRAFT with the Edit tool …
garnish --config "$DRAFT" config check                        # every problem with its TOML path
garnish --config "$DRAFT" preview "$PAYLOAD" --width <columns> --color always
```

Use the width they stated for `--width` (without it `preview` uses `COLUMNS`,
then 120; the lines come out 4 cells narrower, the width of Claude Code's
box). `garnish preview` needs a payload file. If the repository is at hand
use `tests/fixtures/payloads/subscription-full.json`; otherwise write this
sample (the same payload) to a temp file:

```json
{"cwd":"/home/dev/projects/garnish","session_id":"sess-0001-aaaa-bbbb","session_name":"garnish-dev","prompt_id":"550e8400-e29b-41d4-a716-446655440000","transcript_path":"/home/dev/.claude/projects/-home-dev-projects-garnish/sess.jsonl","version":"2.1.260","model":{"id":"claude-opus-5","display_name":"Opus"},"workspace":{"current_dir":"/home/dev/projects/garnish","project_dir":"/home/dev/projects/garnish","added_dirs":[],"repo":{"host":"github.com","owner":"dschwartz","name":"garnish"}},"output_style":{"name":"default"},"cost":{"total_cost_usd":1.2345,"total_duration_ms":4320000,"total_api_duration_ms":500000,"total_lines_added":156,"total_lines_removed":23},"context_window":{"total_input_tokens":420000,"total_output_tokens":1200,"context_window_size":1000000,"used_percentage":42,"remaining_percentage":58,"current_usage":{"input_tokens":8500,"output_tokens":1200,"cache_creation_input_tokens":5000,"cache_read_input_tokens":406500}},"exceeds_200k_tokens":true,"prompt_cache":{"warm":true,"caching_observed":true,"ttl":"1h","expires_at":1738428420,"requests":14,"misses":2,"expected_rebuilds":1,"hit_ratio":0.91,"cache_write_tokens":352000,"miss_recache_tokens":310200,"last_miss_at":1738425230,"recache_tokens_if_cold":45000},"fast_mode":false,"effort":{"level":"high"},"thinking":{"enabled":true},"rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":1738433620},"seven_day":{"used_percentage":41.2,"resets_at":1738699200}},"pr":{"number":42,"url":"https://github.com/dschwartz/garnish/pull/42","review_state":"pending"}}
```

Show the preview, ask whether it reads right, iterate on the draft. When
they approve, copy it into place (`cp "$DRAFT" "$(garnish config path)"`,
creating the directory if needed) and say that the previous file, if any,
was replaced. Explain each key you set in one line so the person can tweak
it later (point at `docs/config.md` and the module pages). Finish with
`garnish config check` reporting `ok`, and remind them that `preview` runs
with the live clock, so animations move between runs; `GARNISH_ANIMATE=0`
freezes them (and cuts a ticker line with `…`) for a still picture.

## 4. Hook it up (only if asked or not yet done)

```sh
garnish install            # merges statusLine into ~/.claude/settings.json, keeps a backup
```

Never write `settings.json` by hand.
