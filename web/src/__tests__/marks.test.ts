import { describe, expect, it } from 'vitest';
import { cidAt, markOrder, marksFor, sortComments, styleMarks } from '../marks';
import type { CommentView } from '../protocol';

const c = (id: string, extra: Partial<CommentView> = {}): CommentView => ({
  id, status: 'open', quote: id, author: 'a', created_at: null, orphaned: false, code_lines: null, anchor: null, entries: [], new_entries: 0, new_thread: false, ...extra,
});

const html = '<p data-sourcepos="1:1-1:9"><mark data-cid="b">x</mark> and <em><mark data-cid="a">y</mark></em><mark data-cid="a">z</mark></p><p><mark data-cid="r">q</mark></p>';

describe('mark <-> card mapping', () => {
  it('finds all marks of a comment and the id under a target', () => {
    const root = document.createElement('div');
    root.innerHTML = html;
    expect(marksFor(root, 'a').map(m => m.textContent)).toEqual(['y', 'z']);
    expect(cidAt(root.querySelector('em mark')!.firstChild)).toBe('a');
    expect(cidAt(root.querySelector('p'))).toBeNull();
    expect(markOrder(root)).toEqual(['b', 'a', 'r']);
  });

  it('dims resolved marks and highlights the active one', () => {
    const root = document.createElement('div');
    root.innerHTML = html;
    styleMarks(root, [c('a'), c('b'), c('r', { status: 'resolved' })], 'a');
    expect(root.querySelector('[data-cid="r"]')!.classList.contains('is-resolved')).toBe(true);
    expect(marksFor(root, 'a').every(m => m.classList.contains('is-active'))).toBe(true);
    expect(root.querySelector('[data-cid="b"]')!.classList.contains('is-active')).toBe(false);
  });

  it('sorts cards in doc order, then code-line, then orphans', () => {
    const list = [c('orph', { orphaned: true }), c('code', { code_lines: [1, 1] }), c('a'), c('b'), c('lost')];
    expect(sortComments(list, ['b', 'a']).map(x => x.id)).toEqual(['b', 'a', 'code', 'orph', 'lost']);
  });
});
