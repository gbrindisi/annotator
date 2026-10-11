//! Port of markedit-comments `src/format/types.ts` (derived from kylemcd/obsidian-document-comments, MIT; see NOTICE).

use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommentStatus {
    #[default]
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadEntry {
    pub author: String,
    /// ISO-8601, optional (the first entry usually carries it via the header).
    pub timestamp: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub authors: Vec<String>,
    /// Zero-based thread entry; None targets the first entry (legacy).
    pub entry: Option<usize>,
}

/// The content of a comment, independent of where it sits in the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommentData {
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub status: CommentStatus,
    /// Redundant copy of the anchored text, the re-anchor fallback.
    pub quote: Option<String>,
    /// Present only for comments anchored to lines inside a fenced code block.
    /// Line numbers, **inclusive at both ends** (`start..end` mirrors TS `{from, to}`).
    pub code_lines: Option<Range<usize>>,
    pub thread: Vec<ThreadEntry>,
    pub reactions: Vec<Reaction>,
    /// Unknown header fields, kept verbatim (e.g. `foo:bar`, `x:"a b"`) and re-emitted after the
    /// known fields so edits never drop them. Not part of the TS reference.
    #[serde(default)]
    pub extra_header: Vec<String>,
}

/// A comment as found in a document, with resolved byte ranges for each piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedComment {
    pub id: String,
    /// `<!--c:ID-->` marker range.
    pub open: Option<Range<usize>>,
    /// `<!--/c:ID-->` marker range.
    pub close: Option<Range<usize>>,
    /// `<!--co:ID ...-->` body block range.
    pub body: Option<Range<usize>>,
    pub data: CommentData,
}
