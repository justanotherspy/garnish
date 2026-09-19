//! Documentation and config generation from the module schemas.
//!
//! The schemas are the single source of truth: `garnish docs` writes the
//! reference pages under `docs/` and `garnish config init|show` write TOML,
//! all from the same [`ModuleSchema`] values, so none of them can drift from
//! the code.

use std::fmt::Write as _;
use std::path::Path;

use crate::config::schema::{
    COMMON_OPTS, Kind, ModuleCfg, ModuleSchema, OptSpec, Preset, Value, toml_string,
};
use crate::config::{self, ColCfg, Config, Overlay};
use crate::frame::FrameStyle;
use crate::icons::IconSet;
use crate::modules::SCHEMAS;
use crate::payload::Payload;
use crate::render::{Clock, render_plain_at};
use crate::theme::{PALETTES, Role};

/// One `#` comment line, written only for an annotated file (`config init`
/// writes them; `config show` writes the values alone).
fn comment(out: &mut String, annotated: bool, text: &str) {
    if annotated {
        let _ = writeln!(out, "# {text}");
    }
}

/// The top-level keys of the config file, in schema order.
fn write_top_level(out: &mut String, cfg: &Config, annotated: bool) {
    comment(out, annotated, "garnish configuration — see docs/config.md for every key.");
    comment(out, annotated, "Top-level preset: default | minimal | full | compact");
    let _ = writeln!(out, "preset = {}", toml_string(cfg.preset.name()));
    comment(out, annotated, "Icon set: nerd | unicode | emoji | ascii");
    let _ = writeln!(out, "icons = {}", toml_string(cfg.icons.name()));
    comment(
        out,
        annotated,
        &format!("Theme: {}", PALETTES.iter().map(|p| p.name).collect::<Vec<_>>().join(" | ")),
    );
    let _ = writeln!(out, "theme = {}", toml_string(&cfg.theme_name));
    comment(out, annotated, "Color output: auto | always | never | 256 | truecolor");
    let _ = writeln!(out, "color = {}", toml_string(cfg.color.name()));
    comment(out, annotated, "Truncate the left group when a line overflows the width.");
    let _ = writeln!(out, "truncate = {}", cfg.truncate);
    comment(out, annotated, "Stale cached values: dim | hide | plain");
    let _ = writeln!(out, "stale_style = {}", toml_string(cfg.stale_style.name()));
    comment(
        out,
        annotated,
        "TTL periods a cached value may be overdue before it is styled stale (>= 1).",
    );
    let _ = writeln!(out, "stale_after = {}", cfg.stale_after);
    comment(
        out,
        annotated,
        "Extra cells subtracted from the width, on top of the 4 Claude Code's box",
    );
    comment(
        out,
        annotated,
        "always takes; set 2 x statusLine.padding when that setting is non-zero.",
    );
    let _ = writeln!(out, "padding = {}", cfg.padding);
    comment(
        out,
        annotated,
        "Pad each module column to the widest module in it across lines, so the",
    );
    comment(out, annotated, "separators line up vertically.");
    let _ = writeln!(out, "align = {}", cfg.align);
    comment(
        out,
        annotated,
        "Where a padded right-group module's text sits: end (hugs the cap) | start (follows the separator)",
    );
    let _ = writeln!(out, "right_justify = {}", toml_string(cfg.right_justify.name()));
    comment(
        out,
        annotated,
        "Drop a row whose modules all rendered nothing (a `modules = []` spacer is kept).",
    );
    let _ = writeln!(out, "hide_empty_rows = {}", cfg.hide_empty_rows);
    comment(
        out,
        annotated,
        "A left group wider than its budget: truncate (cut with …) | ticker (scroll it)",
    );
    let _ = writeln!(out, "overflow = {}", toml_string(cfg.overflow.name()));
    comment(
        out,
        annotated,
        "Ticker: cells scrolled per tick (0.5 = every second tick) and the text between end and start",
    );
    let _ = writeln!(
        out,
        "ticker_step = {}",
        crate::config::schema::Value::Float(cfg.ticker_step).to_toml()
    );
    let _ = writeln!(out, "ticker_gap = {}", toml_string(&cfg.ticker_gap));
    comment(
        out,
        annotated,
        "Master switch for every animation (spinner, scrolling text, rule pattern, separator and icon frames); false freezes them at frame 0 and cuts a ticker line with …",
    );
    comment(
        out,
        annotated,
        "Unset, animations follow Claude Code's prefersReducedMotion setting; GARNISH_ANIMATE=0 freezes a session either way",
    );
    // Left as a comment in an annotated file so the settings rule keeps
    // working after `config init`; `show` prints the value in effect.
    let prefix = if annotated { "# " } else { "" };
    let _ = writeln!(out, "{prefix}animate = {}", cfg.animate.unwrap_or(true));
    comment(
        out,
        annotated,
        "Elapsed times and countdowns: compact (8m20s, 9m, 2h) | fixed (8m20s, 9m00s, 2h00m); unset, it is fixed under overflow = \"ticker\" and compact otherwise, and each timer module can pin its own",
    );
    // Left as a comment in an annotated file so the ticker rule keeps
    // working after `config init`; `show` prints the value in effect.
    let prefix = if annotated { "# " } else { "" };
    let _ = writeln!(out, "{prefix}durations = {}", toml_string(cfg.durations.name()));
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
    write_top_level(&mut out, cfg, annotated);
    write_colors(&mut out, cfg, annotated);
    write_frame(&mut out, cfg, annotated);
    write_rows(&mut out, cfg, annotated);
    write_modules(&mut out, cfg, annotated);
    write_texts(&mut out, cfg, annotated);
    out
}

/// `[colors]`: the role overrides, commented out in an annotated file so the
/// theme keeps deciding them.
fn write_colors(out: &mut String, cfg: &Config, annotated: bool) {
    comment(
        out,
        annotated,
        "Role color overrides; every module color defaults to one of these roles.",
    );
    let _ = writeln!(out, "[colors]");
    let prefix = if annotated { "# " } else { "" };
    for role in Role::ALL {
        let _ = writeln!(
            out,
            "{prefix}{} = {}",
            role.name(),
            toml_string(&cfg.theme.role(role).to_spec())
        );
    }
    let _ = writeln!(out);
}

/// `[frame]`: the style, the custom glyphs when there are any, and the
/// animation keys of SPEC § 4.2.
fn write_frame(out: &mut String, cfg: &Config, annotated: bool) {
    comment(
        out,
        annotated,
        "Frame style: none | rounded | square | double | heavy | powerline | custom",
    );
    let _ = writeln!(out, "[frame]");
    let _ = writeln!(out, "style = {}", toml_string(cfg.frame.style.name()));
    comment(out, annotated, "Extend the rule to the full width and close with the right cap.");
    let _ = writeln!(out, "fill = {}", cfg.frame.fill);
    comment(
        out,
        annotated,
        "Default separator between modules on a line (style-dependent when unset).",
    );
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
        // The box glyphs (SPEC § 4.3) only exist for a style with a box
        // shape; an empty one is `none`'s invisible box, and writing it
        // back would make `config show` disagree with itself.
        for (key, value) in [
            ("top_left", &ch.top_left),
            ("top_right", &ch.top_right),
            ("bottom_left", &ch.bottom_left),
            ("bottom_right", &ch.bottom_right),
            ("side", &ch.side),
        ] {
            if !value.is_empty() {
                let _ = writeln!(out, "{key} = {}", toml_string(value));
            }
        }
    } else {
        comment(
            out,
            annotated,
            "For style = \"custom\": first middle last single fill_char right_first right_middle right_last right_single pad, and the box glyphs top_left top_right bottom_left bottom_right side",
        );
    }
    comment(
        out,
        annotated,
        "Animation (see docs/config.md): a one-cell-glyph pattern travelling along the rule,",
    );
    comment(
        out,
        annotated,
        "and separator frames cycled one per tick (all the same width). Empty = static.",
    );
    let _ = writeln!(out, "fill_pattern = {}", toml_string(&cfg.frame.fill_pattern.concat()));
    let _ = writeln!(out, "fill_step = {}", Value::Float(cfg.frame.fill_step).to_toml());
    let _ = writeln!(out, "fill_direction = {}", toml_string(cfg.frame.fill_direction.name()));
    let _ = writeln!(out, "separator_frames = {}", toml_list(&cfg.frame.separator_frames));
    let _ = writeln!(out, "separator_step = {}", Value::Float(cfg.frame.separator_step).to_toml());
    let _ = writeln!(out);
}

/// `[[row]]` per configured row, with `[[row.col]]` and `[[row.col.row]]`
/// only where the config has them (SPEC § 4.3), so a plain row round-trips
/// as the plain form it was written in.
fn write_rows(out: &mut String, cfg: &Config, annotated: bool) {
    comment(
        out,
        annotated,
        "Rows: `modules` are left-aligned, `right` are right-aligned. Any module may go anywhere.",
    );
    comment(
        out,
        annotated,
        "A row can hold columns side by side instead; see docs/config.md § [[row.col]]:",
    );
    comment(out, annotated, "[[row]]");
    comment(out, annotated, "gap = 2");
    comment(out, annotated, "[[row.col]]");
    comment(out, annotated, "width = \"1fr\"        # \"<n>fr\" | \"auto\" | a cell count");
    comment(out, annotated, "modules = [\"path\", \"branch\"]");
    for row in &cfg.rows {
        // A row left with no ids by a reported mistake renders as an empty
        // row that `hide_empty_rows` drops; written back as `modules = []`
        // it would become a spacer that is always drawn, so it is left out.
        if row.cols.iter().all(ColCfg::is_empty) && !row.spacer {
            continue;
        }
        let _ = writeln!(out, "[[row]]");
        if row.explicit_cols && row.gap != config::DEFAULT_GAP {
            let _ = writeln!(out, "gap = {}", row.gap);
        }
        if let Some(sep) = &row.separator {
            let _ = writeln!(out, "separator = {}", toml_string(sep));
        }
        write_title(out, row.title.as_ref());
        write_box_ref(out, row.boxed.as_ref());
        if row.blank {
            let _ = writeln!(out, "blank = true");
        }
        match row.single().filter(|_| !row.explicit_cols) {
            // The plain form: one column, written as the row's own groups.
            Some(col) => write_groups(out, col),
            _ => {
                for col in &row.cols {
                    let _ = writeln!(out, "[[row.col]]");
                    if col.width != config::Width::default() {
                        let _ = writeln!(out, "width = {}", col.width.to_toml());
                    }
                    if col.justify_set {
                        let _ = writeln!(out, "justify = {}", toml_string(col.justify.name()));
                    }
                    if col.valign != config::VAlign::default() {
                        let _ = writeln!(out, "valign = {}", toml_string(col.valign.name()));
                    }
                    write_box_ref(out, col.boxed.as_ref());
                    if col.rows.is_empty() {
                        write_groups(out, col);
                    }
                    for inner in &col.rows {
                        let _ = writeln!(out, "[[row.col.row]]");
                        if let Some(sep) = &inner.separator {
                            let _ = writeln!(out, "separator = {}", toml_string(sep));
                        }
                        write_title(out, inner.title.as_ref());
                        write_box_ref(out, inner.boxed.as_ref());
                        if inner.blank {
                            let _ = writeln!(out, "blank = true");
                        }
                        if let Some(col) = inner.single() {
                            write_groups(out, col);
                        }
                    }
                }
            }
        }
    }
    let _ = writeln!(out);
    write_boxes(out, cfg, annotated);
}

/// The `modules` / `right` pair of one column.
fn write_groups(out: &mut String, col: &ColCfg) {
    let _ = writeln!(out, "modules = {}", toml_list(&col.left));
    if !col.right.is_empty() {
        let _ = writeln!(out, "right = {}", toml_list(&col.right));
    }
}

/// The four `title*` keys of a row or a box, each only when it is set.
fn write_title(out: &mut String, title: Option<&config::TitleCfg>) {
    let Some(title) = title else { return };
    let _ = writeln!(out, "title = {}", toml_string(&title.text));
    if title.justify != config::Justify::default() {
        let _ = writeln!(out, "title_justify = {}", toml_string(title.justify.name()));
    }
    if title.pad != config::DEFAULT_TITLE_PAD {
        let _ = writeln!(out, "title_pad = {}", title.pad);
    }
    if let Some(color) = title.color {
        let _ = writeln!(out, "title_color = {}", toml_string(&color.to_spec()));
    }
}

/// `box = "<name>"` or `box = true`.
fn write_box_ref(out: &mut String, boxed: Option<&config::BoxRef>) {
    match boxed {
        Some(config::BoxRef::Named(name)) => {
            let _ = writeln!(out, "box = {}", toml_string(name));
        }
        Some(config::BoxRef::Anon) => {
            let _ = writeln!(out, "box = true");
        }
        None => {}
    }
}

/// The `[box.<name>]` tables (SPEC § 4.3), or, in an annotated file without
/// any, one commented example so boxes are discoverable from `config init`.
fn write_boxes(out: &mut String, cfg: &Config, annotated: bool) {
    if cfg.boxes.is_empty() {
        if annotated {
            let _ = writeln!(out, "# A box frames a run of adjacent rows, or a whole column:");
            let _ = writeln!(out, "# [box.repo]");
            let _ = writeln!(out, "# title = \"Repository\"");
            let _ = writeln!(out, "# title_justify = \"left\"  # left | center | right");
            let _ =
                writeln!(out, "# style = \"double\"        # inherits [frame] style when absent");
            let _ = writeln!(out, "# fill = false            # draw the rule inside the box too");
            let _ = writeln!(
                out,
                "# color = \"accent\"        # role or literal; frame colour when absent"
            );
            let _ = writeln!(out);
        }
        return;
    }
    for (name, b) in &cfg.boxes {
        let _ = writeln!(out, "[box.{name}]");
        write_title(out, b.title.as_ref());
        if let Some(style) = b.style {
            let _ = writeln!(out, "style = {}", toml_string(style.name()));
        }
        let _ = writeln!(out, "fill = {}", b.fill);
        if let Some(color) = b.color {
            let _ = writeln!(out, "color = {}", toml_string(&color.to_spec()));
        }
    }
    let _ = writeln!(out);
}

/// The `[modules.text.<name>]` tables (SPEC § 3.7): every defined text module
/// with its resolved values, or, in an annotated file without any, one
/// commented example so the family is discoverable from `config init`.
fn write_texts(out: &mut String, cfg: &Config, annotated: bool) {
    let schema = &*crate::modules::text::SCHEMA;
    if cfg.texts.is_empty() {
        if annotated {
            let _ = writeln!(out, "# text.<name> — {}", schema.summary);
            let _ = writeln!(out, "# Place it on a line as \"text.<name>\"; any number may exist.");
            let _ = writeln!(out, "# [modules.text.motd]");
            for opt in &schema.opts {
                let _ = writeln!(out, "# {} = {}  # {}", opt.key, opt.default.to_toml(), opt.doc);
            }
            let _ = writeln!(out, "# [modules.text.motd.colors]");
            for color in &schema.colors {
                let _ = writeln!(out, "# {} = {}", color.key, toml_string(color.default));
            }
            let _ = writeln!(out);
        }
        return;
    }
    for (name, m) in &cfg.texts {
        if annotated {
            let _ = writeln!(out, "# text.{name} — {}", schema.summary);
        }
        let _ = writeln!(out, "[modules.text.{name}]");
        let _ = writeln!(out, "enabled = {}", m.enabled);
        write_common(out, m, annotated, text_common_opts());
        for opt in &schema.opts {
            if annotated {
                let _ = writeln!(out, "# {} — {}", opt.key, opt.doc);
            }
            let value = m.value(opt.key).map_or_else(|| opt.default.to_toml(), Value::to_toml);
            let _ = writeln!(out, "{} = {}", opt.key, value);
        }
        let _ = writeln!(out, "[modules.text.{name}.colors]");
        for color in &schema.colors {
            let spec =
                if annotated { color.default.to_owned() } else { m.color(color.key).to_spec() };
            let _ = writeln!(out, "{} = {}", color.key, toml_string(&spec));
        }
        let _ = writeln!(out);
    }
}

/// The common options a text module takes: every one but `max_width`, which
/// `config check` rejects there in favour of `width` (SPEC § 3.7).
fn text_common_opts() -> &'static [OptSpec] {
    static OPTS: std::sync::LazyLock<Vec<OptSpec>> = std::sync::LazyLock::new(|| {
        COMMON_OPTS.iter().filter(|o| o.key != "max_width").cloned().collect()
    });
    &OPTS
}

/// The `label`, `prefix`, `suffix`, `hide_when_empty` (and, for a built-in
/// module, `max_width`) lines of a module table, from the same specs the
/// parser bounds them with.
fn write_common(out: &mut String, m: &ModuleCfg, annotated: bool, opts: &[OptSpec]) {
    for opt in opts {
        if annotated {
            let _ = writeln!(
                out,
                "# {} ({}) — {}",
                opt.key,
                kind_column(opt).replace("\\|", "|"),
                opt.doc
            );
        }
        let value = m.common(opt.key).unwrap_or_else(|| opt.default.clone());
        let _ = writeln!(out, "{} = {}", opt.key, value.to_toml());
    }
}

fn write_modules(out: &mut String, cfg: &Config, annotated: bool) {
    for schema in SCHEMAS.iter() {
        let Some(m) = cfg.modules.get(schema.id) else { continue };
        comment(out, annotated, &format!("{} — {}", schema.id, schema.summary));
        let _ = writeln!(out, "[modules.{}]", schema.id);
        comment(
            out,
            annotated,
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
        write_common(out, m, annotated, &COMMON_OPTS);
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
                    kind_column(opt).replace("\\|", "|"),
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
            if annotated {
                let _ = writeln!(
                    out,
                    "# Any key also accepts <key>_frames = [...]: equal-width glyphs cycled one per tick."
                );
            }
            for icon in &schema.icons {
                if annotated {
                    let _ = writeln!(out, "# {} — {}", icon.key, icon.doc);
                }
                let _ = writeln!(out, "{} = {}", icon.key, toml_string(m.icon(icon.key)));
                if let Some(frames) = m.all_icon_frames().get(icon.key) {
                    let _ = writeln!(out, "{}_frames = {}", icon.key, toml_list(frames));
                }
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
    crate::fixtures::payload(name)
}

/// Render one module alone with a preset and icon set, as plain text.
fn module_sample(id: &str, preset: Preset, icons: IconSet) -> String {
    let text = format!(
        "icons = {}\n[frame]\nstyle = \"none\"\nfill = false\n[[row]]\nmodules = [{}]\n[modules.{id}]\npreset = {}\n",
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
        "icons = \"unicode\"\n[frame]\nstyle = {}\n[[row]]\nmodules = [\"model\", \"context\"]\nright = [\"clock\"]\n[[row]]\nmodules = [\"limit5h\", \"limit7d\"]\nright = [\"cache\"]\n",
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
pub fn module_page(schema: &ModuleSchema) -> String {
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
            module_sample(schema.id, p, IconSet::Unicode).replace('|', "\\|")
        );
    }
    let _ = writeln!(o, "\n## Icon sets (default preset)\n\n| icons | render |\n|---|---|");
    for set in IconSet::ALL {
        let _ = writeln!(
            o,
            "| `{}` | `{}` |",
            set.name(),
            module_sample(schema.id, Preset::Default, set).replace('|', "\\|")
        );
    }

    module_reference(&mut o, schema);
    o
}

/// The config the text-module page renders: the SPEC § 3.7 example.
const TEXT_SAMPLE: &str = "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[row]]\nmodules = [\"text.motd\", \"text.clip\", \"text.tag\"]\n[modules.text.motd]\ntext = \"ship it before lunch, then write the docs\"\nwidth = 12\noverflow = \"scroll-wrap\"\ngap = \" · \"\n[modules.text.clip]\ntext = \"a rather long note\"\nwidth = 8\noverflow = \"clip\"\n[modules.text.tag]\ntext = \"v0.2\"\nwidth = 8\njustify = \"right\"\npad = 1\ncolor = \"muted\"\n";

/// The `docs/modules/text.md` page for the `text.<name>` family (SPEC § 3.7).
#[must_use]
pub fn text_page() -> String {
    let schema = &*crate::modules::text::SCHEMA;
    let mut o = String::new();
    let _ = writeln!(o, "# `text.<name>`\n\n{}\n\n{}\n", schema.summary, schema.doc);
    let _ = writeln!(
        o,
        "**Sources:** the config file only. **Refresh:** every tick; nothing to cache.\n"
    );
    let (cfg, _) = config::parse(TEXT_SAMPLE, &SCHEMAS);
    let sample = render_plain_at(&fixture("subscription-full"), &cfg, Some(80), &Clock::fixed());
    let _ = writeln!(
        o,
        "## Example\n\n```toml\n{}```\n\nrenders (frame 0; the first box scrolls in a live session) as\n\n```text\n{}\n```\n",
        TEXT_SAMPLE
            .trim_start_matches("icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n"),
        sample.trim_end()
    );
    let _ = writeln!(o, "## Options\n\n`[modules.text.<name>]`\n");
    let _ = writeln!(o, "| key | type | default | description |\n|---|---|---|---|");
    let _ = writeln!(o, "| `enabled` | bool | `true` | Render this module. |");
    for opt in text_common_opts() {
        let _ = writeln!(
            o,
            "| `{}` | {} | `{}` | {} |",
            opt.key,
            kind_column(opt),
            opt.default.to_toml(),
            opt.doc.replace('|', "\\|")
        );
    }
    for opt in &schema.opts {
        let _ = writeln!(
            o,
            "| `{}` | {} | `{}` | {} |",
            opt.key,
            kind_column(opt),
            opt.default.to_toml().replace('|', "\\|"),
            opt.doc.replace('|', "\\|")
        );
    }
    let _ = writeln!(
        o,
        "\nNo `preset`, no `refresh` and no `max_width` (the box is sized by `width`): a text module renders every tick as configured.\n"
    );
    let _ = writeln!(
        o,
        "## Colors\n\n`[modules.text.<name>.colors]`, or the shorthand `color = …` on the module (an explicit `colors.text` wins over the shorthand). A module name is letters, digits, `_` and `-` only, so `text.<name>` reads the same on a line and in `config show`.\n"
    );
    let _ = writeln!(o, "| key | default | description |\n|---|---|---|");
    for color in &schema.colors {
        let _ = writeln!(o, "| `{}` | `{}` | {} |", color.key, color.default, color.doc);
    }
    o
}

/// The type column of an option row: the kind, and the schema's cap when
/// the option has one (`integer ≤ 1024`, `string ≤ 4096 chars`).
fn kind_column(opt: &OptSpec) -> String {
    let kind = opt.kind.doc_name();
    match (opt.max, opt.kind) {
        (Some(max), Kind::Str) => format!("{kind} ≤ {max} chars"),
        (Some(max), _) => format!("{kind} ≤ {max}"),
        (None, _) => kind,
    }
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
    for opt in &COMMON_OPTS {
        let value = opt.default.to_toml();
        let _ = writeln!(
            o,
            "| `{}` | {} | `{value}` | `{value}` | `{value}` | {} |",
            opt.key,
            kind_column(opt),
            opt.doc.replace('|', "\\|")
        );
    }
    for opt in &schema.opts {
        let _ = writeln!(
            o,
            "| `{}` | {} | `{}` | `{}` | `{}` | {} |",
            opt.key,
            kind_column(opt),
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
        // The alternatives the `setup` glyph picker offers (SPEC § 14), so
        // a hand-written override can start from the same list.
        let also: Vec<String> = schema
            .icons
            .iter()
            .filter_map(|icon| {
                let alternatives = crate::icons::suggestions(schema.id, icon.key);
                (!alternatives.is_empty()).then(|| {
                    let glyphs: Vec<String> =
                        alternatives.iter().map(|g| format!("`{}`", code_points(g))).collect();
                    format!("`{}`: {}", icon.key, glyphs.join(" "))
                })
            })
            .collect();
        if !also.is_empty() {
            let _ = writeln!(o, "\nAlso try ({}).", also.join("; "));
        }
        let _ = writeln!(
            o,
            "\nAny icon key also accepts `<key>_frames = [\"…\", \"…\"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).\n"
        );
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
        "A bad key never blanks the status line: every valid key stays in effect, the built-in default stands in for the bad one, and a dim `⚠ config: <file> <path>: <message>` line is appended; only a file that does not parse as TOML falls back to the defaults wholesale, with the line of the syntax error.\n"
    );

    let _ =
        writeln!(o, "## Top-level keys\n\n| key | values | default | meaning |\n|---|---|---|---|");
    let _ = writeln!(
        o,
        "| `preset` | `default` \\| `minimal` \\| `full` \\| `compact` | `default` | Which rows exist and which module preset they imply, when `[[row]]` is absent. |"
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
        "| `align` | bool | `false` | Pad each module column to the widest module in it across lines, so the separators stack vertically (see [Aligned columns](#aligned-columns)). |\n| `right_justify` | `end` \\| `start` | `end` | Where a padded right-group module's text sits: `end` pads on the left so the text hugs the cap, `start` pads on the right so the text follows the separator. Only matters with `align = true` and a filled rule. |\n| `hide_empty_rows` | bool | `true` | Drop a row whose modules all rendered nothing (outside a repository, a row of `branch sync pr` is empty); the frame's caps follow the surviving rows. A row configured as `modules = []` with no `right` is an intentional spacer and is always kept. With `stale_style = \"hide\"` a row of only cached modules can disappear while its values are overdue and return after the refresh; `hide_when_empty = false` on one module pins the row. `hide_empty_lines` is the permanent alias of this key. |\n| `overflow` | `truncate` \\| `ticker` | `truncate` | A left group wider than its budget is cut with `…` (`truncate`) or scrolled (`ticker`): a window onto the group advances `ticker_step` cells per tick and wraps around with `ticker_gap` between the end and the start. The offset comes from the tick's clock, so it needs no state and `GARNISH_NOW` freezes it; it moves as often as Claude Code ticks (`refreshInterval`, at least 1 s). The right group is never scrolled or cut. With animations off the line is cut with `…` like `truncate`. |\n| `ticker_step` | number | `1` | Cells the ticker advances per tick ({steps}; `0.5` = every second tick). |\n| `ticker_gap` | string | `\"   \"` | Text between the end of a scrolled group and its wrapped-around start. |\n| `animate` | bool | `true` | Master switch for every animation (the clock spinner, scrolling text modules, the ticker, and the animated frame parts of § 4.2): `false` freezes them all at frame 0 and cuts a ticker line with `…`. Unset, garnish follows Claude Code's `prefersReducedMotion` setting (the settings chain of the project directory and the home, the first file that sets it winning), so the two stay in step; an explicit value wins over the setting, and `GARNISH_ANIMATE=0` freezes one session whatever either says. `config show` prints the value in effect. Recommended off for screen readers and recordings. |",
        steps = crate::config::STEP_BOUNDS
    );
    let _ = writeln!(
        o,
        "| `durations` | `compact` \\| `fixed` | `compact` (`fixed` with a ticker) | How elapsed times and countdowns print: `compact` drops a zero second unit (`8m20s`, `9m`, `2h`); `fixed` always shows two units with the small one two digits wide (`8m20s`, `9m00s`, `2h00m`), so timers keep their width. Defaults to `fixed` when `overflow = \"ticker\"`, because a timer changing width inside the scrolled group makes the window jump; set it to `compact` to opt back in. Every module that prints a timer (`session`, `api`, `cache`, `limit5h`, `limit7d`, `spend`, `sync`) has its own `durations` (`inherit` \\| `compact` \\| `fixed`) to pin one module. |"
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
    let _ = writeln!(
        o,
        "| `fill_pattern` | `\"\"` | One-cell glyphs repeated across the rule instead of `fill_char`; each tick the pattern shifts `fill_step` cells in `fill_direction`, so dots appear to travel along the rule. The rule's width never changes, only which glyph lands in each cell. Empty keeps the static rule. |"
    );
    let _ = writeln!(
        o,
        "| `fill_step` | `1` | Cells the pattern shifts per tick ({}; 0.5 = every second tick). |",
        crate::config::STEP_BOUNDS
    );
    let _ = writeln!(
        o,
        "| `fill_direction` | `right` | `left` \\| `right`: which way the pattern travels. |"
    );
    let _ = writeln!(
        o,
        "| `separator_frames` | `[]` | Separator strings cycled one per tick; every frame must have the same width (validation rejects a mismatch so columns cannot jitter). A per-line `separator` wins over the frames. Empty keeps the static `separator`. |"
    );
    let _ = writeln!(
        o,
        "| `separator_step` | `1` | Frames the separator advances per tick ({}). |",
        crate::config::STEP_BOUNDS
    );
    let _ = writeln!(
        o,
        "\nAnimations follow the clock rule of [Animation](guide.md#animation): frame = `floor(now × step) mod period`, so `animate = false` or `GARNISH_ANIMATE=0` freezes them at frame 0, which is also what these generated samples show.\n"
    );
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
        "icons = \"unicode\"\nalign = {align}\ndurations = \"fixed\"\n[[row]]\nmodules = [\"model\", \"context\"]\nright = [\"clock\"]\n[[row]]\nmodules = [\"limit5h\", \"limit7d\"]\nright = [\"lines\"]\n[[row]]\nmodules = [\"session\", \"api\", \"cache\"]\nright = [\"cost\"]\n"
    );
    let (cfg, _) = config::parse(&text, &SCHEMAS);
    render_plain_at(&fixture("subscription-full"), &cfg, Some(80), &Clock::fixed())
}

/// One sample config rendered at a width, for the layout sections.
fn layout_sample(text: &str, columns: usize) -> String {
    let (cfg, _) = config::parse(text, &SCHEMAS);
    render_plain_at(&fixture("subscription-full"), &cfg, Some(columns), &Clock::fixed())
}

/// `[[row.col]]`: widths, `justify`, and stacks (SPEC § 4.3).
fn columns_section(o: &mut String) {
    let _ = writeln!(
        o,
        "## `[[row.col]]`\n\nA row is columns side by side; a row written with `modules`/`right` and no `[[row.col]]` is one column filling the width, which is what every config above is. Columns share the row's width by `width`:\n\n| value | meaning |\n|---|---|\n| `\"<n>fr\"` | a share of the width left over once the others are placed (`\"1fr\"` by default, so three bare columns are thirds and six are sixths) |\n| `\"auto\"` | exactly the column's content, re-measured every tick — for values that hold still (a clock under `durations = \"fixed\"`, a module with `max_width`), not for branch names |\n| an integer | that many cells |\n\n`gap` is the empty cells between columns (1 by default; on a one-line row the rule runs through them, so a centred module floats on one continuous rule). `justify` (`left` \\| `center` \\| `right`) places a column's `modules` when it has no `right` group; its default follows the column's position, so a three-column row reads left / centre / right without saying so. A column with both `modules` and `right` is the flex form of a plain row, laid out to the column's width. Content wider than its column is cut with `…` (or scrolled under `overflow = \"ticker\"`) and never spills into a neighbour, which is what keeps a layout's shape as the terminal is resized.\n"
    );
    let _ = writeln!(
        o,
        "```toml\n[[row]]\ngap = 2\n[[row.col]]\nmodules = [\"path\", \"branch\"]\n[[row.col]]\nmodules = [\"model\", \"effort\"]\n[[row.col]]\nmodules = [\"context\"]\n```\n\n```text\n{}\n```\n",
        layout_sample(
            "icons = \"unicode\"\n[[row]]\ngap = 2\n[[row.col]]\nmodules = [\"path\", \"branch\"]\n[[row.col]]\nmodules = [\"model\", \"effort\"]\n[[row.col]]\nmodules = [\"context\"]\n",
            120,
        )
    );
    let _ = writeln!(
        o,
        "A column can hold a **stack** of rows instead of modules (`[[row.col.row]]`), and then the row is as tall as its tallest column; `valign` (`top` \\| `center` \\| `bottom`) places a stack shorter than its row. An inner row takes every row key but `gap` and `[[row.col]]`: the tree is two levels deep and never deeper.\n"
    );
}

/// Titles on a row's rule (SPEC § 4.3).
fn titles_section(o: &mut String) {
    let _ = writeln!(
        o,
        "## Titles\n\n`title` is plain text set into a row's rule in the frame colour, with `title_pad` spaces on each side (1 by default) and `title_color` for another role or literal. `title_justify` puts it right after the left cap, centred in the widest empty gap of the line, or right before the right cap. A title wider than its space is cut with `…` and never widens the line, and a row with only a title is a titled spacer that is always kept.\n"
    );
    let _ = writeln!(
        o,
        "```toml\n[[row]]\ntitle = \"Session\"\nmodules = []\n\n[[row]]\ntitle = \"Usage\"\ntitle_justify = \"right\"\nmodules = [\"limit5h\", \"limit7d\"]\n```\n\n```text\n{}\n```\n",
        layout_sample(
            "icons = \"unicode\"\n[[row]]\ntitle = \"Session\"\nmodules = []\n[[row]]\ntitle = \"Usage\"\ntitle_justify = \"right\"\nmodules = [\"limit5h\", \"limit7d\"]\n",
            80,
        )
    );
}

/// `[box.<name>]` (SPEC § 4.3).
fn boxes_section(o: &mut String) {
    let _ = writeln!(
        o,
        "## `[box.<name>]`\n\nA box frames a run of rows, or a whole column, with its own corners and sides in place of the frame's caps: two extra lines, so a box is at least three lines tall. Three ways to join one: adjacent rows with the same `box = \"<name>\"` form one box, inside a stack as at the top level; `box = \"<name>\"` or `box = true` on a column makes the whole column one box the row's full height; `box = true` on a row boxes that row alone, and then the row's own `title*` keys title it. Boxes never nest. A box is one run of adjacent rows in the whole file, so a name that comes back anywhere after it is reported and that run left unboxed.\n\n| key | default | meaning |\n|---|---|---|\n| `title` `title_justify` `title_pad` `title_color` | none | the title set into the box's top rule, as for a row |\n| `style` | the `[frame]` style | `none` \\| `rounded` \\| `square` \\| `double` \\| `heavy` \\| `custom`; when the frame's style has no box shape (`none`, `powerline`) an unstyled box is `rounded`, and a box that asks for `none` itself is invisible |\n| `fill` | `false` | draw the rule between a row's groups inside the box; off by default, because a clean interior is what a box is for |\n| `color` | the frame colour | role or literal for the box's glyphs |\n"
    );
    let _ = writeln!(
        o,
        "```toml\n[box.repo]\ntitle = \"Repository\"\nstyle = \"double\"\n\n[[row]]\nbox = \"repo\"\nmodules = [\"path\", \"model\"]\nright   = [\"clock\"]\n\n[[row]]\nbox = \"repo\"\nmodules = [\"context\"]\n```\n\n```text\n{}\n```\n",
        layout_sample(
            "icons = \"unicode\"\n[box.repo]\ntitle = \"Repository\"\nstyle = \"double\"\n[[row]]\nbox = \"repo\"\nmodules = [\"path\", \"model\"]\nright = [\"clock\"]\n[[row]]\nbox = \"repo\"\nmodules = [\"context\"]\n",
            60,
        )
    );
}

fn presets_section(o: &mut String) {
    let _ = writeln!(
        o,
        "## `[[row]]`\n\nEach entry is one row of the status line. `modules` are left-aligned, `right` are right-aligned, `separator` overrides the frame separator for that row. Any module id may appear on any row, in any order; a module that has nothing to show is skipped, and a row whose modules all have nothing to show is dropped (`hide_empty_rows`). `modules = []` with no `right` is a spacer: an empty framed row that always stays. With `style = \"none\"` a spacer is whitespace only, and Claude Code drops whitespace-only rows from the script's output when colour is off (`color = \"never\"`, `NO_COLOR`; with colour on the rule's colour codes keep the row; `preview --color never` shows what the screen drops). `blank = true` on the spacer keeps it on screen either way by giving the row one invisible cell (a braille blank, U+2800, which the harness does not trim and a font with the clock spinner's braille should draw empty). It is off by default, so the harness's own rule stands unless you opt in; on a row with modules it is reported.\n\n`[[line]]` is the permanent alias of `[[row]]`: every config written before rows existed keeps working, and `config check` says nothing about it. A file uses one name or the other — carrying both arrays is reported and the `[[line]]` entries ignored, because TOML gives no order between two arrays of tables.\n"
    );
    let _ = writeln!(
        o,
        "```toml\n[[row]]\nmodules = [\"path\", \"branch\", \"sync\", \"pr\"]\nright   = [\"clock\"]\nseparator = \"  \"\n\n[[row]]\nmodules = []          # a spacer\nblank = true          # keep it on screen even without a frame\n```\n"
    );
    columns_section(o);
    titles_section(o);
    boxes_section(o);

    let _ = writeln!(o, "## Top-level presets\n");
    for preset in config::presets::TopPreset::ALL {
        let lines: Vec<String> = preset
            .rows()
            .iter()
            .map(|r| {
                // Preset rows are the plain one-column form.
                let col = r.single().cloned().unwrap_or_default();
                let right = if col.right.is_empty() {
                    String::new()
                } else {
                    format!(" ⟶ {}", col.right.join(" "))
                };
                format!("`{}`{}", col.left.join(" "), right)
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
        "## `[modules.<id>]`\n\nEvery module accepts `enabled`, `preset`, `refresh`, `label`, `prefix`, `suffix`, `hide_when_empty`, `max_width` (cells the whole module is cut to with `…`, before alignment; 0 = unlimited), an `icons` table and a `colors` table, plus its own options. Resolution order: built-in default → icon set → module preset → top-level preset → explicit key. See the per-module pages in [modules/](modules/). `[modules.text.<name>]` defines a text box of your own, placed as `text.<name>`; see [text](modules/text.md).\n"
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
        "| `GARNISH_ANIMATE` | `0` freezes every animation (spinner, scrolling text, rule pattern, separator and icon frames) at frame 0 for the session and cuts a ticker line with `…`; for screen readers and recordings. |"
    );
    let _ = writeln!(
        o,
        "| `GARNISH_DEBUG` | `1` appends a line per tick to `<cache>/debug.log`, rotated at 1 MiB; `garnish doctor` shows the tail. Nothing is written otherwise. |"
    );
    let _ = writeln!(
        o,
        "| `GARNISH_MANAGED_SETTINGS` | The organisation settings file read first in Claude Code's chain, instead of the platform's; empty means there is none. |"
    );
    let _ = writeln!(
        o,
        "| `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, `DISABLE_AUTO_COMPACT`, `DISABLE_COMPACT` | Read to place the `context` compaction marker exactly where Claude Code will compact; the last two turn compaction off, so the marker goes with it. |"
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

/// The `docs/presets.md` gallery page (SPEC § 12): every embedded preset
/// rendered at its declared width with the `subscription-full` payload at
/// frame 0, followed by the file itself in a collapsed block.
#[must_use]
pub fn presets_page() -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "# Presets gallery\n\nComplete configs from [`presets/`](../presets/). Copy one to `~/.config/garnish/garnish.toml`, point `GARNISH_CONFIG` at it, or write it with `garnish config init --preset <name>`; `garnish presets` lists them. Each sample is rendered at the preset's declared terminal width from the `subscription-full` payload with animations frozen at frame 0 (a ticker preset therefore shows its row cut with `…`, as it looks with animations off; in a live session it scrolls); presets that need a Nerd Font show their glyphs as boxes here unless your browser has one. The fit holds for the icon set the preset declares (`# needs:`); with `--icons emoji` some glyphs are two cells and a tight layout may need a wider terminal. A real-terminal capture may accompany a preset as `presets/screenshots/<name>.png`.\n"
    );
    let _ = writeln!(o, "| name | summary | columns | needs |\n|---|---|---|---|");
    for p in crate::gallery::PRESETS.iter() {
        let _ = writeln!(
            o,
            "| [`{}`](#{}) | {} | {} | {} |",
            p.name,
            p.name,
            p.summary,
            p.columns,
            p.needs.as_deref().unwrap_or("—")
        );
    }
    let payload = fixture("subscription-full");
    for p in crate::gallery::PRESETS.iter() {
        let (cfg, _) = config::parse(&crate::gallery::body(p.source), &SCHEMAS);
        let sample = render_plain_at(&payload, &cfg, Some(p.columns), &Clock::fixed());
        let needs = p.needs.as_deref().map_or(String::new(), |n| format!(", needs {n}"));
        let author = p.author.as_deref().map_or(String::new(), |a| format!(" · by @{a}"));
        let _ = writeln!(
            o,
            "\n## `{}`\n\n{}\n\nAt {} columns{needs}{author}:\n\n```text\n{}\n```\n\n<details><summary><code>presets/{}.toml</code></summary>\n\n```toml\n{}```\n\n</details>",
            p.name,
            p.summary,
            p.columns,
            sample.trim_end(),
            p.name,
            p.source
        );
    }
    o
}

/// The `docs/README.md` index.
#[must_use]
pub fn index_page() -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "# garnish reference\n\nGenerated by `garnish docs` from the module schemas; do not edit these pages by hand (the [guide](guide.md) is the one hand-written page). Start with the guide, then the [configuration reference](config.md).\n"
    );
    let _ = writeln!(o, "## Modules\n\n| module | shows | refresh |\n|---|---|---|");
    for s in SCHEMAS.iter() {
        let refresh = if s.refresh == 0 { "tick".to_owned() } else { format!("{} s", s.refresh) };
        let _ = writeln!(o, "| [`{id}`](modules/{id}.md) | {} | {refresh} |", s.summary, id = s.id);
    }
    let _ = writeln!(
        o,
        "| [`text.<name>`](modules/text.md) | {} | tick |",
        crate::modules::text::SCHEMA.summary
    );
    let _ = writeln!(
        o,
        "\nComplete example configs, rendered at their own widths, are in the [presets gallery](presets.md).\n\n## Default preset, unicode icons\n\n```text\n{}\n```",
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
        std::fs::write(out.join("modules").join(format!("{}.md", s.id)), module_page(s))?;
        n = n.saturating_add(1);
    }
    std::fs::write(out.join("modules").join("text.md"), text_page())?;
    n = n.saturating_add(1);
    std::fs::write(out.join("presets.md"), presets_page())?;
    n = n.saturating_add(1);
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
        // `show` prints the animation switch in effect, so an unset one
        // comes back explicit (SPEC § 4.2); everything else is identical.
        let mut expected = cfg.clone();
        expected.animate = Some(cfg.animate.unwrap_or(true));
        assert_eq!(again, expected);
        let model = again.modules.get("model").unwrap();
        assert_eq!(model.color("name"), cfg.theme.role(Role::Danger));
        assert!(!model.hide_when_empty);
    }

    #[test]
    fn every_module_has_a_page_with_samples_for_every_preset_and_icon_set() {
        for s in SCHEMAS.iter() {
            let page = module_page(s);
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

    /// `config show` of a config with text modules parses back to the same
    /// `Config`: names are bare keys and the `color` shorthand is written as
    /// `colors.text`.
    /// Every animation key survives `config show`: frames, steps, direction
    /// and pattern come back as the same `Config`.
    #[test]
    fn resolved_config_round_trips_animation_keys() {
        let text = "animate = false\n[frame]\nfill_pattern = \"·  \"\nfill_step = 0.5\nfill_direction = \"left\"\nseparator_frames = [\" │ \", \" ┃ \"]\nseparator_step = 2\n[modules.model.icons]\nmodel_frames = [\"◐\", \"◓\"]\n[modules.clock.icons]\nspinner_frames = [\"ab\", \"cd\"]\n";
        let (cfg, errs) = config::parse(text, &SCHEMAS);
        assert_eq!(errs, Vec::new());
        let shown = config_toml(&cfg, false);
        assert!(shown.contains("model_frames = [\"◐\", \"◓\"]"), "{shown}");
        let (again, errs) = config::parse(&shown, &SCHEMAS);
        assert_eq!(errs, Vec::new(), "{shown}");
        assert_eq!(again, cfg);
        assert_eq!(config_toml(&again, false), shown, "show is idempotent");
    }

    #[test]
    fn resolved_config_round_trips_text_modules() {
        let text = "[[line]]\nmodules = [\"path\", \"text.motd\"]\nright = [\"text.tag\"]\n[modules.text.motd]\ntext = \"ship it\"\nwidth = 12\noverflow = \"scroll-wrap\"\ngap = \" · \"\nstep = 0.5\nlabel = \"motd\"\n[modules.text.tag]\ntext = \"v0.2\"\ncolor = \"muted\"\njustify = \"right\"\n";
        let (cfg, errs) = config::parse(text, &SCHEMAS);
        assert_eq!(errs, Vec::new());
        let shown = config_toml(&cfg, false);
        assert!(
            shown.contains("[modules.text.motd]") && shown.contains("[modules.text.tag.colors]"),
            "{shown}"
        );
        let (again, errs) = config::parse(&shown, &SCHEMAS);
        assert_eq!(errs, Vec::new(), "{shown}");
        let mut expected = cfg.clone();
        expected.animate = Some(cfg.animate.unwrap_or(true));
        assert_eq!(again, expected);
        assert_eq!(config_toml(&again, false), shown, "show is idempotent");
        // The annotated form carries the tables too and still parses.
        let (from_init, errs) = config::parse(&config_toml(&cfg, true), &SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(from_init.texts.len(), 2);
        // `init` leaves `animate` to Claude Code's prefersReducedMotion (SPEC § 4.2).
        assert_eq!(from_init.animate, None);
        assert!(config_toml(&cfg, true).contains("\n# animate = true\n"));
    }

    #[test]
    fn generate_writes_all_pages() {
        let dir = tempfile::tempdir().unwrap();
        let n = generate(dir.path()).unwrap();
        assert_eq!(
            n,
            4 + SCHEMAS.len(),
            "index, config, presets, one page per module, the text family"
        );
        let presets = std::fs::read_to_string(dir.path().join("presets.md")).unwrap();
        for p in crate::gallery::PRESETS.iter() {
            assert!(presets.contains(&format!("## `{}`", p.name)), "{}", p.name);
        }
        assert!(presets.contains("<details>") && presets.contains("```toml"), "{presets}");
        assert!(dir.path().join("modules").join("clock.md").exists());
        let text = std::fs::read_to_string(dir.path().join("modules").join("text.md")).unwrap();
        assert!(text.contains("| `overflow` |") && text.contains("v0.2"), "{text}");
    }
}
