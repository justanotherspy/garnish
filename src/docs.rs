//! Documentation and config generation from the module schemas.

use std::fmt::Write as _;

use crate::config::Config;
use crate::config::schema::{Preset, Value, toml_string};
use crate::modules::SCHEMAS;
use crate::theme::PALETTES;

/// Render a config as annotated TOML.
///
/// With `annotated`, every option carries its doc comment and every module is
/// written out with its preset defaults, so the file doubles as a reference.
#[must_use]
pub fn config_toml(cfg: &Config, annotated: bool) -> String {
    let mut out = String::new();
    let c = |s: &mut String, text: &str| {
        if annotated {
            let _ = writeln!(s, "# {text}");
        }
    };
    c(&mut out, "garnish configuration — see docs/config.md for every key.");
    c(&mut out, "Top-level preset: default | minimal | full | compact");
    let _ = writeln!(out, "preset = {}", toml_string(cfg.preset.name()));
    c(&mut out, "Icon set: nerd | unicode | emoji | ascii");
    let _ = writeln!(out, "icons = {}", toml_string(cfg.icons.name()));
    c(
        &mut out,
        &format!("Theme: {}", PALETTES.iter().map(|p| p.name).collect::<Vec<_>>().join(" | ")),
    );
    let _ = writeln!(out, "theme = {}", toml_string(&cfg.theme_name));
    c(&mut out, "Color output: auto | always | never | 256 | truecolor");
    let _ = writeln!(out, "color = {}", toml_string(color_name(cfg.color)));
    c(&mut out, "Truncate the left group when a line overflows $COLUMNS.");
    let _ = writeln!(out, "truncate = {}", cfg.truncate);
    c(&mut out, "Stale cached values: dim | hide | plain");
    let _ = writeln!(out, "stale_style = {}", toml_string(stale_name(cfg.stale_style)));
    c(&mut out, "Cells subtracted from the width (mirror statusLine.padding).");
    let _ = writeln!(out, "padding = {}", cfg.padding);
    let _ = writeln!(out);

    c(
        &mut out,
        "Role color overrides: accent accent2 muted text ok warn hot danger frame band1..band4",
    );
    let _ = writeln!(out, "[colors]");
    c(&mut out, "accent = \"#89b4fa\"");
    let _ = writeln!(out);

    c(&mut out, "Frame style: none | rounded | square | double | heavy | powerline | custom");
    let _ = writeln!(out, "[frame]");
    let _ = writeln!(out, "style = {}", toml_string(cfg.frame.style.name()));
    c(&mut out, "Extend the rule to the full width and close with the right cap.");
    let _ = writeln!(out, "fill = {}", cfg.frame.fill);
    c(&mut out, "Default separator between modules on a line.");
    let _ = writeln!(out, "separator = {}", toml_string(&cfg.frame.chars.separator));
    c(
        &mut out,
        "For style = \"custom\": first middle last single fill_char right_first right_middle right_last right_single pad",
    );
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
            "enabled / preset (minimal|default|full) / refresh (seconds, 0 = every tick) / label / prefix / suffix / hide_when_empty",
        );
        let _ = writeln!(out, "enabled = {}", m.enabled);
        let _ = writeln!(out, "preset = {}", toml_string(m.preset.name()));
        let _ = writeln!(out, "refresh = {}", m.refresh);
        if annotated {
            let _ = writeln!(out, "# label = \"\"");
        }
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
            let _ = writeln!(out, "{} = {}", opt.key, value);
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
                }
                let _ = writeln!(out, "{} = {}", color.key, toml_string(color.default));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    #[test]
    fn generated_config_round_trips_through_the_parser() {
        for annotated in [false, true] {
            let (cfg, errs) = config::parse("", &SCHEMAS);
            assert_eq!(errs.len(), 0);
            let text = config_toml(&cfg, annotated);
            let (again, errs) = config::parse(&text, &SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}\n{text}");
            assert_eq!(again.lines, cfg.lines);
            assert_eq!(again.frame, cfg.frame);
            for (id, m) in &cfg.modules {
                let n = again.modules.get(id).unwrap();
                assert_eq!(n.opts(), m.opts(), "{id}");
                assert_eq!(n.icons(), m.icons(), "{id}");
                assert_eq!(n.preset, m.preset);
            }
        }
    }
}
