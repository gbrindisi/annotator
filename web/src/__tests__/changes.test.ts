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
    const p8 = root.querySelector('[data-sourcepos="8:1-8:5"]') as HTMLElement;
    expect(p8.title).toContain('old two');
    // Removed marker goes after the top-level block containing the `after` element.
    expect(root.querySelector('ul')!.nextElementSibling!.className).toBe('removed-marker');
    expect((root.querySelector('ul')!.nextElementSibling as HTMLElement).title).toContain('gone');
    expect(root.firstElementChild!.className).toBe('removed-marker');

    clearChanges(root);
    expect(root.querySelectorAll('[data-change], .removed-marker').length).toBe(0);
    expect(p8.hasAttribute('title')).toBe(false);
  });
});
