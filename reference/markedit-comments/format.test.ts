import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { parseComments, anchorRange, isAnchored, isOrphan, existingIds, isCodeComment } from '../src/format/parse';
import { serializeBody, openMarker, closeMarker } from '../src/format/serialize';
import { generateId } from '../src/format/ids';
import type { CommentData } from '../src/format/types';

const SAMPLE = readFileSync(new URL('../fixtures/sample.md', import.meta.url), 'utf8');

const CANONICAL = [
  'We should ' + openMarker('k3f9') + 'ship on Friday' + closeMarker('k3f9') + ' regardless of the QA timeline.',
  '<!--co:k3f9 by:kyle at:2026-06-17T10:00 status:open quote:"ship on Friday"',
  'kyle: I thought we agreed Thursday?',
  'sam (2026-06-17T11:00): Thursday is better for QA.',
  '-->',
  '',
  'Next paragraph.',
].join('\n');

describe('parseComments', () => {
  it('parses the canonical example', () => {
    const comments = parseComments(CANONICAL);
    expect(comments).toHaveLength(1);
    const c = comments[0]!;
    expect(c.id).toBe('k3f9');
    expect(c.author).toBe('kyle');
    expect(c.createdAt).toBe('2026-06-17T10:00');
    expect(c.status).toBe('open');
    expect(c.quote).toBe('ship on Friday');
    expect(c.thread).toHaveLength(2);
    expect(c.thread[0]).toMatchObject({ author: 'kyle', text: 'I thought we agreed Thursday?' });
    expect(c.thread[1]).toMatchObject({ author: 'sam', timestamp: '2026-06-17T11:00', text: 'Thursday is better for QA.' });
    const r = anchorRange(c)!;
    expect(CANONICAL.slice(r.from, r.to)).toBe('ship on Friday');
  });

  it('parses the fixture in document order with states', () => {
    const comments = parseComments(SAMPLE);
    expect(comments.map(c => c.id)).toEqual(['k3f9', 'a7b2', 'zz1q', 'p7k2', 'orph1']);
    const byId = new Map(comments.map(c => [c.id, c]));
    expect(byId.get('k3f9')!.reactions).toEqual([{ emoji: '👍', authors: ['sam', 'mike'] }]);
    expect(byId.get('zz1q')!.status).toBe('resolved');
    expect(isCodeComment(byId.get('p7k2')!)).toBe(true);
    expect(byId.get('p7k2')!.codeLines).toEqual({ from: 1, to: 1 });
    expect(byId.get('p7k2')!.quote).toBe('    validate(order)');
    expect(isOrphan(byId.get('orph1')!)).toBe(true);
    expect(isAnchored(byId.get('orph1')!)).toBe(false);
  });

  it('keeps the first occurrence of a duplicated id', () => {
    const doc = `${openMarker('dup')}one${closeMarker('dup')} ${openMarker('dup')}two${closeMarker('dup')}\n<!--co:dup status:open\nme: a\n-->\n<!--co:dup status:open\nme: b\n-->`;
    const c = parseComments(doc)[0]!;
    expect(doc.slice(anchorRange(c)!.from, anchorRange(c)!.to)).toBe('one');
    expect(c.thread[0]!.text).toBe('a');
  });

  it('rejects ids with punctuation', () => {
    expect(parseComments('<!--c:my-id-->x<!--/c:my-id-->')).toHaveLength(0);
  });

  it('treats close-before-open as unanchored', () => {
    const doc = closeMarker('ab1') + 'x' + openMarker('ab1') + '\n<!--co:ab1 status:open\nme: hi\n-->';
    const c = parseComments(doc)[0]!;
    expect(isAnchored(c)).toBe(false);
    expect(isOrphan(c)).toBe(true);
  });

  it('handles nested comments', () => {
    const doc =
      openMarker('aaa') + 'the ' + openMarker('bbb') + 'quick brown' + closeMarker('bbb') + ' fox' + closeMarker('aaa') +
      '\n<!--co:aaa status:open\nme: outer\n-->\n<!--co:bbb status:open\nme: inner\n-->';
    const comments = parseComments(doc);
    const bbb = comments.find(c => c.id === 'bbb')!;
    expect(doc.slice(anchorRange(bbb)!.from, anchorRange(bbb)!.to)).toBe('quick brown');
  });

  it('ignores markers inside fenced blocks and inline code', () => {
    expect(parseComments(SAMPLE).some(c => c.id === 'ex01')).toBe(false);
    expect(parseComments('Use the `' + openMarker('inl01') + '` marker.')).toHaveLength(0);
  });

  it('handles CRLF bodies and escaped newlines', () => {
    const doc = openMarker('cr1') + 'x' + closeMarker('cr1') + '\r\n<!--co:cr1 status:open\r\nme: line one\\nline two\r\n-->';
    const c = parseComments(doc)[0]!;
    expect(c.thread[0]!.text).toBe('line one\nline two');
  });

  it('collects existing ids', () => {
    expect([...existingIds(CANONICAL)]).toEqual(['k3f9']);
  });
});

describe('serializeBody', () => {
  it('writes the canonical header order and trailing newline', () => {
    const data: CommentData = {
      author: 'Gianluca Brindisi',
      createdAt: '2026-09-24T14:30:00.000Z',
      status: 'open',
      quote: 'net  zero\nby "2030"',
      thread: [{ author: 'gianluca', timestamp: '2026-09-24T14:30:00.000Z', text: 'needs a source' }],
      reactions: [],
    };
    expect(serializeBody('q4m2', data)).toBe(
      '<!--co:q4m2 by:Gianluca_Brindisi at:2026-09-24T14:30:00.000Z status:open quote:"net zero by \'2030\'"\n' +
        'gianluca (2026-09-24T14:30:00.000Z): needs a source\n-->',
    );
  });

  it('serializes an empty thread', () => {
    expect(serializeBody('h1', { status: 'open', quote: 'text', thread: [], reactions: [] })).toBe(
      '<!--co:h1 status:open quote:"text"\n-->',
    );
  });

  it('breaks the terminator and escapes newlines in entries', () => {
    const body = serializeBody('t1', {
      status: 'open',
      thread: [{ author: 'me', text: 'a --> b\nc' }],
      reactions: [],
    });
    expect(body).toBe('<!--co:t1 status:open\nme: a --​> b\\nc\n-->');
    const c = parseComments(openMarker('t1') + 'x' + closeMarker('t1') + '\n' + body)[0]!;
    expect(c.thread[0]!.text).toBe('a --​> b\nc');
  });

  it('round-trips every body in the fixture byte for byte', () => {
    for (const c of parseComments(SAMPLE)) {
      if (!c.body) continue;
      const original = SAMPLE.slice(c.body.from, c.body.to);
      const rewritten = serializeBody(c.id, {
        author: c.author,
        createdAt: c.createdAt,
        status: c.status,
        quote: c.quote,
        codeLines: c.codeLines,
        thread: c.thread,
        reactions: c.reactions,
      });
      expect(rewritten).toBe(original);
    }
  });
});

describe('generateId', () => {
  it('produces 5 lowercase alphanumerics and avoids collisions', () => {
    const existing = new Set(['abcde']);
    const id = generateId(existing);
    expect(id).toMatch(/^[a-z0-9]{5}$/);
    expect(existing.has(id)).toBe(false);
  });
});
