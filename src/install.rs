//! `garnish install`: wiring garnish into Claude Code's `settings.json`.
//!
//! The settings file is JSON with many unrelated keys, so garnish only
//! touches the `statusLine` object (keeping any keys it does not own, such as
//! `hideVimModeIndicator`), backs the file up first, and writes atomically.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// What `install` should write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The settings file.
    pub settings: PathBuf,
    /// The `statusLine.command` value.
    pub command: String,
    /// `statusLine.refreshInterval` in seconds.
    pub refresh_interval: u64,
    /// `statusLine.padding`, when given.
    pub padding: Option<u64>,
}

/// The default Claude Code user settings file.
#[must_use]
pub fn default_settings_path() -> Option<PathBuf> {
    // No HOME, no default: never guess the current directory.
    crate::claude_settings::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

/// Whether an executable named `name` is on `PATH`.
#[must_use]
pub fn on_path(name: &str, path_env: Option<&std::ffi::OsStr>) -> bool {
    path_env.is_some_and(|p| {
        std::env::split_paths(p).any(|dir| {
            let candidate = dir.join(name);
            candidate.is_file() && is_executable(&candidate)
        })
    })
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_p: &Path) -> bool {
    true
}

/// Merge the plan into existing settings text, returning the new JSON text.
///
/// # Errors
/// When the existing text is not a JSON object.
pub fn merge(existing: &str, plan: &Plan) -> Result<String, String> {
    let existing = existing.strip_prefix('\u{feff}').unwrap_or(existing);
    let mut root: Map<String, Value> = if existing.trim().is_empty() {
        Map::new()
    } else {
        // The same two problems, worded as `doctor` words them, so every
        // caller can prefix the file's path once.
        match serde_json::from_str::<Value>(existing) {
            Ok(Value::Object(m)) => m,
            Ok(_) => return Err("not a JSON object".to_owned()),
            Err(e) => return Err(format!("not valid JSON: {e}")),
        }
    };
    let mut status = match root.remove("statusLine") {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    status.insert("type".into(), json!("command"));
    status.insert("command".into(), json!(plan.command));
    status.insert("refreshInterval".into(), json!(plan.refresh_interval));
    if let Some(p) = plan.padding {
        status.insert("padding".into(), json!(p));
    }
    root.insert("statusLine".into(), Value::Object(status));
    let mut text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// Read the settings file: `Ok(None)` when it does not exist, `Err` for any
/// other problem (a directory, unreadable), so a dry run reports exactly what
/// the real run would hit.
///
/// # Errors
/// Any I/O error other than "not found".
pub fn read_existing(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("reading {}: {e}", path.display())),
    }
}

/// Write a config file the way every garnish command does (SPEC § 5).
///
/// An existing file is refused without `force`, a file that does not parse
/// is never rewritten, and a replaced file is kept as a backup, whose path
/// comes back.
///
/// # Errors
/// [`Refusal::Exists`] without `force`, [`Refusal::Unparsable`] for a file
/// with a TOML syntax error, [`Refusal::Io`] naming the file.
pub fn write_config(target: &Path, text: &str, force: bool) -> Result<Option<PathBuf>, Refusal> {
    let existed = target.exists();
    if existed && !force {
        return Err(Refusal::Exists(target.to_path_buf()));
    }
    if existed {
        // A file that does not parse is never rewritten (SPEC § 5): the only
        // way past is fixing or moving it by hand. A file with bad values
        // parses, and is replaced under its backup.
        let current = std::fs::read_to_string(target)
            .map_err(|e| Refusal::Io(format!("reading {}: {e}", target.display())))?;
        if let Some(problem) = crate::config::syntax_error(&current) {
            return Err(Refusal::Unparsable { path: target.to_path_buf(), problem });
        }
    }
    replace_file(target, text, existed).map_err(Refusal::Io)
}

/// The line a command prints for a file it wrote: `wrote <path>`, with the
/// backup it kept when it replaced one.
#[must_use]
pub fn wrote_line(path: &Path, backup: Option<&Path>) -> String {
    backup.map_or_else(
        || format!("wrote {}", path.display()),
        |b| format!("wrote {} (backup: {})", path.display(), b.display()),
    )
}

/// Write `contents` over `target` the way every garnish command edits a
/// file it may not lose (SPEC § 5).
///
/// Through a symlink (the target is rewritten, the link stays), keeping the
/// old file's permissions, after a backup next to it that never overwrites
/// another, via a temp file in the same directory and a rename, so no
/// reader ever sees a partial file. `existed` says whether there is a file
/// to back up; the backup's path comes back when one was written.
///
/// # Errors
/// Any I/O failure, naming the file it hit.
pub fn replace_file(
    target: &Path,
    contents: &str,
    existed: bool,
) -> Result<Option<PathBuf>, String> {
    // Through the link whether or not its target exists yet: a dotfiles
    // link made before the file is filled in must stay a link.
    let target = follow_links(target)?;
    let target = std::fs::canonicalize(&target).unwrap_or(target);
    let backup = if existed { Some(write_backup(&target)?) } else { None };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    let name =
        target.file_name().map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let tmp = target.with_file_name(format!("{name}.tmp.{}", std::process::id()));
    let permissions = std::fs::metadata(&target).ok().map(|m| m.permissions());
    // The temp file is born with the old file's mode, so a 0600 settings
    // file is never readable by others even for a moment; a failed write
    // leaves nothing behind.
    let written = create_with(&tmp, permissions.as_ref())
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .map_err(|e| format!("writing {}: {e}", tmp.display()));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Some(p) = permissions {
        let _ = std::fs::set_permissions(&tmp, p);
    }
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("replacing {}: {e}", target.display())
    })?;
    Ok(backup)
}

/// Where a path leads once every symlink on it is followed, a dangling
/// link included (the file the bytes must go to). A relative link target
/// is taken from the link's directory; a loop (or a chain past 40 links)
/// is refused rather than written into.
fn follow_links(path: &Path) -> Result<PathBuf, String> {
    let is_link = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink());
    let mut here = path.to_path_buf();
    for _ in 0..40 {
        if !is_link(&here) {
            return Ok(here);
        }
        let link = std::fs::read_link(&here)
            .map_err(|e| format!("following the link {}: {e}", here.display()))?;
        here = if link.is_absolute() {
            link
        } else {
            here.parent().map_or_else(|| link.clone(), |dir| dir.join(&link))
        };
    }
    Err(format!("{}: too many levels of symbolic links", path.display()))
}

/// Create `path` afresh with `permissions` (the old file's) when known.
#[cfg(unix)]
fn create_with(
    path: &Path,
    permissions: Option<&std::fs::Permissions>,
) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    if let Some(p) = permissions {
        options.mode(p.mode());
    }
    options.open(path)
}

#[cfg(not(unix))]
fn create_with(
    path: &Path,
    _permissions: Option<&std::fs::Permissions>,
) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

/// Copy `target` to `<name>.bak-<epoch>[-n]` next to it, never clobbering
/// an existing backup. Uses the wall clock (not `GARNISH_NOW`).
fn write_backup(target: &Path) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = target
        .file_name()
        .map_or_else(|| "settings.json".to_owned(), |n| n.to_string_lossy().into_owned());
    for attempt in 0..1000_u32 {
        let suffix = if attempt == 0 { String::new() } else { format!("-{attempt}") };
        let path = target.with_file_name(format!("{name}.bak-{stamp}{suffix}"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                let bytes = std::fs::read(target)
                    .map_err(|e| format!("reading {}: {e}", target.display()))?;
                f.write_all(&bytes)
                    .map_err(|e| format!("backing up to {}: {e}", path.display()))?;
                if let Ok(meta) = std::fs::metadata(target) {
                    let _ = std::fs::set_permissions(&path, meta.permissions());
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("backing up to {}: {e}", path.display())),
        }
    }
    Err("too many backups with the same timestamp".to_owned())
}

/// What `garnish install` was asked for: the flags of the command, which the
/// `setup` install screen fills in with their defaults (SPEC § 14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `--settings`; the default user settings file when `None`.
    pub settings: Option<PathBuf>,
    /// `--refresh-interval` (seconds, at least 1).
    pub refresh_interval: u64,
    /// `--padding`.
    pub padding: Option<u64>,
    /// `--absolute`: write this binary's path instead of `garnish`.
    pub absolute: bool,
    /// Write the default config when none exists (`--no-config` clears it).
    pub write_config: bool,
    /// Write the bundled skills (`--no-skills` clears it).
    pub write_skills: bool,
    /// `--config` or `GARNISH_CONFIG`: where the config goes.
    pub config_path: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            settings: None,
            refresh_interval: 1,
            padding: None,
            absolute: false,
            write_config: true,
            write_skills: true,
            config_path: None,
        }
    }
}

/// Why a plan could not be made or applied (SPEC § 5): each is one line to
/// a person, and the CLI turns them into its quiet exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No home directory and no flag saying where a file goes: `flag` names
    /// the flag, `what` the file.
    NoHome {
        /// The flag to pass (`--settings <FILE>`).
        flag: &'static str,
        /// What the flag places (`settings.json is`).
        what: &'static str,
    },
    /// A file that does not parse, which is never rewritten.
    Unparsable {
        /// The file.
        path: PathBuf,
        /// The problem, as `doctor` words it.
        problem: String,
    },
    /// A file that exists where one is to be created, without `--force`.
    Exists(PathBuf),
    /// Anything the file system refused, naming the file.
    Io(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHome { flag, what } => {
                write!(f, "HOME is not set; pass {flag} to say where {what}")
            }
            Self::Exists(path) => write!(f, "{} exists; pass --force to overwrite", path.display()),
            Self::Unparsable { path, problem } => write!(
                f,
                "{}: {problem}; a file that does not parse is never rewritten, fix or move it first",
                path.display()
            ),
            Self::Io(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Refusal {}

/// The config half of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigStep {
    /// `--no-config`.
    Skipped,
    /// A config already exists there; `padding` is the value it would need
    /// to match a `--padding` the user gave, which the report notes.
    Exists {
        /// The file.
        path: PathBuf,
        /// `2 × --padding`, when that flag was given.
        padding: Option<u64>,
    },
    /// The annotated default file will be written there, seeded with
    /// `padding` when `--padding` was given.
    Write {
        /// The file.
        path: PathBuf,
        /// `2 × --padding`, when that flag was given.
        padding: Option<u64>,
    },
}

/// Everything `install` decides before it writes anything.
///
/// The settings text merged, the config and skills paths, the refusals
/// found. The CLI prints it (`--dry-run`) or applies it; the `setup` install
/// screen shows the same plan and applies it through the same code
/// (SPEC § 14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Steps {
    /// The `statusLine` object and where it goes.
    pub plan: Plan,
    /// The settings text on disk, when the file exists.
    pub existing: Option<String>,
    /// The settings text to write.
    pub merged: String,
    /// The config half.
    pub config: ConfigStep,
    /// Where the skills go, unless `--no-skills`.
    pub skills: Option<PathBuf>,
    /// `garnish` is on `PATH` (or the plan writes an absolute path), so the
    /// command written will be found.
    pub found: bool,
}

impl Steps {
    /// Decide everything `install` would do for `options`, writing nothing.
    ///
    /// # Errors
    /// A missing home, an unreadable or unparsable settings file, or a
    /// binary whose own path cannot be found (`--absolute`).
    pub fn plan(options: &Options) -> Result<Self, Refusal> {
        let command = if options.absolute {
            std::env::current_exe()
                .map_err(|e| Refusal::Io(format!("locating this binary: {e}")))?
                .display()
                .to_string()
        } else {
            "garnish".to_owned()
        };
        let Some(settings) = options.settings.clone().or_else(default_settings_path) else {
            return Err(Refusal::NoHome { flag: "--settings <FILE>", what: "settings.json is" });
        };
        let plan = Plan {
            settings,
            command,
            refresh_interval: options.refresh_interval.max(1),
            padding: options.padding,
        };
        let found = options.absolute || on_path("garnish", std::env::var_os("PATH").as_deref());
        let existing = read_existing(&plan.settings).map_err(Refusal::Io)?;
        let merged = merge(existing.as_deref().unwrap_or(""), &plan)
            .map_err(|problem| Refusal::Unparsable { path: plan.settings.clone(), problem })?;
        // The harness pads both sides, so the config mirrors
        // statusLine.padding doubled (SPEC § 2.1).
        let padding = options.padding.map(|p| p.saturating_mul(2));
        let config = if options.write_config {
            let target = options
                .config_path
                .clone()
                .or_else(|| crate::config::env_path(crate::config::CONFIG_ENV))
                .or_else(crate::config::default_path);
            let Some(path) = target else {
                return Err(Refusal::NoHome { flag: "--config <FILE>", what: "the config goes" });
            };
            if path.exists() {
                ConfigStep::Exists { path, padding }
            } else {
                ConfigStep::Write { path, padding }
            }
        } else {
            ConfigStep::Skipped
        };
        let skills = options.write_skills.then(|| crate::skills::default_dir(&plan.settings));
        Ok(Self { plan, existing, merged, config, skills, found })
    }

    /// Whether the settings file already carries exactly this plan.
    #[must_use]
    pub fn settings_up_to_date(&self) -> bool {
        self.existing.as_deref() == Some(self.merged.as_str())
    }

    /// Whether the settings file already names a `statusLine.command`: what
    /// the `setup` picker checks before offering the install screen.
    #[must_use]
    pub fn statusline_configured(&self) -> bool {
        self.existing.as_deref().is_some_and(|text| {
            serde_json::from_str::<Value>(text.strip_prefix('\u{feff}').unwrap_or(text))
                .ok()
                .and_then(|v| v.get("statusLine")?.get("command")?.as_str().map(str::to_owned))
                .is_some_and(|c| !c.is_empty())
        })
    }

    /// The `--dry-run` report: what would be written, the settings text
    /// included.
    #[must_use]
    pub fn dry_run(&self) -> Vec<String> {
        let mut lines = vec![format!("would write {}:", self.plan.settings.display())];
        lines.push(self.merged.trim_end().to_owned());
        match &self.config {
            ConfigStep::Write { path, padding } => {
                lines.push(format!(
                    "would write a default config to {}{}",
                    path.display(),
                    seeded(*padding)
                ));
            }
            ConfigStep::Exists { .. } | ConfigStep::Skipped => {}
        }
        if let Some(dir) = &self.skills {
            lines.push(format!(
                "would write {} skill(s) to {}",
                crate::skills::SKILLS.len(),
                dir.display()
            ));
        }
        lines
    }

    /// The advice a plan carries whether it is applied or not: the PATH
    /// warning, and the `padding` a config that already exists would need.
    #[must_use]
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if !self.found {
            notes.push(
                "warning: `garnish` is not on PATH; run `make install` first or use --absolute"
                    .to_owned(),
            );
        }
        if let ConfigStep::Exists { path, padding: Some(p) } = &self.config {
            notes.push(format!(
                "note: {} already exists; set `padding = {p}` in it to match statusLine.padding",
                path.display()
            ));
        }
        notes
    }

    /// Write everything the plan decided: the settings (with `install`'s
    /// backup), the default config when none exists, the skills last (the
    /// optional part, so a problem with them never leaves the settings
    /// updated and the config unwritten). What was written comes back, one
    /// line each; the advice is [`Steps::notes`], whether applied or not.
    ///
    /// # Errors
    /// The first I/O failure, naming the file; whatever was written before
    /// it stays.
    pub fn apply(&self) -> Result<Vec<String>, Refusal> {
        let mut lines = Vec::new();
        let settings = self.plan.settings.display();
        if self.settings_up_to_date() {
            lines.push(format!("{settings} already up to date"));
        } else {
            let backup = replace_file(&self.plan.settings, &self.merged, self.existing.is_some())
                .map_err(Refusal::Io)?;
            lines.push(backup.map_or_else(
                || format!("wrote {settings}"),
                |b| format!("updated {settings} (backup: {})", b.display()),
            ));
        }
        if let ConfigStep::Write { path, padding } = &self.config {
            let seed = padding.map_or_else(String::new, |p| format!("padding = {p}\n"));
            let (cfg, _) = crate::config::parse(&seed, &crate::modules::SCHEMAS);
            replace_file(path, &crate::docs::config_toml(&cfg, true), false)
                .map_err(Refusal::Io)?;
            lines.push(format!("wrote default config to {}{}", path.display(), seeded(*padding)));
        }
        if let Some(dir) = &self.skills {
            let report = crate::skills::install(dir)
                .map_err(|e| Refusal::Io(format!("writing skills to {}: {e}", dir.display())))?;
            lines.push(report.summary());
        }
        Ok(lines)
    }
}

/// The ` (padding = N)` tail of a config line.
fn seeded(padding: Option<u64>) -> String {
    padding.map_or_else(String::new, |p| format!(" (padding = {p})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(dir: &Path) -> Plan {
        Plan {
            settings: dir.join("settings.json"),
            command: "garnish".into(),
            refresh_interval: 1,
            padding: None,
        }
    }

    #[test]
    fn merge_keeps_unrelated_keys_and_statusline_extras() {
        let p = plan(Path::new("/x"));
        let existing = r#"{"theme":"dark","statusLine":{"type":"command","command":"old.sh","hideVimModeIndicator":true}}"#;
        let out = merge(existing, &p).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["statusLine"]["command"], "garnish");
        assert_eq!(v["statusLine"]["refreshInterval"], 1);
        assert_eq!(v["statusLine"]["hideVimModeIndicator"], true);
        assert!(v["statusLine"].get("padding").is_none());
        assert!(
            out.find("\"theme\"").unwrap() < out.find("\"statusLine\"").unwrap(),
            "key order kept: {out}"
        );
        let ordered = merge(r#"{"z":1,"a":2,"m":{"y":1,"b":2}}"#, &p).unwrap();
        let zi = ordered.find("\"z\"").unwrap();
        let ai = ordered.find("\"a\"").unwrap();
        let yi = ordered.find("\"y\"").unwrap();
        let bi = ordered.find("\"b\"").unwrap();
        assert!(zi < ai && yi < bi, "nested key order kept: {ordered}");
        let with_pad = Plan { padding: Some(2), ..p };
        let v: Value = serde_json::from_str(&merge("", &with_pad).unwrap()).unwrap();
        assert_eq!(v["statusLine"]["padding"], 2);
        assert!(merge("[1]", &with_pad).is_err());
        assert!(merge("{nope", &with_pad).is_err());
    }

    /// Options that touch the settings file alone: no config, no skills.
    fn settings_only(settings: &Path) -> Options {
        Options {
            settings: Some(settings.to_path_buf()),
            write_config: false,
            write_skills: false,
            ..Options::default()
        }
    }

    /// Plan and apply as `garnish install` does, returning the lines.
    fn install(settings: &Path) -> Vec<String> {
        Steps::plan(&settings_only(settings)).unwrap().apply().unwrap()
    }

    /// The `<name>.bak-*` files in `dir`, oldest name first.
    fn backups(dir: &Path, name: &str) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{name}.bak-")))
            .map(|e| e.path())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn apply_backs_up_writes_atomically_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let first = install(&settings);
        assert_eq!(first, vec![format!("wrote {}", settings.display())]);
        let text = std::fs::read_to_string(&settings).unwrap();
        assert!(text.contains("\"command\": \"garnish\""));
        let again = install(&settings);
        assert_eq!(again, vec![format!("{} already up to date", settings.display())]);
        assert_eq!(backups(dir.path(), "settings.json"), Vec::<PathBuf>::new());
        std::fs::write(&settings, r#"{"a":1}"#).unwrap();
        let third = install(&settings);
        assert!(third[0].starts_with("updated ") && third[0].contains("(backup: "), "{third:?}");
        let kept = backups(dir.path(), "settings.json");
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read_to_string(&kept[0]).unwrap(), r#"{"a":1}"#);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["statusLine"]["type"], "command");
        assert!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().contains(".tmp."))
        );
    }

    #[test]
    fn apply_keeps_permissions_backups_and_symlinks() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles").join("settings.json");
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, "\u{feff}{\"dot\":1}").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let first = install(&link);
        assert!(first[0].starts_with("updated "), "{first:?}");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "link kept");
        let text = std::fs::read_to_string(&real).unwrap();
        assert!(text.contains("\"dot\": 1") && text.contains("\"command\": \"garnish\""), "{text}");
        assert!(!text.starts_with('\u{feff}'));
        assert_eq!(std::fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
        // The backup sits next to the link target; compare canonical paths
        // because macOS temp dirs live under the `/var` → `/private/var` symlink.
        let real_dir = std::fs::canonicalize(real.parent().unwrap()).unwrap();
        let kept = backups(&real_dir, "settings.json");
        assert_eq!(kept.len(), 1, "{kept:?}");
        let backup = kept[0].clone();
        assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
        // a second change in the same second gets its own backup
        std::fs::write(&real, "{\"dot\":2}").unwrap();
        install(&link);
        let kept = backups(&real_dir, "settings.json");
        assert_eq!(kept.len(), 2, "{kept:?}");
        let b2 = kept.iter().find(|b| **b != backup).unwrap();
        assert!(std::fs::read_to_string(b2).unwrap().contains("\"dot\":2"));
        assert!(std::fs::read_to_string(&backup).unwrap().contains("\"dot\":1"));
        // read_existing distinguishes missing from unreadable
        assert_eq!(read_existing(&dir.path().join("nope.json")).unwrap(), None);
        assert!(read_existing(dir.path()).is_err());
    }

    /// `replace_file` is the one way a file is rewritten (SPEC § 5): a new
    /// file gets its directory and no backup, an existing one a backup with
    /// its permissions, and no temp file survives either way.
    #[test]
    fn replace_file_creates_or_backs_up_and_leaves_no_temp_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("deep").join("garnish.toml");
        assert_eq!(replace_file(&target, "a = 1\n", false).unwrap(), None);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "a = 1\n");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let backup = replace_file(&target, "a = 2\n", true).unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "a = 2\n");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "a = 1\n");
        assert!(backup.file_name().unwrap().to_string_lossy().starts_with("garnish.toml.bak-"));
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
        let names: Vec<String> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.contains(".tmp.")), "{names:?}");
        assert_eq!(names.len(), 2, "{names:?}");
        // A directory in the way is an error naming it, not a panic, and
        // the temp file does not survive the failed rename.
        let blocked = dir.path().join("blocked");
        std::fs::create_dir_all(blocked.join("garnish.toml")).unwrap();
        let err = replace_file(&blocked.join("garnish.toml"), "x", false).unwrap_err();
        assert!(err.contains("garnish.toml"), "{err}");
        let left: Vec<String> = std::fs::read_dir(&blocked)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["garnish.toml"], "no temp file left behind");
    }

    /// SPEC § 5: a rewrite goes through a symlink even before its target
    /// exists (a dotfiles link made ahead of the file), so the link stays
    /// a link and the bytes land where it points; a relative link resolves
    /// from the link's own directory.
    #[test]
    fn replace_file_fills_a_dangling_symlink_instead_of_replacing_it() {
        let dir = tempfile::tempdir().unwrap();
        let dotfiles = dir.path().join("dotfiles");
        std::fs::create_dir_all(&dotfiles).unwrap();
        let link = dir.path().join("config").join("garnish.toml");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(dotfiles.join("garnish.toml"), &link).unwrap();
        assert!(!link.exists(), "dangling to start with");
        assert_eq!(replace_file(&link, "a = 1\n", false).unwrap(), None);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "link kept");
        assert_eq!(std::fs::read_to_string(dotfiles.join("garnish.toml")).unwrap(), "a = 1\n");
        // Now it exists: the backup lands next to the target, the link stays.
        let backup = replace_file(&link, "a = 2\n", true).unwrap().unwrap();
        assert!(backup.starts_with(std::fs::canonicalize(&dotfiles).unwrap()), "{backup:?}");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "a = 2\n");
        // A relative link, and a chain of two.
        let rel = dir.path().join("config").join("rel.toml");
        std::os::unix::fs::symlink("../dotfiles/rel.toml", &rel).unwrap();
        let hop = dir.path().join("hop.toml");
        std::os::unix::fs::symlink(&rel, &hop).unwrap();
        assert_eq!(replace_file(&hop, "r = 1\n", false).unwrap(), None);
        assert_eq!(std::fs::read_to_string(dotfiles.join("rel.toml")).unwrap(), "r = 1\n");
        assert!(std::fs::symlink_metadata(&hop).unwrap().file_type().is_symlink());
        assert!(std::fs::symlink_metadata(&rel).unwrap().file_type().is_symlink());
        // A loop is refused, naming the file, and both links stay links.
        let a = dir.path().join("a.toml");
        let b = dir.path().join("b.toml");
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();
        let err = replace_file(&a, "x", false).unwrap_err();
        assert!(err.contains("a.toml") && err.contains("symbolic links"), "{err}");
        assert!(std::fs::symlink_metadata(&a).unwrap().file_type().is_symlink());
        assert!(std::fs::symlink_metadata(&b).unwrap().file_type().is_symlink());
    }

    #[test]
    fn path_lookup_finds_executables_only() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("garnish");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        assert!(!on_path("garnish", Some(dir.path().as_os_str())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(on_path("garnish", Some(dir.path().as_os_str())));
        }
        assert!(!on_path("garnish", None));
        assert!(!on_path("nothing-here", Some(dir.path().as_os_str())));
    }
}
