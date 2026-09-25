//! End-to-end tests of the user-facing subcommands: `install`, `doctor`,
//! `modules`, `presets`, and `config init|check|show|path`.

// Integration tests are not `#[cfg(test)]` modules, so the clippy.toml test
// allowances do not apply; panicking on setup failure is the right behaviour here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(args: &[&str], home: &Path, extra: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    // The working directory is the test's own, not the checkout: `config
    // show` and `doctor` read the settings chain of the current directory,
    // and the checkout's `.claude/` must not leak into a test. Every
    // argument a test passes is an absolute path.
    cmd.args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("GARNISH_NOW", "1738425600")
        .env("NO_COLOR", "1")
        // CLAUDE.md § Cache and worker invariants: no test that runs the
        // binary may fork a detached worker. `preview` over a directory
        // renders every fixture, and one whose `cwd` were a real repository
        // would spawn a worker per cached module, outliving the test and
        // writing into a tempdir that is already gone.
        .env("GARNISH_NO_SPAWN", "1")
        .env_remove("GARNISH_CONFIG")
        // A developer running with animations off must not turn the
        // suite red; a test that wants the switch sets it through `extra`.
        .env_remove("GARNISH_ANIMATE")
        // No managed settings file (SPEC § 9); a test that wants one
        // points the hook at its own through `extra`.
        .env("GARNISH_MANAGED_SETTINGS", "")
        // It moves the user settings file and the skills (SPEC § 7); a
        // test that wants it sets it through `extra`.
        .env_remove("CLAUDE_CONFIG_DIR");
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
    assert!(!err.contains("padding"), "the config already matches: {err}");
    assert!(!out.contains("padding"), "{out}");
    // A dry run says so too, rather than "would write" (cli-16).
    let (out, _, ok) = run(
        &["install", "--dry-run", "--absolute", "--refresh-interval", "2", "--padding", "1"],
        home,
        &[],
    );
    let first = out.lines().next().unwrap_or_default();
    assert!(ok && first == format!("{} already up to date", settings.display()), "{out}");
    // Claude Code's minimum is 1: a 0 is refused, not quietly raised.
    let (_, err, ok) = run(&["install", "--dry-run", "--refresh-interval", "0"], home, &[]);
    assert!(!ok && err.contains("--refresh-interval"), "{err}");

    // --no-skills for real: settings and config written, no skills directory.
    let other = tempfile::tempdir().unwrap();
    let (out, _, ok) = run(&["install", "--absolute", "--no-skills"], other.path(), &[]);
    assert!(ok && out.contains("wrote default config") && !out.contains("skill"), "{out}");
    assert!(!other.path().join(".claude/skills").exists());
    assert!(other.path().join(".claude/settings.json").exists());
    // The config key is a u16; a value that would not round-trip is refused up front.
    let (_, err, ok) = run(&["install", "--dry-run", "--padding", "40000"], home, &[]);
    assert!(!ok && err.contains("40000"), "{err}");
    let (out, err, ok) = run(&["install", "--dry-run", "--padding", "3"], home, &[]);
    assert!(ok && !out.contains("would write a default config"), "config exists: {out}");
    assert!(err.contains("set `padding = 6`"), "existing config gets the hint on stderr: {err}");

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
    // `init` leaves `animate` to Claude Code's prefersReducedMotion (SPEC § 4.2).
    let written = std::fs::read_to_string(home.join(".config/garnish/garnish.toml")).unwrap();
    assert!(written.contains("\n# animate = true\n"), "{written}");
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
    let built_in = garnish::modules::SCHEMAS.len();
    assert_eq!(
        out.lines().count(),
        built_in + 1,
        "{built_in} modules plus the text family:\n{out}"
    );
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

/// SPEC § 14: `setup --preset` writes the preset without a screen (with the
/// backup rule and, with `--install`, the settings), the bare `garnish` at
/// a terminal points at `setup` instead of waiting for a payload, and
/// `setup` without a terminal on stdout refuses in one line.
#[test]
fn setup_preset_twin_and_the_tty_pointer_work_without_a_screen() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let cfg = home.join(".config/garnish/garnish.toml");
    let (out, _, ok) = run(&["setup", "--preset", "compact"], home, &[]);
    assert!(ok && out.starts_with("wrote ") && !out.contains("backup"), "{out}");
    let written = std::fs::read_to_string(&cfg).unwrap();
    assert!(written.contains("preset = \"compact\""), "{written}");
    // A second write keeps the first as a backup, as `config init --force` does.
    let (out, _, ok) = run(&["setup", "--preset", "minimal-clean", "--install"], home, &[]);
    assert!(ok && out.contains("backup") && out.contains("wrote"), "{out}");
    assert!(out.contains(".claude/settings.json"), "--install hooks it up: {out}");
    assert!(out.contains("skills in "), "{out}");
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("\"command\": \"garnish\""), "{settings}");
    assert!(std::fs::read_to_string(&cfg).unwrap().contains("preset = \"minimal\""));
    let (_, err, ok) = run(&["setup", "--preset", "nope"], home, &[]);
    assert!(!ok && err.contains("gallery name"), "{err}");
    // `--install` plans before anything is written, as `install` does: a
    // settings file it refuses leaves the config as it was, no backup
    // beside it (cli-v2).
    let settings = home.join(".claude/settings.json");
    let before = std::fs::read_to_string(&cfg).unwrap();
    std::fs::write(&settings, "{ broken").unwrap();
    let names = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(cfg.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let kept = names();
    let (out, err, ok) = run(&["setup", "--preset", "compact", "--install"], home, &[]);
    assert!(!ok && out.is_empty() && err.lines().count() == 1, "{out}{err}");
    assert!(err.contains("settings.json") && !err.contains("Location:"), "{err}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), before, "the config is untouched");
    assert_eq!(names(), kept, "no backup either");
    std::fs::remove_file(&settings).unwrap();
    // A file that does not parse is never rewritten, even by a preset.
    std::fs::write(&cfg, "theme = \n").unwrap();
    let (_, err, ok) = run(&["setup", "--preset", "compact"], home, &[]);
    assert!(!ok && err.contains("never rewritten"), "{err}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "theme = \n");
    // No terminal on stdout: one line, exit 1, nothing drawn.
    let (out, err, ok) = run(&["setup"], home, &[]);
    assert!(!ok && err.contains("needs a terminal") && out.is_empty(), "{err}");
    assert!(!err.contains("Location:"), "{err}");
    // The bare `garnish` with a terminal on stdin points at `setup`; with a
    // pipe it renders (an empty payload is a bad one, which is a row).
    let (out, _, ok) = run(&[], home, &[("GARNISH_STDIN_TTY", "1")]);
    assert!(ok && out.contains("garnish setup"), "{out}");
    let (out, _, ok) = run(&[], home, &[("GARNISH_STDIN_TTY", "0")]);
    assert!(ok && out.contains("bad payload"), "{out}");
    // Every boolean hook reads one rule (SPEC § 9): Claude Code's words.
    let (out, _, ok) = run(&[], home, &[("GARNISH_STDIN_TTY", "true")]);
    assert!(ok && out.contains("garnish setup"), "{out}");
    let (out, _, ok) = run(&[], home, &[("GARNISH_STDIN_TTY", "Off")]);
    assert!(ok && out.contains("bad payload"), "{out}");
    let (out, _, ok) = run(&["render"], home, &[("GARNISH_STDIN_TTY", "1")]);
    assert!(ok && out.contains("bad payload"), "the explicit render always reads stdin: {out}");
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
            .env("GARNISH_MANAGED_SETTINGS", "")
            // Colour on (`color = auto` without NO_COLOR), so `--color
            // never` is what keeps the rows plain.
            .env_remove("NO_COLOR")
            .env_remove("CLAUDE_CONFIG_DIR")
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
fn config_show_prints_the_durations_a_ticker_implies() {
    // The shown config is what is in effect (SPEC § 4.1): a ticker with no
    // `durations` runs fixed, and `show` says so rather than echoing the
    // compact default back.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("ticker.toml");
    std::fs::write(&cfg, "overflow = \"ticker\"\n[modules.api]\ndurations = \"compact\"\n")
        .unwrap();
    let (shown, _, ok) =
        run(&["--config", cfg.to_str().unwrap(), "config", "show"], dir.path(), &[]);
    assert!(ok, "{shown}");
    assert!(shown.contains("\ndurations = \"fixed\"\n"), "{shown}");
    let api = shown.split("[modules.api]").nth(1).unwrap();
    let api = api.split("\n[").next().unwrap();
    assert!(api.contains("durations = \"compact\""), "{api}");
    let session = shown.split("[modules.session]").nth(1).unwrap();
    let session = session.split("\n[").next().unwrap();
    assert!(session.contains("durations = \"inherit\""), "{session}");
}

/// SPEC § 5: a `settings.json` or `garnish.toml` that does not parse is
/// never rewritten by `install` or `config init --force`; the command names
/// the file and the problem on one line and exits 1 without a report. A
/// file that parses is replaced with a never-clobbered backup next to it.
#[test]
fn unparsable_files_are_never_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let settings = home.join(".claude").join("settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let broken_json = "{\"statusLine\": {\"type\": \"command\",\n";
    std::fs::write(&settings, broken_json).unwrap();
    for args in [&["install", "--absolute"][..], &["install", "--absolute", "--dry-run"]] {
        let (out, err, ok) = run(args, home, &[]);
        assert!(!ok, "{args:?}: {out}");
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
        assert!(err.contains("settings.json") && err.contains("JSON"), "{args:?}: {err}");
        assert!(!err.contains("Location:") && !err.contains("Error:"), "{args:?}: {err}");
        assert!(out.is_empty(), "{args:?}: {out}");
    }
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), broken_json, "untouched");
    assert!(!home.join(".config/garnish/garnish.toml").exists(), "nothing else is written");
    let entries = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(entries(settings.parent().unwrap()), vec!["settings.json"], "no backup, no temp");
    std::fs::write(&settings, "[1, 2]\n").unwrap();
    let (_, err, ok) = run(&["install", "--absolute"], home, &[]);
    assert!(!ok && err.contains("object"), "{err}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "[1, 2]\n");
    // Bytes that are not UTF-8 are a file that does not parse, refused the
    // same way, not an error report (cli-14).
    let latin1 = b"{\"theme\": \"caf\xe9\"}\n";
    std::fs::write(&settings, latin1).unwrap();
    let (out, err, ok) = run(&["install", "--absolute"], home, &[]);
    assert!(!ok && out.is_empty(), "{out}");
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("settings.json: not valid UTF-8") && !err.contains("Location:"), "{err}");
    assert_eq!(std::fs::read(&settings).unwrap(), latin1);

    // A TOML syntax error is refused with its line; a bad value still
    // parses, so the file is replaced and kept as a backup.
    let cfg = home.join(".config").join("garnish").join("garnish.toml");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "preset = \"full\"\n[frame\n").unwrap();
    let (out, err, ok) = run(&["config", "init", "--force"], home, &[]);
    assert!(!ok && out.is_empty(), "{out}");
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("garnish.toml") && err.contains("line 2"), "{err}");
    assert!(!err.contains("Location:"), "{err}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "preset = \"full\"\n[frame\n");
    assert_eq!(entries(cfg.parent().unwrap()), vec!["garnish.toml"]);
    std::fs::write(&cfg, b"theme = \"\xff\"\n").unwrap();
    let (_, err, ok) = run(&["config", "init", "--force"], home, &[]);
    assert!(!ok && err.lines().count() == 1 && err.contains("not valid UTF-8"), "{err}");
    assert!(!err.contains("Location:"), "{err}");
    assert_eq!(std::fs::read(&cfg).unwrap(), b"theme = \"\xff\"\n");
    assert_eq!(entries(cfg.parent().unwrap()), vec!["garnish.toml"]);
    std::fs::write(&cfg, "theme = \"nope\"\n").unwrap();
    let (out, _, ok) = run(&["config", "init", "--force"], home, &[]);
    assert!(ok && out.starts_with("wrote ") && out.contains("(backup: "), "{out}");
    let names = entries(cfg.parent().unwrap());
    let backups: Vec<&String> =
        names.iter().filter(|n| n.starts_with("garnish.toml.bak-")).collect();
    assert_eq!(backups.len(), 1, "{names:?}");
    assert_eq!(
        std::fs::read_to_string(cfg.parent().unwrap().join(backups[0])).unwrap(),
        "theme = \"nope\"\n"
    );
    assert!(std::fs::read_to_string(&cfg).unwrap().contains("[modules.context]"));
    assert!(names.iter().all(|n| !n.contains(".tmp.")), "{names:?}");
    // A gallery preset goes through the same probe and backup.
    let (out, _, ok) = run(&["config", "init", "--force", "--preset", "minimal-clean"], home, &[]);
    assert!(ok && out.contains("(backup: "), "{out}");
    assert_eq!(
        entries(cfg.parent().unwrap())
            .iter()
            .filter(|n| n.starts_with("garnish.toml.bak-"))
            .count(),
        2
    );
    // A first write needs no backup and says so by omission.
    let fresh = home.join("fresh").join("garnish.toml");
    let (out, _, ok) = run(&["--config", fresh.to_str().unwrap(), "config", "init"], home, &[]);
    assert!(ok && out.trim_end().ends_with("fresh/garnish.toml"), "{out}");
    assert_eq!(entries(fresh.parent().unwrap()), vec!["garnish.toml"]);
}

/// SPEC § 4.2: `config show` prints the animation switch in effect, which
/// with `animate` unset follows Claude Code's `prefersReducedMotion` in the
/// settings chain of the current directory and the home; an explicit key
/// in the file wins over the setting.
#[test]
fn config_show_prints_the_animate_switch_in_effect() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let cfg = home.join("garnish.toml");
    std::fs::write(&cfg, "preset = \"minimal\"\n").unwrap();
    let show = |extra: &str| {
        std::fs::write(&cfg, format!("preset = \"minimal\"\n{extra}")).unwrap();
        let (shown, _, ok) = run(&["--config", cfg.to_str().unwrap(), "config", "show"], home, &[]);
        assert!(ok, "{shown}");
        shown
    };
    assert!(show("").contains("\nanimate = true\n"), "no settings: on");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude/settings.json"), r#"{"prefersReducedMotion": true}"#)
        .unwrap();
    assert!(show("").contains("\nanimate = false\n"), "the user setting freezes an unset key");
    assert!(show("animate = true\n").contains("\nanimate = true\n"), "an explicit key wins");
    // The project chain of the current directory (the test's home) outranks
    // the user file; the session switch is not part of a config and stays out.
    std::fs::write(home.join(".claude/settings.local.json"), r#"{"prefersReducedMotion": false}"#)
        .unwrap();
    assert!(show("").contains("\nanimate = true\n"), "the local file wins");
    std::fs::remove_file(home.join(".claude/settings.local.json")).unwrap();
    std::fs::write(&cfg, "preset = \"minimal\"\nanimate = true\n").unwrap();
    let (shown, _, ok) = run(
        &["--config", cfg.to_str().unwrap(), "config", "show"],
        home,
        &[("GARNISH_ANIMATE", "0")],
    );
    assert!(
        ok && shown.contains("\nanimate = true\n"),
        "GARNISH_ANIMATE is a session switch: {shown}"
    );
    // What `show` prints is what the tick uses: the shown config renders
    // the same frozen spinner as the original under the same settings.
    let payload =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads/subscription-full.json");
    let shown = show("");
    let copy = home.join("shown.toml");
    std::fs::write(&copy, &shown).unwrap();
    let render = |file: &Path| {
        let args = ["--config", file.to_str().unwrap(), "preview", payload, "--width", "80"];
        let (out, _, ok) = run(&args, home, &[("GARNISH_NOW", "1738425601")]);
        assert!(ok, "{out}");
        out
    };
    assert_eq!(render(&cfg), render(&copy));
}

#[test]
fn preview_typos_are_one_line_not_a_report() {
    let dir = tempfile::tempdir().unwrap();
    let payload =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads/subscription-full.json");
    for (flag, value, expected) in [
        ("--preset", "fulll", "default, minimal, full or compact"),
        ("--icons", "nerdy", "nerd, unicode, emoji or ascii"),
        ("--color", "sometimes", "auto, always, never, 256 or truecolor"),
        // A theme typo was blamed on the config file under every fixture.
        ("--theme", "nrod", "garnish, catppuccin-mocha, nord"),
    ] {
        let (out, err, ok) = run(&["preview", payload, flag, value], dir.path(), &[]);
        assert!(!ok && out.is_empty(), "{flag} {value}: {out}");
        assert_eq!(err.lines().count(), 1, "{flag}: {err}");
        assert!(err.contains(value) && err.contains(expected), "{flag}: {err}");
        assert!(!err.contains("Location:") && !err.contains("Error:"), "{flag}: {err}");
    }
}

/// A listing piped into a reader that stops early (`garnish presets |
/// head`) is not a failure: a broken pipe used to end in a color-eyre
/// report with a source location and exit 1.
#[test]
fn a_reader_that_stops_early_gets_no_report() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads");
    let mut child = Command::new(env!("CARGO_BIN_EXE_garnish"))
        .args(["preview", fixtures, "--width", "100"])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env_remove("GARNISH_CONFIG")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The reader is gone before the first write.
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !err.contains("Location:"), "{:?}: {err}", out.status);
}

/// `garnish docs` is a maintainer's tool: hidden from `--help`, and with
/// no default `--out`, so run in a project of one's own it cannot replace
/// that project's `docs/README.md`.
#[test]
fn docs_needs_an_explicit_out_and_is_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    std::fs::create_dir_all(home.join("docs")).unwrap();
    std::fs::write(home.join("docs/README.md"), "mine").unwrap();
    let (_, err, ok) = run(&["docs"], home, &[]);
    assert!(!ok && err.contains("--out"), "{err}");
    assert_eq!(std::fs::read_to_string(home.join("docs/README.md")).unwrap(), "mine");
    let (help, _, ok) = run(&["--help"], home, &[]);
    assert!(ok && help.contains("preview") && !help.contains("docs"), "{help}");
    let out = home.join("generated");
    let (said, _, ok) = run(&["docs", "--out", out.to_str().unwrap()], home, &[]);
    assert!(ok && out.join("README.md").exists(), "{said}");
}

/// `--lock-held` means "the caller already holds *this module's* lock" and
/// is only ever passed beside `--module` (`spawn::Job::args`). With `--all`
/// it would adopt a lock per module — inventing one where there was none,
/// and taking over one a live worker still holds — so clap refuses it.
#[test]
fn refresh_refuses_all_together_with_lock_held() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let (out, err, ok) =
        run(&["refresh", "--all", "--lock-held", "--session", "s", "--cwd", cwd], dir.path(), &[]);
    assert!(!ok && out.is_empty(), "{out}");
    assert!(err.contains("--all") && err.contains("--lock-held"), "{err}");
    // Each on its own is still accepted.
    let (_, err, ok) = run(&["refresh", "--all", "--session", "s", "--cwd", cwd], dir.path(), &[]);
    assert!(ok, "{err}");
}

/// SPEC § 7: `preview <dir>` renders every `*.json` in the directory, in
/// name order, each under a dim `── <name>` heading.
#[test]
fn preview_of_a_directory_renders_every_fixture_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads");
    let (out, err, ok) = run(&["preview", fixtures, "--width", "100"], dir.path(), &[]);
    assert!(ok, "{err}");
    let mut names: Vec<String> = std::fs::read_dir(fixtures)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    // The heading is dim; compare it without the styling.
    let plain = out.replace("\x1b[2m", "").replace("\x1b[0m", "");
    let headings: Vec<&str> = plain.lines().filter_map(|l| l.strip_prefix("── ")).collect();
    assert_eq!(headings, names, "{out}");
    assert!(!out.contains("⚠ garnish"), "{out}");
}

/// SPEC § 14: a preview is not a tick, so it never reads the cache or
/// spawns a worker; a cached module shows its not-yet-refreshed state.
/// `account` is session-scoped, so with it placed a preview used to log
/// a spawn per fixture (and fork one for real outside the tests), leaving
/// lock files under the fixtures' session ids.
#[test]
fn preview_never_touches_the_cache_or_spawns_a_worker() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("garnish.toml");
    std::fs::write(&cfg, "[[line]]\nmodules = [\"model\", \"account\", \"branch\", \"sync\"]\n")
        .unwrap();
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads");
    let args = ["--config", cfg.to_str().unwrap(), "preview", fixtures, "--width", "80"];
    let (out, err, ok) = run(&args, dir.path(), &[]);
    assert!(ok && out.contains("Opus"), "{out}{err}");
    let cache = dir.path().join("cache");
    assert!(!cache.exists(), "preview touched the cache under {}", cache.display());
}

#[test]
fn config_show_round_trips_every_fixture_and_preset() {
    // `show` prints the resolved config: what it prints must pass `check`
    // and print the same again (whole-stack review: an unknown theme name
    // and unknown line ids used to be echoed back).
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<std::path::PathBuf> = ["tests/fixtures/configs", "presets"]
        .iter()
        .flat_map(|d| std::fs::read_dir(root.join(d)).unwrap().flatten().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.push(root.join("examples/garnish.toml"));
    files.sort();
    assert!(files.len() > 30, "{}", files.len());
    for file in files {
        let (shown, _, ok) =
            run(&["--config", file.to_str().unwrap(), "config", "show"], home, &[]);
        assert!(ok, "{}: show failed", file.display());
        let copy = home.join("shown.toml");
        std::fs::write(&copy, &shown).unwrap();
        let (out, _, ok) = run(&["--config", copy.to_str().unwrap(), "config", "check"], home, &[]);
        assert!(ok && out.contains(": ok"), "{}: show output fails check:\n{out}", file.display());
        let (again, _, _) = run(&["--config", copy.to_str().unwrap(), "config", "show"], home, &[]);
        assert_eq!(shown, again, "{}: show is not a fixed point", file.display());
        // And the shown config renders the same rows as the original (minus
        // the original's `⚠ config:` row, which the resolved form has fixed):
        // a line emptied by a mistake must not come back as a drawn spacer.
        let payload =
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads/subscription-full.json");
        let render = |cfg: &std::path::Path| {
            let args = ["--config", cfg.to_str().unwrap(), "preview", payload, "--width", "120"];
            let (out, _, ok) = run(&args, home, &[]);
            assert!(ok, "{}: preview failed", cfg.display());
            out.lines()
                .skip(1)
                .filter(|l| !l.starts_with("⚠ config: ") && !l.starts_with("! config: "))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(render(&file), render(&copy), "{}: show changes the render", file.display());
    }
}

#[test]
fn writing_commands_refuse_to_guess_a_home_directory() {
    // With HOME unset the defaults fell back to the current directory, so
    // `install` dropped .claude/ and garnish/ into whatever repo it ran from.
    // Every command SPEC § 5 names refuses, with HOME unset and with it set
    // but empty (the shell's "unset", `claude_settings::home_dir`), naming
    // the flag that says where instead.
    let dir = tempfile::tempdir().unwrap();
    let commands: [(&[&str], &str); 5] = [
        (&["install", "--dry-run"], "--settings"),
        (&["config", "init"], "--config"),
        (&["config", "path"], "--config"),
        (&["skills", "install"], "--dir"),
        (&["setup", "--preset", "compact"], "--config"),
    ];
    for empty in [false, true] {
        for (args, flag) in commands {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
            cmd.args(args)
                .current_dir(dir.path())
                .env("GARNISH_MANAGED_SETTINGS", "")
                .env_remove("GARNISH_CONFIG")
                .env_remove("CLAUDE_CONFIG_DIR");
            if empty {
                cmd.env("HOME", "").env("XDG_CONFIG_HOME", "");
            } else {
                cmd.env_remove("HOME").env_remove("XDG_CONFIG_HOME");
            }
            let out = cmd.output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            let what = format!("{args:?} (HOME empty: {empty})");
            assert_eq!(out.status.code(), Some(1), "{what}: {err}");
            assert_eq!(err.lines().count(), 1, "{what}: {err}");
            assert!(err.contains("HOME") && err.contains(flag), "{what}: {err}");
            assert!(!err.contains("Location:"), "{what}: {err}");
        }
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
        .env("GARNISH_MANAGED_SETTINGS", "")
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
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env("GARNISH_CONFIG", &via_env);
    assert!(cmd.output().unwrap().status.success());
    assert!(via_env.exists() && !dir.path().join("garnish").exists());
    std::fs::remove_file(&via_env).unwrap();
    // An explicit path needs no HOME.
    let target = dir.path().join("g.toml");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(["--config", target.to_str().unwrap(), "config", "init"])
        .env_remove("HOME")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env_remove("XDG_CONFIG_HOME");
    assert!(cmd.output().unwrap().status.success());
    assert!(target.exists());
}

/// One tick of `config` over `payload` on stdin, as the harness runs it:
/// the same hermetic environment as [`run`], colour left to the config.
fn tick(config: &Path, home: &Path, payload: &str, extra: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(["--config", config.to_str().unwrap()])
        .current_dir(home)
        .env("HOME", home)
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("GARNISH_NOW", "1738425600")
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env("COLUMNS", "84")
        .env_remove("NO_COLOR")
        .env_remove("GARNISH_ANIMATE")
        .env_remove("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .env_remove("DISABLE_AUTO_COMPACT")
        .env_remove("DISABLE_COMPACT")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// One run of the binary with `args` and a payload piped in, as the
/// harness runs `statusLine.command`: stdout and the exit code.
fn piped(args: &[&str], home: &Path, extra: &[(&str, &str)]) -> (String, Option<i32>) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env("GARNISH_STDIN_TTY", "0")
        .env("NO_COLOR", "1")
        .env_remove("GARNISH_CONFIG")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    // A run refused before it reads stdin may have exited already: the
    // write then fails with a broken pipe, which is not the test's concern.
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    let out = child.wait_with_output().unwrap();
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code())
}

/// SPEC § 5: the render path always exits 0 and prints something, since a
/// non-zero exit clears the status line. A mistyped flag in
/// `statusLine.command` (or one an upgrade removed) and a panic both used
/// to exit non-zero with nothing on stdout; each is a `⚠ garnish:` row
/// now. A subcommand's bad flag is still clap's usage error.
#[test]
fn a_bad_flag_or_a_panic_on_the_render_path_is_a_warning_row() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let (out, code) = piped(&["--confg", "x.toml"], home, &[]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.starts_with("⚠ garnish: ") && out.contains("--confg"), "{out}");
    assert_eq!(out.lines().count(), 1, "{out}");
    let (out, code) = piped(&["--config"], home, &[]);
    assert!(code == Some(0) && out.starts_with("⚠ garnish: "), "{out}");
    let (out, code) = piped(&["config", "--no-such-flag"], home, &[]);
    assert!(code == Some(2) && out.is_empty(), "{out}");
    let (out, code) = piped(&["--version"], home, &[]);
    assert!(code == Some(0) && out.starts_with("garnish "), "{out}");
    // At a terminal a typo is clap's error as usual.
    let (out, code) = piped(&["--confg"], home, &[("GARNISH_STDIN_TTY", "1")]);
    assert!(code == Some(2) && out.is_empty(), "{out}");
    let (out, code) = piped(&[], home, &[("GARNISH_TEST_PANIC", "1")]);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out, "⚠ garnish: internal error\n");
    let (out, code) = piped(&["render"], home, &[("GARNISH_TEST_PANIC", "1")]);
    assert_eq!((out.as_str(), code), ("⚠ garnish: internal error\n", Some(0)));
}

/// SPEC § 9: `GARNISH_MANAGED_SETTINGS` names the managed settings file,
/// first in Claude Code's chain, or, empty, says there is none; `config
/// show`, `doctor` and the tick read the chain through it.
#[test]
fn managed_settings_hook_names_the_first_file_of_the_chain() {
    let dir = tempfile::tempdir().unwrap();
    // Canonical, so the home, the working directory `doctor` reports and
    // the hook's path agree on macOS (`/var` is a link to `/private/var`).
    let home = dir.path().canonicalize().unwrap();
    let home = home.as_path();
    let cfg = home.join("garnish.toml");
    std::fs::write(&cfg, "[[line]]\nmodules = [\"context\"]\n").unwrap();
    let managed = home.join("managed.json");
    std::fs::write(
        &managed,
        r#"{"prefersReducedMotion": true, "autoCompactEnabled": false, "statusLine": {"command": "org-garnish"}}"#,
    )
    .unwrap();
    // The user file says the opposite on every key; the managed file wins.
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/settings.json"),
        r#"{"prefersReducedMotion": false, "autoCompactEnabled": true}"#,
    )
    .unwrap();
    let hook = [("GARNISH_MANAGED_SETTINGS", managed.to_str().unwrap())];
    let show = |extra: &[(&str, &str)]| {
        let (shown, _, ok) =
            run(&["--config", cfg.to_str().unwrap(), "config", "show"], home, extra);
        assert!(ok, "{shown}");
        shown
    };
    assert!(show(&hook).contains("\nanimate = false\n"), "the managed file freezes");
    assert!(show(&[]).contains("\nanimate = true\n"), "the hook left empty: no managed file");
    let (report, _, ok) = run(&["--config", cfg.to_str().unwrap(), "doctor"], home, &hook);
    assert!(ok, "{report}");
    // The file sits under the project directory (the test's home), but
    // only the project's own files are named relative to it: this one is
    // under the home, so it is `~/`.
    assert!(report.contains("  managed  ~/managed.json  ok"), "{report}");
    assert!(report.contains("command=org-garnish (managed)"), "{report}");
    assert!(report.contains("true (managed)"), "{report}");
    assert!(report.contains("GARNISH_MANAGED_SETTINGS=~/managed.json"), "{report}");
    let (report, _, ok) = run(&["--config", cfg.to_str().unwrap(), "doctor"], home, &[]);
    assert!(ok, "{report}");
    assert!(!report.lines().any(|l| l.starts_with("  managed")), "{report}");
    // The home is the project directory here, so the one settings file is
    // both the project's and the user's, and the project entry names it.
    assert!(report.contains("not configured") && report.contains("false (project)"), "{report}");
    // The tick reads the same chain: the managed file switches the
    // compaction marker off, the user file alone leaves it on.
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    let plain = |extra: &[(&str, &str)]| tick(&cfg, home, payload, extra);
    assert_ne!(plain(&hook), plain(&[]), "the managed file changes the tick");
}

/// SPEC § 5, § 9: `GARNISH_DEBUG` appends a line per tick to
/// `<cache>/debug.log` and `doctor` shows its tail; with the hook off the
/// file is never created, and nothing the hook does reaches stdout.
///
/// Serial: the hook and the cache root are per-process environment, and the
/// log is shared with whatever else writes to that root.
#[test]
fn cache_debug_hook_logs_one_line_per_tick_and_nothing_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let cfg = home.join("garnish.toml");
    std::fs::write(&cfg, "[[line]]\nmodules = [\"model\"]\n").unwrap();
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    let log = home.join("cache").join("debug.log");

    let quiet = tick(&cfg, home, payload, &[]);
    assert!(!log.exists(), "the hook is off: {}", log.display());

    let noisy = tick(&cfg, home, payload, &[("GARNISH_DEBUG", "1")]);
    assert_eq!(noisy, quiet, "the hook must not change what a tick prints");
    tick(&cfg, home, payload, &[("GARNISH_DEBUG", "1")]);
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text.lines().count(), 2, "one line per tick: {text}");
    for line in text.lines() {
        assert!(line.contains(" pid="), "{line}");
        assert!(line.contains("columns=84"), "{line}");
        assert!(line.contains("rows=1"), "{line}");
    }

    let (report, _, ok) =
        run(&["doctor"], home, &[("GARNISH_CACHE_DIR", home.join("cache").to_str().unwrap())]);
    assert!(ok, "{report}");
    assert!(report.contains("debug.log (last 2 of 2 lines)"), "{report}");
    assert!(report.contains("GARNISH_CACHE_DIR="), "{report}");
}

/// no-color.org: `NO_COLOR` turns colour off when it is "present and not
/// an empty string"; an empty one (a profile's `export NO_COLOR=`) used to
/// turn it off too, links included, under `color = "auto"`.
#[test]
fn an_empty_no_color_leaves_colour_on() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let cfg = home.join("garnish.toml");
    std::fs::write(&cfg, "[[line]]\nmodules = [\"model\"]\n").unwrap();
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    assert!(tick(&cfg, home, payload, &[("NO_COLOR", "")]).contains('\x1b'), "empty is unset");
    assert!(!tick(&cfg, home, payload, &[("NO_COLOR", "1")]).contains('\x1b'), "set is off");
    let fixture =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads/subscription-full.json");
    let args = ["--config", cfg.to_str().unwrap(), "preview", fixture, "--width", "84"];
    let (out, _, ok) = run(&args, home, &[("NO_COLOR", "")]);
    let rows: String = out.lines().skip(1).collect();
    assert!(ok && rows.contains("38;2;"), "preview too: {out:?}");
}

/// SPEC § 2.1: `preview` paints every row faint, as Claude Code draws the
/// status line on screen; the tick does not, since the harness adds it.
#[test]
fn preview_draws_every_row_faint_and_the_tick_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let cfg = home.join("garnish.toml");
    // 256-colour mode keeps a `2` parameter unambiguous (truecolor carries
    // `38;2;r;g;b`), and `model` has no faint styling of its own.
    std::fs::write(
        &cfg,
        "color = \"256\"\n[frame]\nstyle = \"none\"\n[[line]]\nmodules = [\"model\"]\n",
    )
    .unwrap();
    let payload =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/payloads/subscription-full.json");
    // Every SGR parameter list in `text` but the reset.
    let sgr = |text: &str| -> Vec<String> {
        text.split("\x1b[")
            .skip(1)
            .filter_map(|rest| rest.split_once('m').map(|(params, _)| params.to_owned()))
            .filter(|params| params != "0")
            .collect()
    };
    let faint = |params: &str| params.split(';').take_while(|p| *p != "38").any(|p| p == "2");
    let args = ["--config", cfg.to_str().unwrap(), "preview", payload, "--width", "84"];
    let (out, _, ok) = run(&args, home, &[]);
    assert!(ok, "{out}");
    // The first line is preview's own `── name` heading.
    let params: Vec<String> = out.lines().skip(1).flat_map(sgr).collect();
    assert!(!params.is_empty() && params.iter().all(|p| faint(p)), "{out:?}");
    let out = tick(&cfg, home, include_str!("fixtures/payloads/subscription-full.json"), &[]);
    let params = sgr(&out);
    assert!(!params.is_empty() && !params.iter().any(|p| faint(p)), "{out:?}");
}

/// CLAUDE.md § Conventions: a path variable that is *set but empty* means
/// unset, which is the shell's `FOO= cmd` idiom.
///
/// Both halves used to break, and neither was covered: `GARNISH_CONFIG=`
/// named the empty path, so every tick carried `⚠ config: cannot read` and
/// the real config went unread; `XDG_CONFIG_HOME=` made the candidate the
/// *relative* `garnish/garnish.toml`, so a checkout holding that file became
/// the config for every session started in it. The whole suite used
/// `env_remove` and so could not see either.
#[test]
fn an_empty_path_variable_is_unset_not_a_path() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    let marker = |name: &str| {
        format!("[[line]]\nmodules = [\"text.marker\"]\n[modules.text.marker]\ntext = \"{name}\"\n")
    };
    // `--config` is deliberately not passed: it short-circuits `locate`
    // before either variable is read, which is what made the first version
    // of this test pass with the rule reverted.
    let render = |extra: &[(&str, &str)]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
        cmd.current_dir(home)
            .env("HOME", home)
            .env("GARNISH_CACHE_DIR", home.join("cache"))
            .env("GARNISH_NOW", "1738425600")
            .env("GARNISH_NO_SPAWN", "1")
            .env("GARNISH_MANAGED_SETTINGS", "")
            .env("NO_COLOR", "1")
            .env("COLUMNS", "84")
            .env_remove("GARNISH_ANIMATE")
            .env_remove("CLAUDE_CONFIG_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    // The real config, found through `XDG_CONFIG_HOME`.
    let xdg = home.join(".config");
    std::fs::create_dir_all(xdg.join("garnish")).unwrap();
    std::fs::write(xdg.join("garnish").join("garnish.toml"), marker("REAL")).unwrap();
    let out = render(&[("XDG_CONFIG_HOME", xdg.to_str().unwrap())]);
    assert!(out.contains("REAL"), "the fixture itself is wrong: {out:?}");

    // `GARNISH_CONFIG=` named the *empty path*, so the tick printed
    // `⚠ config:  cannot read` and never consulted the real config.
    let out = render(&[("XDG_CONFIG_HOME", xdg.to_str().unwrap()), ("GARNISH_CONFIG", "")]);
    assert!(!out.contains("config"), "an empty variable named a file: {out:?}");
    assert!(out.contains("REAL"), "the real config was skipped: {out:?}");

    // `XDG_CONFIG_HOME=` made the candidate the *relative* path
    // `garnish/garnish.toml`, so a working directory holding one became the
    // config for every session started there.
    let trap = home.join("garnish");
    std::fs::create_dir_all(&trap).unwrap();
    std::fs::write(trap.join("garnish.toml"), marker("TRAP")).unwrap();
    let out = render(&[("XDG_CONFIG_HOME", "")]);
    assert!(!out.contains("TRAP"), "the working directory became the config: {out:?}");

    // A *relative* XDG_CONFIG_HOME is the same trap, and the XDG Base
    // Directory spec says to ignore one: the candidate is taken from the
    // working directory, which for a tick is the session's repository.
    let rel = home.join("rel").join("garnish");
    std::fs::create_dir_all(&rel).unwrap();
    std::fs::write(rel.join("garnish.toml"), marker("RELATIVE")).unwrap();
    let out = render(&[("XDG_CONFIG_HOME", "rel")]);
    assert!(!out.contains("RELATIVE"), "a relative base named a config: {out:?}");
    let (out, _, ok) = run(&["config", "path"], home, &[("XDG_CONFIG_HOME", "rel")]);
    assert!(ok && !out.contains("rel/garnish"), "{out}");
    assert!(out.trim_end().ends_with(".config/garnish/garnish.toml"), "{out}");
}

/// SPEC § 7: `CLAUDE_CONFIG_DIR` moves every `~/.claude` path, so `install`
/// writes the settings file and the skills where Claude Code reads them,
/// `skills install` defaults there, and the settings chain (`config show`,
/// `doctor`, the tick) reads the user file from there; empty is unset.
#[test]
fn claude_config_dir_moves_the_user_settings_and_the_skills() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let moved = home.join("work-claude");
    let env = [("CLAUDE_CONFIG_DIR", moved.to_str().unwrap())];
    let (out, _, ok) = run(&["install", "--dry-run", "--absolute"], home, &env);
    assert!(ok, "{out}");
    let settings = moved.join("settings.json");
    assert!(out.contains(&format!("would write {}", settings.display())), "{out}");
    assert!(out.contains(&format!("to {}", moved.join("skills").display())), "{out}");
    let (out, _, ok) =
        run(&["install", "--dry-run", "--absolute"], home, &[("CLAUDE_CONFIG_DIR", "")]);
    let default = home.join(".claude").join("settings.json");
    assert!(ok && out.contains(&default.display().to_string()), "empty is unset: {out}");
    let (out, _, ok) = run(&["skills", "install"], home, &env);
    assert!(ok && out.contains("work-claude/skills: wrote 3"), "{out}");
    assert!(!home.join(".claude").exists(), "nothing under ~/.claude");
    // The chain reads the moved user file: its prefersReducedMotion
    // freezes an unset `animate`, and without the variable it is not read.
    std::fs::write(&settings, r#"{"prefersReducedMotion": true}"#).unwrap();
    let cfg = home.join("g.toml");
    std::fs::write(&cfg, "preset = \"minimal\"\n").unwrap();
    let show = ["--config", cfg.to_str().unwrap(), "config", "show"];
    let (shown, _, ok) = run(&show, home, &env);
    assert!(ok && shown.contains("\nanimate = false\n"), "{shown}");
    let (shown, _, ok) = run(&show, home, &[]);
    assert!(ok && shown.contains("\nanimate = true\n"), "{shown}");
    let (report, _, ok) = run(&["doctor"], home, &env);
    assert!(ok && report.contains("work-claude/settings.json  ok"), "{report}");
    assert!(report.contains("true (user)"), "{report}");
}

/// One tick through `sh -c <command>`, as Claude Code runs
/// `statusLine.command`, in the hermetic environment of [`run`].
fn sh_tick(command: &str, home: &Path) -> String {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", command])
        .current_dir(home)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("GARNISH_NOW", "1738425600")
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env("NO_COLOR", "1")
        .env_remove("GARNISH_CONFIG")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
    String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
}

/// SPEC § 7: `install` with an explicit config (`--config`, else
/// `GARNISH_CONFIG`) writes a command that passes it, quoted for the
/// shell, so the tick reads the file it was set up for; a reinstall
/// without one replaces the program word only and keeps the arguments.
#[test]
fn install_passes_an_explicit_config_and_a_reinstall_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let work = home.join("my configs").join("work.toml");
    std::fs::create_dir_all(work.parent().unwrap()).unwrap();
    let marker = "[[line]]\nmodules = [\"text.m\"]\n[modules.text.m]\ntext = \"WORKFILE\"\n";
    std::fs::write(&work, marker).unwrap();
    let settings = home.join(".claude").join("settings.json");
    let command = || -> String {
        let text = std::fs::read_to_string(&settings).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["statusLine"]["command"].as_str().unwrap().to_owned()
    };
    let args = ["--config", work.to_str().unwrap(), "install", "--no-skills", "--absolute"];
    let (out, _, ok) = run(&args, home, &[]);
    assert!(ok, "{out}");
    let written = command();
    let quoted = format!(" --config '{}'", work.display());
    assert!(written.ends_with(&quoted), "{written}");
    assert!(sh_tick(&written, home).contains("WORKFILE"), "the tick reads work.toml");
    // `install --padding 1` (the README's fix for cut rows) used to reset
    // the command to a bare `garnish`, which reads another config.
    let (out, _, ok) =
        run(&["install", "--no-skills", "--no-config", "--absolute", "--padding", "1"], home, &[]);
    assert!(ok, "{out}");
    assert_eq!(command(), written);
    let (out, _, ok) = run(
        &["install", "--no-skills", "--no-config", "--dry-run"],
        home,
        &[("GARNISH_CONFIG", work.to_str().unwrap())],
    );
    assert!(ok && out.contains("\"command\": \"garnish --config '"), "{out}");
}

/// SPEC § 4: `~/.garnish.toml` is the config when there is no XDG file,
/// and the commands that write a config write *that* file rather than
/// creating an XDG one that would hide it from the next tick.
#[test]
fn a_home_dot_file_is_the_config_the_writing_commands_see() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let dot = home.join(".garnish.toml");
    let xdg = home.join(".config/garnish/garnish.toml");
    let marker = "[[line]]\nmodules = [\"text.m\"]\n[modules.text.m]\ntext = \"DOTFILE\"\n";
    std::fs::write(&dot, marker).unwrap();
    let (out, _, ok) = run(&["config", "path"], home, &[]);
    assert!(ok && out.trim_end() == dot.to_str().unwrap(), "{out}");
    // `install` keeps it, and writes no default config over it.
    let (out, _, ok) = run(&["install", "--no-skills", "--absolute"], home, &[]);
    assert!(ok && !out.contains("default config"), "{out}");
    assert!(!xdg.exists(), "an XDG config would hide ~/.garnish.toml");
    let payload = include_str!("fixtures/payloads/subscription-full.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_garnish"));
    cmd.current_dir(home)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GARNISH_CACHE_DIR", home.join("cache"))
        .env("GARNISH_NO_SPAWN", "1")
        .env("GARNISH_MANAGED_SETTINGS", "")
        .env_remove("GARNISH_CONFIG")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
    let tick = String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned();
    assert!(tick.contains("DOTFILE"), "{tick}");
    // `config init` refuses to replace it, naming it.
    let (_, err, ok) = run(&["config", "init"], home, &[]);
    assert!(!ok && err.contains(".garnish.toml exists"), "{err}");
    // `setup --preset` edits it in place, with the backup next to it.
    let (out, _, ok) = run(&["setup", "--preset", "compact"], home, &[]);
    assert!(ok && out.contains(".garnish.toml (backup: "), "{out}");
    assert!(std::fs::read_to_string(&dot).unwrap().contains("preset = \"compact\""));
    let backups = std::fs::read_dir(home)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".garnish.toml.bak-"))
        .count();
    assert_eq!(backups, 1);
    assert!(!xdg.exists(), "no XDG config was created");
}
