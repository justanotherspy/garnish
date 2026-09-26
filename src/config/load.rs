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

/// The config file named explicitly: `--config` (`flag`), else an
/// absolute `GARNISH_CONFIG`; `None` when neither names one.
///
/// A relative `GARNISH_CONFIG` is ignored, as a relative `XDG_*` base is: it
/// would name a file in whatever directory the tick runs in, the session's
/// repository (verification of 2026-09-26: Claude Code passes a settings
/// `env` value unexpanded, so `"~/g.toml"` made a checkout's own `~/g.toml`
/// the config).
#[must_use]
pub fn explicit(flag: Option<&Path>) -> Option<PathBuf> {
    flag.map(Path::to_path_buf).or_else(|| env_path(CONFIG_ENV).filter(|p| p.is_absolute()))
}

/// [`explicit`] for a command run by hand (SPEC § 4).
///
/// A relative `GARNISH_CONFIG` is refused rather than ignored, so the
/// person hears why. Inside a Claude Code session the environment carries
/// the `env` blocks of every settings file Claude Code read, a checkout's
/// included, so a `GARNISH_CONFIG` there counts only when the person's own
/// settings set that very value (`own_env_sets`); anywhere, one that the
/// current directory's checkout files set is a [`Checkout`] (verification of
/// 2026-09-26: matching the checkout's files alone missed a session in a
/// subdirectory, a non-string value and a file serde refuses).
///
/// # Errors
/// That [`Checkout`].
pub fn hand_explicit(flag: Option<&Path>) -> Result<Option<PathBuf>, Checkout> {
    if flag.is_some() {
        return Ok(explicit(flag));
    }
    let Some(value) = env_path(CONFIG_ENV) else { return Ok(None) };
    let refused = |settings| Err(Checkout { settings, key: "GARNISH_CONFIG", path: value.clone() });
    if value.is_relative() || (in_claude_code() && !own_env_sets(CONFIG_ENV, value.as_os_str())) {
        return refused(None);
    }
    let named = |keys: &crate::claude_settings::FileKeys| {
        keys.env(CONFIG_ENV).is_some_and(|set| Path::new(set) == value)
    };
    match chain_files().into_iter().find(|file| !file.own && named(&file.keys)) {
        Some(file) => refused(Some(file.path)),
        None => Ok(Some(value)),
    }
}

/// Whether garnish runs inside a Claude Code session, which marks every
/// process it starts with `CLAUDECODE` (a settings `env` block can blank a
/// variable but not remove it, so presence is the test).
fn in_claude_code() -> bool {
    std::env::var_os("CLAUDECODE").is_some()
}

/// Whether the person's own settings set the variable `name` to `value` in
/// their `env` block ([`own_settings_files`]).
fn own_env_sets(name: &str, value: &std::ffi::OsStr) -> bool {
    use crate::claude_settings as cs;
    own_settings_files().iter().any(|file| {
        match cs::read_file_up_to(file, cs::MAX_COMMAND_SETTINGS_BYTES) {
            cs::FileState::Keys(keys) => {
                keys.env(name).is_some_and(|set| std::ffi::OsStr::new(set) == value)
            }
            _ => false,
        }
    })
}

/// Most entries of the managed drop-in directory garnish looks at: an
/// organisation's own directory, bounded only against a runaway one.
const MAX_DROP_IN_ENTRIES: usize = 4096;

/// The `managed-settings.d/*.json` drop-ins beside the managed file
/// `managed`, which Claude Code applies on top of it, in name order (the
/// last one wins a key it shares with another); a hidden one (`.x.json`)
/// is switched off and only a file or a link counts, as Claude Code has it
/// (verification of 2026-09-26, 2.1.283).
fn drop_ins(managed: &Path) -> Vec<PathBuf> {
    let shown =
        |path: &Path| path.file_name().is_some_and(|name| !name.to_string_lossy().starts_with('.'));
    let mut files: Vec<PathBuf> = managed
        .parent()
        .and_then(|dir| std::fs::read_dir(dir.join("managed-settings.d")).ok())
        .map_or_else(Vec::new, |entries| {
            entries
                .take(MAX_DROP_IN_ENTRIES)
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file() || t.is_symlink()))
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json") && shown(path))
                .collect()
        });
    files.sort();
    files
}

/// The settings files that are the person's (or their organisation's) own
/// wherever garnish runs: the managed file the platform names with its
/// drop-ins, and the user file; never a file a variable names, which the
/// session may have from a checkout.
fn own_settings_files() -> Vec<PathBuf> {
    use crate::claude_settings as cs;
    let user = cs::user_dir(cs::home_dir().as_deref()).map(|dir| dir.join("settings.json"));
    own_settings_files_in(&cs::platform_managed_settings(), user)
}

/// [`own_settings_files`] for an explicit managed file and user file.
fn own_settings_files_in(managed: &Path, user: Option<PathBuf>) -> Vec<PathBuf> {
    std::iter::once(managed.to_path_buf()).chain(drop_ins(managed)).chain(user).collect()
}

/// The managed settings file for a command run by hand.
///
/// The hook's (`claude_settings::managed_settings_path`), unless
/// inside a Claude Code session the person's own settings do not set it,
/// in which case the platform's; a checkout's `env` block could have
/// pointed it at the checkout's own file. `doctor` shows the chain it
/// starts.
#[must_use]
pub fn hand_managed() -> Option<PathBuf> {
    use crate::claude_settings as cs;
    let hook = std::env::var_os(cs::MANAGED_SETTINGS_ENV);
    // An empty hook (no managed file) is left alone: it can only drop a
    // file from the chain, never add one.
    match hook.as_deref() {
        Some(value)
            if !value.is_empty()
                && in_claude_code()
                && !own_env_sets(cs::MANAGED_SETTINGS_ENV, value) =>
        {
            Some(cs::platform_managed_settings())
        }
        _ => cs::managed_settings_path(),
    }
}

/// A config garnish does not follow for where its name came from.
///
/// A checkout's `statusLine.command` or `env` block, a `--settings` file
/// elsewhere, a `GARNISH_CONFIG` none of the person's own settings set
/// inside a Claude Code session, or a relative `GARNISH_CONFIG`. garnish
/// never reads or writes a file a repository nobody here may have built
/// chooses (CLAUDE.md, "The repository is not the user's file"), so it is
/// refused like [`WriteTarget::Unresolved`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    /// The settings file, when one is known to name it.
    pub settings: Option<PathBuf>,
    /// The key naming it: `statusLine.command`, `env.GARNISH_CONFIG` or
    /// `GARNISH_CONFIG`.
    pub key: &'static str,
    /// The file it names.
    pub path: PathBuf,
}

/// Locate the config file: explicit path > `GARNISH_CONFIG` > XDG > `~/.garnish.toml`.
#[must_use]
pub fn locate(flag: Option<&Path>) -> Option<PathBuf> {
    explicit(flag).or_else(lookup)
}

/// [`locate`] without the explicit ones: the XDG file, else
/// `~/.garnish.toml`, whichever exists.
#[must_use]
pub fn lookup() -> Option<PathBuf> {
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

/// Where a command that writes a config writes it ([`write_target`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteTarget {
    /// This file.
    File(PathBuf),
    /// Nothing names the file, and there is no home directory to put it
    /// under (SPEC § 5: never guess the current directory).
    NoHome,
    /// The settings file `settings` names the config by a value that
    /// stands for no one file garnish can find (a relative path, or an
    /// expansion it does not follow): `word`, as written, under `key`.
    Unresolved {
        /// The settings file.
        settings: PathBuf,
        /// The key naming it: `statusLine.command` (its `--config`) or
        /// `env.GARNISH_CONFIG`.
        key: &'static str,
        /// The value, as the file spells it.
        word: String,
    },
    /// A settings file that is not the person's own names the config.
    Checkout(Checkout),
}

/// Which `statusLine.command` a config target follows ([`write_target`],
/// [`read_target`]).
#[derive(Debug, Clone, Copy)]
pub enum CommandFrom<'a> {
    /// The one Claude Code runs from the current directory: the first file
    /// of the settings chain that sets it (managed > local > project >
    /// user, a file Claude Code rejects skipped), as `doctor` shows it.
    /// A `--config` it passes from a checkout's own files is
    /// [`WriteTarget::Checkout`].
    Chain,
    /// This command, read from this settings file: `install`, which
    /// rewrites that file and has read all of it already.
    Given {
        /// The settings file.
        settings: &'a Path,
        /// Its `statusLine.command`, if it has one.
        command: Option<&'a str>,
        /// Its `env.GARNISH_CONFIG`, if it sets one.
        env: Option<&'a str>,
    },
}

/// The file a command that writes a config writes (SPEC § 4).
///
/// The one named explicitly (`flag`, else `GARNISH_CONFIG`); else the one
/// the garnish `statusLine.command` of `from` passes with `--config`,
/// since that is the file its ticks read; else [`locate`]'s; else
/// [`default_path`]. `config init`, `config path` and `setup` go through
/// here with [`CommandFrom::Chain`], `install`'s default config with
/// [`CommandFrom::Given`].
///
/// A default file written while the command names another would never be
/// read, and one written at the default path while `~/.garnish.toml` is
/// the config would be preferred to it by [`locate`]: either way the
/// user's config would stop applying without a word.
#[must_use]
pub fn write_target(flag: Option<&Path>, from: CommandFrom<'_>) -> WriteTarget {
    match hand_explicit(flag) {
        Err(checkout) => return WriteTarget::Checkout(checkout),
        Ok(Some(p)) => return WriteTarget::File(p),
        Ok(None) => {}
    }
    command_target(from).unwrap_or_else(|| {
        lookup().or_else(default_path).map_or(WriteTarget::NoHome, WriteTarget::File)
    })
}

/// The config a command run by hand reads for the person running it
/// (`config check`, `config show`, `preview`, `doctor`): [`read_target`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadTarget {
    /// This file (when it cannot be read, [`load`] says so).
    File(PathBuf),
    /// No file: the built-in defaults.
    Defaults,
    /// As [`WriteTarget::Unresolved`].
    Unresolved {
        /// The settings file.
        settings: PathBuf,
        /// The key naming it.
        key: &'static str,
        /// The value, as the file spells it.
        word: String,
    },
    /// As [`WriteTarget::Checkout`].
    Checkout(Checkout),
}

/// The config a command run by hand reads (SPEC § 4).
///
/// [`write_target`]'s order without its default path, following the
/// command of [`CommandFrom::Chain`], so `config check`, `config show`,
/// `preview` and `doctor` look at the file `config path` prints, the one
/// the status line's ticks read here. The tick itself and its workers use
/// [`locate`] alone: the harness hands the tick the command's `--config`,
/// and the tick passes it on, so neither reads the settings file.
#[must_use]
pub fn read_target(flag: Option<&Path>) -> ReadTarget {
    let target = match hand_explicit(flag) {
        Err(checkout) => return ReadTarget::Checkout(checkout),
        Ok(Some(p)) => return ReadTarget::File(p),
        Ok(None) => command_target(CommandFrom::Chain),
    };
    match target {
        Some(WriteTarget::File(p)) => ReadTarget::File(p),
        Some(WriteTarget::Unresolved { settings, key, word }) => {
            ReadTarget::Unresolved { settings, key, word }
        }
        Some(WriteTarget::Checkout(checkout)) => ReadTarget::Checkout(checkout),
        Some(WriteTarget::NoHome) | None => lookup().map_or(ReadTarget::Defaults, ReadTarget::File),
    }
}

/// One file of the current directory's settings chain as a command run by
/// hand reads it ([`chain_files`]).
struct ChainFile {
    /// `managed`, `local`, `project` or `user`.
    label: &'static str,
    /// The file.
    path: PathBuf,
    /// What it sets.
    keys: crate::claude_settings::FileKeys,
    /// The person's own: the managed file ([`hand_managed`], unless a
    /// checkout's `env` block here named it) or one in their settings
    /// directory ([`users`]); any other local or project file is a
    /// checkout's.
    own: bool,
}

/// The managed layer of the settings chain for a command run by hand.
///
/// Labelled as `doctor` shows it: the managed file ([`hand_managed`],
/// `managed`) and the drop-ins beside it (`drop-in`), highest precedence
/// first (the last drop-in by name, then the others, then the file). The
/// hook's file stands in for the platform's, drop-ins and all, which is
/// also how the tests reach this layer.
#[must_use]
pub fn managed_layer() -> Vec<(&'static str, PathBuf)> {
    hand_managed().map_or_else(Vec::new, layer_of)
}

/// [`managed_layer`] for the managed file `managed`.
fn layer_of(managed: PathBuf) -> Vec<(&'static str, PathBuf)> {
    let mut layer: Vec<_> = drop_ins(&managed).into_iter().map(|file| ("drop-in", file)).collect();
    layer.reverse();
    layer.push(("managed", managed));
    layer
}

/// The settings chain of the current directory, each file read whole, as
/// Claude Code reads it (the cap on a settings file protects the tick, and
/// only a command run by hand is here: final review of 2026-09-25), a file
/// Claude Code rejects left out.
fn chain_files() -> Vec<ChainFile> {
    chain_files_rewriting(None)
}

/// [`chain_files`] for `install`, which rewrites `rewritten`: a file that
/// may lie outside the current directory's chain, and whose `env` block
/// counts for [`demote_hooked`] as the chain's do (a second verification of
/// 2026-09-26: `install --settings` a checkout's file, run from elsewhere,
/// wrote where the managed file it pointed the hook at said).
fn chain_files_rewriting(rewritten: Option<&Path>) -> Vec<ChainFile> {
    use crate::claude_settings as cs;
    let home = cs::home_dir();
    let project = std::env::current_dir().ok();
    let user = cs::user_dir(home.as_deref());
    let managed = managed_layer();
    let base = managed.last().map(|(_, file)| file.clone());
    let chain =
        managed.into_iter().chain(cs::settings_chain(None, project.as_deref(), user.as_deref()));
    let read = |(label, path): (&'static str, PathBuf)| match cs::read_file_up_to(
        &path,
        cs::MAX_COMMAND_SETTINGS_BYTES,
    ) {
        cs::FileState::Keys(keys) if cs::rejected(label, &keys).is_none() => {
            let own = !matches!(label, "local" | "project")
                || users(&path, user.as_deref(), home.as_deref());
            Some(ChainFile { label, path, keys, own })
        }
        _ => None,
    };
    let mut files: Vec<ChainFile> = chain.filter_map(read).collect();
    let extra = rewritten
        .filter(|path| !files.iter().any(|file| file.path == *path))
        .and_then(|path| read(("project", path.to_path_buf())));
    let counted = files.len();
    files.extend(extra);
    let hook = std::env::var_os(cs::MANAGED_SETTINGS_ENV);
    let platform = cs::platform_managed_settings();
    demote_hooked(&mut files, hook.as_deref(), base.as_deref(), &platform);
    files.truncate(counted);
    files
}

/// A managed layer that a checkout's `env` block named through the hook is
/// the checkout's, drop-ins and all: when the hook is in use at all
/// (`managed`, the file the layer ends in, is the hook's; inside a session
/// `hand_managed` puts the platform's in its place), it names a file other
/// than the platform's own `platform`, and a file that is not the person's
/// own sets it.
fn demote_hooked(
    files: &mut [ChainFile],
    hook: Option<&std::ffi::OsStr>,
    managed: Option<&Path>,
    platform: &Path,
) {
    use crate::claude_settings as cs;
    let Some(hook) = hook.filter(|hook| !hook.is_empty()) else { return };
    if managed != Some(Path::new(hook)) || Path::new(hook) == platform {
        return;
    }
    let names_hook = |file: &ChainFile| {
        file.keys.env(cs::MANAGED_SETTINGS_ENV).is_some_and(|set| std::ffi::OsStr::new(set) == hook)
    };
    if files.iter().any(|file| !file.own && names_hook(file)) {
        for file in files.iter_mut().filter(|file| cs::MANAGED_LABELS.contains(&file.label)) {
            file.own = false;
        }
    }
}

/// The config the garnish `statusLine.command` of `from` has its ticks
/// read, as a [`WriteTarget::File`], [`WriteTarget::Unresolved`] or
/// [`WriteTarget::Checkout`]: the one it passes with `--config`, else the
/// `GARNISH_CONFIG` a settings `env` block gives it (Claude Code copies the
/// block into the command's environment); `None` when there is no such
/// command or neither names one.
///
/// Through the chain it is the command Claude Code runs here or, when that
/// one runs another program, the person's own garnish command, which still
/// names the config their status line reads elsewhere; its `env` value is
/// the first the chain sets, as Claude Code's precedence has it (the
/// person's own files alone in the second case, since no tick runs here).
/// For `install` the `env` value is the managed layer's, else the one the
/// file it rewrites sets ([`installed_env_target`]).
fn command_target(from: CommandFrom<'_>) -> Option<WriteTarget> {
    use crate::claude_settings as cs;
    let home = cs::home_dir();
    match from {
        CommandFrom::Given { settings, command, env } => {
            let user = cs::user_dir(home.as_deref());
            let own = users(settings, user.as_deref(), home.as_deref());
            command
                .and_then(|command| flag_target(settings, command, own, home.as_deref()))
                .or_else(|| installed_env_target(settings, env))
        }
        CommandFrom::Chain => {
            let files = chain_files();
            let garnish = |file: &ChainFile| {
                file.keys
                    .status_line_command
                    .as_deref()
                    .is_some_and(|command| crate::install::runs_garnish(command, home.as_deref()))
            };
            let runs = files.iter().find(|file| file.keys.status_line_command.is_some())?;
            let here = garnish(runs);
            let file =
                if here { runs } else { files.iter().find(|file| file.own && garnish(file))? };
            let command = file.keys.status_line_command.as_deref()?;
            flag_target(&file.path, command, file.own, home.as_deref()).or_else(|| {
                let (file, value) = files
                    .iter()
                    .filter(|file| here || file.own)
                    .find_map(|file| file.keys.env(CONFIG_ENV).map(|value| (file, value)))?;
                env_target(&file.path, file.own, value)
            })
        }
    }
}

/// The `GARNISH_CONFIG` a command `install` writes into `settings` gets
/// from an `env` block; `None` when no block sets one.
///
/// The managed layer's value wins, since Claude Code puts that layer above
/// every other file; else `env`, the one `settings` itself sets.
/// `install`'s default config and `setup --install`'s note follow it
/// (verification of 2026-09-26: a managed `env` value was the ticks'
/// config while `install` wrote the lookup's). The layer is read as the
/// chain reads it, so a managed file a checkout named through the hook is
/// the checkout's here too (a second verification: `install` wrote where
/// such a file pointed while every other command refused).
#[must_use]
pub fn installed_env_target(settings: &Path, env: Option<&str>) -> Option<WriteTarget> {
    use crate::claude_settings as cs;
    let home = cs::home_dir();
    let managed = chain_files_rewriting(Some(settings)).into_iter().find_map(|file| {
        let managed = cs::MANAGED_LABELS.contains(&file.label);
        let value = file.keys.env(CONFIG_ENV).filter(|_| managed);
        value.map(str::to_owned).map(|value| (file, value))
    });
    if let Some((file, value)) = managed {
        env_target(&file.path, file.own, &value)
    } else {
        let user = cs::user_dir(home.as_deref());
        let own = users(settings, user.as_deref(), home.as_deref());
        env.and_then(|value| env_target(settings, own, value))
    }
}

/// The `--config` of `command`, from the settings file `settings`, which is
/// the person's own when `own`.
fn flag_target(
    settings: &Path,
    command: &str,
    own: bool,
    home: Option<&Path>,
) -> Option<WriteTarget> {
    let settings = settings.to_path_buf();
    match crate::install::command_config(command, home)? {
        crate::install::CommandConfig::File(path) if !own => {
            let settings = Some(settings);
            Some(WriteTarget::Checkout(Checkout { settings, key: "statusLine.command", path }))
        }
        crate::install::CommandConfig::File(p) => Some(WriteTarget::File(p)),
        crate::install::CommandConfig::Unresolved(word) => {
            Some(WriteTarget::Unresolved { settings, key: "statusLine.command", word })
        }
    }
}

/// The `GARNISH_CONFIG` a settings file's `env` block sets (`value`, as
/// written: Claude Code expands nothing in it), from `settings`, the
/// person's own when `own`; `None` for an empty one, which is unset.
fn env_target(settings: &Path, own: bool, value: &str) -> Option<WriteTarget> {
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    let settings = settings.to_path_buf();
    Some(if own && path.is_absolute() {
        WriteTarget::File(path)
    } else if own {
        WriteTarget::Unresolved { settings, key: "env.GARNISH_CONFIG", word: value.to_owned() }
    } else {
        let settings = Some(settings);
        WriteTarget::Checkout(Checkout { settings, key: "env.GARNISH_CONFIG", path })
    })
}

/// Whether a settings file is one of the person's own: directly in their
/// settings directory `user` or in `~/.claude` (a session started in the
/// home directory reads their files as its project's), by its own path or
/// the one it links to. Anything else is a checkout's, or unknown.
fn users(file: &Path, user: Option<&Path>, home: Option<&Path>) -> bool {
    let canonical = |p: &Path| std::fs::canonicalize(p).ok();
    let dirs: Vec<PathBuf> = [user.map(Path::to_path_buf), home.map(|h| h.join(".claude"))]
        .into_iter()
        .flatten()
        .flat_map(|dir| [canonical(&dir), Some(dir)])
        .flatten()
        .collect();
    let parents = [
        file.parent().map(Path::to_path_buf),
        file.parent().and_then(canonical),
        canonical(file).and_then(|f| f.parent().map(Path::to_path_buf)),
    ];
    parents.into_iter().flatten().any(|parent| dirs.contains(&parent))
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
    load_path(locate(explicit), schemas, overlay)
}

/// [`load`] of exactly `path`, never locating one (so never reading
/// `GARNISH_CONFIG`): the built-in defaults when it is `None`. `doctor`
/// loads a refused config's stand-in this way.
#[must_use]
pub fn load_exactly(path: Option<&Path>, schemas: &[ModuleSchema]) -> Loaded {
    load_path(path.map(Path::to_path_buf), schemas, &Overlay::default())
}

/// [`load_with`] once the file is known.
fn load_path(path: Option<PathBuf>, schemas: &[ModuleSchema], overlay: &Overlay) -> Loaded {
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

    fn chain_file(label: &'static str, json: &str, own: bool) -> ChainFile {
        let keys = crate::claude_settings::parse_settings_json(json).unwrap();
        ChainFile { label, path: PathBuf::from(format!("/{label}.json")), keys, own }
    }

    /// Verification of 2026-09-26: the managed file the hook names is the
    /// checkout's only when the hook is in use, names a file other than the
    /// platform's, and a file that is not the person's own sets it.
    #[test]
    fn a_hooked_managed_file_is_a_checkouts_only_when_a_checkout_names_it() {
        let org = Path::new("/org.json");
        let hook = org.as_os_str();
        let platform = Path::new("/etc/claude-code/managed-settings.json");
        // The files of the chain when the checkout (else the user file)
        // points the hook at `named`.
        let files = |named: &Path, checkout_names: bool| {
            let names = serde_json::json!({"env": {"GARNISH_MANAGED_SETTINGS": named}});
            let names = names.to_string();
            vec![
                chain_file("drop-in", "{}", true),
                chain_file("managed", "{}", true),
                chain_file("project", if checkout_names { &names } else { "{}" }, false),
                chain_file("user", if checkout_names { "{}" } else { &names }, true),
            ]
        };
        let managed_own = |hook: Option<&std::ffi::OsStr>, managed: &Path, checkout: bool| {
            let mut files = files(hook.map_or(org, Path::new), checkout);
            demote_hooked(&mut files, hook, Some(managed), platform);
            let layer = |f: &&ChainFile| crate::claude_settings::MANAGED_LABELS.contains(&f.label);
            let owns: Vec<bool> = files.iter().filter(layer).map(|f| f.own).collect();
            assert!(owns.iter().all(|own| *own == owns[0]), "the layer is demoted whole");
            owns[0]
        };
        assert!(!managed_own(Some(hook), org, true));
        assert!(managed_own(Some(hook), org, false), "the person's own names it");
        assert!(managed_own(None, org, true), "no hook");
        assert!(managed_own(Some(std::ffi::OsStr::new("")), org, true));
        assert!(managed_own(Some(hook), platform, true), "the session put the platform's in place");
        // A checkout that points the hook at the platform's own file names
        // the file every session reads anyway.
        let platform_hook = platform.as_os_str();
        assert!(managed_own(Some(platform_hook), platform, true), "the platform's is never theirs");
    }

    /// Verification of 2026-09-26: the managed layer's drop-ins are every
    /// `*.json` beside the managed file but a hidden one, in name order,
    /// however many and in whatever order the directory lists them.
    #[test]
    fn the_own_settings_files_are_the_managed_file_its_drop_ins_and_the_user_file() {
        let dir = tempfile::tempdir().unwrap();
        let managed = dir.path().join("managed-settings.json");
        let drop_dir = dir.path().join("managed-settings.d");
        std::fs::create_dir(&drop_dir).unwrap();
        for i in 0..400 {
            std::fs::write(drop_dir.join(format!("x{i:03}.json")), "{}").unwrap();
        }
        std::fs::write(drop_dir.join("10-garnish.json"), "{}").unwrap();
        std::fs::write(drop_dir.join("notes.txt"), "").unwrap();
        std::fs::write(drop_dir.join(".off.json"), "{}").unwrap();
        // Claude Code lists files and links only.
        std::fs::create_dir(drop_dir.join("30-dir.json")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(drop_dir.join("10-garnish.json"), drop_dir.join("20-link.json"))
            .unwrap();
        let links = usize::from(cfg!(unix));
        let user = dir.path().join("settings.json");
        let files = own_settings_files_in(&managed, Some(user.clone()));
        assert_eq!(files.len(), 403 + links, "every drop-in, and no other file");
        assert_eq!(files.first(), Some(&managed));
        assert_eq!(files.get(1), Some(&drop_dir.join("10-garnish.json")), "name order");
        assert_eq!(files.get(401 + links), Some(&drop_dir.join("x399.json")));
        assert_eq!(files.last(), Some(&user));
        let alone = dir.path().join("elsewhere/managed-settings.json");
        assert_eq!(own_settings_files_in(&alone, None), std::slice::from_ref(&alone));
        // The chain's managed layer puts them highest precedence first: the
        // last by name wins, the managed file loses to every drop-in; a file
        // with no drop-ins beside it is the layer alone.
        let layer = layer_of(managed.clone());
        assert_eq!(layer.len(), 402 + links);
        assert_eq!(layer.first(), Some(&("drop-in", drop_dir.join("x399.json"))));
        assert_eq!(layer.get(400 + links), Some(&("drop-in", drop_dir.join("10-garnish.json"))));
        assert_eq!(layer.last(), Some(&("managed", managed)));
        assert_eq!(layer_of(alone.clone()), [("managed", alone)]);
    }

    /// Verification of 2026-09-26: an empty `GARNISH_CONFIG` names nothing,
    /// as for the tick, whoever's file sets it; a relative one of the
    /// person's own is unresolved under its key.
    #[test]
    fn an_env_value_names_a_file_only_when_it_is_the_persons_and_absolute() {
        let settings = Path::new("/home/u/.claude/settings.json");
        assert_eq!(env_target(settings, true, ""), None);
        assert_eq!(env_target(settings, false, ""), None);
        assert_eq!(
            env_target(settings, true, "/c.toml"),
            Some(WriteTarget::File("/c.toml".into()))
        );
        assert!(matches!(
            env_target(settings, true, "~/c.toml"),
            Some(WriteTarget::Unresolved { key: "env.GARNISH_CONFIG", .. })
        ));
        assert!(matches!(
            env_target(settings, false, "/c.toml"),
            Some(WriteTarget::Checkout(Checkout { key: "env.GARNISH_CONFIG", .. }))
        ));
    }
}
