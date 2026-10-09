//! Rollout parsing: `session_meta` plus the user-visible turn log (docs/06-algorithms.md §2).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Copied history is written within this window after the session is created (§5.2).
pub const BURST_WINDOW_S: f64 = 2.0;
/// Characters of prompt/reply text kept in the cache; the transcript re-reads full text.
const PREVIEW_CHARS: usize = 240;
const INJECTED_PREFIXES: &[&str] = &[
    "# AGENTS.md",
    "<environment_context",
    "<user_instructions",
    "<permissions",
    "<collaboration_mode",
    "<turn_aborted",
];
const PREFILTER: &[&[u8]] = &[
    b"\"session_meta\"",
    b"\"user_message\"",
    b"\"agent_message\"",
    b"\"thread_rolled_back\"",
    b"\"UserMessage\"",
    b"\"AgentMessage\"",
    b"\"message\"",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    /// `item_completed` UserMessage / AgentMessage (Codex 0.147+)
    F2,
    /// `event_msg` user_message / agent_message (0.104–0.147)
    F1,
    /// raw `response_item` messages with injected context removed
    F3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// Fingerprint of prompt + replies; equal turns mean shared history.
    pub fp: u64,
    pub user: String,
    pub agent: String,
    pub ts: f64,
    pub ordinal: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub path: PathBuf,
    pub cwd: String,
    pub created: f64,
    pub source: String,
    pub cli_version: String,
    pub forked_from: Option<String>,
    /// Ordinal cut for forks that inherit history by reference (§4).
    pub fork_cut: Option<i64>,
    pub format: Format,
    pub turns: Vec<Turn>,
    pub archived: bool,
    pub mtime: f64,
}

/// What line 1 of a rollout says about it.
#[derive(Debug, Clone)]
pub struct Meta {
    pub id: String,
    pub cwd: String,
    pub subagent: bool,
    pub forked_from: Option<String>,
}

pub fn ts(s: Option<&str>) -> f64 {
    s.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis() as f64 / 1000.0)
        .unwrap_or(0.0)
}

fn norm(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|c| c.get("text").and_then(Value::as_str))
            .collect::<String>(),
        _ => String::new(),
    }
}

fn preview(t: &str, full: bool) -> String {
    if full {
        t.to_string()
    } else {
        t.chars().take(PREVIEW_CHARS).collect()
    }
}

pub fn fingerprint(user: &str, agents: &[String]) -> u64 {
    let mut h = sha1_smol::Sha1::new();
    h.update(user.as_bytes());
    h.update(b"\0");
    h.update(agents.join("\x01").as_bytes());
    let d = h.digest().bytes();
    u64::from_be_bytes(d[..8].try_into().unwrap())
}

/// Read only line 1. Cheap enough to classify every rollout on every refresh.
pub fn read_meta(path: &Path) -> Option<Meta> {
    let mut line = String::new();
    BufReader::new(File::open(path).ok()?)
        .read_line(&mut line)
        .ok()?;
    let o: Value = serde_json::from_str(&line).ok()?;
    if o.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let p = o.get("payload")?;
    Some(Meta {
        id: p.get("id")?.as_str()?.to_string(),
        cwd: p
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        subagent: !p.get("source").map_or(true, Value::is_string),
        forked_from: p
            .get("forked_from_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

struct RawTurn {
    user: String,
    agents: Vec<String>,
    ts: f64,
    ordinal: Option<i64>,
}

/// Parse one rollout. Returns `None` for subagents and files without a `session_meta`.
/// With `full = true`, prompt and reply texts are kept whole (used by the transcript).
pub fn parse(path: &Path, full: bool) -> Option<Session> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut meta: Option<(Value, Option<i64>)> = None;
    let (mut f1, mut f2, mut f3): (Vec<RawTurn>, Vec<RawTurn>, Vec<RawTurn>) =
        (vec![], vec![], vec![]);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let head = &buf[..buf.len().min(400)];
        if !first && !PREFILTER.iter().any(|k| contains(head, k)) {
            continue;
        }
        first = false;
        let Ok(o) = serde_json::from_slice::<Value>(&buf) else {
            continue; // torn last line while Codex is writing
        };
        let t = o.get("type").and_then(Value::as_str).unwrap_or("");
        let p = o.get("payload").cloned().unwrap_or(Value::Null);
        let when = ts(o.get("timestamp").and_then(Value::as_str));
        let ordinal = o.get("ordinal").and_then(Value::as_i64);
        match t {
            "session_meta" => {
                if meta.is_none() {
                    if !p.get("source").map_or(true, Value::is_string) {
                        return None; // subagent: provisional, never tracked
                    }
                    meta = Some((p, ordinal));
                }
            }
            "event_msg" => match p.get("type").and_then(Value::as_str).unwrap_or("") {
                "item_completed" => {
                    let item = p.get("item").cloned().unwrap_or(Value::Null);
                    match item.get("type").and_then(Value::as_str) {
                        Some("UserMessage") => f2.push(RawTurn {
                            user: norm(&text_of(item.get("content"))),
                            agents: vec![],
                            ts: when,
                            ordinal,
                        }),
                        Some("AgentMessage") => {
                            if let Some(last) = f2.last_mut() {
                                last.agents.push(norm(&text_of(item.get("content"))));
                            }
                        }
                        _ => {}
                    }
                }
                "user_message" => f1.push(RawTurn {
                    user: norm(p.get("message").and_then(Value::as_str).unwrap_or("")),
                    agents: vec![],
                    ts: when,
                    ordinal,
                }),
                "agent_message" => {
                    if let Some(last) = f1.last_mut() {
                        last.agents
                            .push(norm(p.get("message").and_then(Value::as_str).unwrap_or("")));
                    }
                }
                "thread_rolled_back" => {
                    let n = p.get("num_turns").and_then(Value::as_u64).unwrap_or(1) as usize;
                    for log in [&mut f1, &mut f2, &mut f3] {
                        log.truncate(log.len().saturating_sub(n));
                    }
                }
                _ => {}
            },
            "response_item" if p.get("type").and_then(Value::as_str) == Some("message") => {
                let txt = norm(&text_of(p.get("content")));
                match p.get("role").and_then(Value::as_str) {
                    Some("user")
                        if !txt.is_empty()
                            && !INJECTED_PREFIXES.iter().any(|x| txt.starts_with(x)) =>
                    {
                        f3.push(RawTurn {
                            user: txt,
                            agents: vec![],
                            ts: when,
                            ordinal,
                        })
                    }
                    Some("assistant") => {
                        if let Some(last) = f3.last_mut() {
                            last.agents.push(txt);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let (p, meta_ordinal) = meta?;
    let id = p.get("id")?.as_str()?.to_string();
    let (format, raw) = if !f2.is_empty() {
        (Format::F2, f2)
    } else if !f1.is_empty() {
        (Format::F1, f1)
    } else {
        (Format::F3, f3)
    };
    let forked_from = p
        .get("forked_from_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    // explicit cut (0.160+), else the child's own first ordinal continues the parent's sequence (0.147+)
    let fork_cut = p
        .get("forked_from_ordinal_exclusive")
        .and_then(Value::as_i64)
        .or(meta_ordinal.filter(|o| *o > 0));
    let turns = raw
        .into_iter()
        .map(|r| Turn {
            fp: fingerprint(&r.user, &r.agents),
            user: preview(&r.user, full),
            agent: preview(r.agents.last().map(String::as_str).unwrap_or(""), full),
            ts: r.ts,
            ordinal: r.ordinal,
        })
        .collect();
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_secs_f64());
    Some(Session {
        id,
        path: path.to_path_buf(),
        cwd: p
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        created: ts(p.get("timestamp").and_then(Value::as_str)),
        source: p
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        cli_version: p
            .get("cli_version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        forked_from,
        fork_cut,
        format,
        turns,
        archived: path
            .components()
            .any(|c| c.as_os_str() == "archived_sessions"),
        mtime,
    })
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Leading turns written in one burst right after creation = history copied at fork time.
pub fn burst_len(s: &Session) -> usize {
    s.turns
        .iter()
        .take_while(|t| t.ts - s.created <= BURST_WINDOW_S)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    pub fn write_rollout(dir: &Path, name: &str, lines: &[Value]) -> PathBuf {
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        path
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cft-parse-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn f1_turns_with_rollback() {
        let d = tmpdir("f1");
        let p = write_rollout(
            &d,
            "a.jsonl",
            &[
                serde_json::json!({"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":"A","cwd":"/p","source":"cli","timestamp":"2026-01-01T00:00:00Z"}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:10Z","type":"event_msg","payload":{"type":"user_message","message":"hello  world"}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:11Z","type":"event_msg","payload":{"type":"agent_message","message":"hi"}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:20Z","type":"event_msg","payload":{"type":"user_message","message":"oops"}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:21Z","type":"event_msg","payload":{"type":"thread_rolled_back","num_turns":1}}),
            ],
        );
        let s = parse(&p, false).unwrap();
        assert_eq!(s.format, Format::F1);
        assert_eq!(s.turns.len(), 1);
        assert_eq!(s.turns[0].user, "hello world");
        assert_eq!(s.turns[0].agent, "hi");
        assert_eq!(s.turns[0].fp, fingerprint("hello world", &["hi".into()]));
    }

    #[test]
    fn subagent_is_skipped() {
        let d = tmpdir("sub");
        let p = write_rollout(
            &d,
            "s.jsonl",
            &[
                serde_json::json!({"type":"session_meta","payload":{"id":"S","cwd":"/p","source":{"subagent":"x"}}}),
            ],
        );
        assert!(parse(&p, false).is_none());
        assert!(read_meta(&p).unwrap().subagent);
    }

    #[test]
    fn f2_implicit_cut_and_f3_filter() {
        let d = tmpdir("f2");
        let p = write_rollout(
            &d,
            "c.jsonl",
            &[
                serde_json::json!({"timestamp":"2026-01-01T00:00:00Z","ordinal":100,"type":"session_meta","payload":{"id":"C","cwd":"/p","source":"cli","forked_from_id":"P","timestamp":"2026-01-01T00:00:00Z"}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:05Z","ordinal":101,"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions"}]}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:05Z","ordinal":102,"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","id":"u1","content":[{"type":"text","text":"go"}]}}}),
                serde_json::json!({"timestamp":"2026-01-01T00:00:06Z","ordinal":103,"type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","content":[{"type":"Text","text":"done"}]}}}),
            ],
        );
        let s = parse(&p, false).unwrap();
        assert_eq!(s.format, Format::F2);
        assert_eq!(s.fork_cut, Some(100));
        assert_eq!(s.turns.len(), 1);
        assert_eq!(s.turns[0].ordinal, Some(102));
    }
}
