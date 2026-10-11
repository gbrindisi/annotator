//! Port of markedit-comments `src/format/serialize.ts` (derived from kylemcd/obsidian-document-comments, MIT; see NOTICE).

use super::escape::{
    break_terminator, canonical_author_key, collapse_ws, encode_code_quote, escape_reaction_author,
    escape_text,
};
use super::types::{CommentData, CommentStatus, ThreadEntry};

pub fn open_marker(id: &str) -> String {
    format!("<!--c:{id}-->")
}

pub fn close_marker(id: &str) -> String {
    format!("<!--/c:{id}-->")
}

/// Serialize a comment body block: `<!--co:ID header\n thread\n-->`.
///
/// Header order: `by`, `at`, `status`, `quote`, `line`, then any unknown fields verbatim.
pub fn serialize_body(id: &str, data: &CommentData) -> String {
    let mut head = vec![format!("co:{id}")];
    if let Some(a) = data.author.as_deref().filter(|s| !s.is_empty()) {
        head.push(format!("by:{}", canonical_author_key(a)));
    }
    if let Some(at) = data.created_at.as_deref().filter(|s| !s.is_empty()) {
        head.push(format!("at:{}", collapse_ws(&break_terminator(at), "_")));
    }
    head.push(format!(
        "status:{}",
        match data.status {
            CommentStatus::Open => "open",
            CommentStatus::Resolved => "resolved",
        }
    ));
    if let Some(q) = data.quote.as_deref().filter(|s| !s.is_empty()) {
        let q = if data.code_lines.is_some() {
            encode_code_quote(q)
        } else {
            sanitize_quote(q)
        };
        head.push(format!("quote:\"{q}\""));
    }
    if let Some(r) = &data.code_lines {
        if r.start == r.end {
            head.push(format!("line:{}", r.start));
        } else {
            head.push(format!("line:{}-{}", r.start, r.end));
        }
    }
    head.extend(data.extra_header.iter().cloned());

    let mut body: Vec<String> = data.thread.iter().map(serialize_entry).collect();
    body.extend(
        data.reactions
            .iter()
            .filter(|r| !r.authors.is_empty())
            .map(|r| {
                let target = match r.entry {
                    Some(e) if e > 0 => format!("@{e} "),
                    _ => String::new(),
                };
                let authors: Vec<String> = r
                    .authors
                    .iter()
                    .map(|a| escape_reaction_author(a))
                    .collect();
                format!("+{target}{} {}", r.emoji, authors.join(", "))
            }),
    );
    let block = if body.is_empty() {
        String::new()
    } else {
        body.join("\n") + "\n"
    };
    format!("<!--{}\n{block}-->", head.join(" "))
}

fn serialize_entry(e: &ThreadEntry) -> String {
    let who = match e.timestamp.as_deref().filter(|s| !s.is_empty()) {
        Some(ts) => format!("{} ({ts})", e.author),
        None => e.author.clone(),
    };
    format!(
        "{}: {}",
        break_terminator(&who),
        escape_text(&break_terminator(&e.text))
    )
}

fn sanitize_quote(s: &str) -> String {
    break_terminator(&collapse_ws(s, " ").replace('"', "'"))
        .trim()
        .to_string()
}
