---
name: garnish-statusline
description: "Set up, redesign or tweak a Claude Code status line with garnish: pick a preset or compose rows, columns, titles and boxes, preview at the real width, validate and write the config, and hook it into Claude Code. Use when someone wants to configure their Claude Code status line."
---

# garnish-statusline

garnish renders the Claude Code status line from one TOML file
(`garnish config path`). There are two ways to build that file:

- **`garnish setup`** is the hands-on one: a full-screen preset picker and
  layout builder with a live preview at the real box width, every option
  editable in place. Offer it first when the person is at a terminal.
  `garnish setup --preset <name> [--install]` writes a preset without the
  screen.
- **This skill** is the conversational one: ask, draft, preview, write. Use
  it when they would rather describe what they want.

Work only through the `garnish` CLI and the config file; never edit
`~/.claude/settings.json` yourself (`garnish install` owns it).

## 1. Check

```sh
garnish --version && garnish doctor | head -20
```

No `garnish`: stop and point at the README's install section. A doctor row
saying `statusLine` is `not configured`: offer `garnish install` at the end.
A settings file that does not parse: say so and stop, since `install`
refuses to rewrite it.

## 2. Ask

One or two rounds (AskUserQuestion when available). A free-text "describe
it" answer at any point is mapped onto the same keys.

| question | decides | default |
|---|---|---|
| Nerd Font installed? | `icons = "nerd"`, else `"unicode"` (`"emoji"`, `"ascii"`) | `nerd` |
| Usual terminal width? (`echo $COLUMNS` in their own terminal) | preset and row count: under 90 → `compact` or `minimal`; 90–130 → `compact` or `default`; wider → `default` or `full` | `compact` |
| What matters: repo, model and context, usage limits, timers? | which modules go on which row; `preset = "full"` on the ones they care about | the preset's rows |
| Rows across the width, or panels side by side? | one `[[row]]` per line, or `[[row.col]]` columns with `width` (`"1fr"` \| `"auto"` \| cells), `justify`, `gap`; `[[row.col.row]]` stacks rows in a column | rows |
| Anything to label or frame? | `title` (with `title_justify`) in a row's rule; `[box.<name>]` around adjacent rows or a whole column; `box = true` on one row | nothing |
| Colours: a theme, or match the terminal? | `theme = garnish \| catppuccin-mocha \| nord \| dracula \| tokyonight \| mono`; `color = "256"` without truecolor | `garnish` |
| Frame? | `[frame] style = rounded \| square \| double \| heavy \| powerline \| none \| custom` | `rounded` |
| Columns lined up across rows? | `align = true`, `durations = "fixed"` | `align = true` from two rows |
| Anything that moves? | `overflow = "ticker"`; a scrolling `[modules.text.<name>]`; `fill_pattern`, `separator_frames`, `<key>_frames`; `animate = false` for screen readers | nothing |
| Long branch names or session titles? | `max_width` on that module; `path.style = "fish"` | nothing cut |
| Clickable (a terminal with OSC 8)? | `branch.link = true`; `url` on a text module | nothing |
| Context bar: the window, or how close compaction is? | `context.scale = "usable"` | `window` |
| A window's reset as a countdown or a clock time? | `reset = "countdown" \| "absolute" \| "both"` on `limit5h`, `limit7d`, `spend` | `countdown` |

Start from the closest of the 28 gallery presets (`garnish presets`;
`titled-sections`, `sidebar-panels`, `grid-three`, `boxed-panels`,
`links-and-shortcuts`, `compaction-watch`, `narrow-unicode` and
`ascii-only` between them show every layout feature) rather than writing
rows from nothing. `docs/config.md` and `garnish modules` list every key.

## 3. Draft, preview, write

Draft into a temp file, never over the real one:

```sh
DRAFT=$(mktemp -t garnish.XXXXXX)
garnish --config "$DRAFT" config init --preset <name> --force
# edit $DRAFT with the Edit tool
garnish --config "$DRAFT" config check                                   # every problem with its TOML path
garnish --config "$DRAFT" preview "$PAYLOAD" --width <columns> --color always
```

`$PAYLOAD` is a saved payload: `tests/fixtures/payloads/subscription-full.json`
in the repository, or the sample at the end of this file written to a temp
file. Show the preview (its rows are faint on purpose: Claude Code draws
every status line row dim), iterate, and write only on approval, keeping
the previous file as the `.bak-<epoch>` backup garnish itself keeps:

```sh
TARGET=$(garnish config path)
mkdir -p "$(dirname "$TARGET")"
[ -f "$TARGET" ] && cp "$TARGET" "$TARGET.bak-$(date +%s)"
cp "$DRAFT" "$TARGET" && garnish config check
```

Say where the backup went and explain each key you set in one line.
`preview` runs on the live clock, so animations move between runs
(`GARNISH_ANIMATE=0` freezes them).

## 4. Hook it up

```sh
garnish install      # merges statusLine into ~/.claude/settings.json, keeps a backup
```

## Sample payload

The `subscription-full` fixture, for `garnish preview` without the repository:

```json
{"cwd":"/home/dev/projects/garnish","session_id":"sess-0001-aaaa-bbbb","session_name":"garnish-dev","prompt_id":"550e8400-e29b-41d4-a716-446655440000","transcript_path":"/home/dev/.claude/projects/-home-dev-projects-garnish/sess.jsonl","version":"2.1.260","model":{"id":"claude-opus-5","display_name":"Opus"},"workspace":{"current_dir":"/home/dev/projects/garnish","project_dir":"/home/dev/projects/garnish","added_dirs":[],"repo":{"host":"github.com","owner":"dschwartz","name":"garnish"}},"output_style":{"name":"default"},"cost":{"total_cost_usd":1.2345,"total_duration_ms":4320000,"total_api_duration_ms":500000,"total_lines_added":156,"total_lines_removed":23},"context_window":{"total_input_tokens":420000,"total_output_tokens":1200,"context_window_size":1000000,"used_percentage":42,"remaining_percentage":58,"current_usage":{"input_tokens":8500,"output_tokens":1200,"cache_creation_input_tokens":5000,"cache_read_input_tokens":406500}},"exceeds_200k_tokens":true,"prompt_cache":{"warm":true,"caching_observed":true,"ttl":"1h","expires_at":1738428420,"requests":14,"misses":2,"expected_rebuilds":1,"hit_ratio":0.91,"cache_write_tokens":352000,"miss_recache_tokens":310200,"last_miss_at":1738425230,"recache_tokens_if_cold":45000},"fast_mode":false,"effort":{"level":"high"},"thinking":{"enabled":true},"rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":1738433620},"seven_day":{"used_percentage":41.2,"resets_at":1738699200}},"pr":{"number":42,"url":"https://github.com/dschwartz/garnish/pull/42","review_state":"pending"}}
```
