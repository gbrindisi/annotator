//! `summary::compute`: diff and per-thread comment activity.

use annotator::protocol::{CommentActivity, CommentKind, Summary};
use annotator::summary::compute;
use pretty_assertions::assert_eq;

const P: &str = "/abs/plan.md";

fn run(before: &str, after: &str) -> Summary {
    compute(P, 2, before, after)
}

fn thread(id: &str, status: &str, entries: &[&str]) -> String {
    format!(
        "<!--co:{id} by:a at:2026-01-01T00:00:00.000Z status:{status}\n{}\n-->",
        entries.join("\n")
    )
}

fn by_id<'a>(s: &'a Summary, id: &str) -> &'a CommentActivity {
    s.comments.iter().find(|c| c.id == id).unwrap()
}

#[test]
fn edits_only() {
    let s = run(
        "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n",
        "a\nb\nc\nD\ne\nf\ng\nh\ni\nj\n",
    );
    assert!(s.comments.is_empty());
    assert_eq!((s.edit_hunks, s.lines_added, s.lines_removed), (1, 1, 1));
    assert_eq!(
        s.diff,
        "--- plan.md (round 2 start)\n+++ plan.md (after review)\n@@ -1,7 +1,7 @@\n a\n b\n c\n-d\n+D\n e\n f\n g\n"
    );
}

#[test]
fn no_changes_has_empty_diff_and_no_comments() {
    let s = run("x\n", "x\n");
    assert_eq!(s.diff, "");
    assert!(s.comments.is_empty());
    assert_eq!(s.edit_hunks, 0);
}

#[test]
fn comments_only_leave_diff_empty() {
    let before = "Intro line\n";
    let after = format!(
        "Intro <!--c:n1-->line<!--/c:n1-->\n{}\n",
        thread("n1", "open", &["me (2026-01-02T00:00:00.000Z): hi"])
    );
    let s = run(before, &after);
    assert_eq!(s.diff, "");
    assert_eq!(s.edit_hunks, 0);
    assert_eq!(s.new_threads, ["n1"]);
    assert_eq!(s.comments.len(), 1);
}

#[test]
fn new_reply_resolve_reopen_delete_edit() {
    let before = format!(
        "<!--c:rp-->reply me<!--/c:rp-->\n{}\n\n<!--c:rs-->resolve me<!--/c:rs-->\n{}\n\n\
         <!--c:ro-->reopen me<!--/c:ro-->\n{}\n\n<!--c:de-->delete me<!--/c:de-->\n{}\n\n\
         <!--c:ed-->edit me<!--/c:ed-->\n{}\n\n<!--c:un-->untouched<!--/c:un-->\n{}\n\nend\n",
        thread("rp", "open", &["agent: q?"]),
        thread("rs", "open", &["agent: q?"]),
        thread("ro", "resolved", &["agent: q?"]),
        thread("de", "open", &["agent: first", "me: second"]),
        thread("ed", "open", &["me: typo"]),
        thread("un", "open", &["me: same"]),
    );
    let after = format!(
        "<!--c:rp-->reply me<!--/c:rp-->\n{}\n\n<!--c:rs-->resolve me<!--/c:rs-->\n{}\n\n\
         <!--c:ro-->reopen me<!--/c:ro-->\n{}\n\ndelete me\n\n<!--c:ed-->edit me<!--/c:ed-->\n{}\n\n\
         <!--c:un-->untouched<!--/c:un-->\n{}\n\n<!--c:nw-->end<!--/c:nw-->\n{}\n",
        thread("rp", "open", &["agent: q?", "me: yes", "me: really"]),
        thread("rs", "resolved", &["agent: q?"]),
        thread("ro", "open", &["agent: q?"]),
        thread("ed", "open", &["me: fixed"]),
        thread("un", "open", &["me: same"]),
        thread("nw", "open", &["me: brand new\nsecond line"]),
    );
    let s = run(&before, &after);
    assert_eq!(s.diff, "", "comment activity alone: no diff");
    // Document order of the anchors, deleted last.
    let ids: Vec<_> = s.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["rp", "rs", "ro", "ed", "nw", "de"]);

    let rp = by_id(&s, "rp");
    assert_eq!(rp.kind, CommentKind::Reply);
    assert_eq!(rp.quote, "reply me");
    assert_eq!(rp.status, "open");
    let texts: Vec<_> = rp.new_entries.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, ["yes", "really"]);
    assert_eq!(rp.new_entries[0].author, "me");

    let rs = by_id(&s, "rs");
    assert_eq!(
        (rs.kind, rs.status.as_str()),
        (CommentKind::Resolved, "resolved")
    );
    assert!(rs.new_entries.is_empty());

    let ro = by_id(&s, "ro");
    assert_eq!(
        (ro.kind, ro.status.as_str()),
        (CommentKind::Reopened, "open")
    );

    let ed = by_id(&s, "ed");
    assert_eq!(ed.kind, CommentKind::Edited);
    assert_eq!(ed.new_entries.len(), 1);
    assert_eq!(ed.new_entries[0].text, "fixed");

    let nw = by_id(&s, "nw");
    assert_eq!(nw.kind, CommentKind::New);
    assert_eq!(nw.quote, "end");
    assert_eq!(nw.new_entries[0].text, "brand new\nsecond line");

    let de = by_id(&s, "de");
    assert_eq!(de.kind, CommentKind::Deleted);
    assert_eq!(de.quote, "delete me");
    assert_eq!(de.new_entries.len(), 2, "last known entries");

    // Legacy fields unchanged.
    assert_eq!(s.new_threads, ["nw"]);
    assert_eq!(s.replied_threads, ["rp"]);
    assert_eq!(s.resolved_threads, ["rs"]);
    assert_eq!(s.deleted_threads, ["de"]);
}

#[test]
fn code_block_comment() {
    let block = "```rust\nfn f() {\n    validate(order)\n    log()\n}\n```\n";
    let before = format!(
        "Intro\n\n<!--c:cc-->\n{block}<!--/c:cc-->\n<!--co:cc by:a status:open quote:\"    validate(order)\" line:1-2\nagent: why?\n-->\n"
    );
    let after = before.replace("agent: why?\n", "agent: why?\nme: before logging\n");
    let s = run(&before, &after);
    assert_eq!(s.diff, "");
    let c = by_id(&s, "cc");
    assert_eq!(c.kind, CommentKind::Reply);
    assert_eq!(c.code_lines, Some((1, 2)));
    assert_eq!(c.quote, "    validate(order)");
    assert_eq!(c.new_entries[0].text, "before logging");
}

#[test]
fn non_ascii_text() {
    let before =
        "Größe: <!--c:u1-->日本語 ✓<!--/c:u1-->\n<!--co:u1 by:a status:open\na: ok?\n-->\n\nÜber\n";
    let after = "Größe: <!--c:u1-->日本語 ✓<!--/c:u1-->\n<!--co:u1 by:a status:open\na: ok?\nme: ja — ünï\n-->\n\nÜber alles 🎉\n";
    let s = run(before, after);
    assert_eq!(s.comments.len(), 1);
    assert_eq!(s.comments[0].quote, "日本語 ✓");
    assert_eq!(s.comments[0].new_entries[0].text, "ja — ünï");
    assert!(s.diff.contains("-Über\n+Über alles 🎉\n"), "{}", s.diff);
}

#[test]
fn edits_that_move_anchors() {
    let before = "Start\n\nship <!--c:k3f9-->on Friday<!--/c:k3f9-->\n<!--co:k3f9 by:a status:open\na: hmm\n-->\n";
    // Reviewer inserts a paragraph before the anchor, edits inside the anchor and replies.
    let after = "Start\n\nNew paragraph\n\nship <!--c:k3f9-->on Thursday<!--/c:k3f9-->\n<!--co:k3f9 by:a status:open\na: hmm\nme: ok\n-->\n";
    let s = run(before, after);
    assert_eq!(s.comments.len(), 1);
    assert_eq!(s.comments[0].quote, "on Thursday", "quote as it is now");
    assert_eq!(s.comments[0].kind, CommentKind::Reply);
    // Diff is on clean text: no markers, no bodies.
    assert!(!s.diff.contains("<!--"), "{}", s.diff);
    assert!(
        s.diff
            .contains("-ship on Friday\n+New paragraph\n+\n+ship on Thursday\n"),
        "{}",
        s.diff
    );
}

#[test]
fn orphan_comes_last_with_stored_quote() {
    let before = format!(
        "<!--c:a1-->one<!--/c:a1-->\n{}\n\n<!--c:b2-->two<!--/c:b2-->\n{}\n",
        thread("a1", "open", &["a: x"]),
        thread("b2", "open", &["a: y"]),
    );
    // Reviewer deletes the anchored text of a1 (markers gone, body stays with its quote).
    let after = format!(
        "\n<!--c:b2-->two<!--/c:b2-->\n{}\n<!--co:a1 by:a status:open quote:\"one\"\na: x\nme: orphaned reply\n-->\n",
        thread("b2", "open", &["a: y", "me: z"]),
    );
    let s = run(&before, &after);
    let ids: Vec<_> = s.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["b2", "a1"]);
    assert_eq!(by_id(&s, "a1").quote, "one");
}

#[test]
fn json_round_trip_and_old_payloads() {
    let s = run("a\n", "b\n");
    let v = serde_json::to_value(&s).unwrap();
    assert!(v["diff"].as_str().unwrap().contains("+b"));
    assert!(v["comments"].is_array());
    let back: Summary = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(back, s);
    // A payload from an older daemon (no diff/comments) still parses.
    let mut old = v;
    old.as_object_mut().unwrap().remove("diff");
    old.as_object_mut().unwrap().remove("comments");
    let back: Summary = serde_json::from_value(old).unwrap();
    assert_eq!(back.diff, "");
}

#[test]
fn kind_serializes_snake_case() {
    let s = run(
        "x\n",
        "<!--c:n1-->x<!--/c:n1-->\n<!--co:n1 by:a status:open\na: hi\n-->\n",
    );
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["comments"][0]["kind"], "new");
    assert_eq!(v["comments"][0]["new_entries"][0]["text"], "hi");
}
