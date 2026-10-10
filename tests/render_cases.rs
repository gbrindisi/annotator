//! render(): marker stripping, offset map, marks.

mod common;

use annotator::render::render;
use common::marked_text;

fn check_offsets(raw: &str) {
    let r = render(raw);
    assert_eq!(r.raw_offset.len(), r.clean.len() + 1);
    for (i, b) in r.clean.bytes().enumerate() {
        assert_eq!(raw.as_bytes()[r.raw_offset[i]], b, "clean byte {i}");
    }
    assert!(r.raw_offset.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn sample_fixture() {
    let raw = include_str!("../fixtures/sample.md");
    check_offsets(raw);
    let r = render(raw);
    assert_eq!(marked_text(&r.html, "k3f9"), "ship on Friday");
    assert_eq!(marked_text(&r.html, "a7b2"), "another highlighted span");
    assert_eq!(marked_text(&r.html, "zz1q"), "resolved");
    // No comment bodies leak into clean text or HTML.
    assert!(!r.clean.contains("<!--co:k3f9"));
    assert!(!r.html.contains("I thought we agreed"));
    assert!(!r.html.contains("orphaned thread"));
    assert!(!r.clean.contains("<!--c:p7k2"));
    // Code comment: fence still renders as code, no stray marks inside.
    assert!(
        r.html
            .contains("<code class=\"language-python\">def process(order):")
    );
    // Markers inside a fence are literal text.
    assert!(
        r.html
            .contains("&lt;!--c:ex01--&gt;example span&lt;!--/c:ex01--&gt;")
    );
    assert!(r.html.contains("This body has no anchor any more.</p>"));
    // The body line must not split or join paragraphs.
    assert!(r.html.contains("closes Monday.</p>"));
}

#[test]
fn body_between_paragraph_lines_does_not_split_it() {
    let raw = "line one\n<!--co:a status:open\nme: x\n-->\nline two\n";
    let r = render(raw);
    assert_eq!(r.clean, "line one\nline two\n");
    assert_eq!(r.html.matches("<p ").count(), 1);
    check_offsets(raw);
}

#[test]
fn body_at_end_of_file_without_newline() {
    let raw = "para\n<!--co:a status:open\nme: x\n-->";
    let r = render(raw);
    assert_eq!(r.clean, "para");
    check_offsets(raw);
}

#[test]
fn anchor_spanning_inline_formatting_produces_nested_marks() {
    let raw = "a <!--c:x-->some **bold** and `code` text<!--/c:x--> b\n";
    let r = render(raw);
    assert_eq!(
        r.html,
        "<p data-sourcepos=\"1:1-1:33\">a <mark data-cid=\"x\">some </mark><strong data-sourcepos=\"1:8-1:15\"><mark data-cid=\"x\">bold</mark></strong><mark data-cid=\"x\"> and </mark><code data-sourcepos=\"1:21-1:26\"><mark data-cid=\"x\">code</mark></code><mark data-cid=\"x\"> text</mark> b</p>\n"
    );
}

#[test]
fn anchor_starting_inside_bold() {
    let raw = "x **bo<!--c:y-->ld** z<!--/c:y--> w\n";
    let r = render(raw);
    assert_eq!(marked_text(&r.html, "y"), "ld z");
    assert!(
        r.html.contains(
            "<strong data-sourcepos=\"1:3-1:10\">bo<mark data-cid=\"y\">ld</mark></strong>"
        ),
        "{}",
        r.html
    );
}

#[test]
fn anchor_spanning_blocks_marks_each_part() {
    let raw = "first <!--c:m-->end\n\n- item\n\nsecond<!--/c:m--> rest\n";
    let r = render(raw);
    assert_eq!(marked_text(&r.html, "m"), "enditemsecond");
    assert_eq!(r.html.matches("<mark").count(), 3);
}

#[test]
fn overlapping_anchors_nest_validly() {
    let raw = "<!--c:a-->one <!--c:b-->two<!--/c:a--> three<!--/c:b-->\n";
    let r = render(raw);
    assert_eq!(marked_text(&r.html, "a"), "one two");
    assert_eq!(marked_text(&r.html, "b"), "two three");
    assert_eq!(
        r.html.matches("<mark").count(),
        r.html.matches("</mark>").count()
    );
}

#[test]
fn orphan_and_unpaired_markers_do_not_break() {
    let raw = "text <!--c:q-->open only\n\n<!--/c:z-->close only <!--co:z\n-->\n";
    let r = render(raw);
    assert!(!r.html.contains("<mark"));
    assert!(!r.html.contains("&lt;!--"));
    assert!(r.html.contains("open only"));
    check_offsets(raw);
}

#[test]
fn markers_in_inline_code_are_literal() {
    let raw = "use `<!--c:x-->` and `<!--/c:x-->`\n";
    let r = render(raw);
    assert_eq!(r.clean, raw);
    assert!(!r.html.contains("<mark"));
}

#[test]
fn gfm_and_safe_html() {
    let r = render("~~del~~ www.example.com \"q\" -- x\n\n- [x] done\n\n<script>x</script>\n");
    assert!(r.html.contains("<del"));
    assert!(r.html.contains("href=\"http://www.example.com\""));
    assert!(
        r.html.contains("&quot;q&quot; -- x"),
        "smart punctuation must be off"
    );
    assert!(r.html.contains("checkbox"));
    assert!(!r.html.contains("<script>"));
}

#[test]
fn image_alt_text_not_marked() {
    let r = render("<!--c:i-->see ![alt text](x.png) here<!--/c:i-->\n");
    assert!(r.html.contains("alt=\"alt text\""));
    assert_eq!(marked_text(&r.html, "i"), "see  here");
}
