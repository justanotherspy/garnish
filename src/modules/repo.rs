//! `path`, `branch`, `sync`, `worktree`, `pr`: where you are in the repository.
//!
//! `worktree` and `pr` come straight from the payload. `path` and `branch`
//! read the `.git` directory directly on every tick (a few small file reads,
//! never a process). Ahead/behind counts, the dirty flag and optional fetching
//! come from the background worker through the cache.

use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::time::Duration;

use crate::ansi::{Segment, Style};
use crate::cache::Scope;
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::git::{self, Head};
use crate::icons::glyph;

use super::{Ctx, Freshness, Module, RefreshCtx, Rendered, icon, seg};

/// How long the worker lets a local git command run.
const GIT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the worker lets `git fetch` run (network; opt-in only).
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// Cache scope for a checkout: shared by every session in the same worktree.
fn repo_scope(session: &str, cwd: &Path) -> Scope {
    git::discover(cwd)
        .map_or_else(|| Scope::Session(session.to_owned()), |d| Scope::Repo(d.cache_key()))
}

/// Collapse `$HOME` to `~`.
#[must_use]
pub fn tildify(path: &str, home: Option<&str>) -> String {
    match home.filter(|h| !h.is_empty()) {
        Some(h) if path == h => "~".to_owned(),
        Some(h) => path
            .strip_prefix(h)
            .filter(|r| r.starts_with('/'))
            .map_or_else(|| path.to_owned(), |r| format!("~{r}")),
        None => path.to_owned(),
    }
}

/// Keep the last `depth` components of a path (0 = all). A leading `~` is
/// kept so a home-relative path still reads as one: `~/projects/garnish`.
#[must_use]
pub fn shorten(path: &str, depth: usize) -> String {
    if depth == 0 {
        return path.to_owned();
    }
    let (home, rest) = path.strip_prefix('~').map_or(("", path), |r| ("~", r));
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() <= depth {
        return path.to_owned();
    }
    let skip = parts.len().saturating_sub(depth);
    let tail = parts.iter().skip(skip).copied().collect::<Vec<_>>().join("/");
    if home.is_empty() { tail } else { format!("{home}/{tail}") }
}

/// The path of `cwd` relative to `base`, if `cwd` is inside `base`.
#[must_use]
pub fn subpath(base: &str, cwd: &str) -> Option<String> {
    let rel = Path::new(cwd).strip_prefix(Path::new(base)).ok()?;
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `path`: the working directory, based on the repository root.
pub struct PathModule;

impl Module for PathModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "path",
            summary: "Working directory, based on the repository root.",
            doc: "The base directory is the git top level when inside a repository, otherwise `workspace.project_dir`. When the current directory is deeper than the base, the extra path is shown dimmed. The `full` preset shows the whole tilde-collapsed path and the number of `/add-dir` directories.",
            sources: &[
                "workspace.project_dir",
                "workspace.current_dir",
                "workspace.added_dirs",
                "git top level",
            ],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the folder icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "depth",
                    Kind::Int,
                    "Path components of the base to keep (0 = all).",
                    Value::Int(2),
                )
                .minimal(Value::Int(1))
                .full(Value::Int(0)),
                OptSpec::new(
                    "show_subpath",
                    Kind::Bool,
                    "Show the path below the base.",
                    Value::Bool(true),
                )
                .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_added",
                    Kind::Bool,
                    "Show the count of added directories.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![
                IconSpec {
                    key: "folder",
                    doc: "Folder icon.",
                    glyph: glyph("\u{f07b}", "▣", "📁", ""),
                },
                IconSpec {
                    key: "added",
                    doc: "Added-directories glyph.",
                    glyph: glyph("\u{f067}", "+", "➕", "+"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent" },
                ColorSpec { key: "base", doc: "Base directory.", default: "text" },
                ColorSpec { key: "subpath", doc: "Path below the base.", default: "muted" },
                ColorSpec { key: "added", doc: "Added directories.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let ws = ctx.payload.workspace.as_ref();
        let cwd = ctx.payload.current_dir().unwrap_or("");
        let toplevel = ctx.git_dirs().map(|d| d.toplevel.to_string_lossy().into_owned());
        let base = toplevel
            .as_deref()
            .or_else(|| ws.and_then(|w| w.project_dir.as_deref()))
            .filter(|p| !p.is_empty())
            .unwrap_or(cwd);
        if base.is_empty() {
            return Rendered::empty();
        }
        let shown = shorten(&tildify(base, ctx.home.as_deref()), cfg.size("depth"));
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "folder", "icon"));
        }
        segs.push(Segment::styled(shown, Style::fg(cfg.color("base")).bolded()));
        if cfg.bool("show_subpath")
            && let Some(sub) = subpath(base, cwd)
        {
            segs.push(seg(cfg, format!("/{sub}"), "subpath"));
        }
        if cfg.bool("show_added")
            && let Some(n) = ws.map(|w| w.added_dirs.len()).filter(|n| *n > 0)
        {
            segs.push(seg(cfg, format!(" {}{n}", cfg.icon("added")), "added"));
        }
        Rendered::fresh(segs)
    }
}

/// `worktree`: the git worktree or Claude worktree session.
pub struct WorktreeModule;

impl Module for WorktreeModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "worktree",
            summary: "Git worktree name.",
            doc: "Shown when the current directory is inside a linked git worktree (`workspace.git_worktree`) or the session entered a Claude Code worktree (`worktree.name`). The `full` preset adds the original branch.",
            sources: &[
                "workspace.git_worktree",
                "worktree.name",
                "worktree.branch",
                "worktree.original_branch",
            ],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_original",
                    Kind::Bool,
                    "Show `original → branch`.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![
                IconSpec {
                    key: "worktree",
                    doc: "Worktree icon.",
                    glyph: glyph("\u{f126}", "⑂", "🌳", "wt:"),
                },
                IconSpec {
                    key: "arrow",
                    doc: "Original → branch arrow.",
                    glyph: glyph("→", "→", "→", "->"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "name", doc: "Worktree name.", default: "text" },
                ColorSpec { key: "original", doc: "Original branch.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let p = ctx.payload;
        let name = p
            .worktree
            .as_ref()
            .and_then(|w| w.name.as_deref())
            .or_else(|| p.workspace.as_ref().and_then(|w| w.git_worktree.as_deref()))
            .filter(|n| !n.is_empty());
        let Some(name) = name else { return Rendered::empty() };
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "worktree", "icon"));
        }
        segs.push(seg(cfg, name, "name"));
        if cfg.bool("show_original")
            && let Some(wt) = p.worktree.as_ref()
            && let (Some(orig), Some(branch)) =
                (wt.original_branch.as_deref(), wt.branch.as_deref())
        {
            segs.push(seg(cfg, format!(" {orig} {} {branch}", cfg.icon("arrow")), "original"));
        }
        Rendered::fresh(segs)
    }
}

/// `pr`: the open pull or merge request for the current branch.
pub struct PrModule;

impl Module for PrModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "pr",
            summary: "Open pull/merge request with review state, linked.",
            doc: "The open PR (or GitLab MR) Claude Code found for the current branch, as a clickable OSC 8 link with a glyph for the review state: approved, pending, changes requested, or draft. Hidden when there is none. No network calls: the harness supplies the data.",
            sources: &["pr.number", "pr.url", "pr.review_state", "pr.kind"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the PR icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_state",
                    Kind::Bool,
                    "Show the review-state glyph.",
                    Value::Bool(true),
                )
                .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_state_word",
                    Kind::Bool,
                    "Show the review state as a word.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
                OptSpec::new(
                    "link",
                    Kind::Bool,
                    "Make the number a clickable link.",
                    Value::Bool(true),
                ),
            ],
            icons: vec![
                IconSpec {
                    key: "pr",
                    doc: "Pull request icon.",
                    glyph: glyph("\u{f407}", "⇄", "🔀", "PR"),
                },
                IconSpec {
                    key: "mr",
                    doc: "Merge request icon.",
                    glyph: glyph("\u{f407}", "⇄", "🔀", "MR"),
                },
                IconSpec {
                    key: "approved", doc: "Approved.", glyph: glyph("✓", "✓", "✅", "ok")
                },
                IconSpec {
                    key: "pending",
                    doc: "Pending review.",
                    glyph: glyph("○", "○", "🕓", ".."),
                },
                IconSpec {
                    key: "changes_requested",
                    doc: "Changes requested.",
                    glyph: glyph("✗", "✗", "❌", "xx"),
                },
                IconSpec { key: "draft", doc: "Draft.", glyph: glyph("◌", "◌", "📝", "wip") },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent" },
                ColorSpec { key: "number", doc: "PR number.", default: "text" },
                ColorSpec { key: "approved", doc: "Approved.", default: "ok" },
                ColorSpec { key: "pending", doc: "Pending.", default: "warn" },
                ColorSpec {
                    key: "changes_requested",
                    doc: "Changes requested.",
                    default: "danger",
                },
                ColorSpec { key: "draft", doc: "Draft.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(pr) = ctx.payload.pr.as_ref() else { return Rendered::empty() };
        let Some(number) = pr.number else { return Rendered::empty() };
        let is_mr = pr.kind.as_deref() == Some("mr");
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, if is_mr { "mr" } else { "pr" }, "icon"));
        }
        let label = if is_mr { format!("!{number}") } else { format!("#{number}") };
        let mut num = Segment::styled(
            label,
            Style::fg(cfg.color("number")).bolded().underline_if(cfg.bool("link")),
        );
        if cfg.bool("link")
            && let Some(url) = pr.url.as_deref()
        {
            num = num.with_link(url);
        }
        segs.push(num);
        if let Some(state) = pr.review_state.as_deref() {
            let key = match state {
                "approved" | "pending" | "changes_requested" | "draft" => state,
                _ => "pending",
            };
            if cfg.bool("show_state") && !cfg.icon(key).is_empty() {
                segs.push(seg(cfg, format!(" {}", cfg.icon(key)), key));
            }
            if cfg.bool("show_state_word") {
                segs.push(seg(cfg, format!(" {}", state.replace('_', " ")), key));
            }
        }
        Rendered::fresh(segs)
    }
}

/// `branch`: the checked-out branch.
pub struct BranchModule;

impl Module for BranchModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "branch",
            summary: "Checked-out branch (or detached HEAD).",
            doc: "The current branch read from the repository without spawning git; a detached HEAD shows the short commit. The `full` preset adds the short SHA and a dirty marker (computed by the background worker).",
            sources: &["worktree.branch", ".git/HEAD", "git status (worker)"],
            refresh: 5,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the branch icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_sha",
                    Kind::Bool,
                    "Append the short commit SHA.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
                OptSpec::new(
                    "dirty",
                    Kind::Bool,
                    "Show a marker when the tree has changes.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
                OptSpec::new(
                    "max_length",
                    Kind::Int,
                    "Truncate longer names (0 = no limit).",
                    Value::Int(40),
                ),
            ],
            icons: vec![
                IconSpec {
                    key: "branch",
                    doc: "Branch icon.",
                    glyph: glyph("\u{e725}", "⎇", "🌿", "on"),
                },
                IconSpec {
                    key: "detached",
                    doc: "Detached HEAD icon.",
                    glyph: glyph("\u{f0c1}", "➦", "📌", "@"),
                },
                IconSpec {
                    key: "dirty",
                    doc: "Dirty marker.",
                    glyph: glyph("●", "●", "✏\u{fe0f}", "*"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent" },
                ColorSpec { key: "name", doc: "Branch name.", default: "text" },
                ColorSpec { key: "sha", doc: "Short SHA.", default: "muted" },
                ColorSpec { key: "dirty", doc: "Dirty marker.", default: "warn" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let dirs = ctx.git_dirs();
        let head = dirs.and_then(git::head);
        let (name, detached) =
            match (&head, ctx.payload.worktree.as_ref().and_then(|w| w.branch.as_deref())) {
                (Some(Head::Branch(b)), _) => (b.clone(), false),
                (Some(Head::Detached(sha)), _) => (sha.chars().take(7).collect(), true),
                (None, Some(b)) => (b.to_owned(), false),
                (None, None) => return Rendered::empty(),
            };
        let head_key = name.clone();
        let max = cfg.size("max_length");
        let shown: String = if max > 0 && name.chars().count() > max {
            name.chars().take(max.saturating_sub(1)).chain(std::iter::once('…')).collect()
        } else {
            name
        };
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, if detached { "detached" } else { "branch" }, "icon"));
        }
        segs.push(Segment::styled(shown, Style::fg(cfg.color("name")).bolded()));
        if cfg.bool("show_sha")
            && !detached
            && let Some(sha) = dirs.and_then(git::head_commit)
        {
            segs.push(seg(cfg, format!(" {}", sha.chars().take(7).collect::<String>()), "sha"));
        }
        let mut freshness = Freshness::Fresh;
        if cfg.bool("dirty")
            && let Some(d) = dirs
        {
            let scope = Scope::Repo(d.cache_key());
            let (lookup, fresh) =
                ctx.cached(cfg, &scope, |e| e.get("head").is_none_or(|h| h == head_key));
            if lookup.entry.as_ref().and_then(|e| e.get("dirty")) == Some("1") {
                segs.push(seg(cfg, format!(" {}", cfg.icon("dirty")), "dirty"));
            }
            if lookup.entry.is_some() {
                freshness = fresh;
            }
        }
        Rendered { segments: segs, freshness }
    }

    fn scope(&self, session: &str, cwd: &Path) -> Scope {
        repo_scope(session, cwd)
    }

    fn refresh(&self, ctx: &RefreshCtx<'_>) -> Result<BTreeMap<String, String>, String> {
        let dirs = git::discover(ctx.cwd).ok_or_else(|| "not a git repository".to_owned())?;
        let head = match git::head(&dirs) {
            Some(Head::Branch(b)) => b,
            Some(Head::Detached(sha)) => sha.chars().take(7).collect(),
            None => String::new(),
        };
        let dirty = git::is_dirty(&dirs.toplevel, GIT_TIMEOUT)?;
        let mut values = BTreeMap::new();
        values.insert("dirty".to_owned(), if dirty { "1" } else { "0" }.to_owned());
        values.insert("head".to_owned(), head);
        Ok(values)
    }
}

/// `sync`: commits ahead of and behind the upstream.
pub struct SyncModule;

impl Module for SyncModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "sync",
            summary: "Commits ahead/behind the upstream branch.",
            doc: "Ahead/behind counts against `@{upstream}` using the remote-tracking refs already on disk (no network). The `full` preset names the upstream and hints how long ago the last fetch happened; `fetch_interval` opts into a background `git fetch`.",
            sources: &["git rev-list --left-right --count (worker)", ".git/FETCH_HEAD age"],
            refresh: 5,
            opts: vec![
                OptSpec::new(
                    "show_zero",
                    Kind::Bool,
                    "Show `0` counts instead of hiding them.",
                    Value::Bool(false),
                ),
                OptSpec::new(
                    "show_upstream",
                    Kind::Bool,
                    "Show the upstream name.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
                OptSpec::new(
                    "fetch_age",
                    Kind::Bool,
                    "Hint when the last fetch is older than `fetch_stale_minutes`.",
                    Value::Bool(true),
                )
                .minimal(Value::Bool(false)),
                OptSpec::new(
                    "fetch_stale_minutes",
                    Kind::Int,
                    "Age after which the fetch hint appears.",
                    Value::Int(30),
                ),
                OptSpec::new(
                    "fetch_interval",
                    Kind::Int,
                    "Run `git fetch` in the background every N seconds (0 = never).",
                    Value::Int(0),
                ),
            ],
            icons: vec![
                IconSpec {
                    key: "ahead",
                    doc: "Ahead glyph.",
                    glyph: glyph("⇡", "⇡", "⬆\u{fe0f}", "^"),
                },
                IconSpec {
                    key: "behind",
                    doc: "Behind glyph.",
                    glyph: glyph("⇣", "⇣", "⬇\u{fe0f}", "v"),
                },
                IconSpec {
                    key: "stale",
                    doc: "Stale-fetch glyph.",
                    glyph: glyph("\u{f017}", "⧖", "🕰\u{fe0f}", "?"),
                },
                IconSpec {
                    key: "no_upstream",
                    doc: "No-upstream glyph.",
                    glyph: glyph("\u{f127}", "⊘", "🚫", "-"),
                },
            ],
            colors: vec![
                ColorSpec { key: "ahead", doc: "Ahead count.", default: "ok" },
                ColorSpec { key: "behind", doc: "Behind count.", default: "warn" },
                ColorSpec { key: "upstream", doc: "Upstream name.", default: "muted" },
                ColorSpec { key: "stale", doc: "Fetch-age hint.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(dirs) = ctx.git_dirs() else { return Rendered::empty() };
        let Some(Head::Branch(branch)) = git::head(dirs) else { return Rendered::empty() };
        let mut segs: Vec<Segment> = Vec::new();
        let Some((remote, tracking)) = git::upstream(dirs, &branch) else {
            if !cfg.icon("no_upstream").is_empty() {
                segs.push(seg(cfg, cfg.icon("no_upstream"), "upstream"));
            }
            return Rendered::fresh(segs);
        };
        let scope = Scope::Repo(dirs.cache_key());
        let (lookup, freshness) =
            ctx.cached(cfg, &scope, |e| e.get("upstream").is_none_or(|u| u == tracking));
        let counts = lookup.entry.as_ref().and_then(|e| {
            Some((e.get("ahead")?.parse::<u64>().ok()?, e.get("behind")?.parse::<u64>().ok()?))
        });
        let show_zero = cfg.bool("show_zero");
        if let Some((ahead, behind)) = counts {
            if ahead > 0 || show_zero {
                segs.push(Segment::styled(
                    format!("{}{ahead}", cfg.icon("ahead")),
                    Style::fg(cfg.color("ahead")).bolded(),
                ));
            }
            if behind > 0 || show_zero {
                if !segs.is_empty() {
                    segs.push(Segment::plain(" "));
                }
                segs.push(Segment::styled(
                    format!("{}{behind}", cfg.icon("behind")),
                    Style::fg(cfg.color("behind")).bolded(),
                ));
            }
        }
        if cfg.bool("show_upstream") {
            let sp = if segs.is_empty() { "" } else { " " };
            let short = tracking.strip_prefix("refs/remotes/").unwrap_or(&tracking);
            segs.push(seg(cfg, format!("{sp}{short}"), "upstream"));
        }
        if cfg.bool("fetch_age")
            && let Some(age) = git::fetch_age(dirs)
            && age >= cfg.int("fetch_stale_minutes").saturating_mul(60)
            && !cfg.icon("stale").is_empty()
        {
            let sp = if segs.is_empty() { "" } else { " " };
            segs.push(seg(
                cfg,
                format!("{sp}{}{}", cfg.icon("stale"), crate::time::compact_duration(age)),
                "stale",
            ));
        }
        let _ = remote;
        let freshness = if lookup.entry.is_some() { freshness } else { Freshness::Fresh };
        Rendered { segments: segs, freshness }
    }

    fn scope(&self, session: &str, cwd: &Path) -> Scope {
        repo_scope(session, cwd)
    }

    fn refresh(&self, ctx: &RefreshCtx<'_>) -> Result<BTreeMap<String, String>, String> {
        let dirs = git::discover(ctx.cwd).ok_or_else(|| "not a git repository".to_owned())?;
        let Some(Head::Branch(branch)) = git::head(&dirs) else {
            return Err("detached HEAD".to_owned());
        };
        let (remote, tracking) =
            git::upstream(&dirs, &branch).ok_or_else(|| "no upstream".to_owned())?;
        let mut values = BTreeMap::new();
        let interval = ctx.cfg.int("fetch_interval");
        if interval > 0 && remote != "." {
            // A failed fetch (offline, bad remote) must neither hide the local
            // ahead/behind counts nor be retried on every refresh: the attempt
            // time is remembered in the entry and the interval applies to it.
            let scope = Scope::Repo(dirs.cache_key());
            let last_attempt = ctx
                .cache
                .read(&scope, ctx.cfg.id)
                .and_then(|e| e.get("fetch_attempt")?.parse::<i64>().ok());
            let now = crate::time::now_secs();
            let attempt_age = last_attempt.map(|t| now.saturating_sub(t));
            let due = attempt_age
                .is_none_or(|age| age >= i64::try_from(interval).unwrap_or(i64::MAX))
                && git::fetch_age(&dirs).is_none_or(|age| age >= interval);
            if due {
                values.insert("fetch_attempt".to_owned(), now.to_string());
                if let Err(e) = git::fetch(&dirs.toplevel, &remote, FETCH_TIMEOUT) {
                    values.insert("fetch_error".to_owned(), e);
                }
            } else if let Some(t) = last_attempt {
                values.insert("fetch_attempt".to_owned(), t.to_string());
            }
        }
        let (ahead, behind) = git::ahead_behind(&dirs.toplevel, &tracking, GIT_TIMEOUT)?;
        values.insert("ahead".to_owned(), ahead.to_string());
        values.insert("behind".to_owned(), behind.to_string());
        values.insert("upstream".to_owned(), tracking);
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers() {
        assert_eq!(tildify("/home/dev/x", Some("/home/dev")), "~/x");
        assert_eq!(tildify("/home/dev", Some("/home/dev")), "~");
        assert_eq!(tildify("/home/developer/x", Some("/home/dev")), "/home/developer/x");
        assert_eq!(tildify("/x", None), "/x");
        assert_eq!(shorten("~/projects/garnish", 2), "~/projects/garnish");
        assert_eq!(shorten("~/a/projects/garnish", 2), "~/projects/garnish");
        assert_eq!(shorten("~/projects/garnish", 1), "~/garnish");
        assert_eq!(shorten("~/projects/garnish", 0), "~/projects/garnish");
        assert_eq!(shorten("/srv/a/b/c", 2), "b/c");
        assert_eq!(shorten("garnish", 3), "garnish");
        assert_eq!(subpath("/a/b", "/a/b/c/d"), Some("c/d".into()));
        assert_eq!(subpath("/a/b", "/a/b"), None);
        assert_eq!(subpath("/a/b", "/a/c"), None);
    }
}
