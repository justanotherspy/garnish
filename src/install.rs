//! `garnish install`: wiring garnish into Claude Code's `settings.json`.
//!
//! The settings file is JSON with many unrelated keys, so garnish only
//! touches the `statusLine` object (keeping any keys it does not own, such as
//! `hideVimModeIndicator`), backs the file up first, and writes atomically.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::config::WriteTarget;

/// What `install` should write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The settings file.
    pub settings: PathBuf,
    /// The program word of `statusLine.command`, ready for a shell:
    /// `garnish`, or the path this binary is known by (`--absolute`).
    pub program: String,
    /// The config file named explicitly (`--config`, else
    /// `GARNISH_CONFIG`), absolute: the command then passes it with
    /// `--config`, so the tick reads the file `install` was pointed at.
    pub config: Option<PathBuf>,
    /// `statusLine.refreshInterval` in seconds.
    pub refresh_interval: u64,
    /// `statusLine.padding`, when given.
    pub padding: Option<u64>,
}

impl Plan {
    /// The `statusLine.command` this plan writes over `existing`, the
    /// command already in the file.
    ///
    /// With an explicit config it is `<program> --config <path>`. Otherwise
    /// a command that already runs garnish ([`garnish_at`]) keeps its
    /// environment prefix and its arguments and only its program word is
    /// replaced, so a `--config` written by hand survives a reinstall
    /// (`install --padding 1`, say); anything else becomes the program
    /// alone.
    #[must_use]
    pub fn command(&self, existing: Option<&str>) -> String {
        if let Some(config) = &self.config {
            let config = shell_quote(&config.to_string_lossy());
            return format!("{} --config {config}", self.program);
        }
        existing
            .and_then(|existing| {
                let words = shell_words(existing);
                let program = words.get(garnish_at(&words, existing)?)?;
                let prefix = existing.get(words.first()?.start..program.start)?;
                let args = existing.get(program.end..)?;
                Some(format!("{prefix}{}{args}", self.program))
            })
            .unwrap_or_else(|| self.program.clone())
    }
}

/// `word` as one POSIX shell word.
///
/// As it is when it is made only of characters no shell treats specially,
/// else single-quoted (an embedded `'` written `'\''`). `statusLine.command`
/// runs through a shell, so a path with a space or a quote in it must reach
/// the program whole.
#[must_use]
pub fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '+' | '-'));
    if plain { word.to_owned() } else { format!("'{}'", word.replace('\'', r"'\''")) }
}

/// One word of a shell command line, as `sh` splits it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Word {
    /// The word with its quotes and escapes taken out, and without the
    /// home directory [`Word::home`] places in it.
    text: String,
    /// Where the word starts in the command line, in bytes.
    start: usize,
    /// Where it ends, exclusive.
    end: usize,
    /// Whether the shell passes the word on as `text` (after the home
    /// directory) spells it: every quote closed, nothing expanded, globbed
    /// or redirected.
    literal: bool,
    /// Where in `text` (a byte offset) the shell puts the home directory:
    /// an unquoted `~` alone or before a `/` at the word's start, or one
    /// `$HOME` or `${HOME}` anywhere outside single quotes, which `sh`
    /// expands wherever it stands (`$HOME.x` is the home directory with
    /// `.x` after it, not a file inside it). A second `$HOME` makes the
    /// word not [`Word::literal`].
    home: Option<usize>,
    /// The home directory came from a `$HOME` outside double quotes, which
    /// `sh` field-splits and globs, where a `~` or a quoted `"$HOME"` it
    /// passes whole.
    home_split: bool,
}

impl Word {
    /// The word as the shell passes it, the home directory spliced in at
    /// [`Word::home`]; `None` when it has a home and `home` is `None`, or
    /// when an unquoted `$HOME` would be split or globbed (a home with a
    /// blank or `*?[` in it) into something other than one file.
    fn expanded(&self, home: Option<&Path>) -> Option<std::ffi::OsString> {
        let Some(at) = self.home else { return Some(self.text.clone().into()) };
        let home = home?;
        let splits = |b: &u8| matches!(b, b' ' | b'\t' | b'\n' | b'*' | b'?' | b'[');
        if self.home_split && home.as_os_str().as_encoded_bytes().iter().any(splits) {
            return None;
        }
        let (before, after) = self.text.split_at_checked(at)?;
        let mut word = std::ffi::OsString::from(before);
        word.push(home);
        word.push(after);
        Some(word)
    }
}

/// The words of `command` up to the end of its first simple command (an
/// unquoted `;`, `|`, `&`, newline or leading `#`): enough to find the
/// program a `statusLine.command` runs and what it passes it.
///
/// Words end at `sh`'s blanks alone (space, tab, newline: a non-breaking
/// space or a carriage return is part of a word). Quotes and backslashes
/// are taken out as `sh` takes them out, and where the home directory goes
/// is recorded ([`Word::home`]); any other expansion makes the word not
/// [`Word::literal`], so nothing is ever read as a path the shell would
/// not pass.
fn shell_words(command: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut word: Option<Word> = None;
    let mut quote: Option<char> = None;
    let mut chars = command.char_indices().peekable();
    let ends = |c: char| matches!(c, ' ' | '\t' | '\n' | ';' | '|' | '&');
    while let Some((at, c)) = chars.next() {
        if quote.is_none() && (ends(c) || (c == '#' && word.is_none())) {
            if let Some(mut done) = word.take() {
                done.end = at;
                words.push(done);
            }
            if matches!(c, ';' | '|' | '&' | '\n' | '#') {
                return words;
            }
            continue;
        }
        let started = word.is_some();
        let w = word.get_or_insert_with(|| Word {
            text: String::new(),
            start: at,
            end: at,
            literal: true,
            home: None,
            home_split: false,
        });
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), c) => w.text.push(c),
            (None, '\'' | '"') => quote = Some(c),
            (None, '\\') => match chars.next() {
                Some((_, '\n')) => {}
                Some((_, next)) => w.text.push(next),
                None => w.literal = false,
            },
            (Some(_), '\\') => match chars.peek().map(|&(_, next)| next) {
                Some(next @ ('$' | '`' | '"' | '\\')) => {
                    chars.next();
                    w.text.push(next);
                }
                Some('\n') => {
                    chars.next();
                }
                _ => w.text.push('\\'),
            },
            (_, '$') => {
                let rest = command.get(at.saturating_add(1)..).unwrap_or_default();
                let name_ends = |tail: &str| {
                    tail.chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
                };
                // The characters after the `$` that spell the home directory.
                let home = if rest.starts_with("{HOME}") {
                    "{HOME}".len()
                } else if rest.strip_prefix("HOME").is_some_and(name_ends) {
                    "HOME".len()
                } else {
                    0
                };
                if home > 0 && w.home.is_none() {
                    chars.nth(home.saturating_sub(1));
                    w.home = Some(w.text.len());
                    w.home_split = quote.is_none();
                } else {
                    w.literal = false;
                    w.text.push(c);
                }
            }
            (None, '~') if !started => {
                if chars.peek().is_none_or(|&(_, next)| next == '/' || ends(next)) {
                    w.home = Some(0);
                } else {
                    // `~user`, which garnish does not look up.
                    w.literal = false;
                    w.text.push(c);
                }
            }
            (_, '`') | (None, '*' | '?' | '[' | '<' | '>' | '(' | ')' | '{' | '}') => {
                w.literal = false;
                w.text.push(c);
            }
            (_, c) => w.text.push(c),
        }
    }
    if let Some(mut last) = word {
        last.end = command.len();
        last.literal &= quote.is_none();
        words.push(last);
    }
    words
}

/// Whether a word as written is a shell assignment: an unquoted name, `=`,
/// then anything.
fn is_assignment(raw: &str) -> bool {
    raw.split_once('=').is_some_and(|(name, _)| {
        name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Where in `words` (the [`shell_words`] of `command`) the program word
/// is, when the program is garnish: `garnish` or a path ending in
/// `/garnish`, after any `NAME=value` words and a leading `env` (or a path
/// ending in `/env`) with the assignments after it. `None` for any other
/// program, `env` with an option included.
fn garnish_at(words: &[Word], command: &str) -> Option<usize> {
    let assignment = |w: &Word| command.get(w.start..w.end).is_some_and(is_assignment);
    // A name after a home directory must follow it whole (`~/bin/garnish`).
    let named = |w: &Word, name: &str| {
        let path = format!("/{name}");
        w.literal
            && w.home.map_or_else(
                || w.text == name || w.text.ends_with(&path),
                |at| w.text.get(at..).is_some_and(|rest| rest.ends_with(&path)),
            )
    };
    let mut rest = words.iter().enumerate().skip_while(|(_, w)| assignment(w));
    let (mut at, mut program) = rest.next()?;
    if named(program, "env") {
        (at, program) = rest.find(|(_, w)| !assignment(w))?;
    }
    named(program, "garnish").then_some(at)
}

/// Most characters of a `--config` word a note quotes: the word comes from
/// a settings file, which a project may carry, so a note stays one line's
/// worth (as `doctor` cuts the command it echoes).
const MAX_SHOWN_WORD_CHARS: usize = 200;

/// Why a value under the settings key `key` names no file, when it is an
/// `env` value: its `~` or `$HOME` is as literal as the rest.
fn expands_nothing(key: &str) -> &'static str {
    if key.starts_with("env.") {
        " (Claude Code expands nothing in a settings value, `~` included)"
    } else {
        ""
    }
}

/// `word` cut to [`MAX_SHOWN_WORD_CHARS`], with `…` when something was cut;
/// quoted with `{:?}` by the caller, which also keeps a control character
/// from reaching the terminal.
fn shown_word(word: &str) -> String {
    let mut shown: String = word.chars().take(MAX_SHOWN_WORD_CHARS).collect();
    if word.chars().nth(MAX_SHOWN_WORD_CHARS).is_some() {
        shown.push('…');
    }
    shown
}

/// What the `--config` of a garnish `statusLine.command` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandConfig {
    /// A file: an absolute path, or one under the home directory as the
    /// shell expands `~`, `$HOME` and `${HOME}`.
    File(PathBuf),
    /// A value that stands for no one file garnish can find, as written: a
    /// relative path, which the harness resolves in whatever directory it
    /// runs the command from, or an expansion garnish does not follow.
    Unresolved(String),
}

/// The config file a garnish `statusLine.command` passes with `--config`
/// (or `--config=`), else with a `GARNISH_CONFIG=` assignment before the
/// program, which is the file its ticks read; `None` when the command does
/// not run garnish ([`garnish_at`], or a program word the shell would
/// split) or passes no config. `home` is what `~`, `$HOME` and `${HOME}`
/// expand to ([`Word::home`]).
#[must_use]
pub fn command_config(command: &str, home: Option<&Path>) -> Option<CommandConfig> {
    let words = shell_words(command);
    let at = program_at(&words, command, home)?;
    let args = words.get(at.saturating_add(1)..)?;
    // The arguments as written: what a note quotes when clap would refuse
    // them, so that the ticks read no file at all.
    let span = || {
        let (first, last) = (args.first()?, args.last()?);
        command.get(first.start..last.end).map(|s| CommandConfig::Unresolved(s.to_owned()))
    };
    let (flags, refused) = config_flags(args, command);
    let (value, written) = match flags.as_slice() {
        [] => env_config(words.get(..at)?, command)?,
        [one] => one.clone(),
        // clap refuses a second `--config`.
        [_, _, ..] => return span(),
    };
    if refused {
        return span();
    }
    let path = value.literal.then(|| value.expanded(home)).flatten().map(PathBuf::from);
    Some(
        path.filter(|p| p.is_absolute())
            .map_or(CommandConfig::Unresolved(written), CommandConfig::File),
    )
}

/// Whether a `statusLine.command` runs garnish ([`garnish_at`]) from a
/// program word the shell passes whole.
#[must_use]
pub fn runs_garnish(command: &str, home: Option<&Path>) -> bool {
    program_at(&shell_words(command), command, home).is_some()
}

/// [`garnish_at`], unless the program word holds a home directory the
/// shell would split (or there is none to expand), so that it runs no
/// garnish at all.
fn program_at(words: &[Word], command: &str, home: Option<&Path>) -> Option<usize> {
    let at = garnish_at(words, command)?;
    let program = words.get(at)?;
    (program.home.is_none() || program.expanded(home).is_some()).then_some(at)
}

/// Each `--config` (or `--config=`) value among a garnish command's
/// arguments, with the value as written, and whether clap refuses the
/// arguments: a `--config` with no value, or anything after `--` (garnish
/// takes no positional argument).
fn config_flags(args: &[Word], command: &str) -> (Vec<(Word, String)>, bool) {
    const FLAG: &str = "--config=";
    let raw = |w: &Word| command.get(w.start..w.end).unwrap_or_default().to_owned();
    let plain = |w: &Word, text: &str| w.literal && w.home.is_none() && w.text == text;
    let mut found = Vec::new();
    let mut args = args.iter();
    while let Some(word) = args.next() {
        // A home directory among the flag's own letters makes it another
        // word (`$HOME--config`, `--config$HOME`).
        if word.home.is_some_and(|h| h < FLAG.len()) {
            continue;
        }
        if plain(word, "--") {
            return (found, args.next().is_some());
        }
        if plain(word, "--config") {
            let Some(value) = args.next() else { return (found, true) };
            found.push((value.clone(), raw(value)));
        } else if let Some(rest) = word.text.strip_prefix(FLAG) {
            let raw = raw(word);
            let written = raw.strip_prefix(FLAG).map_or_else(|| raw.clone(), str::to_owned);
            let home = word.home.map(|h| h.saturating_sub(FLAG.len()));
            found.push((Word { text: rest.to_owned(), home, ..word.clone() }, written));
        }
    }
    (found, false)
}

/// The value of the last `GARNISH_CONFIG=` assignment among the words
/// before the program (the shell's own rule), with the value as written;
/// `None` without one, or when it is empty, which garnish reads as unset.
/// A `~` in it is left to the shell (which expands one after the `=` or a
/// `:`), so the value names no one file garnish can find.
fn env_config(prefix: &[Word], command: &str) -> Option<(Word, String)> {
    const VAR: &str = "GARNISH_CONFIG=";
    let word = prefix.iter().rev().find(|w| {
        command.get(w.start..w.end).is_some_and(|raw| is_assignment(raw) && raw.starts_with(VAR))
    })?;
    let rest =
        word.text.strip_prefix(VAR).filter(|rest| !rest.is_empty() || word.home.is_some())?;
    let written = command.get(word.start..word.end)?.strip_prefix(VAR)?.to_owned();
    let home = word.home.map(|h| h.saturating_sub(VAR.len()));
    let literal = word.literal && !(written.starts_with('~') || written.contains(":~"));
    Some((Word { text: rest.to_owned(), home, literal, ..word.clone() }, written))
}

/// Whether two paths name one file: the same path or, when both exist, the
/// same file once links are followed.
fn same_file(a: &Path, b: &Path) -> bool {
    let absolute = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    absolute(a) == absolute(b)
        || std::fs::canonicalize(a).is_ok_and(|a| std::fs::canonicalize(b).is_ok_and(|b| a == b))
}

/// The path this binary is known by, for `install --absolute`: the first
/// `garnish` on `PATH` or the absolute path it was run by (`argv[0]`) that
/// is this very file, else `exe` itself.
///
/// On Linux `current_exe` is the symlink-resolved file, which for a package
/// manager's launcher (`bin/garnish` → `Caskroom/garnish/<version>/garnish`)
/// is a versioned directory the next upgrade deletes; the launcher outlives
/// it.
fn launcher(exe: &Path, path_env: Option<&std::ffi::OsStr>, argv0: Option<&Path>) -> PathBuf {
    let Ok(target) = std::fs::canonicalize(exe) else { return exe.to_path_buf() };
    let on_path = path_env
        .into_iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("garnish"));
    // A bare name was found through PATH, which is searched already.
    let invoked =
        argv0.filter(|p| p.components().nth(1).is_some()).and_then(|p| std::path::absolute(p).ok());
    on_path
        .chain(invoked)
        .find(|candidate| {
            candidate.is_file()
                && is_executable(candidate)
                && std::fs::canonicalize(candidate).is_ok_and(|c| c == target)
        })
        .unwrap_or_else(|| exe.to_path_buf())
}

/// Largest `--padding`: the config's `padding` is a `u16` and gets twice it.
pub const MAX_PADDING: u64 = 32_767;

/// The default Claude Code user settings file: `settings.json` in
/// `$CLAUDE_CONFIG_DIR`, else in `~/.claude`, where Claude Code reads it
/// ([`crate::claude_settings::user_dir`]); the skills go next to it.
#[must_use]
pub fn default_settings_path() -> Option<PathBuf> {
    // No HOME, no default: never guess the current directory.
    crate::claude_settings::user_dir(crate::claude_settings::home_dir().as_deref())
        .map(|dir| dir.join("settings.json"))
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

/// The settings text as a JSON object; an empty (or whitespace-only) text
/// is `{}`, as a freshly `touch`ed file is to Claude Code.
fn settings_object(existing: &str) -> Result<Map<String, Value>, String> {
    let existing = existing.strip_prefix('\u{feff}').unwrap_or(existing);
    if existing.trim().is_empty() {
        return Ok(Map::new());
    }
    // The same two problems, worded as `doctor` words them, so every
    // caller can prefix the file's path once.
    match serde_json::from_str::<Value>(existing) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err("not a JSON object".to_owned()),
        Err(e) => Err(format!("not valid JSON: {e}")),
    }
}

/// Merge the plan into existing settings text, returning the new JSON text.
///
/// Only the `statusLine` object changes, in place: every other key, and
/// every key of `statusLine` the plan does not own, keeps its value and
/// its position.
///
/// # Errors
/// When the existing text is not a JSON object.
pub fn merge(existing: &str, plan: &Plan) -> Result<String, String> {
    let mut root = settings_object(existing)?;
    let old = root.get("statusLine").and_then(|s| s.get("command")).and_then(Value::as_str);
    let command = plan.command(old);
    let fill = |status: &mut Map<String, Value>| {
        status.insert("type".into(), json!("command"));
        status.insert("command".into(), json!(command));
        status.insert("refreshInterval".into(), json!(plan.refresh_interval));
        if let Some(p) = plan.padding {
            status.insert("padding".into(), json!(p));
        }
    };
    // `remove` then `insert` would move the key: with `preserve_order` a
    // removal is a `swap_remove`, so the last key took its slot.
    match root.get_mut("statusLine") {
        Some(Value::Object(status)) => fill(status),
        Some(other) => {
            let mut status = Map::new();
            fill(&mut status);
            *other = Value::Object(status);
        }
        None => {
            let mut status = Map::new();
            fill(&mut status);
            root.insert("statusLine".into(), Value::Object(status));
        }
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// Read the settings file: `Ok(None)` when it does not exist, a refusal
/// for any other problem (a directory, unreadable, not UTF-8), so a dry run
/// reports exactly what the real run would hit.
///
/// # Errors
/// [`Refusal::Unparsable`] for bytes that are not UTF-8 (JSON is UTF-8, so
/// such a file does not parse and is never rewritten), [`Refusal::Io`] for
/// any I/O error other than "not found".
pub fn read_existing(path: &Path) -> Result<Option<String>, Refusal> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| Refusal::Unparsable {
            path: path.to_path_buf(),
            problem: "not valid UTF-8".to_owned(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Refusal::Io(format!("reading {}: {e}", path.display()))),
    }
}

/// Why the config at `path` must never be rewritten (SPEC § 5).
///
/// Bytes that are not UTF-8, or a TOML syntax error; `Ok(None)` when there
/// is no file or it parses (a file with bad values parses, and may be
/// replaced).
///
/// # Errors
/// Any I/O error other than "not found", naming the file.
pub fn config_problem(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8(bytes).map_or_else(
            |_| Some("not valid UTF-8".to_owned()),
            |t| crate::config::syntax_error(&t),
        )),
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
    // A file that does not parse is never rewritten (SPEC § 5): the only
    // way past is fixing or moving it by hand.
    if let Some(problem) = config_problem(target).map_err(Refusal::Io)? {
        return Err(Refusal::Unparsable { path: target.to_path_buf(), problem });
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
    // leaves nothing behind. It is on disk before the rename: without the
    // sync, a crash soon after it can leave a zero-length file on a file
    // system without ext4's rename heuristic (XFS, APFS).
    let written = create_with(&tmp, permissions.as_ref())
        .and_then(|mut file| file.write_all(contents.as_bytes()).and_then(|()| file.sync_all()))
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
    // The rename itself is durable once the directory is; best effort, as a
    // directory cannot be opened for syncing everywhere.
    if let Some(dir) = target.parent() {
        let _ = std::fs::File::open(dir).and_then(|d| d.sync_all());
    }
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
///
/// The bytes and the mode are read first, so a target that cannot be read
/// leaves no empty backup, and the backup is created with the target's
/// mode, as [`replace_file`]'s temp file is: a full copy of a 0600 file is
/// never readable by others, even while it is written. A failed write
/// removes the partial backup.
fn write_backup(target: &Path) -> Result<PathBuf, String> {
    let read = |e: std::io::Error| format!("reading {}: {e}", target.display());
    let bytes = std::fs::read(target).map_err(read)?;
    let permissions = std::fs::metadata(target).map_err(read)?.permissions();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = target
        .file_name()
        .map_or_else(|| "settings.json".to_owned(), |n| n.to_string_lossy().into_owned());
    for attempt in 0..1000_u32 {
        let suffix = if attempt == 0 { String::new() } else { format!("-{attempt}") };
        let path = target.with_file_name(format!("{name}.bak-{stamp}{suffix}"));
        match create_with(&path, Some(&permissions)) {
            Ok(mut f) => {
                let written = f.write_all(&bytes).and_then(|()| f.sync_all());
                if let Err(e) = written {
                    let _ = std::fs::remove_file(&path);
                    return Err(format!("backing up to {}: {e}", path.display()));
                }
                // The creation mode passed through the umask; this is the
                // old file's exactly.
                let _ = std::fs::set_permissions(&path, permissions);
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
    /// The config named explicitly, `--config` or else `GARNISH_CONFIG`
    /// ([`crate::config::hand_explicit`]; the caller reads the variable, so
    /// a plan reads no environment for it): the default config goes there,
    /// and the command written passes it with `--config`.
    pub config_path: Option<PathBuf>,
    /// The config the caller writes itself (`setup`, with `write_config`
    /// off): when the command written reads another, the plan notes it
    /// ([`ConfigStep::Elsewhere`]).
    pub config_written: Option<PathBuf>,
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
            config_written: None,
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
    /// A settings file names the config by a value that stands for no one
    /// file (a `--config` of [`CommandConfig::Unresolved`], or a relative
    /// `env.GARNISH_CONFIG`), so there is no telling which config its ticks
    /// read.
    UnresolvedConfig {
        /// The settings file.
        settings: PathBuf,
        /// The key naming it: `statusLine.command` or `env.GARNISH_CONFIG`.
        key: &'static str,
        /// The value, as the file spells it.
        word: String,
    },
    /// A settings file that is not the person's own names the config: a
    /// file garnish never follows.
    CheckoutConfig(crate::config::Checkout),
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
            Self::UnresolvedConfig { settings, key, word } => write!(
                f,
                "{}: {key} names the config {:?}, which is no one file garnish can find{}; pass --config <FILE> to say which",
                settings.display(),
                shown_word(word),
                expands_nothing(key)
            ),
            Self::CheckoutConfig(c) => {
                let shown = shown_word(&c.path.to_string_lossy());
                match &c.settings {
                    Some(settings) => write!(
                        f,
                        "{}: {} names the config {shown:?}, but this settings file is a checkout's, not yours, and garnish never follows a file one chooses; pass --config <FILE> to say which",
                        settings.display(),
                        c.key
                    ),
                    None if c.path.is_relative() => write!(
                        f,
                        "{} names the config {shown:?}, a relative path, which is no one file garnish can find; pass --config <FILE> to say which",
                        c.key
                    ),
                    None => write!(
                        f,
                        "{} names the config {shown:?}, but inside Claude Code a project's settings can set it and none of your own does, so garnish does not follow it; pass --config <FILE> to say which",
                        c.key
                    ),
                }
            }
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
    /// The settings file names the config by a value that stands for no one
    /// file (`config::WriteTarget::Unresolved`): no default config is
    /// written, since it could land where the command never reads, and the
    /// report says so.
    Unresolved {
        /// The settings file naming it: the one rewritten, or a managed
        /// file whose `env` block Claude Code puts above it.
        settings: PathBuf,
        /// The key naming it: `statusLine.command` or `env.GARNISH_CONFIG`.
        key: &'static str,
        /// The value, as written.
        word: String,
    },
    /// A settings file that is not the person's own names the config
    /// (`config::WriteTarget::Checkout`): no default config is written,
    /// since garnish never writes a file such a file chooses, and the
    /// report says so.
    Checkout(crate::config::Checkout),
    /// `setup` writes its own config there, and the command written reads
    /// `reads` instead: nothing is written for it, and the report says so.
    Elsewhere {
        /// The config the caller writes.
        written: PathBuf,
        /// The config the command reads.
        reads: PathBuf,
    },
    /// A config already exists there; `padding` is the value it would need
    /// to match `statusLine.padding` when it has another, which the report
    /// notes.
    Exists {
        /// The file.
        path: PathBuf,
        /// `2 × statusLine.padding`, when the file's `padding` differs.
        padding: Option<u64>,
    },
    /// The annotated default file will be written there, seeded with
    /// `padding` when `statusLine.padding` is set.
    Write {
        /// The file.
        path: PathBuf,
        /// `2 × statusLine.padding` (`--padding`, else the one the
        /// settings file already has), when there is one.
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
    /// The `statusLine.command` it carries ([`Plan::command`]).
    pub command: String,
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
    /// binary or config path that cannot be written into a command
    /// (`--absolute` without its own path, a path that is not UTF-8).
    pub fn plan(options: &Options) -> Result<Self, Refusal> {
        let utf8 = |path: PathBuf, what: &str| {
            path.to_str().map(str::to_owned).ok_or_else(|| {
                Refusal::Io(format!(
                    "{what} {} is not valid UTF-8, so no command can name it",
                    path.display()
                ))
            })
        };
        let program = if options.absolute {
            let exe = std::env::current_exe()
                .map_err(|e| Refusal::Io(format!("locating this binary: {e}")))?;
            let argv0 = std::env::args_os().next().map(PathBuf::from);
            let path = launcher(&exe, std::env::var_os("PATH").as_deref(), argv0.as_deref());
            shell_quote(&utf8(path, "this binary's path")?)
        } else {
            "garnish".to_owned()
        };
        let config = options
            .config_path
            .as_deref()
            .map(|p| {
                let absolute = std::path::absolute(p)
                    .map_err(|e| Refusal::Io(format!("{}: {e}", p.display())))?;
                utf8(absolute, "the config").map(PathBuf::from)
            })
            .transpose()?;
        let Some(settings) = options.settings.clone().or_else(default_settings_path) else {
            return Err(Refusal::NoHome { flag: "--settings <FILE>", what: "settings.json is" });
        };
        let plan = Plan {
            settings,
            program,
            config,
            refresh_interval: options.refresh_interval.max(1),
            padding: options.padding,
        };
        let found = options.absolute || on_path("garnish", std::env::var_os("PATH").as_deref());
        let existing = read_existing(&plan.settings)?;
        let unparsable = |problem| Refusal::Unparsable { path: plan.settings.clone(), problem };
        let current = settings_object(existing.as_deref().unwrap_or("")).map_err(unparsable)?;
        let status = current.get("statusLine");
        let merged = merge(existing.as_deref().unwrap_or(""), &plan).map_err(unparsable)?;
        let old_command = status.and_then(|s| s.get("command")).and_then(Value::as_str);
        let command = plan.command(old_command);
        // The harness pads both sides, so the config mirrors
        // statusLine.padding doubled (SPEC § 2.1): the flag's, else the one
        // the file keeps, which the merge leaves in place.
        let kept = status
            .and_then(|s| s.get("padding"))
            .and_then(Value::as_u64)
            .filter(|p| *p <= MAX_PADDING);
        let padding = options.padding.or(kept).map(|p| p.saturating_mul(2));
        // The `GARNISH_CONFIG` the file's `env` block gives the command,
        // which the merge keeps, of any JSON type as Claude Code reads it.
        let env = current
            .get("env")
            .and_then(|env| env.get(crate::config::CONFIG_ENV))
            .map(crate::claude_settings::js_string);
        let env = env.as_deref();
        let config = if options.write_config {
            // The file the written command reads: the explicit one, else
            // the one the kept command passes, else the default. The
            // command is the one just read from the file being rewritten,
            // whatever its size, never a second, capped read of it.
            let from = crate::config::CommandFrom::Given {
                settings: &plan.settings,
                command: old_command,
                env,
            };
            match crate::config::write_target(options.config_path.as_deref(), from) {
                WriteTarget::File(path) if path.exists() => {
                    // Noted only when the file says otherwise, so a
                    // reinstall over a matching config is quiet.
                    let loaded = crate::config::load(Some(&path), &crate::modules::SCHEMAS);
                    let has = loaded.config.padding;
                    let padding = padding.filter(|p| u64::try_from(has).ok() != Some(*p));
                    ConfigStep::Exists { path, padding }
                }
                WriteTarget::File(path) => ConfigStep::Write { path, padding },
                WriteTarget::NoHome => {
                    return Err(Refusal::NoHome {
                        flag: "--config <FILE>",
                        what: "the config goes",
                    });
                }
                WriteTarget::Unresolved { settings, key, word } => {
                    ConfigStep::Unresolved { settings, key, word }
                }
                WriteTarget::Checkout(checkout) => ConfigStep::Checkout(checkout),
            }
        } else {
            // What `setup` writes against what the command written reads
            // (verification of 2026-09-26: a preset went where nothing read
            // it, without a word). A command that passes no config reads the
            // `env` block's, else the lookup's file.
            let home = crate::claude_settings::home_dir();
            let reads = match command_config(&command, home.as_deref()) {
                Some(CommandConfig::File(reads)) => Some(reads),
                Some(CommandConfig::Unresolved(_)) => None,
                None => match crate::config::installed_env_target(&plan.settings, env) {
                    Some(WriteTarget::File(set)) => Some(set),
                    Some(_) => None,
                    None => crate::config::lookup().or_else(crate::config::default_path),
                },
            };
            match (&options.config_written, reads) {
                (Some(written), Some(reads)) if !same_file(written, &reads) => {
                    ConfigStep::Elsewhere { written: written.clone(), reads }
                }
                _ => ConfigStep::Skipped,
            }
        };
        let skills = options.write_skills.then(|| crate::skills::default_dir(&plan.settings));
        Ok(Self { plan, existing, merged, command, config, skills, found })
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
    /// included, in the words [`Steps::apply`] would use (a settings file
    /// already up to date is left alone, a replaced one kept as a backup).
    #[must_use]
    pub fn dry_run(&self) -> Vec<String> {
        let settings = self.plan.settings.display();
        let mut lines = if self.settings_up_to_date() {
            vec![format!("{settings} already up to date")]
        } else if self.existing.is_some() {
            vec![
                format!("would write {settings} (a backup is kept):"),
                self.merged.trim_end().to_owned(),
            ]
        } else {
            vec![format!("would write {settings}:"), self.merged.trim_end().to_owned()]
        };
        match &self.config {
            ConfigStep::Write { path, padding } => {
                lines.push(format!(
                    "would write a default config to {}{}",
                    path.display(),
                    seeded(*padding)
                ));
            }
            ConfigStep::Exists { .. }
            | ConfigStep::Skipped
            | ConfigStep::Unresolved { .. }
            | ConfigStep::Checkout(_)
            | ConfigStep::Elsewhere { .. } => {}
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
    /// warning, the `padding` a config that already exists would need, why
    /// no default config is written for a config garnish does not follow,
    /// and a command that reads another config than the one `setup` writes.
    #[must_use]
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if !self.found {
            notes.push(
                "warning: `garnish` is not on PATH; run `make install` first or use --absolute"
                    .to_owned(),
            );
        }
        match &self.config {
            ConfigStep::Exists { path, padding: Some(p) } => notes.push(format!(
                "note: {} already exists; set `padding = {p}` in it to match statusLine.padding",
                path.display()
            )),
            ConfigStep::Unresolved { settings, key, word } => notes.push(format!(
                "note: {}: {key} names the config {:?}, which is no one file garnish can find{}, so no default config is written; pass --config <FILE> to say which",
                settings.display(),
                shown_word(word),
                expands_nothing(key)
            )),
            ConfigStep::Checkout(checkout) => notes.push(format!(
                "note: {} is not your own settings file, and garnish never writes the config {:?} its {} names, so no default config is written; pass --config <FILE> to say which",
                checkout.settings.as_deref().unwrap_or(&self.plan.settings).display(),
                shown_word(&checkout.path.to_string_lossy()),
                checkout.key
            )),
            ConfigStep::Elsewhere { written, reads } => notes.push(format!(
                "note: the status line command reads {}, not {}; `garnish --config {} install` points it there",
                reads.display(),
                written.display(),
                shell_quote(&written.to_string_lossy())
            )),
            ConfigStep::Exists { .. } | ConfigStep::Write { .. } | ConfigStep::Skipped => {}
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
            program: "garnish".into(),
            config: None,
            refresh_interval: 1,
            padding: None,
        }
    }

    /// With `preserve_order`, removing `statusLine` and inserting it again
    /// was a `swap_remove`: the last key jumped into its slot and
    /// `statusLine` to the end, a diff of moves nobody made.
    #[test]
    fn merge_keeps_every_key_in_its_place() {
        let p = plan(Path::new("/x"));
        let at = |out: &str, key: &str| out.find(&format!("\"{key}\"")).unwrap();
        let out = merge(r#"{"statusLine":{"command":"x"},"a":1,"b":2,"c":3}"#, &p).unwrap();
        let order = ["statusLine", "a", "b", "c"].map(|k| at(&out, k));
        assert!(order.is_sorted(), "{out}");
        // Inside statusLine too: the keys it has keep their places, new ones follow.
        let out = merge(r#"{"statusLine":{"padding":1,"command":"x"}}"#, &p).unwrap();
        let order = ["padding", "command", "refreshInterval"].map(|k| at(&out, k));
        assert!(order.is_sorted(), "{out}");
        // A statusLine that is not an object is replaced where it stands.
        let out = merge(r#"{"statusLine":"garnish","z":1}"#, &p).unwrap();
        assert!(at(&out, "statusLine") < at(&out, "z"), "{out}");
        assert!(out.contains("\"type\": \"command\""), "{out}");
    }

    /// The command keeps what a person added to a garnish command (a
    /// `--config` above all) and names an explicit config itself; anything
    /// else is replaced by the program alone.
    #[test]
    fn the_command_replaces_only_the_program_word() {
        let p = plan(Path::new("/x"));
        assert_eq!(p.command(None), "garnish");
        assert_eq!(p.command(Some("garnish --config /x.toml")), "garnish --config /x.toml");
        assert_eq!(p.command(Some("  /old/bin/garnish  render")), "garnish  render");
        assert_eq!(
            p.command(Some("'/a b/garnish' --config \"/c d.toml\"")),
            "garnish --config \"/c d.toml\""
        );
        assert_eq!(p.command(Some("/a\\ b/garnish -q")), "garnish -q");
        assert_eq!(p.command(Some("ccstatusline --x")), "garnish");
        assert_eq!(p.command(Some("garnished")), "garnish");
        assert_eq!(p.command(Some("")), "garnish");
        let absolute = Plan { program: shell_quote("/opt/my tools/garnish"), ..p.clone() };
        assert_eq!(
            absolute.command(Some("garnish --config /x.toml")),
            "'/opt/my tools/garnish' --config /x.toml"
        );
        let explicit = Plan { config: Some(PathBuf::from("/h/it's.toml")), ..p };
        assert_eq!(
            explicit.command(Some("garnish --config /old.toml")),
            r"garnish --config '/h/it'\''s.toml'"
        );
        let merged = merge(
            r#"{"statusLine":{"command":"garnish --config /x.toml"}}"#,
            &plan(Path::new("/x")),
        )
        .unwrap();
        assert!(merged.contains("\"command\": \"garnish --config /x.toml\""), "{merged}");
    }

    /// The splitter takes quotes and backslashes out as `sh` does, knows
    /// where the home directory goes, marks every other expansion, and
    /// stops at the end of the first simple command.
    #[test]
    fn shell_words_split_as_sh_does() {
        let split = |command: &str| -> Vec<(String, bool, Option<usize>)> {
            shell_words(command).into_iter().map(|w| (w.text, w.literal, w.home)).collect()
        };
        let word =
            |text: &str, literal: bool, home: Option<usize>| (text.to_owned(), literal, home);
        assert_eq!(
            split(r#"  a 'b c'"d e"f\ g  "h\"i\$j\k" "#),
            [word("a", true, None), word("b cd ef g", true, None), word(r#"h"i$j\k"#, true, None)]
        );
        assert_eq!(
            split(r#"~ ~/x "$HOME/y" ${HOME} ''$HOME/z"#),
            [
                word("", true, Some(0)),
                word("/x", true, Some(0)),
                word("/y", true, Some(0)),
                word("", true, Some(0)),
                word("/z", true, Some(0))
            ]
        );
        // `$HOME` expands wherever it stands, and what follows it is glued
        // on, not joined as a path (final review: `$HOME.w` read as a file
        // inside the home directory, `--config=$HOME/w` as no file at all).
        assert_eq!(
            split(r#"a$HOME $HOME.w "${HOME}"_w --config=$HOME/w '$HOME'"#),
            [
                word("a", true, Some(1)),
                word(".w", true, Some(0)),
                word("_w", true, Some(0)),
                word("--config=/w", true, Some(9)),
                word("$HOME", true, None)
            ]
        );
        // Anything the shell would rewrite, or that garnish cannot follow.
        for command in [
            "~root/x",
            "$HOMEX",
            "${HOME:-/x}",
            "$HOME$HOME",
            "$X",
            "`pwd`/x",
            "\"$(pwd)\"",
            "*.toml",
            "x>y",
            "{a,b}",
            "'open",
            "a\\",
        ] {
            let words = shell_words(command);
            assert!(!words.iter().all(|w| w.literal), "{command}: {words:?}");
        }
        // A `~` quoted or past the start stays a `~`, and `\$HOME` stays text.
        assert_eq!(
            split(r#""~/x"x~ \$HOME"#),
            [word("~/xx~", true, None), word("$HOME", true, None)]
        );
        // The first simple command only.
        for command in ["a b; c", "a b | c", "a b && c", "a b\nc", "a b # c", "a b&c"] {
            assert_eq!(split(command), [word("a", true, None), word("b", true, None)], "{command}");
        }
        assert_eq!(split("a#b"), [word("a#b", true, None)], "a `#` inside a word is text");
        let words = shell_words("  x 'y z' ");
        assert_eq!((words[0].start, words[0].end, words[1].start, words[1].end), (2, 3, 4, 9));
        // `sh`'s blanks alone end a word (verification of 2026-09-26: a
        // non-breaking space pasted from a web page cut the path short).
        assert_eq!(
            split("a\u{a0}b c\r\td"),
            [word("a\u{a0}b", true, None), word("c\r", true, None), word("d", true, None)]
        );
    }

    /// The home directory an unquoted `$HOME` spells is split and globbed by
    /// `sh`, so a home with a blank or a glob character in it names no one
    /// file; quoted, or as `~`, it passes whole.
    #[test]
    fn a_home_the_shell_would_split_names_no_file() {
        let home = Some(Path::new("/my home"));
        let file = |p: &str| Some(CommandConfig::File(PathBuf::from(p)));
        let unresolved = |w: &str| Some(CommandConfig::Unresolved(w.to_owned()));
        for (command, want) in [
            ("garnish --config $HOME/w.toml", unresolved("$HOME/w.toml")),
            ("garnish --config=${HOME}/w.toml", unresolved("${HOME}/w.toml")),
            ("garnish --config \"$HOME/w.toml\"", file("/my home/w.toml")),
            ("garnish --config ~/w.toml", file("/my home/w.toml")),
        ] {
            assert_eq!(command_config(command, home), want, "{command}");
        }
        let glob = Some(Path::new("/h*"));
        assert_eq!(command_config("garnish --config $HOME/w", glob), unresolved("$HOME/w"));
        assert_eq!(command_config("garnish --config \"$HOME/w\"", glob), file("/h*/w"));
    }

    /// Verification of 2026-09-26: the file `setup` writes and the one the
    /// installed command reads are one file through a link or a relative
    /// spelling, so no note says otherwise.
    #[cfg(unix)]
    #[test]
    fn one_file_by_a_link_or_a_relative_path_is_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.toml");
        std::fs::write(&real, "").unwrap();
        let link = dir.path().join("link.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(same_file(&link, &real));
        let cwd = std::env::current_dir().unwrap();
        assert!(same_file(Path::new("rel.toml"), &cwd.join("rel.toml")));
        assert!(!same_file(&real, &dir.path().join("other.toml")));
        let settings = dir.path().join("settings.json");
        let options = Options {
            settings: Some(settings),
            config_path: Some(PathBuf::from("rel.toml")),
            config_written: Some(PathBuf::from("rel.toml")),
            write_config: false,
            write_skills: false,
            ..Options::default()
        };
        assert_eq!(Steps::plan(&options).unwrap().config, ConfigStep::Skipped);
    }

    /// A quoted `--config` word is cut to a line's worth: it comes from a
    /// settings file a project may carry.
    #[test]
    fn a_quoted_config_word_is_cut() {
        let long = "x".repeat(5000);
        let settings = PathBuf::from("/s.json");
        let key = "statusLine.command";
        let note =
            Refusal::UnresolvedConfig { settings: settings.clone(), key, word: long }.to_string();
        assert!(note.chars().count() < 400, "{} characters", note.chars().count());
        assert!(note.contains(&format!("\"{}…\"", "x".repeat(MAX_SHOWN_WORD_CHARS))), "{note}");
        let short =
            Refusal::UnresolvedConfig { settings, key, word: "\"~/w\u{1b}.toml\"".to_owned() };
        assert!(short.to_string().contains(r#"the config "\"~/w\u{1b}.toml\"""#), "{short}");
        // `install`'s note too.
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let command = format!("garnish --config {}", "x".repeat(5000));
        let status = serde_json::json!({"statusLine": {"type": "command", "command": command}});
        std::fs::write(&settings, status.to_string()).unwrap();
        // The note names the settings file, whose temporary path is long on
        // macOS; the word is what must be cut.
        let named = settings.display().to_string().chars().count();
        let options =
            Options { settings: Some(settings), write_skills: false, ..Options::default() };
        let notes = Steps::plan(&options).unwrap().notes();
        let note = notes.iter().find(|n| n.contains("is no one file")).unwrap();
        let count = note.chars().count().saturating_sub(named);
        assert!(count < 400, "{count} characters besides the settings path");
    }

    /// The config a garnish command passes is the file its ticks read: an
    /// absolute path, or one under the home as the shell expands it. A
    /// value the harness would resolve elsewhere (a relative path, from
    /// whatever directory it runs the command in) or through an expansion
    /// garnish does not follow is named as written, never guessed.
    #[test]
    fn the_config_a_garnish_command_passes() {
        let home = Some(Path::new("/h"));
        let file = |p: &str| Some(CommandConfig::File(PathBuf::from(p)));
        let unresolved = |w: &str| Some(CommandConfig::Unresolved(w.to_owned()));
        for (command, want) in [
            ("garnish --config /x.toml", file("/x.toml")),
            ("/opt/garnish render --config=/x.toml", file("/x.toml")),
            ("A=1 env B=2 garnish --config '/a b.toml' render", file("/a b.toml")),
            ("garnish --config ~/w.toml", file("/h/w.toml")),
            ("garnish --config \"$HOME/w.toml\"", file("/h/w.toml")),
            ("garnish --config ${HOME}/w.toml", file("/h/w.toml")),
            ("garnish --config w.toml", unresolved("w.toml")),
            ("garnish --config=~/w.toml", unresolved("~/w.toml")),
            // Final review: `$HOME` is spliced in where it stands, as `sh`
            // does, in a `--config=` word too.
            ("garnish --config=$HOME/w.toml", file("/h/w.toml")),
            ("garnish --config=\"${HOME}/w\"", file("/h/w")),
            ("garnish --config ${HOME}w.toml", file("/hw.toml")),
            ("garnish --config \"$HOME\"w.toml", file("/hw.toml")),
            ("garnish --config $HOME.w.toml", file("/h.w.toml")),
            ("garnish --config $HOME-w.toml", file("/h-w.toml")),
            ("garnish --config a$HOME", unresolved("a$HOME")),
            ("garnish --config$HOME /x.toml", None),
            ("garnish $HOME--config /x.toml", None),
            ("garnish \"--config=/a b\"", file("/a b")),
            ("garnish --config \"~/w.toml\"", unresolved("\"~/w.toml\"")),
            ("garnish --config $XDG_CONFIG_HOME/g.toml", unresolved("$XDG_CONFIG_HOME/g.toml")),
            ("garnish --config ~root/w.toml", unresolved("~root/w.toml")),
            ("garnish", None),
            ("garnish --config", None),
            ("garnish --configure /x.toml", None),
            ("garnish \"--config\" /x.toml", file("/x.toml")),
            ("ccstatusline --config /x.toml", None),
            ("garnish; other --config /x.toml", None),
            // Verification of 2026-09-26: the ticks read `GARNISH_CONFIG`
            // when no `--config` is passed, the last assignment winning and
            // an empty one unset; clap reads no option after `--` and
            // refuses a second `--config`, so the ticks read no file.
            ("GARNISH_CONFIG=/e.toml garnish", file("/e.toml")),
            ("env GARNISH_CONFIG=\"$HOME/e\" garnish render", file("/h/e")),
            ("GARNISH_CONFIG=/a GARNISH_CONFIG=/b garnish", file("/b")),
            ("GARNISH_CONFIG=/e.toml garnish --config /x.toml", file("/x.toml")),
            ("GARNISH_CONFIG=e.toml garnish", unresolved("e.toml")),
            ("GARNISH_CONFIG=/a GARNISH_CONFIG= garnish", None),
            ("MY_GARNISH_CONFIG=/e.toml garnish", None),
            ("garnish -- --config /x.toml", None),
            ("garnish --config /a --config=/b", unresolved("--config /a --config=/b")),
            ("garnish --config /a --config", unresolved("--config /a --config")),
            ("garnish --config /a -- --config /b", unresolved("--config /a -- --config /b")),
            ("garnish --config /a --", file("/a")),
            ("GARNISH_CONFIG=/a garnish --config", unresolved("--config")),
            // A `~` after the `=` or a `:` of an assignment is the shell's.
            ("GARNISH_CONFIG=/a:~/b garnish", unresolved("/a:~/b")),
            ("GARNISH_CONFIG=~/b garnish", unresolved("~/b")),
            ("GARNISH_CONFIG=/a/b~c garnish", file("/a/b~c")),
        ] {
            assert_eq!(command_config(command, home), want, "{command}");
        }
        assert_eq!(command_config("garnish --config ~/w.toml", None), unresolved("~/w.toml"));
        // A program word the shell would split runs no garnish at all.
        let spaced = Some(Path::new("/my home"));
        assert_eq!(command_config("$HOME/bin/garnish --config /x.toml", spaced), None);
        assert_eq!(
            command_config("\"$HOME/bin/garnish\" --config /x.toml", spaced),
            file("/x.toml")
        );
    }

    /// An environment prefix (`NAME=value` words, after a leading `env`
    /// or not) comes before the program word and is kept with the
    /// arguments; a command carrying one read as not running garnish, so
    /// a reinstall dropped its `--config`.
    #[test]
    fn the_command_keeps_an_environment_prefix() {
        let p = plan(Path::new("/x"));
        for (old, new) in [
            (
                "GARNISH_ANIMATE=0 garnish --config /x.toml",
                "GARNISH_ANIMATE=0 garnish --config /x.toml",
            ),
            (
                "env GARNISH_ANIMATE=0 /old/garnish --config /x.toml",
                "env GARNISH_ANIMATE=0 garnish --config /x.toml",
            ),
            ("  /usr/bin/env A='x y'  B=  garnish -q", "/usr/bin/env A='x y'  B=  garnish -q"),
            ("env garnish render", "env garnish render"),
            ("_A1=\"$HOME\" ~/bin/garnish", "_A1=\"$HOME\" garnish"),
            // Not garnish: another program, or a word that only looks
            // like an assignment or like `env`.
            ("GARNISH_ANIMATE=0 ccstatusline --x", "garnish"),
            ("\"A\"=1 garnish", "garnish"),
            ("1A=1 garnish", "garnish"),
            ("env -i garnish", "garnish"),
            ("A=1", "garnish"),
            ("env", "garnish"),
            ("exec garnish", "garnish"),
        ] {
            assert_eq!(p.command(Some(old)), new, "{old}");
        }
        let absolute = Plan { program: shell_quote("/opt/my tools/garnish"), ..p };
        assert_eq!(
            absolute.command(Some("X=1 garnish --config /x.toml")),
            "X=1 '/opt/my tools/garnish' --config /x.toml"
        );
    }

    #[test]
    fn shell_quote_leaves_plain_words_and_quotes_the_rest() {
        assert_eq!(shell_quote("/usr/bin/garnish"), "/usr/bin/garnish");
        assert_eq!(shell_quote("/a b/garnish"), "'/a b/garnish'");
        assert_eq!(shell_quote("/x'y/garnish"), r"'/x'\''y/garnish'");
        assert_eq!(shell_quote("/h/$HOME/~x"), "'/h/$HOME/~x'");
        assert_eq!(shell_quote(""), "''");
    }

    /// SPEC § 7: `--absolute` records the launcher found on PATH (or the
    /// path the binary was run by) when it is this very file, not the
    /// resolved target, which for a package manager sits in a versioned
    /// directory the next upgrade removes.
    #[cfg(unix)]
    #[test]
    fn absolute_prefers_the_launcher_to_the_versioned_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(dir.path()).unwrap();
        let versioned = dir.join("Caskroom").join("garnish").join("0.3.0").join("garnish");
        std::fs::create_dir_all(versioned.parent().unwrap()).unwrap();
        std::fs::write(&versioned, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&versioned, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(&versioned, bin.join("garnish")).unwrap();
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("garnish"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(other.join("garnish"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let path = std::env::join_paths([&other, &bin]).unwrap();
        assert_eq!(launcher(&versioned, Some(&path), None), bin.join("garnish"), "not other's");
        assert_eq!(launcher(&versioned, Some(other.as_os_str()), None), versioned, "none: exe");
        let invoked = bin.join("garnish");
        assert_eq!(launcher(&versioned, None, Some(&invoked)), invoked, "argv[0]");
        assert_eq!(launcher(&versioned, None, Some(Path::new("garnish"))), versioned);
    }

    /// cli-05: a padding the settings file already has seeds a new config
    /// as `--padding` would (the merge keeps it), and a config that
    /// already matches gets no note.
    #[test]
    fn the_padding_the_settings_file_keeps_seeds_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let config = dir.path().join("garnish.toml");
        std::fs::write(&settings, r#"{"statusLine":{"command":"x","padding":1}}"#).unwrap();
        let options = Options {
            settings: Some(settings.clone()),
            config_path: Some(config.clone()),
            write_skills: false,
            ..Options::default()
        };
        let steps = Steps::plan(&options).unwrap();
        assert_eq!(steps.config, ConfigStep::Write { path: config.clone(), padding: Some(2) });
        assert!(steps.merged.contains("\"padding\": 1"), "{}", steps.merged);
        steps.apply().unwrap();
        assert!(std::fs::read_to_string(&config).unwrap().contains("\npadding = 2\n"));
        let again = Steps::plan(&options).unwrap();
        assert_eq!(again.config, ConfigStep::Exists { path: config.clone(), padding: None });
        assert!(again.notes().iter().all(|n| !n.contains("padding")), "{:?}", again.notes());
        std::fs::write(&config, "padding = 0\n").unwrap();
        let off = Steps::plan(&options).unwrap();
        assert_eq!(off.config, ConfigStep::Exists { path: config, padding: Some(2) });
        assert!(off.notes().iter().any(|n| n.contains("set `padding = 2`")), "{:?}", off.notes());
        // A value no config could hold is not carried over.
        std::fs::write(&settings, r#"{"statusLine":{"padding":99999}}"#).unwrap();
        let none = dir.path().join("none.toml");
        let huge = Steps::plan(&Options { config_path: Some(none.clone()), ..options }).unwrap();
        assert_eq!(huge.config, ConfigStep::Write { path: none, padding: None });
    }

    /// JSON is UTF-8, so a settings file that is not is one that does not
    /// parse: refused on one line like any other, never rewritten.
    #[test]
    fn a_settings_file_that_is_not_utf8_is_unparsable() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, b"{\"theme\": \"caf\xe9\"}").unwrap();
        let err = Steps::plan(&settings_only(&settings)).unwrap_err();
        assert!(
            matches!(err, Refusal::Unparsable { ref problem, .. } if problem == "not valid UTF-8")
        );
        let config = dir.path().join("garnish.toml");
        std::fs::write(&config, b"theme = \"\xff\"\n").unwrap();
        assert_eq!(config_problem(&config), Ok(Some("not valid UTF-8".to_owned())));
        let err = write_config(&config, "x = 1\n", true).unwrap_err();
        assert!(matches!(err, Refusal::Unparsable { .. }), "{err}");
    }

    /// `--dry-run` says what the real run would do: nothing for a file
    /// already up to date, and a backup for one it replaces.
    #[test]
    fn a_dry_run_says_what_the_real_run_would_do() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let fresh = Steps::plan(&settings_only(&settings)).unwrap().dry_run();
        assert_eq!(fresh[0], format!("would write {}:", settings.display()));
        install(&settings);
        let same = Steps::plan(&settings_only(&settings)).unwrap().dry_run();
        assert_eq!(same, vec![format!("{} already up to date", settings.display())]);
        std::fs::write(&settings, "{}").unwrap();
        let changed = Steps::plan(&settings_only(&settings)).unwrap().dry_run();
        assert_eq!(changed[0], format!("would write {} (a backup is kept):", settings.display()));
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

    /// The backup is born with the target's mode, so a 0600 settings file
    /// (an `env` block can hold a token) is never readable by others while
    /// it is copied; and a target that is gone by the time of the backup
    /// leaves no empty `.bak-` behind.
    #[cfg(unix)]
    #[test]
    fn a_backup_is_born_private_and_never_left_empty() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        std::fs::write(&target, "{\"env\":{}}").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let backup = write_backup(&target).unwrap();
        assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{\"env\":{}}");
        let gone = dir.path().join("gone.json");
        let err = write_backup(&gone).unwrap_err();
        assert!(err.contains("gone.json"), "{err}");
        assert_eq!(backups(dir.path(), "gone.json"), Vec::<PathBuf>::new(), "no empty backup");
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
