import { beforeEach, describe, expect, it } from 'vitest';
import { selectionPayload } from '../selection';

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '';
  root = document.createElement('article');
  document.body.append(root);
});

const rangeOf = (node: Node, start: number, endNode: Node, end: number): Range => {
  const r = document.createRange();
  r.setStart(node, start);
  r.setEnd(endNode, end);
  return r;
};

describe('selectionPayload', () => {
  it('computes offset within the block for plain text', () => {
    root.innerHTML = '<p data-sourcepos="5:1-5:20">We should ship on Friday</p>';
    const t = root.querySelector('p')!.firstChild!;
    const res = selectionPayload(rangeOf(t, 10, t, 24), root);
    expect(res).toEqual({ ok: true, selection: { sourcepos: '5:1-5:20', text: 'ship on Friday', offset: 10 } });
  });

  it('counts emoji as UTF-16 code units', () => {
    root.innerHTML = '<p data-sourcepos="1:1-1:30">👍 ok 🎉 then text</p>';
    const t = root.querySelector('p')!.firstChild as Text;
    const start = t.data.indexOf('then');
    const res = selectionPayload(rangeOf(t, start, t, start + 4), root);
    expect(res).toMatchObject({ ok: true, selection: { text: 'then', offset: 9 } });
    expect(start).toBe(9); // 👍 and 🎉 are 2 units each
  });

  it('crosses inline elements and marks, resolving to the enclosing block', () => {
    root.innerHTML = '<p data-sourcepos="3:1-3:60">Go <strong data-sourcepos="3:4-3:12">bold 🚀</strong> and <mark data-cid="x">marked</mark> end</p>';
    const strongText = root.querySelector('strong')!.firstChild!;
    const markText = root.querySelector('mark')!.firstChild!;
    const res = selectionPayload(rangeOf(strongText, 5, markText, 3), root);
    expect(res).toEqual({ ok: true, selection: { sourcepos: '3:1-3:60', text: '🚀 and mar', offset: 8 } });
  });

  it('uses the inner block (li) for nested lists', () => {
    root.innerHTML = '<ul data-sourcepos="1:1-2:9"><li data-sourcepos="1:1-1:9">one</li><li data-sourcepos="2:1-2:9">two items</li></ul>';
    const t = root.querySelectorAll('li')[1].firstChild!;
    expect(selectionPayload(rangeOf(t, 4, t, 9), root)).toMatchObject({ ok: true, selection: { sourcepos: '2:1-2:9', offset: 4, text: 'items' } });
  });

  it('flags selections spanning top-level blocks', () => {
    root.innerHTML = '<p data-sourcepos="1:1-1:3">abc</p><p data-sourcepos="3:1-3:3">def</p>';
    const [a, b] = root.querySelectorAll('p');
    expect(selectionPayload(rangeOf(a.firstChild!, 1, b.firstChild!, 2), root)).toMatchObject({ ok: false });
  });

  it('ignores collapsed and outside selections', () => {
    root.innerHTML = '<p data-sourcepos="1:1-1:3">abc</p>';
    const t = root.querySelector('p')!.firstChild!;
    expect(selectionPayload(rangeOf(t, 1, t, 1), root)).toBeNull();
    const outside = document.createElement('p');
    outside.setAttribute('data-sourcepos', '9:1-9:3');
    outside.textContent = 'xyz';
    document.body.append(outside);
    expect(selectionPayload(rangeOf(outside.firstChild!, 0, outside.firstChild!, 2), root)).toBeNull();
  });
});
