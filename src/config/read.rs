//! Readers for one TOML value at a time: each converts a value to its typed
//! field or reports it under its TOML path and leaves the field unset, so a
//! bad key falls back to its default alone (SPEC § 5).

use std::collections::BTreeMap;

use super::{ConfigError, MAX_TEXT_CHARS, Vocab};
use crate::ansi::Color;
use crate::theme::{Role, Theme};

/// A non-negative count with a ceiling: above it the key is reported and
/// left unset, so its default applies (the pattern of
/// [`super::schema::OptSpec`]'s `max`, for the layout keys the schemas do
/// not own).
pub(super) fn bounded_count(
    path: &str,
    value: toml::Value,
    max: usize,
    errors: &mut Vec<ConfigError>,
) -> Option<usize> {
    let n = field::<usize>(path, value, errors)?;
    if n > max {
        errors.push(problem(path, &format!("must be at most {max}")));
        return None;
    }
    Some(n)
}

/// A config string that reaches a row: reduced to plain text and capped, as
/// every other row string is (SPEC § 5).
pub(super) fn text_field(
    path: &str,
    value: toml::Value,
    errors: &mut Vec<ConfigError>,
) -> Option<String> {
    let text = field::<String>(path, value, errors)?;
    if text.chars().count() > MAX_TEXT_CHARS {
        errors.push(problem(path, &format!("must be at most {MAX_TEXT_CHARS} characters")));
        return None;
    }
    Some(crate::ansi::plain_text(&text))
}

/// Convert one TOML value to its typed field, reporting a bad one under
/// `path` and leaving the field unset so its default applies.
pub(super) fn field<T: serde::de::DeserializeOwned>(
    path: &str,
    value: toml::Value,
    errors: &mut Vec<ConfigError>,
) -> Option<T> {
    match value.try_into::<T>() {
        Ok(v) => Some(v),
        Err(e) => {
            // serde names Rust types; say what a person can type instead.
            let message = e
                .message()
                .replace("expected u16", "expected an integer 0–65535")
                .replace("expected u32", "expected a non-negative integer")
                .replace("expected u64", "expected a non-negative integer")
                .replace("expected f64", "expected a number");
            errors.push(problem(path, &message));
            None
        }
    }
}

/// A key with a fixed vocabulary: one of `T`'s words, read through
/// [`Vocab::parse`] and refused naming [`Vocab::choices`], so the words the
/// parser takes and the ones its message lists are the same list. The list
/// is only built on the error path.
pub(super) fn enum_field<T: Vocab>(
    path: &str,
    value: &toml::Value,
    errors: &mut Vec<ConfigError>,
) -> Option<T> {
    let Some(text) = value.as_str() else {
        errors.push(problem(path, &format!("expected a string, one of {}", T::choices())));
        return None;
    };
    T::parse(text).or_else(|| {
        let message = format!("unknown value {text:?}; expected one of {}", T::choices());
        errors.push(problem(path, &message));
        None
    })
}

/// A `[[line]]` module list kept item by item: a non-string item is reported
/// under its index and skipped, the rest of the line stays (one typo must not
/// blank a whole row, SPEC § 5).
pub(super) fn id_list(
    path: &str,
    value: toml::Value,
    errors: &mut Vec<ConfigError>,
) -> Vec<String> {
    let toml::Value::Array(items) = value else {
        errors.push(problem(path, "expected a list of module ids"));
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(j, item)| {
            if let toml::Value::String(s) = item {
                Some(s)
            } else {
                errors.push(problem(&format!("{path}[{j}]"), "expected a module id string"));
                None
            }
        })
        .collect()
}

/// A table of string values, skipping (and reporting) entries of another type.
pub(super) fn string_table(
    path: &str,
    table: toml::Table,
    errors: &mut Vec<ConfigError>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (k, v) in table {
        match v {
            toml::Value::String(s) => {
                out.insert(k, s);
            }
            _ => errors.push(problem(&format!("{path}.{k}"), "expected a string")),
        }
    }
    out
}

pub(super) fn problem(path: &str, message: &str) -> ConfigError {
    ConfigError { path: path.to_owned(), message: message.to_owned(), line: None }
}

/// What a key that takes a colour spec accepts, as its message names it.
const COLOR_SPECS: &str = "a role name, a color name, 0-255, or #rrggbb";

/// Whether `spec` is a colour a role-or-literal key takes: a theme role
/// (resolved against the theme in effect) or a literal colour.
pub(super) fn is_color_spec(spec: &str) -> bool {
    Role::parse(spec).is_some() || Color::parse(spec).is_some()
}

/// A role-or-literal colour (a title, a box, a module's `colors.*`, a text
/// module's `color`, a colour list) resolved against `theme`, or the one
/// message every such key reports.
pub(super) fn color_spec(theme: &Theme, spec: &str) -> Result<Color, String> {
    theme.resolve(spec).ok_or_else(|| bad_color(spec))
}

/// The message for a value that is no colour spec.
pub(super) fn bad_color(spec: &str) -> String {
    format!("invalid color {spec:?}; use {COLOR_SPECS}")
}

/// [`bad_color`] for `separator_color`, which takes `inherit` too.
pub(super) fn bad_color_or_inherit(spec: &str) -> String {
    format!("invalid color {spec:?}; use inherit, {COLOR_SPECS}")
}

/// A literal colour (a `[colors]` role's value: a role defined by another
/// role would have no ground), or the literal-only form of the message.
pub(super) fn literal_color(spec: &str) -> Result<Color, String> {
    Color::parse(spec)
        .ok_or_else(|| format!("invalid color {spec:?}; use a color name, 0-255, or #rrggbb"))
}

/// A bare TOML key: what a text module or a box may be called, so
/// `text.<name>` and `box = "<name>"` are unambiguous on a line and
/// `config show` can write `[modules.text.<name>]` and `[box.<name>]` back
/// verbatim.
#[must_use]
pub fn is_bare_key(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// An animation frame list (SPEC § 4.2): plain text, at least one frame, and
/// every frame the same width, or whatever it decorates jitters as the
/// frames cycle. `what` names that thing in the message.
///
/// One rule for the frame's `separator_frames` and every icon table's
/// `<key>_frames`: every frame is the same width, or the thing they draw
/// jitters from tick to tick.
///
/// `allow_empty` is what the two keys disagree on, and it is a
/// compatibility rule rather than a design one: `[]` is the line every
/// `garnish config init` has ever written for `separator_frames` (it means
/// "no animation, keep the static separator"), so rejecting it would put a
/// `⚠ config:` row on every tick of every config in the wild. An icon's
/// `<key>_frames = []` has always been reported and nothing generates it.
pub(super) fn equal_width_frames<'a>(
    frames: impl IntoIterator<Item = &'a str>,
    what: &str,
    allow_empty: bool,
) -> Result<Vec<String>, String> {
    let frames: Vec<String> = frames.into_iter().map(crate::ansi::plain_text).collect();
    if frames.is_empty() && allow_empty {
        return Ok(frames);
    }
    let Some(width) = frames.first().map(|f| crate::ansi::display_width(f)) else {
        return Err("expected at least one frame".to_owned());
    };
    if frames.iter().all(|f| crate::ansi::display_width(f) == width) {
        Ok(frames)
    } else {
        Err(format!("every frame must have the same width, or {what} would jitter"))
    }
}
