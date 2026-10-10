//! Review sessions keyed by canonical file path, with rounds and on-disk snapshots under
//! `~/.annotator/sessions/<id>/`. Owned by the daemon/CLI Worker.
//!
//! Layout:
//! ```text
//! $ANNOTATOR_HOME (default ~/.annotator)/
//!   daemon.json            {pid, port, token}   (present while a daemon runs)
//!   daemon.log             daemon stderr
//!   spawn.lock             flock guarding daemon spawn
//!   sessions/<id>/meta.json        {path, round, round_state, last_result}
//!   sessions/<id>/round-<N>.md     snapshot at the start of round N
//!   sessions/<id>/done-<N>.md      snapshot when round N was sent back
//! ```

use crate::protocol::WaitResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// `$ANNOTATOR_HOME` or `~/.annotator`.
pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("ANNOTATOR_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(h);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".annotator")
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Session id: first 12 hex chars of sha256(canonical path).
pub fn session_id(canonical: &Path) -> String {
    sha256_hex(canonical.to_string_lossy().as_bytes())[..12].to_string()
}

pub fn valid_id(id: &str) -> bool {
    id.len() == 12 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Write via temp file in the same directory + rename.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.annotator-{}.tmp", std::process::id()));
    std::fs::write(&tmp, data)?;
    // Keep the original permissions when replacing an existing file.
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundState {
    Active,
    Done,
    Abandoned,
}

impl RoundState {
    pub fn as_str(self) -> &'static str {
        match self {
            RoundState::Active => "active",
            RoundState::Done => "done",
            RoundState::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub path: String,
    pub round: u32,
    pub round_state: RoundState,
    /// Result of the last closed round, so late waiters still get it.
    #[serde(default)]
    pub last_result: Option<WaitResult>,
}

pub fn session_dir(home: &Path, id: &str) -> PathBuf {
    home.join("sessions").join(id)
}

pub fn load_meta(home: &Path, id: &str) -> Option<Meta> {
    let data = std::fs::read(session_dir(home, id).join("meta.json")).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn save_meta(home: &Path, id: &str, meta: &Meta) -> std::io::Result<()> {
    let dir = session_dir(home, id);
    std::fs::create_dir_all(&dir)?;
    atomic_write(&dir.join("meta.json"), &serde_json::to_vec_pretty(meta)?)
}

pub fn round_snapshot(home: &Path, id: &str, round: u32) -> PathBuf {
    session_dir(home, id).join(format!("round-{round}.md"))
}

pub fn done_snapshot(home: &Path, id: &str, round: u32) -> PathBuf {
    session_dir(home, id).join(format!("done-{round}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_12_hex() {
        let id = session_id(Path::new("/tmp/plan.md"));
        assert!(valid_id(&id));
        assert!(!valid_id("../etc"));
    }
}
