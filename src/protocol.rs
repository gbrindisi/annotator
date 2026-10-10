//! Wire contracts. Shared by all Workers: extend, don't rename.
//!
//! Offsets: inside Rust everything is byte offsets into the raw file. Anything that crosses
//! the wire to the browser is in UTF-16 code units (DOM and CodeMirror positions).

use serde::{Deserialize, Serialize};

// ---------- CLI <-> daemon (HTTP, JSON) ----------

/// `POST /api/open`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRequest {
    /// Absolute, canonical path of the file to review.
    pub path: String,
    /// Launch/reuse a browser tab.
    pub browser: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenResponse {
    pub session_id: String,
    pub round: u32,
    /// Browser URL for this session (includes the token on first use).
    pub url: String,
}

/// `GET /api/sessions/:id/wait?round=N` long-poll result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WaitResult {
    Done { round: u32, summary: Summary },
    Abandoned { round: u32 },
}

/// Factual summary of what the reviewer did in a round. Computed by the daemon on Done.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub path: String,
    pub round: u32,
    /// Ids of threads created this round.
    pub new_threads: Vec<String>,
    /// Ids of pre-existing threads that got new entries this round.
    pub replied_threads: Vec<String>,
    pub resolved_threads: Vec<String>,
    pub deleted_threads: Vec<String>,
    /// Text edits outside comment markers/bodies.
    pub edit_hunks: u32,
    pub lines_added: u32,
    pub lines_removed: u32,
}

// ---------- browser <-> daemon (WebSocket, JSON) ----------

/// Messages from the browser: intents only. The browser never writes files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Add a comment on a selection made in the rendered view.
    AddComment {
        selection: Selection,
        body: String,
    },
    /// Add a comment on a raw source range (source mode), UTF-16 offsets into the raw file.
    AddCommentRaw {
        from: usize,
        to: usize,
        body: String,
    },
    Reply {
        id: String,
        body: String,
    },
    EditEntry {
        id: String,
        entry: usize,
        body: String,
    },
    DeleteEntry {
        id: String,
        entry: usize,
    },
    Resolve {
        id: String,
    },
    Reopen {
        id: String,
    },
    DeleteThread {
        id: String,
    },
    /// Source-mode autosave: full raw text, rejected if the file changed since `base_hash`.
    ReplaceSource {
        text: String,
        base_hash: String,
    },
    Done,
    Abandon,
}

/// A selection in the rendered view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Selection {
    /// `data-sourcepos` of the innermost block element containing the selection
    /// (comrak format `l1:c1-l2:c2`, refers to the text the daemon rendered).
    pub sourcepos: String,
    /// The selected text as the browser sees it (`Selection.toString()`).
    pub text: String,
    /// Start of the selection within the block element's `textContent`, UTF-16 units.
    pub offset: usize,
}

/// Messages from the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Full document state; sent on connect and after every change.
    Doc(DocState),
    /// An intent failed (bad selection, hash conflict, ...). `reloaded` = a fresh Doc follows.
    Error { message: String, reloaded: bool },
    /// The round ended; the page should show a "sent back" state until the next round.
    RoundClosed { round: u32 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocState {
    pub path: String,
    pub round: u32,
    /// sha256 hex of the raw file; echo it as `base_hash` in ReplaceSource.
    pub hash: String,
    pub raw: String,
    /// Rendered HTML (markers stripped, `<mark data-cid=ID>` around anchors, `data-sourcepos`).
    pub html: String,
    pub comments: Vec<CommentView>,
    /// UTF-16 ranges of marker/body text in `raw`, for hiding in CodeMirror.
    pub marker_ranges: Vec<(usize, usize)>,
    /// Changes vs the last Done snapshot (empty in round 1). Filled in wave 2.
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentView {
    pub id: String,
    pub status: String,
    pub quote: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub orphaned: bool,
    pub entries: Vec<EntryView>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryView {
    pub author: String,
    pub timestamp: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Change {
    pub kind: String,
    /// `data-sourcepos` of the changed block in the current render, when known.
    pub sourcepos: Option<String>,
}

// ---------- daemon status (additive) ----------

/// `GET /api/status`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusResponse {
    pub pid: u32,
    pub port: u16,
    pub uptime_secs: u64,
    pub sessions: Vec<SessionStatus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStatus {
    pub id: String,
    pub path: String,
    pub round: u32,
    /// `active`, `done` or `abandoned`.
    pub state: String,
    /// Connected WebSocket clients (browser tabs).
    pub clients: usize,
    /// Pending `wait` long-polls.
    pub waiters: usize,
}

/// Contents of `daemon.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
    pub token: String,
}
