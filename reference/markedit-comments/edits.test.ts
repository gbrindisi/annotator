import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import {
  applyChanges,
  blockEnd,
  computeAddComment,
  computeAppendReply,
  computeDeleteComment,
  computeDeleteEntry,
  computeEditEntry,
  normalizeSelection,
  validateSelection,
} from '../src/format/edits';
import { parseComments, anchorRange } from '../src/format/parse';

const SAMPLE = readFileSync(new URL('../fixtures/sample.md', import.meta.url), 'utf8');
const NOW = '2026-09-24T14:30:00.000Z';
const input = (text: string, id = 'q4m2') => ({ id, createdAt: NOW, author: 'bomber', text });

const add = (doc: string, from: number, to: number, text = 'needs a source') => {
  const r = computeAddComment(doc, from, to, input(text));
  if (!r.ok) throw new Error(r.error);
  return applyChanges(doc, r.changes);
};

describe('blockEnd', () => {
  it('finds the paragraph end', () => {
    const doc = 'line one\nline two\n\nother';
    expect(blockEnd(doc, 2)).toBe('line one\nline two'.length);
  });
  it('handles the last line without newline', () => {
    expect(blockEnd('only', 1)).toBe(4);
  });
  it('handles a trailing newline at EOF', () => {
    expect(blockEnd('a\nb\n', 0)).toBe(3);
  });
});

describe('computeAddComment', () => {
  it('produces the exact expected document', () => {
    const doc = 'Our roadmap commits us to net zero by 2030 across all operations.\n\nNext.';
    const from = doc.indexOf('net zero');
    const to = from + 'net zero by 2030'.length;
    expect(add(doc, from, to, 'this needs a source')).toBe(
      'Our roadmap commits us to <!--c:q4m2-->net zero by 2030<!--/c:q4m2--> across all operations.\n' +
        '<!--co:q4m2 by:bomber at:2026-09-24T14:30:00.000Z status:open quote:"net zero by 2030"\n' +
        'bomber (2026-09-24T14:30:00.000Z): this needs a source\n-->\n\nNext.',
    );
  });

  it('places the body after the whole paragraph, and works on headings', () => {
    const doc = '# Title\n\nfirst line\nsecond line\n\nend';
    const out = add(doc, 2, 7);
    expect(out.startsWith('# <!--c:q4m2-->Title<!--/c:q4m2-->\n<!--co:q4m2')).toBe(true);
    const out2 = add(doc, doc.indexOf('first'), doc.indexOf('first') + 5);
    expect(out2).toContain('second line\n<!--co:q4m2');
  });

  it('trims whitespace and expands inline code', () => {
    const doc = 'call ` foo() ` now';
    const r = normalizeSelection(doc, doc.indexOf('foo'), doc.indexOf('foo') + 3);
    expect(doc.slice(r.from, r.to)).toBe('` foo() `');
    const r2 = normalizeSelection('a  word  b', 1, 8);
    expect(r2).toEqual({ from: 3, to: 7 });
  });

  it('refuses invalid selections', () => {
    expect(validateSelection('abc', 1, 1)).toMatch(/Select some text/);
    expect(validateSelection('para one\n\npara two', 0, 15)).toMatch(/single paragraph/);
    const fence = SAMPLE.indexOf('validate(order)');
    expect(validateSelection(SAMPLE, fence, fence + 5)).toMatch(/fenced code/);
    const marker = SAMPLE.indexOf('<!--c:k3f9-->');
    expect(validateSelection(SAMPLE, marker + 3, marker + 20)).toMatch(/cuts through/);
    // Fully containing an existing anchor is allowed, as is a selection inside it.
    const closeMarker = SAMPLE.indexOf('<!--/c:k3f9-->');
    expect(validateSelection(SAMPLE, marker - 3, closeMarker + '<!--/c:k3f9-->'.length + 3)).toBeNull();
    expect(validateSelection(SAMPLE, marker + '<!--c:k3f9-->'.length + 1, closeMarker - 1)).toBeNull();
    // Interleaving: starts before the open marker and ends inside the anchor.
    expect(validateSelection(SAMPLE, marker - 3, closeMarker - 2)).toMatch(/partially overlaps/);
    // Interleaving: starts inside the anchor and ends after the close marker.
    expect(validateSelection(SAMPLE, marker + 20, closeMarker + 20)).toMatch(/partially overlaps/);
  });

  it('strips nested markers from the quote', () => {
    const doc = 'We should <!--c:k3f9-->ship on Friday<!--/c:k3f9--> now.\n<!--co:k3f9 status:open\nkyle: hi\n-->';
    const out = add(doc, 3, doc.indexOf(' now.'), 'outer');
    const outer = parseComments(out).find(c => c.id === 'q4m2')!;
    expect(outer.quote).toBe('should ship on Friday');
  });

  it('refuses when the selection moved', () => {
    const r = computeAddComment('hello world', 0, 5, { ...input('x'), expected: 'other' });
    expect(r.ok).toBe(false);
  });

  it('refuses empty text', () => {
    expect(computeAddComment('hello world', 0, 5, input('  ')).ok).toBe(false);
  });
});

describe('thread edits', () => {
  it('appends a reply and preserves reactions and header', () => {
    const r = computeAppendReply(SAMPLE, 'k3f9', { createdAt: NOW, author: 'bomber', text: 'agreed' });
    expect(r.ok).toBe(true);
    const out = applyChanges(SAMPLE, r.ok ? r.changes : []);
    const c = parseComments(out).find(x => x.id === 'k3f9')!;
    expect(c.thread).toHaveLength(3);
    expect(c.thread[2]).toEqual({ author: 'bomber', timestamp: NOW, text: 'agreed' });
    expect(c.reactions).toEqual([{ emoji: '👍', authors: ['sam', 'mike'] }]);
    expect(out).toContain('<!--co:k3f9 by:kyle at:2026-06-17T10:00:00.000Z status:open quote:"ship on Friday"\n');
  });

  it('edits an entry', () => {
    const r = computeEditEntry(SAMPLE, 'a7b2', 0, 'changed');
    const out = applyChanges(SAMPLE, r.ok ? r.changes : []);
    expect(parseComments(out).find(x => x.id === 'a7b2')!.thread[0]!.text).toBe('changed');
    expect(computeEditEntry(SAMPLE, 'a7b2', 5, 'x').ok).toBe(false);
    expect(computeEditEntry(SAMPLE, 'nope', 0, 'x').ok).toBe(false);
  });

  it('deletes an entry and re-indexes reactions', () => {
    const doc = '<!--c:r1-->x<!--/c:r1-->\n<!--co:r1 status:open\na: one\nb: two\nc: three\n+👍 a\n+@1 🎉 b\n+@2 👀 c\n-->';
    const r = computeDeleteEntry(doc, 'r1', 1);
    const out = applyChanges(doc, r.ok ? r.changes : []);
    const c = parseComments(out)[0]!;
    expect(c.thread.map(e => e.text)).toEqual(['one', 'three']);
    expect(c.reactions).toEqual([
      { emoji: '👍', authors: ['a'] },
      { emoji: '👀', authors: ['c'], entry: 1 },
    ]);
  });

  it('deleting the only entry deletes the whole thread', () => {
    const out = applyChanges(SAMPLE, (() => { const r = computeDeleteEntry(SAMPLE, 'a7b2', 0); return r.ok ? r.changes : []; })());
    expect(parseComments(out).some(x => x.id === 'a7b2')).toBe(false);
    expect(out).toContain('paragraph with another highlighted span placed');
  });

  it('deletes a thread leaving the text and no blank line', () => {
    const r = computeDeleteComment(SAMPLE, 'k3f9');
    const out = applyChanges(SAMPLE, r.ok ? r.changes : []);
    expect(out).toContain('We should ship on Friday regardless');
    expect(out).toContain('closes Monday.\n\nHere is a second');
    expect(out).not.toContain('k3f9');
  });

  it('deletes a code comment including own-line markers', () => {
    const r = computeDeleteComment(SAMPLE, 'p7k2');
    const out = applyChanges(SAMPLE, r.ok ? r.changes : []);
    expect(out).toContain('## Code\n\n```python\ndef process(order):\n    validate(order)\n```\n\nThis body');
  });

  it('add then delete is a no-op', () => {
    const doc = 'hello brave world';
    const added = add(doc, 6, 11);
    const c = parseComments(added)[0]!;
    expect(added.slice(anchorRange(c)!.from, anchorRange(c)!.to)).toBe('brave');
    const r = computeDeleteComment(added, 'q4m2');
    expect(applyChanges(added, r.ok ? r.changes : [])).toBe(doc);
  });
});
