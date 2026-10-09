//! Application state, key handling, Codex handoff, and the non-interactive commands.

use crate::cache::Cache;
use crate::codex::{project_root, resolve_focus, thread_names};
use crate::graph::{EdgeKind, Graph};
use crate::layout::{Key, Line, Role, Row, RowKind, View, short};
use crate::ledger::{Ledger, Verdict};
use crate::theme::{Theme, ThemeName};
use crate::transcript::{self, Transcript};
use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const REFRESH_EVERY: Duration = Duration::from_secs(2);
const FRESH_FOR: Duration = Duration::from_secs(2);
const TOAST_FOR: Duration = Duration::from_millis(2800);

pub enum Overlay {
    None,
    Help,
    Details,
    Transcript {
        id: String,
        t: Transcript,
        width: usize,
        scroll: usize,
    },
    ConfirmArchive {
        id: String,
        unarchive: bool,
    },
    ConfirmDelete {
        id: String,
        typed: String,
    },
}

pub enum Exec {
    Resume(String),
    Fork(String),
}

pub struct App {
    pub home: PathBuf,
    pub root: PathBuf,
    cache: Cache,
    ledger: Ledger,
    pub names: HashMap<String, String>,
    pub graph: Graph,
    pub focus_id: String,
    pub sel: Option<Key>,
    pub scroll: usize,
    pub collapsed: HashSet<String>,
    fresh_until: HashMap<String, Instant>,
    pub fresh: HashSet<String>,
    pub show_archived: bool,
    pub show_inferred: bool,
    pub show_empty: bool,
    pub overlay: Overlay,
    pub toast: Option<(String, Role, Instant)>,
    pub theme: Theme,
    pub split: f32,
    pub refreshed_at: Instant,
    pub rows: Vec<Row>,
    pub tree_width: usize,
    pub screen_width: usize,
    pub body_height: usize,
    pub narrow: bool,
    pub exec: Option<Exec>,
    pub quit: bool,
    last_g: Option<Instant>,
}

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn load(
    home: &Path,
    root: &Path,
    rebuild: bool,
    verbose: bool,
) -> (Cache, Ledger, HashMap<String, String>, Graph) {
    let mut cache = Cache::open(root, rebuild);
    let cold = cache.is_cold();
    let stats = cache.refresh(home, |done, total| {
        if verbose || cold {
            eprint!("\rindexing {} · {done}/{total} rollouts ", root.display());
        }
    });
    if (verbose || cold) && stats.parsed > 0 {
        eprintln!(
            "\rindexed {} · {} sessions parsed            ",
            root.display(),
            stats.parsed
        );
    }
    cache.save();
    let ledger = Ledger::load(Ledger::default_path());
    let names = thread_names(home);
    let graph = Graph::build(cache.sessions(), &ledger);
    (cache, ledger, names, graph)
}

impl App {
    pub fn focus(&self) -> usize {
        self.graph.by_id.get(&self.focus_id).copied().unwrap_or(0)
    }

    pub fn view(&self) -> View<'_> {
        View::new(
            &self.graph,
            &self.names,
            self.focus(),
            &self.collapsed,
            &self.fresh,
            (self.show_archived, self.show_inferred, self.show_empty),
            now(),
        )
    }

    fn reset_collapse(&mut self) {
        let keep = self.graph.path_to_root(self.focus());
        self.collapsed = (0..self.graph.sessions.len())
            .filter(|s| !keep.contains(s) && !self.graph.children[*s].is_empty())
            .map(|s| self.graph.sessions[s].id.clone())
            .collect();
    }

    pub fn toast(&mut self, msg: impl Into<String>, role: Role) {
        self.toast = Some((msg.into(), role, Instant::now() + TOAST_FOR));
    }

    pub fn project_count(&self) -> usize {
        self.graph.sessions.len()
    }

    /// Re-stat rollouts; rebuild the graph when anything changed. Keeps selection by key.
    pub fn refresh(&mut self) -> bool {
        let before: HashSet<String> = self.graph.sessions.iter().map(|s| s.id.clone()).collect();
        let stats = self.cache.refresh(&self.home, |_, _| {});
        self.refreshed_at = Instant::now();
        if !stats.changed {
            return false;
        }
        self.cache.save();
        self.names = thread_names(&self.home);
        self.graph = Graph::build(self.cache.sessions(), &self.ledger);
        let focus_tree: HashSet<usize> = self
            .graph
            .tree_members(self.graph.root_of(self.focus()))
            .into_iter()
            .collect();
        for s in &self.graph.sessions {
            if !before.contains(&s.id)
                && self
                    .graph
                    .by_id
                    .get(&s.id)
                    .is_some_and(|i| focus_tree.contains(i))
            {
                self.fresh_until
                    .insert(s.id.clone(), Instant::now() + FRESH_FOR);
            }
        }
        true
    }

    pub fn tick(&mut self) {
        let now_i = Instant::now();
        if now_i.duration_since(self.refreshed_at) >= REFRESH_EVERY {
            self.refresh();
        }
        self.fresh_until.retain(|_, until| *until > now_i);
        self.fresh = self.fresh_until.keys().cloned().collect();
        if self.toast.as_ref().is_some_and(|t| t.2 <= now_i) {
            self.toast = None;
        }
    }

    // ---- selection helpers ---------------------------------------------------------------------

    fn selectable(&self) -> Vec<&Row> {
        self.rows.iter().filter(|r| r.key.is_some()).collect()
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows
            .iter()
            .find(|r| r.key.is_some() && r.key == self.sel)
    }

    fn selected_session(&self) -> Option<usize> {
        let r = self.selected_row()?;
        r.session.or(r.fork.map(|f| f.0))
    }

    /// Make sure the selection points at an existing row (after rebuilds, folds, filters).
    pub fn fix_selection(&mut self) {
        if self.selected_row().is_some() {
            return;
        }
        let focus_key = Key::Session(self.focus_id.clone());
        self.sel = if self.rows.iter().any(|r| r.key.as_ref() == Some(&focus_key)) {
            Some(focus_key)
        } else {
            self.rows.iter().find_map(|r| r.key.clone())
        };
    }

    fn move_sel(&mut self, d: isize) {
        let keys: Vec<Key> = self
            .selectable()
            .iter()
            .filter_map(|r| r.key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        let i = keys
            .iter()
            .position(|k| Some(k) == self.sel.as_ref())
            .unwrap_or(0) as isize;
        let j = (i + d).clamp(0, keys.len() as isize - 1) as usize;
        self.sel = Some(keys[j].clone());
    }

    // ---- key handling --------------------------------------------------------------------------

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match &mut self.overlay {
            Overlay::None => {}
            Overlay::Help => {
                self.overlay = Overlay::None;
                return;
            }
            Overlay::Transcript { t, scroll, .. } => {
                let page = self.body_height.saturating_sub(2).max(1);
                let max = t.lines.len().saturating_sub(self.body_height.max(1));
                match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => self.overlay = Overlay::None,
                    KeyCode::Char('j') | KeyCode::Down => *scroll = (*scroll + 1).min(max),
                    KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
                    KeyCode::Char('d') if ctrl => *scroll = (*scroll + page / 2).min(max),
                    KeyCode::Char('u') if ctrl => *scroll = scroll.saturating_sub(page / 2),
                    KeyCode::PageDown | KeyCode::Char(' ') => *scroll = (*scroll + page).min(max),
                    KeyCode::PageUp => *scroll = scroll.saturating_sub(page),
                    KeyCode::Char('G') | KeyCode::End => *scroll = max,
                    KeyCode::Char('g') | KeyCode::Home => *scroll = 0,
                    KeyCode::Char('}') => {
                        if let Some(&n) = t.turn_lines.iter().find(|&&l| l > *scroll) {
                            *scroll = n.min(max);
                        }
                    }
                    KeyCode::Char('{') => {
                        if let Some(&n) = t.turn_lines.iter().rev().find(|&&l| l < *scroll) {
                            *scroll = n;
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('n') => {
                        self.overlay = Overlay::None;
                        self.on_key(k);
                    }
                    _ => {}
                }
                return;
            }
            Overlay::Details => {
                match k.code {
                    KeyCode::Enter | KeyCode::Char('t') => self.open_transcript(),
                    KeyCode::Char('r') | KeyCode::Char('n') => {
                        self.overlay = Overlay::None;
                        self.on_key(k);
                    }
                    _ => self.overlay = Overlay::None,
                }
                return;
            }
            Overlay::ConfirmArchive { id, unarchive } => {
                let (id, unarchive) = (id.clone(), *unarchive);
                self.overlay = Overlay::None;
                if matches!(
                    k.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.codex_cmd(if unarchive { "unarchive" } else { "archive" }, &id);
                }
                return;
            }
            Overlay::ConfirmDelete { id, typed } => {
                match k.code {
                    KeyCode::Esc => self.overlay = Overlay::None,
                    KeyCode::Backspace => {
                        typed.pop();
                    }
                    KeyCode::Enter => {
                        let (id, ok) = (
                            id.clone(),
                            id.starts_with(typed.as_str()) && typed.len() >= 4,
                        );
                        self.overlay = Overlay::None;
                        if ok {
                            self.codex_cmd("delete", &id);
                        } else {
                            self.toast("delete cancelled: the typed id did not match", Role::Warn);
                        }
                    }
                    KeyCode::Char(c) if typed.len() < 8 => typed.push(c),
                    _ => {}
                }
                return;
            }
        }

        let row = self.selected_row().cloned();
        let sess = self.selected_session();
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('r') if ctrl => {
                let changed = self.refresh();
                self.toast(
                    if changed {
                        "re-indexed: tree updated"
                    } else {
                        "re-indexed: no changes"
                    },
                    Role::Dim,
                );
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sel(-1),
            KeyCode::Char('d') if ctrl => self.move_sel((self.body_height / 2) as isize),
            KeyCode::Char('u') if ctrl => self.move_sel(-((self.body_height / 2) as isize)),
            KeyCode::PageDown => self.move_sel(self.body_height as isize),
            KeyCode::PageUp => self.move_sel(-(self.body_height as isize)),
            KeyCode::Char('G') | KeyCode::End => self.move_sel(isize::MAX / 2),
            KeyCode::Home => self.move_sel(isize::MIN / 2),
            KeyCode::Char('g') => {
                if self
                    .last_g
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
                {
                    self.move_sel(isize::MIN / 2);
                    self.last_g = None;
                } else {
                    self.last_g = Some(Instant::now());
                }
            }
            KeyCode::Char('F') => self.sel = Some(Key::Session(self.focus_id.clone())),
            KeyCode::Char('p') => {
                if let Some(p) = row.and_then(|r| r.parent) {
                    self.sel = Some(p);
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(r) = row {
                    let id = r.session.map(|s| self.graph.sessions[s].id.clone());
                    match (r.kind, id) {
                        (RowKind::FrameTop, Some(id)) => {
                            self.collapsed.insert(id);
                        }
                        _ => {
                            if let Some(p) = r.parent {
                                self.sel = Some(p);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(r) = row {
                    let id = r.session.map(|s| self.graph.sessions[s].id.clone());
                    if let Some(id) = id.filter(|id| self.collapsed.contains(id)) {
                        self.collapsed.remove(&id);
                    } else if let Some(c) = self
                        .rows
                        .iter()
                        .find(|x| x.parent.is_some() && x.parent == r.key)
                    {
                        self.sel = c.key.clone();
                    }
                }
            }
            KeyCode::Char('[') | KeyCode::Char(']') => {
                if let Some(r) = row {
                    let sibs: Vec<Key> = self
                        .rows
                        .iter()
                        .filter(|x| x.key.is_some() && x.parent == r.parent)
                        .filter_map(|x| x.key.clone())
                        .collect();
                    if let Some(i) = sibs.iter().position(|x| Some(x) == r.key.as_ref()) {
                        let j = if k.code == KeyCode::Char(']') {
                            i + 1
                        } else {
                            i.wrapping_sub(1)
                        };
                        if let Some(n) = sibs.get(j) {
                            self.sel = Some(n.clone());
                        }
                    }
                }
            }
            KeyCode::Char('z') => self.reset_collapse(),
            KeyCode::Char('Z') => self.collapsed.clear(),
            KeyCode::Enter => {
                if self.narrow {
                    self.overlay = Overlay::Details;
                } else {
                    self.open_transcript();
                }
            }
            KeyCode::Char('t') => self.open_transcript(),
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('a') => self.show_archived = !self.show_archived,
            KeyCode::Char('i') => self.show_inferred = !self.show_inferred,
            KeyCode::Char('e') => self.show_empty = !self.show_empty,
            KeyCode::Char('<') => self.split = (self.split - 0.05).max(0.35),
            KeyCode::Char('>') => self.split = (self.split + 0.05).min(0.8),
            KeyCode::Char('f') => {
                if let Some(s) = sess {
                    self.focus_id = self.graph.sessions[s].id.clone();
                    self.sel = Some(Key::Session(self.focus_id.clone()));
                    self.reset_collapse();
                    self.toast(format!("focus → {}", short(&self.focus_id)), Role::Accent);
                }
            }
            KeyCode::Char('r') => {
                if let Some(s) = sess {
                    let id = self.graph.sessions[s].id.clone();
                    if self.view().is_live(s)
                        && !matches!(self.toast, Some((ref m, _, _)) if m.starts_with("live"))
                    {
                        self.toast("live: Codex is writing to this session; press r again to resume anyway", Role::Warn);
                    } else {
                        self.exec = Some(Exec::Resume(id));
                    }
                }
            }
            KeyCode::Char('n') => {
                if let Some(s) = sess {
                    self.exec = Some(Exec::Fork(self.graph.sessions[s].id.clone()));
                }
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(s) = sess {
                    let text = if k.code == KeyCode::Char('y') {
                        self.graph.sessions[s].id.clone()
                    } else {
                        self.graph.sessions[s].path.display().to_string()
                    };
                    osc52(&text);
                    self.toast(format!("copied {text}"), Role::Accent);
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                let edge = sess.and_then(|s| self.graph.edge[s].map(|e| (s, e)));
                match edge {
                    Some((s, e)) if e.kind == EdgeKind::Inferred => {
                        let (child, parent) = (
                            self.graph.sessions[s].id.clone(),
                            self.graph.sessions[e.parent].id.clone(),
                        );
                        let confirm = k.code == KeyCode::Char('x');
                        self.ledger.set_verdict(
                            &child,
                            &parent,
                            if confirm {
                                Verdict::Confirm
                            } else {
                                Verdict::Reject
                            },
                        );
                        let _ = self.ledger.save();
                        self.graph = Graph::build(self.cache.sessions(), &self.ledger);
                        self.toast(
                            if confirm {
                                format!("confirmed {} as a fork", short(&child))
                            } else {
                                format!("rejected {}; Ctrl-r keeps it out", short(&child))
                            },
                            Role::Accent,
                        );
                    }
                    _ => self.toast("x / X apply to inferred forks (≈)", Role::Dim),
                }
            }
            KeyCode::Char('A') => {
                if let Some(s) = sess {
                    let sess = &self.graph.sessions[s];
                    self.overlay = Overlay::ConfirmArchive {
                        id: sess.id.clone(),
                        unarchive: sess.archived,
                    };
                }
            }
            KeyCode::Char('D') => {
                if let Some(s) = sess {
                    self.overlay = Overlay::ConfirmDelete {
                        id: self.graph.sessions[s].id.clone(),
                        typed: String::new(),
                    };
                }
            }
            _ => {}
        }
    }

    /// Re-wrap an open transcript after a resize, keeping roughly the same position.
    pub fn rewrap_transcript(&mut self, w: usize) {
        let Overlay::Transcript {
            id,
            width,
            scroll,
            t,
        } = &self.overlay
        else {
            return;
        };
        if *width == w {
            return;
        }
        let (id, frac) = (id.clone(), *scroll as f64 / t.lines.len().max(1) as f64);
        let Some(&s) = self.graph.by_id.get(&id) else {
            return;
        };
        let view = self.view();
        let title = view.base_title(s);
        let parent_title = self.graph.parent(s).map(|p| view.base_title(p));
        let t = transcript::build(&self.graph, s, &title, parent_title.as_deref(), w);
        let scroll = (frac * t.lines.len() as f64) as usize;
        self.overlay = Overlay::Transcript {
            id,
            t,
            width: w,
            scroll,
        };
    }

    fn open_transcript(&mut self) {
        let Some(s) = self.selected_session() else {
            return;
        };
        let width = self.screen_width.max(40);
        let view = self.view();
        let title = view.base_title(s);
        let parent_title = self.graph.parent(s).map(|p| view.base_title(p));
        let t = transcript::build(&self.graph, s, &title, parent_title.as_deref(), width);
        let scroll = t.fork_line.saturating_sub(2);
        self.overlay = Overlay::Transcript {
            id: self.graph.sessions[s].id.clone(),
            t,
            width,
            scroll,
        };
    }

    fn codex_cmd(&mut self, verb: &str, id: &str) {
        match Command::new(codex_bin()).args([verb, id]).output() {
            Ok(o) if o.status.success() => {
                self.refresh();
                self.toast(format!("{verb}d {}", short(id)), Role::Accent);
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                let msg = err
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty() && !l.contains("WARNING"))
                    .unwrap_or("failed");
                self.toast(format!("codex {verb} failed: {msg}"), Role::Danger);
            }
            Err(e) => self.toast(format!("could not run codex: {e}"), Role::Danger),
        }
    }

    /// Called after `codex resume` / `codex fork` returns.
    pub fn after_exec(
        &mut self,
        exec: Exec,
        before: HashSet<String>,
        status: std::io::Result<std::process::ExitStatus>,
    ) {
        if let Err(e) = status {
            self.toast(format!("could not run codex: {e}"), Role::Danger);
            return;
        }
        self.refresh();
        match exec {
            Exec::Resume(id) => self.sel = Some(Key::Session(id)),
            Exec::Fork(parent) => {
                let new: Vec<&crate::parse::Session> = self
                    .graph
                    .sessions
                    .iter()
                    .filter(|s| !before.contains(&s.id))
                    .collect();
                let child = new
                    .iter()
                    .find(|s| s.forked_from.as_deref() == Some(parent.as_str()))
                    .or_else(|| new.iter().max_by(|a, b| a.created.total_cmp(&b.created)))
                    .map(|s| (s.id.clone(), s.forked_from.clone()));
                match child {
                    Some((cid, ff)) => {
                        if ff.as_deref() != Some(parent.as_str()) {
                            self.ledger.record_fork(&cid, &parent);
                            let _ = self.ledger.save();
                            self.graph = Graph::build(self.cache.sessions(), &self.ledger);
                        }
                        self.focus_id = cid.clone();
                        self.sel = Some(Key::Session(cid.clone()));
                        self.reset_collapse();
                        self.toast(
                            format!("new fork {} · now in focus", short(&cid)),
                            Role::Accent,
                        );
                    }
                    None => self.toast("no new session was created; focus unchanged", Role::Dim),
                }
            }
        }
    }
}

pub fn codex_bin() -> String {
    std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".into())
}

fn osc52(text: &str) {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let b = text.as_bytes();
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    let mut so = std::io::stdout();
    let _ = write!(so, "\x1b]52;c;{out}\x07");
    let _ = so.flush();
}

// ---- entry points ------------------------------------------------------------------------------

pub fn run(
    home: &Path,
    session: Option<&str>,
    project: Option<&Path>,
    theme: ThemeName,
) -> Result<()> {
    let resolved = resolve_focus(home, session, project)?;
    let root = project_root(&resolved.meta.cwd);
    let (cache, ledger, names, graph) = load(home, &root, false, false);
    if !graph.by_id.contains_key(&resolved.meta.id) {
        anyhow::bail!(
            "session {} has no conversation turns yet",
            short(&resolved.meta.id)
        );
    }
    let mut app = App {
        home: home.to_path_buf(),
        root,
        cache,
        ledger,
        names,
        graph,
        focus_id: resolved.meta.id.clone(),
        sel: Some(Key::Session(resolved.meta.id.clone())),
        scroll: 0,
        collapsed: HashSet::new(),
        fresh_until: HashMap::new(),
        fresh: HashSet::new(),
        show_archived: false,
        show_inferred: true,
        show_empty: false,
        overlay: Overlay::None,
        toast: None,
        theme: Theme::resolve(theme),
        split: 0.6,
        refreshed_at: Instant::now(),
        rows: vec![],
        tree_width: 80,
        screen_width: 80,
        body_height: 20,
        narrow: false,
        exec: None,
        quit: false,
        last_g: None,
    };
    app.reset_collapse();
    if let Some(n) = resolved.notice {
        app.toast(n, Role::Warn);
    }

    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| crate::ui::draw(f, &mut app))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => app.on_key(k),
                    Event::Resize(..) => {}
                    _ => {}
                }
            }
            app.tick();
            if app.quit {
                return Ok(());
            }
            if let Some(exec) = app.exec.take() {
                let before: HashSet<String> =
                    app.graph.sessions.iter().map(|s| s.id.clone()).collect();
                let args: Vec<&str> = match &exec {
                    Exec::Resume(id) => vec!["resume", id],
                    Exec::Fork(id) => vec!["fork", id],
                };
                ratatui::restore();
                let status = Command::new(codex_bin())
                    .args(&args)
                    .current_dir(&app.root)
                    .status();
                terminal = ratatui::init();
                terminal.clear()?;
                app.after_exec(exec, before, status);
            }
        }
    })();
    ratatui::restore();
    result
}

pub fn print_tree(home: &Path, session: Option<&str>, width: usize, all: bool) -> Result<()> {
    let resolved = resolve_focus(home, session, None)?;
    if let Some(n) = &resolved.notice {
        eprintln!("{n}");
    }
    let root = project_root(&resolved.meta.cwd);
    let (_c, _l, names, graph) = load(home, &root, false, false);
    let focus = *graph
        .by_id
        .get(&resolved.meta.id)
        .context("the focus session has no conversation turns yet")?;
    let empty = HashSet::new();
    let view = View::new(
        &graph,
        &names,
        focus,
        &empty,
        &empty,
        (all, true, all),
        now(),
    );
    for r in view.rows(width) {
        println!("{}", plain(&r.line).trim_end());
    }
    Ok(())
}

pub fn plain(l: &Line) -> String {
    l.iter().map(|s| s.text.as_str()).collect()
}

pub fn index(home: &Path, project: Option<PathBuf>, rebuild: bool) -> Result<()> {
    let dir = project.unwrap_or(std::env::current_dir()?);
    let root = project_root(&dir.to_string_lossy());
    let (_c, _l, _n, graph) = load(home, &root, rebuild, true);
    let trees: Vec<usize> = (0..graph.sessions.len())
        .filter(|&s| graph.parent(s).is_none() && !graph.children[s].is_empty())
        .collect();
    println!(
        "{} sessions in {} · {} trees with forks",
        graph.sessions.len(),
        root.display(),
        trees.len()
    );
    Ok(())
}

pub fn debug_edges(home: &Path, project: Option<PathBuf>) -> Result<()> {
    let dir = project.unwrap_or(std::env::current_dir()?);
    let root = if dir == Path::new("/") {
        dir.clone()
    } else {
        project_root(&dir.to_string_lossy())
    };
    let (_c, _l, _n, graph) = load(home, &root, false, false);
    let mut lines: Vec<String> = (0..graph.sessions.len())
        .filter_map(|s| {
            graph.edge[s].map(|e| {
                format!(
                    "{} {} {} {}",
                    graph.sessions[s].id,
                    graph.sessions[e.parent].id,
                    e.k,
                    e.kind.label()
                )
            })
        })
        .collect();
    lines.sort();
    for l in lines {
        println!("{l}");
    }
    Ok(())
}
