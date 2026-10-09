//! Framed-trie rows and the details pane (docs/03-ui-style.md, docs/06-algorithms.md §7).
//!
//! Everything here produces lines of styled segments whose display width is exact, so the same
//! output feeds both the TUI and `tree --plain`.

use crate::graph::{EdgeKind, Graph};
use std::collections::{HashMap, HashSet};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Fg,
    Dim,
    Accent,
    Own,
    Live,
    Fork,
    Inferred,
    Warn,
    Danger,
}

#[derive(Clone, Debug)]
pub struct Seg {
    pub text: String,
    pub role: Role,
    pub bold: bool,
}

pub type Line = Vec<Seg>;

pub fn seg(text: impl Into<String>, role: Role) -> Seg {
    Seg {
        text: text.into(),
        role,
        bold: false,
    }
}

pub fn bold(text: impl Into<String>, role: Role) -> Seg {
    Seg {
        text: text.into(),
        role,
        bold: true,
    }
}

pub fn dw(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub fn line_width(l: &[Seg]) -> usize {
    l.iter().map(|s| dw(&s.text)).sum()
}

/// Truncate to at most `w` cells (with `…`) and pad to exactly `w`.
pub fn fit(segs: Line, w: usize) -> Line {
    let total = line_width(&segs);
    let mut out = Vec::with_capacity(segs.len() + 1);
    if total <= w {
        out.extend(segs);
        if total < w {
            out.push(seg(" ".repeat(w - total), Role::Fg));
        }
        return out;
    }
    let mut n = 0;
    'outer: for s in segs {
        let mut t = String::new();
        for c in s.text.chars() {
            let cw = c.width().unwrap_or(0);
            if n + cw > w.saturating_sub(1) {
                if !t.is_empty() {
                    out.push(Seg {
                        text: t,
                        ..s.clone()
                    });
                }
                out.push(Seg {
                    text: "…".into(),
                    ..s
                });
                n += 1;
                break 'outer;
            }
            t.push(c);
            n += cw;
        }
        out.push(Seg { text: t, ..s });
    }
    if n < w {
        out.push(seg(" ".repeat(w - n), Role::Fg));
    }
    out
}

/// Left segments, then right segments flush to the right edge; the left side gives way.
pub fn lr(left: Line, right: Line, w: usize) -> Line {
    let rw = line_width(&right);
    if rw + 8 > w {
        return fit(left, w);
    }
    let mut out = fit(left, w - rw - 1);
    out.push(seg(" ", Role::Fg));
    out.extend(right);
    out
}

/// Word wrap by display width; breaks inside a word (or CJK run) when it alone is too wide.
pub fn wrap(text: &str, w: usize) -> Vec<String> {
    let w = w.max(4);
    let mut lines = vec![];
    let mut cur = String::new();
    let mut cur_w = 0;
    for word in text.split(' ') {
        let ww = dw(word);
        if cur_w > 0 && cur_w + 1 + ww <= w {
            cur.push(' ');
            cur.push_str(word);
            cur_w += 1 + ww;
            continue;
        }
        if cur_w > 0 {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        for c in word.chars() {
            let cw = c.width().unwrap_or(0);
            if cur_w + cw > w {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            cur.push(c);
            cur_w += cw;
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

pub fn rel_age(now: f64, t: f64) -> String {
    let h = (now - t) / 3600.0;
    if h < 1.0 / 60.0 * 5.0 {
        "now".into()
    } else if h < 1.0 {
        format!("{}m", (h * 60.0).round() as i64)
    } else if h < 24.0 {
        format!("{}h", h.round() as i64)
    } else if h < 24.0 * 7.0 {
        format!("{}d", (h / 24.0).round() as i64)
    } else if h < 24.0 * 60.0 {
        format!("{}w", (h / 168.0).round() as i64)
    } else {
        format!("{}mo", (h / 730.0).round() as i64)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Session(String),
    Fork(String, usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    FrameTop,
    Fork,
    Leaf,
    Border,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub key: Option<Key>,
    pub parent: Option<Key>,
    pub kind: RowKind,
    pub session: Option<usize>,
    /// (session owning the fork point, k, previous fork point)
    pub fork: Option<(usize, usize, usize)>,
    pub line: Line,
}

pub struct View<'a> {
    pub g: &'a Graph,
    pub names: &'a HashMap<String, String>,
    pub focus: usize,
    pub collapsed: &'a HashSet<String>,
    pub fresh: &'a HashSet<String>,
    pub show_archived: bool,
    pub show_inferred: bool,
    pub show_empty: bool,
    pub now: f64,
    pub path: HashSet<usize>,
}

pub const MIN_FRAME: usize = 30;
const LIVE_WINDOW_S: f64 = 5.0;

struct BoxChars {
    tl: &'static str,
    tr: &'static str,
    bl: &'static str,
    br: &'static str,
    h: &'static str,
    v: &'static str,
    role: Role,
}

impl<'a> View<'a> {
    pub fn new(
        g: &'a Graph,
        names: &'a HashMap<String, String>,
        focus: usize,
        collapsed: &'a HashSet<String>,
        fresh: &'a HashSet<String>,
        toggles: (bool, bool, bool),
        now: f64,
    ) -> View<'a> {
        View {
            g,
            names,
            focus,
            collapsed,
            fresh,
            show_archived: toggles.0,
            show_inferred: toggles.1,
            show_empty: toggles.2,
            now,
            path: g.path_to_root(focus),
        }
    }

    pub fn is_live(&self, s: usize) -> bool {
        self.now - self.g.sessions[s].mtime < LIVE_WINDOW_S
    }

    pub fn is_inferred(&self, s: usize) -> bool {
        self.g.edge[s].is_some_and(|e| e.kind == EdgeKind::Inferred)
    }

    pub fn visible(&self, s: usize) -> bool {
        if self.path.contains(&s) {
            return true;
        }
        let sess = &self.g.sessions[s];
        if sess.archived && !self.show_archived {
            return false;
        }
        if self.is_inferred(s) && !self.show_inferred {
            return false;
        }
        if self.g.own(s) == 0 && !self.show_empty && self.g.children[s].is_empty() {
            return false;
        }
        true
    }

    pub fn kids(&self, s: usize) -> Vec<usize> {
        self.g.children[s]
            .iter()
            .copied()
            .filter(|&c| self.visible(c))
            .collect()
    }

    pub fn desc_count(&self, s: usize) -> usize {
        self.kids(s).iter().map(|&c| 1 + self.desc_count(c)).sum()
    }

    pub fn first_own_prompt(&self, s: usize) -> Option<&str> {
        self.g
            .eff_turn(s, self.g.fork_k(s))
            .map(|t| t.user.as_str())
    }

    pub fn base_title(&self, s: usize) -> String {
        let id = &self.g.sessions[s].id;
        self.names
            .get(id)
            .cloned()
            .or_else(|| self.first_own_prompt(s).map(str::to_string))
            .unwrap_or_else(|| "(no messages yet)".into())
    }

    /// Thread names are inherited by forks; when a parent or sibling shares the name, add the
    /// first own prompt as a dim disambiguator.
    pub fn title_segs(&self, s: usize, role: Role, is_bold: bool) -> Line {
        let t = self.base_title(s);
        let mut out = vec![Seg {
            text: t.clone(),
            role,
            bold: is_bold,
        }];
        if self.names.contains_key(&self.g.sessions[s].id) {
            let clash = self.g.parent(s).is_some_and(|p| {
                self.base_title(p) == t
                    || self.g.children[p]
                        .iter()
                        .any(|&c| c != s && self.base_title(c) == t)
            }) || self.g.children[s].iter().any(|&c| self.base_title(c) == t);
            if clash {
                if let Some(p) = self.first_own_prompt(s) {
                    out.push(seg(format!(" · “{p}”"), Role::Dim));
                }
            }
        }
        out
    }

    fn leaf_marker(&self, s: usize) -> Seg {
        if s == self.focus {
            bold("●", Role::Accent)
        } else if self.is_live(s) {
            seg("◉", Role::Live)
        } else if self.g.sessions[s].archived {
            seg("◌", Role::Dim)
        } else {
            seg("○", Role::Fg)
        }
    }

    fn owner_marker(&self, s: usize) -> Seg {
        if s == self.focus {
            bold("▣", Role::Accent)
        } else if self.is_live(s) {
            seg("▣", Role::Live)
        } else {
            seg("▣", Role::Own)
        }
    }

    fn meta(&self, s: usize, from: usize) -> Line {
        let mut m = vec![];
        if self.is_inferred(s) {
            m.push(seg("≈ ", Role::Inferred));
        }
        m.push(seg(
            format!("+{}", self.g.len(s).saturating_sub(from)),
            Role::Dim,
        ));
        m.push(seg(" · ", Role::Dim));
        m.push(seg(
            format!("{:>3}", rel_age(self.now, self.g.sessions[s].created)),
            Role::Dim,
        ));
        m
    }

    fn box_chars(&self, s: usize) -> BoxChars {
        if s == self.focus {
            BoxChars {
                tl: "┏",
                tr: "┓",
                bl: "┗",
                br: "┛",
                h: "━",
                v: "┃",
                role: Role::Accent,
            }
        } else if self.is_inferred(s) {
            BoxChars {
                tl: "╭",
                tr: "╮",
                bl: "╰",
                br: "╯",
                h: "┄",
                v: "┆",
                role: Role::Inferred,
            }
        } else {
            BoxChars {
                tl: "╭",
                tr: "╮",
                bl: "╰",
                br: "╯",
                h: "─",
                v: "│",
                role: Role::Dim,
            }
        }
    }

    fn framed(&self, s: usize, width: usize) -> bool {
        width >= MIN_FRAME
            && !self.collapsed.contains(&self.g.sessions[s].id)
            && !self.kids(s).is_empty()
    }

    fn edge_style(&self, s: usize) -> (&'static str, Role) {
        if self.is_inferred(s) {
            ("┄", Role::Inferred)
        } else {
            ("─", Role::Dim)
        }
    }

    fn leaf(
        &self,
        s: usize,
        from: usize,
        w: usize,
        conn: Option<(String, Role)>,
        parent: Option<Key>,
    ) -> Row {
        let mut left = vec![];
        if let Some((c, r)) = conn {
            left.push(seg(c, r));
        }
        left.push(self.leaf_marker(s));
        left.push(seg(" ", Role::Fg));
        let sess = &self.g.sessions[s];
        let role = if self.fresh.contains(&sess.id) {
            Role::Live
        } else if sess.archived {
            Role::Dim
        } else {
            Role::Fg
        };
        left.extend(self.title_segs(s, role, s == self.focus));
        let n = self.desc_count(s);
        if n > 0 {
            left.push(seg(format!(" ▸ +{n}"), Role::Dim));
        }
        Row {
            key: Some(Key::Session(sess.id.clone())),
            parent,
            kind: RowKind::Leaf,
            session: Some(s),
            fork: None,
            line: lr(left, self.meta(s, from), w),
        }
    }

    fn frame(&self, s: usize, from: usize, fw: usize, parent: Option<Key>, out: &mut Vec<Row>) {
        let bx = self.box_chars(s);
        let id = self.g.sessions[s].id.clone();
        let key = Key::Session(id.clone());
        let lab = self.meta(s, from);
        let lab_w = line_width(&lab);
        let t_segs = self.title_segs(s, Role::Own, s == self.focus);
        let t_max = fw.saturating_sub(11 + lab_w).max(4);
        let t_w = line_width(&t_segs).min(t_max);
        let title = fit(t_segs, t_w);
        let fill = fw.saturating_sub(10 + t_w + lab_w).max(1);
        let mut top = vec![
            seg(format!("{}{} ", bx.tl, bx.h), bx.role),
            self.owner_marker(s),
            seg(" ", Role::Fg),
        ];
        top.extend(title);
        top.push(seg(format!(" {} ", bx.h.repeat(fill)), bx.role));
        top.extend(lab);
        top.push(seg(format!(" {}{}", bx.h, bx.tr), bx.role));
        out.push(Row {
            key: Some(key.clone()),
            parent,
            kind: RowKind::FrameTop,
            session: Some(s),
            fork: None,
            line: top,
        });

        let iw = fw - 4;
        let kids: Vec<usize> = self
            .kids(s)
            .into_iter()
            .filter(|&c| self.g.fork_k(c) > from)
            .collect();
        let mut forks: Vec<usize> = kids.iter().map(|&c| self.g.fork_k(c)).collect();
        forks.dedup();
        let mut inner: Vec<Row> = vec![];
        let mut prev = from;
        for (fi, &k) in forks.iter().enumerate() {
            let fk = Key::Fork(id.clone(), k);
            let prompt = self.g.eff_turn(s, k - 1).map_or("?", |t| t.user.as_str());
            inner.push(Row {
                key: Some(fk.clone()),
                parent: Some(key.clone()),
                kind: RowKind::Fork,
                session: None,
                fork: Some((s, k, prev)),
                line: lr(
                    vec![
                        seg("◆ ", Role::Fork),
                        seg(format!("turn {k} "), Role::Dim),
                        seg(format!("“{prompt}”"), Role::Fork),
                    ],
                    vec![seg(format!("+{} turns", k - prev), Role::Dim)],
                    iw,
                ),
            });
            prev = k;
            let group: Vec<usize> = kids
                .iter()
                .copied()
                .filter(|&c| self.g.fork_k(c) == k)
                .collect();
            for (j, &c) in group.iter().enumerate() {
                let last = fi == forks.len() - 1 && j == group.len() - 1;
                let (h, role) = self.edge_style(c);
                let conn = format!("{}{}", if last { "└" } else { "├" }, h);
                let cont = if last { "  " } else { "│ " };
                if iw >= 2 && self.framed(c, iw - 2) {
                    let mut sub = vec![];
                    self.frame(c, k, iw - 2, Some(fk.clone()), &mut sub);
                    for (i, mut r) in sub.into_iter().enumerate() {
                        let p = if i == 0 {
                            seg(conn.clone(), role)
                        } else {
                            seg(cont, Role::Dim)
                        };
                        r.line.insert(0, p);
                        inner.push(r);
                    }
                } else {
                    inner.push(self.leaf(c, k, iw, Some((conn, role)), Some(fk.clone())));
                }
            }
        }
        for mut r in inner {
            r.line.insert(0, seg(format!("{} ", bx.v), bx.role));
            r.line.push(seg(format!(" {}", bx.v), bx.role));
            out.push(r);
        }
        let end = format!(" ends at turn {} ", self.g.len(s));
        out.push(Row {
            key: None,
            parent: None,
            kind: RowKind::Border,
            session: None,
            fork: None,
            line: vec![
                seg(
                    format!(
                        "{}{}",
                        bx.bl,
                        bx.h.repeat(fw.saturating_sub(3 + dw(&end)).max(1))
                    ),
                    bx.role,
                ),
                seg(end, Role::Dim),
                seg(format!("{}{}", bx.h, bx.br), bx.role),
            ],
        });
    }

    /// All rows of the focus tree, each exactly `w` cells wide.
    pub fn rows(&self, w: usize) -> Vec<Row> {
        let root = self.g.root_of(self.focus);
        let inner = w.saturating_sub(1);
        let mut rows = vec![];
        if self.framed(root, inner) {
            self.frame(root, 0, inner, None, &mut rows);
        } else {
            rows.push(self.leaf(root, 0, inner, None, None));
        }
        for r in &mut rows {
            r.line.push(seg(" ", Role::Fg)); // one column of air before the pane divider
        }
        rows
    }

    // ---- details pane ------------------------------------------------------------------------

    pub fn details(&self, row: Option<&Row>, w: usize) -> Vec<Line> {
        let mut l: Vec<Line> = vec![];
        let Some(row) = row else { return l };
        let push = |l: &mut Vec<Line>, segs: Line| l.push(fit(segs, w));
        if let Some((s, k, _)) = row.fork {
            let mut br = vec![s];
            br.extend(self.kids(s).into_iter().filter(|&c| self.g.fork_k(c) == k));
            push(
                &mut l,
                vec![
                    bold("Fork point", Role::Fg),
                    seg(format!("  ·  turn {k}"), Role::Dim),
                ],
            );
            push(
                &mut l,
                vec![
                    seg(format!("in {} ", short(&self.g.sessions[s].id)), Role::Dim),
                    seg(self.base_title(s), Role::Own),
                ],
            );
            push(&mut l, vec![]);
            push(&mut l, vec![seg("LAST SHARED PROMPT", Role::Dim)]);
            let prompt = self
                .g
                .eff_turn(s, k - 1)
                .map_or(String::new(), |t| t.user.clone());
            for x in wrap(&prompt, w.saturating_sub(4)).into_iter().take(4) {
                push(&mut l, vec![seg(format!("  {x}"), Role::Fork)]);
            }
            push(&mut l, vec![]);
            push(
                &mut l,
                vec![seg(format!("BRANCHES  {}", br.len()), Role::Dim)],
            );
            for (j, &b) in br.iter().enumerate() {
                let note = if j == 0 {
                    seg("  (continues)", Role::Dim)
                } else if self.is_inferred(b) {
                    seg("  ≈ inferred", Role::Inferred)
                } else {
                    seg(
                        format!("  {}", self.g.edge[b].map_or("", |e| e.kind.label())),
                        Role::Dim,
                    )
                };
                let mut line = vec![
                    seg("  ", Role::Fg),
                    if j == 0 {
                        self.owner_marker(b)
                    } else {
                        self.leaf_marker(b)
                    },
                    seg(" ", Role::Fg),
                ];
                line.extend(self.title_segs(
                    b,
                    if j == 0 { Role::Own } else { Role::Fg },
                    b == self.focus,
                ));
                line.push(note);
                push(&mut l, line);
            }
            push(&mut l, vec![]);
            push(
                &mut l,
                vec![seg(
                    format!("shared before this point: {k} turns"),
                    Role::Dim,
                )],
            );
            return l;
        }
        let Some(s) = row.session else { return l };
        let sess = &self.g.sessions[s];
        let framed = row.kind == RowKind::FrameTop;
        let badge = if s == self.focus {
            seg(if framed { "▣ focus" } else { "● focus" }, Role::Accent)
        } else if self.is_live(s) {
            seg("◉ live", Role::Live)
        } else if sess.archived {
            seg("◌ archived", Role::Dim)
        } else if framed {
            seg("▣ has forks", Role::Own)
        } else {
            seg("", Role::Fg)
        };
        l.push(lr(
            self.title_segs(s, if framed { Role::Own } else { Role::Fg }, true),
            vec![badge],
            w,
        ));
        l.push(lr(
            vec![seg(sess.id.clone(), Role::Dim)],
            vec![seg(
                format!("{} · {}", sess.source, sess.cli_version),
                Role::Dim,
            )],
            w,
        ));
        let created = chrono::DateTime::from_timestamp(sess.created as i64, 0)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        push(
            &mut l,
            vec![seg(
                format!(
                    "created {created} · {} ago · {} turns",
                    rel_age(self.now, sess.created),
                    self.g.len(s)
                ),
                Role::Dim,
            )],
        );
        push(&mut l, vec![]);
        if let Some(e) = self.g.edge[s] {
            let p = e.parent;
            push(&mut l, vec![seg("FORK", Role::Dim)]);
            let kind_role = if e.kind == EdgeKind::Inferred {
                Role::Inferred
            } else {
                Role::Dim
            };
            let mut from = vec![
                seg("  from   ", Role::Fg),
                seg(format!("{} ", short(&self.g.sessions[p].id)), Role::Dim),
            ];
            from.extend(self.title_segs(p, Role::Own, false));
            l.push(lr(from, vec![seg(e.kind.label(), kind_role)], w));
            let prompt = self
                .g
                .eff_turn(p, e.k - 1)
                .map_or(String::new(), |t| t.user.clone());
            push(
                &mut l,
                vec![
                    seg(format!("  after  turn {}  ", e.k), Role::Fg),
                    seg(format!("“{prompt}”"), Role::Fork),
                ],
            );
            push(
                &mut l,
                vec![seg(
                    format!("  shared {} turns · own {} turns", e.k, self.g.own(s)),
                    Role::Fg,
                )],
            );
            if e.kind == EdgeKind::Inferred {
                push(
                    &mut l,
                    vec![
                        seg("  match  copied history · ", Role::Inferred),
                        seg("x", Role::Accent),
                        seg(" confirm  ", Role::Inferred),
                        seg("X", Role::Accent),
                        seg(" reject", Role::Inferred),
                    ],
                );
                for &o in self.g.also_matches[s].iter().take(3) {
                    push(
                        &mut l,
                        vec![seg(
                            format!(
                                "  also   {} {}",
                                short(&self.g.sessions[o].id),
                                self.base_title(o)
                            ),
                            Role::Dim,
                        )],
                    );
                }
            }
        } else {
            push(
                &mut l,
                vec![
                    seg("ROOT", Role::Dim),
                    seg("  every session in this tree starts here", Role::Dim),
                ],
            );
        }
        let sibs = self
            .g
            .parent(s)
            .map_or(0, |p| self.kids(p).len().saturating_sub(1));
        push(
            &mut l,
            vec![seg(
                format!("  children {} · siblings {sibs}", self.kids(s).len()),
                Role::Dim,
            )],
        );
        push(&mut l, vec![]);
        push(
            &mut l,
            vec![seg(
                if self.g.edge[s].is_some() {
                    "FIRST OWN PROMPT"
                } else {
                    "FIRST PROMPT"
                },
                Role::Dim,
            )],
        );
        let first = self.first_own_prompt(s).unwrap_or("(none yet)").to_string();
        for (i, x) in wrap(&first, w.saturating_sub(9))
            .into_iter()
            .take(2)
            .enumerate()
        {
            push(
                &mut l,
                vec![
                    seg(if i == 0 { "  you    " } else { "         " }, Role::Accent),
                    seg(x, Role::Fg),
                ],
            );
        }
        if let Some(last) = self.g.eff[s].last().map(|&r| self.g.turn(r)) {
            push(&mut l, vec![]);
            push(&mut l, vec![seg("LAST EXCHANGE", Role::Dim)]);
            push(
                &mut l,
                vec![
                    seg("  you    ", Role::Accent),
                    seg(last.user.clone(), Role::Fg),
                ],
            );
            for (i, x) in wrap(&last.agent, w.saturating_sub(9))
                .into_iter()
                .take(2)
                .enumerate()
            {
                push(
                    &mut l,
                    vec![
                        seg(if i == 0 { "  codex  " } else { "         " }, Role::Dim),
                        seg(x, Role::Fg),
                    ],
                );
            }
        }
        push(&mut l, vec![]);
        push(
            &mut l,
            vec![
                seg("r", Role::Accent),
                seg(" resume  ", Role::Fg),
                seg("n", Role::Accent),
                seg(" fork  ", Role::Fg),
                seg("t", Role::Accent),
                seg(" transcript  ", Role::Fg),
                seg("f", Role::Accent),
                seg(" focus  ", Role::Fg),
                seg("y", Role::Accent),
                seg(" copy id", Role::Fg),
            ],
        );
        l
    }
}

pub fn short(id: &str) -> &str {
    &id[..8.min(id.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_is_exact_with_wide_chars() {
        let l = fit(
            vec![
                seg("键值存储的写入路径设计", Role::Fg),
                seg(" tail", Role::Dim),
            ],
            9,
        );
        assert_eq!(line_width(&l), 9);
        let l = fit(vec![seg("ab", Role::Fg)], 5);
        assert_eq!(line_width(&l), 5);
    }

    #[test]
    fn wrap_handles_cjk_without_spaces() {
        for line in wrap(
            "先运行崩溃恢复测试 tests/crash_recovery.rs，然后报告失败的用例 summary",
            12,
        ) {
            assert!(dw(&line) <= 12, "{line}");
        }
    }
}
