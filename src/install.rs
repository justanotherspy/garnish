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
pub fn default_settings_path() -> PathBuf {
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".claude")
        .join("settings.json")
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
        match serde_json::from_str::<Value>(existing) {
            Ok(Value::Object(m)) => m,
            Ok(_) => return Err("settings file is not a JSON object".to_owned()),
            Err(e) => return Err(format!("settings file is not valid JSON: {e}")),
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
/// and backups never overwrite each other.
///
/// # Errors
/// Propagates I/O errors and invalid existing JSON.
pub fn apply(plan: &Plan) -> Result<Outcome, String> {
    let existing = read_existing(&plan.settings)?;
    let merged = merge(existing.as_deref().unwrap_or(""), plan)?;
    if existing.as_deref() == Some(merged.as_str()) {
        return Ok(Outcome { backup: None, changed: false });
    }
    let target = if existing.is_some() {
        std::fs::canonicalize(&plan.settings).unwrap_or_else(|_| plan.settings.clone())
    } else {
        plan.settings.clone()
    };
    let backup = if existing.is_some() { Some(write_backup(&target)?) } else { None };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    let tmp = target.with_extension(format!("json.tmp.{}", std::process::id()));
    std::fs::write(&tmp, &merged).map_err(|e| format!("writing {}: {e}", tmp.display()))?;
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, &target).map_err(|e| format!("replacing {}: {e}", target.display()))?;
    Ok(Outcome { backup, changed: true })
}

/// Copy `target` to `settings.json.bak-<epoch>[-n]`, never clobbering an
/// existing backup. Uses the wall clock (not `GARNISH_NOW`).
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
        assert!(backup.starts_with(real.parent().unwrap()));
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
