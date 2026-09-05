//! `session_name`, `vim`, `agent`, `lines`: who and what this session is.

use crate::ansi::{Segment, Style};
use crate::config::schema::{ColorSpec, IconSpec, Kind, ModuleCfg, ModuleSchema, OptSpec, Value};
use crate::icons::glyph;

use super::{Ctx, Module, Rendered, icon, seg};

/// `session_name`: the custom or AI-generated session title.
pub struct SessionNameModule;

impl Module for SessionNameModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "session_name",
            summary: "Session name.",
            doc: "The name set with `--name` or `/rename`, or the AI-generated title. Hidden when the session only has its default name. The `full` preset appends the short session id.",
            sources: &["session_name", "session_id"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_id",
                    Kind::Bool,
                    "Append the first 8 characters of the session id.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
                OptSpec::new(
                    "max_length",
                    Kind::Int,
                    "Truncate longer names (0 = no limit).",
                    Value::Int(32),
                ),
            ],
            icons: vec![IconSpec {
                key: "name",
                doc: "Name icon.",
                glyph: glyph("\u{f02b}", "❯", "🔖", ""),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "name", doc: "Name.", default: "text" },
                ColorSpec { key: "id", doc: "Session id.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(name) = ctx.payload.session_name.as_deref().filter(|n| !n.is_empty()) else {
            return Rendered::empty();
        };
        let max = cfg.size("max_length");
        let shown: String = if max > 0 && name.chars().count() > max {
            name.chars().take(max.saturating_sub(1)).chain(std::iter::once('…')).collect()
        } else {
            name.to_owned()
        };
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "name", "icon"));
        }
        segs.push(seg(cfg, shown, "name"));
        if cfg.bool("show_id")
            && let Some(id) = ctx.payload.session_id.as_deref()
        {
            let short: String = id.chars().take(8).collect();
            segs.push(seg(cfg, format!(" {short}"), "id"));
        }
        Rendered::fresh(segs)
    }
}

/// `vim`: the vim mode badge.
pub struct VimModule;

impl Module for VimModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "vim",
            summary: "Vim mode badge.",
            doc: "`vim.mode` when vim mode is enabled (`NORMAL`, `INSERT`, `VISUAL`, `VISUAL LINE`). Set `hideVimModeIndicator = true` in the `statusLine` settings so the mode is not shown twice.",
            sources: &["vim.mode"],
            refresh: 0,
            opts: vec![
                OptSpec::new(
                    "style",
                    Kind::Enum(&["badge", "short"]),
                    "Full word or one letter.",
                    Value::Str("badge".into()),
                )
                .minimal(Value::Str("short".into())),
                OptSpec::new("show_icon", Kind::Bool, "Show the vim icon.", Value::Bool(false))
                    .full(Value::Bool(true)),
            ],
            icons: vec![IconSpec {
                key: "vim",
                doc: "Vim icon.",
                glyph: glyph("\u{e62b}", "", "", ""),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "normal", doc: "NORMAL mode.", default: "accent" },
                ColorSpec { key: "insert", doc: "INSERT mode.", default: "ok" },
                ColorSpec { key: "visual", doc: "VISUAL modes.", default: "warn" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(mode) = ctx.payload.vim.as_ref().and_then(|v| v.mode.as_deref()) else {
            return Rendered::empty();
        };
        let color_key = match mode {
            "NORMAL" => "normal",
            "INSERT" => "insert",
            _ => "visual",
        };
        let text = if cfg.str("style") == "short" {
            match mode {
                "VISUAL LINE" => "VL".to_owned(),
                other => other.chars().take(1).collect(),
            }
        } else {
            mode.to_owned()
        };
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "vim", "icon"));
        }
        segs.push(Segment::styled(text, Style::fg(cfg.color(color_key)).bolded()));
        Rendered::fresh(segs)
    }
}

/// `agent`: the agent name when running with `--agent`.
pub struct AgentModule;

impl Module for AgentModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "agent",
            summary: "Agent name.",
            doc: "`agent.name` when Claude Code runs with `--agent` or agent settings. Hidden otherwise. The `full` preset adds a glyph when extended thinking is enabled.",
            sources: &["agent.name", "thinking.enabled"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new(
                    "show_thinking",
                    Kind::Bool,
                    "Show the thinking glyph.",
                    Value::Bool(false),
                )
                .full(Value::Bool(true)),
            ],
            icons: vec![
                IconSpec {
                    key: "agent",
                    doc: "Agent icon.",
                    glyph: glyph("\u{f21b}", "✪", "👤", "agent:"),
                },
                IconSpec {
                    key: "thinking",
                    doc: "Thinking glyph.",
                    glyph: glyph("\u{f0eb}", "⋯", "💭", "~"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "name", doc: "Agent name.", default: "text" },
                ColorSpec { key: "thinking", doc: "Thinking glyph.", default: "accent2" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(name) =
            ctx.payload.agent.as_ref().and_then(|a| a.name.as_deref()).filter(|n| !n.is_empty())
        else {
            return Rendered::empty();
        };
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "agent", "icon"));
        }
        segs.push(seg(cfg, name, "name"));
        if cfg.bool("show_thinking")
            && ctx.payload.thinking.as_ref().and_then(|t| t.enabled) == Some(true)
            && !cfg.icon("thinking").is_empty()
        {
            segs.push(seg(cfg, format!(" {}", cfg.icon("thinking")), "thinking"));
        }
        Rendered::fresh(segs)
    }
}

/// `lines`: lines added and removed this session.
pub struct LinesModule;

impl Module for LinesModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "lines",
            summary: "Lines added and removed this session.",
            doc: "`cost.total_lines_added` and `cost.total_lines_removed`. The `full` preset adds the net delta.",
            sources: &["cost.total_lines_added", "cost.total_lines_removed"],
            refresh: 0,
            opts: vec![
                OptSpec::new("show_icon", Kind::Bool, "Show the icon.", Value::Bool(true))
                    .minimal(Value::Bool(false)),
                OptSpec::new("show_net", Kind::Bool, "Append the net change.", Value::Bool(false))
                    .full(Value::Bool(true)),
                OptSpec::new(
                    "hide_zero",
                    Kind::Bool,
                    "Hide when nothing changed.",
                    Value::Bool(true),
                ),
            ],
            icons: vec![
                IconSpec {
                    key: "lines",
                    doc: "Diff icon.",
                    glyph: glyph("\u{f440}", "Δ", "📝", ""),
                },
                IconSpec { key: "added", doc: "Added glyph.", glyph: glyph("+", "+", "+", "+") },
                IconSpec {
                    key: "removed",
                    doc: "Removed glyph.",
                    glyph: glyph("−", "−", "−", "-"),
                },
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "added", doc: "Added count.", default: "ok" },
                ColorSpec { key: "removed", doc: "Removed count.", default: "danger" },
                ColorSpec { key: "net", doc: "Net delta.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(cost) = ctx.payload.cost.as_ref() else { return Rendered::empty() };
        let added = cost.total_lines_added.unwrap_or(0);
        let removed = cost.total_lines_removed.unwrap_or(0);
        if cfg.bool("hide_zero") && added == 0 && removed == 0 {
            return Rendered::empty();
        }
        let mut segs: Vec<Segment> = Vec::new();
        if cfg.bool("show_icon") {
            segs.extend(icon(cfg, "lines", "icon"));
        }
        segs.push(seg(cfg, format!("{}{added}", cfg.icon("added")), "added"));
        segs.push(seg(cfg, format!(" {}{removed}", cfg.icon("removed")), "removed"));
        if cfg.bool("show_net") {
            let net = i128::from(added).saturating_sub(i128::from(removed));
            let sign = if net >= 0 { "+" } else { "" };
            segs.push(seg(cfg, format!(" ({sign}{net})"), "net"));
        }
        Rendered::fresh(segs)
    }
}
