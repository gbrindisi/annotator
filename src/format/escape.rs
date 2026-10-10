//! Port of `reference/markedit-comments/format/escape.ts`.
//! Byte compatibility of the on-disk format depends on this file.

/// Replace each `\X` (X not a line terminator) via `map`; `None` keeps the sequence verbatim.
fn unescape_with(s: &str, map: impl Fn(char) -> Option<&'static str>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek().copied() {
                Some(n) if !matches!(n, '\n' | '\r' | '\u{2028}' | '\u{2029}') => {
                    chars.next();
                    match map(n) {
                        Some(r) => out.push_str(r),
                        None => {
                            out.push('\\');
                            out.push(n);
                        }
                    }
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Thread entries are one per physical line, so newlines and backslashes are escaped.
pub fn escape_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

pub fn unescape_text(s: &str) -> String {
    unescape_with(s, |c| match c {
        'n' => Some("\n"),
        'r' => Some("\r"),
        '\\' => Some("\\"),
        _ => None,
    })
}

/// Reaction authors are comma-joined on one line; escape commas in names.
pub fn escape_reaction_author(s: &str) -> String {
    s.replace('\\', "\\\\").replace(',', "\\,")
}

/// Split on commas not directly preceded by a backslash, unescape, trim, drop empties.
pub fn split_reaction_authors(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b',' && (i == 0 || bytes[i - 1] != b'\\') {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    parts
        .into_iter()
        .map(|a| {
            unescape_with(a.trim(), |c| match c {
                ',' => Some(","),
                '\\' => Some("\\"),
                _ => None,
            })
        })
        .filter(|a| !a.is_empty())
        .collect()
}

/// A code comment's quote must round-trip byte-for-byte inside the header's `quote:"…"`.
pub fn encode_code_quote(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\q")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace("-->", "--\\g>")
}

pub fn decode_code_quote(s: &str) -> String {
    unescape_with(s, |c| match c {
        'n' => Some("\n"),
        'r' => Some("\r"),
        'q' => Some("\""),
        'g' => Some(""),
        '\\' => Some("\\"),
        _ => None,
    })
}

/// `-->` must never appear literally inside the body; break it with a zero-width space.
pub fn break_terminator(s: &str) -> String {
    s.replace("-->", "--\u{200B}>")
}

/// Replace every run of whitespace with `with`.
pub(crate) fn collapse_ws(s: &str, with: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push_str(with);
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// Author handles are single tokens: whitespace becomes `_`.
pub fn canonical_author_key(author: &str) -> String {
    collapse_ws(&break_terminator(author.trim()), "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_non_ascii() {
        let s = "café 🎉 \\ a\nb\r -->";
        assert_eq!(unescape_text(&escape_text(s)), s);
        assert_eq!(
            decode_code_quote(&encode_code_quote("\"é\"\n-->🎉")),
            "\"é\"\n-->🎉"
        );
        assert_eq!(unescape_text("\\é \\"), "\\é \\");
    }

    #[test]
    fn reaction_authors() {
        let names = ["a,b", "zoë", "x\\y"];
        let joined: Vec<_> = names.iter().map(|n| escape_reaction_author(n)).collect();
        assert_eq!(split_reaction_authors(&joined.join(", ")), names);
        assert_eq!(split_reaction_authors(" , sam ,"), ["sam"]);
    }
}
