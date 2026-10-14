//! Edit mode on the real fixtures: editing one block leaves every other byte identical and
//! keeps every comment that was not inside the edited block.

use annotator::blockedit::{block_range, editable_blocks, merge_blocks, replace_block};
use annotator::format::parse_comments;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn anchored(raw: &str) -> Vec<String> {
    parse_comments(raw)
        .into_iter()
        .filter(|c| c.is_anchored())
        .map(|c| c.id)
        .collect()
}

/// Replace each editable block in turn and check that bytes outside the block are unchanged.
fn check_every_block(raw: &str) {
    let blocks = editable_blocks(raw);
    assert!(blocks.len() > 5, "{blocks:?}");
    let before = anchored(raw);
    for sp in &blocks {
        let r = block_range(raw, sp).unwrap_or_else(|e| panic!("{sp}: {e}"));
        // Keep the comments of the block: re-emit its markers around the new text.
        let inside: Vec<String> = parse_comments(&raw[r.clone()])
            .into_iter()
            .filter(|c| c.is_anchored())
            .map(|c| c.id)
            .collect();
        let mut md = String::from("Edited \u{e9} *text*");
        for id in &inside {
            md = format!("<!--c:{id}-->{md}<!--/c:{id}-->");
        }
        let out = replace_block(raw, sp, &md).unwrap_or_else(|e| panic!("{sp}: {e}"));
        assert_eq!(&out[..r.start], &raw[..r.start], "{sp}: prefix changed");
        let tail = raw.len() - r.end;
        assert_eq!(
            &out[out.len() - tail..],
            &raw[r.end..],
            "{sp}: suffix changed"
        );
        assert_eq!(&out[r.start..out.len() - tail], md, "{sp}");
        assert_eq!(anchored(&out), before, "{sp}: comments changed");
        // No-op edits are byte-identical.
        let same = replace_block(raw, sp, &raw[r.clone()]).unwrap();
        assert_eq!(same, raw, "{sp}: identity edit changed the file");
    }
}

#[test]
fn sample_every_block_byte_identity() {
    check_every_block(&fixture("sample.md"));
}

#[test]
fn mermaid_every_block_byte_identity() {
    check_every_block(&fixture("mermaid.md"));
}

#[test]
fn sample_split_and_merge_roundtrip() {
    let raw = fixture("sample.md");
    let blocks = editable_blocks(&raw);
    // "We should <!--c:k3f9-->ship on Friday<!--/c:k3f9--> regardless..." (block 2).
    let sp = &blocks[2];
    let r = block_range(&raw, sp).unwrap();
    let content = raw[r.clone()].to_string();
    assert!(content.starts_with("We should <!--c:k3f9-->"), "{content}");
    let split = replace_block(&raw, sp, &format!("{content}\n\nA brand new paragraph.")).unwrap();
    // The new paragraph lands after the comment body, before the next block.
    assert!(split.contains("+👍 sam, mike\n-->\n\nA brand new paragraph.\n\nHere is a second"));
    assert_eq!(anchored(&split), anchored(&raw));
    let b = editable_blocks(&split);
    assert_eq!(b.len(), blocks.len() + 1);
    let merged = merge_blocks(&split, &b[2], &b[3], &content).unwrap();
    assert_eq!(merged, raw);
}
