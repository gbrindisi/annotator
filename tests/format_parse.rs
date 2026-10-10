use annotator::format::*;

const SAMPLE: &str = include_str!("../fixtures/sample.md");

fn canonical() -> String {
    [
        format!(
            "We should {}ship on Friday{} regardless of the QA timeline.",
            open_marker("k3f9"),
            close_marker("k3f9")
        ),
        "<!--co:k3f9 by:kyle at:2026-06-17T10:00 status:open quote:\"ship on Friday\"".into(),
        "kyle: I thought we agreed Thursday?".into(),
        "sam (2026-06-17T11:00): Thursday is better for QA.".into(),
        "-->".into(),
        "".into(),
        "Next paragraph.".into(),
    ]
    .join("\n")
}

fn get<'a>(cs: &'a [ParsedComment], id: &str) -> &'a ParsedComment {
    cs.iter().find(|c| c.id == id).unwrap()
}

#[test]
fn parses_the_canonical_example() {
    let doc = canonical();
    let cs = parse_comments(&doc);
    assert_eq!(cs.len(), 1);
    let c = &cs[0];
    assert_eq!(c.id, "k3f9");
    assert_eq!(c.data.author.as_deref(), Some("kyle"));
    assert_eq!(c.data.created_at.as_deref(), Some("2026-06-17T10:00"));
    assert_eq!(c.data.status, CommentStatus::Open);
    assert_eq!(c.data.quote.as_deref(), Some("ship on Friday"));
    assert_eq!(c.data.thread.len(), 2);
    assert_eq!(c.data.thread[0].author, "kyle");
    assert_eq!(c.data.thread[0].text, "I thought we agreed Thursday?");
    assert_eq!(c.data.thread[1].author, "sam");
    assert_eq!(
        c.data.thread[1].timestamp.as_deref(),
        Some("2026-06-17T11:00")
    );
    assert_eq!(c.data.thread[1].text, "Thursday is better for QA.");
    assert_eq!(&doc[c.anchor_range().unwrap()], "ship on Friday");
}

#[test]
fn parses_the_fixture_in_order_with_states() {
    let cs = parse_comments(SAMPLE);
    let ids: Vec<_> = cs.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["k3f9", "a7b2", "zz1q", "p7k2", "orph1"]);
    assert_eq!(
        get(&cs, "k3f9").data.reactions,
        [Reaction {
            emoji: "👍".into(),
            authors: vec!["sam".into(), "mike".into()],
            entry: None
        }]
    );
    assert_eq!(get(&cs, "zz1q").data.status, CommentStatus::Resolved);
    let p = get(&cs, "p7k2");
    assert!(p.is_code_comment());
    assert_eq!(p.data.code_lines, Some(1..1));
    assert_eq!(p.data.quote.as_deref(), Some("    validate(order)"));
    assert!(get(&cs, "orph1").is_orphan());
    assert!(!get(&cs, "orph1").is_anchored());
}

#[test]
fn keeps_first_occurrence_of_duplicate_id() {
    let (o, c) = (open_marker("dup"), close_marker("dup"));
    let doc = format!(
        "{o}one{c} {o}two{c}\n<!--co:dup status:open\nme: a\n-->\n<!--co:dup status:open\nme: b\n-->"
    );
    let cm = &parse_comments(&doc)[0];
    assert_eq!(&doc[cm.anchor_range().unwrap()], "one");
    assert_eq!(cm.data.thread[0].text, "a");
}

#[test]
fn rejects_ids_with_punctuation() {
    assert!(parse_comments("<!--c:my-id-->x<!--/c:my-id-->").is_empty());
}

#[test]
fn close_before_open_is_unanchored() {
    let doc = format!(
        "{}x{}\n<!--co:ab1 status:open\nme: hi\n-->",
        close_marker("ab1"),
        open_marker("ab1")
    );
    let c = &parse_comments(&doc)[0];
    assert!(!c.is_anchored());
    assert!(c.is_orphan());
}

#[test]
fn handles_nested_comments() {
    let doc = format!(
        "{}the {}quick brown{} fox{}\n<!--co:aaa status:open\nme: outer\n-->\n<!--co:bbb status:open\nme: inner\n-->",
        open_marker("aaa"),
        open_marker("bbb"),
        close_marker("bbb"),
        close_marker("aaa")
    );
    let cs = parse_comments(&doc);
    assert_eq!(&doc[get(&cs, "bbb").anchor_range().unwrap()], "quick brown");
}

#[test]
fn ignores_markers_in_fenced_and_inline_code() {
    assert!(!parse_comments(SAMPLE).iter().any(|c| c.id == "ex01"));
    assert!(parse_comments(&format!("Use the `{}` marker.", open_marker("inl01"))).is_empty());
}

#[test]
fn handles_crlf_and_escaped_newlines() {
    let doc = format!(
        "{}x{}\r\n<!--co:cr1 status:open\r\nme: line one\\nline two\r\n-->",
        open_marker("cr1"),
        close_marker("cr1")
    );
    assert_eq!(
        parse_comments(&doc)[0].data.thread[0].text,
        "line one\nline two"
    );
}

#[test]
fn collects_existing_ids() {
    assert_eq!(existing_ids(&canonical()), ["k3f9"]);
}

#[test]
fn non_ascii_offsets_are_bytes() {
    let doc = "Ça 🎉 <!--c:u1-->naïve 🐙<!--/c:u1--> fin\n<!--co:u1 by:zoë status:open quote:\"naïve 🐙\"\nzoë (t): très “bien” 🎉: ok\n+@0 🐙 a\\, b, ç\n-->";
    let c = &parse_comments(doc)[0];
    assert_eq!(&doc[c.anchor_range().unwrap()], "naïve 🐙");
    assert_eq!(c.data.author.as_deref(), Some("zoë"));
    assert_eq!(c.data.thread[0].text, "très “bien” 🎉: ok");
    assert_eq!(c.data.reactions[0].authors, ["a, b", "ç"]);
    assert_eq!(
        &doc[c.body.clone().unwrap()],
        &doc[doc.find("<!--co").unwrap()..]
    );
}

#[test]
fn serializes_canonical_header_order() {
    let data = CommentData {
        author: Some("Gianluca Brindisi".into()),
        created_at: Some("2026-09-24T14:30:00.000Z".into()),
        quote: Some("net  zero\nby \"2030\"".into()),
        thread: vec![ThreadEntry {
            author: "gianluca".into(),
            timestamp: Some("2026-09-24T14:30:00.000Z".into()),
            text: "needs a source".into(),
        }],
        ..Default::default()
    };
    assert_eq!(
        serialize_body("q4m2", &data),
        "<!--co:q4m2 by:Gianluca_Brindisi at:2026-09-24T14:30:00.000Z status:open quote:\"net zero by '2030'\"\ngianluca (2026-09-24T14:30:00.000Z): needs a source\n-->"
    );
}

#[test]
fn serializes_empty_thread() {
    let data = CommentData {
        quote: Some("text".into()),
        ..Default::default()
    };
    assert_eq!(
        serialize_body("h1", &data),
        "<!--co:h1 status:open quote:\"text\"\n-->"
    );
}

#[test]
fn breaks_terminator_and_escapes_newlines() {
    let data = CommentData {
        thread: vec![ThreadEntry {
            author: "me".into(),
            timestamp: None,
            text: "a --> b\nc".into(),
        }],
        ..Default::default()
    };
    let body = serialize_body("t1", &data);
    assert_eq!(body, "<!--co:t1 status:open\nme: a --\u{200B}> b\\nc\n-->");
    let doc = format!("{}x{}\n{body}", open_marker("t1"), close_marker("t1"));
    assert_eq!(
        parse_comments(&doc)[0].data.thread[0].text,
        "a --\u{200B}> b\nc"
    );
}

#[test]
fn round_trips_every_body_in_fixture() {
    for c in parse_comments(SAMPLE) {
        let Some(body) = c.body.clone() else { continue };
        assert_eq!(serialize_body(&c.id, &c.data), &SAMPLE[body]);
    }
}

#[test]
fn rewriting_every_thread_leaves_document_identical() {
    let mut doc = SAMPLE.to_string();
    for c in parse_comments(SAMPLE) {
        if c.body.is_some() {
            doc = replace_comment_data(&doc, &c.id, &c.data).unwrap();
        }
    }
    assert_eq!(doc, SAMPLE);
}

#[test]
fn code_quote_round_trips() {
    let data = CommentData {
        quote: Some("x = \"é\" --> \\n\n  🎉".into()),
        code_lines: Some(2..4),
        ..Default::default()
    };
    let body = serialize_body("cq1", &data);
    assert!(body.contains(" line:2-4"));
    let parsed = &parse_comments(&body)[0];
    assert_eq!(parsed.data, data);
}

#[test]
fn unknown_header_fields_survive_edits() {
    let doc = "<!--c:u1-->x<!--/c:u1-->\n<!--co:u1 by:me status:open quote:\"x\" line:3 prio:high note:\"a b\"\nme: hi\n-->";
    let c = &parse_comments(doc)[0];
    assert_eq!(c.data.extra_header, ["prio:high", "note:\"a b\""]);
    let out = set_resolved(doc, "u1", true).unwrap();
    assert!(out.contains("status:resolved quote:\"x\" line:3 prio:high note:\"a b\"\n"));
    assert_eq!(set_resolved(&out, "u1", false).unwrap(), doc);
}

#[test]
fn generates_ids() {
    let existing = ["abcde"];
    for _ in 0..100 {
        let id = new_id(&existing);
        assert_eq!(id.len(), 5);
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        );
        assert_ne!(id, "abcde");
    }
    assert_ne!(new_id(&[]), new_id(&[]));
}

#[test]
fn malformed_input_does_not_panic() {
    let cases = [
        "<!--c:a1-->unclosed",
        "<!--/c:a1-->",
        "<!--co:a1 status:open\nme: never closed",
        "<!--co:a1-->",
        "<!--co:a1 ::: \"\" line:9-2 line:x quote:\"unterminated\n\n-->",
        "<!--co:a1 status:open\n: no author\n+\n+@99999999999999999999999 x y\n(ts): z\n\\\n-->",
        "```\n<!--c:a1-->",
        "`<!--c:a1-->",
        "<!--c:-->x<!--/c:-->",
        "🎉<!--co:é status:open\n-->",
        "",
    ];
    for doc in cases {
        let cs = parse_comments(doc);
        for c in &cs {
            let _ = serialize_body(&c.id, &c.data);
            let _ = append_reply(doc, &c.id, "t", "me", "hi");
            let _ = set_resolved(doc, &c.id, true);
            let _ = edit_entry(doc, &c.id, 0, "x");
            let _ = delete_entry(doc, &c.id, 0);
            let _ = delete_comment(doc, &c.id);
        }
        for i in 0..=doc.len() + 2 {
            let _ = normalize_selection(doc, i, doc.len() + 3);
            let _ = validate_selection(doc, i, doc.len());
            let _ = block_end(doc, i);
            let _ = add_comment(
                doc,
                0,
                i,
                &NewCommentInput {
                    id: None,
                    created_at: "t".into(),
                    author: "a".into(),
                    text: "x".into(),
                    expected: None,
                },
            );
        }
        let _ = locate_text(doc, "c");
    }
    let c = &parse_comments("<!--co:a1 status:open\nme: never closed")[..];
    assert!(c.is_empty());
    let c = &parse_comments("<!--co:a1 line:9-2\n-->")[0];
    assert_eq!(c.data.code_lines, None);
}
