//! The bundled Claude Code skills (SPEC § 13).
//!
//! The sources are `skills/<name>/SKILL.md` in the repository, embedded here
//! so a `cargo install` has them; `garnish install` and `garnish skills
//! install` write them to `~/.claude/skills/<name>/SKILL.md`.

use std::path::{Path, PathBuf};

/// The skills, `(name, SKILL.md text)`.
pub const SKILLS: [(&str, &str); 3] = [
    ("garnish-statusline", include_str!("../skills/garnish-statusline/SKILL.md")),
    ("garnish-feedback", include_str!("../skills/garnish-feedback/SKILL.md")),
    ("garnish-submit-preset", include_str!("../skills/garnish-submit-preset/SKILL.md")),
];

/// The one-line `description:` of a skill's frontmatter, without its quotes.
#[must_use]
pub fn description(text: &str) -> &str {
    let raw = text.lines().find_map(|l| l.strip_prefix("description: ")).unwrap_or("").trim();
    raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(raw)
}

/// Where `garnish install` puts the skills: the `skills` directory next to
/// the settings file (`~/.claude/skills`).
#[must_use]
pub fn default_dir(settings: &Path) -> PathBuf {
    settings.parent().map_or_else(|| PathBuf::from("skills"), |p| p.join("skills"))
}

/// What [`install`] did with one skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No file was there; it was written.
    Written,
    /// A different file was there; it was replaced.
    Updated,
    /// The file already had this text; nothing was touched.
    Unchanged,
    /// The skill directory or its `SKILL.md` is a symlink (a checkout linked
    /// into place); it was left alone so the link target is never rewritten.
    Skipped,
}

/// The result of [`install`]: the target directory and one status per skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The directory the skills went to.
    pub dir: PathBuf,
    /// `(skill name, what happened)`, in [`SKILLS`] order.
    pub statuses: Vec<(&'static str, Status)>,
}

impl Report {
    /// How many skills have a given status.
    #[must_use]
    pub fn count(&self, status: Status) -> usize {
        self.statuses.iter().filter(|(_, s)| *s == status).count()
    }

    /// The names that were [`Status::Skipped`].
    pub fn skipped(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.statuses.iter().filter(|(_, s)| *s == Status::Skipped).map(|(n, _)| *n)
    }

    /// One line for a person: `wrote 3 skill(s) to DIR`, `skills up to date in
    /// DIR`, or the mix, plus the skipped names.
    #[must_use]
    pub fn summary(&self) -> String {
        let (written, updated, unchanged) = (
            self.count(Status::Written),
            self.count(Status::Updated),
            self.count(Status::Unchanged),
        );
        let mut parts = Vec::new();
        if written > 0 {
            parts.push(format!("wrote {written}"));
        }
        if updated > 0 {
            parts.push(format!("updated {updated}"));
        }
        if unchanged > 0 {
            parts.push(format!("{unchanged} up to date"));
        }
        let skipped: Vec<_> = self.skipped().collect();
        if !skipped.is_empty() {
            parts.push(format!("left {} alone (symlink)", skipped.join(", ")));
        }
        format!("skills in {}: {}", self.dir.display(), parts.join(", "))
    }
}

/// Write every skill to `<dir>/<name>/SKILL.md`, creating the directories.
///
/// Only garnish's own three files are ever written, so nothing else in `dir`
/// is touched; a file that already has the right text is left as it is, a
/// symlinked skill is skipped (see [`Status::Skipped`]), and a replacement
/// goes through a temp file and `rename` so a reader never sees a torn file.
///
/// # Errors
/// Propagates the first I/O error (the skills already written stay).
pub fn install(dir: &Path) -> std::io::Result<Report> {
    let mut statuses = Vec::with_capacity(SKILLS.len());
    for (name, text) in SKILLS {
        let target = dir.join(name);
        let file = target.join("SKILL.md");
        let is_link = |p: &Path| p.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
        if is_link(&target) || is_link(&file) {
            statuses.push((name, Status::Skipped));
            continue;
        }
        std::fs::create_dir_all(&target).map_err(|e| at(&target, &e))?;
        let status = match std::fs::read_to_string(&file) {
            Ok(existing) if existing == text => Status::Unchanged,
            Ok(_) => Status::Updated,
            Err(_) => Status::Written,
        };
        if status != Status::Unchanged {
            // A fresh, pid-named temp file (`create_new`, so a planted symlink
            // of that name is an error, never followed), then an atomic rename.
            let tmp = target.join(format!("SKILL.md.{}.tmp", std::process::id()));
            let write = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()))
                .and_then(|()| std::fs::rename(&tmp, &file));
            if let Err(e) = write {
                let _ = std::fs::remove_file(&tmp);
                return Err(at(&file, &e));
            }
        }
        statuses.push((name, status));
    }
    Ok(Report { dir: dir.to_path_buf(), statuses })
}

/// An I/O error that names the path it happened at.
fn at(path: &Path, e: &std::io::Error) -> std::io::Error {
    std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `---` block of a skill: exactly a `name` and a quoted `description`.
    fn frontmatter(text: &str) -> Vec<&str> {
        let body = text.strip_prefix("---\n").unwrap();
        let end = body.find("\n---\n").unwrap();
        body.get(..end).unwrap().lines().collect()
    }

    #[test]
    fn skills_have_plain_two_key_frontmatter() {
        for (name, text) in SKILLS {
            let lines = frontmatter(text);
            assert_eq!(lines.len(), 2, "{name}: {lines:?}");
            assert_eq!(lines[0], format!("name: {name}"), "{name}");
            let desc = lines[1].strip_prefix("description: ").unwrap();
            // A plain scalar with `: ` inside is a YAML error and the skill
            // never loads (Phase 18 review), so the value is always quoted and
            // contains no quote of its own.
            assert!(
                desc.starts_with('"') && desc.ends_with('"') && desc.len() > 40,
                "{name}: {desc}"
            );
            assert!(!desc.trim_matches('"').contains('"'), "{name}: {desc}");
            assert_eq!(format!("\"{}\"", description(text)), desc, "{name}");
            assert!(text.contains("garnish"), "{name}");
        }
    }

    #[test]
    fn statusline_sample_payload_is_the_subscription_full_fixture() {
        let text = SKILLS[0].1;
        let (_, after) = text.split_once("```json\n").unwrap();
        let (json, _) = after.split_once("\n```").unwrap();
        let sample: serde_json::Value = serde_json::from_str(json).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/payloads/subscription-full.json"))
                .unwrap();
        assert_eq!(sample, fixture, "keep the embedded sample equal to the fixture");
    }

    #[test]
    fn install_writes_updates_leaves_unchanged_and_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skills");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mine.md"), "keep").unwrap();
        std::fs::create_dir_all(root.join("garnish-feedback")).unwrap();
        std::fs::write(root.join("garnish-feedback/SKILL.md"), "old text").unwrap();

        let report = install(&root).unwrap();
        assert_eq!(report.count(Status::Written), 2);
        assert_eq!(report.count(Status::Updated), 1);
        for (name, text) in SKILLS {
            let file = root.join(name).join("SKILL.md");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "{name}");
            assert!(!root.join(name).join("SKILL.md.tmp").exists());
        }
        assert!(report.summary().starts_with("skills in "), "{}", report.summary());
        assert!(report.summary().contains("wrote 2, updated 1"), "{}", report.summary());

        // A second run changes nothing; a stranger's file is left alone.
        let again = install(&root).unwrap();
        assert_eq!(again.count(Status::Unchanged), 3);
        assert!(again.summary().ends_with("3 up to date"), "{}", again.summary());
        assert_eq!(std::fs::read_to_string(root.join("mine.md")).unwrap(), "keep");

        assert_eq!(
            default_dir(Path::new("/home/dev/.claude/settings.json")),
            PathBuf::from("/home/dev/.claude/skills")
        );
    }

    #[cfg(unix)]
    #[test]
    fn install_never_writes_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skills");
        let checkout = dir.path().join("checkout");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(checkout.join("statusline")).unwrap();
        std::fs::write(checkout.join("statusline/SKILL.md"), "my edits").unwrap();
        std::fs::write(checkout.join("feedback.md"), "my edits").unwrap();
        // A linked directory and a linked file, both pointing into a checkout.
        std::os::unix::fs::symlink(checkout.join("statusline"), root.join("garnish-statusline"))
            .unwrap();
        std::fs::create_dir_all(root.join("garnish-feedback")).unwrap();
        std::os::unix::fs::symlink(
            checkout.join("feedback.md"),
            root.join("garnish-feedback/SKILL.md"),
        )
        .unwrap();

        let report = install(&root).unwrap();
        assert_eq!(report.count(Status::Skipped), 2);
        assert_eq!(report.count(Status::Written), 1);
        assert_eq!(
            std::fs::read_to_string(checkout.join("statusline/SKILL.md")).unwrap(),
            "my edits"
        );
        assert_eq!(std::fs::read_to_string(checkout.join("feedback.md")).unwrap(), "my edits");
        assert!(
            report.summary().contains("left garnish-statusline, garnish-feedback alone (symlink)"),
            "{}",
            report.summary()
        );

        // A planted temp-file symlink is never followed: create_new refuses it
        // and the error names the skill file; the victim is untouched.
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "precious").unwrap();
        let preset = root.join("garnish-submit-preset");
        std::fs::write(preset.join("SKILL.md"), "stale").unwrap();
        let tmp = preset.join(format!("SKILL.md.{}.tmp", std::process::id()));
        std::os::unix::fs::symlink(&victim, &tmp).unwrap();
        let err = install(&root).unwrap_err().to_string();
        assert!(err.contains("garnish-submit-preset/SKILL.md"), "{err}");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
        assert_eq!(std::fs::read_to_string(preset.join("SKILL.md")).unwrap(), "stale");
    }

    #[test]
    fn install_error_names_the_path_in_the_way() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skills");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("garnish-feedback"), "a file, not a directory").unwrap();
        let err = install(&root).unwrap_err().to_string();
        assert!(err.contains("garnish-feedback"), "{err}");
    }

    #[test]
    fn reporting_skills_ask_before_posting_publicly() {
        // SPEC § 13: the body is shown and the person asked before
        // `gh issue create`; the ask must come first in the text.
        for (name, text) in SKILLS.iter().skip(1) {
            let ask = text.find("post this to justanotherspy/garnish as a public issue?");
            let post = text.find("gh issue create");
            assert!(ask.is_some_and(|a| post.is_some_and(|p| a < p)), "{name}: {ask:?} {post:?}");
            assert!(text.contains("with `~`"), "{name}: home directory redaction");
        }
    }
}
