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

/// The `path` module's `style` choices (SPEC § 3.1).
pub const PATH_STYLES: &[&str] = &["full", "fish"];

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

/// `style = "fish"` (SPEC § 3.1): every directory of the base but the last
/// abbreviated to its first character, the way the fish shell prompts.
///
/// `~/projects/garnish` reads `~/p/garnish`; a dot-directory keeps its dot
/// and its first letter (`.config` → `.c`), as fish does. A leading `~` is
/// not a segment and stays whole, the last segment is never abbreviated,
/// and a root or one-segment path is returned as is. Runs after
/// [`shorten`], so `depth` applies first.
#[must_use]
pub fn fish(path: &str) -> String {
    let (home, rest) = path.strip_prefix('~').map_or(("", path), |r| ("~", r));
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    let last = parts.len().saturating_sub(1);
    let body = parts
        .iter()
        .enumerate()
        .map(|(i, p)| if i == last { (*p).to_owned() } else { initial(p) })
        .collect::<Vec<_>>()
        .join("/");
    match (home, rest.starts_with('/'), body.is_empty()) {
        ("~", _, true) => "~".to_owned(),
        ("~", _, false) => format!("~/{body}"),
        (_, true, _) => format!("/{body}"),
        _ => body,
    }
}

/// The abbreviation of one directory name: its first character (a terminal
/// cluster, so a flag, a skin tone or a combining mark stays whole), or the
/// dot and the character after it for a dot-directory.
///
/// A segment whose abbreviation would read as `.`, `..` or nothing at all
/// is kept whole instead: `...` shortened to `..` would show the path as
/// its own parent, and a zero-width first character would show a segment
/// that is not there.
fn initial(segment: &str) -> String {
    let keep = if segment.starts_with('.') { 2 } else { 1 };
    let short: String = crate::ansi::clusters(segment).into_iter().take(keep).collect();
    if matches!(short.as_str(), "" | "." | "..") || crate::ansi::display_width(&short) == 0 {
        segment.to_owned()
    } else {
        short
    }
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
                    "style",
                    Kind::Enum(PATH_STYLES),
                    "How the base prints: `full` as is; `fish` abbreviates every directory but the last to its first character (`~/p/garnish`; a dot-directory to `.c`), as the fish shell prompts. `depth` applies first; the subpath is untouched.",
                    Value::Str("full".into()),
                ),
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
                    glyph: glyph("\u{f07b}", "❒", "📁", ""),
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
        let shown = if cfg.str("style") == "fish" { fish(&shown) } else { shown };
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
                    glyph: glyph("\u{f178}", "➔", "➡", "->"),
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
                    glyph: glyph("\u{f10c}", "❍", "🕓", ".."),
                },
                IconSpec {
                    key: "changes_requested",
                    doc: "Changes requested.",
                    glyph: glyph("✗", "✗", "❌", "xx"),
                },
                IconSpec {
                    key: "draft",
                    doc: "Draft.",
                    glyph: glyph("\u{f192}", "❏", "🚧", "wip"),
                },
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
                    "Cut the name itself to this many characters with `…` (0 = no limit); the common `max_width` caps the whole module in cells instead.",
                    Value::Int(40),
                ),
                OptSpec::new(
                    "link",
                    Kind::Bool,
                    "Link the name to the branch on the forge (`https://<host>/<owner>/<name>/tree/<branch>`, `/-/tree/` on GitLab), built from `workspace.repo` in the payload; nothing is linked without it or on a detached HEAD. GitLab is recognised by a host named after it or an open merge request, so a self-hosted GitLab on an unrelated host name links to `/tree/` until one is open.",
                    Value::Bool(false),
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
                    glyph: glyph("\u{f111}", "✱", "✨", "*"),
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
        // SPEC § 3.1 `link`: the branch on the forge, from the payload's
        // repo identity alone (no git call); a detached head has no page.
        let url = (cfg.bool("link") && !detached)
            .then(|| ctx.payload.workspace.as_ref()?.repo.as_ref())
            .flatten()
            .and_then(|repo| branch_url(repo, &head_key, is_gitlab(repo, ctx.payload)));
        let mut name_seg = Segment::styled(
            shown,
            Style::fg(cfg.color("name")).bolded().underline_if(url.is_some()),
        );
        if let Some(url) = url {
            name_seg = name_seg.with_link(url);
        }
        segs.push(name_seg);
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
                super::durations_opt(),
            ],
            icons: vec![
                IconSpec {
                    key: "ahead", doc: "Ahead glyph.", glyph: glyph("⇡", "⇡", "🔼", "^")
                },
                IconSpec {
                    key: "behind", doc: "Behind glyph.", glyph: glyph("⇣", "⇣", "🔽", "v")
                },
                IconSpec {
                    key: "stale",
                    doc: "Stale-fetch glyph.",
                    glyph: glyph("\u{f017}", "↻", "⌛", "?"),
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
        if let Some((ahead, behind)) = counts {
            segs.extend(count_segments(cfg, ctx.theme, ahead, behind, cfg.bool("show_zero")));
        }
        if cfg.bool("show_upstream") {
            let sp = if segs.is_empty() { "" } else { " " };
            let short = tracking.strip_prefix("refs/remotes/").unwrap_or(&tracking);
            segs.push(seg(cfg, format!("{sp}{short}"), "upstream"));
        }
        if cfg.bool("fetch_age")
            && let Some(age) = git::fetch_age(dirs, ctx.now.as_second())
            && age >= cfg.int("fetch_stale_minutes").saturating_mul(60)
            && !cfg.icon("stale").is_empty()
        {
            let hint = fetch_age_hint(cfg.icon("stale"), &ctx.duration(cfg, age), !segs.is_empty());
            segs.push(seg(cfg, hint, "stale"));
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
                && git::fetch_age(&dirs, now).is_none_or(|age| age >= interval);
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

/// Whether the forge is GitLab, whose tree URLs carry `/-/` (SPEC § 3.1):
/// a host named after it, or an open merge request (`pr.kind = "mr"`, the
/// payload's other GitLab signal, which covers a self-hosted name). A host
/// that names GitHub wins over the `mr` signal: the host is what serves the
/// URL, and `/-/tree/` on github.com is a 404.
fn is_gitlab(repo: &crate::payload::Repo, payload: &crate::payload::Payload) -> bool {
    let host = repo.host.as_deref().map(str::to_ascii_lowercase);
    match host.as_deref() {
        Some(h) if h.contains("gitlab") => true,
        Some(h) if h.contains("github") => false,
        _ => payload.pr.as_ref().and_then(|p| p.kind.as_deref()) == Some("mr"),
    }
}

/// The page of `branch` on the forge (SPEC § 3.1): `https://<host>/<owner>/
/// <name>/tree/<branch>`, `/-/tree/` on GitLab; `None` when the payload's
/// repo identity is incomplete, its host is not an authority, or there is
/// no branch to link to (an empty name would link to the repository root,
/// which is not the page the row claims). The path parts are
/// percent-encoded so the painter's rule (SPEC § 5: printable ASCII) holds
/// for any name.
fn branch_url(repo: &crate::payload::Repo, branch: &str, gitlab: bool) -> Option<String> {
    let host = repo.host.as_deref().and_then(authority)?;
    let owner = repo.owner.as_deref().filter(|s| !s.is_empty())?;
    let name = repo.name.as_deref().filter(|s| !s.is_empty())?;
    let branch = Some(branch).filter(|b| !b.is_empty())?;
    let tree = if gitlab { "/-/tree/" } else { "/tree/" };
    Some(format!(
        "https://{host}/{}/{}{tree}{}",
        percent_encode(owner),
        percent_encode(name),
        percent_encode(branch)
    ))
}

/// The host as a URL authority, used verbatim, or `None` when it is not one.
///
/// A host cannot be percent-encoded like a path part: a self-hosted forge
/// on a port (`gitlab.example.com:8443`) would become
/// `gitlab.example.com%3A8443`, a host that no browser resolves. It is
/// checked instead, so a name with a slash, userinfo or any other
/// authority syntax in it drops the link rather than pointing the link
/// somewhere else (SPEC § 3.1).
fn authority(host: &str) -> Option<&str> {
    let (name, port) = host.split_once(':').map_or((host, None), |(n, p)| (n, Some(p)));
    let named = !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    let ported = port.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    (named && ported).then_some(host)
}

/// Percent-encode a URL path.
///
/// The RFC 3986 unreserved characters and `/` are kept; every other byte
/// of the UTF-8 encoding becomes `%XX`, so `feature/#12` and a non-ASCII
/// name make a valid, printable-ASCII link. A `.` or `..` segment has its
/// dots encoded, so a payload-supplied owner or name cannot walk the URL
/// up to a different page when a browser normalises the path.
#[must_use]
pub fn percent_encode(s: &str) -> String {
    s.split('/').map(encode_segment).collect::<Vec<_>>().join("/")
}

/// One path segment encoded: the unreserved characters kept, every other
/// byte `%XX`, and the dots of a `.` or `..` segment encoded too.
fn encode_segment(segment: &str) -> String {
    use std::fmt::Write as _;
    let dots = matches!(segment, "." | "..");
    let mut out = String::with_capacity(segment.len());
    for b in segment.bytes() {
        match b {
            b'.' if dots => out.push_str("%2E"),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(b));
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// The `⇡N ⇣M` counts.
///
/// A non-zero count carries its `ahead`/`behind` colour; a zero shown because
/// of `show_zero` is muted, so only real drift is coloured (SPEC § 4.1 `sync`).
fn count_segments(
    cfg: &ModuleCfg,
    theme: &crate::theme::Theme,
    ahead: u64,
    behind: u64,
    show_zero: bool,
) -> Vec<Segment> {
    let mut segs: Vec<Segment> = Vec::new();
    for (count, key) in [(ahead, "ahead"), (behind, "behind")] {
        if count == 0 && !show_zero {
            continue;
        }
        if !segs.is_empty() {
            segs.push(Segment::plain(" "));
        }
        let text = format!("{}{count}", cfg.icon(key));
        segs.push(if count == 0 {
            crate::modules::muted(theme, text)
        } else {
            Segment::styled(text, Style::fg(cfg.color(key)).bolded())
        });
    }
    segs
}

/// The fetch-age hint: glyph, a space, the age (`↻ 2h13m`), preceded by a
/// space when something already stands before it. Every other module puts a
/// space between its glyph and its value; this one used not to (bug 9).
fn fetch_age_hint(icon: &str, age: &str, after_text: bool) -> String {
    let sp = if after_text { " " } else { "" };
    format!("{sp}{icon} {age}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{Overrides, Preset};
    use crate::icons::IconSet;
    use crate::theme::{Role, Theme};

    #[test]
    fn fetch_age_hint_separates_glyph_and_age() {
        assert_eq!(fetch_age_hint("↻", "2h13m", false), "↻ 2h13m");
        assert_eq!(fetch_age_hint("↻", "2h13m", true), " ↻ 2h13m");
        assert_eq!(fetch_age_hint("?", "12m", true), " ? 12m");
    }

    #[test]
    fn zero_sync_counts_are_muted_and_non_zero_coloured() {
        let theme = Theme::default();
        let cfg = ModuleCfg::resolve(
            &SyncModule.schema(),
            Preset::Default,
            IconSet::Unicode,
            &theme,
            &Overrides::default(),
        );
        let text = |segs: &[Segment]| crate::ansi::Painter::PLAIN.paint(segs);
        // Without show_zero a zero count is not shown at all.
        let segs = count_segments(&cfg, &theme, 2, 0, false);
        assert_eq!(text(&segs), "⇡2");
        assert_eq!(segs[0].style.fg, cfg.color("ahead"));
        assert!(segs[0].style.bold);
        assert_eq!(count_segments(&cfg, &theme, 0, 0, false), Vec::new());
        // With show_zero the zero is muted, the non-zero keeps its colour.
        let segs = count_segments(&cfg, &theme, 0, 3, true);
        assert_eq!(text(&segs), "⇡0 ⇣3");
        assert_eq!(segs[0].style.fg, theme.role(Role::Muted), "zero ahead is muted");
        assert!(segs[0].style.dim && !segs[0].style.bold);
        assert_eq!(segs[2].style.fg, cfg.color("behind"), "non-zero behind keeps its colour");
        let both = count_segments(&cfg, &theme, 0, 0, true);
        assert_eq!(text(&both), "⇡0 ⇣0");
        assert!(
            both.iter().filter(|s| s.text() != " ").all(|s| s.style.fg == theme.role(Role::Muted))
        );
    }

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

    /// SPEC § 3.1 `link`: the forge URL from the payload's repo identity,
    /// percent-encoded, `/-/tree/` on GitLab, nothing without a repo or
    /// on a detached head; the link is off by default.
    #[test]
    fn branch_link_is_built_from_the_payload_repo() {
        assert_eq!(percent_encode("feature/#12"), "feature/%2312");
        assert_eq!(percent_encode("fix ünï"), "fix%20%C3%BCn%C3%AF");
        assert_eq!(percent_encode("a-b.c_d~e"), "a-b.c_d~e");
        assert_eq!(percent_encode("x?y&z=1"), "x%3Fy%26z%3D1");
        let repo = |host: &str| crate::payload::Repo {
            host: Some(host.into()),
            owner: Some("dschwartz".into()),
            name: Some("garnish".into()),
        };
        assert_eq!(
            branch_url(&repo("github.com"), "feature/#12", false).as_deref(),
            Some("https://github.com/dschwartz/garnish/tree/feature/%2312")
        );
        assert_eq!(
            branch_url(&repo("gitlab.example.org"), "main", true).as_deref(),
            Some("https://gitlab.example.org/dschwartz/garnish/-/tree/main")
        );
        assert_eq!(branch_url(&crate::payload::Repo::default(), "main", false), None);
        let mut half = repo("github.com");
        half.name = Some(String::new());
        assert_eq!(branch_url(&half, "main", false), None);
        // A self-hosted forge on a port keeps its `:` (percent-encoding the
        // host would give `gitlab.example.com%3A8443`, which resolves
        // nowhere), and a host that is not an authority drops the link
        // rather than pointing it somewhere else.
        assert_eq!(
            branch_url(&repo("gitlab.example.com:8443"), "main", true).as_deref(),
            Some("https://gitlab.example.com:8443/dschwartz/garnish/-/tree/main")
        );
        for bad in [
            "evil.com/dschwartz/other",
            "user@evil.com",
            "github.com:",
            "github.com:80x",
            "exämple.com",
            ":8443",
        ] {
            assert_eq!(branch_url(&repo(bad), "main", false), None, "{bad}");
        }
        // No branch, no page: an empty name would link to the repository
        // root, which is not what the row says it points at.
        assert_eq!(branch_url(&repo("github.com"), "", false), None);
        // A payload-supplied owner or name cannot walk the URL up to
        // another page: a `.` or `..` segment has its dots encoded.
        let mut dotted = repo("github.com");
        dotted.owner = Some("a/../..".into());
        assert_eq!(
            branch_url(&dotted, "main", false).as_deref(),
            Some("https://github.com/a/%2E%2E/%2E%2E/garnish/tree/main")
        );
        assert_eq!(percent_encode("a/./b"), "a/%2E/b");
        assert_eq!(percent_encode("a/...b/c"), "a/...b/c", "only a whole dot segment");
        // Every URL built passes the painter's rule, whatever the name.
        for name in ["feature/#12", "ünïcode", "a b", "tab\tname", "\u{202e}rtl"] {
            let url = branch_url(&repo("github.com"), name, false).unwrap();
            assert!(crate::ansi::safe_link(&url), "{url}");
        }
        // GitLab: the host's name, or an open merge request on any host.
        let with_pr = |kind: Option<&str>| crate::payload::Payload {
            pr: Some(crate::payload::Pr { kind: kind.map(str::to_owned), ..Default::default() }),
            ..Default::default()
        };
        assert!(is_gitlab(&repo("gitlab.com"), &crate::payload::Payload::default()));
        assert!(is_gitlab(&repo("GitLab.example.org"), &crate::payload::Payload::default()));
        assert!(!is_gitlab(&repo("git.example.com"), &crate::payload::Payload::default()));
        assert!(is_gitlab(&repo("git.example.com"), &with_pr(Some("mr"))));
        assert!(!is_gitlab(&repo("git.example.com"), &with_pr(Some("pr"))));
        assert!(!is_gitlab(&repo("github.com"), &with_pr(None)));
        // A host that names GitHub wins over the `mr` signal: `/-/tree/`
        // on github.com is a 404, and the host is what serves the URL.
        assert!(!is_gitlab(&repo("github.com"), &with_pr(Some("mr"))));
        assert!(!is_gitlab(&repo("github.example.com"), &with_pr(Some("mr"))));
        // With no host at all the `mr` signal is all there is (and the URL
        // is dropped anyway, for want of a host).
        let no_host = crate::payload::Repo::default();
        assert!(is_gitlab(&no_host, &with_pr(Some("mr"))));
        // Through the module: the fixture's worktree branch, linked and
        // underlined only with `link = true`.
        let render = |extra: &str, fixture: &str| {
            let path =
                format!("{}/tests/fixtures/payloads/{fixture}.json", env!("CARGO_MANIFEST_DIR"));
            let payload =
                crate::payload::Payload::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            let text = format!(
                "icons = \"unicode\"\ncolor = \"always\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"branch\"]\n[modules.branch]\n{extra}"
            );
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = crate::render::Clock::fixed();
            let lines = crate::render::render_lines_at(&payload, &config, Some(80), &clock);
            lines.into_iter().next().unwrap_or_default()
        };
        let linked = render("link = true\n", "worktree-session");
        let name = linked.iter().find(|s| s.text() == "worktree-feature-x").unwrap();
        assert_eq!(
            name.link.as_deref(),
            Some("https://github.com/dschwartz/garnish/tree/worktree-feature-x")
        );
        assert!(name.style.underline);
        let plain = render("", "worktree-session");
        let name = plain.iter().find(|s| s.text() == "worktree-feature-x").unwrap();
        assert_eq!(name.link, None);
        assert!(!name.style.underline);
        // A name cut by `max_length` still links to the whole branch.
        let cut = render("link = true\nmax_length = 5\n", "worktree-session");
        let name = cut.iter().find(|s| s.link.is_some()).unwrap();
        assert_eq!(name.text(), "work…");
        assert_eq!(
            name.link.as_deref(),
            Some("https://github.com/dschwartz/garnish/tree/worktree-feature-x")
        );
    }

    /// SPEC § 3.1 `style = "fish"`: every directory but the last to its
    /// first character, `~` and the last segment whole, a dot-directory to
    /// its dot and first letter, root and one-segment paths untouched, and
    /// `depth` (which keeps the `~`) applied first.
    #[test]
    fn fish_abbreviates_every_directory_but_the_last() {
        assert_eq!(fish("~/projects/garnish"), "~/p/garnish");
        assert_eq!(fish("~"), "~");
        assert_eq!(fish("~/garnish"), "~/garnish");
        assert_eq!(fish("/"), "/");
        assert_eq!(fish("/srv/a/b/c"), "/s/a/b/c");
        assert_eq!(fish("garnish"), "garnish");
        assert_eq!(fish("projects/garnish"), "p/garnish");
        assert_eq!(fish("/home/dev/.config/garnish"), "/h/d/.c/garnish");
        assert_eq!(fish("~/Übung/ü/x"), "~/Ü/ü/x", "the first character, not the first byte");
        assert_eq!(fish("/e\u{301}tude/x"), "/e\u{301}/x", "a combining mark stays with its base");
        assert_eq!(fish("/🇺🇸flags/x"), "/🇺🇸/x", "half a flag is a different glyph");
        // An abbreviation that would read as `.`, `..` or nothing keeps the
        // whole segment: the path must never show as its own parent, and a
        // zero-width initial would show a segment that is not there.
        assert_eq!(fish("/.../x"), "/.../x");
        assert_eq!(fish("/../x"), "/../x");
        assert_eq!(fish("/./x"), "/./x");
        assert_eq!(fish("/\u{200b}hidden/x"), "/\u{200b}hidden/x");
        assert_eq!(fish(&shorten("~/repos/garnish/src", 2)), "~/g/src");
        assert_eq!(fish(&shorten("/srv/repos/garnish/src", 2)), "g/src");
        assert_eq!(fish(&shorten("~/repos/garnish/src", 0)), "~/r/g/src");
        // Through the module: the base is abbreviated after `depth`, the
        // subpath below it stays whole.
        let payload = crate::payload::Payload::parse(
            "{\"cwd\": \"/home/dev/projects/garnish/src/modules\", \"workspace\": {\"current_dir\": \"/home/dev/projects/garnish/src/modules\", \"project_dir\": \"/home/dev/projects/garnish\"}}",
        )
        .unwrap();
        let render = |depth: u8| {
            let text = format!(
                "icons = \"ascii\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"path\"]\n[modules.path]\nstyle = \"fish\"\ndepth = {depth}\n"
            );
            let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
            assert!(errs.is_empty(), "{errs:?}");
            let clock = crate::render::Clock::fixed();
            crate::render::render_plain_at(&payload, &config, Some(80), &clock)
                .trim_end()
                .to_owned()
        };
        assert_eq!(render(0), "~/p/garnish/src/modules");
        assert_eq!(render(2), "~/p/garnish/src/modules");
        assert_eq!(render(1), "~/garnish/src/modules");
    }
}
