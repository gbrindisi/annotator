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
    /// Unified diff (3 lines of context) of the document with comment markers and bodies
    /// stripped, from the round's start snapshot to the file at Done. Empty when no text edits.
    /// Additive; absent from older daemons.
    #[serde(default)]
    pub diff: String,
    /// Per thread touched this round, in document order of the anchor (orphans and deleted
    /// threads last). Additive; absent from older daemons.
    #[serde(default)]
    pub comments: Vec<CommentActivity>,
}

/// What happened to one comment thread in a round. If several things happened to a thread, `kind`
/// is the first that applies of: `deleted`, `new`, `resolved`, `reopened`, `reply`, `edited`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommentKind {
    New,
    Reply,
    Resolved,
    Reopened,
    Deleted,
    /// An existing entry's text changed (or an entry was removed).
    Edited,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommentActivity {
    pub id: String,
    pub kind: CommentKind,
    /// The anchored text as it is now; the stored quote for orphans, code comments and deleted
    /// threads.
    pub quote: String,
    /// For code-block comments: 0-based inclusive lines within the block (as `CommentView`).
    pub code_lines: Option<(usize, usize)>,
    /// Entries added this round and entries whose text changed (new text). For `deleted`: the
    /// last known entries.
    pub new_entries: Vec<EntryView>,
    /// `open` or `resolved` after the round (last known status for `deleted`).
    pub status: String,
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
    /// Edit mode: replace the content of one rendered block (`data-sourcepos` of a paragraph,
    /// heading, table cell or list item) with `markdown` (inline Markdown for that block only;
    /// a blank line splits it into new blocks). Rejected if the file changed since `base_hash`.
    ReplaceBlock {
        sourcepos: String,
        markdown: String,
        base_hash: String,
    },
    /// Edit mode: Backspace at the start of block `second` merges it into the preceding block
    /// `first`, whose new content is `markdown`.
    MergeBlocks {
        first: String,
        second: String,
        markdown: String,
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
#[allow(clippy::large_enum_variant)] // Doc dominates traffic; boxing buys nothing here.
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
    /// Changes vs the last Done snapshot (empty in round 1).
    pub changes: Vec<Change>,
    /// `active` (reviewer's turn) or `closed` (sent back, waiting for the agent's next round).
    pub round_state: String,
    /// Reviewer name used for new comments/replies (config `author`, else $USER).
    pub author: String,
    /// Pending `wait` long-polls (agents blocked on `annotator open`) for the current round.
    /// 0 = nobody is waiting (e.g. `open --no-wait`). Additive; a fresh Doc is pushed on change.
    #[serde(default)]
    pub waiters: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentView {
    pub id: String,
    pub status: String,
    pub quote: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub orphaned: bool,
    /// Anchored to lines of a fenced code block (`line:`), 0-based inclusive lines.
    pub code_lines: Option<(usize, usize)>,
    /// UTF-16 range of the anchored text in `raw` (between the markers), when anchored.
    pub anchor: Option<(usize, usize)>,
    /// For code-line comments (`code_lines`): the `data-sourcepos` of the `<pre>` of the code block
    /// it belongs to, exactly as it appears in `DocState.html`. Additive (null otherwise, absent
    /// from older daemons); lets the UI map comments to blocks, e.g. rendered Mermaid diagrams.
    #[serde(default)]
    pub block_sourcepos: Option<String>,
    pub entries: Vec<EntryView>,
    /// Entries added since the last Done snapshot (round > 1), e.g. agent replies. 0 in round 1.
    pub new_entries: usize,
    /// Thread did not exist at the last Done snapshot.
    pub new_thread: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EntryView {
    pub author: String,
    pub timestamp: Option<String>,
    pub text: String,
}

/// A block-level change vs the last Done snapshot, computed on clean text (markers excluded).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Change {
    /// `added`, `modified` or `removed`.
    pub kind: String,
    /// `data-sourcepos` of the changed top-level block in the current render (None for `removed`).
    pub sourcepos: Option<String>,
    /// Previous text of the block (clean markdown) for `modified`/`removed`, for a hover diff.
    pub old_text: Option<String>,
    /// For `removed`: `data-sourcepos` of the block after which the removed text used to be
    /// (None = start of document).
    pub after: Option<String>,
    /// Old block rendered to HTML (same pipeline as the doc) for `modified`/`removed`, so the UI
    /// can show the removed content inline. Additive; absent from older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_html: Option<String>,
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
