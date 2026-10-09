//! Project-wide cache of parsed rollouts (docs/06-algorithms.md §8).
//!
//! One JSON file per project root under `$XDG_CACHE_HOME/codex-fork-tree/`. Each rollout is keyed by
//! path and re-parsed only when its size or mtime changes. Rollouts outside the project and subagent
//! rollouts are remembered as such, so line 1 is read once per file version.

use crate::codex::{in_project, rollout_files};
use crate::parse::{Session, parse, read_meta};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Bump when parsing rules change; old caches are then rebuilt.
const NORMALIZER_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
enum Kind {
    Session(Box<Session>),
    Subagent,
    Elsewhere,
    Invalid,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    size: u64,
    mtime_ns: u128,
    kind: Kind,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    version: u32,
    root: PathBuf,
    entries: HashMap<PathBuf, Entry>,
}

pub struct Cache {
    path: PathBuf,
    store: Store,
    dirty: bool,
}

pub struct RefreshStats {
    pub changed: bool,
    pub parsed: usize,
}

impl Cache {
    pub fn path_for(root: &Path) -> PathBuf {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::codex::home_dir().join(".cache"));
        let mut h = sha1_smol::Sha1::new();
        h.update(root.to_string_lossy().as_bytes());
        base.join("codex-fork-tree")
            .join(format!("{}.json", &h.digest().to_string()[..16]))
    }

    pub fn open(root: &Path, rebuild: bool) -> Cache {
        let path = Cache::path_for(root);
        let store = (!rebuild)
            .then(|| std::fs::read(&path).ok())
            .flatten()
            .and_then(|b| serde_json::from_slice::<Store>(&b).ok())
            .filter(|s| s.version == NORMALIZER_VERSION && s.root == root)
            .unwrap_or(Store {
                version: NORMALIZER_VERSION,
                root: root.to_path_buf(),
                entries: HashMap::new(),
            });
        Cache {
            path,
            store,
            dirty: rebuild,
        }
    }

    pub fn is_cold(&self) -> bool {
        self.store.entries.is_empty()
    }

    /// Stat every rollout; re-read only what changed. `progress(done, total)` is called while parsing.
    pub fn refresh(&mut self, home: &Path, mut progress: impl FnMut(usize, usize)) -> RefreshStats {
        let files = rollout_files(home);
        let mut seen = std::collections::HashSet::new();
        let mut parsed = 0;
        let mut changed = false;
        let total = files.len();
        for (i, f) in files.iter().enumerate() {
            seen.insert(f.clone());
            let Ok(md) = std::fs::metadata(f) else {
                continue;
            };
            let size = md.len();
            let mtime_ns = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            if let Some(e) = self.store.entries.get(f) {
                if e.size == size && e.mtime_ns == mtime_ns {
                    continue;
                }
            }
            let kind = match read_meta(f) {
                None => Kind::Invalid,
                Some(m) if m.subagent => Kind::Subagent,
                Some(m) if !in_project(&m.cwd, &self.store.root) => Kind::Elsewhere,
                Some(_) => {
                    parsed += 1;
                    if parsed % 8 == 0 {
                        progress(i, total);
                    }
                    parse(f, false).map_or(Kind::Invalid, |s| Kind::Session(Box::new(s)))
                }
            };
            if matches!(kind, Kind::Session(_))
                || self
                    .store
                    .entries
                    .get(f)
                    .is_some_and(|e| matches!(e.kind, Kind::Session(_)))
            {
                changed = true;
            }
            self.store.entries.insert(
                f.clone(),
                Entry {
                    size,
                    mtime_ns,
                    kind,
                },
            );
            self.dirty = true;
        }
        let before = self.store.entries.len();
        self.store.entries.retain(|p, _| seen.contains(p));
        if self.store.entries.len() != before {
            changed = true;
            self.dirty = true;
        }
        RefreshStats { changed, parsed }
    }

    pub fn sessions(&self) -> Vec<Session> {
        let mut v: Vec<Session> = self
            .store
            .entries
            .values()
            .filter_map(|e| match &e.kind {
                Kind::Session(s) => Some(s.as_ref().clone()),
                _ => None,
            })
            .collect();
        v.sort_by(|a, b| {
            a.created
                .total_cmp(&b.created)
                .then_with(|| a.id.cmp(&b.id))
        });
        v
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec(&self.store) {
            let tmp = self.path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &self.path).is_ok() {
                self.dirty = false;
            }
        }
    }
}
