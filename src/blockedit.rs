//! Edit mode (WYSIWYG in the rendered view): per-block edits without re-serializing the file.
//!
//! The browser serializes one edited block back to Markdown (inline content only) and sends
//! `ReplaceBlock { sourcepos, markdown }` / `MergeBlocks { first, second, markdown }`. Here:
//!
//! 1. `sourcepos` (positions in the *clean* text, as rendered) is resolved to a block node of
//!    the re-parsed clean text: a paragraph, a heading, a table cell or a list item (whose first
//!    paragraph is edited).
//! 2. The block's *content* clean range (after the prefix: heading `#`s, list marker,
//!    checkbox, blockquote `>`; before an ATX closing sequence / setext underline) is mapped to
//!    raw bytes through `raw_offset`, then widened to include the markers of every comment
//!    anchored entirely inside the block. Comment bodies are never included; a comment that
//!    straddles the block boundary makes the block uneditable here (use Source mode).
//! 3. That raw range is replaced by the first chunk of `markdown`; continuation lines get the
//!    original continuation prefixes (`> `, list indentation). Further chunks (separated by a
//!    blank line) become new blocks inserted after the block and its comment bodies: new list
//!    items for list items, new paragraphs otherwise.
//! 4. Marker balance is validated: every comment id in the replaced text must be balanced
//!    (open before close) or entirely absent, and no foreign id may appear.
//!
//! Every byte outside the replaced range (and the insertion point) is left untouched.

use crate::format::parse_comments;
use crate::render::{
    Marker, MarkerKind, line_starts, options, scan_markers, sourcepos_range, strip,
};
use comrak::nodes::{AstNode, NodeValue, Sourcepos};
use comrak::{Arena, parse_document};
use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

static OPEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--c:([A-Za-z0-9]+)-->").unwrap());
static CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--/c:([A-Za-z0-9]+)-->").unwrap());
static ITEM_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.*?)([-*+]|[0-9]{1,9}[.)])([ \t]+)(\[[ xX]\][ \t]+)?$").unwrap()
});
static TASK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[([ xX])\](?:[ \t]+|$)").unwrap());

const STALE: &str =
    "This block no longer exists in the file (the view may be stale); reload and try again.";

/// Replace the content of the block at `sourcepos` with `markdown` (see the module docs).
pub fn replace_block(raw: &str, sourcepos: &str, markdown: &str) -> Result<String, String> {
    let doc = Doc::new(raw);
    let arena = Arena::new();
    let root = parse_document(&arena, &doc.clean, &options());
    let (_, t) = doc.target(root, sourcepos)?;
    let edit = t.build(markdown)?;
    let mut out = String::with_capacity(raw.len() + markdown.len());
    out.push_str(&raw[..t.range.start]);
    out.push_str(&edit.first);
    out.push_str(&raw[t.range.end..t.insert_at]);
    out.push_str(&edit.extra);
    out.push_str(&raw[t.insert_at..]);
    let out = apply_task(out, raw, &t, edit.checked);
    verify(&out, &t.owned, &edit)?;
    Ok(out)
}

/// Merge the block `second` into the block `first` (Backspace at the start of `second`):
/// `first`'s content becomes `markdown` (the browser's serialization of the merged block) and
/// `second`'s lines up to the end of its content are removed. Comment bodies that followed
/// `first` stay where they are; anything after `second`'s content (its bodies, nested lists)
/// is kept. The blocks must be adjacent (no other block in between) and not table cells.
pub fn merge_blocks(
    raw: &str,
    first: &str,
    second: &str,
    markdown: &str,
) -> Result<String, String> {
    let doc = Doc::new(raw);
    let arena = Arena::new();
    let root = parse_document(&arena, &doc.clean, &options());
    let (n1, t1) = doc.target(root, first)?;
    let (n2, t2) = doc.target(root, second)?;
    if t1.kind == Kind::Cell || t2.kind == Kind::Cell {
        return Err("Table cells can't be merged.".into());
    }
    if t2.clean.start <= t1.clean.end || t2.range.start < t1.after_content {
        return Err("The blocks to merge are not in order; reload and try again.".into());
    }
    // Adjacent: no other leaf block starts between the two.
    let between = root.descendants().any(|n| {
        let d = n.data.borrow();
        let leaf = matches!(
            d.value,
            NodeValue::Paragraph
                | NodeValue::Heading(_)
                | NodeValue::CodeBlock(_)
                | NodeValue::HtmlBlock(_)
                | NodeValue::Table(_)
                | NodeValue::ThematicBreak
        );
        if !leaf || n.ancestors().any(|a| same(a, n1) || same(a, n2)) {
            return false;
        }
        let s = sourcepos_range(&d.sourcepos, &doc.starts, doc.clean.len()).start;
        s > t1.clean.end && s < t2.clean.start
    });
    if between {
        return Err("Only adjacent blocks can be merged.".into());
    }
    let gap = t1.after_content..t2.range.start;
    if doc.markers.iter().any(|m| overlaps(&m.range, &gap)) {
        return Err("A comment sits between these blocks; merge them in Source mode.".into());
    }
    let mut owned = t1.owned.clone();
    owned.extend(t2.owned.iter().cloned());
    let edit = t1.build(markdown)?;
    let mut out = String::with_capacity(raw.len() + markdown.len());
    out.push_str(&raw[..t1.range.start]);
    out.push_str(&edit.first);
    out.push_str(&raw[t1.range.end..t1.after_content]);
    out.push_str(&edit.extra);
    out.push_str(&raw[t2.range.end..]);
    let out = apply_task(out, raw, &t1, edit.checked);
    verify(&out, &owned, &edit)?;
    Ok(out)
}

/// Raw byte range that `replace_block` would replace for `sourcepos` (for tests and tools).
pub fn block_range(raw: &str, sourcepos: &str) -> Result<Range<usize>, String> {
    let doc = Doc::new(raw);
    let arena = Arena::new();
    let root = parse_document(&arena, &doc.clean, &options());
    doc.target(root, sourcepos).map(|(_, t)| t.range)
}

/// `data-sourcepos` of every block the UI makes editable, in document order: paragraphs
/// (except the text of tight list items), headings, table cells, tight list items with text
/// (rendered without `<p>`, so the UI sends the `<li>`'s sourcepos).
pub fn editable_blocks(raw: &str) -> Vec<String> {
    let doc = Doc::new(raw);
    let arena = Arena::new();
    let root = parse_document(&arena, &doc.clean, &options());
    root.descendants()
        .filter(|n| {
            let d = n.data.borrow();
            match d.value {
                NodeValue::Heading(_) | NodeValue::TableCell => true,
                NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                    tight_item(n)
                        && n.first_child()
                            .is_some_and(|c| matches!(c.data.borrow().value, NodeValue::Paragraph))
                }
                NodeValue::Paragraph => !is_item_text(n) || !n.parent().is_some_and(tight_item),
                _ => false,
            }
        })
        .map(|n| n.data.borrow().sourcepos.to_string())
        .collect()
}

// ---------- internals ----------

fn same<'a>(a: &AstNode<'a>, b: &AstNode<'a>) -> bool {
    std::ptr::eq(a, b)
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// A list item of a tight list (its paragraphs render without `<p>`).
fn tight_item<'a>(n: &'a AstNode<'a>) -> bool {
    n.parent()
        .is_some_and(|l| matches!(&l.data.borrow().value, NodeValue::List(nl) if nl.tight))
}

/// First paragraph of a list item (rendered without `<p>` in tight lists).
fn is_item_text<'a>(n: &'a AstNode<'a>) -> bool {
    n.parent().is_some_and(|p| {
        matches!(
            p.data.borrow().value,
            NodeValue::Item(_) | NodeValue::TaskItem(_)
        ) && p.first_child().is_some_and(|c| same(c, n))
    })
}

fn parse_sourcepos(s: &str) -> Option<Sourcepos> {
    let (a, b) = s.trim().split_once('-')?;
    let lc = |x: &str| -> Option<(usize, usize)> {
        let (l, c) = x.split_once(':')?;
        Some((l.parse().ok()?, c.parse().ok()?))
    };
    let (a, b) = (lc(a)?, lc(b)?);
    Some(Sourcepos {
        start: a.into(),
        end: b.into(),
    })
}

/// Container prefix for new/continuation lines: keep `>` and whitespace, blank out list
/// markers and checkboxes.
fn blank_markers(prefix: &str) -> String {
    prefix
        .chars()
        .map(|c| {
            if c == '>' || c == ' ' || c == '\t' {
                c
            } else {
                ' '
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Paragraph,
    Heading { atx: bool },
    Cell,
}

#[derive(Debug, Clone)]
enum Split {
    /// New chunks become paragraphs with this continuation prefix.
    Paragraph,
    /// New chunks become sibling list items.
    Item {
        lead: String,
        marker: String,
        space: String,
        task: Option<String>,
        tight: bool,
    },
}

#[derive(Debug)]
struct Target {
    kind: Kind,
    /// Raw range replaced by the first chunk (owned markers included).
    range: Range<usize>,
    /// Clean content range.
    clean: Range<usize>,
    /// Comments anchored entirely inside the block.
    owned: Vec<String>,
    /// Original prefixes of the continuation lines inside `range`.
    cont_prefixes: Vec<String>,
    /// Prefix for new continuation lines.
    cont: String,
    split: Split,
    /// Raw insertion point for new blocks (after the block or list item and its comment bodies).
    insert_at: usize,
    /// Raw position after the block's own content line(s) and their comment bodies.
    after_content: usize,
    /// Raw byte of the task checkbox symbol and its current value.
    task: Option<(usize, char)>,
    /// Empty ATX heading: a `#` right before / after the insertion point needs a space.
    pad: (bool, bool),
}

struct Edit {
    first: String,
    extra: String,
    /// New checkbox state (task items), if the markdown carried one.
    checked: Option<bool>,
}

struct Doc<'r> {
    raw: &'r str,
    clean: String,
    raw_offset: Vec<usize>,
    kept: Vec<bool>,
    anchors: Vec<(String, Range<usize>)>,
    markers: Vec<Marker>,
    starts: Vec<usize>,
}

impl<'r> Doc<'r> {
    fn new(raw: &'r str) -> Self {
        let (clean, raw_offset, anchors) = strip(raw);
        let mut kept = vec![false; raw.len()];
        for &o in &raw_offset[..clean.len()] {
            kept[o] = true;
        }
        let starts = line_starts(&clean);
        Doc {
            raw,
            markers: scan_markers(raw),
            clean,
            raw_offset,
            kept,
            anchors,
            starts,
        }
    }

    fn range(&self, sp: &Sourcepos) -> Range<usize> {
        sourcepos_range(sp, &self.starts, self.clean.len())
    }

    /// Raw offset just after clean range `[cs, ce)` (start of it when empty).
    fn raw_range(&self, r: &Range<usize>) -> Range<usize> {
        let s = self.raw_offset[r.start];
        let e = if r.end > r.start {
            self.raw_offset[r.end - 1] + 1
        } else {
            s
        };
        s..e
    }

    fn trim_end(&self, r: Range<usize>) -> Range<usize> {
        let t = self.clean[r.clone()].trim_end().len();
        r.start..r.start + t
    }

    /// Insertion point after raw position `p` (end of a block's content): past trailing
    /// markers/spaces on the line, then past the comment bodies / marker-only lines that follow,
    /// stopping before their last newline.
    fn after(&self, mut p: usize) -> usize {
        let (raw, len) = (self.raw.as_bytes(), self.raw.len());
        while p < len && (!self.kept[p] || raw[p] == b' ' || raw[p] == b'\t' || raw[p] == b'\r') {
            p += 1;
        }
        if p >= len || raw[p] != b'\n' {
            return p.min(len);
        }
        let q = p + 1;
        let mut r = q;
        while r < len && !self.kept[r] {
            r += 1;
        }
        if r == q {
            p
        } else if raw[r - 1] == b'\n' {
            r - 1
        } else {
            r
        }
    }

    /// The close marker of `id` sits alone on the line right after the content (the anchor
    /// then ends at the start of the following line in clean text).
    fn own_line_close(&self, id: &str, clean: &Range<usize>, end: usize) -> bool {
        let gap = &self.clean[clean.end..end];
        if !gap.trim().is_empty() || gap.matches('\n').count() > 1 {
            return false;
        }
        let Some(m) = self
            .markers
            .iter()
            .find(|m| m.kind == MarkerKind::Close && m.id == id)
        else {
            return false;
        };
        let b = self.raw.as_bytes();
        let ls = self.raw[..m.range.start].rfind('\n').map_or(0, |i| i + 1);
        let le = self.raw[m.range.end..]
            .find('\n')
            .map_or(b.len(), |i| m.range.end + i);
        (ls..le).all(|i| !self.kept[i])
    }

    fn target<'a>(
        &self,
        root: &'a AstNode<'a>,
        sourcepos: &str,
    ) -> Result<(&'a AstNode<'a>, Target), String> {
        let sp = parse_sourcepos(sourcepos).ok_or(STALE)?;
        let node = root
            .descendants()
            .find(|n| {
                let d = n.data.borrow();
                d.sourcepos == sp
                    && matches!(
                        d.value,
                        NodeValue::Paragraph
                            | NodeValue::Heading(_)
                            | NodeValue::TableCell
                            | NodeValue::Item(_)
                            | NodeValue::TaskItem(_)
                    )
            })
            .ok_or(STALE)?;
        let node = match node.data.borrow().value {
            NodeValue::Item(_) | NodeValue::TaskItem(_) => node
                .first_child()
                .filter(|c| matches!(c.data.borrow().value, NodeValue::Paragraph))
                .ok_or("This list item has no text to edit here; use Source mode.")?,
            _ => node,
        };
        let value = node.data.borrow().value.clone();
        let block = self.range(&node.data.borrow().sourcepos);
        let (kind, clean) = match &value {
            NodeValue::Paragraph => (Kind::Paragraph, self.trim_end(block)),
            NodeValue::Heading(h) if h.setext => {
                // Content = every line but the underline.
                let text = &self.clean[block.clone()];
                let end = text
                    .trim_end()
                    .rfind('\n')
                    .map_or(block.start, |i| block.start + i);
                let lead = self.clean[block.start..end].len()
                    - self.clean[block.start..end].trim_start().len();
                (
                    Kind::Heading { atx: false },
                    self.trim_end(block.start + lead..end),
                )
            }
            NodeValue::Heading(_) => {
                let text = &self.clean[block.clone()];
                let lead = text.len() - text.trim_start_matches([' ', '\t']).len();
                let hashes = text[lead..].len() - text[lead..].trim_start_matches('#').len();
                let after = &text[lead + hashes..];
                let ws = after.len() - after.trim_start_matches([' ', '\t']).len();
                let cs = block.start + lead + hashes + ws;
                let mut content = self.clean[cs..block.end].trim_end();
                // Optional closing sequence: spaces + `#`s (or nothing but `#`s).
                let no_hash = content.trim_end_matches('#');
                if no_hash.len() < content.len()
                    && (no_hash.is_empty() || no_hash.ends_with([' ', '\t']))
                {
                    content = no_hash.trim_end();
                }
                (Kind::Heading { atx: true }, cs..cs + content.len())
            }
            NodeValue::TableCell => {
                let text = &self.clean[block.clone()];
                let t = text.trim();
                let r = if t.is_empty() {
                    let at = block.start + usize::from(!text.is_empty());
                    at..at
                } else {
                    let s = block.start + (text.len() - text.trim_start().len());
                    s..s + t.len()
                };
                (Kind::Cell, r)
            }
            _ => return Err(STALE.into()),
        };

        // Comments anchored inside the content; a comment crossing its boundary is refused.
        let mut owned = Vec::new();
        for (id, a) in &self.anchors {
            let within = a.start >= clean.start
                && (a.end <= clean.end || self.own_line_close(id, &clean, a.end));
            if within {
                owned.push(id.clone());
            } else if a.start < clean.end && a.end > clean.start {
                return Err("This block is part of a comment that spans several blocks; edit it in Source mode.".into());
            }
        }
        let content_raw = self.raw_range(&clean);
        let mut range = content_raw.clone();
        for m in &self.markers {
            if owned.contains(&m.id) && m.kind != MarkerKind::Body {
                range.start = range.start.min(m.range.start);
                range.end = range.end.max(m.range.end);
            }
        }
        for m in &self.markers {
            if overlaps(&m.range, &range) && (m.kind == MarkerKind::Body || !owned.contains(&m.id))
            {
                return Err(
                    "This block overlaps a comment that can't be kept; edit it in Source mode."
                        .into(),
                );
            }
        }

        // Prefixes from the raw lines.
        let raw = self.raw;
        let ls = raw[..content_raw.start].rfind('\n').map_or(0, |i| i + 1);
        let mut first_prefix = String::new();
        let mut i = ls;
        for m in self
            .markers
            .iter()
            .filter(|m| m.range.start >= ls && m.range.end <= content_raw.start)
        {
            if m.range.start >= i {
                first_prefix.push_str(&raw[i..m.range.start]);
                i = m.range.end;
            }
        }
        if i < content_raw.start {
            first_prefix.push_str(&raw[i..content_raw.start]);
        }
        let cont_prefixes: Vec<String> = raw[range.clone()]
            .split('\n')
            .skip(1)
            .map(|l| l[..l.len() - l.trim_start_matches([' ', '\t', '>']).len()].to_string())
            .collect();
        let container = match kind {
            Kind::Heading { atx: true } => first_prefix
                .trim_end_matches([' ', '\t'])
                .trim_end_matches('#')
                .to_string(),
            _ => first_prefix.clone(),
        };
        let cont = cont_prefixes
            .last()
            .cloned()
            .unwrap_or_else(|| blank_markers(&container));

        let parent = node.parent();
        let item_text = is_item_text(node);
        let mut split = Split::Paragraph;
        let mut task = None;
        let mut insert_end = range.end;
        if item_text && let Some(item) = parent {
            if let NodeValue::TaskItem(t) = &item.data.borrow().value {
                let r = self.range(&t.symbol_sourcepos);
                task = Some((self.raw_offset[r.start], t.symbol.unwrap_or(' ')));
            }
            let tight = item.parent().is_some_and(|l| match &l.data.borrow().value {
                NodeValue::List(nl) => nl.tight,
                _ => true,
            });
            if let Some(c) = ITEM_PREFIX_RE.captures(&first_prefix) {
                let marker = &c[2];
                let marker = match marker.find(['.', ')']) {
                    Some(i) => match marker[..i].parse::<u64>() {
                        Ok(n) => format!("{}{}", n + 1, &marker[i..]),
                        Err(_) => marker.to_string(),
                    },
                    None => marker.to_string(),
                };
                split = Split::Item {
                    lead: blank_markers(&c[1]),
                    marker,
                    space: c[3].to_string(),
                    task: c.get(4).map(|t| format!("[ ]{}", &t.as_str()[3..])),
                    tight,
                };
                let ir = self.trim_end(self.range(&item.data.borrow().sourcepos));
                insert_end = insert_end.max(self.raw_range(&ir).end);
            }
        }
        let b = raw.as_bytes();
        let pad = (
            range.start > 0 && b[range.start - 1] == b'#',
            b.get(range.start) == Some(&b'#'),
        );
        let insert_at = self.after(insert_end);
        let after_content = self.after(range.end);
        Ok((
            node,
            Target {
                kind,
                range,
                clean,
                owned,
                cont_prefixes,
                cont,
                split,
                insert_at,
                after_content,
                task,
                pad,
            },
        ))
    }
}

impl Target {
    fn join(&self, lines: &[&str], originals: &[String], cont: &str) -> String {
        if matches!(self.kind, Kind::Heading { atx: true } | Kind::Cell) {
            return lines.join(" ");
        }
        let mut s = String::new();
        for (i, l) in lines.iter().enumerate() {
            if i > 0 {
                s.push('\n');
                s.push_str(originals.get(i - 1).map_or(cont, String::as_str));
            }
            s.push_str(l);
        }
        s
    }

    fn build(&self, markdown: &str) -> Result<Edit, String> {
        let md = markdown.replace("\r\n", "\n");
        let mut chunks: Vec<Vec<&str>> = vec![Vec::new()];
        for line in md.split('\n') {
            if line.trim().is_empty() {
                if !chunks.last().is_some_and(Vec::is_empty) {
                    chunks.push(Vec::new());
                }
            } else {
                chunks.last_mut().unwrap().push(line);
            }
        }
        if chunks.len() > 1 && chunks.last().is_some_and(Vec::is_empty) {
            chunks.pop();
        }
        if chunks[0].is_empty() && chunks.len() > 1 {
            chunks.remove(0);
        }
        if self.kind == Kind::Cell {
            if chunks.len() > 1 {
                return Err("A table cell holds a single line of text.".into());
            }
            let s = chunks[0].join(" ");
            let b = s.as_bytes();
            if (0..b.len()).any(|i| b[i] == b'|' && (i == 0 || b[i - 1] != b'\\')) {
                return Err("A `|` in a table cell must be escaped as `\\|`.".into());
            }
        }
        let mut checked = None;
        if self.task.is_some()
            && let Some(first) = chunks[0].first_mut()
            && let Some(c) = TASK_RE.captures(first)
        {
            checked = Some(&c[1] != " ");
            *first = &first[c[0].len()..];
        }
        let first = self.join(&chunks[0], &self.cont_prefixes, &self.cont);
        let blank = self.cont.trim_end().to_string();
        let mut extra = String::new();
        for (k, lines) in chunks.iter().enumerate().skip(1) {
            match &self.split {
                Split::Paragraph => {
                    extra.push('\n');
                    extra.push_str(&blank);
                    extra.push('\n');
                    extra.push_str(&self.cont);
                    extra.push_str(&self.join(lines, &[], &self.cont));
                }
                Split::Item {
                    lead,
                    marker,
                    space,
                    task,
                    tight,
                } => {
                    let marker = bump(marker, k - 1);
                    let head = format!("{lead}{marker}{space}{}", task.as_deref().unwrap_or(""));
                    let cont = blank_markers(&head);
                    extra.push('\n');
                    if !tight {
                        extra.push_str(lead.trim_end());
                        extra.push('\n');
                    }
                    extra.push_str(&head);
                    extra.push_str(&self.join(lines, &[], &cont));
                }
            }
        }
        // An empty cell keeps a space before the closing pipe; an empty ATX heading (`#`)
        // needs one after the hashes.
        let first = if first.is_empty() || !self.clean.is_empty() {
            first
        } else if self.kind == Kind::Cell {
            format!("{first} ")
        } else if self.kind == (Kind::Heading { atx: true }) {
            let (before, after) = self.pad;
            format!(
                "{}{first}{}",
                if before { " " } else { "" },
                if after { " " } else { "" }
            )
        } else {
            first
        };
        Ok(Edit {
            first,
            extra,
            checked,
        })
    }
}

/// `3.` + 2 -> `5.`; bullets unchanged.
fn bump(marker: &str, by: usize) -> String {
    match marker.find(['.', ')']) {
        Some(i) => match marker[..i].parse::<usize>() {
            Ok(n) => format!("{}{}", n + by, &marker[i..]),
            Err(_) => marker.to_string(),
        },
        None => marker.to_string(),
    }
}

/// Toggle the task checkbox symbol (a byte before the replaced range, so offsets still hold).
fn apply_task(mut out: String, raw: &str, t: &Target, checked: Option<bool>) -> String {
    if let (Some((at, cur)), Some(want)) = (t.task, checked)
        && (cur != ' ') != want
        && at < t.range.start
        && raw.as_bytes()[at] == cur as u8
    {
        out.replace_range(at..at + 1, if want { "x" } else { " " });
    }
    out
}

/// Marker balance: every id in the new text is owned, appears at most once as open and close,
/// open before close, and still parses as an anchor in the result; bodies can't be injected.
fn verify(out: &str, owned: &[String], edit: &Edit) -> Result<(), String> {
    let text = format!("{}\n{}", edit.first, edit.extra);
    if text.contains("<!--co:") {
        return Err("The edited text contains a comment body; edit it in Source mode.".into());
    }
    let opens: Vec<(String, usize)> = OPEN_RE
        .captures_iter(&text)
        .map(|c| (c[1].to_string(), c.get(0).unwrap().start()))
        .collect();
    let closes: Vec<(String, usize)> = CLOSE_RE
        .captures_iter(&text)
        .map(|c| (c[1].to_string(), c.get(0).unwrap().start()))
        .collect();
    let bad = |id: &str| {
        Err(format!(
            "Comment markers for `{id}` would be unbalanced; edit not applied."
        ))
    };
    let parsed = parse_comments(out);
    for id in opens.iter().chain(&closes).map(|(id, _)| id) {
        if !owned.contains(id) {
            return bad(id);
        }
    }
    for id in owned {
        let o: Vec<usize> = opens.iter().filter(|(i, _)| i == id).map(|x| x.1).collect();
        let c: Vec<usize> = closes
            .iter()
            .filter(|(i, _)| i == id)
            .map(|x| x.1)
            .collect();
        let anchored = parsed
            .iter()
            .find(|p| &p.id == id)
            .is_some_and(|p| p.anchor_range().is_some());
        let ok = match (o.as_slice(), c.as_slice()) {
            ([], []) => !anchored,
            ([a], [b]) => a < b && anchored,
            _ => false,
        };
        if !ok {
            return bad(id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Sourcepos of the n-th editable block.
    fn sp(raw: &str, n: usize) -> String {
        editable_blocks(raw)[n].clone()
    }

    fn rep(raw: &str, n: usize, md: &str) -> String {
        replace_block(raw, &sp(raw, n), md).unwrap()
    }

    #[test]
    fn paragraph_and_heading() {
        let raw = "# Title #\n\nHello *world*.\nSecond line\n\nSetext\ntwo\n---\n";
        assert_eq!(editable_blocks(raw), ["1:1-1:9", "3:1-4:11", "6:1-8:3"]);
        assert_eq!(&raw[block_range(raw, "1:1-1:9").unwrap()], "Title");
        assert_eq!(
            rep(raw, 0, "New **t**"),
            raw.replace("# Title #", "# New **t** #")
        );
        assert_eq!(
            rep(raw, 1, "Hi\nthere"),
            raw.replace("Hello *world*.\nSecond line", "Hi\nthere")
        );
        assert_eq!(rep(raw, 2, "S\nT"), raw.replace("Setext\ntwo", "S\nT"));
        // Newlines in an ATX heading become spaces.
        assert_eq!(rep(raw, 0, "a\nb"), raw.replace("# Title #", "# a b #"));
        // Empty heading content.
        assert_eq!(rep("#\n", 0, "x"), "# x\n");
        assert_eq!(rep("## #\n", 0, "x"), "## x #\n");
    }

    #[test]
    fn nested_list_items_and_blockquotes() {
        let raw = "- a\n- [ ] task *t*\n  cont\n  - nested\n    more\n\n> q1\n> q2\n>\n> > deep\n";
        let blocks = editable_blocks(raw);
        assert_eq!(
            blocks,
            ["1:1-1:3", "2:1-6:0", "4:3-5:8", "7:3-8:4", "10:5-10:8"]
        );
        assert_eq!(
            &raw[block_range(raw, &blocks[1]).unwrap()],
            "task *t*\n  cont"
        );
        assert_eq!(
            rep(raw, 1, "[ ] T\nC"),
            raw.replace("task *t*\n  cont", "T\n  C")
        );
        // Checkbox toggles the symbol byte only.
        assert_eq!(rep(raw, 1, "[x] task *t*\ncont"), raw.replace("[ ]", "[x]"));
        assert_eq!(
            rep(raw, 2, "N\nM"),
            raw.replace("nested\n    more", "N\n    M")
        );
        assert_eq!(
            rep(raw, 3, "x\ny\nz"),
            raw.replace("q1\n> q2", "x\n> y\n> z")
        );
        assert_eq!(rep(raw, 4, "D"), raw.replace("deep", "D"));
    }

    #[test]
    fn table_cells() {
        let raw = "| a | b \\| c |\n|---|---|\n| x | |\n";
        let blocks = editable_blocks(raw);
        assert_eq!(blocks.len(), 4);
        assert_eq!(&raw[block_range(raw, &blocks[1]).unwrap()], "b \\| c");
        assert_eq!(rep(raw, 1, "B \\| C"), raw.replace("b \\| c", "B \\| C"));
        assert_eq!(rep(raw, 3, "y"), raw.replace("| x | |", "| x | y |"));
        assert!(replace_block(raw, &blocks[0], "a | b").is_err());
        assert!(replace_block(raw, &blocks[0], "a\n\nb").is_err());
    }

    #[test]
    fn inline_markers_and_following_body() {
        let raw = "Intro <!--c:a1-->anchored<!--/c:a1--> text.\n<!--co:a1 status:open\nme: hi\n-->\n\n<!--c:b2-->Whole<!--/c:b2-->\n<!--co:b2 status:open\nme: x\n-->\n";
        let r = block_range(raw, &sp(raw, 0)).unwrap();
        assert_eq!(&raw[r], "Intro <!--c:a1-->anchored<!--/c:a1--> text.");
        let r = block_range(raw, &sp(raw, 1)).unwrap();
        assert_eq!(&raw[r], "<!--c:b2-->Whole<!--/c:b2-->");
        let out = rep(raw, 0, "Intro <!--c:a1-->ANCHORED<!--/c:a1--> more.");
        assert_eq!(
            out,
            raw.replace("anchored<!--/c:a1--> text.", "ANCHORED<!--/c:a1--> more.")
        );
        // Deleting the anchored text drops both markers: the thread becomes orphaned.
        let out = rep(raw, 0, "Intro text.");
        assert!(out.starts_with("Intro text.\n<!--co:a1"));
        assert!(
            parse_comments(&out)
                .iter()
                .find(|c| c.id == "a1")
                .unwrap()
                .is_orphan()
        );
        // Unbalanced or foreign markers are refused.
        assert!(replace_block(raw, &sp(raw, 0), "Intro <!--c:a1-->x").is_err());
        assert!(replace_block(raw, &sp(raw, 0), "x<!--/c:a1--> <!--c:a1-->y").is_err());
        assert!(replace_block(raw, &sp(raw, 0), "<!--c:b2-->x<!--/c:b2-->").is_err());
        assert!(replace_block(raw, &sp(raw, 0), "x <!--co:zz status:open\n-->").is_err());
        // A marker inside a code span is literal text: the anchor would break.
        assert!(replace_block(raw, &sp(raw, 0), "`<!--c:a1-->x<!--/c:a1-->`").is_err());
    }

    #[test]
    fn own_line_markers_are_owned() {
        let raw = "<!--c:w1-->\nWhole paragraph\n<!--/c:w1-->\n<!--co:w1 status:open\nme: x\n-->\n\nNext\n";
        let r = block_range(raw, &sp(raw, 0)).unwrap();
        assert_eq!(&raw[r], "<!--c:w1-->\nWhole paragraph\n<!--/c:w1-->");
        let out = rep(raw, 0, "<!--c:w1-->Whole para<!--/c:w1-->");
        assert_eq!(
            out,
            "<!--c:w1-->Whole para<!--/c:w1-->\n<!--co:w1 status:open\nme: x\n-->\n\nNext\n"
        );
    }

    #[test]
    fn comment_across_blocks_is_refused() {
        let raw = "One <!--c:x1-->a\n\nb<!--/c:x1--> two\n";
        assert!(
            block_range(raw, &sp(raw, 0))
                .unwrap_err()
                .contains("several blocks")
        );
        assert!(
            block_range(raw, &sp(raw, 1))
                .unwrap_err()
                .contains("several blocks")
        );
        // A close marker inline at the start of the next block is not "own line".
        let raw = "a\n<!--/c:x1--># H\n\n<!--c:x1-->z\n";
        assert!(block_range(raw, &sp(raw, 0)).is_ok_and(|r| &raw[r] == "a"));
        // Neither is one after a blank line.
        let raw = "<!--c:x1-->a\n\n<!--/c:x1-->\n\nb\n";
        assert!(block_range(raw, &sp(raw, 0)).is_err());
    }

    #[test]
    fn non_ascii() {
        let raw = "# Ünïcode 🎉\n\nCafé <!--c:u1-->crème 🎂<!--/c:u1--> brûlée\n<!--co:u1 status:open\nme: é\n-->\n";
        assert_eq!(&raw[block_range(raw, &sp(raw, 0)).unwrap()], "Ünïcode 🎉");
        let out = rep(raw, 1, "Café <!--c:u1-->crème 🎂!<!--/c:u1--> ñ");
        assert_eq!(
            out,
            raw.replace("🎂<!--/c:u1--> brûlée", "🎂!<!--/c:u1--> ñ")
        );
    }

    #[test]
    fn split_paragraph_after_body() {
        let raw = "A <!--c:a1-->x<!--/c:a1-->\n<!--co:a1 status:open\nme: hi\n-->\n\nB\n";
        let out = rep(raw, 0, "A <!--c:a1-->x<!--/c:a1-->\n\nnew one\n\nnew two");
        assert_eq!(
            out,
            "A <!--c:a1-->x<!--/c:a1-->\n<!--co:a1 status:open\nme: hi\n-->\n\nnew one\n\nnew two\n\nB\n"
        );
        // At the end of the file, without a trailing newline.
        assert_eq!(rep("A", 0, "A\n\nB"), "A\n\nB");
        let raw = "A\n<!--co:z status:open\nme: x\n-->";
        assert_eq!(rep(raw, 0, "A\n\nB"), format!("{raw}\n\nB"));
    }

    #[test]
    fn split_list_items_and_quotes() {
        let raw = "- a\n  - n\n- b\n";
        assert_eq!(rep(raw, 0, "a\n\nc"), "- a\n  - n\n- c\n- b\n");
        let raw = "1. one\n\n2. [x] two\n";
        assert_eq!(
            rep(raw, 0, "one\n\nnew"),
            "1. one\n\n2. new\n\n2. [x] two\n"
        );
        assert_eq!(
            rep(raw, 1, "[x] two\n\nthree"),
            "1. one\n\n2. [x] two\n\n3. [ ] three\n"
        );
        let raw = "> - x\n> - y\n";
        assert_eq!(rep(raw, 0, "x\n\nz"), "> - x\n> - z\n> - y\n");
        let raw = "> p\n\nq\n";
        assert_eq!(rep(raw, 0, "p\n\nr"), "> p\n>\n> r\n\nq\n");
        let raw = "## H\n\np\n";
        assert_eq!(rep(raw, 0, "H\n\nbody"), "## H\n\nbody\n\np\n");
    }

    #[test]
    fn merge_paragraphs_keeps_bodies() {
        let raw = "One <!--c:a1-->x<!--/c:a1-->\n<!--co:a1 status:open\nme: hi\n-->\n\nTwo <!--c:b2-->y<!--/c:b2-->\n<!--co:b2 status:open\nme: yo\n-->\n\nThree\n";
        let b = editable_blocks(raw);
        let out = merge_blocks(
            raw,
            &b[0],
            &b[1],
            "One <!--c:a1-->x<!--/c:a1-->Two <!--c:b2-->y<!--/c:b2-->",
        )
        .unwrap();
        assert_eq!(
            out,
            "One <!--c:a1-->x<!--/c:a1-->Two <!--c:b2-->y<!--/c:b2-->\n<!--co:a1 status:open\nme: hi\n-->\n<!--co:b2 status:open\nme: yo\n-->\n\nThree\n"
        );
        // Not adjacent / wrong order / cells.
        assert!(merge_blocks(raw, &b[0], &b[2], "x").is_err());
        assert!(merge_blocks(raw, &b[1], &b[0], "x").is_err());
        let raw = "- a\n- b\n  - c\n";
        let b = editable_blocks(raw);
        assert_eq!(
            merge_blocks(raw, &b[0], &b[1], "ab").unwrap(),
            "- ab\n  - c\n"
        );
        let raw = "para\n\n```\ncode\n```\n\nafter\n";
        let b = editable_blocks(raw);
        assert!(merge_blocks(raw, &b[0], &b[1], "x").is_err());
        let raw = "# Head\n\nbody\nmore\n";
        let b = editable_blocks(raw);
        assert_eq!(
            merge_blocks(raw, &b[0], &b[1], "Headbody\nmore").unwrap(),
            "# Headbody more\n"
        );
    }

    #[test]
    fn stale_sourcepos() {
        assert!(replace_block("a\n", "9:1-9:3", "x").is_err());
        assert!(replace_block("a\n", "garbage", "x").is_err());
        // Code blocks are not editable blocks.
        assert!(replace_block("```\nx\n```\n", "1:1-3:3", "y").is_err());
    }
}
