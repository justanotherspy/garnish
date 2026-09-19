//! Badges from Claude Code's own files (SPEC § 3.8): `sandbox` and `voice`
//! from the settings chain, `account` from `~/.claude.json`.
//!
//! The two switches ride on the one settings read a tick makes
//! (`Ctx::settings`), so a config without them reads nothing new. The
//! account is the one cached module outside the repo group: the file can
//! be hundreds of KB, so a worker reads it and the tick reads the entry.

use std::collections::BTreeMap;
use std::path::Path;

use crate::ansi::Segment;
use crate::cache::Scope;
use crate::claude_settings::{self, FileKeys};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::glyph;

use super::{Ctx, Module, RefreshCtx, Rendered, lead, lead_only, seg};

/// How a settings badge shows: the glyph alone, or the glyph and its word.
const BADGE_STYLES: &[&str] = &["glyph", "word"];

/// What `account` prints: the whole address, or the part before `@`.
const ACCOUNT_STYLES: &[&str] = &["email", "user"];

/// Most bytes the `account` worker reads from `.claude.json` (SPEC § 5);
/// a longer file is a failed entry, never a partial parse.
pub const MAX_CLAUDE_JSON_BYTES: u64 = 8 << 20;

/// Most characters of the email kept in the entry: the file is outside
/// garnish, and the tick renders the value on every row it is placed on.
pub const MAX_EMAIL_CHARS: usize = 254;

/// The schema the two settings badges share: `id` is the word, the icon
/// key and the settings table.
fn badge_schema(
    id: &'static str,
    summary: &'static str,
    doc: &'static str,
    sources: &'static [&'static str],
    icon: IconSpec,
) -> ModuleSchema {
    ModuleSchema {
        id,
        measure: None,
        summary,
        doc,
        sources,
        refresh: 0,
        opts: vec![
            OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true)),
            OptSpec::new(
                "style",
                Kind::Enum(BADGE_STYLES),
                "`glyph` shows the icon alone; `word` adds the module's name after it.",
                Value::Str("glyph".into()),
            )
            .full(Value::Str("word".into())),
        ],
        icons: vec![icon],
        colors: vec![
            ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
            ColorSpec { key: "word", doc: "The word, under `style = \"word\"`.", default: "text" },
        ],
    }
}

/// A settings badge for one tick: nothing unless the first file of the
/// chain that sets the switch sets it to `true` (resolved as
/// `prefersReducedMotion` is, SPEC § 4.2); then the glyph, or the glyph
/// and the word.
fn settings_badge(
    ctx: &Ctx<'_>,
    cfg: &ModuleCfg,
    key: &str,
    pick: impl Fn(&FileKeys) -> Option<bool>,
) -> Rendered {
    if !claude_settings::flag(ctx.settings(), pick) {
        return Rendered::empty();
    }
    if cfg.str("style") == "word" {
        let mut segs = lead(cfg, key);
        segs.push(seg(cfg, key, "word"));
        Rendered::fresh(segs)
    } else {
        Rendered::fresh(lead_only(cfg, key))
    }
}

/// `sandbox`: a badge while Bash sandboxing is on.
pub struct SandboxModule;

impl Module for SandboxModule {
    fn schema(&self) -> ModuleSchema {
        badge_schema(
            "sandbox",
            "A badge while Bash sandboxing is on.",
            "Shows while `sandbox.enabled` is `true` in Claude Code's settings chain (the first file that sets it wins, as for `prefersReducedMotion`): Bash commands run isolated from the filesystem and the network. Nothing otherwise, so `hide_when_empty = false` prints `–` as it does for any absent value.",
            &[".claude/settings.json sandbox.enabled (the settings chain)"],
            IconSpec {
                key: "sandbox",
                doc: "Sandbox glyph.",
                glyph: glyph("\u{f023}", "⊡", "🔒", "[]"),
            },
        )
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        settings_badge(ctx, cfg, "sandbox", |k| k.sandbox_enabled)
    }
}

/// `voice`: a badge while voice dictation is on.
pub struct VoiceModule;

impl Module for VoiceModule {
    fn schema(&self) -> ModuleSchema {
        badge_schema(
            "voice",
            "A badge while voice dictation is on.",
            "Shows while `voice.enabled` is `true` in Claude Code's settings chain (`/voice` writes it to the user file). Claude Code drops its own `hold space to speak` hint once a custom status line is configured, which is what this badge stands in for. Nothing otherwise, so `hide_when_empty = false` prints `–`.",
            &[".claude/settings.json voice.enabled (the settings chain)"],
            IconSpec {
                key: "voice",
                doc: "Voice glyph.",
                glyph: glyph("\u{f130}", "∿", "🎤", "mic"),
            },
        )
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        settings_badge(ctx, cfg, "voice", |k| k.voice_enabled)
    }
}

/// `account`: the email of the claude.ai sign-in, from `~/.claude.json`.
pub struct AccountModule;

impl Module for AccountModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "account",
            measure: None,
            summary: "The claude.ai account the session is signed in with.",
            doc: "The `oauthAccount.emailAddress` of `~/.claude.json` (`$CLAUDE_CONFIG_DIR/.claude.json` when that is set), the file Claude Code keeps for itself. It can be hundreds of KB, so a background worker reads it every `refresh` seconds and the tick shows the cached value: the first tick after a session starts shows nothing. A file without the field (an API-key session) shows nothing either; a file that cannot be read or parsed is a failed entry, marked `✗`.",
            sources: &["~/.claude.json oauthAccount.emailAddress (worker)"],
            refresh: 600,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true)),
                OptSpec::new(
                    "style",
                    Kind::Enum(ACCOUNT_STYLES),
                    "`email` shows the whole address; `user` the part before `@`.",
                    Value::Str("email".into()),
                )
                .minimal(Value::Str("user".into())),
            ],
            icons: vec![IconSpec {
                key: "account",
                doc: "Account icon.",
                glyph: glyph("\u{f007}", "@", "👤", "@"),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "name", doc: "The address or user.", default: "text" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let scope = Scope::Session(ctx.session_id().to_owned());
        let (lookup, freshness) = ctx.cached(cfg, &scope, |_| true);
        let Some(entry) = lookup.entry else { return Rendered::empty() };
        // An `ok` entry without the line is a session with no account
        // (nothing to show); a failed entry has no value either and keeps
        // its freshness, so `decorate` puts the mark on the row.
        let Some(email) = entry.get("email").filter(|e| !e.is_empty()) else {
            return Rendered { segments: Vec::new(), freshness, measure: None };
        };
        let shown = if cfg.str("style") == "user" {
            email.split_once('@').map_or(email, |(user, _)| user)
        } else {
            email
        };
        let mut segs: Vec<Segment> = lead(cfg, "account");
        segs.push(seg(cfg, shown, "name"));
        Rendered { segments: segs, freshness, measure: None }
    }

    fn refresh(&self, _ctx: &RefreshCtx<'_>) -> Result<BTreeMap<String, String>, String> {
        let path = claude_settings::claude_json_path(claude_settings::home_dir().as_deref())
            .ok_or_else(|| "no home directory: HOME is not set".to_owned())?;
        read_account(&path)
    }
}

/// The `account` entry for the `.claude.json` at `path`: an `email` value
/// when the file carries `oauthAccount.emailAddress`, nothing when it
/// does not or when there is no file at all.
///
/// # Errors
/// A file that cannot be read, is longer than [`MAX_CLAUDE_JSON_BYTES`] or
/// is not a JSON object: the text of a failed entry, retried once per TTL.
pub fn read_account(path: &Path) -> Result<BTreeMap<String, String>, String> {
    use std::io::Read as _;
    let shown = path.display();
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("{shown}: {e}")),
    };
    // Bytes first, one past the cap, as `claude_settings::read_file` reads
    // a settings file: an over-long file is told from one at the cap, and
    // never parsed.
    let mut bytes = Vec::new();
    file.take(MAX_CLAUDE_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{shown}: {e}"))?;
    if u64::try_from(bytes.len()).is_ok_and(|n| n > MAX_CLAUDE_JSON_BYTES) {
        return Err(format!(
            "{shown}: longer than the {MAX_CLAUDE_JSON_BYTES} bytes garnish reads"
        ));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("{shown}: not valid JSON: {e}"))?;
    let serde_json::Value::Object(map) = json else {
        return Err(format!("{shown}: not a JSON object"));
    };
    let email = map
        .get("oauthAccount")
        .and_then(serde_json::Value::as_object)
        .and_then(|account| account.get("emailAddress"))
        .and_then(serde_json::Value::as_str)
        .map(|email| {
            crate::ansi::plain_text(email).chars().take(MAX_EMAIL_CHARS).collect::<String>()
        })
        .filter(|email| !email.is_empty());
    Ok(email.map(|email| ("email".to_owned(), email)).into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;
    use crate::cache::{Cache, Entry};
    use crate::render::{Clock, render_plain_at};

    /// One module alone on an unframed line under a clock, plain.
    fn row(id: &str, extra: &str, clock: &Clock) -> String {
        let payload = crate::fixtures::payload("subscription-full");
        let text = format!(
            "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"{id}\"]\n[modules.{id}]\n{extra}"
        );
        let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        strip_ansi(&render_plain_at(&payload, &config, Some(80), clock)).trim_end().to_owned()
    }

    /// A pinned clock whose settings chain says what `keys` says.
    fn seeded(sandbox: Option<bool>, voice: Option<bool>) -> Clock {
        let keys =
            FileKeys { sandbox_enabled: sandbox, voice_enabled: voice, ..Default::default() };
        Clock { settings_keys: Some(vec![keys]), ..Clock::fixed() }
    }

    /// SPEC § 3.8: a badge shows only while its switch is on, as the glyph
    /// alone (one cell, no trailing space) or with its word; the seeded
    /// keys stand in for the chain, and the fixed clock alone shows neither.
    #[test]
    fn settings_badges_follow_their_switches() {
        assert_eq!(row("sandbox", "", &seeded(Some(true), None)), "⊡");
        assert_eq!(row("voice", "", &seeded(None, Some(true))), "∿");
        assert_eq!(row("sandbox", "", &seeded(None, Some(true))), "");
        assert_eq!(row("voice", "", &seeded(Some(true), Some(false))), "");
        assert_eq!(row("sandbox", "", &Clock::fixed()), "");
        assert_eq!(row("voice", "", &Clock::fixed()), "");
        let on = seeded(Some(true), Some(true));
        assert_eq!(row("sandbox", "style = \"word\"\n", &on), "⊡ sandbox");
        assert_eq!(row("voice", "preset = \"full\"\n", &on), "∿ voice");
        assert_eq!(row("sandbox", "style = \"word\"\nshow_icon = false\n", &on), "sandbox");
        assert_eq!(row("voice", "show_icon = false\n", &on), "", "no glyph, no word: nothing");
        assert_eq!(row("sandbox", "hide_when_empty = false\n", &seeded(Some(false), None)), "–");
        // A blank glyph override leaves no lone space behind.
        assert_eq!(row("sandbox", "[modules.sandbox.icons]\nsandbox = \"\"\n", &on), "");
    }

    /// SPEC § 3.8: the worker's read of `.claude.json`: the email when the
    /// field is there, an empty `ok` value without it or without the file,
    /// a failure for anything that cannot be read as a JSON object.
    #[test]
    fn account_reads_the_email_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".claude.json");
        let email = |text: &str| {
            std::fs::write(&file, text).unwrap();
            read_account(&file)
        };
        assert_eq!(read_account(&file), Ok(BTreeMap::new()), "no file: no account");
        let mut expected = BTreeMap::new();
        expected.insert("email".to_owned(), "dev@example.com".to_owned());
        assert_eq!(
            email(
                r#"{"numStartups": 3, "oauthAccount": {"accountUuid": "u", "emailAddress": "dev@example.com"}, "projects": {}}"#
            ),
            Ok(expected)
        );
        for empty in [
            "{}",
            r#"{"oauthAccount": {}}"#,
            r#"{"oauthAccount": {"emailAddress": ""}}"#,
            r#"{"oauthAccount": {"emailAddress": 7}}"#,
            r#"{"oauthAccount": "dev@example.com"}"#,
            r#"{"oauthAccount": null}"#,
        ] {
            assert_eq!(email(empty), Ok(BTreeMap::new()), "{empty}");
        }
        assert!(email("{ broken").is_err_and(|e| e.contains("not valid JSON")));
        assert!(email("[1]").is_err_and(|e| e.contains("not a JSON object")));
        assert!(email("").is_err_and(|e| e.contains("not valid JSON")));
        assert!(read_account(dir.path()).is_err(), "a directory cannot be read");
        // The cap: one byte over is a failure, at the cap the file is read.
        let body = r#"{"oauthAccount": {"emailAddress": "dev@example.com"}, "pad": ""#;
        let tail = "\"}";
        let room = usize::try_from(MAX_CLAUDE_JSON_BYTES).unwrap() - body.len() - tail.len();
        let at_cap = format!("{body}{}{tail}", "x".repeat(room));
        assert_eq!(email(&at_cap).map(|v| v.len()), Ok(1));
        assert!(
            email(&format!("{body}{}{tail}", "x".repeat(room + 1)))
                .is_err_and(|e| e.contains("longer than"))
        );
        // The value is bounded and plain: an escape sequence in the field
        // never reaches an entry, nor does an endless address.
        let hostile =
            format!(r#"{{"oauthAccount": {{"emailAddress": "a\u001b[31m@{}"}}}}"#, "x".repeat(400));
        let value = email(&hostile).unwrap();
        let value = value.get("email").unwrap();
        assert!(!value.contains('\u{1b}') && value.chars().count() == MAX_EMAIL_CHARS, "{value:?}");
    }

    /// SPEC § 3.8: the tick shows the entry the worker wrote, the address
    /// or its user part, dimmed with the mark once overdue and marked `✗`
    /// when the worker failed; without an entry, or an entry without the
    /// line, nothing.
    #[test]
    fn account_renders_its_cache_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("cache"));
        let payload = crate::fixtures::payload("subscription-full");
        let scope = Scope::Session(payload.session_id.unwrap());
        let clock =
            Clock { workers: true, cache: Some(dir.path().join("cache")), ..Clock::fixed() };
        let at = |extra: &str| row("account", extra, &clock);
        // Every entry below is written now and fresh for its TTL, so no
        // render here misses and nothing is spawned.
        let mut values = BTreeMap::new();
        values.insert("email".to_owned(), "dev@example.com".to_owned());
        cache.write(&scope, "account", &Entry::ok(600_000, values)).unwrap();
        assert_eq!(at(""), "@ dev@example.com");
        assert_eq!(at("style = \"user\"\n"), "@ dev");
        assert_eq!(at("preset = \"minimal\"\nshow_icon = false\n"), "dev");
        cache.write(&scope, "account", &Entry::ok(600_000, BTreeMap::new())).unwrap();
        assert_eq!(at(""), "", "an entry without the line: no account");
        assert_eq!(at("hide_when_empty = false\n"), "–");
        cache.write(&scope, "account", &Entry::err(600_000, "boom")).unwrap();
        assert_eq!(at(""), "– ✗", "a failed worker is never silent");
        // Under the pinned clock nothing is looked up: the entry is there
        // and the row stays empty, and no cache directory is created.
        let pinned = Clock { cache: Some(dir.path().join("none")), ..Clock::fixed() };
        assert_eq!(row("account", "", &pinned), "");
        assert!(!dir.path().join("none").exists());
    }
}
