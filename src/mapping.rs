//! Rendered-view selection -> raw source byte range. Owned by the render/mapping Worker.
//!
//! Algorithm:
//! 1. Strip markers (`render::strip`) and re-parse `clean` with the render options.
//! 2. Find the node whose sourcepos equals `sel.sourcepos` (any node with `data-sourcepos`
//!    works, block or inline; the first in document order wins).
//! 3. Rebuild that element's text (≈ `textContent`) from its descendants; every char from a
//!    Text/Code node knows the clean byte it came from (comrak's inline sourcepos is precise,
//!    plus an escape/entity-aware alignment inside each text node). Soft/hard breaks and block
//!    boundaries contribute `\n` with no source.
//! 4. Find `sel.text` in it, whitespace-insensitively (any whitespace run matches any whitespace
//!    run; leading/trailing whitespace of the selection is ignored), choosing the occurrence
//!    whose UTF-16 start is closest to `sel.offset`.
//! 5. Range = clean byte of the first selected char .. end of the last selected char, mapped
//!    to raw through `raw_offset`.
//!
//! Anchoring rule for markup at the edges: the range always starts at the first selected
//! *rendered* char and ends after the last one, so it hugs the text and lies *inside*
//! surrounding markup: selecting the bold word in `**bold**` gives `**<!--c-->bold<!--/c-->**`.
//! Markup crossed in the middle (`a **b** c`) is simply included. Because the range is made of
//! char positions, it can never split a marker; a selection that exactly covers an existing
//! anchor's text lands inside that anchor's markers (properly nested).

use crate::protocol::Selection;
use crate::render::{
    CharMap, char_map, entity_len, line_starts, node_key, options, sourcepos_range, strip,
};
use comrak::nodes::{AstNode, NodeValue, Sourcepos};
use comrak::{Arena, parse_document};
use std::ops::Range;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MappingError {
    #[error("bad sourcepos: {0}")]
    BadSourcepos(String),
    #[error("selected text not found in source")]
    NotFound,
    #[error("selection spans multiple blocks")]
    CrossBlock,
    /// The selection is inside a fenced/indented code block. Inline markers there would be
    /// literal text, so the caller should create a line comment (`line:FROM-TO`, 0-based
    /// lines of the code content, as in the format) instead.
    #[error("selection is inside a code block (lines {from_line}-{to_line})")]
    InCodeBlock {
        /// Raw byte range of the whole code block (fences included).
        block: Range<usize>,
        from_line: usize,
        to_line: usize,
    },
}

/// One char of the reconstructed element text.
#[derive(Debug, Clone)]
struct RChar {
    c: char,
    clean: Option<usize>,
    /// Index of the leaf block (paragraph, heading, cell, code block) it belongs to.
    leaf: Option<usize>,
    /// 0-based line within a code block's content, for code-block chars.
    code_line: Option<usize>,
    /// UTF-16 offset of this char in the element text.
    u16: usize,
    /// Clean range of the enclosing code span, if any.
    code_span: Option<Range<usize>>,
}

struct Ctx<'c> {
    clean: &'c str,
    starts: Vec<usize>,
    out: Vec<RChar>,
    leaves: Vec<Range<usize>>,
    u16: usize,
    map: CharMap,
}

impl Ctx<'_> {
    fn push(&mut self, c: char, clean: Option<usize>, leaf: Option<usize>, line: Option<usize>) {
        self.out.push(RChar {
            c,
            clean,
            leaf,
            code_line: line,
            u16: self.u16,
            code_span: None,
        });
        self.u16 += c.len_utf16();
    }
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

fn is_leaf_block(v: &NodeValue) -> bool {
    matches!(
        v,
        NodeValue::Paragraph
            | NodeValue::Heading(_)
            | NodeValue::TableCell
            | NodeValue::CodeBlock(_)
            | NodeValue::HtmlBlock(_)
    )
}

fn collect<'a>(node: &'a AstNode<'a>, leaf: Option<usize>, ctx: &mut Ctx<'_>) {
    let value = node.data.borrow().value.clone();
    let mut leaf = leaf;
    if leaf.is_none() && is_leaf_block(&value) {
        // Separate leaf blocks like the `\n` between block tags in the HTML.
        if !ctx.out.is_empty() {
            ctx.push('\n', None, None, None);
        }
        let r = sourcepos_range(&node.data.borrow().sourcepos, &ctx.starts, ctx.clean.len());
        ctx.leaves.push(r);
        leaf = Some(ctx.leaves.len() - 1);
    }
    match &value {
        NodeValue::Text(_) | NodeValue::Code(_) => {
            let nc = ctx.map.get(&node_key(node)).cloned().unwrap_or_default();
            let is_code = matches!(value, NodeValue::Code(_));
            for (c, off) in nc.chars {
                ctx.push(c, off, leaf, None);
                if is_code {
                    ctx.out.last_mut().unwrap().code_span = Some(nc.range.clone());
                }
            }
        }
        NodeValue::SoftBreak | NodeValue::LineBreak => ctx.push('\n', None, leaf, None),
        NodeValue::CodeBlock(cb) => {
            for (line, text) in cb.literal.split('\n').enumerate() {
                if line > 0 {
                    ctx.push('\n', None, leaf, Some(line - 1));
                }
                for c in text.chars() {
                    ctx.push(c, None, leaf, Some(line));
                }
            }
        }
        // Not part of textContent: alt text lives in an attribute, raw HTML is omitted.
        NodeValue::Image(_) | NodeValue::HtmlInline(_) | NodeValue::HtmlBlock(_) => {}
        _ => {
            for child in node.children() {
                collect(child, leaf, ctx);
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tok {
    Ws,
    Ch(char),
}

/// Whitespace-collapsed tokens with (first, last) index into the source sequence.
fn tokens(chars: impl Iterator<Item = char>) -> Vec<(Tok, usize, usize)> {
    let mut out: Vec<(Tok, usize, usize)> = Vec::new();
    for (i, c) in chars.enumerate() {
        if c.is_whitespace() {
            if let Some(last) = out.last_mut()
                && last.0 == Tok::Ws
            {
                last.2 = i;
                continue;
            }
            out.push((Tok::Ws, i, i));
        } else {
            out.push((Tok::Ch(c), i, i));
        }
    }
    out
}

/// Map a rendered-view selection to a raw byte range suitable for inserting markers.
pub fn map_selection(raw: &str, sel: &Selection) -> Result<Range<usize>, MappingError> {
    let bad = || MappingError::BadSourcepos(sel.sourcepos.clone());
    let sp = parse_sourcepos(&sel.sourcepos).ok_or_else(bad)?;
    let (clean, raw_offset, _) = strip(raw);
    let opts = options();
    let arena = Arena::new();
    let root = parse_document(&arena, &clean, &opts);
    let target = root
        .descendants()
        .skip(1)
        .find(|n| n.data.borrow().sourcepos == sp)
        .ok_or_else(bad)?;

    let starts = line_starts(&clean);
    let mut ctx = Ctx {
        clean: &clean,
        map: char_map(root, &clean, &starts),
        starts,
        out: Vec::new(),
        leaves: Vec::new(),
        u16: 0,
    };
    // When the target is inside a leaf block (e.g. a <strong>), inherit that leaf.
    let inherited = target
        .ancestors()
        .skip(1)
        .find(|a| is_leaf_block(&a.data.borrow().value))
        .map(|a| {
            let r = sourcepos_range(&a.data.borrow().sourcepos, &ctx.starts, clean.len());
            ctx.leaves.push(r);
            0
        });
    collect(target, inherited, &mut ctx);
    let rendered = ctx.out;

    // Trim the selection; shift the offset by the trimmed leading part.
    let trimmed = sel.text.trim_start();
    let lead_u16: usize = sel.text[..sel.text.len() - trimmed.len()]
        .chars()
        .map(char::len_utf16)
        .sum();
    let needle_text = trimmed.trim_end();
    if needle_text.is_empty() {
        return Err(MappingError::NotFound);
    }
    let target_u16 = sel.offset + lead_u16;
    let hay = tokens(rendered.iter().map(|r| r.c));
    let needle = tokens(needle_text.chars());

    let mut best: Option<(usize, usize, usize)> = None; // (distance, first, last)
    if needle.len() <= hay.len() {
        for s in 0..=hay.len() - needle.len() {
            if hay[s..s + needle.len()]
                .iter()
                .zip(&needle)
                .all(|(h, n)| h.0 == n.0)
            {
                let first = hay[s].1;
                let last = hay[s + needle.len() - 1].2;
                let d = rendered[first].u16.abs_diff(target_u16);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, first, last));
                }
            }
        }
    }
    let Some((_, first, last)) = best else {
        let leaves: std::collections::HashSet<_> = rendered.iter().filter_map(|r| r.leaf).collect();
        return Err(if leaves.len() > 1 && needle_text.contains('\n') {
            MappingError::CrossBlock
        } else {
            MappingError::NotFound
        });
    };
    let span = &rendered[first..=last];
    let leaf = span[0].leaf;
    if span.iter().any(|r| r.leaf.is_some() && r.leaf != leaf) {
        return Err(MappingError::CrossBlock);
    }
    if let Some(from_line) = span[0].code_line {
        let to_line = span[span.len() - 1].code_line.unwrap_or(from_line);
        let block = leaf.map(|l| ctx.leaves[l].clone()).unwrap_or(0..0);
        return Err(MappingError::InCodeBlock {
            block: raw_offset[block.start]..raw_end(&raw_offset, raw, block.end),
            from_line,
            to_line,
        });
    }
    let first_c = span
        .iter()
        .find(|r| r.clean.is_some())
        .ok_or(MappingError::NotFound)?;
    let last_c = span
        .iter()
        .rev()
        .find(|r| r.clean.is_some())
        .ok_or(MappingError::NotFound)?;
    // Code spans are atomic: a marker inside backticks would be literal text, so an edge that
    // falls inside a code span expands to the whole span (backticks included).
    let start = match &first_c.code_span {
        Some(r) => r.start,
        None => first_c.clean.unwrap(),
    };
    let end = match &last_c.code_span {
        Some(r) => r.end,
        None => source_char_end(&clean, last_c.clean.unwrap(), last_c.c),
    };
    Ok(raw_offset[start]..raw_end(&raw_offset, raw, end))
}

/// End (exclusive) of the source construct that produced a rendered char at `off`:
/// a plain char, a backslash escape, or an entity.
fn source_char_end(clean: &str, off: usize, rendered: char) -> usize {
    let rest = &clean[off..];
    if let Some(n) = entity_len(rest) {
        return off + n;
    }
    let mut it = rest.chars();
    match it.next() {
        Some('\\') if rendered != '\\' || rest.starts_with("\\\\") => {
            off + 1 + it.next().map_or(0, char::len_utf8)
        }
        Some(c) => off + c.len_utf8(),
        None => off,
    }
}

/// Raw end offset for a clean exclusive end (end of the last kept byte).
fn raw_end(raw_offset: &[usize], raw: &str, clean_end: usize) -> usize {
    if clean_end == 0 {
        0
    } else {
        (raw_offset[clean_end - 1] + 1).min(raw.len())
    }
}
