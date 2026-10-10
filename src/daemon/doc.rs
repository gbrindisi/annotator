//! DocState pieces derived from the raw file: comment views and hidden marker ranges.

use crate::format::{CommentStatus, ParsedComment, parse_comments};
use crate::protocol::{CommentView, EntryView};
use crate::utf16::Utf16Index;
use std::collections::HashMap;
use std::ops::Range;

/// Comment views for `raw`. `previous` = last Done snapshot (round > 1) for new-thread/new-entry
/// flags.
pub fn comment_views(raw: &str, previous: Option<&str>) -> Vec<CommentView> {
    let prev: Option<HashMap<String, usize>> = previous.map(|p| {
        parse_comments(p)
            .into_iter()
            .map(|c| (c.id, c.data.thread.len()))
            .collect()
    });
    parse_comments(raw)
        .into_iter()
        .map(|c| view(c, prev.as_ref()))
        .collect()
}

fn view(c: ParsedComment, prev: Option<&HashMap<String, usize>>) -> CommentView {
    let orphaned = c.is_orphan();
    let n = c.data.thread.len();
    let (new_thread, new_entries) = match prev {
        None => (false, 0),
        Some(p) => match p.get(&c.id) {
            None => (true, n),
            Some(&old) => (false, n.saturating_sub(old)),
        },
    };
    let d = c.data;
    CommentView {
        id: c.id,
        status: match d.status {
            CommentStatus::Open => "open",
            CommentStatus::Resolved => "resolved",
        }
        .into(),
        quote: d.quote,
        author: d.author,
        created_at: d.created_at,
        orphaned,
        code_lines: d.code_lines.map(|r| (r.start, r.end)),
        entries: d
            .thread
            .into_iter()
            .map(|e| EntryView {
                author: e.author,
                timestamp: e.timestamp,
                text: e.text,
            })
            .collect(),
        new_entries,
        new_thread,
    }
}

/// UTF-16 ranges of all markers and body blocks in `raw`, merged and sorted. A marker/body that
/// occupies whole lines also covers one adjacent newline (the preceding one if any), so hiding
/// the range removes the whole line(s) in CodeMirror.
pub fn marker_ranges(raw: &str) -> Vec<(usize, usize)> {
    let b = raw.as_bytes();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for c in parse_comments(raw) {
        for r in [c.open, c.close, c.body].into_iter().flatten() {
            ranges.push(r);
        }
    }
    ranges.sort_by_key(|r| r.start);
    // Merge first so that consecutive markers on one line count as one "whole line" run.
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(l) if r.start <= l.end => l.end = l.end.max(r.end),
            _ => merged.push(r),
        }
    }
    let mut out: Vec<Range<usize>> = Vec::new();
    for mut r in merged {
        let line_start = r.start == 0 || b[r.start - 1] == b'\n';
        let line_end = r.end == b.len() || b[r.end] == b'\n' || raw[r.end..].starts_with("\r\n");
        if line_start && line_end {
            if r.start > 0 {
                r.start -= 1;
                if r.start > 0 && b[r.start - 1] == b'\r' {
                    r.start -= 1;
                }
            } else if r.end < b.len() {
                r.end += if b[r.end] == b'\r' { 2 } else { 1 };
            }
        }
        match out.last_mut() {
            Some(l) if r.start <= l.end => l.end = l.end.max(r.end),
            _ => out.push(r),
        }
    }
    let idx = Utf16Index::new(raw);
    out.into_iter()
        .map(|r| (idx.from_byte(r.start), idx.from_byte(r.end)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(raw: &str) -> String {
        let u: Vec<u16> = raw.encode_utf16().collect();
        let mut out = Vec::new();
        let mut last = 0;
        for (a, b) in marker_ranges(raw) {
            out.extend_from_slice(&u[last..a]);
            last = b;
        }
        out.extend_from_slice(&u[last..]);
        String::from_utf16(&out).unwrap()
    }

    #[test]
    fn ranges_hide_markers_and_body_lines() {
        let raw =
            "🎉 é <!--c:a1-->x<!--/c:a1--> y\n<!--co:a1 by:me status:open\nme: hi\n-->\n\nnext\n";
        let r = marker_ranges(raw);
        // "🎉 é " = 2 + 1 + 1 + 1 = 5 UTF-16 units.
        assert_eq!(r[0], (5, 5 + "<!--c:a1-->".len()));
        assert_eq!(hidden(raw), "🎉 é x y\n\nnext\n");
        let code = "t\n\n<!--c:p1-->\n```\nc\n```\n<!--/c:p1-->\n<!--co:p1 status:open line:0\nme: q\n-->\n\nz\n";
        assert_eq!(hidden(code), "t\n\n```\nc\n```\n\nz\n");
        // Marker at the very start of the document takes the following newline.
        assert_eq!(
            hidden("<!--c:q-->\n```\nc\n```\n<!--/c:q-->\n"),
            "```\nc\n```\n"
        );
    }

    #[test]
    fn views_and_round_deltas() {
        let prev = "<!--c:a1-->x<!--/c:a1-->\n<!--co:a1 by:me status:open\nme: hi\n-->\n";
        let raw = "<!--c:a1-->x<!--/c:a1-->\n<!--co:a1 by:me status:resolved quote:\"x\"\nme: hi\nagent: done\n-->\n\nfoo\n<!--co:b2 by:agent status:open\nagent: orphan\n-->\n";
        let v = comment_views(raw, Some(prev));
        assert_eq!(v.len(), 2);
        assert_eq!(
            (
                v[0].status.as_str(),
                v[0].quote.as_deref(),
                v[0].new_entries,
                v[0].new_thread,
                v[0].orphaned
            ),
            ("resolved", Some("x"), 1, false, false)
        );
        assert_eq!(v[0].entries[1].author, "agent");
        assert!(v[1].orphaned && v[1].new_thread && v[1].new_entries == 1);
        let v = comment_views(raw, None);
        assert!(!v[1].new_thread && v[1].new_entries == 0);
    }
}
