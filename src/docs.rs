//! Documentation and config generation from the module schemas.
//!
//! The schemas are the single source of truth: `garnish docs` writes the
//! reference pages under `docs/` and `garnish config init|show` write TOML,
//! all from the same [`ModuleSchema`] values, so none of them can drift from
//! the code.

use std::fmt::Write as _;
use std::path::Path;

use crate::config::schema::{ModuleSchema, Preset, Value, toml_string};
use crate::config::{self, Config, Overlay};
use crate::frame::FrameStyle;
use crate::icons::IconSet;
use crate::modules::SCHEMAS;
use crate::payload::Payload;
use crate::render::{Clock, render_plain_at};
use crate::theme::{PALETTES, Role};

/// The top-level keys of the config file, in schema order.
fn write_top_level(out: &mut String, cfg: &Config, annotated: bool) {
    let c = |s: &mut String, text: &str| {
        if annotated {
            let _ = writeln!(s, "# {text}");
        }
    };
    c(out, "garnish configuration — see docs/config.md for every key.");
    c(out, "Top-level preset: default | minimal | full | compact");
    let _ = writeln!(out, "preset = {}", toml_string(cfg.preset.name()));
    c(out, "Icon set: nerd | unicode | emoji | ascii");
    let _ = writeln!(out, "icons = {}", toml_string(cfg.icons.name()));
    c(out, &format!("Theme: {}", PALETTES.iter().map(|p| p.name).collect::<Vec<_>>().join(" | ")));
    let _ = writeln!(out, "theme = {}", toml_string(&cfg.theme_name));
    c(out, "Color output: auto | always | never | 256 | truecolor");
    let _ = writeln!(out, "color = {}", toml_string(color_name(cfg.color)));
    c(out, "Truncate the left group when a line overflows the width.");
    let _ = writeln!(out, "truncate = {}", cfg.truncate);
    c(out, "Stale cached values: dim | hide | plain");
    let _ = writeln!(out, "stale_style = {}", toml_string(stale_name(cfg.stale_style)));
    c(out, "TTL periods a cached value may be overdue before it is styled stale (>= 1).");
    let _ = writeln!(out, "stale_after = {}", cfg.stale_after);
    c(out, "Extra cells subtracted from the width, on top of the 4 Claude Code's box");
    c(out, "always takes; set 2 x statusLine.padding when that setting is non-zero.");
    let _ = writeln!(out, "padding = {}", cfg.padding);
    c(out, "Pad each module column to the widest module in it across lines, so the");
    c(out, "separators line up vertically.");
    let _ = writeln!(out, "align = {}", cfg.align);
    c(out, "Elapsed times and countdowns: compact (8m20s, 9m, 2h) | fixed (8m20s, 9m00s, 2h00m)");
    let _ = writeln!(out, "durations = {}", toml_string(cfg.durations.name()));
    let _ = writeln!(out);
}

/// Render a config as TOML.
///
/// With `annotated`, every option carries its doc comment and colors are
/// written as theme roles so the file follows the theme (`config init`).
/// Without, every value is fully resolved: colors as literal specs, lines,
/// presets and options exactly as the tick will use them (`config show`).
#[must_use]
pub fn config_toml(cfg: &Config, annotated: bool) -> String {
    let mut out = String::new();
    let c = |s: &mut String, text: &str| {
        if annotated {
            let _ = writeln!(s, "# {text}");
        }
    };
    write_top_level(&mut out, cfg, annotated);

    c(&mut out, "Role color overrides; every module color defaults to one of these roles.");
    let _ = writeln!(out, "[colors]");
    if annotated {
        for role in Role::ALL {
            let _ = writeln!(
                out,
                "# {} = {}",
                role.name(),
                toml_string(&cfg.theme.role(role).to_spec())
            );
        }
    } else {
        for role in Role::ALL {
            let _ =
                writeln!(out, "{} = {}", role.name(), toml_string(&cfg.theme.role(role).to_spec()));
        }
    }
    let _ = writeln!(out);

    c(&mut out, "Frame style: none | rounded | square | double | heavy | powerline | custom");
    let _ = writeln!(out, "[frame]");
    let _ = writeln!(out, "style = {}", toml_string(cfg.frame.style.name()));
    c(&mut out, "Extend the rule to the full width and close with the right cap.");
    let _ = writeln!(out, "fill = {}", cfg.frame.fill);
    c(&mut out, "Default separator between modules on a line (style-dependent when unset).");
    if annotated && cfg.frame.style != FrameStyle::Custom {
        let _ = writeln!(out, "# separator = {}", toml_string(&cfg.frame.chars.separator));
    } else {
        let _ = writeln!(out, "separator = {}", toml_string(&cfg.frame.chars.separator));
    }
    if cfg.frame.style == FrameStyle::Custom || !annotated {
        let ch = &cfg.frame.chars;
        for (key, value) in [
            ("first", &ch.first),
            ("middle", &ch.middle),
            ("last", &ch.last),
            ("single", &ch.single),
            ("fill_char", &ch.fill),
            ("right_first", &ch.right_first),
            ("right_middle", &ch.right_middle),
            ("right_last", &ch.right_last),
            ("right_single", &ch.right_single),
            ("pad", &ch.pad),
        ] {
            let _ = writeln!(out, "{key} = {}", toml_string(value));
        }
    } else {
        c(
            &mut out,
            "For style = \"custom\": first middle last single fill_char right_first right_middle right_last right_single pad",
        );
    }
    let _ = writeln!(out);

    c(
        &mut out,
        "Lines: `modules` are left-aligned, `right` are right-aligned. Any module may go anywhere.",
    );
    for line in &cfg.lines {
        let _ = writeln!(out, "[[line]]");
        let _ = writeln!(out, "modules = {}", toml_list(&line.left));
        if !line.right.is_empty() {
            let _ = writeln!(out, "right = {}", toml_list(&line.right));
        }
        if let Some(sep) = &line.separator {
            let _ = writeln!(out, "separator = {}", toml_string(sep));
        }
    }
    let _ = writeln!(out);

    write_modules(&mut out, cfg, annotated);
    out
}

fn write_modules(out: &mut String, cfg: &Config, annotated: bool) {
    let c = |s: &mut String, text: &str| {
        if annotated {
            let _ = writeln!(s, "# {text}");
        }
    };
    for schema in SCHEMAS.iter() {
        let Some(m) = cfg.modules.get(schema.id) else { continue };
        c(out, &format!("{} — {}", schema.id, schema.summary));
        let _ = writeln!(out, "[modules.{}]", schema.id);
        c(
            out,
            "preset: minimal | default | full; refresh: seconds between refreshes (0 = every tick)",
        );
        let _ = writeln!(out, "enabled = {}", m.enabled);
        // An annotated file leaves the module preset and option values as
        // comments so the top-level `preset` keeps driving them after `init`.
        if annotated {
            let _ = writeln!(out, "# preset = {}", toml_string(m.preset.name()));
        } else {
            let _ = writeln!(out, "preset = {}", toml_string(m.preset.name()));
        }
        let _ = writeln!(out, "refresh = {}", m.refresh);
        c(out, "label: text before the value; prefix/suffix: text around the module");
        let _ = writeln!(out, "label = {}", toml_string(&m.label));
        let _ = writeln!(out, "prefix = {}", toml_string(&m.prefix));
        let _ = writeln!(out, "suffix = {}", toml_string(&m.suffix));
        c(
            out,
            "hide_when_empty: hide the module entirely when it has nothing to show (else a dim –)",
        );
        let _ = writeln!(out, "hide_when_empty = {}", m.hide_when_empty);
        for opt in &schema.opts {
            if annotated {
                let presets: Vec<String> = Preset::ALL
                    .iter()
                    .map(|p| format!("{}={}", p.name(), opt.for_preset(*p).to_toml()))
                    .collect();
                let _ = writeln!(
                    out,
                    "# {} ({}) — {} [{}]",
                    opt.key,
                    opt.kind.doc_name().replace("\\|", "|"),
                    opt.doc,
                    presets.join(" ")
                );
            }
            let value = m.value(opt.key).map_or_else(|| opt.default.to_toml(), Value::to_toml);
            let prefix = if annotated { "# " } else { "" };
            let _ = writeln!(out, "{prefix}{} = {}", opt.key, value);
        }
        if !schema.icons.is_empty() {
            let _ = writeln!(out, "[modules.{}.icons]", schema.id);
            for icon in &schema.icons {
                if annotated {
                    let _ = writeln!(out, "# {} — {}", icon.key, icon.doc);
                }
                let _ = writeln!(out, "{} = {}", icon.key, toml_string(m.icon(icon.key)));
            }
        }
        if !schema.colors.is_empty() {
            let _ = writeln!(out, "[modules.{}.colors]", schema.id);
            for color in &schema.colors {
                if annotated {
                    let _ = writeln!(out, "# {} — {}", color.key, color.doc);
                    let _ = writeln!(out, "{} = {}", color.key, toml_string(color.default));
                } else {
                    let _ = writeln!(
                        out,
                        "{} = {}",
                        color.key,
                        toml_string(&m.color(color.key).to_spec())
                    );
                }
            }
        }
        let _ = writeln!(out);
    }
}

fn toml_list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "))
}

const fn color_name(c: crate::config::ColorChoice) -> &'static str {
    match c {
        crate::config::ColorChoice::Auto => "auto",
        crate::config::ColorChoice::Always => "always",
        crate::config::ColorChoice::Never => "never",
        crate::config::ColorChoice::Ansi256 => "256",
        crate::config::ColorChoice::TrueColor => "truecolor",
    }
}

const fn stale_name(s: crate::config::StaleStyle) -> &'static str {
    match s {
        crate::config::StaleStyle::Dim => "dim",
        crate::config::StaleStyle::Hide => "hide",
        crate::config::StaleStyle::Plain => "plain",
    }
}

/// Payload fixtures embedded for the documentation renders.
const FIXTURES: [(&str, &str); 8] = [
    ("subscription-full", include_str!("../tests/fixtures/payloads/subscription-full.json")),
    ("output-style", include_str!("../tests/fixtures/payloads/output-style.json")),
    ("api-key", include_str!("../tests/fixtures/payloads/api-key.json")),
    ("worktree-session", include_str!("../tests/fixtures/payloads/worktree-session.json")),
    ("pr-approved", include_str!("../tests/fixtures/payloads/pr-approved.json")),
    ("spend-limit", include_str!("../tests/fixtures/payloads/spend-limit.json")),
    ("vim", include_str!("../tests/fixtures/payloads/vim.json")),
    ("agent", include_str!("../tests/fixtures/payloads/agent.json")),
];

/// The fixture that shows a module best.
fn sample_fixture(id: &str) -> &'static str {
    match id {
        "branch" | "worktree" => "worktree-session",
        "pr" => "pr-approved",
        "spend" => "spend-limit",
        "cost" => "api-key",
        "vim" => "vim",
        "agent" => "agent",
        "style" => "output-style",
        _ => "subscription-full",
    }
}

fn fixture(name: &str) -> Payload {
    FIXTURES
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, text)| Payload::parse(text).ok())
        .unwrap_or_default()
}

/// Render one module alone with a preset and icon set, as plain text.
fn module_sample(id: &str, preset: Preset, icons: IconSet) -> String {
    let text = format!(
        "icons = {}\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [{}]\n[modules.{id}]\npreset = {}\n",
        toml_string(icons.name()),
        toml_string(id),
        toml_string(preset.name())
    );
    let (cfg, _) = config::parse(&text, &SCHEMAS);
    let out = render_plain_at(&fixture(sample_fixture(id)), &cfg, Some(80), &Clock::fixed());
    let line = out.lines().next().unwrap_or("").trim_end().to_owned();
    if !line.is_empty() {
        return line;
    }
    match id {
        "sync" => "(shown inside a git repository with an upstream, e.g. `⇡2 ⇣1`)".to_owned(),
        _ => "(nothing to show for this payload)".to_owned(),
    }
}

/// Render a small two-line status line with a frame style.
fn frame_sample(style: FrameStyle) -> String {
    let text = format!(
        "icons = \"unicode\"\n[frame]\nstyle = {}\n[[line]]\nmodules = [\"model\", \"context\"]\nright = [\"clock\"]\n[[line]]\nmodules = [\"limit5h\", \"limit7d\"]\nright = [\"cache\"]\n",
        toml_string(style.name())
    );
    let (cfg, _) = config::parse(&text, &SCHEMAS);
    render_plain_at(&fixture("subscription-full"), &cfg, Some(72), &Clock::fixed())
}

/// Terminal width for a preset's sample: a round number at which nothing is
/// cut, kept small so the samples read on GitHub without a scroll bar
/// (`full` is the exception; its four lines need about 120 columns).
const fn preset_columns(preset: config::presets::TopPreset) -> usize {
    use config::presets::TopPreset;
    match preset {
        TopPreset::Default | TopPreset::Minimal => 80,
        TopPreset::Compact => 90,
        TopPreset::Full => 120,
    }
}

/// Render a whole top-level preset at [`preset_columns`].
fn preset_sample(preset: config::presets::TopPreset, icons: IconSet) -> String {
    let (cfg, _) = config::parse_with(
        "",
        &SCHEMAS,
        &Overlay { preset: Some(preset), icons: Some(icons), ..Default::default() },
    );
    render_plain_at(
        &fixture("subscription-full"),
        &cfg,
        Some(preset_columns(preset)),
        &Clock::fixed(),
    )
}

/// The `docs/modules/<id>.md` page for one module.
#[must_use]
pub fn module_page(id: &str) -> Option<String> {
    let schema = SCHEMAS.iter().find(|s| s.id == id)?;
    let mut o = String::new();
    let _ = writeln!(o, "# `{}`\n\n{}\n\n{}\n", schema.id, schema.summary, schema.doc);
    let _ = writeln!(
        o,
        "**Sources:** {}\n",
        schema.sources.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ")
    );
    let refresh = if schema.refresh == 0 {
        "every tick (payload only)".to_owned()
    } else {
        format!("cached, refreshed in the background every {} s", schema.refresh)
    };
    let _ = writeln!(o, "**Refresh:** {refresh}\n");

    let _ = writeln!(o, "## Presets\n\n| preset | render |\n|---|---|");
    for p in Preset::ALL {
        let _ = writeln!(
            o,
            "| `{}` | `{}` |",
            p.name(),
            module_sample(id, p, IconSet::Unicode).replace('|', "\\|")
        );
    }
    let _ = writeln!(o, "\n## Icon sets (default preset)\n\n| icons | render |\n|---|---|");
    for set in IconSet::ALL {
        let _ = writeln!(
            o,
            "| `{}` | `{}` |",
            set.name(),
            module_sample(id, Preset::Default, set).replace('|', "\\|")
        );
    }

    module_reference(&mut o, schema);
    Some(o)
}

/// The option, icon and color tables of a module page.
fn module_reference(o: &mut String, schema: &ModuleSchema) {
    let _ = writeln!(o, "\n## Options\n\n`[modules.{}]`\n", schema.id);
    let _ = writeln!(
        o,
        "| key | type | minimal | default | full | description |\n|---|---|---|---|---|---|"
    );
    let _ = writeln!(o, "| `enabled` | bool | `true` | `true` | `true` | Render this module. |");
    let _ = writeln!(
        o,
        "| `preset` | `minimal` \\| `default` \\| `full` | — | — | — | Which preset the options below default to. |"
    );
    let _ = writeln!(
        o,
        "| `refresh` | integer | `{r}` | `{r}` | `{r}` | Seconds between background refreshes; 0 = every tick. |",
        r = schema.refresh
    );
    let _ =
        writeln!(o, "| `label` | string | `\"\"` | `\"\"` | `\"\"` | Dim text before the value. |");
    let _ = writeln!(
        o,
        "| `prefix` / `suffix` | string | `\"\"` | `\"\"` | `\"\"` | Text around the module. |"
    );
    let _ = writeln!(
        o,
        "| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |"
    );
    for opt in &schema.opts {
        let _ = writeln!(
            o,
            "| `{}` | {} | `{}` | `{}` | `{}` | {} |",
            opt.key,
            opt.kind.doc_name(),
            opt.for_preset(Preset::Minimal).to_toml(),
            opt.for_preset(Preset::Default).to_toml(),
            opt.for_preset(Preset::Full).to_toml(),
            opt.doc
        );
    }
    if !schema.icons.is_empty() {
        let _ = writeln!(o, "\n## Icons\n\n`[modules.{}.icons]`\n", schema.id);
        let _ = writeln!(
            o,
            "| key | nerd | unicode | emoji | ascii | description |\n|---|---|---|---|---|---|"
        );
        for icon in &schema.icons {
            let g = icon.glyph;
            let _ = writeln!(
                o,
                "| `{}` | `{}` | `{}` | `{}` | `{}` | {} |",
                icon.key,
                code_points(g.nerd),
                g.unicode,
                g.emoji,
                g.ascii,
                icon.doc
            );
        }
    }
    if !schema.colors.is_empty() {
        let _ = writeln!(
            o,
            "\n## Colors\n\n`[modules.{}.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).\n",
            schema.id
        );
        let _ = writeln!(o, "| key | default | description |\n|---|---|---|");
        for color in &schema.colors {
            let _ = writeln!(o, "| `{}` | `{}` | {} |", color.key, color.default, color.doc);
        }
    }
}

/// Nerd Font glyphs as `U+XXXX` so the page is readable without the font.
fn code_points(s: &str) -> String {
    if s.chars().any(|c| matches!(u32::from(c), 0xE000..=0xF8FF | 0xF_0000..=0x10_FFFD)) {
        s.chars().map(|c| format!("U+{:04X}", u32::from(c))).collect::<Vec<_>>().join(" ")
    } else {
        s.to_owned()
    }
}

/// The `docs/config.md` page.
#[must_use]
pub fn config_page() -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# Configuration reference\n");
    let _ = writeln!(
        o,
        "garnish reads `--config`, else `$GARNISH_CONFIG`, else `$XDG_CONFIG_HOME/garnish/garnish.toml` (`~/.config/garnish/garnish.toml`), else `~/.garnish.toml`. Without a file the built-in `default` preset is used. `garnish config init` writes an annotated file; `garnish config check` validates it; `garnish config show` prints the fully resolved result.\n"
    );
    let _ = writeln!(
        o,
        "An invalid file never blanks the status line: garnish renders the defaults and appends a dim `⚠ config: <file>:<line> <message>` line.\n"
    );

    let _ =
        writeln!(o, "## Top-level keys\n\n| key | values | default | meaning |\n|---|---|---|---|");
    let _ = writeln!(
        o,
        "| `preset` | `default` \\| `minimal` \\| `full` \\| `compact` | `default` | Which lines exist and which module preset they imply, when `[[line]]` is absent. |"
    );
    let _ = writeln!(
        o,
        "| `icons` | `nerd` \\| `unicode` \\| `emoji` \\| `ascii` | `nerd` | Glyph set. `nerd` needs a Nerd Font. |"
    );
    let _ = writeln!(
        o,
        "| `theme` | {} | `garnish` | Color palette (see below). |",
        PALETTES.iter().map(|p| format!("`{}`", p.name)).collect::<Vec<_>>().join(" \\| ")
    );
    let _ = writeln!(
        o,
        "| `color` | `auto` \\| `always` \\| `never` \\| `256` \\| `truecolor` | `auto` | Escape-code output. `auto` is truecolor unless `NO_COLOR` is set. |"
    );
    let _ = writeln!(
        o,
        "| `truncate` | bool | `true` | Truncate the left group when a line overflows the width (`$COLUMNS − 4 − padding`); the right group is never cut. |"
    );
    let _ = writeln!(
        o,
        "| `stale_style` | `dim` \\| `hide` \\| `plain` | `dim` | How overdue cached values are shown. |"
    );
    let _ = writeln!(
        o,
        "| `stale_after` | integer ≥ 1 | `5` | TTL periods a cached value may be overdue before it is styled stale; until then the last value shows unchanged while a worker refreshes it. |"
    );
    let _ = writeln!(
        o,
        "| `padding` | integer | `0` | Extra cells subtracted from the width, on top of the 4 Claude Code's box always takes; set `2 × statusLine.padding` when that setting is non-zero. |"
    );
    let _ = writeln!(
        o,
        "| `align` | bool | `false` | Pad each module column to the widest module in it across lines, so the separators stack vertically (see [Aligned columns](#aligned-columns)). |"
    );
    let _ = writeln!(
        o,
        "| `durations` | `compact` \\| `fixed` | `compact` | How elapsed times and countdowns print: `compact` drops a zero second unit (`8m20s`, `9m`, `2h`); `fixed` always shows two units with the small one two digits wide (`8m20s`, `9m00s`, `2h00m`), so timers keep their width. |"
    );

    let _ = writeln!(
        o,
        "\n## `[colors]` — theme roles\n\nEvery module color defaults to a role; override a role here to restyle every module at once.\n"
    );
    let _ = writeln!(o, "| role | garnish default | used for |\n|---|---|---|");
    let garnish = PALETTES.first();
    for role in Role::ALL {
        let def = garnish.map_or("", |p| p.spec(role));
        let _ = writeln!(o, "| `{}` | `{def}` | {} |", role.name(), role_doc(role));
    }
    let _ = writeln!(o, "\n### Themes\n\n| theme | description |\n|---|---|");
    for p in &PALETTES {
        let _ = writeln!(o, "| `{}` | {} |", p.name, p.doc);
    }

    frame_section(&mut o);
    presets_section(&mut o);
    environment_section(&mut o);
    o
}

fn frame_section(o: &mut String) {
    let _ = writeln!(o, "\n## `[frame]`\n\n| key | default | meaning |\n|---|---|---|");
    let _ = writeln!(
        o,
        "| `style` | `rounded` (`none` for the `minimal` preset) | `none` \\| `rounded` \\| `square` \\| `double` \\| `heavy` \\| `powerline` \\| `custom` |"
    );
    let _ = writeln!(
        o,
        "| `fill` | `true` | Extend the rule between the left and right groups to the full width and close with the right cap. With `false`, lines are left-packed. |"
    );
    let _ = writeln!(o, "| `separator` | style-dependent | Default separator between modules. |");
    let _ = writeln!(
        o,
        "| `first` `middle` `last` `single` | style-dependent | Line prefixes (`single` when there is one line). |"
    );
    let _ = writeln!(
        o,
        "| `right_first` `right_middle` `right_last` `right_single` | style-dependent | Right caps. |"
    );
    let _ = writeln!(
        o,
        "| `fill_char` | style-dependent | The rule character (must be one cell wide). |"
    );
    let _ =
        writeln!(o, "| `pad` | style-dependent | Text between prefix/content and content/rule. |");
    let _ = writeln!(o, "\n### Frame styles\n");
    for style in FrameStyle::ALL {
        if style == FrameStyle::Custom {
            continue;
        }
        let _ = writeln!(o, "`{}`\n\n```text\n{}\n```\n", style.name(), frame_sample(style));
    }
    let _ = writeln!(
        o,
        "### Aligned columns\n\nWith `align = true` every module column is padded to the widest module in it, so the separators fall on the same cell in every line (only between lines that share a `separator`). `durations = \"fixed\"` keeps timers from changing width as they tick. The same three lines, `align = false` then `align = true`:\n\n```text\n{}\n```\n\n```text\n{}\n```\n",
        align_sample(false),
        align_sample(true)
    );
}

/// Three lines whose first modules differ in width, with and without `align`.
fn align_sample(align: bool) -> String {
    let text = format!(
        "icons = \"unicode\"\nalign = {align}\ndurations = \"fixed\"\n[[line]]\nmodules = [\"model\", \"context\"]\nright = [\"clock\"]\n[[line]]\nmodules = [\"limit5h\", \"limit7d\"]\nright = [\"lines\"]\n[[line]]\nmodules = [\"session\", \"api\", \"cache\"]\nright = [\"cost\"]\n"
    );
    let (cfg, _) = config::parse(&text, &SCHEMAS);
    render_plain_at(&fixture("subscription-full"), &cfg, Some(80), &Clock::fixed())
}

fn presets_section(o: &mut String) {
    let _ = writeln!(
        o,
        "## `[[line]]`\n\nEach entry is one output row. `modules` are left-aligned, `right` are right-aligned, `separator` overrides the frame separator for that line. Any module id may appear on any line, in any order; a module that has nothing to show is skipped.\n"
    );
    let _ = writeln!(
        o,
        "```toml\n[[line]]\nmodules = [\"path\", \"branch\", \"sync\", \"pr\"]\nright   = [\"clock\"]\nseparator = \"  \"\n```\n"
    );

    let _ = writeln!(o, "## Top-level presets\n");
    for preset in config::presets::TopPreset::ALL {
        let lines: Vec<String> = preset
            .lines()
            .iter()
            .map(|l| {
                let right = if l.right.is_empty() {
                    String::new()
                } else {
                    format!(" ⟶ {}", l.right.join(" "))
                };
                format!("`{}`{}", l.left.join(" "), right)
            })
            .collect();
        let _ = writeln!(
            o,
            "### `{}`\n\nModule preset `{}`. Lines:\n\n{}\n\nAt {} columns, unicode icons:\n\n```text\n{}\n```\n",
            preset.name(),
            preset.module_preset().name(),
            lines.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"),
            preset_columns(preset),
            preset_sample(preset, IconSet::Unicode)
        );
    }

    let _ = writeln!(
        o,
        "## `[modules.<id>]`\n\nEvery module accepts `enabled`, `preset`, `refresh`, `label`, `prefix`, `suffix`, `hide_when_empty`, an `icons` table and a `colors` table, plus its own options. Resolution order: built-in default → icon set → module preset → top-level preset → explicit key. See the per-module pages in [modules/](modules/).\n"
    );
}

fn environment_section(o: &mut String) {
    let _ = writeln!(o, "## Environment\n\n| variable | effect |\n|---|---|");
    let _ = writeln!(
        o,
        "| `COLUMNS` | Terminal width (set by Claude Code). `GARNISH_COLUMNS` is the fallback; 120 when neither is set. The lines are rendered 4 cells narrower, plus `padding`: the width of Claude Code's status line box. |"
    );
    let _ = writeln!(o, "| `NO_COLOR` | Disables escape codes under `color = \"auto\"`. |");
    let _ = writeln!(o, "| `GARNISH_CONFIG` | Config file path. |");
    let _ = writeln!(
        o,
        "| `GARNISH_CACHE_DIR` | Cache root (default `$XDG_RUNTIME_DIR/garnish`, `$XDG_CACHE_HOME/garnish`, `~/.cache/garnish`). |"
    );
    let _ = writeln!(
        o,
        "| `GARNISH_NOW` | Freeze the clock (epoch seconds or RFC 3339) for reproducible renders. |"
    );
    let _ = writeln!(
        o,
        "| `GARNISH_NO_SPAWN` | Log intended background refreshes to `<cache>/spawns.log` instead of spawning them (tests). |"
    );
    let _ = writeln!(
        o,
        "| `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, `DISABLE_AUTO_COMPACT` | Read to place the `context` compaction marker exactly where Claude Code will compact. |"
    );
}

const fn role_doc(role: Role) -> &'static str {
    match role {
        Role::Accent => "primary highlight: icons and names",
        Role::Accent2 => "secondary highlight",
        Role::Muted => "de-emphasised text, separators, stale values",
        Role::Text => "ordinary text",
        Role::Ok => "good / low usage",
        Role::Warn => "caution / medium usage",
        Role::Hot => "high usage",
        Role::Danger => "critical, errors, exceeded limits",
        Role::Frame => "frame lines and rules",
        Role::Band1 => "bar band 1 (lowest)",
        Role::Band2 => "bar band 2",
        Role::Band3 => "bar band 3",
        Role::Band4 => "bar band 4 (highest)",
    }
}

/// The `docs/README.md` index.
#[must_use]
pub fn index_page() -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "# garnish reference\n\nGenerated by `garnish docs` from the module schemas; do not edit by hand. Start with the [guide](guide.md), then the [configuration reference](config.md).\n"
    );
    let _ = writeln!(o, "## Modules\n\n| module | shows | refresh |\n|---|---|---|");
    for s in SCHEMAS.iter() {
        let refresh = if s.refresh == 0 { "tick".to_owned() } else { format!("{} s", s.refresh) };
        let _ = writeln!(o, "| [`{id}`](modules/{id}.md) | {} | {refresh} |", s.summary, id = s.id);
    }
    let _ = writeln!(
        o,
        "\n## Default preset, unicode icons\n\n```text\n{}\n```",
        preset_sample(config::presets::TopPreset::Default, IconSet::Unicode)
    );
    o
}

/// Write every generated page under `out`. Returns the number of files written.
///
/// # Errors
/// Propagates I/O errors.
pub fn generate(out: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(out.join("modules"))?;
    let mut n: usize = 0;
    std::fs::write(out.join("README.md"), index_page())?;
    n = n.saturating_add(1);
    std::fs::write(out.join("config.md"), config_page())?;
    n = n.saturating_add(1);
    for s in SCHEMAS.iter() {
        if let Some(page) = module_page(s.id) {
            std::fs::write(out.join("modules").join(format!("{}.md", s.id)), page)?;
            n = n.saturating_add(1);
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_config_round_trips_and_keeps_theme_roles() {
        let (cfg, errs) = config::parse("", &SCHEMAS);
        assert_eq!(errs.len(), 0);
        let text = config_toml(&cfg, true);
        assert!(
            text.contains("icon = \"accent\""),
            "roles, not literal colors, in an annotated file"
        );
        let (again, errs) = config::parse(&text, &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}\n{text}");
        assert_eq!(again, cfg);
        // the top-level preset still drives modules and the frame after `init`
        let switched = text.replacen("preset = \"default\"", "preset = \"full\"", 1);
        let (full, errs) = config::parse(&switched, &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(full.modules.get("context").unwrap().int("width"), 30);
        let powerline = text.replacen("style = \"rounded\"", "style = \"powerline\"", 1);
        let (pl, _) = config::parse(&powerline, &SCHEMAS);
        assert_eq!(pl.frame.chars.separator, " \u{e0b1} ");
    }

    #[test]
    fn resolved_config_round_trips_exactly_including_overrides() {
        let source = "theme = \"nord\"\n[colors]\naccent = \"#010203\"\n[modules.model]\nlabel = \"M\"\nprefix = \"<\"\nhide_when_empty = false\nshow_id = true\n[modules.model.colors]\nname = \"danger\"\n[frame]\nstyle = \"custom\"\nfirst = \">>\"\n";
        let (cfg, errs) = config::parse(source, &SCHEMAS);
        assert_eq!(errs.len(), 0, "{errs:?}");
        let text = config_toml(&cfg, false);
        assert!(text.contains("accent = \"#010203\""), "{text}");
        assert!(text.contains("label = \"M\""), "{text}");
        assert!(text.contains("first = \">>\""), "{text}");
        let (again, errs) = config::parse(&text, &SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}\n{text}");
        assert_eq!(again, cfg);
        let model = again.modules.get("model").unwrap();
        assert_eq!(model.color("name"), cfg.theme.role(Role::Danger));
        assert!(!model.hide_when_empty);
    }

    #[test]
    fn every_module_has_a_page_with_samples_for_every_preset_and_icon_set() {
        for s in SCHEMAS.iter() {
            let page = module_page(s.id).unwrap();
            assert!(page.starts_with(&format!("# `{}`", s.id)));
            for p in Preset::ALL {
                assert!(page.contains(&format!("| `{}` |", p.name())), "{}: {}", s.id, p.name());
            }
            for set in IconSet::ALL {
                assert!(
                    page.contains(&format!("| `{}` |", set.name())),
                    "{}: {}",
                    s.id,
                    set.name()
                );
            }
            for opt in &s.opts {
                assert!(page.contains(&format!("| `{}` |", opt.key)), "{}: {}", s.id, opt.key);
            }
        }
        assert!(module_page("nope").is_none());
    }

    #[test]
    fn samples_are_deterministic_and_non_empty_for_showcase_modules() {
        for id in ["model", "context", "clock", "path", "branch", "pr", "cost", "vim", "agent"] {
            let a = module_sample(id, Preset::Default, IconSet::Unicode);
            assert_eq!(a, module_sample(id, Preset::Default, IconSet::Unicode));
            assert!(!a.starts_with("(nothing"), "{id}: {a}");
        }
        assert!(module_sample("clock", Preset::Default, IconSet::Unicode).contains("16:00:00"));
        assert!(
            module_sample("path", Preset::Default, IconSet::Unicode).contains("~/projects/garnish")
        );
        assert!(config_page().contains("### Frame styles"));
        assert!(index_page().contains("[`context`](modules/context.md)"));
    }

    #[test]
    fn generate_writes_all_pages() {
        let dir = tempfile::tempdir().unwrap();
        let n = generate(dir.path()).unwrap();
        assert_eq!(n, 2 + SCHEMAS.len());
        assert!(dir.path().join("modules").join("clock.md").exists());
    }
}
