import { describe, expect, it } from 'vitest';
import { applyChanges, clearChanges } from '../changes';
import { selectionPayload } from '../selection';
import { applyWordDiff, changedRatio, tokenize, wordDiff } from '../worddiff';

const show = (ops: ReturnType<typeof wordDiff>) =>
  ops.map(o => (o.type === 'eq' ? o.text : o.type === 'ins' ? `{+${o.text}+}` : `[-${o.text}-]`)).join('');

describe('wordDiff', () => {
  it('tokenizes keeping whitespace', () => {
    expect(tokenize('a  b\nc')).toEqual(['a', '  ', 'b', '\n', 'c']);
  });
  it('insert', () => expect(show(wordDiff('a c', 'a b c'))).toBe('a {+b +}c'));
  it('delete', () => expect(show(wordDiff('one two three four', 'one four'))).toBe('one [-two three -]four'));
  it('replace groups del before ins', () => expect(show(wordDiff('ship on Friday now', 'ship on Thursday now'))).toBe('ship on [-Friday-]{+Thursday+} now'));
  it('whitespace-only change', () => expect(show(wordDiff('a b', 'a  b'))).toBe('a[- -]{+  +}b'));
  it('emoji', () => expect(show(wordDiff('go 🚀 now', 'go 🎉 now'))).toBe('go [-🚀-]{+🎉+} now'));
  it('identical / ratio', () => {
    expect(wordDiff('x y', 'x y')).toEqual([{ type: 'eq', text: 'x y' }]);
    expect(changedRatio(wordDiff('x y', 'x y'))).toBe(0);
    expect(changedRatio(wordDiff('aaa', 'bbb'))).toBe(1);
  });
});

const BLOCK = '<p data-sourcepos="1:1-1:60">We <strong data-sourcepos="1:4-1:20">should <code>ship</code></strong> on <mark data-cid="k1">Friday after QA</mark> done.</p>';

describe('applyChanges inline diff', () => {
  const setup = () => {
    document.body.innerHTML = '';
    const root = document.createElement('article');
    root.innerHTML = BLOCK + '<p data-sourcepos="3:1-3:5">tail</p>';
    document.body.append(root);
    return root;
  };
  const oldHtml = '<p data-sourcepos="1:1-1:9">We should <code>ship</code> on Friday regardless of QA done.</p>';

  it('wraps insertions, inserts deletions, keeps formatting and marks; toggle off restores', () => {
    const root = setup();
    const before = root.innerHTML;
    const els = applyChanges(root, [{ kind: 'modified', sourcepos: '1:1-1:60', old_text: null, after: null, old_html: oldHtml }]);
    expect(els).toHaveLength(1);
    const p = root.firstElementChild as HTMLElement;
    expect(p.querySelector('strong code')!.textContent).toBe('ship');
    expect(p.querySelector('mark[data-cid="k1"]')).not.toBeNull();
    expect([...p.querySelectorAll('del.diff-del')].map(d => d.textContent)).toEqual(['regardless of']);
    expect([...p.querySelectorAll('ins.diff-ins')].map(d => d.textContent)).toEqual(['after']);
    expect(p.querySelector('ins')!.closest('mark')).not.toBeNull();
    p.querySelectorAll('[data-diff]').forEach(n => {
      expect(n.getAttribute('contenteditable')).toBe('false');
    });
    // Text without deleted nodes equals the original text.
    const clone = p.cloneNode(true) as HTMLElement;
    clone.querySelectorAll('del').forEach(d => d.remove());
    expect(clone.textContent).toBe('We should ship on Friday after QA done.');
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });

  it('falls back to a whole struck-through block when mostly different', () => {
    const root = setup();
    const before = root.innerHTML;
    applyChanges(root, [{ kind: 'modified', sourcepos: '3:1-3:5', old_text: 'x', after: null, old_html: '<p data-sourcepos="1:1-1:30">completely unrelated old words</p>' }]);
    const del = root.querySelector('.diff-del-block')!;
    expect(del.nextElementSibling!.getAttribute('data-sourcepos')).toBe('3:1-3:5');
    expect(del.querySelector('[data-sourcepos]')).toBeNull();
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });

  it('inserts removed blocks as HTML without sourcepos', () => {
    const root = setup();
    const before = root.innerHTML;
    applyChanges(root, [
      { kind: 'removed', sourcepos: null, old_text: '- a', after: '1:4-1:20', old_html: '<ul data-sourcepos="1:1-1:3"><li data-sourcepos="1:1-1:3">a <em>b</em></li></ul>' },
      { kind: 'removed', sourcepos: null, old_text: 'z', after: '1:4-1:20', old_html: '<p>z</p>' },
    ]);
    const d = root.children[1];
    expect(d.className).toBe('diff-del-block');
    expect(d.querySelector('li em')!.textContent).toBe('b');
    expect(d.querySelector('[data-sourcepos]')).toBeNull();
    expect(root.children[2].textContent).toBe('z');
    expect(root.querySelector('[data-sourcepos="1:1-1:3"]')).toBeNull();
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });

  it('selection payload ignores deleted text', () => {
    const root = setup();
    applyChanges(root, [{ kind: 'modified', sourcepos: '1:1-1:60', old_text: null, after: null, old_html: oldHtml }]);
    const p = root.firstElementChild as HTMLElement;
    const del = p.querySelector('del')!;
    // Select from inside "Friday " (before del) to inside " QA" (after del and ins).
    const fri = del.previousSibling as Text;
    expect(fri.data).toBe('Friday ');
    const qa = del.parentElement!.lastChild as Text;
    expect(qa.data).toBe(' QA');
    const r = document.createRange();
    r.setStart(fri, 3);
    r.setEnd(qa, 3);
    const res = selectionPayload(r, root);
    expect(res).toEqual({ ok: true, selection: { sourcepos: '1:1-1:60', text: 'day after QA', offset: 21 } });
    // Entirely inside deleted text.
    const r2 = document.createRange();
    r2.setStart(del.firstChild!, 0);
    r2.setEnd(del.firstChild!, 5);
    expect(selectionPayload(r2, root)).toBeNull();
    // Selection after deleted text: offset excludes it.
    const done = p.lastChild as Text;
    const r3 = document.createRange();
    r3.setStart(done, 1);
    r3.setEnd(done, 5);
    expect(selectionPayload(r3, root)).toEqual({ ok: true, selection: { sourcepos: '1:1-1:60', text: 'done', offset: 'We should ship on Friday after QA '.length } });
  });

  it('applyWordDiff refuses mismatched ops', () => {
    const p = document.createElement('p');
    p.textContent = 'abc';
    expect(applyWordDiff(p, [{ type: 'eq', text: 'xyz' }])).toBe(false);
    expect(p.innerHTML).toBe('abc');
  });
});
