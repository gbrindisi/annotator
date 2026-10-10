// Ported and reduced from kylemcd/obsidian-document-comments (MIT). See NOTICE.
// Pure string operations so they can be unit-tested without CodeMirror.

import type { CommentData, ParsedComment, Reaction, TextRange } from './types';
import { anchorRange, isInFencedCode, parseComments } from './parse';
import { closeMarker, openMarker, serializeBody } from './serialize';

/** A document edit in original coordinates (matches CodeMirror's ChangeSpec shape). */
export type Change = {
  from: number;
  to: number;
  insert: string;
};

export type EditResult = { ok: true; changes: Change[] } | { ok: false; error: string };

const ok = (changes: Change[]): EditResult => ({ ok: true, changes });
const err = (error: string): EditResult => ({ ok: false, error });

export type NewCommentInput = {
  id: string;
  createdAt: string;
  author: string;
  text: string;
  /** The text the user selected when the composer opened; refuse if the doc moved. */
  expected?: string;
};

/** Normalize a raw selection into the range that will be wrapped by markers. */
export const normalizeSelection = (doc: string, from: number, to: number): TextRange => {
  if (to < from) [from, to] = [to, from];
  // Trim whitespace at both ends so markers hug the text.
  while (from < to && /\s/.test(doc.charAt(from))) from++;
  while (to > from && /\s/.test(doc.charAt(to - 1))) to--;
  return expandInlineCodeSelection(doc, from, to);
};

/** Validate a selection for a new comment. Returns an error message or null. */
export const validateSelection = (doc: string, from: number, to: number): string | null => {
  if (to === from) return 'Select some text to comment on.';
  if (isInFencedCode(doc, from) || isInFencedCode(doc, to - 1)) {
    return 'Comments inside fenced code blocks are not supported yet.';
  }
  if (/\n[ \t]*\n/.test(doc.slice(from, to))) return 'Select text within a single paragraph.';
  for (const c of parseComments(doc)) {
    const open = c.open;
    const close = c.close;
    for (const marker of [open, close]) {
      if (marker && marker.from < to && marker.to > from && !(marker.from >= from && marker.to <= to)) {
        return 'The selection cuts through an existing comment marker.';
      }
    }
    if (!open || !close) continue;
    // Allowed: the selection contains the whole anchor, or sits entirely inside it. Anything else
    // interleaves the markers and produces two half-overlapping comments.
    const containsBoth = open.from >= from && close.to <= to;
    const insideAnchor = from >= open.to && to <= close.from;
    const touchesEither = (open.from < to && open.to > from) || (close.from < to && close.to > from);
    const straddles = from < open.from && to > open.from && to < close.to;
    if (!containsBoth && !insideAnchor && (touchesEither || straddles)) {
      return 'The selection partially overlaps an existing comment. Select all of it or a part inside it.';
    }
  }
  return null;
};

/** Wrap [from,to] with anchor markers and append a body block after the containing block. */
export const computeAddComment = (doc: string, from: number, to: number, input: NewCommentInput): EditResult => {
  ({ from, to } = normalizeSelection(doc, from, to));
  const invalid = validateSelection(doc, from, to);
  if (invalid) return err(invalid);
  if (input.expected !== undefined && doc.slice(from, to) !== input.expected) {
    return err('The selection moved. Try adding the comment again.');
  }
  if (!input.text.trim()) return err('Write a comment first.');

  const quote = stripMarkers(doc.slice(from, to));
  const data: CommentData = {
    author: input.author,
    createdAt: input.createdAt,
    status: 'open',
    quote,
    thread: [{ author: input.author, timestamp: input.createdAt, text: input.text }],
    reactions: [],
  };
  const paraEnd = blockEnd(doc, to);
  return ok([
    { from, to: from, insert: openMarker(input.id) },
    { from: to, to, insert: closeMarker(input.id) },
    { from: paraEnd, to: paraEnd, insert: '\n' + serializeBody(input.id, data) },
  ]);
};

/** When a selection is within one inline-code token, anchor the whole token. */
export const expandInlineCodeSelection = (doc: string, from: number, to: number): TextRange => {
  const lineFrom = doc.lastIndexOf('\n', from - 1) + 1;
  const nextLine = doc.indexOf('\n', to);
  const lineTo = nextLine < 0 ? doc.length : nextLine;

  for (let open = lineFrom; open < lineTo; open++) {
    if (doc.charAt(open) !== '`' || isEscaped(doc, open)) continue;
    const ticks = backtickRun(doc, open, lineTo);
    const contentFrom = open + ticks;
    let cursor = contentFrom;
    while (cursor < lineTo) {
      const candidate = doc.indexOf('`', cursor);
      if (candidate < 0 || candidate >= lineTo) break;
      const closeTicks = backtickRun(doc, candidate, lineTo);
      if (closeTicks === ticks && !isEscaped(doc, candidate)) {
        if (from >= contentFrom && to <= candidate) {
          return { from: open, to: candidate + closeTicks };
        }
        open = candidate + closeTicks - 1;
        break;
      }
      cursor = candidate + closeTicks;
    }
  }
  return { from, to };
};

const backtickRun = (doc: string, from: number, limit: number): number => {
  let to = from;
  while (to < limit && doc.charAt(to) === '`') to++;
  return to - from;
};

const isEscaped = (doc: string, position: number): boolean => {
  let slashes = 0;
  for (let cursor = position - 1; cursor >= 0 && doc.charAt(cursor) === '\\'; cursor--) slashes++;
  return slashes % 2 === 1;
};

export const computeAppendReply = (
  doc: string,
  id: string,
  entry: { createdAt: string; author: string; text: string },
): EditResult => {
  return replaceBody(doc, id, c => ({
    ...toData(c),
    thread: [...c.thread, { author: entry.author, timestamp: entry.createdAt, text: entry.text }],
  }));
};

export const computeSetResolved = (doc: string, id: string, resolved: boolean): EditResult => {
  return replaceBody(doc, id, c => ({ ...toData(c), status: resolved ? 'resolved' : 'open' }));
};

/** Replace the text of the i-th message in a thread. */
export const computeEditEntry = (doc: string, id: string, index: number, text: string): EditResult => {
  return replaceBody(doc, id, c => {
    if (index < 0 || index >= c.thread.length) return null;
    return { ...toData(c), thread: c.thread.map((e, i) => (i === index ? { ...e, text } : e)) };
  });
};

/** Remove the i-th message from a thread. Removing the last remaining entry deletes the thread. */
export const computeDeleteEntry = (doc: string, id: string, index: number): EditResult => {
  const c = parseComments(doc).find(x => x.id === id);
  if (c && c.thread.length === 1 && index === 0) return computeDeleteComment(doc, id);
  return replaceBody(doc, id, c => {
    if (index < 0 || index >= c.thread.length) return null;
    return {
      ...toData(c),
      thread: c.thread.filter((_, i) => i !== index),
      reactions: reactionsAfterEntryDelete(c.reactions, index),
    };
  });
};

const replaceBody = (doc: string, id: string, mutate: (c: ParsedComment) => CommentData | null): EditResult => {
  const c = parseComments(doc).find(x => x.id === id);
  if (!c) return err('Comment not found.');
  if (!c.body) return err('Comment has no body to update.');
  const data = mutate(c);
  if (!data) return err('That entry no longer exists.');
  return ok([{ from: c.body.from, to: c.body.to, insert: serializeBody(id, data) }]);
};

const toData = (c: ParsedComment): CommentData => ({
  author: c.author,
  createdAt: c.createdAt,
  status: c.status,
  quote: c.quote,
  codeLines: c.codeLines,
  thread: c.thread,
  reactions: c.reactions,
});

const reactionsAfterEntryDelete = (reactions: Reaction[], deletedEntry: number): Reaction[] => {
  return reactions.flatMap(reaction => {
    const entry = reaction.entry ?? 0;
    if (entry === deletedEntry) return [];
    const nextEntry = entry > deletedEntry ? entry - 1 : entry;
    const copied = { emoji: reaction.emoji, authors: [...reaction.authors] };
    return [nextEntry === 0 ? copied : { ...copied, entry: nextEntry }];
  });
};

/** Remove every marker and body block for `id`, leaving the anchored text in place. */
export const computeDeleteComment = (doc: string, id: string): EditResult => {
  if (!parseComments(doc).some(x => x.id === id)) return err('Comment not found.');
  const ranges: Change[] = [];
  const leadingTerm = (p: number): number =>
    doc.charCodeAt(p - 1) === 10 ? (doc.charCodeAt(p - 2) === 13 ? 2 : 1) : 0;
  const trailingTerm = (p: number): number =>
    doc.charCodeAt(p) === 13 && doc.charCodeAt(p + 1) === 10 ? 2 : doc.charCodeAt(p) === 10 ? 1 : 0;
  const aloneOnLine = (from: number, to: number): boolean =>
    (from === 0 || leadingTerm(from) > 0) && (to === doc.length || trailingTerm(to) > 0);
  scanAll(doc, new RegExp(`<!--c:${id}-->`, 'g'), (from, to) => {
    const end = aloneOnLine(from, to) ? to + trailingTerm(to) : to;
    ranges.push({ from, to: end, insert: '' });
  });
  scanAll(doc, new RegExp(`<!--/c:${id}-->`, 'g'), (from, to) => {
    const start = aloneOnLine(from, to) ? from - leadingTerm(from) : from;
    ranges.push({ from: start, to, insert: '' });
  });
  scanAll(doc, new RegExp(`<!--co:${id}(?![A-Za-z0-9])[\\s\\S]*?-->`, 'g'), (from, to) => {
    ranges.push({ from: from - leadingTerm(from), to, insert: '' });
  });
  if (ranges.length === 0) return err('Nothing to delete.');
  ranges.sort((a, b) => a.from - b.from);
  return ok(ranges);
};

const scanAll = (doc: string, re: RegExp, fn: (from: number, to: number) => void): void => {
  let m: RegExpExecArray | null;
  while ((m = re.exec(doc))) fn(m.index, m.index + m[0].length);
};

/** Remove anchor markers from text, used when quoting a selection that contains nested comments. */
export const stripMarkers = (s: string): string => s.replace(/<!--\/?c:[A-Za-z0-9]+-->/g, '');

/** Apply changes given in original coordinates (CodeMirror semantics). Used by tests. */
export const applyChanges = (doc: string, changes: Change[]): string => {
  const ordered = changes.map((c, i) => ({ ...c, i })).sort((a, b) => a.from - b.from || a.i - b.i);
  let out = '';
  let last = 0;
  for (const c of ordered) {
    out += doc.slice(last, c.from) + c.insert;
    last = Math.max(last, c.to);
  }
  return out + doc.slice(last);
};

/** End offset of the contiguous (non-blank) block of lines containing `pos`. */
export const blockEnd = (doc: string, pos: number): number => {
  let lineEnd = doc.indexOf('\n', pos);
  if (lineEnd === -1) return doc.length;
  for (;;) {
    const nextStart = lineEnd + 1;
    let nextEnd = doc.indexOf('\n', nextStart);
    if (nextEnd === -1) nextEnd = doc.length;
    if (doc.slice(nextStart, nextEnd).trim() === '') return lineEnd;
    lineEnd = nextEnd;
    if (nextEnd === doc.length) return doc.length;
  }
};

/** The comment whose anchor contains `pos`, innermost first. */
export const commentAt = (comments: ParsedComment[], pos: number): ParsedComment | null => {
  let best: ParsedComment | null = null;
  let bestLen = Infinity;
  for (const c of comments) {
    const r = anchorRange(c);
    if (r && pos >= r.from && pos <= r.to && r.to - r.from < bestLen) {
      best = c;
      bestLen = r.to - r.from;
    }
  }
  return best;
};
