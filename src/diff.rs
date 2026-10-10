//! Block-level changes between rounds, computed on clean text (comment markers excluded).

use crate::protocol::Change;
use crate::render::{line_starts, options, sourcepos_range, strip};
use comrak::{Arena, parse_document};
use similar::{Algorithm, DiffOp, capture_diff_slices};

/// A top-level block: its clean source text and its `data-sourcepos` string.
struct Block {
    text: String,
    sourcepos: String,
}

fn blocks(clean: &str) -> Vec<Block> {
    let arena = Arena::new();
    let root = parse_document(&arena, clean, &options());
    let starts = line_starts(clean);
    root.children()
        .map(|n| {
            let sp = n.data.borrow().sourcepos;
            let r = sourcepos_range(&sp, &starts, clean.len());
            Block {
                text: clean[r].trim_end().to_string(),
                sourcepos: sp.to_string(),
            }
        })
        .collect()
}

/// Changes from `old_raw` (last Done snapshot) to `new_raw` (current file).
pub fn changes(old_raw: &str, new_raw: &str) -> Vec<Change> {
    changes_clean(&strip(old_raw).0, &strip(new_raw).0)
}

/// Same as [`changes`], on already-stripped text (`render(..).clean`).
pub fn changes_clean(old_clean: &str, new_clean: &str) -> Vec<Change> {
    let old = blocks(old_clean);
    let new = blocks(new_clean);
    let ok: Vec<&str> = old.iter().map(|b| b.text.as_str()).collect();
    let nk: Vec<&str> = new.iter().map(|b| b.text.as_str()).collect();
    let mut out = Vec::new();
    let after = |ni: usize| (ni > 0).then(|| new[ni - 1].sourcepos.clone());
    for op in capture_diff_slices(Algorithm::Myers, &ok, &nk) {
        let (oi, ol, ni, nl) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_len, new_index, 0),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, 0, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, old_len, new_index, new_len),
        };
        // `capture_diff_slices` merges delete+insert at one spot into Replace: pair those up
        // as modifications, the excess is added/removed.
        let paired = ol.min(nl);
        for k in 0..paired {
            out.push(Change {
                kind: "modified".into(),
                sourcepos: Some(new[ni + k].sourcepos.clone()),
                old_text: Some(old[oi + k].text.clone()),
                after: None,
            });
        }
        for k in paired..nl {
            out.push(Change {
                kind: "added".into(),
                sourcepos: Some(new[ni + k].sourcepos.clone()),
                old_text: None,
                after: None,
            });
        }
        for k in paired..ol {
            out.push(Change {
                kind: "removed".into(),
                sourcepos: None,
                old_text: Some(old[oi + k].text.clone()),
                after: after(ni + paired),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render;

    #[test]
    fn added_modified_removed() {
        let old = "# T\n\npara one\n\npara two\n\npara three\n";
        let new = "# T\n\npara one!\n\npara three\n\nnew para\n";
        let ch = changes(old, new);
        let html = render(new).html;
        assert_eq!(ch.len(), 3, "{ch:?}");
        assert_eq!(ch[0].kind, "modified");
        assert_eq!(ch[0].old_text.as_deref(), Some("para one"));
        assert!(html.contains(&format!(
            "data-sourcepos=\"{}\"",
            ch[0].sourcepos.as_ref().unwrap()
        )));
        // "para two" removed after the modified block.
        assert_eq!(ch[1].kind, "removed");
        assert_eq!(ch[1].old_text.as_deref(), Some("para two"));
        assert_eq!(ch[1].after, ch[0].sourcepos);
        assert_eq!(ch[2].kind, "added");
        assert_eq!(ch[2].sourcepos.as_deref(), Some("7:1-7:8"));
        assert!(html.contains("data-sourcepos=\"7:1-7:8\""));
    }

    #[test]
    fn markers_alone_are_not_changes() {
        let old = "a b c\n\nnext\n";
        let new =
            "a <!--c:x1-->b<!--/c:x1--> c\n<!--co:x1 by:me status:open\nme: hi\n-->\n\nnext\n";
        assert!(changes(old, new).is_empty());
        let removed_first = changes("gone\n\nkeep\n", "keep\n");
        assert_eq!(removed_first.len(), 1);
        assert_eq!(removed_first[0].after, None);
    }
}
