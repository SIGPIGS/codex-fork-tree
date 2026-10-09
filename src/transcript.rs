//! Full-text transcript of a session's effective history, with the fork point marked.

use crate::graph::Graph;
use crate::layout::{Line, Role, fit, seg, wrap};
use crate::parse::parse;
use std::collections::HashMap;

pub struct Transcript {
    pub lines: Vec<Line>,
    /// Line index of the "forked after" rule (initial scroll target).
    pub fork_line: usize,
    /// Line index of every "you · turn N" header, for `{` / `}`.
    pub turn_lines: Vec<usize>,
}

pub fn build(g: &Graph, s: usize, title: &str, parent_title: Option<&str>, w: usize) -> Transcript {
    // full text for every source file in the lineage, keyed by session index
    let mut full: HashMap<usize, Vec<(String, String)>> = HashMap::new();
    for &(src, _) in &g.eff[s] {
        full.entry(src).or_insert_with(|| {
            parse(&g.sessions[src].path, true)
                .map(|p| p.turns.into_iter().map(|t| (t.user, t.agent)).collect())
                .unwrap_or_default()
        });
    }
    let k = g.fork_k(s);
    let mut lines: Vec<Line> = vec![];
    let mut turn_lines = vec![];
    let mut fork_line = 0;
    const INDENT: usize = 17; // "   you   turn 12 " / "   codex         "
    let text_w = w.saturating_sub(INDENT + 1);
    lines.push(fit(
        vec![
            seg(" ", Role::Fg),
            crate::layout::bold(title.to_string(), Role::Fg),
            seg(
                format!(
                    " · {} · {} turns",
                    crate::layout::short(&g.sessions[s].id),
                    g.len(s)
                ),
                Role::Dim,
            ),
        ],
        w,
    ));
    lines.push(fit(
        vec![seg(
            " injected context (AGENTS.md, environment, instructions) is not shown",
            Role::Dim,
        )],
        w,
    ));
    lines.push(vec![]);
    if k > 0 {
        lines.push(fit(
            vec![seg(
                format!(
                    " ┄ {k} shared turns from {}",
                    parent_title.unwrap_or("parent")
                ),
                Role::Dim,
            )],
            w,
        ));
        lines.push(vec![]);
    }
    for (i, &(src, ti)) in g.eff[s].iter().enumerate() {
        let n = i + 1;
        let shared = n <= k;
        let (user, agent) = full
            .get(&src)
            .and_then(|v| v.get(ti))
            .cloned()
            .unwrap_or_else(|| {
                (
                    g.turn((src, ti)).user.clone(),
                    g.turn((src, ti)).agent.clone(),
                )
            });
        let text_role = if shared { Role::Dim } else { Role::Fg };
        turn_lines.push(lines.len());
        for (j, x) in wrap(&user, text_w).into_iter().enumerate() {
            let head = if j == 0 {
                vec![
                    seg("   you   ", if shared { Role::Dim } else { Role::Accent }),
                    seg(format!("{:<8}", format!("turn {n}")), Role::Dim),
                ]
            } else {
                vec![seg(" ".repeat(INDENT), Role::Fg)]
            };
            lines.push(fit([head, vec![seg(x, text_role)]].concat(), w));
        }
        if !agent.is_empty() {
            for (j, x) in wrap(&agent, text_w).into_iter().enumerate() {
                let head = seg(
                    if j == 0 {
                        format!("{:<INDENT$}", "   codex")
                    } else {
                        " ".repeat(INDENT)
                    },
                    Role::Dim,
                );
                lines.push(fit(vec![head, seg(x, text_role)], w));
            }
        }
        lines.push(vec![]);
        if n == k {
            fork_line = lines.len();
            let label = format!(" forked after turn {k} ");
            let side = w.saturating_sub(label.chars().count()) / 2;
            lines.push(fit(
                vec![seg(
                    format!("{}{label}{}", "─".repeat(side), "─".repeat(side)),
                    Role::Fork,
                )],
                w,
            ));
            lines.push(vec![]);
        }
    }
    Transcript {
        lines,
        fork_line,
        turn_lines,
    }
}
