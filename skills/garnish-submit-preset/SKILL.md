---
name: garnish-submit-preset
description: "Turn the current garnish config into a gallery preset proposal (name, summary, designed width, font requirement, author, a validated file with the gallery header, a rendered sample) and open a GitHub issue labelled preset on justanotherspy/garnish. Use when someone wants to share or contribute their garnish status line layout."
---

# garnish-submit-preset

The garnish presets gallery (`presets/<name>.toml`, shown in `docs/presets.md`
and `garnish presets`) is fed by issues labelled `preset`; a maintainer turns
an accepted one into a pull request. You are preparing such an issue from the
config in use.

## 1. Read the config

```sh
garnish config path
garnish config check                             # must say ok; fix problems first
garnish config show                              # the resolved form; the file itself is what gets submitted
```

Submit the file as written (comments and all), not the `show` output, unless
the person prefers the resolved form.

## 2. Ask

- **Name**: kebab-case, letters, digits and dashes, unique among
  `garnish presets` (check).
- **Summary**: one line, what the layout is for.
- **Columns**: the terminal width it was designed for; the sample is rendered
  there and must fit uncut. Ask for it (`echo $COLUMNS` in their own
  terminal; a shell without a tty reports 80 or nothing).
- **Needs**: `nerd-font`, `emoji`, or nothing (does it use Nerd Font glyphs?
  `icons = "nerd"` means `nerd-font`).
- **Author**: their GitHub handle, if they want credit.

## 3. Build the file and the sample

The file is the header followed by the config:

```toml
# name: <name>
# summary: <summary>
# columns: <N>
# needs: nerd-font
# author: <handle>

<the config, verbatim>
```

`needs` and `author` are optional: leave the line out rather than writing a
placeholder, since everything after `# needs: ` is taken literally. Write it
to a temp path, then validate and render exactly as the gallery test does:

```sh
garnish --config "$FILE" config check
garnish --config "$FILE" preview "$PAYLOAD" --width "$N" --color never
```

(`$PAYLOAD` is a saved payload; with the repository at hand use
`tests/fixtures/payloads/subscription-full.json`, else the sample payload in
the `garnish-statusline` skill.) The render must show no `…` and no row
wider than `N − 4` cells; if it does, raise `N` or trim the layout and ask.
An animated preset (`overflow = "ticker"`, `fill_pattern`, `*_frames`) is
fine; say in the summary that it moves.

## 4. Open the issue

Title: `preset: <name> — <summary>`. Body: the file in a `toml` code block,
the sample in a `text` code block with the width stated, the requirement, and
a request for a real-terminal screenshot (to be attached after creation; it
becomes `presets/screenshots/<name>.png`).

The issue is public. Replace the home directory in any path with `~`, print
the **whole body** so the person can read it, and ask (AskUserQuestion when
available): "post this to justanotherspy/garnish as a public issue?". Only a
yes runs:

```sh
gh issue create --repo justanotherspy/garnish --title "$TITLE" --body-file "$BODY" --label preset
```

If the `preset` label does not exist, create the issue without it and say so.
Show the URL. Nothing in the person's own config is changed.
