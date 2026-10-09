//! Where Codex keeps things, project roots, thread names, and focus resolution.

use crate::parse::{Meta, read_meta};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex"))
}

/// Every rollout file: `sessions/**` and `archived_sessions/**`.
pub fn rollout_files(home: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for dir in ["sessions", "archived_sessions"] {
        walk(&home.join(dir), &mut out);
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&p, out),
            Ok(t) if t.is_file() && p.extension().is_some_and(|x| x == "jsonl") => out.push(p),
            _ => {}
        }
    }
}

/// `rollout-<timestamp>-<uuid>.jsonl` → `<uuid>`
pub fn id_from_filename(p: &Path) -> Option<&str> {
    let stem = p.file_stem()?.to_str()?;
    (stem.len() >= 36).then(|| &stem[stem.len() - 36..])
}

/// `thread_name` per session id from `session_index.jsonl` (last line wins).
pub fn thread_names(home: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(home.join("session_index.jsonl")) {
        for line in text.lines() {
            if let Ok(o) = serde_json::from_str::<Value>(line) {
                if let (Some(id), Some(name)) = (
                    o.get("id").and_then(Value::as_str),
                    o.get("thread_name").and_then(Value::as_str),
                ) {
                    if !name.trim().is_empty() {
                        names.insert(id.to_string(), name.to_string());
                    }
                }
            }
        }
    }
    names
}

/// Git root of `cwd`, or `cwd` itself.
pub fn project_root(cwd: &str) -> PathBuf {
    let start = PathBuf::from(cwd);
    let mut cur = Some(start.as_path());
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return d.to_path_buf();
        }
        cur = d.parent();
    }
    start
}

pub fn in_project(cwd: &str, root: &Path) -> bool {
    let c = Path::new(cwd);
    c == root || c.starts_with(root)
}

pub struct Resolved {
    pub meta: Meta,
    pub notice: Option<String>,
}

/// Resolve the focus session from a uuid, uuid prefix, thread name or rollout path (docs/02 §Launch).
pub fn resolve_focus(home: &Path, arg: Option<&str>, last_in: Option<&Path>) -> Result<Resolved> {
    let files = rollout_files(home);
    let by_id = |id: &str| {
        files
            .iter()
            .find(|p| id_from_filename(p) == Some(id))
            .cloned()
    };
    let path = match arg {
        Some(a) if Path::new(a).is_file() => PathBuf::from(a),
        Some(a) => {
            let hits: Vec<&PathBuf> = files
                .iter()
                .filter(|p| id_from_filename(p).is_some_and(|id| id.starts_with(a)))
                .collect();
            match hits.len() {
                1 => hits[0].clone(),
                0 => {
                    let names = thread_names(home);
                    let named: Vec<&String> = names
                        .iter()
                        .filter(|(_, n)| n.as_str() == a)
                        .map(|(id, _)| id)
                        .collect();
                    match named.as_slice() {
                        [id] => by_id(id)
                            .with_context(|| format!("session {id} has no rollout file"))?,
                        [] => bail!("no session matches {a:?}"),
                        _ => bail!(
                            "{} sessions are named {a:?}; pass an id prefix",
                            named.len()
                        ),
                    }
                }
                n => bail!("{n} sessions match {a:?}; pass a longer id prefix"),
            }
        }
        None => {
            let dir = last_in
                .map(Path::to_path_buf)
                .unwrap_or(std::env::current_dir()?);
            let root = project_root(&dir.to_string_lossy());
            newest_in_project(&files, &root)
                .with_context(|| format!("no Codex sessions found for {}", root.display()))?
        }
    };
    let mut meta =
        read_meta(&path).with_context(|| format!("{} is not a Codex rollout", path.display()))?;
    let mut notice = None;
    // subagents are out of scope: show the nearest user session instead
    let mut hops = 0;
    while meta.subagent {
        let short = meta.id[..8.min(meta.id.len())].to_string();
        let parent = meta.forked_from.clone().and_then(|p| by_id(&p));
        match parent.and_then(|p| read_meta(&p)) {
            Some(m) if hops < 16 => {
                notice = Some(format!(
                    "{short}… is a subagent; showing its parent session"
                ));
                meta = m;
                hops += 1;
            }
            _ => bail!("{short}… is a subagent session, and its parent was not found"),
        }
    }
    Ok(Resolved { meta, notice })
}

fn newest_in_project(files: &[PathBuf], root: &Path) -> Option<PathBuf> {
    // file names sort by creation time; walk newest first and stop at the first match
    let mut sorted: Vec<&PathBuf> = files.iter().collect();
    sorted.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    sorted
        .into_iter()
        .find(|p| read_meta(p).is_some_and(|m| !m.subagent && in_project(&m.cwd, root)))
        .cloned()
}
