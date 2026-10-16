//! Drawing: header, adaptive panes, overlays, footer (docs/02-interaction.md, docs/03-ui-style.md).

use crate::app::{App, Mode, Overlay};
use crate::overview;
use std::collections::HashSet;
use crate::layout::{Line, Role, bold, fit, line_width, lr, seg};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line as TLine, Span};
use ratatui::widgets::Paragraph;

/// Tree-view header link back to the project overview (click, or Esc / o / Tab).
const BACK: &str = " ‹ all trees";

const HELP: &[(&str, &str)] = &[
    ("j k ↑ ↓", "move"),
    ("h l ← →", "fold / go to parent · unfold / first child"),
    ("[ ]", "previous / next sibling"),
    ("gg G", "first / last"),
    ("p F", "parent · back to focus"),
    ("⏎ t", "transcript (narrow: details first)"),
    ("r", "resume in codex (bypass mode)"),
    ("n", "fork in codex (bypass mode)"),
    ("f", "make selected the focus"),
    ("a i e", "archived · inferred · empty"),
    ("x X", "confirm / reject inferred fork"),
    ("z Z", "fold off-path · expand all"),
    ("A D", "archive · delete"),
    ("y Y", "copy id · copy path"),
    ("< >", "narrow / widen tree pane"),
    ("Ctrl-r", "re-index now"),
    ("esc o Tab", "back to all trees (or click ‹ all trees)"),
    ("=", "comfortable / compact spacing"),
    ("mouse", "wheel · click selects · double-click focuses"),
    ("q", "quit (esc closes an overlay first)"),
];

/// What every glyph and color in the tree means: (sample, explanation).
fn legend() -> Vec<(Line, &'static str)> {
    vec![
        (vec![bold("●", Role::Accent), seg(" orange title", Role::Accent)], "focus: the conversation this tree is about"),
        (vec![seg("│ ├─", Role::Accent)], "orange lines: the path from the root to the focus"),
        (vec![seg("○", Role::Fg), seg(" title", Role::Fg)], "a session"),
        (vec![seg("▣", Role::Own), bold(" green title", Role::Own)], "a session that has forks of its own"),
        (vec![seg("◉", Role::Live)], "live: Codex is writing to it right now"),
        (vec![seg("◌", Role::Dim)], "archived (shown with a)"),
        (vec![seg("├─ └─", Role::Dim)], "fork link recorded by Codex (exact)"),
        (vec![seg("├┄ ≈", Role::Inferred)], "inferred fork: copied history, no metadata (x confirm · X reject)"),
        (
            vec![seg("turn 9", Role::Fork), seg(" · ", Role::Dim), seg("“…”", Role::Subtle)],
            "forked after turn 9; the quote is the last shared prompt",
        ),
        (vec![seg("+55 · 7d", Role::Dim)], "own turns after the fork point · age"),
        (vec![seg("▸ +3", Role::Dim)], "folded: 3 sessions hidden below (l unfolds)"),
        (vec![seg("+N new", Role::Live)], "sessions that appeared since the last refresh"),
    ]
}

fn help_lines(w: usize) -> Vec<Line> {
    let mut out: Vec<Line> = vec![
        fit(vec![bold(" Symbols", Role::Fg)], w),
        vec![],
    ];
    for (sample, text) in legend() {
        let mut l = vec![seg("  ", Role::Fg)];
        l.extend(fit(sample, 16));
        l.push(seg(" ", Role::Fg));
        l.push(seg(text, Role::Fg));
        out.push(fit(l, w));
    }
    out.push(vec![]);
    out.push(fit(vec![bold(" Keys", Role::Fg)], w));
    out.push(vec![]);
    let key_line = |(k, d): &(&str, &str), cw: usize| {
        fit(vec![seg(format!("  {k:<9} "), Role::Accent), seg(d.to_string(), Role::Fg)], cw)
    };
    if w < 90 {
        out.extend(HELP.iter().map(|e| key_line(e, w)));
    } else {
        let cw = w.saturating_sub(2) / 2;
        let half = HELP.len().div_ceil(2);
        for i in 0..half {
            let mut l = key_line(&HELP[i], cw);
            l.push(seg("  ", Role::Fg));
            if let Some(e) = HELP.get(i + half) {
                l.extend(key_line(e, cw));
            }
            out.push(l);
        }
    }
    out
}

/// Full-screen keys & symbols, reachable with `?` from the tree and the overview.
fn draw_help(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let theme = *theme;
    let w = area.width as usize;
    let body_h = (area.height as usize).saturating_sub(4);
    app.body_height = body_h;
    let lines = help_lines(w);
    let crate::app::Overlay::Help { scroll } = &mut app.overlay else { return };
    *scroll = (*scroll).min(lines.len().saturating_sub(body_h));
    put(
        f,
        &theme,
        Rect::new(area.x, area.y, area.width, 2),
        vec![
            (lr(vec![bold(" codex-fork-tree", Role::Accent), seg(" · keys & symbols", Role::Dim)], vec![], w), false),
            (rule(w), false),
        ],
    );
    let shown = lines.iter().skip(*scroll).take(body_h).map(|l| (l.clone(), false)).collect();
    put(f, &theme, Rect::new(area.x, area.y + 2, area.width, body_h as u16), shown);
    let fy = area.y + area.height - 2;
    put(f, &theme, Rect::new(area.x, fy, area.width, 1), vec![(rule(w), false)]);
    let more = lines.len() > body_h;
    put(
        f,
        &theme,
        Rect::new(area.x, fy + 1, area.width, 1),
        vec![(
            fit(
                vec![seg(if more { " j/k scroll · any other key closes" } else { " any key closes" }, Role::Dim)],
                w,
            ),
            false,
        )],
    );
}

fn tline(l: &Line, theme: &Theme, selected: bool) -> TLine<'static> {
    TLine::from(
        l.iter()
            .map(|s| Span::styled(s.text.clone(), theme.style(s.role, s.bold, selected)))
            .collect::<Vec<_>>(),
    )
}

fn put(f: &mut Frame, theme: &Theme, area: Rect, lines: Vec<(Line, bool)>) {
    let text: Vec<TLine> = lines.iter().map(|(l, sel)| tline(l, theme, *sel)).collect();
    f.render_widget(Paragraph::new(text).style(theme.base()), area);
}

fn rule(w: usize) -> Line {
    vec![seg("─".repeat(w), Role::Dim)]
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let theme = app.theme;
    let (w, h) = (area.width as usize, area.height as usize);
    f.render_widget(Paragraph::new("").style(theme.base()), area);
    if w < 40 || h < 12 {
        let mid = h / 2;
        let mut lines = vec![(vec![], false); h];
        let msg = [
            "terminal too small".to_string(),
            format!("{w}×{h} · needs 40×12"),
        ];
        for (i, m) in msg.iter().enumerate() {
            if let Some(slot) = lines.get_mut((mid + i).saturating_sub(1)) {
                let pad = w.saturating_sub(m.chars().count()) / 2;
                *slot = (
                    fit(
                        vec![
                            seg(" ".repeat(pad), Role::Fg),
                            seg(m.clone(), if i == 0 { Role::Warn } else { Role::Dim }),
                        ],
                        w,
                    ),
                    false,
                );
            }
        }
        put(f, &theme, area, lines);
        return;
    }

    app.screen_width = w;
    app.hit_tree = None;
    app.hit_list = None;
    app.hit_back = None;
    app.hit_body = None;
    if matches!(app.overlay, Overlay::Help { .. }) {
        draw_help(f, app, area, &theme);
        return;
    }
    if app.mode == Mode::Overview {
        draw_overview(f, app, area, &theme);
        return;
    }
    app.rewrap_transcript(w);
    let compact = h < 20;
    let narrow = w < 60;
    let stacked = !narrow && w < 100;
    let tw = if narrow || stacked {
        w
    } else {
        (((w - 1) as f32) * app.split).round() as usize
    };
    // ---- rows (computed before borrowing app for the header) ----
    let rows = app.view().rows(tw);
    app.rows = rows;
    app.fix_selection();
    app.tree_width = tw;
    app.narrow = narrow;

    let focus = app.focus();
    let selected = app.selected_row().cloned();
    let view = crate::layout::View::new(
        &app.graph,
        &app.names,
        focus,
        &app.collapsed,
        &app.fresh,
        (app.show_archived, app.show_inferred, app.show_empty),
        crate::app::now(),
    )
    .with_compact(app.compact);
    let root = app.graph.root_of(focus);
    let shown = app.rows.iter().filter(|r| r.key.is_some()).count();
    let project = app.root.file_name().map_or_else(
        || app.root.display().to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let status: Line = if !app.fresh.is_empty() {
        vec![seg(format!("+{} new ", app.fresh.len()), Role::Live)]
    } else {
        let secs = app.refreshed_at.elapsed().as_secs();
        vec![seg(
            if secs < 3 {
                "indexed just now ".into()
            } else {
                format!("indexed {secs}s ago ")
            },
            Role::Dim,
        )]
    };
    let title = view.base_title(focus);
    let mut header: Vec<Line> = vec![];
    let id8 = crate::layout::short(&app.focus_id).to_string();
    if compact {
        header.push(lr(
            vec![
                bold(BACK, Role::Accent),
                seg(" · focus ", Role::Dim),
                seg(id8, Role::Accent),
                seg(" ", Role::Fg),
                seg(title, Role::Fg),
            ],
            status,
            w,
        ));
    } else {
        header.push(lr(
            vec![
                bold(BACK, Role::Accent),
                seg(" · ", Role::Dim),
                seg(project, Role::Fg),
                seg(" · focus ", Role::Dim),
                seg(id8, Role::Accent),
                seg(" ", Role::Fg),
                seg(title, Role::Fg),
            ],
            vec![seg(
                format!("{shown} shown · {} in project ", app.project_count()),
                Role::Dim,
            )],
            w,
        ));
        let chip = |name: &str, on: bool| {
            vec![
                seg("[", Role::Dim),
                seg(
                    format!("{name} {}", if on { "on" } else { "off" }),
                    if on { Role::Accent } else { Role::Dim },
                ),
                seg("] ", Role::Dim),
            ]
        };
        let mut chips = vec![seg(" ", Role::Fg)];
        chips.extend(chip("archived", app.show_archived));
        chips.extend(chip("inferred", app.show_inferred));
        chips.extend(chip("empty", app.show_empty));
        header.push(lr(chips, status, w));
        let anchor = app
            .graph
            .eff_turn(root, 0)
            .map_or(String::new(), |t| t.user.clone());
        header.push(fit(
            vec![
                seg(" anchor ⌂ ", Role::Dim),
                seg(format!("“{anchor}”"), Role::Dim),
            ],
            w,
        ));
    }
    header.push(rule(w));
    let body_h = h.saturating_sub(header.len() + 2);
    app.body_height = body_h;
    app.hit_back = Some(Rect::new(area.x, area.y, crate::layout::dw(BACK) as u16, 1));
    let mut y = area.y;
    let hlen = header.len() as u16;
    put(
        f,
        &theme,
        Rect::new(area.x, y, area.width, hlen),
        header.into_iter().map(|l| (l, false)).collect(),
    );
    y += hlen;
    let body = Rect::new(area.x, y, area.width, body_h as u16);

    // ---- overlays that replace the body ----
    let full_overlay = match &mut app.overlay {
        Overlay::Transcript { t, scroll, turn, .. } => {
            let max = t.lines.len().saturating_sub(body_h.max(1));
            *scroll = (*scroll).min(max);
            let (a, b) = (
                t.turn_lines.get(*turn).copied().unwrap_or(0),
                t.turn_ends.get(*turn).copied().unwrap_or(0),
            );
            let lines = t
                .lines
                .iter()
                .enumerate()
                .skip(*scroll)
                .take(body_h)
                .map(|(i, l)| (fit(l.clone(), w), i >= a && i < b))
                .collect();
            put(f, &theme, body, lines);
            app.hit_body = Some(body);
            true
        }
        Overlay::Details if narrow => {
            let det = view.details(selected.as_ref(), w - 2);
            put(
                f,
                &theme,
                body,
                det.into_iter().map(|l| (pad1(l), false)).collect(),
            );
            true
        }
        _ => false,
    };

    if !full_overlay {
        let detail_h = if stacked { (body_h * 2 / 5).min(11) } else { 0 };
        let tree_h = if stacked {
            body_h.saturating_sub(detail_h + 1)
        } else {
            body_h
        };
        // keep the selection in view
        let sel_idx = app
            .rows
            .iter()
            .position(|r| r.key.is_some() && r.key == app.sel)
            .unwrap_or(0);
        if sel_idx < app.scroll {
            app.scroll = sel_idx;
        }
        if sel_idx >= app.scroll + tree_h {
            app.scroll = sel_idx + 1 - tree_h;
        }
        app.scroll = app.scroll.min(app.rows.len().saturating_sub(tree_h));
        let mut tree: Vec<(Line, bool)> = app
            .rows
            .iter()
            .skip(app.scroll)
            .take(tree_h)
            .map(|r| (r.line.clone(), r.key.is_some() && r.key == app.sel))
            .collect();
        let alone = app.graph.tree_members(root).len() == 1;
        if alone && tree.len() < tree_h {
            tree.push((vec![], false));
            tree.push((
                fit(
                    vec![seg(
                        "   No forks share history with this conversation.",
                        Role::Dim,
                    )],
                    tw,
                ),
                false,
            ));
            tree.push((
                fit(
                    vec![
                        seg("   ", Role::Fg),
                        seg("n", Role::Accent),
                        seg(" fork it · ", Role::Dim),
                        seg("o", Role::Accent),
                        seg(" other trees in this project", Role::Dim),
                    ],
                    tw,
                ),
                false,
            ));
        }
        put(
            f,
            &theme,
            Rect::new(body.x, body.y, tw as u16, tree_h as u16),
            tree,
        );
        app.hit_tree = Some(Rect::new(body.x, body.y, tw as u16, tree_h as u16));
        if stacked {
            put(
                f,
                &theme,
                Rect::new(body.x, body.y + tree_h as u16, area.width, 1),
                vec![(rule(w), false)],
            );
            let det = view.details(selected.as_ref(), w - 2);
            put(
                f,
                &theme,
                Rect::new(
                    body.x,
                    body.y + tree_h as u16 + 1,
                    area.width,
                    detail_h as u16,
                ),
                det.into_iter().map(|l| (pad1(l), false)).collect(),
            );
        } else if !narrow {
            let dw = w - tw - 1;
            let divider: Vec<(Line, bool)> = (0..body_h)
                .map(|_| (vec![seg("│", Role::Dim)], false))
                .collect();
            put(
                f,
                &theme,
                Rect::new(body.x + tw as u16, body.y, 1, body_h as u16),
                divider,
            );
            let det = view.details(selected.as_ref(), dw.saturating_sub(2));
            put(
                f,
                &theme,
                Rect::new(body.x + tw as u16 + 1, body.y, dw as u16, body_h as u16),
                det.into_iter().map(|l| (pad1(l), false)).collect(),
            );
        }
    }

    draw_footer(f, app, area, w, &theme);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect, w: usize, theme: &Theme) {
    let theme = *theme;
    let fy = area.y + area.height - 2;
    put(
        f,
        &theme,
        Rect::new(area.x, fy, area.width, 1),
        vec![(rule(w), false)],
    );
    let footer: Line = match &app.overlay {
        Overlay::ConfirmArchive { id, unarchive } => vec![
            seg(
                format!(
                    " {} {}? ",
                    if *unarchive { "Unarchive" } else { "Archive" },
                    crate::layout::short(id)
                ),
                Role::Warn,
            ),
            seg("y", Role::Accent),
            seg(" confirm · any other key cancels", Role::Dim),
        ],
        Overlay::ConfirmDelete { id, typed } => vec![
            seg(
                format!(
                    " Delete {} permanently? Type its first 4 characters: ",
                    crate::layout::short(id)
                ),
                Role::Danger,
            ),
            bold(format!("{typed}_"), Role::Danger),
            seg("  ⏎ confirm · esc cancel", Role::Dim),
        ],
        _ => {
            let hints: Line = if matches!(app.overlay, Overlay::Transcript { .. }) {
                let mut v = vec![];
                for (k, d) in [("j/k", " scroll  "), ("{ }", " select turn  "), ("y", " copy prompt  "), ("Y", " copy reply  "), ("esc", " close")] {
                    v.push(seg(if v.is_empty() { format!(" {k}") } else { k.to_string() }, Role::Accent));
                    v.push(seg(d, Role::Fg));
                }
                v
            } else {
                let mut v = vec![];
                for (k, d) in [
                    ("j/k", " move  "),
                    ("⏎", " open  "),
                    ("r", " resume  "),
                    ("n", " fork  "),
                    ("f", " focus  "),
                    ("esc", " all trees  "),
                    ("?", " keys & symbols"),
                ] {
                    v.push(seg(
                        if v.is_empty() {
                            format!(" {k}")
                        } else {
                            k.to_string()
                        },
                        Role::Accent,
                    ));
                    v.push(seg(d, Role::Fg));
                }
                v
            };
            let right: Line = match &app.toast {
                Some((m, role, _)) => vec![seg(format!("{m} "), *role)],
                None => vec![],
            };
            if right.is_empty() {
                hints
            } else if line_width(&right) + 10 > w {
                right
            } else {
                lr(hints, right, w)
            }
        }
    };
    put(
        f,
        &theme,
        Rect::new(area.x, fy + 1, area.width, 1),
        vec![(fit(footer, w), false)],
    );
}

/// Sessions with children that are not on the path to `focus` start folded.
fn folded_off_path(g: &crate::graph::Graph, focus: usize) -> HashSet<String> {
    let keep = g.path_to_root(focus);
    (0..g.sessions.len())
        .filter(|s| !keep.contains(s) && !g.children[*s].is_empty())
        .map(|s| g.sessions[s].id.clone())
        .collect()
}

fn draw_overview(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let theme = *theme;
    let (w, h) = (area.width as usize, area.height as usize);
    let all = app.overview_entries();
    let visible = app.picker.visible(&all, app.show_archived);
    app.picker.sel = app.picker.sel.min(visible.len().saturating_sub(1));
    let trees = all.iter().filter(|e| !e.standalone()).count();
    let project = app.root.file_name().map_or_else(|| app.root.display().to_string(), |n| n.to_string_lossy().to_string());
    let empty = HashSet::new();
    let list_view = crate::layout::View::new(&app.graph, &app.names, 0, &empty, &app.fresh, (true, true, true), crate::app::now());

    // ---- header ----
    let mut header: Vec<Line> = vec![lr(
        vec![bold(" codex-fork-tree", Role::Accent), seg(" · ", Role::Dim), seg(project, Role::Fg), seg(" · overview", Role::Dim)],
        vec![seg(format!("{trees} fork trees · {} standalone ", all.len() - trees), Role::Dim)],
        w,
    )];
    let p = &app.picker;
    if p.editing || !p.filter.is_empty() {
        let mut l = vec![seg(" / ", Role::Accent), seg(p.filter.clone(), Role::Fg)];
        if p.editing {
            l.push(seg("_", Role::Accent));
        }
        header.push(lr(l, vec![seg(format!("{} match ", visible.len()), Role::Dim)], w));
    } else {
        let chip = |name: &str, on: bool| {
            vec![
                seg("[", Role::Dim),
                seg(format!("{name} {}", if on { "on" } else { "off" }), if on { Role::Accent } else { Role::Dim }),
                seg("] ", Role::Dim),
            ]
        };
        let mut chips = vec![seg(" ", Role::Fg)];
        if let Some(n) = p.recent {
            chips.extend(vec![
                seg("[", Role::Dim),
                seg(format!("last {n} sessions"), Role::Accent),
                seg(" · esc shows all] ", Role::Dim),
            ]);
        }
        chips.extend(chip("standalone", !p.hide_standalone));
        chips.extend(chip("archived", app.show_archived));
        header.push(fit(chips, w));
    }
    header.push(rule(w));
    let body_h = h.saturating_sub(header.len() + 2);
    app.body_height = body_h;
    let hlen = header.len() as u16;
    put(f, &theme, Rect::new(area.x, area.y, area.width, hlen), header.into_iter().map(|l| (l, false)).collect());
    let by = area.y + hlen;

    // ---- geometry: side by side ≥ 100 cols, stacked 60–99, list only below ----
    let side = w >= 100;
    let stacked = !side && w >= 60;
    let lw = if side { ((w - 1) as f32 * 0.45).round() as usize } else { w };
    let list_h = if stacked { body_h * 11 / 20 } else { body_h };

    // ---- list ----
    let rows = overview::list_rows(&list_view, &visible, &app.root, lw);
    let sel_row = rows.iter().position(|r| r.entry == Some(app.picker.sel)).unwrap_or(0);
    let pk = &mut app.picker;
    if sel_row < pk.scroll + 1 {
        pk.scroll = sel_row.saturating_sub(1); // keep the section header in view when possible
    }
    if sel_row >= pk.scroll + list_h {
        pk.scroll = sel_row + 1 - list_h;
    }
    let lines: Vec<(Line, bool)> = rows
        .iter()
        .skip(pk.scroll)
        .take(list_h)
        .map(|r| (r.line.clone(), r.entry.is_some() && r.entry == Some(pk.sel)))
        .collect();
    put(f, &theme, Rect::new(area.x, by, lw as u16, list_h as u16), lines);
    app.hit_list = Some(Rect::new(area.x, by, lw as u16, list_h as u16));
    app.hit_list_rows = rows.iter().skip(app.picker.scroll).take(list_h).map(|r| r.entry).collect();

    // ---- preview of the selected tree, focused where Enter will land ----
    if let Some(e) = visible.get(app.picker.sel) {
        let (px, py, pw, ph) = if side {
            let divider: Vec<(Line, bool)> = (0..body_h).map(|_| (vec![seg("│", Role::Dim)], false)).collect();
            put(f, &theme, Rect::new(area.x + lw as u16, by, 1, body_h as u16), divider);
            (area.x + lw as u16 + 1, by, w - lw - 1, body_h)
        } else if stacked {
            put(f, &theme, Rect::new(area.x, by + list_h as u16, area.width, 1), vec![(rule(w), false)]);
            (area.x, by + list_h as u16 + 1, w, body_h.saturating_sub(list_h + 1))
        } else {
            (0, 0, 0, 0)
        };
        if pw >= 30 && ph >= 3 {
            let folded = folded_off_path(&app.graph, e.target);
            let view = crate::layout::View::new(
                &app.graph,
                &app.names,
                e.target,
                &folded,
                &app.fresh,
                (app.show_archived, app.show_inferred, app.show_empty),
                crate::app::now(),
            )
            .with_compact(app.compact);
            let tree = view.rows(pw.saturating_sub(1));
            let target_key = crate::layout::Key::Session(app.graph.sessions[e.target].id.clone());
            let at = tree.iter().position(|r| r.key.as_ref() == Some(&target_key)).unwrap_or(0);
            let room = ph.saturating_sub(2);
            // show the tree from its root when the target fits; otherwise center the target
            let start = if at < room { 0 } else { at.saturating_sub(room / 2).min(tree.len().saturating_sub(room)) };
            let mut lines = vec![(pad1(overview::opens_at(&view, e, pw - 1)), false), (vec![], false)];
            lines.extend(tree.iter().skip(start).take(room).map(|r| (pad1(r.line.clone()), r.key.as_ref() == Some(&target_key))));
            put(f, &theme, Rect::new(px, py, pw as u16, ph as u16), lines);
        }
    }

    // ---- footer ----
    let fy = area.y + area.height - 2;
    put(f, &theme, Rect::new(area.x, fy, area.width, 1), vec![(rule(w), false)]);
    let mut hints = vec![];
    let keys: &[(&str, &str)] = if app.picker.editing {
        &[("type", " to filter  "), ("⏎", " done  "), ("esc", " clear")]
    } else if app.has_tree {
        &[("j/k", " move  "), ("⏎", " open  "), ("/", " filter  "), ("s", " standalone  "), ("o", " back  "), ("?", " symbols  "), ("q", " quit")]
    } else {
        &[("j/k", " move  "), ("⏎", " open  "), ("/", " filter  "), ("s", " standalone  "), ("?", " symbols  "), ("q", " quit")]
    };
    for (k, d) in keys {
        hints.push(seg(if hints.is_empty() { format!(" {k}") } else { k.to_string() }, Role::Accent));
        hints.push(seg(*d, Role::Fg));
    }
    if all.is_empty() {
        hints = vec![seg(" no sessions with conversation turns in this project", Role::Dim)];
    }
    put(f, &theme, Rect::new(area.x, fy + 1, area.width, 1), vec![(fit(hints, w), false)]);
}

fn pad1(l: Line) -> Line {
    let mut v = vec![seg(" ", Role::Fg)];
    v.extend(l);
    v
}
