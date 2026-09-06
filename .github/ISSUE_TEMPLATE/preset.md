---
name: Preset submission
about: Share a config for the presets gallery (docs/presets.md). The garnish-submit-preset skill fills this in for you.
title: "preset: <name> — <summary>"
labels: preset
---

<!-- Tip: in Claude Code, `/garnish-submit-preset` prepares everything below
     from your current config. A maintainer turns an accepted issue into
     `presets/<name>.toml`. -->

## The preset

<!-- Your `garnish.toml` with the gallery header (SPEC § 12):
       # name: <kebab-case>
       # summary: <one line>
       # columns: <terminal width it was designed for>
       # needs: nerd-font          (optional)
       # author: <GitHub handle>   (optional)
     `garnish --config <file> config check` must pass. -->

```toml

```

## Sample

<!-- `garnish --config <file> preview <payload.json> --width <columns> --color never`
     No `…` and no row wider than columns − 4. Say so if the preset moves
     (ticker, patterns, frames). -->

Rendered at `columns = `:

```text

```

## Screenshot

<!-- Drag a real-terminal screenshot into the issue after creating it; it
     becomes `presets/screenshots/<name>.png`. -->

Screenshot to follow.
