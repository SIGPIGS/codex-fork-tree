//! Fork evidence → edges → trees (docs/06-algorithms.md §4–§6).

use crate::ledger::{Ledger, Verdict};
use crate::parse::{Session, Turn, burst_len};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// `forked_from_id` in session_meta
    Native,
    /// written to our ledger when the TUI ran `codex fork`
    Recorded,
    /// burst-copied history matching an earlier session
    Inferred,
    /// inferred, then confirmed by the user
    Confirmed,
}

impl EdgeKind {
    pub fn label(self) -> &'static str {
        match self {
            EdgeKind::Native => "native",
            EdgeKind::Recorded => "recorded",
            EdgeKind::Inferred => "inferred",
            EdgeKind::Confirmed => "confirmed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub parent: usize,
    /// Fork point: number of turns shared with the parent.
    pub k: usize,
    pub kind: EdgeKind,
}

/// Effective turn: (session index, turn index) into `Graph::sessions`.
pub type TurnRef = (usize, usize);

pub struct Graph {
    pub sessions: Vec<Session>,
    pub by_id: HashMap<String, usize>,
    pub eff: Vec<Vec<TurnRef>>,
    pub edge: Vec<Option<Edge>>,
    pub children: Vec<Vec<usize>>,
    /// Other sessions that also matched an inferred edge (shown in details).
    pub also_matches: Vec<Vec<usize>>,
}

impl Graph {
    pub fn turn(&self, r: TurnRef) -> &Turn {
        &self.sessions[r.0].turns[r.1]
    }

    pub fn eff_turn(&self, s: usize, k: usize) -> Option<&Turn> {
        self.eff[s].get(k).map(|&r| self.turn(r))
    }

    pub fn len(&self, s: usize) -> usize {
        self.eff[s].len()
    }

    pub fn fork_k(&self, s: usize) -> usize {
        self.edge[s].map_or(0, |e| e.k)
    }

    /// Own turns: turns after the fork point.
    pub fn own(&self, s: usize) -> usize {
        self.len(s).saturating_sub(self.fork_k(s))
    }

    pub fn parent(&self, s: usize) -> Option<usize> {
        self.edge[s].map(|e| e.parent)
    }

    pub fn root_of(&self, s: usize) -> usize {
        let mut cur = s;
        let mut seen = HashSet::new();
        while let Some(p) = self.parent(cur) {
            if !seen.insert(cur) {
                break;
            }
            cur = p;
        }
        cur
    }

    pub fn path_to_root(&self, s: usize) -> HashSet<usize> {
        let mut out = HashSet::new();
        let mut cur = Some(s);
        while let Some(c) = cur {
            if !out.insert(c) {
                break;
            }
            cur = self.parent(c);
        }
        out
    }

    pub fn tree_members(&self, root: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(s) = stack.pop() {
            if seen.insert(s) {
                out.push(s);
                stack.extend(self.children[s].iter().copied());
            }
        }
        out
    }

    pub fn build(sessions: Vec<Session>, ledger: &Ledger) -> Graph {
        let n = sessions.len();
        let by_id: HashMap<String, usize> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();

        // §4 effective history (memoized, cycle-guarded)
        let mut eff: Vec<Option<Vec<TurnRef>>> = vec![None; n];
        fn effective(
            i: usize,
            sessions: &[Session],
            by_id: &HashMap<String, usize>,
            eff: &mut Vec<Option<Vec<TurnRef>>>,
            visiting: &mut HashSet<usize>,
        ) -> Vec<TurnRef> {
            if let Some(e) = &eff[i] {
                return e.clone();
            }
            let own: Vec<TurnRef> = (0..sessions[i].turns.len()).map(|t| (i, t)).collect();
            let s = &sessions[i];
            let mut result = own.clone();
            if let (Some(ff), Some(cut)) = (&s.forked_from, s.fork_cut) {
                if let Some(&p) = by_id.get(ff) {
                    if visiting.insert(i) {
                        let parent = effective(p, sessions, by_id, eff, visiting);
                        visiting.remove(&i);
                        let mut inherited: Vec<TurnRef> = parent
                            .into_iter()
                            .filter(|&(si, ti)| {
                                sessions[si].turns[ti].ordinal.is_some_and(|o| o < cut)
                            })
                            .collect();
                        inherited.extend(own);
                        result = inherited;
                    }
                }
            }
            eff[i] = Some(result.clone());
            result
        }
        let mut visiting = HashSet::new();
        for i in 0..n {
            effective(i, &sessions, &by_id, &mut eff, &mut visiting);
        }
        let eff: Vec<Vec<TurnRef>> = eff.into_iter().map(Option::unwrap_or_default).collect();
        let fps: Vec<Vec<u64>> = eff
            .iter()
            .map(|e| e.iter().map(|&(s, t)| sessions[s].turns[t].fp).collect())
            .collect();

        // §3 prefix index: (k, P_k) -> sessions
        let mut prefix: Vec<Vec<u64>> = Vec::with_capacity(n);
        let mut index: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
        for (i, f) in fps.iter().enumerate() {
            let mut h = 0u64;
            let mut ps = Vec::with_capacity(f.len());
            for (k, fp) in f.iter().enumerate() {
                h = mix(h, *fp);
                ps.push(h);
                index.entry((k + 1, h)).or_default().push(i);
            }
            prefix.push(ps);
        }
        let lcp = |a: usize, b: usize| {
            fps[a]
                .iter()
                .zip(&fps[b])
                .take_while(|(x, y)| x == y)
                .count()
        };
        let bursts: Vec<usize> = sessions.iter().map(burst_len).collect();

        // §5 edges, precedence: verdict > native > recorded > inferred
        let mut edge: Vec<Option<Edge>> = vec![None; n];
        let mut also_matches: Vec<Vec<usize>> = vec![vec![]; n];
        for i in 0..n {
            let s = &sessions[i];
            if let Some(&p) = s.forked_from.as_ref().and_then(|ff| by_id.get(ff)) {
                edge[i] = Some(Edge {
                    parent: p,
                    k: lcp(p, i),
                    kind: EdgeKind::Native,
                });
                continue;
            }
            if let Some(&p) = ledger.recorded_parent(&s.id).and_then(|pid| by_id.get(pid)) {
                edge[i] = Some(Edge {
                    parent: p,
                    k: lcp(p, i),
                    kind: EdgeKind::Recorded,
                });
                continue;
            }
            let k = bursts[i];
            if k == 0 || prefix[i].len() < k {
                continue; // organic first turn: a root
            }
            let mut cands: Vec<usize> = index
                .get(&(k, prefix[i][k - 1]))
                .map(|v| {
                    v.iter()
                        .copied()
                        .filter(|&c| {
                            c != i && sessions[c].created < s.created && fps[c][..k] == fps[i][..k]
                        })
                        .collect()
                })
                .unwrap_or_default();
            cands.retain(|&c| ledger.verdict(&s.id, &sessions[c].id) != Some(Verdict::Reject));
            // the origin of turn k is the candidate that wrote it organically
            let origin: Vec<usize> = cands.iter().copied().filter(|&c| bursts[c] < k).collect();
            let pool = if origin.is_empty() { &cands } else { &origin };
            if let Some(&p) = pool.iter().min_by(|&&a, &&b| {
                sessions[a]
                    .created
                    .total_cmp(&sessions[b].created)
                    .then_with(|| sessions[a].id.cmp(&sessions[b].id))
            }) {
                let kind = if ledger.verdict(&s.id, &sessions[p].id) == Some(Verdict::Confirm) {
                    EdgeKind::Confirmed
                } else {
                    EdgeKind::Inferred
                };
                edge[i] = Some(Edge { parent: p, k, kind });
                also_matches[i] = cands.into_iter().filter(|&c| c != p).collect();
            }
        }
        // §5.5 nonempty rule, and break any cycles
        for i in 0..n {
            if edge[i].is_some_and(|e| e.k == 0 || e.parent == i) {
                edge[i] = None;
            }
        }
        for i in 0..n {
            let mut seen = HashSet::new();
            let mut cur = i;
            while let Some(e) = edge[cur] {
                if !seen.insert(cur) {
                    edge[i] = None;
                    break;
                }
                cur = e.parent;
            }
        }
        let mut children: Vec<Vec<usize>> = vec![vec![]; n];
        for i in 0..n {
            if let Some(e) = edge[i] {
                children[e.parent].push(i);
            }
        }
        for c in &mut children {
            c.sort_by(|&a, &b| {
                let (ea, eb) = (edge[a].unwrap().k, edge[b].unwrap().k);
                ea.cmp(&eb)
                    .then(sessions[a].created.total_cmp(&sessions[b].created))
            });
        }
        Graph {
            sessions,
            by_id,
            eff,
            edge,
            children,
            also_matches,
        }
    }
}

fn mix(h: u64, fp: u64) -> u64 {
    let mut s = sha1_smol::Sha1::new();
    s.update(&h.to_be_bytes());
    s.update(&fp.to_be_bytes());
    u64::from_be_bytes(s.digest().bytes()[..8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Format, fingerprint};
    use std::path::PathBuf;

    fn turn(text: &str, reply: &str, ts: f64, ordinal: Option<i64>) -> Turn {
        Turn {
            fp: fingerprint(text, &[reply.into()]),
            user: text.into(),
            agent: reply.into(),
            ts,
            ordinal,
        }
    }

    fn session(id: &str, created: f64, turns: Vec<Turn>) -> Session {
        Session {
            id: id.into(),
            path: PathBuf::from(format!("/x/{id}.jsonl")),
            cwd: "/p".into(),
            created,
            source: "cli".into(),
            cli_version: "test".into(),
            forked_from: None,
            fork_cut: None,
            format: Format::F1,
            turns,
            archived: false,
            mtime: created,
        }
    }

    #[test]
    fn ordinal_reference_fork() {
        let a = session(
            "A",
            0.0,
            vec![
                turn("a1", "r1", 10.0, Some(1)),
                turn("a2", "r2", 20.0, Some(5)),
                turn("a3", "r3", 30.0, Some(9)),
            ],
        );
        let mut b = session("B", 100.0, vec![turn("b1", "x", 110.0, Some(7))]);
        b.forked_from = Some("A".into());
        b.fork_cut = Some(6);
        let g = Graph::build(vec![a, b], &Ledger::default());
        let e = g.edge[1].unwrap();
        assert_eq!((e.parent, e.k, e.kind), (0, 2, EdgeKind::Native));
        assert_eq!(g.len(1), 3);
        assert_eq!(g.own(1), 1);
    }

    #[test]
    fn burst_copy_is_inferred_but_coincidence_is_not() {
        let a = session(
            "A",
            0.0,
            vec![
                turn("hi", "hello", 10.0, None),
                turn("more", "ok", 20.0, None),
            ],
        );
        // copied A's first two turns within 0.1 s of creation, then went on
        let b = session(
            "B",
            100.0,
            vec![
                turn("hi", "hello", 100.05, None),
                turn("more", "ok", 100.06, None),
                turn("own", "z", 130.0, None),
            ],
        );
        // typed the same first prompt and got the same reply, but organically
        let c = session("C", 200.0, vec![turn("hi", "hello", 213.0, None)]);
        let g = Graph::build(vec![a, b, c], &Ledger::default());
        let e = g.edge[1].unwrap();
        assert_eq!((e.parent, e.k, e.kind), (0, 2, EdgeKind::Inferred));
        assert!(g.edge[2].is_none());
    }

    #[test]
    fn zero_shared_turns_drops_the_edge() {
        let a = session("A", 0.0, vec![turn("a", "r", 10.0, None)]);
        let mut b = session("B", 50.0, vec![turn("b", "r", 60.0, None)]);
        b.forked_from = Some("A".into());
        let g = Graph::build(vec![a, b], &Ledger::default());
        assert!(g.edge[1].is_none());
    }

    #[test]
    fn rejected_inference_falls_back_to_root() {
        let a = session("A", 0.0, vec![turn("hi", "hello", 10.0, None)]);
        let b = session("B", 100.0, vec![turn("hi", "hello", 100.05, None)]);
        let mut ledger = Ledger::default();
        ledger.set_verdict("B", "A", Verdict::Reject);
        let g = Graph::build(vec![a, b], &ledger);
        assert!(g.edge[1].is_none());
    }
}
