//! Golden cases shared with the web serializer (`web/src/blockmd.ts`): for each document, the
//! rendered HTML and, for every editable block, the Markdown the browser must send for an
//! unchanged block. Here we check that sending exactly that leaves the file byte-identical;
//! `web/src/__tests__/blockmd.test.ts` checks that serializing the rendered block yields it.
//!
//! Regenerate after changing the cases or the renderer:
//! `UPDATE_WEB_CASES=1 cargo test --test blockedit_web_cases`

use annotator::blockedit::{block_range, editable_blocks, replace_block};
use serde_json::{Value, json};

const OUT: &str = "web/src/__tests__/blockmd_cases.json";

fn cases() -> Vec<(String, String)> {
    let fx = |n: &str| {
        std::fs::read_to_string(format!("{}/fixtures/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    };
    let docs = [
        (
            "inline",
            "Plain paragraph with *em*, **strong**, ***both***, ~~strike~~ and `code`.\n\nCode with ``a`b`` backticks and `` `x` `` padded.\n\nLinks: [text](https://example.com), [titled](https://e.com \"T\") and https://bare.example/path_x.\n\nImage ![alt text](img.png) inline.\n\nline one\\\nline two\nsoft three\n",
        ),
        (
            "escapes",
            "Escapes: \\*not em\\*, 2 * 3, snake_case_name, a \\_b\\_ c, \\`tick\\`, 1 < 2, \\<div>, &lt;!-- no, [link\\](x), Tom & Jerry, \\&copy;, back\\slash, end\\\\.\n\n\\# not a heading\n\n\\- not a list\n\n1\\. not ordered\n\n\\> not a quote\n\n\\~\\~not struck\\~\\~ and ~ alone\n",
        ),
        (
            "headings",
            "# Title with `code` and *em*\n\n## Closed heading ##\n\nSetext *heading*\n---\n",
        ),
        (
            "lists",
            "- item *one*\n- [ ] task\n- [x] done task\n  continued\n  - nested **bold**\n\n1. first\n2. second\n\n- loose a\n\n- loose b\n\n  second para\n\n+ [x] loose task\n\n+ [ ] other\n",
        ),
        ("quote", "> quoted *text*\n> second line\n>\n> > deeper\n"),
        (
            "table",
            "| a | b \\| c |\n|---|---|\n| `x\\|y` | [l](u) |\n| Café 🎉 | **B** |\n",
        ),
        (
            "comments",
            "Text <!--c:aa1-->anchored *partly<!--/c:aa1-->* and **<!--c:bb2-->bold<!--/c:bb2-->** end.\n<!--co:aa1 status:open\nme: a\n-->\n<!--co:bb2 status:open\nme: b\n-->\n\nUse <!--c:cc3-->`the code`<!--/c:cc3--> here, <!--c:dd4-->across [a link](x) and<!--/c:dd4--> on.\n<!--co:cc3 status:open\nme: c\n-->\n<!--co:dd4 status:open\nme: d\n-->\n\n- <!--c:ee5-->list<!--/c:ee5--> item\n<!--co:ee5 status:open\nme: e\n-->\n",
        ),
        ("unicode", "Café 🎉 *crème* — “quotes” and ñ.\n"),
    ];
    let mut v: Vec<(String, String)> = docs
        .iter()
        .map(|(n, d)| (n.to_string(), d.to_string()))
        .collect();
    v.push(("sample.md".into(), fx("sample.md")));
    v.push(("mermaid.md".into(), fx("mermaid.md")));
    v
}

/// What the browser sends for an unchanged block: the content with continuation prefixes
/// removed, plus the task checkbox for task items.
fn client_markdown(raw: &str, sp: &str) -> String {
    let r = block_range(raw, sp).unwrap();
    let mut lines = raw[r.clone()].split('\n');
    let mut md = lines.next().unwrap_or("").to_string();
    for l in lines {
        md.push('\n');
        md.push_str(l.trim_start_matches([' ', '\t', '>']));
    }
    let line = &raw[raw[..r.start].rfind('\n').map_or(0, |i| i + 1)..r.start];
    let t = line.trim_end();
    if t.len() >= 3 && t.ends_with(']') && t.as_bytes()[t.len() - 3] == b'[' {
        md = format!("{} {md}", &t[t.len() - 3..]);
    }
    md
}

#[test]
fn web_cases_are_identity_edits_and_up_to_date() {
    let mut out = Vec::new();
    for (name, raw) in cases() {
        let html = annotator::render::render(&raw).html;
        let mut blocks = Vec::new();
        for sp in editable_blocks(&raw) {
            let md = client_markdown(&raw, &sp);
            let res = replace_block(&raw, &sp, &md).unwrap_or_else(|e| panic!("{name} {sp}: {e}"));
            assert_eq!(res, raw, "{name} {sp}: identity edit with {md:?}");
            blocks.push(json!({"sourcepos": sp, "markdown": md}));
        }
        out.push(json!({"name": name, "html": html, "blocks": blocks}));
    }
    let text = serde_json::to_string_pretty(&Value::Array(out)).unwrap() + "\n";
    let path = format!("{}/{OUT}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("UPDATE_WEB_CASES").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let cur = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        cur == text,
        "{OUT} is stale: run UPDATE_WEB_CASES=1 cargo test --test blockedit_web_cases"
    );
}
