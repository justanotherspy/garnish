//! The `[frame]` table (SPEC § 4, § 4.2, § 4.3): the caps, rule and
//! separator glyphs, their animation and the separator colour, resolved
//! into a [`FrameCfg`].

use super::presets::TopPreset;
use super::read::{
    bad_color_or_inherit, color_spec, enum_field, equal_width_frames, field, problem,
};
use super::{ConfigError, resolve_step};
use crate::ansi::Color;
use crate::frame::{FrameChars, FrameStyle};
use crate::theme::{Role, Theme};

/// Which way an animated rule pattern travels (`[frame] fill_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillDirection {
    /// Toward the left cap.
    Left,
    /// Toward the right cap.
    #[default]
    Right,
}

impl FillDirection {
    /// Both directions, in the order the reference lists them.
    pub const ALL: [Self; 2] = [Self::Left, Self::Right];

    /// Config name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// `[frame] separator_color` (SPEC § 4.1): one colour for every separator,
/// or the colour of the module before each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeparatorColor {
    /// A role or a literal, as written, and what it resolves to.
    Fixed {
        /// The value as the file wrote it (`muted` when unset), for `config show`.
        spec: String,
        /// The resolved colour.
        color: Color,
    },
    /// The first coloured, undimmed segment of the module before the
    /// separator (an icon or a value, never a label or an align pad).
    Inherit,
}

impl SeparatorColor {
    /// The value as written in a config.
    #[must_use]
    pub fn spec(&self) -> &str {
        match self {
            Self::Fixed { spec, .. } => spec,
            Self::Inherit => "inherit",
        }
    }
}

/// Frame configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameCfg {
    /// Style.
    pub style: FrameStyle,
    /// Characters in effect.
    pub chars: FrameChars,
    /// Fill the rule to the full width.
    pub fill: bool,
    /// The colour of every separator (SPEC § 4.1).
    pub separator_color: SeparatorColor,
    /// One-cell glyphs repeated across the rule instead of `fill_char`;
    /// empty means the static rule (SPEC § 4.2).
    pub fill_pattern: Vec<String>,
    /// Cells the pattern shifts per tick.
    pub fill_step: f64,
    /// Which way the pattern travels.
    pub fill_direction: FillDirection,
    /// Separator frames cycled one per tick (all the same width); empty
    /// means the static `separator`.
    pub separator_frames: Vec<String>,
    /// Frames the separator advances per tick.
    pub separator_step: f64,
}

#[derive(Debug, Default)]
pub(super) struct RawFrame {
    style: Option<FrameStyle>,
    fill: Option<bool>,
    first: Option<String>,
    middle: Option<String>,
    last: Option<String>,
    single: Option<String>,
    fill_char: Option<String>,
    right_first: Option<String>,
    right_middle: Option<String>,
    right_last: Option<String>,
    right_single: Option<String>,
    pad: Option<String>,
    separator: Option<String>,
    separator_color: Option<String>,
    fill_pattern: Option<String>,
    fill_step: Option<f64>,
    fill_direction: Option<FillDirection>,
    separator_frames: Option<Vec<String>>,
    separator_step: Option<f64>,
    top_left: Option<String>,
    top_right: Option<String>,
    bottom_left: Option<String>,
    bottom_right: Option<String>,
    side: Option<String>,
}

/// Every key `[frame]` takes, in the order the "expected one of" message
/// names them.
pub(super) const FRAME_KEYS: [&str; 24] = [
    "style",
    "fill",
    "first",
    "middle",
    "last",
    "single",
    "fill_char",
    "right_first",
    "right_middle",
    "right_last",
    "right_single",
    "pad",
    "separator",
    "separator_color",
    "fill_pattern",
    "fill_step",
    "fill_direction",
    "separator_frames",
    "separator_step",
    "top_left",
    "top_right",
    "bottom_left",
    "bottom_right",
    "side",
];

impl RawFrame {
    pub(super) fn from_table(table: toml::Table, errors: &mut Vec<ConfigError>) -> Self {
        let mut f = Self::default();
        for (key, value) in table {
            let path = format!("frame.{key}");
            let text_slot = match key.as_str() {
                "first" => Some(&mut f.first),
                "middle" => Some(&mut f.middle),
                "last" => Some(&mut f.last),
                "single" => Some(&mut f.single),
                "fill_char" => Some(&mut f.fill_char),
                "right_first" => Some(&mut f.right_first),
                "right_middle" => Some(&mut f.right_middle),
                "right_last" => Some(&mut f.right_last),
                "right_single" => Some(&mut f.right_single),
                "pad" => Some(&mut f.pad),
                "separator" => Some(&mut f.separator),
                "fill_pattern" => Some(&mut f.fill_pattern),
                "top_left" => Some(&mut f.top_left),
                "top_right" => Some(&mut f.top_right),
                "bottom_left" => Some(&mut f.bottom_left),
                "bottom_right" => Some(&mut f.bottom_right),
                "side" => Some(&mut f.side),
                _ => None,
            };
            if let Some(slot) = text_slot {
                // Frame glyphs enter the width arithmetic directly, so they
                // are reduced to plain text here, before any cell is counted.
                *slot = field::<String>(&path, value, errors).map(|s| crate::ansi::plain_text(&s));
                continue;
            }
            match key.as_str() {
                "style" => f.style = enum_field(&path, &value, errors),
                "fill" => f.fill = field(&path, value, errors),
                // A colour spec, resolved against the theme in `resolve_frame`.
                "separator_color" => f.separator_color = field(&path, value, errors),
                "fill_step" => f.fill_step = field(&path, value, errors),
                "fill_direction" => f.fill_direction = enum_field(&path, &value, errors),
                "separator_frames" => f.separator_frames = field(&path, value, errors),
                "separator_step" => f.separator_step = field(&path, value, errors),
                _ => errors.push(problem(
                    &path,
                    &format!("unknown key; expected one of {}", FRAME_KEYS.join(", ")),
                )),
            }
        }
        f
    }
}

pub(super) fn resolve_frame(
    raw: Option<&RawFrame>,
    preset: TopPreset,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> FrameCfg {
    let fallback = if preset.framed() { FrameStyle::Rounded } else { FrameStyle::None };
    let style = raw.and_then(|f| f.style).unwrap_or(fallback);
    let mut chars = FrameChars::for_style(style);
    if let Some(f) = raw {
        let set = |dst: &mut String, src: &Option<String>| {
            if let Some(v) = src {
                dst.clone_from(v);
            }
        };
        set(&mut chars.first, &f.first);
        set(&mut chars.middle, &f.middle);
        set(&mut chars.last, &f.last);
        set(&mut chars.single, &f.single);
        // The fill glyph is repeated across the rule, so it has to be one
        // cell; anything else is reported and the style's own glyph stays.
        match &f.fill_char {
            Some(c) if crate::ansi::display_width(c) == 1 => chars.fill.clone_from(c),
            Some(_) => errors.push(problem("frame.fill_char", "must be exactly one cell wide")),
            None => {}
        }
        set(&mut chars.right_first, &f.right_first);
        set(&mut chars.right_middle, &f.right_middle);
        set(&mut chars.right_last, &f.right_last);
        set(&mut chars.right_single, &f.right_single);
        set(&mut chars.pad, &f.pad);
        set(&mut chars.separator, &f.separator);
        // The five box glyphs (SPEC § 4.3) are drawn one per cell at the
        // ends of a box's lines, so each is one cell or the style's own
        // glyph stays.
        for (key, given, dst) in [
            ("top_left", &f.top_left, &mut chars.top_left),
            ("top_right", &f.top_right, &mut chars.top_right),
            ("bottom_left", &f.bottom_left, &mut chars.bottom_left),
            ("bottom_right", &f.bottom_right, &mut chars.bottom_right),
            ("side", &f.side, &mut chars.side),
        ] {
            match given {
                Some(c) if crate::ansi::display_width(c) == 1 => dst.clone_from(c),
                Some(_) => {
                    let path = format!("frame.{key}");
                    errors.push(problem(&path, "must be exactly one cell wide"));
                }
                None => {}
            }
        }
    }
    // Filling is on for every style: with `none` the rule is spaces, which is
    // what right-aligns the `right` group on an unframed line.
    let fill = raw.and_then(|f| f.fill).unwrap_or(true);
    // SPEC § 4.2: a pattern is one-cell glyphs (each lands in exactly one
    // rule cell); separator frames all share one width so columns never
    // jitter. Anything else is reported and the static fallback stays.
    let fill_pattern: Vec<String> = raw
        .and_then(|f| f.fill_pattern.as_deref())
        .map(crate::ansi::plain_text)
        .filter(|p| !p.is_empty())
        .map_or_else(Vec::new, |p| {
            let cells: Vec<String> = p.chars().map(|c| c.to_string()).collect();
            if cells.iter().all(|c| crate::ansi::display_width(c) == 1) {
                cells
            } else {
                errors.push(problem(
                    "frame.fill_pattern",
                    "every glyph in the pattern must be one cell wide",
                ));
                Vec::new()
            }
        });
    let fill_pattern = if !fill && !fill_pattern.is_empty() {
        errors.push(problem(
            "frame.fill_pattern",
            "has no effect with fill = false (there is no rule to paint)",
        ));
        Vec::new()
    } else {
        fill_pattern
    };
    let separator_frames: Vec<String> =
        raw.and_then(|f| f.separator_frames.as_deref()).map_or_else(Vec::new, |frames| {
            equal_width_frames(frames.iter().map(String::as_str), "the columns", true)
                .inspect_err(|msg| errors.push(problem("frame.separator_frames", msg)))
                .unwrap_or_default()
        });
    let separator_color =
        resolve_separator_color(raw.and_then(|f| f.separator_color.as_deref()), theme, errors);
    FrameCfg {
        style,
        chars,
        fill,
        separator_color,
        fill_pattern,
        fill_step: resolve_step("frame.fill_step", raw.and_then(|f| f.fill_step), errors),
        fill_direction: raw.and_then(|f| f.fill_direction).unwrap_or_default(),
        separator_frames,
        separator_step: resolve_step(
            "frame.separator_step",
            raw.and_then(|f| f.separator_step),
            errors,
        ),
    }
}

/// SPEC § 4.1 `separator_color`: `inherit`, or a role or literal resolved
/// now, the muted role standing in for a bad value as it does when unset.
fn resolve_separator_color(
    spec: Option<&str>,
    theme: &Theme,
    errors: &mut Vec<ConfigError>,
) -> SeparatorColor {
    let muted =
        || SeparatorColor::Fixed { spec: "muted".to_owned(), color: theme.role(Role::Muted) };
    match spec {
        None => muted(),
        Some("inherit") => SeparatorColor::Inherit,
        Some(spec) => color_spec(theme, spec).map_or_else(
            |_| {
                errors.push(problem("frame.separator_color", &bad_color_or_inherit(spec)));
                muted()
            },
            |color| SeparatorColor::Fixed { spec: spec.to_owned(), color },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::schemas;
    use crate::config::{RowCfg, parse};

    /// A `custom` frame carries the five box glyphs too (SPEC § 4.3), each
    /// one cell like the caps, and a built-in style brings its own.
    #[test]
    fn a_custom_frame_takes_the_five_box_glyphs() {
        let schemas = schemas();
        let keys = ["top_left", "top_right", "bottom_left", "bottom_right", "side"];
        let mut body = String::new();
        for key in keys {
            body.push_str(key);
            body.push_str(" = \"+\"\n");
        }
        let (c, errs) = parse(&format!("[frame]\nstyle = \"custom\"\n{body}"), &schemas);
        assert_eq!(errs, Vec::new());
        let ch = &c.frame.chars;
        assert_eq!(
            [&ch.top_left, &ch.top_right, &ch.bottom_left, &ch.bottom_right, &ch.side],
            [&"+".to_owned(); 5]
        );
        // A glyph wider than one cell would push every box line out of line.
        for key in keys {
            let (c, errs) =
                parse(&format!("[frame]\nstyle = \"custom\"\n{key} = \"ab\"\n"), &schemas);
            assert_eq!(errs.len(), 1, "{key}: {errs:?}");
            assert_eq!(errs[0].path, format!("frame.{key}"));
            assert!(errs[0].message.contains("one cell"), "{}", errs[0].message);
            assert!(c.frame.chars.side.is_empty(), "{key}: the style's own glyph stays");
        }
        // The built-in shapes come with their own; `none` has none, which is
        // what makes its box invisible.
        let (c, _) = parse("[frame]\nstyle = \"double\"\n", &schemas);
        assert_eq!(c.frame.chars.side, "║");
        let (c, _) = parse("[frame]\nstyle = \"none\"\n", &schemas);
        assert_eq!(c.frame.chars.side, "", "an invisible box has no side glyph");
    }

    /// frm-12: `FrameChars::named` is the form's, its suggestions' and
    /// `config show`'s glyph table, so it names every glyph key the parser
    /// takes and nothing else, and each key reads the field the parser sets.
    #[test]
    fn the_named_glyphs_are_the_parsers_glyph_keys() {
        let schemas = schemas();
        let named: Vec<&str> =
            FrameChars::for_style(FrameStyle::Custom).named().iter().map(|g| g.key).collect();
        let not_glyphs = [
            "style",
            "fill",
            "separator_color",
            "fill_pattern",
            "fill_step",
            "fill_direction",
            "separator_frames",
            "separator_step",
        ];
        let mut expected: Vec<&str> =
            FRAME_KEYS.iter().copied().filter(|k| !not_glyphs.contains(k)).collect();
        let mut sorted = named.clone();
        expected.sort_unstable();
        sorted.sort_unstable();
        assert_eq!(sorted, expected);
        // One distinct one-cell glyph per key: each comes back under its key.
        let glyphs = "abcdefghijklmnop";
        let body = named
            .iter()
            .zip(glyphs.chars())
            .map(|(key, glyph)| format!("{key} = \"{glyph}\""))
            .collect::<Vec<_>>()
            .join("\n");
        let (c, errs) = parse(&format!("[frame]\nstyle = \"custom\"\n{body}"), &schemas);
        assert_eq!(errs, Vec::new());
        for (g, glyph) in c.frame.chars.named().iter().zip(glyphs.chars()) {
            assert_eq!(g.value, glyph.to_string(), "{}", g.key);
            assert!(!g.doc.is_empty(), "{}", g.key);
        }
        // A one-cell glyph is one the parser refuses wider.
        for g in c.frame.chars.named() {
            let (_, errs) =
                parse(&format!("[frame]\nstyle = \"custom\"\n{} = \"ab\"\n", g.key), &schemas);
            assert_eq!(g.one_cell, !errs.is_empty(), "{}: {errs:?}", g.key);
        }
    }

    /// SPEC § 4.2: a rule pattern is one-cell glyphs, separator frames share
    /// one width; bad values are reported and the static frame stays.
    #[test]
    fn frame_animation_keys_parse_and_validate() {
        let schemas = schemas();
        let (c, errs) = parse("", &schemas);
        assert_eq!(errs, Vec::new());
        assert!(c.frame.fill_pattern.is_empty() && c.frame.separator_frames.is_empty());
        assert_eq!(c.frame.fill_direction, FillDirection::Right);
        let text = "[frame]\nfill_pattern = \"·  \"\nfill_step = 0.5\nfill_direction = \"left\"\nseparator_frames = [\" │ \", \" ┃ \", \" ╎ \"]\nseparator_step = 2\n";
        let (c, errs) = parse(text, &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.fill_pattern, vec!["·", " ", " "]);
        assert!((c.frame.fill_step - 0.5).abs() < f64::EPSILON);
        assert_eq!(c.frame.fill_direction, FillDirection::Left);
        assert_eq!(c.frame.separator_frames, vec![" │ ", " ┃ ", " ╎ "]);
        assert_eq!(c.separator_at(&c.rows[0], 1), " ┃ ");
        assert_eq!(c.separator_at(&c.rows[0], 7), " │ ", "out of range → static");
        let line = RowCfg { separator: Some("--".into()), ..c.rows[0].clone() };
        assert_eq!(c.separator_at(&line, 1), "--", "a per-line separator wins");
        // A two-cell glyph in the pattern, frames of unequal width, a bad
        // direction and a zero step: each reported under its path, each
        // falling back to the static frame.
        let bad = "[frame]\nfill_pattern = \"·🌿\"\nseparator_frames = [\" │ \", \"│\"]\nfill_direction = \"up\"\nfill_step = 0\n";
        let (c, errs) = parse(bad, &schemas);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "frame.fill_direction",
                "frame.fill_pattern",
                "frame.separator_frames",
                "frame.fill_step"
            ],
            "{errs:?}"
        );
        assert!(c.frame.fill_pattern.is_empty() && c.frame.separator_frames.is_empty());
        assert!((c.frame.fill_step - 1.0).abs() < f64::EPSILON);
        // Escapes in a frame never reach the row.
        let (c, errs) =
            parse("[frame]\nseparator_frames = [\"\\u001b[1m|\\u001b[0m\", \":\"]\n", &schemas);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_frames, vec!["|", ":"]);
        // A pattern without a rule to paint is a dead key: said, not ignored.
        let (c, errs) = parse("[frame]\nfill = false\nfill_pattern = \"·  \"\n", &schemas);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].path, "frame.fill_pattern");
        assert_eq!(c.frame.fill_pattern, Vec::<String>::new());
    }

    /// `separator_frames = []` is the line every `garnish config init` has
    /// written, and it means "no animation". Rejecting it would put a
    /// `⚠ config:` row on every tick of every config already on disk, so the
    /// empty list is legal there and reported for an icon, which nothing
    /// generates.
    #[test]
    fn an_empty_frame_list_is_legal_for_the_separator_and_not_for_an_icon() {
        // The real schema set: `config show` writes every module, so a round
        // trip through a reduced one would fail on the modules it omits.
        let all = &crate::modules::SCHEMAS;
        let (c, errs) = parse("[frame]\nseparator_frames = []\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_frames, Vec::<String>::new());
        // What `config init` writes still parses clean after a round trip.
        let shown = crate::docs::config_toml(&c, false);
        assert!(shown.contains("separator_frames = []"), "{shown}");
        assert_eq!(parse(&shown, all).1, Vec::new());

        let (_, errs) = parse("[modules.model.icons]\nmodel_frames = []\n", all);
        let problems: Vec<(&str, &str)> =
            errs.iter().map(|e| (e.path.as_str(), e.message.as_str())).collect();
        assert_eq!(problems, [("modules.model.icons.model_frames", "expected at least one frame")]);
    }

    /// SPEC § 4.1 `separator_color`: a role or a literal resolved at config
    /// time, `inherit`, the muted role when unset or bad, and `config show`
    /// writes the value as it was written.
    #[test]
    fn separator_color_resolves_inherits_or_falls_back_to_muted() {
        let all = &crate::modules::SCHEMAS;
        let (c, errs) = parse("", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(
            c.frame.separator_color,
            SeparatorColor::Fixed { spec: "muted".into(), color: c.theme.role(Role::Muted) }
        );
        let (c, errs) = parse("[frame]\nseparator_color = \"inherit\"\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_color, SeparatorColor::Inherit);
        let (c, errs) = parse("[frame]\nseparator_color = \"accent\"\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(
            c.frame.separator_color,
            SeparatorColor::Fixed { spec: "accent".into(), color: c.theme.role(Role::Accent) }
        );
        let (c, errs) = parse("[frame]\nseparator_color = \"#ff8800\"\n", all);
        assert_eq!(errs, Vec::new());
        assert_eq!(c.frame.separator_color.spec(), "#ff8800");
        let (c, errs) = parse("[frame]\nseparator_color = \"nope\"\n", all);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "frame.separator_color");
        assert!(errs[0].message.contains("inherit"), "{}", errs[0].message);
        assert_eq!(c.frame.separator_color.spec(), "muted");
        let (_, errs) = parse("[frame]\nseparator_color = 3\n", all);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "frame.separator_color");
        // `config show` writes the value as written and it parses back.
        for spec in ["inherit", "accent", "#ff8800", "muted"] {
            let (c, _) = parse(&format!("[frame]\nseparator_color = \"{spec}\"\n"), all);
            let shown = crate::docs::config_toml(&c, false);
            assert!(shown.contains(&format!("separator_color = \"{spec}\"")), "{shown}");
            let (again, errs) = parse(&shown, all);
            assert_eq!(errs, Vec::new());
            assert_eq!(again.frame, c.frame);
        }
    }
}
