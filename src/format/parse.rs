//! Port of `reference/markedit-comments/format/parse.ts`.

use super::escape::{decode_code_quote, split_reaction_authors, unescape_text};
use super::types::{CommentData, CommentStatus, ParsedComment, Reaction, ThreadEntry};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::LazyLock;

static OPEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--c:([A-Za-z0-9]+)-->").unwrap());
static CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--/c:([A-Za-z0-9]+)-->").unwrap());
static BODY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--co:([A-Za-z0-9]+)[ \t]*([^\n]*)\n?((?s:.*?))-->").unwrap());
static HEADER_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([A-Za-z0-9_]+):(?:"([^"]*)"|(\S+))"#).unwrap());
static THREAD_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.*?)(?:\s\(([^)]*)\))?:\s?((?s:.*))$").unwrap());
static REACTION_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\+\s*(?:@([0-9]+)\s+)?(\S+)\s+(.+)$").unwrap());
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*(`{3,}|~{3,})").unwrap());
static INLINE_CODE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`+[^`\n]*`+").unwrap());

/// Parse every comment in a document, in order of first appearance (opens, then closes, then
/// bodies). Markers inside fenced or inline code are ignored. Duplicate ids keep the first.
pub fn parse_comments(doc: &str) -> Vec<ParsedComment> {
    let masks = masked_ranges(doc);
    let masked = |i: usize| is_inside(&masks, i);

    let mut opens: HashMap<String, Range<usize>> = HashMap::new();
    let mut closes: HashMap<String, Range<usize>> = HashMap::new();
    let mut bodies: HashMap<String, (Range<usize>, CommentData)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut track = |id: &str| {
        if seen.insert(id.to_string()) {
            order.push(id.to_string());
        }
    };

    for (re, map) in [(&*OPEN_RE, &mut opens), (&*CLOSE_RE, &mut closes)] {
        for m in re.captures_iter(doc) {
            let full = m.get(0).unwrap();
            if masked(full.start()) {
                continue;
            }
            let id = &m[1];
            map.entry(id.to_string()).or_insert(full.range());
            track(id);
        }
    }

    for m in BODY_RE.captures_iter(doc) {
        let full = m.get(0).unwrap();
        if masked(full.start()) {
            continue;
        }
        let id = &m[1];
        if !bodies.contains_key(id) {
            let mut data = parse_header(&m[2]);
            (data.thread, data.reactions) = parse_body(&m[3]);
            bodies.insert(id.to_string(), (full.range(), data));
        }
        track(id);
    }

    order
        .into_iter()
        .map(|id| {
            let (body, data) = match bodies.remove(&id) {
                Some((r, d)) => (Some(r), d),
                None => (None, CommentData::default()),
            };
            ParsedComment {
                open: opens.get(&id).cloned(),
                close: closes.get(&id).cloned(),
                body,
                data,
                id,
            }
        })
        .collect()
}

/// The ids already present in a document (for id generation), in document order.
pub fn existing_ids(doc: &str) -> Vec<String> {
    parse_comments(doc).into_iter().map(|c| c.id).collect()
}

impl ParsedComment {
    /// Both markers present and ordered.
    pub fn is_anchored(&self) -> bool {
        self.anchor_range().is_some()
    }

    /// Has a body but is not properly anchored.
    pub fn is_orphan(&self) -> bool {
        self.body.is_some() && !self.is_anchored()
    }

    /// Anchored to lines inside a fenced block.
    pub fn is_code_comment(&self) -> bool {
        self.data.code_lines.is_some()
    }

    /// The highlighted text range (between the markers), or None if not anchored.
    pub fn anchor_range(&self) -> Option<Range<usize>> {
        match (&self.open, &self.close) {
            (Some(o), Some(c)) if o.end <= c.start => Some(o.end..c.start),
            _ => None,
        }
    }
}

fn parse_header(header: &str) -> CommentData {
    let mut attrs: HashMap<&str, &str> = HashMap::new();
    let mut extra = Vec::new();
    for m in HEADER_ATTR_RE.captures_iter(header) {
        let key = m.get(1).unwrap().as_str();
        let value = m.get(2).or(m.get(3)).map_or("", |v| v.as_str());
        attrs.insert(key, value);
        if !matches!(key, "by" | "at" | "status" | "quote" | "line") {
            extra.push(m[0].to_string());
        }
    }
    let status = if attrs.get("status") == Some(&"resolved") {
        CommentStatus::Resolved
    } else {
        CommentStatus::Open
    };
    let code_lines = attrs.get("line").and_then(|v| parse_line_range(v));
    let quote = attrs.get("quote").map(|q| {
        if code_lines.is_some() {
            decode_code_quote(q)
        } else {
            q.to_string()
        }
    });
    CommentData {
        author: attrs.get("by").map(|s| s.to_string()),
        created_at: attrs.get("at").map(|s| s.to_string()),
        status,
        quote,
        code_lines,
        thread: Vec::new(),
        reactions: Vec::new(),
        extra_header: extra,
    }
}

/// Parse `F-T` (or `F`) into an inclusive line range (`start..end` both inclusive).
fn parse_line_range(value: &str) -> Option<Range<usize>> {
    let (a, b) = match value.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (value, None),
    };
    let num = |s: &str| {
        (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse::<usize>().ok())
            .flatten()
    };
    let from = num(a)?;
    let to = match b {
        Some(b) => num(b)?,
        None => from,
    };
    (to >= from).then_some(from..to)
}

/// Fenced code-block ranges (``` or ~~~), from the opening fence to the end of the closing
/// fence line (exclusive of its newline). An unclosed fence runs to the end of the document.
pub fn fenced_ranges(doc: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut fence: Option<(usize, u8)> = None;
    for line in doc.split('\n') {
        let line_end = offset + line.len();
        let marker = FENCE_RE.captures(line).map(|c| c[1].as_bytes()[0]);
        match (fence, marker) {
            (None, Some(ch)) => fence = Some((offset, ch)),
            (Some((start, ch)), Some(m)) if m == ch => {
                ranges.push(start..line_end);
                fence = None;
            }
            _ => {}
        }
        offset = line_end + 1;
    }
    if let Some((start, _)) = fence {
        ranges.push(start..doc.len());
    }
    ranges
}

/// True when `pos` sits inside a fenced code block.
pub fn is_in_fenced_code(doc: &str, pos: usize) -> bool {
    is_in_ranges(&fenced_ranges(doc), pos)
}

/// Fenced ranges plus inline code spans (`` `+[^`\n]*`+ ``): markers starting in these are ignored.
pub fn masked_ranges(doc: &str) -> Vec<Range<usize>> {
    let mut ranges = fenced_ranges(doc);
    ranges.extend(INLINE_CODE_RE.find_iter(doc).map(|m| m.range()));
    ranges
}

fn is_inside(ranges: &[Range<usize>], i: usize) -> bool {
    is_in_ranges(ranges, i)
}

pub(crate) fn is_in_ranges(ranges: &[Range<usize>], i: usize) -> bool {
    ranges.iter().any(|r| r.contains(&i))
}

fn parse_body(block: &str) -> (Vec<ThreadEntry>, Vec<Reaction>) {
    let mut thread: Vec<ThreadEntry> = Vec::new();
    let mut reactions = Vec::new();
    for raw in block.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }
        if let Some(rx) = REACTION_LINE_RE.captures(line) {
            reactions.push(Reaction {
                emoji: rx[2].to_string(),
                authors: split_reaction_authors(&rx[3]),
                entry: rx.get(1).and_then(|e| e.as_str().parse().ok()),
            });
            continue;
        }
        let m = THREAD_LINE_RE
            .captures(line)
            .filter(|m| !m[1].trim().is_empty());
        if let Some(m) = m {
            thread.push(ThreadEntry {
                author: m[1].trim().to_string(),
                timestamp: m
                    .get(2)
                    .map(|t| t.as_str().to_string())
                    .filter(|t| !t.is_empty()),
                text: unescape_text(&m[3]),
            });
        } else if let Some(last) = thread.last_mut() {
            // Unstructured continuation line (legacy, pre-escaping): fold into the previous entry.
            last.text.push('\n');
            last.text.push_str(&unescape_text(line));
        } else {
            thread.push(ThreadEntry {
                author: String::new(),
                timestamp: None,
                text: unescape_text(line),
            });
        }
    }
    (thread, reactions)
}
