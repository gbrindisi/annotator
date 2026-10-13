import { describe, expect, it, vi } from 'vitest';
import { applyChanges, clearChanges } from '../changes';
import { MermaidView, type Engine } from '../mermaid';

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

// ---------- Mermaid blocks ----------
describe('applyChanges with rendered mermaid blocks', () => {
  const fence = (sp: string, code: string) => `<pre data-sourcepos="${sp}"><code class="language-mermaid">${code}</code></pre>\n`;
  const NEW = 'flowchart LR\n  A --> B\n  B --> C\n';
  const OLD = 'flowchart LR\n  A --> B\n';
  const doc = `<p data-sourcepos="1:1-1:5">intro</p>\n${fence('3:1-7:3', NEW)}<p data-sourcepos="9:1-9:5">outro</p>\n`;
  const flush = async () => {
    for (let i = 0; i < 8; i++) await Promise.resolve();
    await new Promise(r => setTimeout(r, 0));
  };
  const mount = async () => {
    const engine: Engine = {
      api: { initialize: vi.fn(), render: vi.fn(async (id: string, code: string) => ({ svg: `<svg id="${id}"><text>${code.length}</text></svg>` })) },
      config: () => ({}),
    };
    const view = new MermaidView({ comment: vi.fn(), focusThread: vi.fn() }, async () => engine);
    const root = document.createElement('article');
    document.body.append(root);
    root.innerHTML = doc;
    view.attach(root);
    await flush();
    return { view, root };
  };

  it('marks a modified diagram, never word-diffs the hidden <pre>, offers "Previous version" and restores exactly', async () => {
    const { view, root } = await mount();
    const before = root.innerHTML;
    const pre = root.querySelector('pre')!;
    const els = applyChanges(root, [{ kind: 'modified', sourcepos: '3:1-7:3', old_text: OLD, after: null, old_html: fence('1:1-4:3', OLD) }]);
    view.decorateChanges(root);
    const c = root.querySelector<HTMLElement>('.mermaid-block')!;
    expect(els).toEqual([c]);
    expect(c.getAttribute('data-change')).toBe('modified');
    expect(pre.hasAttribute('data-change')).toBe(false);
    expect(root.querySelector('ins, del, [data-diff]')).toBeNull(); // no word diff into the pre
    expect(pre.textContent).toBe(NEW);

    const btn = c.querySelector<HTMLElement>('.mermaid-btn-prev')!;
    expect(btn.textContent).toBe('Previous version');
    btn.click();
    await flush();
    const old = c.querySelector('.mermaid-old')!;
    expect(c.classList.contains('show-prev')).toBe(true);
    expect(old.querySelector('.mermaid-old-label')!.textContent).toBe('previous');
    expect(old.querySelector('svg text')!.textContent).toBe(String(OLD.length)); // the OLD source was rendered
    expect(btn.textContent).toBe('Current version');
    // Doc updates while toggled: still shown after re-render + re-apply.
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
    applyChanges(root, [{ kind: 'modified', sourcepos: '3:1-7:3', old_text: OLD, after: null, old_html: fence('1:1-4:3', OLD) }]);
    view.decorateChanges(root);
    expect(c.querySelector('.mermaid-old')).not.toBeNull();
    c.querySelector<HTMLElement>('.mermaid-btn-prev')!.click(); // a fresh button after re-apply
    expect(c.querySelector('.mermaid-old')).toBeNull();
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });

  it('restores exactly when changes are toggled off while "Previous version" is open', async () => {
    const { view, root } = await mount();
    const before = root.innerHTML;
    const ch = { kind: 'modified', sourcepos: '3:1-7:3', old_text: OLD, after: null, old_html: fence('1:1-4:3', OLD) };
    applyChanges(root, [ch]);
    view.decorateChanges(root);
    root.querySelector<HTMLElement>('.mermaid-btn-prev')!.click();
    await flush();
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });

  it('shows the old diagram of a removed block inside the removed-block box, after the anchor diagram', async () => {
    const { view, root } = await mount();
    const before = root.innerHTML;
    const els = applyChanges(root, [{ kind: 'removed', sourcepos: null, old_text: OLD, after: '3:1-7:3', old_html: fence('1:1-4:3', OLD) }]);
    view.decorateChanges(root);
    await flush();
    const box = root.querySelector<HTMLElement>('.diff-del-block')!;
    expect(els).toEqual([box]);
    // After the diagram container, not between the <pre> and its container.
    expect(root.querySelector('.mermaid-block[data-mermaid-for]')!.nextElementSibling).toBe(box);
    const old = box.querySelector<HTMLElement>('.mermaid-block.is-old')!;
    expect(old.querySelector('svg')).not.toBeNull();
    expect(box.querySelector('pre')!.hidden).toBe(true);
    expect(old.querySelector('.mermaid-old-label')!.textContent).toBe('removed');
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });

  it('falls back to the removed-block box when the old block was not a diagram; added diagrams get the marker', async () => {
    const { view, root } = await mount();
    const before = root.innerHTML;
    const els = applyChanges(root, [
      { kind: 'modified', sourcepos: '3:1-7:3', old_text: 'was text', after: null, old_html: '<p data-sourcepos="1:1-1:8">was text</p>' },
    ]);
    view.decorateChanges(root);
    const c = root.querySelector<HTMLElement>('.mermaid-block')!;
    expect(c.getAttribute('data-change')).toBe('replaced');
    expect(c.previousElementSibling!.className).toBe('diff-del-block');
    expect(c.querySelector('.mermaid-btn-prev')).toBeNull();
    expect(els).toEqual([c]);
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
    applyChanges(root, [{ kind: 'added', sourcepos: '3:1-7:3', old_text: null, after: null }]);
    expect(c.getAttribute('data-change')).toBe('added');
    clearChanges(root);
    expect(root.innerHTML).toBe(before);
  });
});
