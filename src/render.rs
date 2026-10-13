//! Markdown -> HTML for the rendered view. Owned by the render/mapping Worker.
//!
//! Contract: comment markers and body blocks never reach comrak; anchors come back as
//! `<mark data-cid="ID">`, every block carries `data-sourcepos`, and the offset map lets
//! `mapping` translate positions in the rendered text back to raw byte offsets.
//!
//! Pipeline:
//! 1. [`scan_markers`] finds open/close markers and body blocks via `crate::format`.
//! 2. [`strip`] removes them; a line that consisted only of markers/bodies is removed together
//!    with its newline, so bodies never introduce blank lines that change block structure.
//! 3. comrak parses `clean`. Text/code-span nodes carry precise inline sourcepos (verified in the
//!    spike), so each rendered char is aligned to a clean byte (`char_map`).
//! 4. Anchored chars get private-use sentinels inserted into the AST text (one mark run per text
//!    node, so marks always nest validly), which are replaced by `<mark>` after HTML output.
//!
//! Limitation: documents containing U+E000..U+E002 themselves would confuse step 4.

use comrak::nodes::{AstNode, NodeValue, Sourcepos};
use comrak::{Arena, Options};
use std::collections::HashMap;
use std::ops::Range;

/// Output of [`render`].
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub html: String,
    /// The document with all markers and comment bodies removed (what comrak parsed).
    pub clean: String,
    /// `raw_offset[i]` = raw byte offset of clean byte `i` (len = clean.len() + 1).
    pub raw_offset: Vec<usize>,
    /// Anchored comments: id and the anchored range in `clean` bytes.
    pub anchors: Vec<(String, Range<usize>)>,
    /// Every code block (fenced or indented, nested ones included): its range in `clean` bytes
    /// and the exact `data-sourcepos` of its `<pre>` in `html`.
    pub code_blocks: Vec<(Range<usize>, String)>,
}

const OPEN_S: char = '\u{E000}';
const OPEN_E: char = '\u{E001}';
const CLOSE: char = '\u{E002}';

/// The comrak options used for the rendered view (and for re-parsing in `mapping`).
pub(crate) fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.parse.smart = false;
    o.render.sourcepos = true;
    o
}

pub fn render(raw: &str) -> Rendered {
    let (clean, raw_offset, anchors) = strip(raw);
    let opts = options();
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, &clean, &opts);
    if !anchors.is_empty() {
        let starts = line_starts(&clean);
        mark_anchors(root, &clean, &starts, &anchors);
    }
    let starts = line_starts(&clean);
    let code_blocks = root
        .descendants()
        .filter_map(|n| {
            let d = n.data.borrow();
            matches!(d.value, NodeValue::CodeBlock(_)).then(|| {
                (
                    sourcepos_range(&d.sourcepos, &starts, clean.len()),
                    d.sourcepos.to_string(),
                )
            })
        })
        .collect();
    let mut html = String::new();
    comrak::html::format_document(root, &opts, &mut html).expect("fmt to String");
    Rendered {
        html: replace_sentinels(&html),
        clean,
        raw_offset,
        anchors,
        code_blocks,
    }
}

impl Rendered {
    /// `data-sourcepos` of the `<pre>` of the code block a code-line comment (`line:`) belongs
    /// to: the first code block overlapping the comment's anchored range (the markers wrap the
    /// whole fence). `None` when the comment has no anchor or no code block lies inside it.
    pub fn code_block_of(&self, id: &str) -> Option<&str> {
        let (_, range) = self.anchors.iter().find(|(a, _)| a == id)?;
        self.code_blocks
            .iter()
            .find(|(b, _)| b.start < range.end && b.end > range.start)
            .map(|(_, sp)| sp.as_str())
    }
}

// ---------- marker scanning (delegates to crate::format) ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Open,
    Close,
    Body,
}

#[derive(Debug, Clone)]
pub(crate) struct Marker {
    pub kind: MarkerKind,
    pub id: String,
    pub range: Range<usize>,
}

/// Markers as found by [`crate::format::parse_comments`] (fenced and inline code masked; only
/// the first open/close/body per id counts), sorted by position.
pub(crate) fn scan_markers(doc: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    for c in crate::format::parse_comments(doc) {
        for (kind, r) in [
            (MarkerKind::Open, &c.open),
            (MarkerKind::Close, &c.close),
            (MarkerKind::Body, &c.body),
        ] {
            if let Some(range) = r {
                out.push(Marker {
                    kind,
                    id: c.id.clone(),
                    range: range.clone(),
                });
            }
        }
    }
    out.sort_by_key(|m| m.range.start);
    out
}

// ---------- stripping ----------

type Stripped = (String, Vec<usize>, Vec<(String, Range<usize>)>);

/// Remove markers and bodies. Returns `(clean, raw_offset, anchors)`.
pub(crate) fn strip(raw: &str) -> Stripped {
    let markers = scan_markers(raw);
    let bytes = raw.as_bytes();
    let mut removed = vec![false; raw.len()];
    for m in &markers {
        removed[m.range.clone()].iter_mut().for_each(|b| *b = true);
    }
    // Lines made only of markers/bodies disappear entirely (including one newline).
    let mut ls = 0;
    while ls < raw.len() {
        let le = raw[ls..].find('\n').map_or(raw.len(), |i| ls + i);
        if le > ls && removed[ls..le].iter().all(|&b| b) {
            if le < raw.len() {
                removed[le] = true;
            } else if let Some(nl) = (0..ls).rev().find(|&i| !removed[i]) {
                // Last line: drop the newline that precedes the removed run instead.
                if bytes[nl] == b'\n' {
                    removed[nl] = true;
                }
            }
        }
        ls = le + 1;
    }
    let mut clean = Vec::with_capacity(raw.len());
    let mut raw_offset = Vec::with_capacity(raw.len() + 1);
    // clean_at[r] = clean index of the first kept byte at or after raw byte r.
    let mut clean_at = Vec::with_capacity(raw.len() + 1);
    for (i, &b) in bytes.iter().enumerate() {
        clean_at.push(clean.len());
        if !removed[i] {
            clean.push(b);
            raw_offset.push(i);
        }
    }
    clean_at.push(clean.len());
    raw_offset.push(raw.len());
    let clean = String::from_utf8(clean).expect("removed ranges are char-aligned");

    let mut anchors = Vec::new();
    for open in markers.iter().filter(|m| m.kind == MarkerKind::Open) {
        let close = markers
            .iter()
            .find(|m| m.kind == MarkerKind::Close && m.id == open.id);
        if let Some(close) = close
            && open.range.end <= close.range.start
        {
            let r = clean_at[open.range.end]..clean_at[close.range.start];
            anchors.push((open.id.clone(), r));
        }
    }
    (clean, raw_offset, anchors)
}

// ---------- sourcepos / alignment helpers (shared with mapping) ----------

pub(crate) fn line_starts(s: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(s.match_indices('\n').map(|(i, _)| i + 1));
    v
}

/// Clean byte offsets `[start, end)` of a sourcepos (comrak columns are 1-based UTF-8 bytes,
/// end inclusive).
pub(crate) fn sourcepos_range(sp: &Sourcepos, starts: &[usize], len: usize) -> Range<usize> {
    let at = |line: usize, col: usize| -> usize {
        let base = starts.get(line.saturating_sub(1)).copied().unwrap_or(len);
        (base + col.saturating_sub(1)).min(len)
    };
    let s = at(sp.start.line, sp.start.column);
    let e = (at(sp.end.line, sp.end.column) + 1).min(len);
    s..e.max(s)
}

/// Rendered chars of a Text or Code node and the true clean range of the node.
#[derive(Debug, Clone, Default)]
pub(crate) struct NodeChars {
    pub chars: Vec<(char, Option<usize>)>,
    pub range: Range<usize>,
}

pub(crate) type CharMap = HashMap<*const (), NodeChars>;

pub(crate) fn node_key(node: &AstNode<'_>) -> *const () {
    node as *const AstNode<'_> as *const ()
}

/// For every Text/Code node under `root`: its rendered chars aligned to clean byte offsets.
///
/// comrak's inline sourcepos is exact, except inside table cells where `\|` is unescaped
/// before inline parsing, so positions after an escaped pipe drift left by one per pipe;
/// that drift is corrected here (reset per cell).
pub(crate) fn char_map<'a>(root: &'a AstNode<'a>, clean: &str, starts: &[usize]) -> CharMap {
    let mut map = HashMap::new();
    let mut drift = 0usize;
    let mut cell: Option<*const ()> = None;
    for node in root.descendants() {
        let data = node.data.borrow();
        let (val, ticks) = match &data.value {
            NodeValue::Text(t) => (t.as_ref(), 0),
            NodeValue::Code(c) => (c.literal.as_str(), c.num_backticks),
            _ => continue,
        };
        let this_cell = node
            .ancestors()
            .find(|a| matches!(a.data.borrow().value, NodeValue::TableCell))
            .map(node_key);
        if this_cell != cell {
            cell = this_cell;
            drift = 0;
        }
        let r = sourcepos_range(&data.sourcepos, starts, clean.len());
        let (mut s, mut e) = (
            (r.start + drift).min(clean.len()),
            (r.end + drift).min(clean.len()),
        );
        if cell.is_some() {
            let mut extra = 0;
            loop {
                let end = (e + extra).min(clean.len());
                let k = clean.get(s..end).map_or(0, |x| x.matches("\\|").count());
                if k == extra {
                    break;
                }
                extra = k;
            }
            e = (e + extra).min(clean.len());
            drift += extra;
        }
        if s > e {
            s = e;
        }
        let inner = (s + ticks).min(e)..e.saturating_sub(ticks).max((s + ticks).min(e));
        let src = clean.get(inner.clone()).unwrap_or("");
        map.insert(
            node_key(node),
            NodeChars {
                chars: align(src, inner.start, val),
                range: s..e,
            },
        );
    }
    map
}

/// Greedy alignment of rendered text `val` against its source slice `src` (at `base`):
/// skips backslash escapes, maps entities and escapes to their
/// first source byte, tolerates whitespace changes.
fn align(src: &str, base: usize, val: &str) -> Vec<(char, Option<usize>)> {
    let mut out = Vec::with_capacity(val.len());
    let mut p = 0;
    for c in val.chars() {
        let mut hit = None;
        while let Some(sc) = src[p..].chars().next() {
            if sc == c || (c.is_whitespace() && sc.is_whitespace()) {
                hit = Some(base + p);
                p += sc.len_utf8();
                break;
            }
            let rest = &src[p + 1..];
            if sc == '\\' && rest.starts_with(c) {
                hit = Some(base + p);
                p += 1 + c.len_utf8();
                break;
            }
            if sc == '&'
                && let Some(n) = entity_len(&src[p..])
            {
                hit = Some(base + p);
                p += n;
                break;
            }
            p += sc.len_utf8();
        }
        out.push((c, hit));
    }
    out
}

pub(crate) fn entity_len(s: &str) -> Option<usize> {
    if !s.starts_with('&') {
        return None;
    }
    let semi = s.find(';')?;
    (semi > 1
        && semi <= 32
        && s[1..semi]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '#'))
    .then_some(semi + 1)
}

// ---------- marks ----------

fn mark_anchors<'a>(
    root: &'a AstNode<'a>,
    clean: &str,
    starts: &[usize],
    anchors: &[(String, Range<usize>)],
) {
    let mut order: Vec<usize> = (0..anchors.len()).collect();
    order.sort_by_key(|&i| (anchors[i].1.start, std::cmp::Reverse(anchors[i].1.end)));
    let map = char_map(root, clean, starts);
    for node in root.descendants() {
        let in_image = node
            .ancestors()
            .any(|a| matches!(a.data.borrow().value, NodeValue::Image(_)));
        if in_image {
            continue; // alt text ends up in an attribute
        }
        let Some(chars) = map.get(&node_key(node)).map(|n| n.chars.clone()) else {
            continue;
        };
        let mut out = String::new();
        let mut active: Vec<usize> = Vec::new();
        for (c, off) in chars {
            let now: Vec<usize> = match off {
                Some(o) => order
                    .iter()
                    .copied()
                    .filter(|&i| anchors[i].1.contains(&o))
                    .collect(),
                None => active.clone(),
            };
            if now != active {
                active.iter().for_each(|_| out.push(CLOSE));
                for &i in &now {
                    out.push(OPEN_S);
                    out.push_str(&anchors[i].0);
                    out.push(OPEN_E);
                }
                active = now;
            }
            out.push(c);
        }
        if active.is_empty() && !out.contains(OPEN_S) {
            continue;
        }
        active.iter().for_each(|_| out.push(CLOSE));
        let mut data = node.data.borrow_mut();
        match &mut data.value {
            NodeValue::Text(t) => *t = out.into(),
            NodeValue::Code(c) => c.literal = out,
            _ => {}
        }
    }
}

fn replace_sentinels(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut it = html.chars();
    while let Some(c) = it.next() {
        match c {
            OPEN_S => {
                out.push_str("<mark data-cid=\"");
                for c in it.by_ref() {
                    if c == OPEN_E {
                        break;
                    }
                    out.push(c);
                }
                out.push_str("\">");
            }
            CLOSE => out.push_str("</mark>"),
            c => out.push(c),
        }
    }
    out
}
