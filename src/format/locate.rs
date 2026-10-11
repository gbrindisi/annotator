//! Port of markedit-comments `src/format/locate.ts` (derived from kylemcd/obsidian-document-comments, MIT; see NOTICE).

use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

static MARKER_AT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<!--/?c:[A-Za-z0-9]+-->").unwrap());

/// Find rendered (preview) text back in the Markdown source. Whitespace is collapsed on both
/// sides and comment markers in the source are skipped. Returns the first match as a byte range
/// (always on char boundaries), or None when the text cannot be found or is blank.
pub fn locate_text(doc: &str, rendered: &str) -> Option<Range<usize>> {
    let needle = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    if needle.is_empty() {
        return None;
    }
    // Normalized haystack; `map[k]` is the source char range for haystack byte k.
    let mut hay = String::new();
    let mut map: Vec<Range<usize>> = Vec::new();
    let mut last_was_space = true;
    let mut i = 0;
    while i < doc.len() {
        if let Some(m) = MARKER_AT_RE.find(&doc[i..]) {
            i += m.end();
            continue;
        }
        let ch = doc[i..].chars().next().unwrap();
        let src = i..i + ch.len_utf8();
        if ch.is_whitespace() {
            if !last_was_space {
                hay.push(' ');
                map.push(src);
                last_was_space = true;
            }
        } else {
            hay.push(ch);
            map.extend(std::iter::repeat_n(src, ch.len_utf8()));
            last_was_space = false;
        }
        i += ch.len_utf8();
    }
    let at = hay.find(&needle)?;
    let from = map.get(at)?.start;
    let to = map.get(at + needle.len() - 1)?.end;
    Some(from..to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_plain_text() {
        assert_eq!(
            locate_text("Hello brave new world.", "brave new"),
            Some(6..15)
        );
    }

    #[test]
    fn collapses_whitespace_and_skips_markers() {
        let doc = "We should <!--c:k3f9-->ship on\nFriday<!--/c:k3f9--> regardless.";
        let r = locate_text(doc, "should ship  on Friday regardless").unwrap();
        assert_eq!(
            &doc[r],
            "should <!--c:k3f9-->ship on\nFriday<!--/c:k3f9--> regardless"
        );
    }

    #[test]
    fn none_when_missing_or_empty() {
        assert_eq!(locate_text("abc", "zzz"), None);
        assert_eq!(locate_text("abc", "   "), None);
    }

    #[test]
    fn non_ascii() {
        let doc = "Ça va 🎉 <!--c:x1-->très\u{00A0}bien🎉<!--/c:x1-->!";
        let r = locate_text(doc, "très bien🎉").unwrap();
        assert_eq!(&doc[r], "très\u{00A0}bien🎉");
    }
}
