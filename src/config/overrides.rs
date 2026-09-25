//! Module tables (`[modules.<id>]` and `[modules.text.<name>]`, SPEC § 3,
//! § 3.7): each key checked against the module's schema and collected as
//! [`Overrides`] for [`super::schema::ModuleCfg::resolve`].

use std::collections::BTreeMap;

use super::read::{bad_color, equal_width_frames, is_bare_key, is_color_spec, problem};
use super::schema::{
    COMMON_OPTS, HideRule, Kind, ModuleCfg, ModuleSchema, OptSpec, Overrides, Preset, Value,
    common_keys,
};
use super::{ConfigError, STEP_MESSAGE, STEP_RANGE, Vocab};
use crate::icons::IconSet;
use crate::theme::Theme;

/// The common keys a `[modules.text.<name>]` table may not carry (SPEC
/// § 3.7), each with why: reported and removed before the shared parser
/// sees the table, and left out of the "expected one of" list for a text
/// module so the message never recommends one of them.
const TEXT_REJECTED_KEYS: [(&str, &str); 4] = [
    ("refresh", "text modules render every tick; remove this key"),
    ("preset", "text modules have no presets; remove this key"),
    ("icons", "text modules have no icons; remove this table"),
    ("max_width", "a text module's box is sized by `width`; remove this key"),
];

/// Whether a `[modules.text.<name>]` table takes the common key: every one
/// but the [`TEXT_REJECTED_KEYS`] (SPEC § 3.7).
#[must_use]
pub fn text_takes(key: &str) -> bool {
    !TEXT_REJECTED_KEYS.iter().any(|(rejected, _)| *rejected == key)
}

/// The `[modules.text.<name>]` tables (SPEC § 3.7): each is validated against
/// the text schema under its own path. [`TEXT_REJECTED_KEYS`] do not apply to
/// text modules, `step` must lie in [`STEP_RANGE`], `color` is the shorthand
/// for `colors.text` (an explicit `colors.text` wins), and `text` and `gap`
/// are reduced to plain text so a scrolled window can never cut an escape
/// sequence.
pub(super) fn resolve_texts(
    family: Option<&toml::Table>,
    icons: IconSet,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<String, ModuleCfg> {
    let schema = &*crate::modules::text::SCHEMA;
    let mut texts = BTreeMap::new();
    for (name, value) in family.into_iter().flatten() {
        let base = format!("modules.text.{name}");
        if !is_bare_key(name) {
            errors.push(problem(
                &base,
                "a text module name is letters, digits, `_` and `-` only (it becomes the id text.<name>)",
            ));
            continue;
        }
        let Some(table) = value.as_table() else {
            errors.push(problem(&base, "expected a [modules.text.<name>] table"));
            continue;
        };
        let mut table = table.clone();
        let color = table.remove("color");
        for (key, why) in TEXT_REJECTED_KEYS {
            if table.remove(key).is_some() {
                errors.push(problem(&format!("{base}.{key}"), why));
            }
        }
        let mut ov = parse_overrides(schema, &base, &table, errors);
        if let Some(color) = color {
            let path = format!("{base}.color");
            match color.as_str() {
                Some(s) if is_color_spec(s) => {
                    ov.colors.entry("text".to_owned()).or_insert_with(|| s.to_owned());
                }
                Some(s) => errors.push(problem(&path, &bad_color(s))),
                None => errors.push(problem(&path, "expected a string")),
            }
        }
        if let Some(Value::Float(step)) = ov.opts.get("step")
            && !STEP_RANGE.contains(step)
        {
            errors.push(problem(&format!("{base}.step"), STEP_MESSAGE));
            ov.opts.remove("step");
        }
        for key in ["text", "gap"] {
            if let Some(Value::Str(s)) = ov.opts.get_mut(key) {
                *s = crate::ansi::plain_text(s);
            }
        }
        // `url` (SPEC § 3.7) must meet the painter's rule (§ 5), or the
        // link would vanish on screen with `config check` saying nothing.
        if let Some(Value::Str(url)) = ov.opts.get("url")
            && !url.is_empty()
            && !crate::ansi::safe_link(url)
        {
            errors.push(problem(
                &format!("{base}.url"),
                "must be an http:// or https:// URL of printable ASCII (percent-encode anything else)",
            ));
            ov.opts.remove("url");
        }
        texts.insert(name.clone(), ModuleCfg::resolve(schema, Preset::Default, icons, theme, &ov));
    }
    texts
}

pub(super) fn parse_overrides(
    schema: &ModuleSchema,
    base: &str,
    table: &toml::Table,
    errors: &mut Vec<ConfigError>,
) -> Overrides {
    let mut ov = Overrides::default();
    let mut err = |key: &str, msg: String| {
        errors.push(ConfigError { path: format!("{base}.{key}"), message: msg, line: None });
    };
    for (key, value) in table {
        match key.as_str() {
            "enabled" => match value.as_bool() {
                Some(b) => ov.enabled = Some(b),
                None => err(key, "expected true or false".into()),
            },
            "preset" => match value.as_str().and_then(Preset::parse) {
                Some(p) => ov.preset = Some(p),
                None => err(key, format!("expected one of {}", Preset::choices())),
            },
            "refresh" => match value.as_integer().and_then(|i| u64::try_from(i).ok()) {
                Some(0) if schema.refresh > 0 => err(
                    key,
                    "this module is refreshed by a background worker; use at least 1 second".into(),
                ),
                Some(n) => ov.refresh = Some(n),
                None => err(key, "expected a non-negative integer (seconds)".into()),
            },
            // Checked against the schema's measure (SPEC § 3), as `refresh`
            // is against `schema.refresh`, so hand-parsed like it.
            "hide" => match hide_rules(schema, value) {
                Ok(rules) => ov.hide = Some(rules),
                Err(msg) => err(key, msg),
            },
            "icons" => match value.as_table() {
                Some(t) => parse_icons(schema, t, &mut ov, &mut err),
                None => err(key, "expected a table of icon overrides".into()),
            },
            "colors" => match value.as_table() {
                Some(t) => parse_colors(schema, t, &mut ov, &mut err),
                None => err(key, "expected a table of color overrides".into()),
            },
            // The common options and the module's own go through the same
            // coercion and cap (`COMMON_OPTS` first: a schema never redeclares
            // a common key).
            other => {
                match COMMON_OPTS.iter().find(|o| o.key == other).or_else(|| schema.opt(other)) {
                    Some(spec) => match coerce(spec.kind, value).and_then(|v| bounded(spec, v)) {
                        Ok(v) if !set_common(&mut ov, other, v.clone()) => {
                            ov.opts.insert(other.to_owned(), v);
                        }
                        Ok(_) => {}
                        Err(msg) => err(other, msg),
                    },
                    None => err(other, unknown_option_message(schema)),
                }
            }
        }
    }
    ov
}

/// Store a coerced [`COMMON_OPTS`] value on the overrides; `false` when the
/// key is not a common option. The row strings are reduced to plain text
/// here like `text` and `gap` (SPEC § 5).
fn set_common(ov: &mut Overrides, key: &str, value: Value) -> bool {
    match (key, value) {
        ("label", Value::Str(s)) => ov.label = Some(crate::ansi::plain_text(&s)),
        ("prefix", Value::Str(s)) => ov.prefix = Some(crate::ansi::plain_text(&s)),
        ("suffix", Value::Str(s)) => ov.suffix = Some(crate::ansi::plain_text(&s)),
        ("hide_when_empty", Value::Bool(b)) => ov.hide_when_empty = Some(b),
        // `coerce` rejects a negative integer, so the conversion cannot fail.
        ("max_width", Value::Int(n)) => ov.max_width = Some(u64::try_from(n).unwrap_or(0)),
        _ => return false,
    }
    true
}

/// A module's `hide` list (SPEC § 3): every entry a state the schema's
/// measure allows, or the reason the key is refused whole (the per-key
/// fallback of § 5: the default, no hiding, stands in for the list).
fn hide_rules(schema: &ModuleSchema, value: &toml::Value) -> Result<Vec<HideRule>, String> {
    let accepts = || format!("this module accepts {}", schema.hide_states().join(", "));
    let items = value.as_array().ok_or_else(|| "expected a list of strings".to_owned())?;
    items
        .iter()
        .map(|item| {
            let text = item.as_str().ok_or_else(|| "expected a list of strings".to_owned())?;
            let rule = HideRule::parse(text).map_err(|e| format!("{e}; {}", accepts()))?;
            if rule.applies_to(schema.measure) {
                Ok(rule)
            } else {
                Err(format!("{text:?} does not apply here; {}", accepts()))
            }
        })
        .collect()
}

/// The size limit a module option must respect, from its schema
/// ([`OptSpec::max`]): cell counts at most [`super::MAX_CELLS`], row text at
/// most [`super::MAX_TEXT_CHARS`] characters, and so on. A row is a fixed,
/// small thing; a number beyond the cap is a mistake, and honouring it would
/// size an allocation or a loop on every tick.
fn bounded(spec: &OptSpec, value: Value) -> Result<Value, String> {
    spec.over_max(&value).map_or(Ok(value), Err)
}

fn unknown_option_message(schema: &ModuleSchema) -> String {
    let is_text = schema.id == crate::modules::text::SCHEMA.id;
    format!(
        "unknown option; expected one of {}",
        common_keys()
            .filter(|k| !is_text || text_takes(k))
            .chain(std::iter::once("colors"))
            .chain(schema.opts.iter().map(|o| o.key))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn parse_icons(
    schema: &ModuleSchema,
    table: &toml::Table,
    ov: &mut Overrides,
    err: &mut impl FnMut(&str, String),
) {
    for (ik, iv) in table {
        // `<key>_frames`: equal-width frames cycled one per tick (SPEC § 4.2).
        if let Some(base) = ik.strip_suffix("_frames")
            && let Some(spec) = schema.icon(base)
        {
            let given: Option<Vec<&str>> =
                iv.as_array().and_then(|items| items.iter().map(toml::Value::as_str).collect());
            match given {
                Some(frames) => match equal_width_frames(frames, "the row", false) {
                    // A frame becomes the glyph for that tick, so the bar's
                    // one-cell rule applies to every frame too. Checking only
                    // the static arm below left the same defect reachable
                    // through `fill_frames`, equal widths and all.
                    Ok(frames)
                        if spec.one_cell()
                            && frames.iter().any(|f| {
                                !(f.is_empty() && spec.may_be_blank())
                                    && crate::ansi::display_width(f) != 1
                            }) =>
                    {
                        err(&format!("icons.{ik}"), "must be exactly one cell wide".into());
                    }
                    Ok(frames) => {
                        ov.icon_frames.insert(base.to_owned(), frames);
                    }
                    Err(msg) => err(&format!("icons.{ik}"), msg),
                },
                None => err(&format!("icons.{ik}"), "expected a list of strings".into()),
            }
            continue;
        }
        match (schema.icon(ik), iv.as_str()) {
            (Some(spec), Some(s)) => {
                let glyph = crate::ansi::plain_text(s);
                // A bar glyph is repeated cell by cell, so anything but one
                // cell breaks the row's arithmetic; `bar` would swap in a
                // safe glyph and the override would vanish in silence. Same
                // rule, same message as `frame.fill_char`. Blanking the
                // marker is not that case: it is how the marker is turned
                // off, and `bar` honours it (`IconSpec::may_be_blank`).
                let blanked = glyph.is_empty() && spec.may_be_blank();
                if spec.one_cell() && !blanked && crate::ansi::display_width(&glyph) != 1 {
                    err(&format!("icons.{ik}"), "must be exactly one cell wide".into());
                } else {
                    ov.icons.insert(ik.clone(), glyph);
                }
            }
            (None, _) => err(
                &format!("icons.{ik}"),
                format!(
                    "unknown icon; expected one of {}",
                    schema.icons.iter().map(|i| i.key).collect::<Vec<_>>().join(", ")
                ),
            ),
            (_, None) => err(&format!("icons.{ik}"), "expected a string".into()),
        }
    }
}

fn parse_colors(
    schema: &ModuleSchema,
    table: &toml::Table,
    ov: &mut Overrides,
    err: &mut impl FnMut(&str, String),
) {
    for (ck, cv) in table {
        match (schema.color(ck), cv.as_str()) {
            (Some(_), Some(s)) if is_color_spec(s) => {
                ov.colors.insert(ck.clone(), s.to_owned());
            }
            (Some(_), Some(s)) => err(&format!("colors.{ck}"), bad_color(s)),
            (None, _) => err(
                &format!("colors.{ck}"),
                format!(
                    "unknown color; expected one of {}",
                    schema.colors.iter().map(|c| c.key).collect::<Vec<_>>().join(", ")
                ),
            ),
            (_, None) => err(&format!("colors.{ck}"), "expected a string".into()),
        }
    }
}

fn coerce(kind: Kind, value: &toml::Value) -> Result<Value, String> {
    match kind {
        Kind::Bool => {
            value.as_bool().map(Value::Bool).ok_or_else(|| "expected true or false".into())
        }
        Kind::Int => value
            .as_integer()
            .filter(|i| *i >= 0)
            .map(Value::Int)
            .ok_or_else(|| "expected a non-negative integer".into()),
        // TOML takes `nan` and `inf`; no option means either.
        Kind::Float => value
            .as_float()
            .or_else(|| {
                value.as_integer().map(|i| crate::num::u64_to_f64(u64::try_from(i).unwrap_or(0)))
            })
            .filter(|f| f.is_finite())
            .map(Value::Float)
            .ok_or_else(|| "expected a number".into()),
        Kind::Str => value
            .as_str()
            .map(|s| Value::Str(s.to_owned()))
            .ok_or_else(|| "expected a string".into()),
        Kind::Enum(allowed) => value
            .as_str()
            .filter(|s| allowed.contains(s))
            .map(|s| Value::Str(s.to_owned()))
            .ok_or_else(|| {
                format!(
                    "expected one of {}",
                    allowed.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(", ")
                )
            }),
        Kind::StrList | Kind::ColorList => {
            let items = value.as_array().ok_or_else(|| "expected a list of strings".to_owned())?;
            let strs: Option<Vec<String>> =
                items.iter().map(|i| i.as_str().map(str::to_owned)).collect();
            let strs = strs.ok_or_else(|| "expected a list of strings".to_owned())?;
            let bad = (kind == Kind::ColorList)
                .then(|| strs.iter().enumerate().find(|(_, s)| !is_color_spec(s)))
                .flatten();
            if let Some((i, spec)) = bad {
                return Err(format!("item {i}: {}", bad_color(spec)));
            }
            Ok(Value::StrList(strs))
        }
        Kind::NumList => {
            let items = value.as_array().ok_or_else(|| "expected a list of numbers".to_owned())?;
            let nums: Option<Vec<f64>> = items
                .iter()
                .map(|i| {
                    i.as_float()
                        .or_else(|| {
                            i.as_integer()
                                .map(|n| crate::num::u64_to_f64(u64::try_from(n).unwrap_or(0)))
                        })
                        .filter(|f| f.is_finite())
                })
                .collect();
            nums.map(Value::NumList).ok_or_else(|| "expected a list of numbers".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::Color;
    use crate::config::tests::schemas;
    use crate::config::{MAX_CELLS, MAX_DECIMALS, MAX_TEXT_CHARS, parse};
    use crate::theme::Role;

    /// SPEC § 3.7: `[modules.text.<name>]` tables are validated against the
    /// text schema under their own path; `text.<name>` is a valid line id
    /// only with a table; `refresh`, `preset` and a non-positive `step` are
    /// rejected; a non-table entry is reported.
    #[test]
    fn text_modules_are_defined_by_the_config() {
        let schemas = schemas();
        let text = "[[line]]\nmodules = [\"path\", \"text.motd\"]\nright = [\"text.tag\"]\n[modules.text.motd]\ntext = \"hello\"\nwidth = 12\noverflow = \"scroll-wrap\"\nstep = 0.5\n[modules.text.tag]\ntext = \"v0.2\"\ncolor = \"muted\"\njustify = \"right\"\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.texts.len(), 2);
        let motd = c.texts.get("motd").unwrap();
        assert_eq!(motd.str("text"), "hello");
        assert_eq!(motd.size("width"), 12);
        assert_eq!(motd.str("overflow"), "scroll-wrap");
        assert!((motd.float("step") - 0.5).abs() < f64::EPSILON);
        let tag = c.texts.get("tag").unwrap();
        assert_eq!(tag.str("justify"), "right");
        assert_eq!(tag.color("text"), c.theme.role(Role::Muted), "`color` shorthand");
        assert!(!c.modules.contains_key("text"), "the family is not a registry module");

        let bad = "[[line]]\nmodules = [\"text.ghost\"]\n[modules.text.motd]\ntext = \"x\"\nrefresh = 5\npreset = \"bogus\"\nstep = 0\njustify = \"middle\"\ncolor = \"bogus\"\nwat = 1\n[modules.text.motd.icons]\nfoo = \"x\"\n[modules.text.\"sp ace\"]\ntext = \"y\"\n[modules.text]\nplain = 3\n";
        let (c, errs) = parse(bad, &schemas);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        for expected in [
            "line[0].modules[0]",
            "modules.text.motd.refresh",
            "modules.text.motd.preset",
            "modules.text.motd.step",
            "modules.text.motd.justify",
            "modules.text.motd.color",
            "modules.text.motd.wat",
            "modules.text.motd.icons",
            "modules.text.sp ace",
            "modules.text.plain",
        ] {
            assert!(paths.contains(&expected), "{expected} missing from {paths:?}");
        }
        // One error per bad key, not one from the generic walk plus one from the family.
        assert_eq!(paths.iter().filter(|p| **p == "modules.text.motd.preset").count(), 1);
        assert_eq!(paths.iter().filter(|p| **p == "modules.text.motd.refresh").count(), 1);
        assert!(!c.texts.contains_key("sp ace"), "a non-bare name is rejected");
        assert!(errs.iter().any(|e| e.message.contains("define [modules.text.ghost]")), "{errs:?}");
        let motd = c.texts.get("motd").unwrap();
        assert!((motd.float("step") - 1.0).abs() < f64::EPSILON, "bad step → default");
        assert_eq!(motd.str("justify"), "left", "bad justify → default");
        assert_eq!(motd.refresh, 0);
        assert_eq!(c.rows[0].cols[0].left, Vec::<String>::new(), "the unknown id is removed");

        // An explicit colors.text wins over the shorthand; text and gap are plain.
        let text = "[modules.text.x]\ntext = \"\\u001b[31mred\\u001b[0m\\tnote\"\ngap = \" \\u001b[5m·\\u001b[0m \"\ncolor = \"muted\"\n[modules.text.x.colors]\ntext = \"red\"\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        let x = c.texts.get("x").unwrap();
        assert_eq!(x.color("text"), Color::Ansi(1), "explicit colors.text wins");
        assert_eq!(x.str("text"), "rednote");
        assert_eq!(x.str("gap"), " · ");

        // `url` (SPEC § 3.7) must meet the painter's rule, else it is
        // reported and dropped; an empty one is no link at all.
        let text = "[modules.text.a]\ntext = \"a\"\nurl = \"https://example.com/x?y=1\"\n[modules.text.b]\ntext = \"b\"\nurl = \"ftp://example.com\"\n[modules.text.c]\ntext = \"c\"\nurl = \"https://ex ample.com\"\n[modules.text.d]\ntext = \"d\"\nurl = \"\"\n";
        let (c, errs) = parse(text, &schemas);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["modules.text.b.url", "modules.text.c.url"], "{errs:?}");
        assert!(errs[0].message.contains("http:// or https://"), "{errs:?}");
        assert_eq!(c.texts.get("a").unwrap().str("url"), "https://example.com/x?y=1");
        assert_eq!(c.texts.get("b").unwrap().str("url"), "", "dropped");
        assert_eq!(c.texts.get("c").unwrap().str("url"), "", "dropped");
        assert_eq!(c.texts.get("d").unwrap().str("url"), "");
    }

    /// SPEC § 4.2 Animated glyphs: `<key>_frames` on any icon key, equal
    /// widths enforced, plain text, unknown base keys and bad shapes reported.
    #[test]
    fn icon_frames_parse_and_validate() {
        let schemas = schemas();
        let text =
            "[modules.path.icons]\nfolder_frames = [\"a\", \"\\u001b[1mb\\u001b[0m\", \"c\"]\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.modules.get("path").unwrap().icon_frames("folder"), ["a", "b", "c"]);
        for (bad, path) in [
            (
                "[modules.path.icons]\nfolder_frames = [\"a\", \"🌿\"]\n",
                "modules.path.icons.folder_frames",
            ),
            ("[modules.path.icons]\nfolder_frames = []\n", "modules.path.icons.folder_frames"),
            ("[modules.path.icons]\nfolder_frames = \"abc\"\n", "modules.path.icons.folder_frames"),
            ("[modules.path.icons]\nghost_frames = [\"a\"]\n", "modules.path.icons.ghost_frames"),
        ] {
            let (c, errs) = parse(bad, &schemas);
            assert_eq!(errs.len(), 1, "{bad}: {errs:?}");
            assert_eq!(errs[0].path, path, "{bad}");
            assert!(c.modules.get("path").unwrap().icon_frames("folder").is_empty(), "{bad}");
        }
    }

    #[test]
    fn sizes_and_text_lengths_are_bounded_at_config_time() {
        // A cell count or a string a row can never show would only size an
        // allocation or a loop on every tick; it is reported and defaulted.
        let big = "[modules.context]\nwidth = 99999999999\n[modules.text.a]\ntext = \"hi\"\nwidth = 1025\npad = 4000000000\n[modules.limit5h]\nbar_width = 1025\n";
        let (c, errs) = parse(big, &crate::modules::SCHEMAS);
        // The modules come in schema order and a table's keys in the order
        // the file wrote them (the table keeps its order, so `setup` can
        // write it back as read).
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "modules.context.width",
                "modules.limit5h.bar_width",
                "modules.text.a.width",
                "modules.text.a.pad"
            ],
            "{errs:?}"
        );
        assert!(errs.iter().all(|e| e.message == "must be at most 1024"), "{errs:?}");
        assert_eq!(c.modules.get("context").unwrap().size("width"), 20, "default stands in");
        let a = c.texts.get("a").unwrap();
        assert_eq!((a.size("width"), a.size("pad")), (0, 0));
        // `decimals` sizes the money formatter's buffer: capped the same way.
        let (c, errs) = parse("[modules.cost]\ndecimals = 4000000000\n", &crate::modules::SCHEMAS);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(
            (errs[0].path.as_str(), errs[0].message.as_str()),
            ("modules.cost.decimals", "must be at most 8")
        );
        assert_eq!(c.modules.get("cost").unwrap().size("decimals"), 2);
        // Every cap lives in the schema, so the reference can print it.
        let capped: std::collections::BTreeSet<(&str, &str, usize)> = crate::modules::SCHEMAS
            .iter()
            .chain(std::iter::once(&*crate::modules::text::SCHEMA))
            .flat_map(|s| s.opts.iter().filter_map(|o| Some((s.id, o.key, o.max?))))
            .collect();
        assert_eq!(
            capped,
            [
                ("context", "width", MAX_CELLS),
                ("cost", "decimals", MAX_DECIMALS),
                ("limit5h", "bar_width", MAX_CELLS),
                ("limit7d", "bar_width", MAX_CELLS),
                ("spend", "bar_width", MAX_CELLS),
                ("text", "gap", MAX_TEXT_CHARS),
                ("text", "pad", MAX_CELLS),
                ("text", "text", MAX_TEXT_CHARS),
                ("text", "url", MAX_TEXT_CHARS),
                ("text", "width", MAX_CELLS),
            ]
            .into_iter()
            .collect()
        );
        let (ok, errs) = parse("[modules.context]\nwidth = 1024\n", &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(ok.modules.get("context").unwrap().size("width"), 1024);
        let long = format!(
            "[modules.text.a]\ntext = \"{}\"\ngap = \"{}\"\n",
            "x".repeat(MAX_TEXT_CHARS + 1),
            "y".repeat(MAX_TEXT_CHARS)
        );
        let (c, errs) = parse(&long, &crate::modules::SCHEMAS);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "modules.text.a.text");
        assert!(errs[0].message.contains("at most 4096 characters"), "{errs:?}");
        assert_eq!(c.texts.get("a").unwrap().str("gap").chars().count(), MAX_TEXT_CHARS);
        // The common row strings are bounded the same way.
        let long = format!(
            "[modules.model]\nlabel = \"{}\"\nprefix = \"{}\"\nsuffix = \"ok\"\n",
            "x".repeat(MAX_TEXT_CHARS + 1),
            "y".repeat(MAX_TEXT_CHARS)
        );
        let (c, errs) = parse(&long, &crate::modules::SCHEMAS);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["modules.model.label"], "{errs:?}");
        let model = c.modules.get("model").unwrap();
        assert_eq!((model.label.as_str(), model.prefix.chars().count()), ("", MAX_TEXT_CHARS));
        // The common options carry their caps as specs, like any option.
        let common: Vec<(&str, Option<usize>)> =
            COMMON_OPTS.iter().map(|o| (o.key, o.max)).collect();
        assert_eq!(
            common,
            [
                ("label", Some(MAX_TEXT_CHARS)),
                ("prefix", Some(MAX_TEXT_CHARS)),
                ("suffix", Some(MAX_TEXT_CHARS)),
                ("hide_when_empty", None),
                ("max_width", Some(MAX_CELLS)),
            ]
        );
    }

    /// `parse_overrides` looks a key up in `COMMON_OPTS` before the module's
    /// own schema, so a schema that redeclared a common key would be silently
    /// shadowed: its option would parse against the common spec and
    /// `cfg.str("label")` would read the schema default for ever. The
    /// comment there states the invariant; this is what enforces it. The
    /// other half of the same trap is a `COMMON_OPTS` entry with no
    /// `set_common` arm, which would be accepted and dropped into `ov.opts`.
    #[test]
    fn no_schema_redeclares_a_common_key_and_every_common_key_is_stored() {
        // Every key `parse_overrides` handles before the schema, not just
        // the `COMMON_OPTS` five: `enabled`, `preset`, `refresh`, `icons`
        // and `colors` are matched by name too, so a schema option with one
        // of those keys would be parsed by the hand-written arm and its
        // `cfg.bool(..)`/`cfg.int(..)` reader would see the default for ever.
        let common: Vec<&str> = common_keys().chain(std::iter::once("colors")).collect();
        let schemas =
            crate::modules::SCHEMAS.iter().chain(std::iter::once(&*crate::modules::text::SCHEMA));
        for schema in schemas {
            for opt in &schema.opts {
                assert!(
                    !common.contains(&opt.key),
                    "module `{}` redeclares the common key `{}`",
                    schema.id,
                    opt.key
                );
            }
        }
        for opt in &COMMON_OPTS {
            let mut ov = Overrides::default();
            assert!(
                set_common(&mut ov, opt.key, opt.default.clone()),
                "`{}` is a common option with no `set_common` arm: it would be dropped",
                opt.key
            );
        }

        // The whole chain, end to end, so the tripwire covers all four edits
        // a sixth common option needs rather than only the `set_common` arm:
        // every key is written into a config, and the value that comes back
        // out of the resolved `ModuleCfg` must be the one that went in. A
        // missing `Overrides` field, `resolve` arm or `common` arm all show
        // up here as the default coming back.
        let not_the_default = |opt: &OptSpec| match &opt.default {
            Value::Bool(b) => Value::Bool(!b),
            Value::Int(n) => Value::Int(n.saturating_add(1)),
            Value::Float(f) => Value::Float(f + 0.5),
            Value::Str(s) => Value::Str(format!("{s}x")),
            other => other.clone(),
        };
        let rows = COMMON_OPTS.iter().map(|opt| {
            let value = match not_the_default(opt) {
                Value::Bool(b) => b.to_string(),
                Value::Int(n) => n.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Str(s) => crate::config::schema::toml_string(&s),
                other => panic!("no literal for {other:?}"),
            };
            format!("{} = {value}\n", opt.key)
        });
        let text: String = std::iter::once("[modules.clock]\n".to_owned()).chain(rows).collect();
        let (cfg, errs) = parse(&text, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new(), "{text}");
        let clock = cfg.modules.get("clock").expect("the clock module");
        for opt in &COMMON_OPTS {
            assert_eq!(
                clock.common(opt.key),
                Some(not_the_default(opt)),
                "`{}` did not survive the config → `ModuleCfg` chain ({text})",
                opt.key
            );
        }

        // sch-11: and unset, each resolves to its spec's default, which is
        // what the reference and `config init` print: `ModuleCfg::resolve`
        // spells its own fallbacks, and they must not drift from the specs.
        let (cfg, errs) = parse("[modules.clock]\n", &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        let clock = cfg.modules.get("clock").expect("the clock module");
        for opt in &COMMON_OPTS {
            assert_eq!(clock.common(opt.key), Some(opt.default.clone()), "`{}` unset", opt.key);
        }
    }

    /// SPEC § 3 `hide`: every state is checked against the schema's measure,
    /// a bad list is refused whole (the default stands in), the list and
    /// `hide_when_empty` are a union, and `config show` writes it back.
    #[test]
    fn hide_lists_follow_the_schemas_measure() {
        let (c, errs) = parse(
            "[modules.cost]\nhide = [\"zero\", \"empty\"]\n[modules.context]\nhide = [\"below:10\", \"above:90.5\"]\n[modules.text.note]\ntext = \"x\"\nhide = [\"empty\"]\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        let cost = c.modules.get("cost").unwrap();
        assert_eq!(cost.hide, vec![HideRule::Zero, HideRule::Empty]);
        assert_eq!(
            c.modules.get("context").unwrap().hide,
            vec![HideRule::Below(10.0), HideRule::Above(90.5)]
        );
        assert_eq!(c.texts.get("note").unwrap().hide, vec![HideRule::Empty]);
        assert!(cost.hides_empty());
        // The union: `empty` in the list hides an empty render whatever
        // `hide_when_empty` says, and the flag alone still works.
        let (c, errs) = parse(
            "[modules.pr]\nhide_when_empty = false\nhide = [\"empty\"]\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        assert!(c.modules.get("pr").unwrap().hides_empty());
        let (c, _) = parse("[modules.pr]\nhide_when_empty = false\n", &crate::modules::SCHEMAS);
        assert!(!c.modules.get("pr").unwrap().hides_empty());
        // Refused lists, each naming what the module accepts; the default stands.
        let refused = [
            (
                "[modules.context]\nhide = [\"zero\"]\n",
                "modules.context",
                "empty, below:N, above:N",
            ),
            ("[modules.cost]\nhide = [\"below:5\"]\n", "modules.cost", "accepts empty, zero"),
            ("[modules.model]\nhide = [\"zero\"]\n", "modules.model", "accepts empty"),
            (
                "[modules.text.a]\ntext = \"x\"\nhide = [\"zero\"]\n",
                "modules.text.a",
                "accepts empty",
            ),
            (
                "[modules.cost]\nhide = [\"empty\", \"nope\"]\n",
                "modules.cost",
                "unknown hide state",
            ),
            ("[modules.context]\nhide = [\"below:2000\"]\n", "modules.context", "0 to 1000"),
            ("[modules.context]\nhide = [\"below:x\"]\n", "modules.context", "needs a number"),
            ("[modules.cost]\nhide = \"zero\"\n", "modules.cost", "expected a list"),
            ("[modules.cost]\nhide = [1]\n", "modules.cost", "expected a list"),
        ];
        for (text, base, message) in refused {
            let (c, errs) = parse(text, &crate::modules::SCHEMAS);
            assert_eq!(errs.len(), 1, "{text}: {errs:?}");
            assert_eq!(errs[0].path, format!("{base}.hide"), "{text}");
            assert!(errs[0].message.contains(message), "{text}: {}", errs[0].message);
            // A state the parser does not know is refused naming what the
            // module accepts, as one the measure disallows is.
            if message == "unknown hide state" {
                assert!(
                    errs[0].message.ends_with("; this module accepts empty, zero"),
                    "{text}: {}",
                    errs[0].message
                );
            }
            let id = base.trim_start_matches("modules.");
            let hide =
                c.modules.get(id).map(|m| m.hide.clone()).or_else(|| {
                    c.texts.get(id.trim_start_matches("text.")).map(|m| m.hide.clone())
                });
            assert_eq!(hide, Some(Vec::new()), "{text}");
        }
        // `config show` writes the list, and it parses back to the same rules.
        let (c, _) = parse("[modules.context]\nhide = [\"below:10\"]\n", &crate::modules::SCHEMAS);
        let shown = crate::docs::config_toml(&c, false);
        assert!(shown.contains("hide = [\"below:10\"]"), "{shown}");
        let (again, errs) = parse(&shown, &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(again.modules.get("context").unwrap().hide, vec![HideRule::Below(10.0)]);
        // Every module table round-trips (`show` pins `animate`, so the whole
        // config is compared in the shorthand test, not here).
        assert_eq!(again.modules, c.modules);
        assert_eq!(again.texts, c.texts);
    }

    /// SPEC § 4.1: a bar glyph is repeated cell by cell, so an override
    /// that is not one cell is reported and the schema glyph stays — the
    /// same rule `frame.fill_char` has always had. It used to be swallowed:
    /// `config check` said `ok`, `config show` echoed the glyph, and the
    /// tick drew `█` because `util::bar` substituted one.
    #[test]
    fn a_bar_glyph_override_must_be_one_cell() {
        for (id, key) in [("context", "fill"), ("context", "marker"), ("limit5h", "empty")] {
            let text = format!("[modules.{id}.icons]\n{key} = \"🟩\"\n");
            let (cfg, errs) = parse(&text, &crate::modules::SCHEMAS);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(
                problems,
                [(&*format!("modules.{id}.icons.{key}"), "must be exactly one cell wide")],
                "{text}"
            );
            let module = cfg.modules.get(id).unwrap();
            assert_eq!(crate::ansi::display_width(module.icon(key)), 1, "{text}");
        }
        // A one-cell override is still taken, and a key that is not a bar
        // cell may be any width.
        let (cfg, errs) = parse(
            "[modules.context.icons]\nfill = \"▓\"\ncontext = \"ctx:\"\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        let context = cfg.modules.get("context").unwrap();
        assert_eq!((context.icon("fill"), context.icon("context")), ("▓", "ctx:"));

        // An animated bar glyph is the same rule: a frame is the glyph for
        // its tick, so equal widths are not enough.
        let (_, errs) = parse(
            "[modules.context.icons]\nfill_frames = [\"🟩\", \"🟥\"]\n",
            &crate::modules::SCHEMAS,
        );
        let problems: Vec<(&str, &str)> =
            errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
        assert_eq!(
            problems,
            [("modules.context.icons.fill_frames", "must be exactly one cell wide")]
        );
        let (cfg, errs) = parse(
            "[modules.context.icons]\nfill_frames = [\"▓\", \"▒\"]\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new(), "one-cell frames are still taken");
        assert!(cfg.modules.contains_key("context"));

        // Blanking the marker is how it is turned off, and `util::bar`
        // honours it, so the width rule must not refuse it. The cells
        // themselves cannot be blanked: a zero-width cell has no width to
        // repeat.
        let (cfg, errs) =
            parse("[modules.context.icons]\nmarker = \"\"\n", &crate::modules::SCHEMAS);
        assert_eq!(errs, Vec::new());
        assert_eq!(cfg.modules.get("context").map(|m| m.icon("marker")), Some(""));
        for key in ["fill", "empty"] {
            let text = format!("[modules.context.icons]\n{key} = \"\"\n");
            let (_, errs) = parse(&text, &crate::modules::SCHEMAS);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(
                problems,
                [(&*format!("modules.context.icons.{key}"), "must be exactly one cell wide")],
                "{text}"
            );
        }
    }

    /// SPEC § 3.7: the keys a text module refuses are one table, so the
    /// refusal and the "expected one of" list can never drift apart.
    #[test]
    fn every_rejected_text_key_is_refused_with_its_own_message() {
        for (key, why) in TEXT_REJECTED_KEYS {
            let value = if key == "icons" { "{ folder = \"x\" }" } else { "1" };
            let text = format!("[modules.text.a]\ntext = \"hi\"\n{key} = {value}\n");
            let (c, errs) = parse(&text, &crate::modules::SCHEMAS);
            let problems: Vec<(&str, &str)> =
                errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
            assert_eq!(problems, [(&*format!("modules.text.a.{key}"), why)], "{key}");
            assert_eq!(c.texts.get("a").map(|t| t.str("text")), Some("hi"), "{key}");
            assert!(!text_takes(key), "{key}");
        }
        // Every common key `text_takes` (what the docs page and the setup
        // form list) parses there without a word.
        for opt in COMMON_OPTS.iter().filter(|o| text_takes(o.key)) {
            let text = format!("[modules.text.a]\n{} = {}\n", opt.key, opt.default.to_toml());
            assert_eq!(parse(&text, &crate::modules::SCHEMAS).1, Vec::new(), "{}", opt.key);
        }
    }

    /// A number option takes finite numbers only: TOML's `nan` and `inf`
    /// are reported at the key (frm-07), and the default stands in.
    #[test]
    fn a_number_option_refuses_nan_and_inf() {
        for (text, path) in [
            ("[modules.context]\nwarn_at = nan\n", "modules.context.warn_at"),
            ("[modules.context]\nwarn_at = -inf\n", "modules.context.warn_at"),
            ("[modules.context]\nthresholds = [50.0, nan]\n", "modules.context.thresholds"),
        ] {
            let (c, errs) = parse(text, &crate::modules::SCHEMAS);
            let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
            assert_eq!(paths, [path], "{text}");
            let ctx = c.modules.get("context").unwrap();
            assert!(ctx.value("warn_at").is_none_or(|v| *v == Value::Float(0.0)), "{text}");
        }
    }

    /// SPEC § 3: `max_width` is a common option bounded like a cell count;
    /// a text module has `width` instead and is told so (SPEC § 3.7).
    #[test]
    fn max_width_is_a_common_option_except_on_text_modules() {
        let text = "[modules.branch]\nmax_width = 12\n[modules.model]\nmax_width = 2000\n[modules.pr]\nmax_width = -1\n[modules.text.a]\ntext = \"hi\"\nmax_width = 3\n";
        let (c, errs) = parse(text, &crate::modules::SCHEMAS);
        let problems: Vec<(&str, &str)> =
            errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
        assert_eq!(
            problems,
            [
                // Modules are checked in registry order (`pr` before `model`).
                ("modules.pr.max_width", "expected a non-negative integer"),
                ("modules.model.max_width", "must be at most 1024"),
                (
                    "modules.text.a.max_width",
                    "a text module's box is sized by `width`; remove this key"
                ),
            ]
        );
        assert_eq!(c.modules.get("branch").unwrap().max_width, 12);
        assert_eq!(c.modules.get("model").unwrap().max_width, 0, "the default stands in");
        assert_eq!(c.modules.get("pr").unwrap().max_width, 0);
        assert_eq!(c.texts.get("a").unwrap().max_width, 0);
        // An unknown key names it among the common keys, except on a text
        // module, where the message never recommends a key it would reject.
        let (_, errs) = parse("[modules.model]\nmax_widht = 1\n", &crate::modules::SCHEMAS);
        assert!(errs[0].message.contains("max_width"), "{errs:?}");
        let (_, errs) =
            parse("[modules.text.a]\ntext = \"x\"\nmax_widht = 1\n", &crate::modules::SCHEMAS);
        assert_eq!(errs.len(), 1, "{errs:?}");
        for rejected in ["max_width", "preset", "refresh", "icons"] {
            assert!(!errs[0].message.contains(rejected), "{rejected}: {}", errs[0].message);
        }
        assert!(errs[0].message.contains("hide_when_empty, colors, text, width"), "{errs:?}");
        // The common strings are still reduced to plain text on the way in.
        let (c, errs) = parse(
            "[modules.model]\nlabel = \"a\\u001b[31mb\"\nhide_when_empty = false\n",
            &crate::modules::SCHEMAS,
        );
        assert_eq!(errs, Vec::new());
        let model = c.modules.get("model").unwrap();
        assert_eq!(model.label, "ab");
        assert!(!model.hide_when_empty);
        assert_eq!(model.common("hide_when_empty"), Some(Value::Bool(false)));
        assert_eq!(model.common("max_width"), Some(Value::Int(0)));
        assert_eq!(model.common("show_id"), None);
    }
}
