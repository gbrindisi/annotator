use annotator::format::*;

const SAMPLE: &str = include_str!("../fixtures/sample.md");
const NOW: &str = "2026-09-24T14:30:00.000Z";

fn input(text: &str) -> NewCommentInput {
    NewCommentInput {
        id: Some("q4m2".into()),
        created_at: NOW.into(),
        author: "bomber".into(),
        text: text.into(),
        expected: None,
    }
}

fn add(doc: &str, from: usize, to: usize, text: &str) -> String {
    let (out, id) = add_comment(doc, from, to, &input(text)).unwrap();
    assert_eq!(id, "q4m2");
    out
}

fn get(doc: &str, id: &str) -> ParsedComment {
    parse_comments(doc)
        .into_iter()
        .find(|c| c.id == id)
        .unwrap()
}

#[test]
fn block_end_cases() {
    assert_eq!(
        block_end("line one\nline two\n\nother", 2),
        "line one\nline two".len()
    );
    assert_eq!(block_end("only", 1), 4);
    assert_eq!(block_end("a\nb\n", 0), 3);
}

#[test]
fn add_produces_exact_document() {
    let doc = "Our roadmap commits us to net zero by 2030 across all operations.\n\nNext.";
    let from = doc.find("net zero").unwrap();
    let to = from + "net zero by 2030".len();
    assert_eq!(
        add(doc, from, to, "this needs a source"),
        "Our roadmap commits us to <!--c:q4m2-->net zero by 2030<!--/c:q4m2--> across all operations.\n\
         <!--co:q4m2 by:bomber at:2026-09-24T14:30:00.000Z status:open quote:\"net zero by 2030\"\n\
         bomber (2026-09-24T14:30:00.000Z): this needs a source\n-->\n\nNext."
    );
}

#[test]
fn add_places_body_after_paragraph_and_on_headings() {
    let doc = "# Title\n\nfirst line\nsecond line\n\nend";
    assert!(add(doc, 2, 7, "x").starts_with("# <!--c:q4m2-->Title<!--/c:q4m2-->\n<!--co:q4m2"));
    let f = doc.find("first").unwrap();
    assert!(add(doc, f, f + 5, "x").contains("second line\n<!--co:q4m2"));
}

#[test]
fn trims_whitespace_and_expands_inline_code() {
    let doc = "call ` foo() ` now";
    let f = doc.find("foo").unwrap();
    assert_eq!(&doc[normalize_selection(doc, f, f + 3)], "` foo() `");
    assert_eq!(normalize_selection("a  word  b", 1, 8), 3..7);
    // Non-ASCII whitespace and text.
    let doc = "é\u{00A0}🎉 ok\u{3000}ü";
    let r = normalize_selection(doc, "é".len(), doc.len() - "ü".len());
    assert_eq!(&doc[r], "🎉 ok");
    // Mid-char offsets are snapped down.
    let r = normalize_selection("🎉🎉", 1, 6);
    assert_eq!(r, 0..4);
}

#[test]
fn refuses_invalid_selections() {
    assert!(
        validate_selection("abc", 1, 1)
            .unwrap()
            .contains("Select some text")
    );
    assert!(
        validate_selection("para one\n\npara two", 0, 15)
            .unwrap()
            .contains("single paragraph")
    );
    let fence = SAMPLE.find("validate(order)").unwrap();
    assert!(
        validate_selection(SAMPLE, fence, fence + 5)
            .unwrap()
            .contains("fenced code")
    );
    let marker = SAMPLE.find("<!--c:k3f9-->").unwrap();
    assert!(
        validate_selection(SAMPLE, marker + 3, marker + 20)
            .unwrap()
            .contains("cuts through")
    );
    let close = SAMPLE.find("<!--/c:k3f9-->").unwrap();
    assert_eq!(
        validate_selection(SAMPLE, marker - 3, close + "<!--/c:k3f9-->".len() + 3),
        None
    );
    assert_eq!(
        validate_selection(SAMPLE, marker + "<!--c:k3f9-->".len() + 1, close - 1),
        None
    );
    assert!(
        validate_selection(SAMPLE, marker - 3, close - 2)
            .unwrap()
            .contains("partially overlaps")
    );
    assert!(
        validate_selection(SAMPLE, marker + 20, close + 20)
            .unwrap()
            .contains("partially overlaps")
    );
}

#[test]
fn strips_nested_markers_from_quote() {
    let doc = "We should <!--c:k3f9-->ship on Friday<!--/c:k3f9--> now.\n<!--co:k3f9 status:open\nkyle: hi\n-->";
    let out = add(doc, 3, doc.find(" now.").unwrap(), "outer");
    assert_eq!(
        get(&out, "q4m2").data.quote.as_deref(),
        Some("should ship on Friday")
    );
}

#[test]
fn refuses_when_selection_moved() {
    let mut i = input("x");
    i.expected = Some("other".into());
    assert_eq!(
        add_comment("hello world", 0, 5, &i),
        Err(EditError::SelectionMoved)
    );
}

#[test]
fn refuses_empty_text() {
    assert_eq!(
        add_comment("hello world", 0, 5, &input("  ")),
        Err(EditError::EmptyText)
    );
}

#[test]
fn add_generates_id_when_missing() {
    let mut i = input("x");
    i.id = None;
    let (out, id) = add_comment(SAMPLE, 0, 3, &i).unwrap();
    assert_eq!(id.len(), 5);
    assert!(get(&out, &id).is_anchored());
}

#[test]
fn add_on_non_ascii_text() {
    let doc = "Привет, мир 🌍! Ça marche.\n\nfin";
    let f = doc.find("мир").unwrap();
    let out = add(doc, f, f + "мир 🌍".len(), "ok");
    let c = get(&out, "q4m2");
    assert_eq!(&out[c.anchor_range().unwrap()], "мир 🌍");
    assert_eq!(c.data.quote.as_deref(), Some("мир 🌍"));
    assert_eq!(delete_comment(&out, "q4m2").unwrap(), doc);
}

#[test]
fn appends_reply_preserving_reactions_and_header() {
    let out = append_reply(SAMPLE, "k3f9", NOW, "bomber", "agreed").unwrap();
    let c = get(&out, "k3f9");
    assert_eq!(c.data.thread.len(), 3);
    assert_eq!(
        c.data.thread[2],
        ThreadEntry {
            author: "bomber".into(),
            timestamp: Some(NOW.into()),
            text: "agreed".into()
        }
    );
    assert_eq!(
        c.data.reactions,
        [Reaction {
            emoji: "👍".into(),
            authors: vec!["sam".into(), "mike".into()],
            entry: None
        }]
    );
    assert!(out.contains(
        "<!--co:k3f9 by:kyle at:2026-06-17T10:00:00.000Z status:open quote:\"ship on Friday\"\n"
    ));
}

#[test]
fn resolve_and_reopen() {
    let out = set_resolved(SAMPLE, "k3f9", true).unwrap();
    assert_eq!(get(&out, "k3f9").data.status, CommentStatus::Resolved);
    assert_eq!(set_resolved(&out, "k3f9", false).unwrap(), SAMPLE);
    // Code comment keeps its line anchor.
    let out = set_resolved(SAMPLE, "p7k2", true).unwrap();
    assert!(out.contains("status:resolved quote:\"    validate(order)\" line:1\n"));
}

#[test]
fn edits_an_entry() {
    let out = edit_entry(SAMPLE, "a7b2", 0, "changed").unwrap();
    assert_eq!(get(&out, "a7b2").data.thread[0].text, "changed");
    assert_eq!(
        edit_entry(SAMPLE, "a7b2", 5, "x"),
        Err(EditError::NoSuchEntry)
    );
    assert_eq!(edit_entry(SAMPLE, "nope", 0, "x"), Err(EditError::NotFound));
}

#[test]
fn deletes_entry_and_reindexes_reactions() {
    let doc = "<!--c:r1-->x<!--/c:r1-->\n<!--co:r1 status:open\na: one\nb: two\nc: three\n+👍 a\n+@1 🎉 b\n+@2 👀 c\n-->";
    let out = delete_entry(doc, "r1", 1).unwrap();
    let c = &parse_comments(&out)[0];
    let texts: Vec<_> = c.data.thread.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, ["one", "three"]);
    assert_eq!(
        c.data.reactions,
        [
            Reaction {
                emoji: "👍".into(),
                authors: vec!["a".into()],
                entry: None
            },
            Reaction {
                emoji: "👀".into(),
                authors: vec!["c".into()],
                entry: Some(1)
            },
        ]
    );
}

#[test]
fn deleting_only_entry_deletes_thread() {
    let out = delete_entry(SAMPLE, "a7b2", 0).unwrap();
    assert!(!parse_comments(&out).iter().any(|c| c.id == "a7b2"));
    assert!(out.contains("paragraph with another highlighted span placed"));
}

#[test]
fn deletes_thread_leaving_text_and_no_blank_line() {
    let out = delete_comment(SAMPLE, "k3f9").unwrap();
    assert!(out.contains("We should ship on Friday regardless"));
    assert!(out.contains("closes Monday.\n\nHere is a second"));
    assert!(!out.contains("k3f9"));
}

#[test]
fn deletes_code_comment_with_own_line_markers() {
    let out = delete_comment(SAMPLE, "p7k2").unwrap();
    assert!(out.contains(
        "## Code\n\n```python\ndef process(order):\n    validate(order)\n```\n\nThis body"
    ));
}

#[test]
fn add_then_delete_is_noop() {
    let doc = "hello brave world";
    let added = add(doc, 6, 11, "x");
    let c = &parse_comments(&added)[0];
    assert_eq!(&added[c.anchor_range().unwrap()], "brave");
    assert_eq!(delete_comment(&added, "q4m2").unwrap(), doc);
}

#[test]
fn comment_at_prefers_innermost() {
    let doc = "<!--c:aaa-->the <!--c:bbb-->quick<!--/c:bbb--> fox<!--/c:aaa-->";
    let cs = parse_comments(doc);
    assert_eq!(
        comment_at(&cs, doc.find("quick").unwrap()).unwrap().id,
        "bbb"
    );
    assert_eq!(comment_at(&cs, doc.find("fox").unwrap()).unwrap().id, "aaa");
    assert!(comment_at(&cs, doc.len()).is_none());
}
