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

/// The one-line `description:` of a skill's frontmatter.
#[must_use]
pub fn description(text: &str) -> &str {
    text.lines().find_map(|l| l.strip_prefix("description: ")).unwrap_or("").trim()
}

/// Where `garnish install` puts the skills: the `skills` directory next to
/// the settings file (`~/.claude/skills`).
#[must_use]
pub fn default_dir(settings: &Path) -> PathBuf {
    settings.parent().map_or_else(|| PathBuf::from("skills"), |p| p.join("skills"))
}

/// Write every skill to `<dir>/<name>/SKILL.md`, creating the directories.
///
/// Only garnish's own three files are ever written, so an existing skill of
/// the same name is refreshed and nothing else in `dir` is touched.
///
/// # Errors
/// Propagates the first I/O error (the files already written stay).
pub fn install(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::with_capacity(SKILLS.len());
    for (name, text) in SKILLS {
        let target = dir.join(name);
        std::fs::create_dir_all(&target)?;
        let file = target.join("SKILL.md");
        std::fs::write(&file, text)?;
        written.push(file);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_have_frontmatter_and_install_writes_all_three() {
        for (name, text) in SKILLS {
            assert!(text.starts_with("---\n"), "{name}: frontmatter");
            assert!(text.contains(&format!("name: {name}\n")), "{name}: name line");
            assert!(!description(text).is_empty(), "{name}: description");
            assert!(text.contains("garnish"), "{name}");
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skills");
        let written = install(&root).unwrap();
        assert_eq!(written.len(), 3);
        for (name, text) in SKILLS {
            let file = root.join(name).join("SKILL.md");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "{name}");
        }
        // Idempotent, and a stranger's file in the directory is left alone.
        std::fs::write(root.join("mine.md"), "keep").unwrap();
        install(&root).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("mine.md")).unwrap(), "keep");
        assert_eq!(
            default_dir(Path::new("/home/dev/.claude/settings.json")),
            PathBuf::from("/home/dev/.claude/skills")
        );
    }
}
