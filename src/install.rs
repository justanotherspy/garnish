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

/// Back the settings file up (if it exists) and write the merged text atomically.
///
/// A symlinked settings file is updated through the link (the target is
/// rewritten, the link stays), the new file keeps the old file's permissions,
/// and backups never overwrite each other ([`replace_file`]).
///
/// # Errors
/// Propagates I/O errors and invalid existing JSON.
pub fn apply(plan: &Plan) -> Result<Outcome, String> {
    let existing = read_existing(&plan.settings)?;
    let merged = merge(existing.as_deref().unwrap_or(""), plan)?;
    if existing.as_deref() == Some(merged.as_str()) {
        return Ok(Outcome { backup: None, changed: false });
    }
    let backup = replace_file(&plan.settings, &merged, existing.is_some())?;
    Ok(Outcome { backup, changed: true })
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

/// What `apply` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The backup file, when one was written.
    pub backup: Option<PathBuf>,
    /// Whether the settings file changed.
    pub changed: bool,
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

    #[test]
    fn apply_backs_up_writes_atomically_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = plan(dir.path());
        let first = apply(&p).unwrap();
        assert!(first.changed && first.backup.is_none());
        let text = std::fs::read_to_string(&p.settings).unwrap();
        assert!(text.contains("\"command\": \"garnish\""));
        let again = apply(&p).unwrap();
        assert!(!again.changed && again.backup.is_none());
        std::fs::write(&p.settings, r#"{"a":1}"#).unwrap();
        let third = apply(&p).unwrap();
        assert!(third.changed);
        let backup = third.backup.unwrap();
        assert_eq!(std::fs::read_to_string(backup).unwrap(), r#"{"a":1}"#);
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&p.settings).unwrap()).unwrap();
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
        let p = Plan { settings: link.clone(), ..plan(dir.path()) };
        let first = apply(&p).unwrap();
        assert!(first.changed);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "link kept");
        let text = std::fs::read_to_string(&real).unwrap();
        assert!(text.contains("\"dot\": 1") && text.contains("\"command\": \"garnish\""), "{text}");
        assert!(!text.starts_with('\u{feff}'));
        assert_eq!(std::fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
        let backup = first.backup.unwrap();
        // The backup sits next to the link target; compare canonical paths
        // because macOS temp dirs live under the `/var` → `/private/var` symlink.
        let real_dir = std::fs::canonicalize(real.parent().unwrap()).unwrap();
        assert!(backup.starts_with(&real_dir), "{backup:?} not under {real_dir:?}");
        assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
        // a second change in the same second gets its own backup
        std::fs::write(&real, "{\"dot\":2}").unwrap();
        let second = apply(&p).unwrap();
        let b2 = second.backup.unwrap();
        assert_ne!(b2, backup);
        assert!(std::fs::read_to_string(&b2).unwrap().contains("\"dot\":2"));
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
