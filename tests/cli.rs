//! End-to-end tests of the user-facing subcommands: `install`, `doctor`,
//! `modules`, `presets`, and `config init|check|show|path`.

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

fn run(args: &[&str], home: &Path, extra: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("GARNISH_NOW", "1738425600")
        .env("NO_COLOR", "1")
        .env_remove("GARNISH_CONFIG");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

#[test]
fn install_dry_run_writes_nothing_and_real_install_merges_with_backup() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let settings = home.join(".claude").join("settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, r#"{"theme":"dark","statusLine":{"type":"command","command":"old.sh","hideVimModeIndicator":true}}"#).unwrap();

    let (out, _, ok) = run(&["install", "--dry-run", "--absolute"], home, &[]);
    assert!(ok, "{out}");
    assert!(out.contains("would write"), "{out}");
    assert!(out.contains("\"hideVimModeIndicator\": true"), "{out}");
    assert!(out.contains("would write a default config"), "{out}");
    assert!(out.contains("would write 3 skill(s)"), "{out}");
    assert!(!home.join(".claude/skills").exists(), "dry run writes nothing");
    let (out, _, ok) = run(&["install", "--dry-run", "--absolute", "--no-skills"], home, &[]);
    assert!(ok && !out.contains("skill"), "{out}");
    let (out, _, ok) = run(&["install", "--dry-run", "--absolute", "--padding", "1"], home, &[]);
    assert!(
        ok && out.contains("would write a default config") && out.contains("(padding = 2)"),
        "{out}"
    );
    assert!(std::fs::read_to_string(&settings).unwrap().contains("old.sh"));
    assert!(!home.join(".config/garnish/garnish.toml").exists());

    let (out, _, ok) =
        run(&["install", "--absolute", "--refresh-interval", "2", "--padding", "1"], home, &[]);
    assert!(ok, "{out}");
    assert!(out.contains("updated") && out.contains("backup"), "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["statusLine"]["type"], "command");
    assert!(v["statusLine"]["command"].as_str().unwrap().ends_with("garnish"));
    assert_eq!(v["statusLine"]["refreshInterval"], 2);
    assert_eq!(v["statusLine"]["padding"], 1);
    assert_eq!(v["statusLine"]["hideVimModeIndicator"], true);
    let backups: Vec<_> = std::fs::read_dir(settings.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("settings.json.bak-"))
        .collect();
    assert_eq!(backups.len(), 1);
    let cfg = home.join(".config/garnish/garnish.toml");
    assert!(cfg.exists());
    // The skills land next to settings.json (SPEC § 13).
    assert!(out.contains("wrote 3 skill(s)"), "{out}");
    for name in ["garnish-statusline", "garnish-feedback", "garnish-submit-preset"] {
        let skill = home.join(".claude/skills").join(name).join("SKILL.md");
        assert!(std::fs::read_to_string(&skill).unwrap().starts_with("---\nname: "), "{name}");
    }
    let cfg_text = std::fs::read_to_string(&cfg).unwrap();
    assert!(cfg_text.contains("[modules.context]"));
    // statusLine.padding = 1 pads both sides, so the config mirrors it doubled.
    assert!(cfg_text.contains("\npadding = 2\n"), "{cfg_text}");

    let (out, err, ok) =
        run(&["install", "--absolute", "--refresh-interval", "2", "--padding", "1"], home, &[]);
    assert!(ok && out.contains("already up to date"), "{out}");
    assert!(err.contains("set `padding = 2`"), "existing config gets the hint on stderr: {err}");
    assert!(!out.contains("padding"), "{out}");
    // The config key is a u16; a value that would not round-trip is refused up front.
    let (_, err, ok) = run(&["install", "--dry-run", "--padding", "40000"], home, &[]);
    assert!(!ok && err.contains("40000"), "{err}");
    let (out, _, ok) = run(&["install", "--dry-run", "--padding", "3"], home, &[]);
    assert!(ok && !out.contains("would write a default config"), "config exists: {out}");

    let (out, err, _) = run(&["install", "--dry-run"], home, &[("PATH", "/nonexistent")]);
    assert!(err.contains("not on PATH"), "{err}");
    assert!(!out.contains("not on PATH"), "the JSON preview stays clean: {out}");
    // a dry run reports the same read errors the real run would
    let bad = home.join("dir.json");
    std::fs::create_dir_all(&bad).unwrap();
    let (_, err, ok) =
        run(&["install", "--dry-run", "--settings", bad.to_str().unwrap()], home, &[]);
    assert!(!ok && err.contains("reading"), "{err}");
}

#[test]
fn config_subcommands_and_doctor_work_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let (out, _, ok) = run(&["config", "path"], home, &[]);
    assert!(ok && out.trim().ends_with(".config/garnish/garnish.toml"), "{out}");
    let (out, _, ok) = run(&["config", "check"], home, &[]);
    assert!(ok && out.contains("no config file"), "{out}");
    let (out, _, ok) = run(&["config", "init", "--preset", "compact"], home, &[]);
    assert!(ok && out.starts_with("wrote "), "{out}");
    let (out, err, ok) = run(&["config", "init"], home, &[]);
    assert!(!ok, "refuses to overwrite without --force");
    // A refusal is one line on stderr, not an error report (walkthrough bug 7).
    assert!(err.contains("pass --force"), "{err}");
    assert!(!err.contains("Location:") && !err.contains("Error:"), "{err}");
    assert!(out.is_empty(), "{out}");
    let (out, _, ok) = run(&["config", "check"], home, &[]);
    assert!(ok && out.contains(": ok"), "{out}");
    let (out, _, ok) = run(&["config", "show"], home, &[]);
    assert!(ok && out.contains("preset = \"compact\"") && out.contains("[modules.clock]"), "{out}");
    let cfg = home.join(".config/garnish/garnish.toml");
    std::fs::write(&cfg, "theme = \"nope\"\n[modules.context]\nwidth = -1\n").unwrap();
    let (out, err, ok) = run(&["config", "check"], home, &[]);
    assert!(!ok);
    assert!(out.contains("theme:") && out.contains("modules.context.width"), "{out}");
    // The problem list and the count are the whole output: no report on stderr (bug 7).
    assert!(out.trim_end().ends_with("2 problem(s) found"), "{out}");
    assert!(err.is_empty(), "{err}");

    let (out, _, ok) = run(&["modules"], home, &[]);
    assert!(ok);
    assert_eq!(out.lines().count(), 22, "21 modules plus the text family:\n{out}");
    assert!(out.lines().any(|l| l.starts_with("context ")));
    assert!(out.lines().last().unwrap().starts_with("text.<name>  "), "{out}");

    let (out, _, ok) = run(&["doctor"], home, &[]);
    assert!(ok, "{out}");
    for needle in
        ["garnish 0.", "claude settings", "2 problem(s)", "cache", "glyph test", "unicode"]
    {
        assert!(out.contains(needle), "{needle}\n{out}");
    }

    // The gallery: listed, written without its tooling header, valid, and an
    // unknown name is a real error that names the choices.
    let (out, _, ok) = run(&["presets"], home, &[]);
    assert!(ok, "{out}");
    assert!(out.lines().count() >= 15, "{out}");
    assert!(out.lines().any(|l| l.starts_with("minimal-clean ")), "{out}");
    let (out, _, ok) = run(&["config", "init", "--force", "--preset", "minimal-clean"], home, &[]);
    assert!(ok && out.starts_with("wrote "), "{out}");
    let written = std::fs::read_to_string(&cfg).unwrap();
    assert!(!written.contains("# name:") && !written.contains("# columns:"), "{written}");
    assert!(written.contains("preset = \"minimal\""), "{written}");
    let (out, _, ok) = run(&["config", "check"], home, &[]);
    assert!(ok && out.contains(": ok"), "{out}");
    let (_, err, ok) = run(&["config", "init", "--force", "--preset", "nope"], home, &[]);
    assert!(!ok && err.contains("gallery name") && err.contains("minimal-clean"), "{err}");
    assert!(!err.contains("Location:"), "a typo is one line, not a report: {err}");

    // Skills: listed with descriptions, and written to a chosen directory.
    let (out, _, ok) = run(&["skills", "list"], home, &[]);
    assert!(ok && out.lines().count() == 3, "{out}");
    assert!(out.lines().all(|l| l.starts_with("garnish-") && l.len() > 30), "{out}");
    let dir = home.join("my-skills");
    let (out, _, ok) = run(&["skills", "install", "--dir", dir.to_str().unwrap()], home, &[]);
    assert!(ok && out.contains("wrote 3 skill(s)"), "{out}");
    assert!(dir.join("garnish-feedback/SKILL.md").exists());
}
