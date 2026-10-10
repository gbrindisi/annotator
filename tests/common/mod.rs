#![allow(dead_code)]
//! Shared helpers for render/mapping tests.

/// Concatenated text inside `<mark data-cid="{id}">` elements (tags stripped, entities decoded).
pub fn marked_text(html: &str, id: &str) -> String {
    let open = format!("<mark data-cid=\"{id}\">");
    let mut out = String::new();
    let mut stack: Vec<bool> = Vec::new();
    let mut i = 0;
    while i < html.len() {
        let rest = &html[i..];
        if rest.starts_with("<mark") {
            stack.push(rest.starts_with(&open));
            i += rest.find('>').unwrap() + 1;
        } else if rest.starts_with("</mark>") {
            stack.pop();
            i += 7;
        } else if rest.starts_with('<') {
            i += rest.find('>').unwrap() + 1;
        } else {
            let c = rest.chars().next().unwrap();
            if stack.iter().any(|&b| b) {
                out.push(c);
            }
            i += c.len_utf8();
        }
    }
    decode(&out)
}

pub fn decode(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

pub fn norm_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Insert `<!--c:id-->`/`<!--/c:id-->` around `range`.
pub fn wrap(raw: &str, range: std::ops::Range<usize>, id: &str) -> String {
    format!(
        "{}<!--c:{id}-->{}<!--/c:{id}-->{}",
        &raw[..range.start],
        &raw[range.clone()],
        &raw[range.end..]
    )
}
