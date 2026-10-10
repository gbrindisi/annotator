//! Round summary: what the reviewer did between the round snapshot and Done.

use crate::format::{CommentStatus, parse_comments};
use crate::protocol::Summary;
use crate::render::strip;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

/// `before` = `round-N.md` snapshot, `after` = file content at Done.
pub fn compute(path: &str, round: u32, before: &str, after: &str) -> Summary {
    let mut s = Summary {
        path: path.to_string(),
        round,
        ..Default::default()
    };
    let old: HashMap<String, _> = parse_comments(before)
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();
    let new = parse_comments(after);
    for c in &new {
        match old.get(&c.id) {
            None => s.new_threads.push(c.id.clone()),
            Some(o) => {
                if c.data.thread.len() > o.data.thread.len() {
                    s.replied_threads.push(c.id.clone());
                }
                if o.data.status == CommentStatus::Open && c.data.status == CommentStatus::Resolved
                {
                    s.resolved_threads.push(c.id.clone());
                }
            }
        }
    }
    let mut deleted: Vec<String> = old
        .keys()
        .filter(|id| !new.iter().any(|c| &c.id == *id))
        .cloned()
        .collect();
    deleted.sort();
    s.deleted_threads = deleted;

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
    s
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn ids(v: &[String]) -> String {
    if v.is_empty() {
        "none".into()
    } else {
        v.join(", ")
    }
}

pub fn to_text(s: &Summary) -> String {
    let mut out = format!(
        "annotator: review of {} finished (round {})\n",
        s.path, s.round
    );
    let nothing = s.new_threads.is_empty()
        && s.replied_threads.is_empty()
        && s.resolved_threads.is_empty()
        && s.deleted_threads.is_empty()
        && s.edit_hunks == 0;
    if nothing {
        out.push_str("- no changes\n");
        return out;
    }
    if !s.new_threads.is_empty() {
        out.push_str(&format!(
            "- {}: {}\n",
            plural(s.new_threads.len(), "new comment", "new comments"),
            ids(&s.new_threads)
        ));
    }
    if !s.replied_threads.is_empty() {
        out.push_str(&format!(
            "- {}: {}\n",
            plural(
                s.replied_threads.len(),
                "reply on an existing comment",
                "replies on existing comments"
            ),
            ids(&s.replied_threads)
        ));
    }
    out.push_str(&format!(
        "- resolved: {}; deleted: {}\n",
        ids(&s.resolved_threads),
        ids(&s.deleted_threads)
    ));
    if s.edit_hunks == 0 {
        out.push_str("- no edits\n");
    } else {
        out.push_str(&format!(
            "- edits: {}, +{} -{} lines\n",
            plural(s.edit_hunks as usize, "hunk", "hunks"),
            s.lines_added,
            s.lines_removed
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats() {
        let s = compute("/p.md", 1, "a\nb\nc\nd\ne\nf\n", "a\nB\nc\nd\ne\nf\ng\nh\n");
        assert_eq!((s.edit_hunks, s.lines_added, s.lines_removed), (2, 3, 1));
        assert!(to_text(&s).contains("- edits: 2 hunks, +3 -1 lines"));
        let s = compute("/p.md", 1, "x\n", "x\n");
        assert_eq!(
            to_text(&s),
            "annotator: review of /p.md finished (round 1)\n- no changes\n"
        );
    }

    #[test]
    fn comments_and_edits() {
        let before = "intro <!--c:p7k2-->text<!--/c:p7k2-->\n<!--co:p7k2 by:agent status:open\nagent: q?\n-->\n\n<!--c:r1-->old<!--/c:r1-->\n<!--co:r1 by:a status:open\na: x\n-->\n\n<!--c:d1-->gone<!--/c:d1-->\n<!--co:d1 by:a status:open\na: x\n-->\n\nend\n";
        let after = "intro <!--c:p7k2-->text<!--/c:p7k2-->\n<!--co:p7k2 by:agent status:open\nagent: q?\nme: yes\n-->\n\n<!--c:r1-->old<!--/c:r1-->\n<!--co:r1 by:a status:resolved\na: x\n-->\n\ngone\n\n<!--c:n1-->end<!--/c:n1-->\n<!--co:n1 by:me status:open\nme: new\n-->\nadded line\n";
        let s = compute("/abs/plan.md", 2, before, after);
        assert_eq!(s.new_threads, ["n1"]);
        assert_eq!(s.replied_threads, ["p7k2"]);
        assert_eq!(s.resolved_threads, ["r1"]);
        assert_eq!(s.deleted_threads, ["d1"]);
        // Only the added text line counts (markers/bodies excluded).
        assert_eq!((s.edit_hunks, s.lines_added, s.lines_removed), (1, 1, 0));
        assert_eq!(
            to_text(&s),
            "annotator: review of /abs/plan.md finished (round 2)\n\
             - 1 new comment: n1\n\
             - 1 reply on an existing comment: p7k2\n\
             - resolved: r1; deleted: d1\n\
             - edits: 1 hunk, +1 -0 lines\n"
        );
    }
}
