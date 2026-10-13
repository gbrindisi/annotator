//! Table-driven selection -> raw range tests. Each case also re-renders with markers inserted
//! and checks the new `<mark>` wraps exactly the selected text.

mod common;

use annotator::mapping::{MappingError, map_selection};
use annotator::protocol::Selection;
use annotator::render::render;
use common::{marked_text, wrap};

/// Soft breaks between mark runs aren't inside a mark, so compare without whitespace.
fn no_ws(s: &str) -> String {
    s.split_whitespace().collect()
}

struct Case {
    name: &'static str,
    raw: &'static str,
    sourcepos: &'static str,
    text: &'static str,
    /// UTF-16 offset in the element's textContent.
    offset: usize,
    /// Expected raw slice and its byte start.
    slice: &'static str,
    start: usize,
}

const CASES: &[Case] = &[
    Case {
        name: "plain paragraph",
        raw: "Hello world, this is plain.\n",
        sourcepos: "1:1-1:27",
        text: "world",
        offset: 6,
        slice: "world",
        start: 6,
    },
    Case {
        name: "repeated word, first",
        raw: "the cat and the dog and the end\n",
        sourcepos: "1:1-1:31",
        text: "the",
        offset: 0,
        slice: "the",
        start: 0,
    },
    Case {
        name: "repeated word, third",
        raw: "the cat and the dog and the end\n",
        sourcepos: "1:1-1:31",
        text: "the",
        offset: 24,
        slice: "the",
        start: 24,
    },
    Case {
        name: "repeated word, stale offset picks nearest",
        raw: "the cat and the dog and the end\n",
        sourcepos: "1:1-1:31",
        text: "the",
        offset: 14,
        slice: "the",
        start: 12,
    },
    Case {
        name: "bold inside selection",
        raw: "Some **bold** text here\n",
        sourcepos: "1:1-1:23",
        text: "Some bold text",
        offset: 0,
        slice: "Some **bold** text",
        start: 0,
    },
    Case {
        name: "whole bold word anchors inside **",
        raw: "a **bold** b\n",
        sourcepos: "1:1-1:12",
        text: "bold",
        offset: 2,
        slice: "bold",
        start: 4,
    },
    Case {
        name: "whole bold word, selection includes spaces",
        raw: "a **bold** b\n",
        sourcepos: "1:1-1:12",
        text: " bold ",
        offset: 1,
        slice: "bold",
        start: 4,
    },
    Case {
        name: "italic and code span",
        raw: "x *it* and `co de` y\n",
        sourcepos: "1:1-1:20",
        text: "it and co de",
        offset: 2,
        slice: "it* and `co de`",
        start: 3,
    },
    Case {
        name: "edge inside code span expands to the whole span",
        raw: "run `cargo test` now\n",
        sourcepos: "1:1-1:20",
        text: "go te",
        offset: 6,
        slice: "`cargo test`",
        start: 4,
    },
    Case {
        name: "link text",
        raw: "see [the docs](http://x.y \"T\") now\n",
        sourcepos: "1:1-1:34",
        text: "the docs now",
        offset: 4,
        slice: "the docs](http://x.y \"T\") now",
        start: 5,
    },
    Case {
        name: "partially inside bold",
        raw: "plain **bold words** end\n",
        sourcepos: "1:1-1:24",
        text: "ain bo",
        offset: 2,
        slice: "ain **bo",
        start: 2,
    },
    Case {
        name: "partially inside bold, from the inside",
        raw: "plain **bold words** end\n",
        sourcepos: "1:1-1:24",
        text: "words end",
        offset: 11,
        slice: "words** end",
        start: 13,
    },
    Case {
        name: "inline element as target (strong)",
        raw: "Some **bold** text\n",
        sourcepos: "1:6-1:13",
        text: "old",
        offset: 1,
        slice: "old",
        start: 8,
    },
    Case {
        name: "nested list item",
        raw: "- one\n  - two *deep*\n  - three\n",
        sourcepos: "2:3-2:14",
        text: "deep",
        offset: 4,
        slice: "deep",
        start: 15,
    },
    Case {
        name: "outer list item",
        raw: "- one\n  - two *deep*\n  - three\n",
        sourcepos: "1:1-3:9",
        text: "one",
        offset: 0,
        slice: "one",
        start: 2,
    },
    Case {
        name: "ordered list item",
        raw: "1. first\n2. second item\n",
        sourcepos: "2:1-2:14",
        text: "item",
        offset: 7,
        slice: "item",
        start: 19,
    },
    Case {
        name: "task list item",
        raw: "- [ ] do the thing\n",
        sourcepos: "1:1-1:18",
        text: "the thing",
        offset: 4,
        slice: "the thing",
        start: 9,
    },
    Case {
        name: "heading",
        raw: "## A heading here ##\n",
        sourcepos: "1:1-1:20",
        text: "heading",
        offset: 2,
        slice: "heading",
        start: 5,
    },
    Case {
        name: "blockquote across soft break",
        raw: "> quoted line\n> second line\n",
        sourcepos: "1:3-2:13",
        text: "line\nsecond",
        offset: 7,
        slice: "line\n> second",
        start: 9,
    },
    Case {
        name: "blockquote, browser collapsed newline to space",
        raw: "> quoted line\n> second line\n",
        sourcepos: "1:3-2:13",
        text: "line second",
        offset: 7,
        slice: "line\n> second",
        start: 9,
    },
    Case {
        name: "table cell with escaped pipe",
        raw: "| h1 | h2 |\n|----|----|\n| x \\| y | **z** |\n",
        sourcepos: "3:2-3:9",
        text: "x | y",
        offset: 0,
        slice: "x \\| y",
        start: 26,
    },
    Case {
        name: "table cell bold",
        raw: "| h1 | h2 |\n|----|----|\n| x \\| y | **z** |\n",
        sourcepos: "3:11-3:17",
        text: "z",
        offset: 0,
        slice: "z",
        start: 37,
    },
    Case {
        name: "table cell, text after escaped pipes (sourcepos drift)",
        raw: "| a | b |\n|---|---|\n| `x\\|y` p\\|q end | c |\n",
        sourcepos: "3:2-3:18",
        text: "end",
        offset: 8,
        slice: "end",
        start: 34,
    },
    Case {
        name: "table cell, escaped pipe after code span",
        raw: "| a | b |\n|---|---|\n| `x\\|y` p\\|q end | c |\n",
        sourcepos: "3:2-3:18",
        text: "p|q",
        offset: 4,
        slice: "p\\|q",
        start: 29,
    },
    Case {
        name: "escapes and entities",
        raw: "a \\*star\\* &amp; b\n",
        sourcepos: "1:1-1:18",
        text: "*star* & b",
        offset: 2,
        slice: "\\*star\\* &amp; b",
        start: 2,
    },
    Case {
        name: "ends on entity",
        raw: "fish &amp; chips\n",
        sourcepos: "1:1-1:16",
        text: "fish &",
        offset: 0,
        slice: "fish &amp;",
        start: 0,
    },
    Case {
        name: "text after existing anchor",
        raw: "We <!--c:a1-->ship it<!--/c:a1--> on Friday.\n<!--co:a1 status:open\nme: hi\n-->\n\nNext.\n",
        sourcepos: "1:1-1:21",
        text: "on Friday",
        offset: 11,
        slice: "on Friday",
        start: 34,
    },
    Case {
        name: "exactly an existing anchor nests inside its markers",
        raw: "We <!--c:a1-->ship it<!--/c:a1--> on Friday.\n<!--co:a1 status:open\nme: hi\n-->\n",
        sourcepos: "1:1-1:21",
        text: "ship it",
        offset: 3,
        slice: "ship it",
        start: 14,
    },
    Case {
        name: "overlapping an existing anchor",
        raw: "We <!--c:a1-->ship it<!--/c:a1--> on Friday.\n",
        sourcepos: "1:1-1:21",
        text: "it on",
        offset: 8,
        slice: "it<!--/c:a1--> on",
        start: 19,
    },
    Case {
        name: "second paragraph after a body block",
        raw: "One.\n<!--co:zz status:open\nme: x\n-->\n\nTwo words.\n",
        sourcepos: "3:1-3:10",
        text: "words",
        offset: 4,
        slice: "words",
        start: 42,
    },
    Case {
        name: "non-ASCII and emoji (UTF-16 offset)",
        raw: "😀 café 😀 café end\n",
        sourcepos: "1:1-1:25",
        text: "café",
        offset: 11,
        slice: "café",
        start: 16,
    },
    Case {
        name: "emoji inside selection",
        raw: "say 👋🏽 hi\n",
        sourcepos: "1:1-1:15",
        text: "👋🏽 hi",
        offset: 4,
        slice: "👋🏽 hi",
        start: 4,
    },
    Case {
        name: "marker-looking text in inline code is literal",
        raw: "use `<!--c:x-->` here\n",
        sourcepos: "1:1-1:21",
        text: "here",
        offset: 15,
        slice: "here",
        start: 17,
    },
];

#[test]
fn mapping_table() {
    let mut failures = Vec::new();
    for c in CASES {
        let sel = Selection {
            sourcepos: c.sourcepos.into(),
            text: c.text.into(),
            offset: c.offset,
        };
        let got = match map_selection(c.raw, &sel) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{}: error {e:?}", c.name));
                continue;
            }
        };
        if &c.raw[got.clone()] != c.slice || got.start != c.start {
            failures.push(format!(
                "{}: got {:?} {:?}, want {:?} at {}",
                c.name,
                got,
                &c.raw[got.clone()],
                c.slice,
                c.start
            ));
            continue;
        }
        let html = render(&wrap(c.raw, got, "zzz")).html;
        let marked = marked_text(&html, "zzz");
        // Code-span edges expand to the whole span (documented rule).
        let want = if c.slice.starts_with('`') {
            c.slice.trim_matches('`')
        } else {
            c.text
        };
        if no_ws(&marked) != no_ws(want) {
            failures.push(format!("{}: marked {marked:?}, html {html}", c.name));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn code_block_selection_is_rejected_with_lines() {
    let raw = "Intro.\n\n```rust\nfn a() {}\nlet x = 1;\nlet y = 2;\n```\n";
    let sel = Selection {
        sourcepos: "3:1-7:3".into(),
        text: "x = 1;\nlet y".into(),
        offset: 14,
    };
    match map_selection(raw, &sel) {
        Err(MappingError::InCodeBlock {
            block,
            from_line,
            to_line,
        }) => {
            assert_eq!((from_line, to_line), (1, 2));
            assert_eq!(
                &raw[block],
                "```rust\nfn a() {}\nlet x = 1;\nlet y = 2;\n```"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn cross_block_rejected() {
    let raw = "> one\n>\n> two\n";
    let sel = Selection {
        sourcepos: "1:1-3:5".into(),
        text: "one\n\ntwo".into(),
        offset: 1,
    };
    assert_eq!(map_selection(raw, &sel), Err(MappingError::CrossBlock));
    let raw = "- a\n- b\n";
    let sel = Selection {
        sourcepos: "1:1-2:3".into(),
        text: "a b".into(),
        offset: 1,
    };
    assert_eq!(map_selection(raw, &sel), Err(MappingError::CrossBlock));
}

#[test]
fn errors() {
    let raw = "Hello world\n";
    let sel = |sp: &str, t: &str| Selection {
        sourcepos: sp.into(),
        text: t.into(),
        offset: 0,
    };
    assert!(matches!(
        map_selection(raw, &sel("nope", "x")),
        Err(MappingError::BadSourcepos(_))
    ));
    assert!(matches!(
        map_selection(raw, &sel("9:1-9:2", "x")),
        Err(MappingError::BadSourcepos(_))
    ));
    assert_eq!(
        map_selection(raw, &sel("1:1-1:11", "absent")),
        Err(MappingError::NotFound)
    );
    assert_eq!(
        map_selection(raw, &sel("1:1-1:11", "  ")),
        Err(MappingError::NotFound)
    );
}

#[test]
fn sample_fixture_selection() {
    let raw = include_str!("../fixtures/sample.md");
    let r = render(raw);
    // The paragraph with the first anchor: find its sourcepos in the HTML.
    let i = r.html.find("We should").unwrap();
    let p = r.html[..i].rfind("data-sourcepos=\"").unwrap() + 16;
    let sp = &r.html[p..p + r.html[p..].find('"').unwrap()];
    let sel = Selection {
        sourcepos: sp.into(),
        text: "QA timeline".into(),
        offset: 36,
    };
    let got = map_selection(raw, &sel).unwrap();
    assert_eq!(&raw[got.clone()], "QA timeline");
    let html = render(&wrap(raw, got, "new1")).html;
    assert_eq!(marked_text(&html, "new1"), "QA timeline");
}

/// The UI's "Comment" button on a rendered Mermaid diagram sends the whole code block:
/// sourcepos of the `<pre>`, its full textContent and offset 0. That must become a line
/// comment covering every line of the block.
#[test]
fn whole_code_block_selection_covers_all_lines() {
    let raw =
        "Plan:\n\n```mermaid\nflowchart LR\n  A[Start] --> B{Ok?}\n\n  B -->|yes| C\n```\n\nEnd.\n";
    let r = render(raw);
    let sp = "3:1-8:3";
    assert!(
        r.html.contains(&format!("<pre data-sourcepos=\"{sp}\">")),
        "{}",
        r.html
    );
    // textContent of <pre><code>: the literal, including the trailing newline.
    let text = "flowchart LR\n  A[Start] --> B{Ok?}\n\n  B -->|yes| C\n";
    let sel = Selection {
        sourcepos: sp.into(),
        text: text.into(),
        offset: 0,
    };
    match map_selection(raw, &sel) {
        Err(MappingError::InCodeBlock {
            block,
            from_line,
            to_line,
        }) => {
            assert_eq!((from_line, to_line), (0, 3));
            assert!(raw[block.clone()].starts_with("```mermaid") && raw[block].ends_with("```"));
        }
        other => panic!("{other:?}"),
    }
    // A single-line diagram and an indented block inside a list item behave the same.
    let one = "```mermaid\ngraph TD\n```\n";
    let sel = Selection {
        sourcepos: "1:1-3:3".into(),
        text: "graph TD\n".into(),
        offset: 0,
    };
    assert!(matches!(
        map_selection(one, &sel),
        Err(MappingError::InCodeBlock {
            from_line: 0,
            to_line: 0,
            ..
        })
    ));
    let nested = "- a\n\n  ```mermaid\n  graph TD\n    A-->B\n  ```\n";
    let r = render(nested);
    assert!(
        r.html.contains("<pre data-sourcepos=\"3:3-6:5\">"),
        "{}",
        r.html
    );
    let sel = Selection {
        sourcepos: "3:3-6:5".into(),
        text: "graph TD\n  A-->B\n".into(),
        offset: 0,
    };
    assert!(matches!(
        map_selection(nested, &sel),
        Err(MappingError::InCodeBlock {
            from_line: 0,
            to_line: 1,
            ..
        })
    ));
}
