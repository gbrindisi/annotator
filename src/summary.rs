//! Round summary: what the reviewer changed. Comment-aware fields are filled later.

use crate::protocol::Summary;
use similar::{ChangeTag, TextDiff};

pub fn compute(path: &str, round: u32, before: &str, after: &str) -> Summary {
    let diff = TextDiff::from_lines(before, after);
    let mut added = 0;
    let mut removed = 0;
    for c in diff.iter_all_changes() {
        match c.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    Summary {
        path: path.to_string(),
        round,
        edit_hunks: diff.grouped_ops(0).len() as u32,
        lines_added: added,
        lines_removed: removed,
        ..Default::default()
    }
}

pub fn to_text(s: &Summary) -> String {
    let edits = if s.edit_hunks == 0 {
        "no edits".to_string()
    } else {
        format!(
            "edits: {} hunk{}, +{} -{} lines",
            s.edit_hunks,
            if s.edit_hunks == 1 { "" } else { "s" },
            s.lines_added,
            s.lines_removed
        )
    };
    format!(
        "annotator: review of {} finished (round {})\n- new comments: {}\n- replies on existing comments: {}\n- resolved: {}, deleted: {}\n- {}\n",
        s.path,
        s.round,
        s.new_threads.len(),
        s.replied_threads.len(),
        s.resolved_threads.len(),
        s.deleted_threads.len(),
        edits
    )
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
        assert!(to_text(&s).contains("- no edits"));
    }
}
