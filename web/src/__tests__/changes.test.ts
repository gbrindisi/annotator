import { describe, expect, it } from 'vitest';
import { applyChanges, clearChanges } from '../changes';

const html = '<h1 data-sourcepos="1:1-1:5">T</h1><p data-sourcepos="3:1-3:5">one</p><ul data-sourcepos="5:1-6:5"><li data-sourcepos="5:1-5:5">a</li></ul><p data-sourcepos="8:1-8:5">two</p>';

describe('applyChanges', () => {
  it('outlines added/modified blocks by exact sourcepos and inserts removed markers', () => {
    const root = document.createElement('article');
    root.innerHTML = html;
    const els = applyChanges(root, [
      { kind: 'modified', sourcepos: '8:1-8:5', old_text: 'old two', after: null },
      { kind: 'added', sourcepos: '3:1-3:5', old_text: null, after: null },
      { kind: 'removed', sourcepos: null, old_text: 'gone', after: '5:1-5:5' },
      { kind: 'removed', sourcepos: null, old_text: 'first', after: null },
      { kind: 'added', sourcepos: '3:1-3:9', old_text: null, after: null }, // no exact match
    ]);
    expect(els.map(e => e.getAttribute('data-change'))).toEqual(['removed', 'added', 'removed', 'modified']);
    // Removed block goes after the top-level block containing the `after` element.
    expect(root.querySelector('ul')!.nextElementSibling!.className).toBe('diff-del-block');
    expect(root.querySelector('ul')!.nextElementSibling!.textContent).toBe('gone');
    expect(root.firstElementChild!.className).toBe('diff-del-block');

    clearChanges(root);
    expect(root.querySelectorAll('[data-change], [data-diff]').length).toBe(0);
    expect(root.innerHTML).toBe(html);
  });
});
