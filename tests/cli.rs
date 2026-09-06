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
    // The skills land next to settings.json (SPEC § 13), after the config.
    assert!(out.contains("skills in ") && out.contains(": wrote 3"), "{out}");
    assert!(out.find("wrote default config").unwrap() < out.find("skills in ").unwrap(), "{out}");
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
    assert!(out.contains(": 3 up to date"), "unchanged skills are not rewritten: {out}");
    assert!(err.contains("set `padding = 2`"), "existing config gets the hint on stderr: {err}");
    assert!(!out.contains("padding"), "{out}");

    // --no-skills for real: settings and config written, no skills directory.
    let other = tempfile::tempdir().unwrap();
    let (out, _, ok) = run(&["install", "--absolute", "--no-skills"], other.path(), &[]);
    assert!(ok && out.contains("wrote default config") && !out.contains("skill"), "{out}");
    assert!(!other.path().join(".claude/skills").exists());
    assert!(other.path().join(".claude/settings.json").exists());
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

    // Skills: listed with descriptions (name column, no quotes), written to a
    // chosen directory or, by default, next to the settings file.
    let (out, _, ok) = run(&["skills", "list"], home, &[]);
    assert!(ok && out.lines().count() == 3, "{out}");
    for line in out.lines() {
        let (name, desc) = line.split_at(25);
        assert!(name.starts_with("garnish-") && name.ends_with(' '), "{line}");
        assert!(desc.len() > 40 && !desc.contains('"') && desc.ends_with('.'), "{line}");
    }
    let dir = home.join("my-skills");
    let (out, _, ok) = run(&["skills", "install", "--dir", dir.to_str().unwrap()], home, &[]);
    assert!(ok && out.contains("my-skills: wrote 3"), "{out}");
    assert!(dir.join("garnish-feedback/SKILL.md").exists());
    let (out, _, ok) = run(&["skills", "install"], home, &[]);
    assert!(ok && out.contains(".claude/skills: wrote 3"), "{out}");
    assert!(home.join(".claude/skills/garnish-statusline/SKILL.md").exists());
}

#[test]
fn preview_of_an_unreadable_config_keeps_the_overrides() {
    // The feedback skill renders with `--color never` for people whose config
    // may be broken; the overlay used to be dropped on the cannot-read path.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let missing = home.join("nope.toml");
    let broken = home.join("broken.toml");
    std::fs::write(&broken, "preset = \"full\"\n[frame\n").unwrap();
    let payload =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads/subscription-full.json");
    for (file, problem) in [(&missing, "cannot read"), (&broken, "broken.toml:2 ")] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
        cmd.args(["--config", file.to_str().unwrap(), "preview", payload])
            .args(["--color", "never", "--icons", "ascii", "--width", "120"])
            .env("HOME", home)
            .env("GARNISH_CACHE_DIR", home.join("cache"))
            .env("GARNISH_NOW", "1738425600")
            .env("GARNISH_NO_SPAWN", "1")
            .env("CLICOLOR_FORCE", "1")
            .env_remove("NO_COLOR")
            .env_remove("GARNISH_CONFIG");
        let out = cmd.output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.contains(problem), "{out}");
        // The header is the one dim escape preview prints itself; the rows
        // are plain (`--color never`) with the ascii bars (`--icons ascii`;
        // the frame glyphs come from the config, not the icon set).
        let rows: Vec<_> = out.lines().skip(1).collect();
        assert!(rows.len() > 1, "{out}");
        assert!(rows.iter().all(|l| !l.contains('\x1b')), "{out:?}");
        assert!(out.contains("ctx: ####") && !out.contains('█'), "{out}");
    }
}

#[test]
fn writing_commands_refuse_to_guess_a_home_directory() {
    // With HOME unset the defaults fell back to the current directory, so
    // `install` dropped .claude/ and garnish/ into whatever repo it ran from.
    let dir = tempfile::tempdir().unwrap();
    for args in [&["install", "--dry-run"][..], &["config", "init"]] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
        cmd.args(args)
            .current_dir(dir.path())
            .env_remove("HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("GARNISH_CONFIG");
        let out = cmd.output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {err}");
        assert!(err.contains("HOME") && !err.contains("Location:"), "{args:?}: {err}");
    }
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none(), "nothing written");
    // Neither does an explicit settings file make `install` guess the config
    // location, nor does GARNISH_CONFIG make `config init` guess it: the
    // config goes where GARNISH_CONFIG says.
    let settings = dir.path().join("settings.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(["install", "--settings", settings.to_str().unwrap()])
        .current_dir(dir.path())
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("GARNISH_CONFIG");
    let out = cmd.output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("HOME"), "{err}");
    assert!(!dir.path().join("garnish").exists(), "no ./garnish/ in the cwd");
    let via_env = dir.path().join("env.toml");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(["config", "init"])
        .current_dir(dir.path())
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("GARNISH_CONFIG", &via_env);
    assert!(cmd.output().unwrap().status.success());
    assert!(via_env.exists() && !dir.path().join("garnish").exists());
    std::fs::remove_file(&via_env).unwrap();
    // An explicit path needs no HOME.
    let target = dir.path().join("g.toml");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(["--config", target.to_str().unwrap(), "config", "init"])
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME");
    assert!(cmd.output().unwrap().status.success());
    assert!(target.exists());
}
