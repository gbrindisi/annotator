//! Round summary: what the reviewer did between the round snapshot and Done.

use crate::format::{CommentStatus, ParsedComment, ThreadEntry, parse_comments};
use crate::protocol::{CommentActivity, CommentKind, EntryView, Summary};
use crate::render::strip;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

fn entry_view(e: &ThreadEntry) -> EntryView {
    EntryView {
        author: e.author.clone(),
        timestamp: e.timestamp.clone(),
        text: e.text.clone(),
    }
}

fn status_str(s: CommentStatus) -> String {
    match s {
        CommentStatus::Open => "open",
        CommentStatus::Resolved => "resolved",
    }
    .into()
}

fn code_lines(c: &ParsedComment) -> Option<(usize, usize)> {
    c.data.code_lines.as_ref().map(|r| (r.start, r.end))
}

/// The anchored text of `c` in `doc` (markers stripped), or its stored quote.
fn quote_of(c: &ParsedComment, doc: &str) -> String {
    let stored = c.data.quote.clone().unwrap_or_default();
    if c.data.code_lines.is_some() {
        return stored;
    }
    match c.anchor_range() {
        Some(r) => {
            let t = strip(&doc[r]).0;
            if t.is_empty() { stored } else { t }
        }
        None => stored,
    }
}

/// `before` = `round-N.md` snapshot, `after` = file content at Done.
pub fn compute(path: &str, round: u32, before: &str, after: &str) -> Summary {
    let mut s = Summary {
        path: path.to_string(),
        round,
        ..Default::default()
    };
    let old_list = parse_comments(before);
    let old: HashMap<&str, &ParsedComment> = old_list.iter().map(|c| (c.id.as_str(), c)).collect();
    let new = parse_comments(after);
    // (sort key: anchor start or MAX, activity); stable sort keeps parse order among orphans.
    let mut activity: Vec<(usize, CommentActivity)> = Vec::new();
    for c in &new {
        let key = c.anchor_range().map_or(usize::MAX, |r| r.start);
        let mk = |kind: CommentKind, new_entries: Vec<EntryView>| CommentActivity {
            id: c.id.clone(),
            kind,
            quote: quote_of(c, after),
            code_lines: code_lines(c),
            new_entries,
            status: status_str(c.data.status),
        };
        match old.get(c.id.as_str()) {
            None => {
                s.new_threads.push(c.id.clone());
                let entries = c.data.thread.iter().map(entry_view).collect();
                activity.push((key, mk(CommentKind::New, entries)));
            }
            Some(o) => {
                let (ot, nt) = (&o.data.thread, &c.data.thread);
                let replied = nt.len() > ot.len();
                if replied {
                    s.replied_threads.push(c.id.clone());
                }
                let resolved = o.data.status == CommentStatus::Open
                    && c.data.status == CommentStatus::Resolved;
                if resolved {
                    s.resolved_threads.push(c.id.clone());
                }
                let reopened = o.data.status == CommentStatus::Resolved
                    && c.data.status == CommentStatus::Open;
                let edited_idx: Vec<usize> = (0..ot.len().min(nt.len()))
                    .filter(|&i| ot[i].text != nt[i].text || ot[i].author != nt[i].author)
                    .collect();
                let edited = !edited_idx.is_empty() || nt.len() < ot.len();
                let kind = if resolved {
                    CommentKind::Resolved
                } else if reopened {
                    CommentKind::Reopened
                } else if replied {
                    CommentKind::Reply
                } else if edited {
                    CommentKind::Edited
                } else {
                    continue;
                };
                let mut entries: Vec<EntryView> =
                    edited_idx.iter().map(|&i| entry_view(&nt[i])).collect();
                entries.extend(nt.iter().skip(ot.len()).map(entry_view));
                activity.push((key, mk(kind, entries)));
            }
        }
    }
    let mut deleted: Vec<&ParsedComment> = old_list
        .iter()
        .filter(|c| !new.iter().any(|n| n.id == c.id))
        .collect();
    for c in &deleted {
        let mut quote = c.data.quote.clone().unwrap_or_default();
        if quote.is_empty() {
            quote = quote_of(c, before);
        }
        activity.push((
            usize::MAX,
            CommentActivity {
                id: c.id.clone(),
                kind: CommentKind::Deleted,
                quote,
                code_lines: code_lines(c),
                new_entries: c.data.thread.iter().map(entry_view).collect(),
                status: status_str(c.data.status),
            },
        ));
    }
    deleted.sort_by(|a, b| a.id.cmp(&b.id));
    s.deleted_threads = deleted.iter().map(|c| c.id.clone()).collect();
    // Anchored threads by document order; orphans and deleted last (stable).
    let (mut anchored, rest): (Vec<_>, Vec<_>) =
        activity.into_iter().partition(|(k, _)| *k != usize::MAX);
    anchored.sort_by_key(|(k, _)| *k);
    s.comments = anchored.into_iter().chain(rest).map(|(_, a)| a).collect();

    let (b, a) = (strip(before).0, strip(after).0);
    let diff = TextDiff::from_lines(&b, &a);
    for c in diff.iter_all_changes() {
        match c.tag() {
            ChangeTag::Insert => s.lines_added += 1,
            ChangeTag::Delete => s.lines_removed += 1,
            ChangeTag::Equal => {}
        }
    }
    s.edit_hunks = diff.grouped_ops(0).len() as u32;
    if b != a {
        let name = path.rsplit('/').next().unwrap_or(path);
        s.diff = diff
            .unified_diff()
            .context_radius(3)
            .header(
                &format!("{name} (round {round} start)"),
                &format!("{name} (after review)"),
            )
            .to_string();
    }
    s
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `text` with continuation lines indented by `indent`; blank lines stay empty.
fn indent_text(text: &str, indent: &str) -> String {
    text.trim_end()
        .lines()
        .enumerate()
        .map(|(i, l)| match (i, l.is_empty()) {
            (0, _) => l.to_string(),
            (_, true) => String::new(),
            _ => format!("{indent}{l}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn short_quote(q: &str) -> String {
    let flat = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 100 {
        format!("{}…", flat.chars().take(100).collect::<String>())
    } else {
        flat
    }
}

fn comment_header(c: &CommentActivity) -> String {
    let kind = match c.kind {
        CommentKind::New => "new",
        CommentKind::Reply => "reply",
        CommentKind::Resolved => "resolved",
        CommentKind::Reopened => "reopened",
        CommentKind::Deleted => "deleted",
        CommentKind::Edited => "edited",
    };
    let quote = short_quote(&c.quote);
    let place = match (c.code_lines, quote.is_empty()) {
        (Some((a, b)), q) => {
            // 1-based, as the UI shows them.
            let lines = if a == b {
                format!("code line {}", a + 1)
            } else {
                format!("code lines {}-{}", a + 1, b + 1)
            };
            if q {
                format!(" on {lines}")
            } else {
                format!(" on {lines} (\"{quote}\")")
            }
        }
        (None, true) => String::new(),
        (None, false) => format!(" on \"{quote}\""),
    };
    format!("[{}] {kind}{place}", c.id)
}

pub fn to_text(s: &Summary) -> String {
    let mut out = format!(
        "annotator: review of {} finished (round {})\n",
        s.path, s.round
    );
    if s.comments.is_empty() && s.diff.is_empty() && s.edit_hunks == 0 {
        out.push_str("- no changes\n");
        return out;
    }
    if !s.comments.is_empty() {
        let count = |k: CommentKind| s.comments.iter().filter(|c| c.kind == k).count();
        let parts: Vec<String> = [
            (CommentKind::New, "new", "new"),
            (CommentKind::Reply, "reply", "replies"),
            (CommentKind::Edited, "edited", "edited"),
            (CommentKind::Resolved, "resolved", "resolved"),
            (CommentKind::Reopened, "reopened", "reopened"),
            (CommentKind::Deleted, "deleted", "deleted"),
        ]
        .iter()
        .filter_map(|&(k, one, many)| {
            let n = count(k);
            (n > 0).then(|| {
                if one == many {
                    format!("{n} {one}")
                } else {
                    plural(n, one, many)
                }
            })
        })
        .collect();
        out.push_str(&format!("\n## Comments ({})\n", parts.join(", ")));
        for c in &s.comments {
            out.push_str(&comment_header(c));
            out.push('\n');
            for e in &c.new_entries {
                let text = indent_text(&format!("{}: {}", e.author, e.text), "    ");
                out.push_str(&format!("  {text}\n"));
            }
        }
    }
    if !s.diff.is_empty() {
        let hunks = s.diff.lines().filter(|l| l.starts_with("@@ ")).count();
        out.push_str(&format!(
            "\n## Edits ({}, +{} -{})\n{}",
            plural(hunks, "hunk", "hunks"),
            s.lines_added,
            s.lines_removed,
            s.diff
        ));
        if !s.diff.ends_with('\n') {
            out.push('\n');
        }
    } else if s.edit_hunks > 0 {
        out.push_str(&format!(
            "\n## Edits ({}, +{} -{})\n",
            plural(s.edit_hunks as usize, "hunk", "hunks"),
            s.lines_added,
            s.lines_removed
        ));
    }
    out
}
