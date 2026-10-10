//! Port of `reference/markedit-comments/format/edits.ts`: pure string operations.
//!
//! The `compute_*` functions return [`Change`]s in original coordinates (CodeMirror semantics);
//! the convenience wrappers (`add_comment`, `append_reply`, ...) apply them and return the new
//! document. Offsets are byte offsets; offsets that are out of bounds or not on a char boundary
//! are clamped/snapped down to the previous char boundary.

use super::ids::new_id;
use super::parse::{is_in_fenced_code, parse_comments};
use super::serialize::{close_marker, open_marker, serialize_body};
use super::types::{CommentData, CommentStatus, ParsedComment, Reaction, ThreadEntry};
use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

/// A document edit in original coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("{0}")]
    InvalidSelection(String),
    #[error("The selection moved. Try adding the comment again.")]
    SelectionMoved,
    #[error("Write a comment first.")]
    EmptyText,
    #[error("Comment not found.")]
    NotFound,
    #[error("Comment has no body to update.")]
    NoBody,
    #[error("That entry no longer exists.")]
    NoSuchEntry,
    #[error("Nothing to delete.")]
    NothingToDelete,
}

pub type EditResult = Result<Vec<Change>, EditError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommentInput {
    /// Comment id; `None` generates a fresh one unique in the document.
    pub id: Option<String>,
    pub created_at: String,
    pub author: String,
    pub text: String,
    /// The text the user selected when the composer opened; refuse if the doc moved.
    pub expected: Option<String>,
}

fn floor_boundary(doc: &str, mut i: usize) -> usize {
    i = i.min(doc.len());
    while !doc.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn char_at(doc: &str, i: usize) -> Option<char> {
    doc.get(i..).and_then(|s| s.chars().next())
}

fn char_before(doc: &str, i: usize) -> Option<char> {
    doc.get(..i).and_then(|s| s.chars().next_back())
}

/// Normalize a raw selection into the range that will be wrapped by markers.
pub fn normalize_selection(doc: &str, from: usize, to: usize) -> Range<usize> {
    let (mut from, mut to) = if to < from { (to, from) } else { (from, to) };
    from = floor_boundary(doc, from);
    to = floor_boundary(doc, to);
    while from < to {
        match char_at(doc, from) {
            Some(c) if c.is_whitespace() => from += c.len_utf8(),
            _ => break,
        }
    }
    while to > from {
        match char_before(doc, to) {
            Some(c) if c.is_whitespace() => to -= c.len_utf8(),
            _ => break,
        }
    }
    expand_inline_code_selection(doc, from, to)
}

static BLANK_LINE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t]*\n").unwrap());

/// Validate a selection for a new comment. Returns an error message or None.
pub fn validate_selection(doc: &str, from: usize, to: usize) -> Option<&'static str> {
    if to == from {
        return Some("Select some text to comment on.");
    }
    if is_in_fenced_code(doc, from) || is_in_fenced_code(doc, to.saturating_sub(1)) {
        return Some("Comments inside fenced code blocks are not supported yet.");
    }
    let (sf, st) = (floor_boundary(doc, from), floor_boundary(doc, to));
    if sf <= st && BLANK_LINE_RE.is_match(&doc[sf..st]) {
        return Some("Select text within a single paragraph.");
    }
    for c in parse_comments(doc) {
        for m in [&c.open, &c.close].into_iter().flatten() {
            if m.start < to && m.end > from && !(m.start >= from && m.end <= to) {
                return Some("The selection cuts through an existing comment marker.");
            }
        }
        let (Some(open), Some(close)) = (&c.open, &c.close) else {
            continue;
        };
        let contains_both = open.start >= from && close.end <= to;
        let inside_anchor = from >= open.end && to <= close.start;
        let touches_either =
            (open.start < to && open.end > from) || (close.start < to && close.end > from);
        let straddles = from < open.start && to > open.start && to < close.end;
        if !contains_both && !inside_anchor && (touches_either || straddles) {
            return Some(
                "The selection partially overlaps an existing comment. Select all of it or a part inside it.",
            );
        }
    }
    None
}

/// Wrap `from..to` with anchor markers and append a body block after the containing block.
pub fn compute_add_comment(
    doc: &str,
    from: usize,
    to: usize,
    id: &str,
    input: &NewCommentInput,
) -> EditResult {
    let Range {
        start: from,
        end: to,
    } = normalize_selection(doc, from, to);
    if let Some(msg) = validate_selection(doc, from, to) {
        return Err(EditError::InvalidSelection(msg.to_string()));
    }
    if input
        .expected
        .as_deref()
        .is_some_and(|e| e != &doc[from..to])
    {
        return Err(EditError::SelectionMoved);
    }
    if input.text.trim().is_empty() {
        return Err(EditError::EmptyText);
    }
    let data = CommentData {
        author: Some(input.author.clone()),
        created_at: Some(input.created_at.clone()),
        status: CommentStatus::Open,
        quote: Some(strip_markers(&doc[from..to])),
        code_lines: None,
        thread: vec![ThreadEntry {
            author: input.author.clone(),
            timestamp: Some(input.created_at.clone()),
            text: input.text.clone(),
        }],
        reactions: Vec::new(),
        extra_header: Vec::new(),
    };
    let para_end = block_end(doc, to);
    Ok(vec![
        Change {
            from,
            to: from,
            insert: open_marker(id),
        },
        Change {
            from: to,
            to,
            insert: close_marker(id),
        },
        Change {
            from: para_end,
            to: para_end,
            insert: format!("\n{}", serialize_body(id, &data)),
        },
    ])
}

/// When a selection is within one inline-code token, anchor the whole token.
pub fn expand_inline_code_selection(doc: &str, from: usize, to: usize) -> Range<usize> {
    let b = doc.as_bytes();
    let (from, to) = (from.min(b.len()), to.min(b.len()));
    // JS `lastIndexOf('\n', -1)` only inspects index 0.
    let line_from = if from == 0 {
        usize::from(b.first() == Some(&b'\n'))
    } else {
        b[..from]
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(0, |p| p + 1)
    };
    let line_to = b[to..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| p + to);

    let mut open = line_from;
    while open < line_to {
        if b[open] == b'`' && !is_escaped(b, open) {
            let ticks = backtick_run(b, open, line_to);
            let content_from = open + ticks;
            let mut cursor = content_from;
            while cursor < line_to {
                let Some(candidate) = b[cursor..line_to]
                    .iter()
                    .position(|&c| c == b'`')
                    .map(|p| p + cursor)
                else {
                    break;
                };
                let close_ticks = backtick_run(b, candidate, line_to);
                if close_ticks == ticks && !is_escaped(b, candidate) {
                    if from >= content_from && to <= candidate {
                        return open..candidate + close_ticks;
                    }
                    open = candidate + close_ticks - 1;
                    break;
                }
                cursor = candidate + close_ticks;
            }
        }
        open += 1;
    }
    from..to
}

fn backtick_run(b: &[u8], from: usize, limit: usize) -> usize {
    b[from..limit].iter().take_while(|&&c| c == b'`').count()
}

fn is_escaped(b: &[u8], pos: usize) -> bool {
    b[..pos].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1
}

pub fn compute_append_reply(
    doc: &str,
    id: &str,
    created_at: &str,
    author: &str,
    text: &str,
) -> EditResult {
    replace_body(doc, id, |c| {
        let mut d = c.data.clone();
        d.thread.push(ThreadEntry {
            author: author.to_string(),
            timestamp: Some(created_at.to_string()),
            text: text.to_string(),
        });
        Some(d)
    })
}

pub fn compute_set_resolved(doc: &str, id: &str, resolved: bool) -> EditResult {
    replace_body(doc, id, |c| {
        let mut d = c.data.clone();
        d.status = if resolved {
            CommentStatus::Resolved
        } else {
            CommentStatus::Open
        };
        Some(d)
    })
}

/// Replace the text of the `index`-th message in a thread.
pub fn compute_edit_entry(doc: &str, id: &str, index: usize, text: &str) -> EditResult {
    replace_body(doc, id, |c| {
        let mut d = c.data.clone();
        d.thread.get_mut(index)?.text = text.to_string();
        Some(d)
    })
}

/// Remove the `index`-th message. Removing the last remaining entry deletes the thread.
pub fn compute_delete_entry(doc: &str, id: &str, index: usize) -> EditResult {
    if let Some(c) = parse_comments(doc).into_iter().find(|c| c.id == id)
        && c.data.thread.len() == 1
        && index == 0
    {
        return compute_delete_comment(doc, id);
    }
    replace_body(doc, id, |c| {
        if index >= c.data.thread.len() {
            return None;
        }
        let mut d = c.data.clone();
        d.thread.remove(index);
        d.reactions = reactions_after_entry_delete(&c.data.reactions, index);
        Some(d)
    })
}

fn replace_body(
    doc: &str,
    id: &str,
    mutate: impl FnOnce(&ParsedComment) -> Option<CommentData>,
) -> EditResult {
    let c = parse_comments(doc)
        .into_iter()
        .find(|c| c.id == id)
        .ok_or(EditError::NotFound)?;
    let body = c.body.clone().ok_or(EditError::NoBody)?;
    let data = mutate(&c).ok_or(EditError::NoSuchEntry)?;
    Ok(vec![Change {
        from: body.start,
        to: body.end,
        insert: serialize_body(id, &data),
    }])
}

fn reactions_after_entry_delete(reactions: &[Reaction], deleted: usize) -> Vec<Reaction> {
    reactions
        .iter()
        .filter_map(|r| {
            let entry = r.entry.unwrap_or(0);
            if entry == deleted {
                return None;
            }
            let next = if entry > deleted { entry - 1 } else { entry };
            Some(Reaction {
                emoji: r.emoji.clone(),
                authors: r.authors.clone(),
                entry: (next != 0).then_some(next),
            })
        })
        .collect()
}

/// Remove every marker and body block for `id`, leaving the anchored text in place.
///
/// Like the reference, this scans the raw document (no code masking), so identically-named
/// markers inside code blocks are removed too.
pub fn compute_delete_comment(doc: &str, id: &str) -> EditResult {
    if !parse_comments(doc).iter().any(|c| c.id == id) {
        return Err(EditError::NotFound);
    }
    let b = doc.as_bytes();
    let at = |p: usize| b.get(p).copied();
    let leading = |p: usize| -> usize {
        if p >= 1 && at(p - 1) == Some(b'\n') {
            if p >= 2 && at(p - 2) == Some(b'\r') {
                2
            } else {
                1
            }
        } else {
            0
        }
    };
    let trailing = |p: usize| -> usize {
        match (at(p), at(p + 1)) {
            (Some(b'\r'), Some(b'\n')) => 2,
            (Some(b'\n'), _) => 1,
            _ => 0,
        }
    };
    let alone = |from: usize, to: usize| {
        (from == 0 || leading(from) > 0) && (to == b.len() || trailing(to) > 0)
    };

    let mut ranges = Vec::new();
    let open = open_marker(id);
    for (from, _) in doc.match_indices(&open) {
        let to = from + open.len();
        let end = if alone(from, to) {
            to + trailing(to)
        } else {
            to
        };
        ranges.push(Change {
            from,
            to: end,
            insert: String::new(),
        });
    }
    let close = close_marker(id);
    for (from, _) in doc.match_indices(&close) {
        let to = from + close.len();
        let start = if alone(from, to) {
            from - leading(from)
        } else {
            from
        };
        ranges.push(Change {
            from: start,
            to,
            insert: String::new(),
        });
    }
    let head = format!("<!--co:{id}");
    let mut pos = 0;
    while let Some(rel) = doc[pos..].find(&head) {
        let from = pos + rel;
        let after = from + head.len();
        if at(after).is_some_and(|c| c.is_ascii_alphanumeric()) {
            pos = from + 1;
            continue;
        }
        let Some(end) = doc[after..].find("-->") else {
            break;
        };
        let to = after + end + 3;
        ranges.push(Change {
            from: from - leading(from),
            to,
            insert: String::new(),
        });
        pos = to;
    }
    if ranges.is_empty() {
        return Err(EditError::NothingToDelete);
    }
    ranges.sort_by_key(|c| c.from);
    Ok(ranges)
}

static MARKER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--/?c:[A-Za-z0-9]+-->").unwrap());

/// Remove anchor markers from text.
pub fn strip_markers(s: &str) -> String {
    MARKER_RE.replace_all(s, "").into_owned()
}

/// Apply changes given in original coordinates (stable-sorted by `from`).
pub fn apply_changes(doc: &str, changes: &[Change]) -> String {
    let mut ordered: Vec<&Change> = changes.iter().collect();
    ordered.sort_by_key(|c| c.from);
    let mut out = String::with_capacity(doc.len());
    let mut last = 0;
    for c in ordered {
        if c.from > last {
            out.push_str(&doc[floor_boundary(doc, last)..floor_boundary(doc, c.from)]);
        }
        out.push_str(&c.insert);
        last = last.max(c.to);
    }
    out.push_str(&doc[floor_boundary(doc, last)..]);
    out
}

/// End offset of the contiguous (non-blank) block of lines containing `pos`.
pub fn block_end(doc: &str, pos: usize) -> usize {
    let pos = floor_boundary(doc, pos);
    let Some(mut line_end) = doc[pos..].find('\n').map(|p| p + pos) else {
        return doc.len();
    };
    loop {
        let next_start = line_end + 1;
        let next_end = doc[next_start..]
            .find('\n')
            .map_or(doc.len(), |p| p + next_start);
        if doc[next_start..next_end].trim().is_empty() {
            return line_end;
        }
        line_end = next_end;
        if next_end == doc.len() {
            return doc.len();
        }
    }
}

/// The comment whose anchor contains `pos` (inclusive at both ends), innermost first.
pub fn comment_at(comments: &[ParsedComment], pos: usize) -> Option<&ParsedComment> {
    let mut best = None;
    let mut best_len = usize::MAX;
    for c in comments {
        if let Some(r) = c.anchor_range()
            && pos >= r.start
            && pos <= r.end
            && r.end - r.start < best_len
        {
            best = Some(c);
            best_len = r.end - r.start;
        }
    }
    best
}

// ---- Convenience wrappers returning the new document ----

/// Add a comment; returns `(new_doc, id)`. Generates an id when `input.id` is None.
pub fn add_comment(
    doc: &str,
    from: usize,
    to: usize,
    input: &NewCommentInput,
) -> Result<(String, String), EditError> {
    let id = match &input.id {
        Some(id) => id.clone(),
        None => {
            let ids = super::parse::existing_ids(doc);
            new_id(&ids.iter().map(String::as_str).collect::<Vec<_>>())
        }
    };
    let changes = compute_add_comment(doc, from, to, &id, input)?;
    Ok((apply_changes(doc, &changes), id))
}

pub fn append_reply(
    doc: &str,
    id: &str,
    created_at: &str,
    author: &str,
    text: &str,
) -> Result<String, EditError> {
    Ok(apply_changes(
        doc,
        &compute_append_reply(doc, id, created_at, author, text)?,
    ))
}

pub fn set_resolved(doc: &str, id: &str, resolved: bool) -> Result<String, EditError> {
    Ok(apply_changes(
        doc,
        &compute_set_resolved(doc, id, resolved)?,
    ))
}

pub fn edit_entry(doc: &str, id: &str, index: usize, text: &str) -> Result<String, EditError> {
    Ok(apply_changes(
        doc,
        &compute_edit_entry(doc, id, index, text)?,
    ))
}

pub fn delete_entry(doc: &str, id: &str, index: usize) -> Result<String, EditError> {
    Ok(apply_changes(doc, &compute_delete_entry(doc, id, index)?))
}

pub fn delete_comment(doc: &str, id: &str) -> Result<String, EditError> {
    Ok(apply_changes(doc, &compute_delete_comment(doc, id)?))
}

/// Replace a thread's body block with `data` re-serialized (generic update hook for the daemon).
pub fn replace_comment_data(doc: &str, id: &str, data: &CommentData) -> Result<String, EditError> {
    Ok(apply_changes(
        doc,
        &replace_body(doc, id, |_| Some(data.clone()))?,
    ))
}
