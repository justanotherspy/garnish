---
name: garnish-feedback
description: "File a garnish bug report or feedback as a GitHub issue on justanotherspy/garnish with the environment, config, doctor output and rendered line attached. Use when a garnish status line looks wrong (misaligned, cut, wrong glyph, stale), or when someone wants to report or request something about garnish."
---

# garnish-feedback

You are filing an issue for [garnish](https://github.com/justanotherspy/garnish)
with `gh`. The maintainer needs to reproduce the row exactly, so collect
the facts verbatim; never paraphrase a render.

## 1. Collect

```sh
garnish --version
garnish doctor                          # toolchain, settings, config, cache, glyph grid
garnish config show                     # the fully resolved config
garnish config path
echo "$TERM_PROGRAM $TERM"
garnish preview "$PAYLOAD" --color never   # a saved payload, or the sample in garnish-statusline
```

Ask for what the commands cannot tell: the terminal application and
version, the font (a Nerd Font?), the OS, the terminal width (`echo
$COLUMNS` in their own terminal), and one sentence on what looks wrong
against what they expected. For a width or alignment problem (a wandering
right edge, an unexpected `…`, a glyph drawn wide) the doctor's **glyph
grid** is the evidence: keep it whole.

## 2. Write the issue

Title: one line naming the symptom (`unicode set: right edge wanders on the
usage line in COSMIC`). Body, in this order:

1. **What I see / what I expected**, in their words.
2. **Environment**: terminal and version, font, OS, `garnish --version`,
   width.
3. **Rendered line** in a `text` block (from `--color never`).
4. **Config** (`config show`) in a `toml` block.
5. **Doctor** output in a `text` block, glyph grid included.
6. **Screenshot**: ask them to attach one after the issue exists; write
   "screenshot to follow".

Labels: `feedback`, plus `alignment` for widths, glyphs or the right edge.

## 3. Redact, show, ask, post

The issue is public. Replace the home directory in every path with `~`
(`doctor` already does; `config show` prints no path), keep only the
`GARNISH_*` lines of the doctor's environment section, print the **whole
body**, and ask (AskUserQuestion when available): "post this to
justanotherspy/garnish as a public issue?". Only a yes runs:

```sh
gh issue create --repo justanotherspy/garnish --title "$TITLE" --body-file "$BODY" \
  --label feedback [--label alignment]
```

A label that does not exist: create the issue without it and say so. Show
the URL and remind them about the screenshot. Change nothing in their
config while filing.
