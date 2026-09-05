---
name: garnish-feedback
description: File a garnish bug report or feedback as a GitHub issue on justanotherspy/garnish with the environment, config, doctor output and rendered line attached. Use when a garnish status line looks wrong (misaligned, cut, wrong glyph, stale), or when someone wants to report or request something about garnish.
---

# garnish-feedback

You are filing an issue for [garnish](https://github.com/justanotherspy/garnish)
with `gh`. The maintainer needs to reproduce the row exactly, so collect the
facts below verbatim; do not paraphrase renders.

## 1. Collect

```sh
garnish --version
garnish doctor                                  # toolchain, settings, config, cache, glyph grid
garnish config show                             # the fully resolved config
garnish config path
echo "$TERM_PROGRAM $TERM"; tput cols            # terminal application and width
```

Ask the person for what the commands cannot tell: the terminal application
and version, the font (and whether it is a Nerd Font), the OS, and one
sentence on what looks wrong versus what they expected. If the complaint is
about widths or alignment (a wandering right edge, a `…` that should not be
there, a glyph drawn wide), the doctor's **glyph grid** is the key evidence:
keep it whole.

Render the line as plain text so it can be pasted:

```sh
garnish preview "$PAYLOAD" --color never          # a saved payload, or the sample from garnish-statusline
```

## 2. Write the issue

Title: one line naming the symptom (`unicode set: right edge wanders on the
usage line in COSMIC`). Body (Markdown, in this order):

1. **What I see / what I expected** (their words).
2. **Environment**: terminal + version, font, OS, `garnish --version`,
   terminal width.
3. **Rendered line** in a `text` code block (from `--color never`).
4. **Config** (`garnish config show`) in a `toml` code block.
5. **Doctor** output in a `text` code block, glyph grid included.
6. **Screenshot**: ask them to take one and attach it to the issue after it
   is created (drag it into the issue on GitHub); note "screenshot to
   follow" in the body.

Labels: `feedback`, plus `alignment` when the report is about widths,
glyphs or the right edge.

```sh
gh issue create --repo justanotherspy/garnish --title "$TITLE" --body-file "$BODY" \
  --label feedback [--label alignment]
```

If a label does not exist yet, `gh` reports it: create the issue without it
and say so. Show the issue URL and remind them about the screenshot. Do not
change their config while filing.
