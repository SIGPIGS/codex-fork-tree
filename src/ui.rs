//! Drawing: header, adaptive panes, overlays, footer (docs/02-interaction.md, docs/03-ui-style.md).

use crate::app::{App, Overlay};
use crate::layout::{Line, Role, bold, fit, line_width, lr, seg};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line as TLine, Span};
use ratatui::widgets::Paragraph;

const HELP: &[(&str, &str)] = &[
    ("j k ↑ ↓", "move"),
    ("h l ← →", "fold frame / parent · unfold / child"),
    ("[ ]", "previous / next sibling"),
    ("gg G", "first / last"),
    ("p F", "parent · back to focus"),
    ("⏎ t", "transcript (narrow: details first)"),
    ("r", "resume in codex"),
    ("n", "fork in codex"),
    ("f", "make selected the focus"),
    ("a i e", "archived · inferred · empty"),
    ("x X", "confirm / reject inferred fork"),
    ("z Z", "fold off-path · expand all"),
    ("A D", "archive · delete"),
    ("y Y", "copy id · copy path"),
    ("< >", "narrow / widen tree pane"),
    ("Ctrl-r", "re-index now"),
    ("q esc", "close / quit"),
];

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
    );
    let root = app.graph.root_of(focus);
    let shown = app.rows.iter().filter(|r| r.session.is_some()).count();
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
                bold(" codex-fork-tree", Role::Accent),
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
                bold(" codex-fork-tree", Role::Accent),
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
        Overlay::Transcript { t, scroll, .. } => {
            let max = t.lines.len().saturating_sub(body_h.max(1));
            *scroll = (*scroll).min(max);
            let lines = t
                .lines
                .iter()
                .skip(*scroll)
                .take(body_h)
                .map(|l| (fit(l.clone(), w), false))
                .collect();
            put(f, &theme, body, lines);
            true
        }
        Overlay::Help => {
            let cw = (w.saturating_sub(2)) / 2;
            let mut lines = vec![
                (
                    fit(
                        vec![
                            bold(" Keys", Role::Fg),
                            seg("  ·  any key closes", Role::Dim),
                        ],
                        w,
                    ),
                    false,
                ),
                (vec![], false),
            ];
            if w < 90 {
                for (k, d) in HELP {
                    lines.push((
                        fit(
                            vec![
                                seg(format!("  {k:<9} "), Role::Accent),
                                seg(d.to_string(), Role::Fg),
                            ],
                            w,
                        ),
                        false,
                    ));
                }
                put(f, &theme, body, lines);
                return draw_footer(f, app, area, w, &theme);
            }
            let half = HELP.len().div_ceil(2);
            for i in 0..half {
                let cell = |e: Option<&(&str, &str)>| match e {
                    Some((k, d)) => fit(
                        vec![
                            seg(format!("  {k:<9} "), Role::Accent),
                            seg(d.to_string(), Role::Fg),
                        ],
                        cw,
                    ),
                    None => fit(vec![], cw),
                };
                let mut l = cell(HELP.get(i));
                l.push(seg("  ", Role::Fg));
                l.extend(cell(HELP.get(i + half)));
                lines.push((l, false));
            }
            put(f, &theme, body, lines);
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
                        seg("Ctrl-r", Role::Accent),
                        seg(" re-index", Role::Dim),
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
                vec![seg(
                    " j/k scroll  {/} user turn  r resume  n fork  esc close",
                    Role::Dim,
                )]
            } else {
                let mut v = vec![];
                for (k, d) in [
                    ("j/k", " move  "),
                    ("⏎", " open  "),
                    ("r", " resume  "),
                    ("n", " fork  "),
                    ("f", " focus  "),
                    ("?", " help"),
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

fn pad1(l: Line) -> Line {
    let mut v = vec![seg(" ", Role::Fg)];
    v.extend(l);
    v
}
