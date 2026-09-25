//! Finding and reading the config file (SPEC § 4, § 5): the location
//! order, the path-variable rules, and the entry points that turn text or a
//! table into a resolved [`Config`] with its problems.

use std::path::{Path, PathBuf};

use super::schema::ModuleSchema;
use super::{Config, ConfigError, Loaded, Overlay, RawConfig, resolve};

/// Environment variable naming the config file.
pub const CONFIG_ENV: &str = "GARNISH_CONFIG";

/// An environment variable holding a path, or `None` when it is unset *or
/// empty*.
///
/// An empty value is the shell's idiom for "unset" (`FOO= cmd`), and the two
/// mean the same thing here: `GARNISH_CONFIG=` once named the empty path,
/// which put `⚠ config: cannot read` on every tick, and `XDG_CONFIG_HOME=`
/// once made the candidate the *relative* `garnish/garnish.toml`, so a
/// checkout holding that file became the user's config for every session
/// started in it. [`crate::claude_settings::home_dir`] is the same rule for
/// `HOME`.
pub fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// An XDG base directory variable (`XDG_CONFIG_HOME`, `XDG_CACHE_HOME`,
/// `XDG_RUNTIME_DIR`): [`env_path`], and `None` for a relative value too.
///
/// The XDG Base Directory spec calls a relative value invalid and says to
/// ignore it, and garnish must: a relative base is the working directory's,
/// which for a tick is the session's repository, so `XDG_CONFIG_HOME=.config`
/// made a checkout's own `.config/garnish/garnish.toml` the config.
fn xdg_path(key: &str) -> Option<PathBuf> {
    xdg_base(env_path(key))
}

/// [`xdg_path`]'s rule for a value already looked up.
pub fn xdg_base(value: Option<PathBuf>) -> Option<PathBuf> {
    value.filter(|p| p.is_absolute())
}

/// The XDG base for garnish's own files: `XDG_CONFIG_HOME`, else `~/.config`.
fn config_home() -> Option<PathBuf> {
    xdg_path("XDG_CONFIG_HOME")
        .or_else(|| crate::claude_settings::home_dir().map(|h| h.join(".config")))
}

/// The config file named explicitly: `--config` (`flag`), else
/// `GARNISH_CONFIG`; `None` when neither names one.
#[must_use]
pub fn explicit(flag: Option<&Path>) -> Option<PathBuf> {
    flag.map(Path::to_path_buf).or_else(|| env_path(CONFIG_ENV))
}

/// Locate the config file: explicit path > `GARNISH_CONFIG` > XDG > `~/.garnish.toml`.
#[must_use]
pub fn locate(flag: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit(flag) {
        return Some(p);
    }
    let xdg = config_home().map(|d| d.join("garnish").join("garnish.toml"));
    if let Some(p) = xdg.filter(|p| p.is_file()) {
        return Some(p);
    }
    crate::claude_settings::home_dir().map(|h| h.join(".garnish.toml")).filter(|p| p.is_file())
}

/// The default location a new config should be written to.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    // Without a home there is no default: guessing `.` would write into
    // whatever directory garnish happens to run from (a repository, say).
    Some(config_home()?.join("garnish").join("garnish.toml"))
}

/// The file a command that writes a config writes: [`locate`], else
/// [`default_path`].
///
/// `config init`, `setup` and `install`'s default config all go there;
/// `None` without a home and without `--config` or `GARNISH_CONFIG` (SPEC
/// § 5: never guess the current directory). Writing the default path while `~/.garnish.toml` is the config would
/// create a file that [`locate`] prefers, and the user's config would stop
/// applying without a word.
#[must_use]
pub fn write_target(explicit: Option<&Path>) -> Option<PathBuf> {
    locate(explicit).or_else(default_path)
}

/// Load and resolve the configuration. Never fails: a bad key is reported
/// and defaulted on its own; only an unreadable or non-TOML file yields the
/// built-in defaults wholesale, plus the error.
#[must_use]
pub fn load(explicit: Option<&Path>, schemas: &[ModuleSchema]) -> Loaded {
    load_with(explicit, schemas, &Overlay::default())
}

/// [`load`] with command-line overrides.
#[must_use]
pub fn load_with(explicit: Option<&Path>, schemas: &[ModuleSchema], overlay: &Overlay) -> Loaded {
    let path = locate(explicit);
    let Some(p) = path.clone() else {
        let (config, errors) = parse_with("", schemas, overlay);
        return Loaded { config, path: None, errors };
    };
    match std::fs::read_to_string(&p) {
        Ok(text) => {
            let (config, errors) = parse_with(&text, schemas, overlay);
            Loaded { config, path, errors }
        }
        Err(e) => {
            // The defaults, but still under the command-line overlay: a
            // `--color never` render of an unreadable config must stay plain.
            let (config, mut errors) = parse_with("", schemas, overlay);
            errors.push(ConfigError {
                path: String::new(),
                message: format!("cannot read: {e}"),
                line: None,
            });
            Loaded { config, path, errors }
        }
    }
}

/// Parse and resolve TOML text.
///
/// Every valid key takes effect; each invalid one is reported and its
/// built-in default used instead. Only text that is not TOML yields the
/// defaults wholesale, with the line of the syntax error (SPEC § 5).
#[must_use]
pub fn parse(text: &str, schemas: &[ModuleSchema]) -> (Config, Vec<ConfigError>) {
    parse_with(text, schemas, &Overlay::default())
}

/// [`parse`] with command-line overrides.
#[must_use]
pub fn parse_with(
    text: &str,
    schemas: &[ModuleSchema],
    overlay: &Overlay,
) -> (Config, Vec<ConfigError>) {
    let mut errors = Vec::new();
    let table = match toml::from_str::<toml::Table>(text) {
        Ok(table) => table,
        Err(e) => {
            // The whole file falls back to the defaults, under the same
            // command-line overrides as a good file would be: `preview
            // --color never` of a broken config must still be plain.
            let line = e.span().map(|s| line_of(text, s.start));
            errors.push(ConfigError { path: String::new(), message: e.message().to_owned(), line });
            toml::Table::new()
        }
    };
    let (config, more) = resolve_table(table, schemas, overlay);
    errors.extend(more);
    (config, errors)
}

/// [`parse`] of a file already read as a TOML table: what `setup` renders
/// its draft through on every edit (SPEC § 14), so an edit never round-trips
/// through text.
#[must_use]
pub fn parse_table(table: toml::Table, schemas: &[ModuleSchema]) -> (Config, Vec<ConfigError>) {
    resolve_table(table, schemas, &Overlay::default())
}

fn resolve_table(
    table: toml::Table,
    schemas: &[ModuleSchema],
    overlay: &Overlay,
) -> (Config, Vec<ConfigError>) {
    let mut errors = Vec::new();
    let mut raw = RawConfig::from_table(table, &mut errors);
    if overlay.preset.is_some() {
        // The preset's rows replace the file's, so their problems are moot,
        // under either array name, and so is a box only they could join.
        raw.preset = overlay.preset;
        raw.row.clear();
        raw.rows_replaced = true;
        errors.retain(|e| {
            !["row", "line"].iter().any(|key| {
                e.path == *key || e.path.strip_prefix(key).is_some_and(|rest| rest.starts_with('['))
            })
        });
    }
    raw.icons = overlay.icons.or(raw.icons);
    raw.theme = overlay.theme.clone().or(raw.theme);
    raw.color = overlay.color.or(raw.color);
    let config = resolve(&raw, schemas, &mut errors);
    (config, errors)
}

fn line_of(text: &str, byte: usize) -> usize {
    text.bytes().take(byte).filter(|&b| b == b'\n').count().saturating_add(1)
}

/// The TOML syntax error of `text` as `line N: message`, when it has one.
///
/// A syntax error is the one problem that makes a file unreadable rather
/// than fixable per key (SPEC § 5), and so the one a writing command must
/// refuse to paper over.
#[must_use]
pub fn syntax_error(text: &str) -> Option<String> {
    toml::from_str::<toml::Table>(text).err().map(|e| {
        e.span().map_or_else(
            || e.message().to_owned(),
            |s| format!("line {}: {}", line_of(text, s.start), e.message()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::presets::TopPreset;
    use crate::config::tests::schemas;
    use crate::config::{ColorChoice, Config};
    use crate::icons::IconSet;

    #[test]
    fn preset_overlay_drops_line_errors_with_the_lines() {
        let overlay = Overlay { preset: Some(TopPreset::Minimal), ..Default::default() };
        let (c, errs) = parse_with("[[line]]\nmodules = [3]\n", &schemas(), &overlay);
        assert_eq!(errs, Vec::new(), "the overlay replaces the lines, so their problems are moot");
        assert_eq!(c.rows.len(), 1);
        // cfg-09: nor does a box the file's rows joined read as unused, and
        // a file carrying both arrays loses both, with their "not both".
        for text in [
            "[box.a]\n[[row]]\nbox = \"a\"\nmodules = [\"path\"]\n",
            "[[row]]\nmodules = [\"path\"]\n[[line]]\nmodules = [3]\n",
        ] {
            let (_, errs) = parse_with(text, &schemas(), &overlay);
            assert_eq!(errs, Vec::new(), "{text}");
        }
        // A box's own mistakes are the file's and still reported.
        let (_, errs) = parse_with("[box.a]\nfill = 1\n", &schemas(), &overlay);
        let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["box.a.fill"]);
    }

    #[test]
    fn syntax_errors_carry_a_line_and_fall_back_wholesale() {
        let (c, errs) = parse("preset = \"minimal\"\n[frame\nstyle = 1", &schemas());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, Some(2));
        assert!(errs[0].to_string().starts_with("line 2: "));
        assert_eq!(c, Config::defaults(&schemas()), "not TOML: nothing can be trusted");
        // ... but the command line still is: `preview --color never --icons
        // ascii` of a broken file renders plain ascii (whole-stack review).
        let overlay = Overlay {
            color: Some(ColorChoice::Never),
            icons: Some(IconSet::Ascii),
            preset: Some(TopPreset::Compact),
            theme: Some("nord".into()),
        };
        let (c, errs) = parse_with("[frame\nstyle = 1", &schemas(), &overlay);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].line, Some(1));
        assert_eq!(c.color, ColorChoice::Never);
        assert_eq!(c.icons, IconSet::Ascii);
        assert_eq!(c.preset, TopPreset::Compact);
        assert_eq!(c.theme_name, "nord");
        let (c, errs) = parse("unknown_top = 1\npreset = \"minimal\"", &schemas());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "unknown_top");
        assert!(errs[0].message.contains("unknown key"), "{}", errs[0].message);
        assert_eq!(c.preset, TopPreset::Minimal, "the valid key next to it still counts");
    }
}
