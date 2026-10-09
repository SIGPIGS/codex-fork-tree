//! Project overview: every fork tree and standalone session in the project, newest activity first.
//! Picking one opens its tree focused on the tree's most recently active session.

use crate::graph::Graph;
use crate::layout::{Line, Role, View, fit, lr, rel_age, seg, short};
use std::collections::HashMap;
use std::path::Path;

pub struct Entry {
    pub root: usize,
    pub members: usize,
    /// The session the tree opens on: the most recently active member.
    pub target: usize,
    pub latest: f64,
    pub archived: bool,
    search: String,
}

impl Entry {
    pub fn standalone(&self) -> bool {
        self.members == 1
    }
}

#[derive(Default)]
pub struct Picker {
    /// `--last N`: only trees that contain one of the N most recently active sessions.
    pub recent: Option<usize>,
    pub sel: usize,
    pub scroll: usize,
    pub filter: String,
    pub editing: bool,
    pub hide_standalone: bool,
}

/// Roots of the trees that contain one of the `n` most recently active sessions.
pub fn recent_roots(g: &Graph, n: usize) -> std::collections::HashSet<usize> {
    let mut by_activity: Vec<usize> = (0..g.sessions.len()).collect();
    by_activity.sort_by(|&a, &b| g.sessions[b].mtime.total_cmp(&g.sessions[a].mtime));
    by_activity.into_iter().take(n).map(|s| g.root_of(s)).collect()
}

/// The most recently active session (what `--last` opens).
pub fn most_recent(g: &Graph) -> Option<usize> {
    (0..g.sessions.len()).max_by(|&a, &b| g.sessions[a].mtime.total_cmp(&g.sessions[b].mtime))
}

/// All trees, fork trees first, each group by latest activity.
pub fn entries(g: &Graph, names: &HashMap<String, String>) -> Vec<Entry> {
    let mut out: Vec<Entry> = (0..g.sessions.len())
        .filter(|&s| g.parent(s).is_none())
        .map(|root| {
            let members = g.tree_members(root);
            let target = *members
                .iter()
                .max_by(|&&a, &&b| {
                    g.sessions[a]
                        .mtime
                        .total_cmp(&g.sessions[b].mtime)
                        .then(g.sessions[a].created.total_cmp(&g.sessions[b].created))
                })
                .unwrap_or(&root);
            let mut search = String::new();
            for &m in &members {
                let s = &g.sessions[m];
                search.push_str(&s.id);
                search.push(' ');
                if let Some(n) = names.get(&s.id) {
                    search.push_str(n);
                    search.push(' ');
                }
                if let Some(t) = g.eff_turn(m, g.fork_k(m)) {
                    search.push_str(&t.user);
                    search.push(' ');
                }
            }
            Entry {
                root,
                members: members.len(),
                target,
                latest: g.sessions[target].mtime,
                archived: members.iter().all(|&m| g.sessions[m].archived),
                search: search.to_lowercase(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.standalone()
            .cmp(&b.standalone())
            .then(b.latest.total_cmp(&a.latest))
    });
    out
}

impl Picker {
    pub fn visible<'e>(&self, all: &'e [Entry], show_archived: bool) -> Vec<&'e Entry> {
        let needle = self.filter.to_lowercase();
        all.iter()
            .filter(|e| !(self.hide_standalone && e.standalone()))
            .filter(|e| show_archived || !e.archived)
            .filter(|e| needle.is_empty() || needle.split_whitespace().all(|w| e.search.contains(w)))
            .collect()
    }

    /// Select the entry whose tree contains session `s`.
    pub fn select_tree_of(&mut self, g: &Graph, all: &[Entry], s: usize, show_archived: bool) {
        let root = g.root_of(s);
        if let Some(i) = self.visible(all, show_archived).iter().position(|e| e.root == root) {
            self.sel = i;
        }
    }
}

pub struct ListRow {
    /// Index into the visible entries, or None for a section header.
    pub entry: Option<usize>,
    pub line: Line,
}

/// List rows: a header per section, then one row per entry.
pub fn list_rows(view: &View, entries: &[&Entry], root_dir: &Path, w: usize) -> Vec<ListRow> {
    let g = view.g;
    let mut rows = vec![];
    let trees = entries.iter().filter(|e| !e.standalone()).count();
    let mut section = None;
    for (i, e) in entries.iter().enumerate() {
        let standalone = e.standalone();
        if section != Some(standalone) {
            if section.is_some() {
                rows.push(ListRow { entry: None, line: vec![] });
            }
            let (label, n) = if standalone {
                ("STANDALONE SESSIONS", entries.len() - trees)
            } else {
                ("FORK TREES", trees)
            };
            rows.push(ListRow {
                entry: None,
                line: fit(vec![seg(format!(" {label}  {n}"), Role::Dim)], w),
            });
            section = Some(standalone);
        }
        let s = e.root;
        let mut left = vec![seg(" ", Role::Fg)];
        if standalone {
            left.push(seg("○ ", Role::Fg));
            left.extend(view.title_segs(s, Role::Fg, false));
        } else {
            left.push(seg("▣ ", Role::Own));
            left.extend(view.title_segs(s, Role::Own, false));
        }
        let cwd = &g.sessions[s].cwd;
        if let Ok(rel) = Path::new(cwd).strip_prefix(root_dir) {
            if !rel.as_os_str().is_empty() {
                left.push(seg(format!(" · {}", rel.display()), Role::Dim));
            }
        }
        let size = if standalone {
            format!("{} turn{}", g.len(s), if g.len(s) == 1 { "" } else { "s" })
        } else {
            format!("{} sessions", e.members)
        };
        let right = vec![
            seg(size, Role::Dim),
            seg(" · ", Role::Dim),
            seg(format!("{:>3}", rel_age(view.now, e.latest)), Role::Dim),
            seg(" ", Role::Fg),
        ];
        rows.push(ListRow { entry: Some(i), line: lr(left, right, w) });
    }
    if entries.is_empty() {
        rows.push(ListRow {
            entry: None,
            line: fit(vec![seg("   nothing matches the filter", Role::Dim)], w),
        });
    }
    rows
}

/// One-line summary under the preview: where Enter will land.
pub fn opens_at(view: &View, e: &Entry, w: usize) -> Line {
    let mut left = vec![seg(" ⏎ opens at ", Role::Dim), seg(short(&view.g.sessions[e.target].id), Role::Accent), seg(" ", Role::Fg)];
    left.extend(view.title_segs(e.target, Role::Fg, false));
    lr(left, {
            let age = rel_age(view.now, e.latest);
            let txt = if age == "now" { "active now ".to_string() } else { format!("active {age} ago ") };
            vec![seg(txt, Role::Dim)]
        }, w)
}
