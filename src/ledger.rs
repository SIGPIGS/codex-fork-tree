//! User data that must survive cache rebuilds: forks the TUI created and confirm/reject verdicts.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Confirm,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEdge {
    pub child: String,
    pub parent: String,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictEntry {
    pub child: String,
    pub parent: String,
    pub verdict: Verdict,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub recorded: Vec<RecordedEdge>,
    #[serde(default)]
    pub verdicts: Vec<VerdictEntry>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Ledger {
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::codex::home_dir().join(".local/share"));
        base.join("codex-fork-tree/ledger.json")
    }

    pub fn load(path: PathBuf) -> Ledger {
        let mut l: Ledger = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        l.path = Some(path);
        l
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = p.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
            std::fs::rename(tmp, p)?;
        }
        Ok(())
    }

    pub fn recorded_parent(&self, child: &str) -> Option<&str> {
        self.recorded
            .iter()
            .rev()
            .find(|e| e.child == child)
            .map(|e| e.parent.as_str())
    }

    pub fn record_fork(&mut self, child: &str, parent: &str) {
        self.recorded.retain(|e| e.child != child);
        self.recorded.push(RecordedEdge {
            child: child.into(),
            parent: parent.into(),
            at: chrono::Utc::now().timestamp(),
        });
    }

    pub fn verdict(&self, child: &str, parent: &str) -> Option<Verdict> {
        self.verdicts
            .iter()
            .rev()
            .find(|v| v.child == child && v.parent == parent)
            .map(|v| v.verdict)
    }

    pub fn set_verdict(&mut self, child: &str, parent: &str, verdict: Verdict) {
        self.verdicts
            .retain(|v| !(v.child == child && v.parent == parent));
        self.verdicts.push(VerdictEntry {
            child: child.into(),
            parent: parent.into(),
            verdict,
        });
    }
}
