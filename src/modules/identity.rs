//! `session_name`, `vim`, `agent`, `lines`, `version`: who and what this
//! session is.

use crate::ansi::{Segment, Style};
use crate::config::schema::{
    ColorSpec, IconSpec, Kind, MeasureKind, ModuleCfg, ModuleSchema, OptSpec, Value,
};
use crate::icons::glyph;

use super::util::{
    added_removed, added_removed_colors, added_removed_icons, cut_name, first_chars,
};
use super::{Ctx, IconShown, Module, Rendered, badge, lead, seg, show_icon_opt};

/// `session_name`: the custom or AI-generated session title.
pub struct SessionNameModule;

impl Module for SessionNameModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "session_name",
            measure: None,
            summary: "Session name.",
            doc: "The name set with `--name` or `/rename`, or the AI-generated title. Hidden when the session only has its default name. The `full` preset appends the short session id.",
            sources: &["session_name", "session_id"],
            refresh: 0,
            opts: vec![
                show_icon_opt("Show the icon.", IconShown::ExceptMinimal),
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
                    "Cut a longer name to this many characters with `…` (`..` in the ascii set; 0 = no limit).",
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
        let shown = cut_name(name, cfg.size("max_length"), ctx.icons);
        let mut segs: Vec<Segment> = lead(cfg, "name");
        segs.push(seg(cfg, shown, "name"));
        if cfg.bool("show_id")
            && let Some(id) = ctx.payload.session_id.as_deref()
        {
            let short = first_chars(id, 8);
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
            measure: None,
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
                show_icon_opt("Show the vim icon.", IconShown::OnlyFull),
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
                other => first_chars(other, 1),
            }
        } else {
            mode.to_owned()
        };
        let mut segs: Vec<Segment> = lead(cfg, "vim");
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
            measure: None,
            summary: "Agent name.",
            doc: "`agent.name` when Claude Code runs with `--agent` or agent settings. Hidden otherwise. The `full` preset adds a glyph when extended thinking is enabled.",
            sources: &["agent.name", "thinking.enabled"],
            refresh: 0,
            opts: vec![
                show_icon_opt("Show the icon.", IconShown::ExceptMinimal),
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
        let mut segs: Vec<Segment> = lead(cfg, "agent");
        segs.push(seg(cfg, name, "name"));
        if cfg.bool("show_thinking")
            && ctx.payload.thinking.as_ref().and_then(|t| t.enabled) == Some(true)
        {
            segs.extend(badge(cfg, "thinking", "thinking"));
        }
        Rendered::fresh(segs)
    }
}

/// `lines`: lines added and removed this session.
pub struct LinesModule;

impl Module for LinesModule {
    fn schema(&self) -> ModuleSchema {
        let [added, removed] = added_removed_icons();
        let [added_color, removed_color] = added_removed_colors();
        ModuleSchema {
            id: "lines",
            measure: Some(MeasureKind::Count),
            summary: "Lines added and removed this session.",
            doc: "`cost.total_lines_added` and `cost.total_lines_removed`. The `full` preset adds the net delta.",
            sources: &["cost.total_lines_added", "cost.total_lines_removed"],
            refresh: 0,
            opts: vec![
                show_icon_opt("Show the icon.", IconShown::ExceptMinimal),
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
                added,
                removed,
            ],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                added_color,
                removed_color,
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
        let mut segs: Vec<Segment> = lead(cfg, "lines");
        segs.extend(added_removed(cfg, "", added, removed));
        if cfg.bool("show_net") {
            // Signed with the counts' own glyphs, so the row has one minus.
            let (glyph, net) = if added >= removed {
                (cfg.icon("added"), added.saturating_sub(removed))
            } else {
                (cfg.icon("removed"), removed.saturating_sub(added))
            };
            segs.extend(super::detail(ctx, cfg, "", &format!("{glyph}{net}"), "net"));
        }
        Rendered::fresh(segs).measured(super::Measure::Count(added.saturating_add(removed)))
    }
}

/// `version`: the Claude Code version the payload reports (SPEC § 3.8).
pub struct VersionModule;

impl Module for VersionModule {
    fn schema(&self) -> ModuleSchema {
        ModuleSchema {
            id: "version",
            measure: None,
            summary: "The Claude Code version.",
            doc: "The payload's `version`, printed dim as `v2.1.270`: what a bug report needs and what shows an upgrade. Nothing shows when the payload carries no version, so `hide_when_empty = false` prints `–` as it does for any absent field.",
            sources: &["version"],
            refresh: 0,
            opts: vec![show_icon_opt("Show the icon.", IconShown::OnlyFull)],
            icons: vec![IconSpec {
                key: "version",
                doc: "Version icon.",
                glyph: glyph("\u{f02c}", "⊛", "📦", ""),
            }],
            colors: vec![
                ColorSpec { key: "icon", doc: "Icon.", default: "accent2" },
                ColorSpec { key: "version", doc: "The version.", default: "muted" },
            ],
        }
    }

    fn render(&self, ctx: &Ctx<'_>, cfg: &ModuleCfg) -> Rendered {
        let Some(version) = ctx.payload.version.as_deref().map(str::trim).filter(|v| !v.is_empty())
        else {
            return Rendered::empty();
        };
        let mut segs: Vec<Segment> = lead(cfg, "version");
        // A payload that already says `v2.1.270` is not doubled to `vv`.
        let bare = version.strip_prefix('v').unwrap_or(version);
        segs.push(Segment::styled(format!("v{bare}"), Style::fg(cfg.color("version")).dimmed()));
        Rendered::fresh(segs)
    }
}

#[cfg(test)]
mod tests {
    use crate::ansi::strip_ansi;
    use crate::render::{Clock, render_plain_at};

    /// `version` alone on an unframed line, for a payload text.
    fn version_row(payload: &str, extra: &str) -> String {
        let payload = crate::payload::Payload::parse(payload).unwrap();
        let text = format!(
            "icons = \"unicode\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"version\"]\n[modules.version]\n{extra}"
        );
        let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()))
            .trim_end()
            .to_owned()
    }

    /// `lines` alone on an unframed line with the net delta on.
    fn lines_row(added: u64, removed: u64, icons: &str, extra: &str) -> String {
        let payload = crate::payload::Payload::parse(&format!(
            "{{\"session_id\": \"s\", \"cost\": {{\"total_lines_added\": {added}, \"total_lines_removed\": {removed}}}}}"
        ))
        .unwrap();
        let text = format!(
            "icons = \"{icons}\"\n[frame]\nstyle = \"none\"\nfill = false\n[[line]]\nmodules = [\"lines\"]\n[modules.lines]\nshow_net = true\n{extra}"
        );
        let (config, errs) = crate::config::parse(&text, &crate::modules::SCHEMAS);
        assert!(errs.is_empty(), "{errs:?}");
        strip_ansi(&render_plain_at(&payload, &config, Some(80), &Clock::fixed()))
            .trim_end()
            .to_owned()
    }

    /// The net delta is signed with the module's own glyphs, as the two
    /// counts are: it printed Rust's ASCII `-` beside the set's `−`
    /// (`+156 −200 (-44)`), and an `added`/`removed` override never
    /// reached it. A zero net reads as nothing removed, `+0`.
    #[test]
    fn the_net_delta_takes_the_modules_glyphs() {
        assert_eq!(lines_row(156, 200, "unicode", ""), "Δ +156 −200 (−44)");
        assert_eq!(lines_row(156, 23, "unicode", ""), "Δ +156 −23 (+133)");
        assert_eq!(lines_row(7, 7, "unicode", ""), "Δ +7 −7 (+0)");
        assert_eq!(lines_row(156, 200, "ascii", ""), "+156 -200 (-44)");
        let arrows = "[modules.lines.icons]\nadded = \"▲\"\nremoved = \"▼\"\n";
        assert_eq!(lines_row(1, 3, "unicode", arrows), "Δ ▲1 ▼3 (▼2)");
        assert_eq!(
            lines_row(u64::MAX, 0, "unicode", "").split(' ').next_back(),
            Some("(+18446744073709551615)")
        );
    }

    /// SPEC § 3.8: the payload's version, one `v` in front whatever the
    /// payload wrote, the icon only when asked, and nothing without the
    /// field (a placeholder only when `hide_when_empty` is off).
    #[test]
    fn version_prints_the_payload_field_once_prefixed() {
        let with = |v: &str| format!("{{\"session_id\": \"s\", \"version\": \"{v}\"}}");
        assert_eq!(version_row(&with("2.1.270"), ""), "v2.1.270");
        assert_eq!(version_row(&with("v2.1.270"), ""), "v2.1.270");
        assert_eq!(version_row(&with(" 2.1.270 "), "show_icon = true\n"), "⊛ v2.1.270");
        assert_eq!(version_row("{\"session_id\": \"s\"}", ""), "");
        assert_eq!(version_row(&with(""), "hide_when_empty = false\n"), "–");
    }
}
