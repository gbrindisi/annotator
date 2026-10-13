import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import type { CommentView, Selection } from '../protocol';
import { MermaidView, type Engine, type MermaidHost } from '../mermaid';
import { closeZoom } from '../zoom';

const flush = async (n = 6) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
  await new Promise(r => setTimeout(r, 0));
};

/** The zoom module is imported on first use. */
const zoomLoaded = () => new Promise(r => setTimeout(r, 30));

const fence = (sp: string, code: string, lang = 'mermaid') =>
  `<pre data-sourcepos="${sp}"><code class="language-${lang}">${code.replace(/&/g, '&amp;').replace(/</g, '&lt;')}</code></pre>\n`;

const A = 'flowchart LR\n  A --> B\n';
const B = 'sequenceDiagram\n  A->>B: hi\n';
const html = (a = A, b = B) =>
  `<p data-sourcepos="1:1-1:3">Text</p>\n${fence('3:1-6:3', a)}${fence('8:1-11:3', b)}${fence('13:1-15:3', 'x = 1\n', 'python')}`;

interface Fake {
  engine: Engine;
  render: ReturnType<typeof vi.fn>;
  initialize: ReturnType<typeof vi.fn>;
  load: Mock<() => Promise<Engine>>;
  /** When set, renders wait for `release()`. */
  hold(): void;
  release(): void;
}

const fake = (impl?: (id: string, code: string) => Promise<{ svg: string }>): Fake => {
  let gate: Promise<void> | null = null;
  let open: (() => void) | null = null;
  const render = vi.fn(async (id: string, code: string) => {
    if (gate) await gate;
    if (impl) return impl(id, code);
    return { svg: `<svg id="${id}" width="120" height="60" viewBox="0 0 120 60"><text>${code.split('\n')[0]}</text></svg>` };
  });
  const initialize = vi.fn();
  const engine: Engine = { api: { initialize, render }, config: scheme => ({ scheme }) };
  const load = vi.fn<() => Promise<Engine>>(async () => engine);
  return {
    engine, render, initialize, load,
    hold: () => void (gate = new Promise<void>(r => (open = r))),
    release: () => { open?.(); gate = null; },
  };
};

const setup = (f: Fake, markup = html()) => {
  const host = { comment: vi.fn<(s: Selection) => void>(), focusThread: vi.fn<(id: string) => void>() } satisfies MermaidHost;
  const view = new MermaidView(host, f.load);
  const root = document.createElement('article');
  document.body.append(root);
  root.innerHTML = markup;
  view.attach(root);
  return { host, view, root };
};

const containers = (root: HTMLElement) => [...root.querySelectorAll<HTMLElement>('.mermaid-block[data-mermaid-for]')];

beforeEach(() => {
  document.body.innerHTML = '';
});
afterEach(() => closeZoom());

describe('MermaidView', () => {
  it('wraps mermaid fences only, keeps the hidden <pre> and puts the container right after it', async () => {
    const f = fake();
    const { root } = setup(f);
    const pres = [...root.querySelectorAll('pre')];
    expect(pres.map(p => p.hidden)).toEqual([true, true, false]);
    const cs = containers(root);
    expect(cs.map(c => c.dataset.mermaidFor)).toEqual(['3:1-6:3', '8:1-11:3']);
    expect(pres[0].nextElementSibling).toBe(cs[0]);
    expect(pres[0].getAttribute('data-sourcepos')).toBe('3:1-6:3');
    expect(pres[0].textContent).toBe(A); // source of truth is untouched
    expect(pres[2].nextElementSibling).toBeNull();
    await flush();
    expect(cs[0].querySelector('svg')).not.toBeNull();
    expect(cs[0].dataset.state).toBe('ok');
    expect(cs[1].querySelector('text')!.textContent).toBe('sequenceDiagram');
    expect(f.initialize).toHaveBeenCalledWith({ scheme: 'light' });
  });

  it('does not load mermaid for documents without diagrams', async () => {
    const f = fake();
    setup(f, '<p data-sourcepos="1:1-1:3">Hi</p><pre data-sourcepos="3:1-5:3"><code class="language-js">1</code></pre>');
    await flush();
    expect(f.load).not.toHaveBeenCalled();
    expect(f.render).not.toHaveBeenCalled();
  });

  it('caches by source and scheme: unchanged diagrams are not re-rendered and appear synchronously', async () => {
    const f = fake();
    const { view, root } = setup(f);
    await flush();
    expect(f.render).toHaveBeenCalledTimes(2);
    root.innerHTML = html(); // a Doc update with identical diagrams
    view.attach(root);
    expect(containers(root).every(c => c.querySelector('svg'))).toBe(true); // no await: no flicker
    await flush();
    expect(f.render).toHaveBeenCalledTimes(2);
    // Changing one diagram renders only that one.
    root.innerHTML = html('flowchart TD\n  X --> Y\n');
    view.attach(root);
    await flush();
    expect(f.render).toHaveBeenCalledTimes(3);
    // A new colour scheme is a new cache key and re-renders what is on screen.
    view.setScheme('dark');
    await flush();
    expect(f.render).toHaveBeenCalledTimes(5);
    expect(f.initialize).toHaveBeenLastCalledWith({ scheme: 'dark' });
    view.setScheme('light');
    await flush();
    expect(f.render).toHaveBeenCalledTimes(5); // light versions were cached
  });

  it('shares one render between identical concurrent blocks', async () => {
    const f = fake();
    setup(f, fence('1:1-3:3', A) + fence('5:1-7:3', A));
    await flush();
    expect(f.render).toHaveBeenCalledTimes(1);
  });

  it('discards a stale render (doc changed meanwhile)', async () => {
    const f = fake();
    f.hold();
    const { view, root } = setup(f, fence('3:1-6:3', A));
    const stale = containers(root)[0];
    await flush();
    root.innerHTML = fence('3:1-6:3', 'flowchart TD\n  NEW --> X\n');
    view.attach(root);
    const fresh = containers(root)[0];
    f.release();
    await flush(12);
    expect(stale.isConnected).toBe(false);
    expect(stale.querySelector('svg')).toBeNull();
    expect(fresh.querySelector('text')!.textContent).toBe('flowchart TD');
    expect(root.querySelectorAll('svg').length).toBe(1);
    // The same source at a different position still shows: only a changed *source* is stale.
    expect(f.render.mock.calls.map(c => c[1])).toEqual([A, 'flowchart TD\n  NEW --> X\n']);
  });

  it('shows an error box above the code block for invalid diagrams and cleans <body>', async () => {
    const f = fake(async id => {
      // mermaid leaves its bomb SVG (and the temp wrapper) in <body>
      document.body.insertAdjacentHTML('beforeend', `<div id="d${id}"><svg id="${id}"><text>Syntax error in text</text></svg></div><svg><text>stray measuring element</text></svg>`);
      throw Object.assign(new Error('Parse error on line 2:\n  A -->> B\nExpecting X'), { hash: { line: 1 } });
    });
    const { root } = setup(f, fence('3:1-6:3', 'flowchart LR\n  A -->> B\n'));
    await flush();
    const c = containers(root)[0];
    const pre = root.querySelector('pre')!;
    expect(c.dataset.state).toBe('error');
    expect(c.nextElementSibling).toBe(pre); // above the code
    expect(pre.hidden).toBe(false); // code shown as a normal block
    expect(c.querySelector('.mermaid-error-title')!.textContent).toBe('Mermaid diagram error · line 2');
    expect(c.querySelector('.mermaid-error-msg')!.textContent).toContain('Expecting X');
    expect(c.querySelector('.mermaid-btn-source')!.hasAttribute('hidden')).toBe(true);
    expect(document.body.querySelector('[id^="amm-"], [id^="damm-"]')).toBeNull();
    expect(document.body.textContent).not.toContain('Syntax error in text');
    expect(document.body.textContent).not.toContain('stray measuring element');
    expect(root.parentElement).toBe(document.body); // only mermaid's leftovers were removed
  });

  it('takes the line from the error hash when the message has none, and retries a failed load', async () => {
    let fail = true;
    const f = fake(async () => {
      throw Object.assign(new Error('boom'), { hash: { line: 4 } });
    });
    f.load.mockImplementation(async () => {
      if (fail) throw new Error('chunk 404');
      return f.engine;
    });
    const { view, root } = setup(f, fence('3:1-6:3', A));
    await flush();
    expect(containers(root)[0].querySelector('.mermaid-error-msg')!.textContent).toContain('Could not load the diagram renderer: chunk 404');
    fail = false;
    root.innerHTML = fence('3:1-6:3', A);
    view.attach(root); // load failures are not cached
    await flush();
    expect(containers(root)[0].querySelector('.mermaid-error-title')!.textContent).toBe('Mermaid diagram error · line 5');
  });

  it('Comment sends the whole code block: sourcepos, full textContent, offset 0', async () => {
    const f = fake();
    const { host, root } = setup(f);
    await flush();
    containers(root)[1].querySelector<HTMLElement>('.mermaid-btn-comment')!.click();
    expect(host.comment).toHaveBeenCalledWith({ sourcepos: '8:1-11:3', text: B, offset: 0 });
  });

  it('Source toggles code block vs diagram per block and keeps the state across Doc updates', async () => {
    const f = fake();
    const { view, root } = setup(f);
    await flush();
    const first = () => containers(root)[0];
    const pre = () => root.querySelector<HTMLElement>('pre[data-sourcepos="3:1-6:3"]')!;
    first().querySelector<HTMLElement>('.mermaid-btn-source')!.click();
    expect(pre().hidden).toBe(false);
    expect(first().dataset.view).toBe('source');
    expect(first().nextElementSibling).toBe(pre());
    expect(first().querySelector('.mermaid-btn-source')!.textContent).toBe('Diagram');
    expect(containers(root)[1].dataset.view).toBe('diagram'); // other blocks unaffected
    root.innerHTML = html();
    view.attach(root);
    expect(pre().hidden).toBe(false); // survives a new Doc
    first().querySelector<HTMLElement>('.mermaid-btn-source')!.click();
    expect(pre().hidden).toBe(true);
    expect(pre().nextElementSibling).toBe(first());
  });

  it('marks diagrams with comments (frame + count badge) and the badge focuses the thread', async () => {
    const f = fake();
    const { host, view, root } = setup(f);
    await flush();
    const cm = (id: string, status: string, bs: string | null, lines: [number, number] | null = [0, 1]): CommentView => ({
      id, status, quote: null, author: null, created_at: null, orphaned: false, code_lines: lines, anchor: null, block_sourcepos: bs, entries: [], new_entries: 0, new_thread: false,
    });
    view.decorateComments(root, [cm('r1', 'resolved', '8:1-11:3'), cm('o1', 'open', '8:1-11:3'), cm('x', 'open', '13:1-15:3'), cm('inline', 'open', null, null)]);
    const [c0, c1] = containers(root);
    expect(c0.hasAttribute('data-commented')).toBe(false);
    expect(c0.querySelector<HTMLElement>('.mermaid-badge')!.hidden).toBe(true);
    expect(c1.hasAttribute('data-commented')).toBe(true);
    const badge = c1.querySelector<HTMLElement>('.mermaid-badge')!;
    expect(badge.hidden).toBe(false);
    expect(badge.textContent).toBe('2');
    badge.click();
    badge.click();
    badge.click();
    expect(host.focusThread.mock.calls.map(c => c[0])).toEqual(['o1', 'r1', 'o1']); // open first, cycling
    view.decorateComments(root, []);
    expect(c1.hasAttribute('data-commented')).toBe(false);
  });

  it('click zooms the diagram (overlay, Esc closes); not while text is selected', async () => {
    const f = fake();
    const { root } = setup(f);
    await flush();
    const v = containers(root)[0].querySelector<HTMLElement>('.mermaid-view')!;
    v.querySelector('svg')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await zoomLoaded();
    const overlay = document.querySelector('.mermaid-zoom')!;
    expect(overlay).not.toBeNull();
    expect(overlay.querySelector('svg')).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(document.querySelector('.mermaid-zoom')).toBeNull();

    const p = document.createElement('p');
    p.textContent = 'some text';
    root.append(p);
    const range = document.createRange();
    range.selectNodeContents(p);
    document.getSelection()!.removeAllRanges();
    document.getSelection()!.addRange(range);
    v.querySelector('svg')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await zoomLoaded();
    expect(document.querySelector('.mermaid-zoom')).toBeNull();
    document.getSelection()!.removeAllRanges();
  });

  it('zoom overlay closes on a click outside the diagram and zooms with the wheel', async () => {
    const f = fake();
    const { root } = setup(f);
    await flush();
    containers(root)[0].querySelector('svg')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await zoomLoaded();
    const overlay = document.querySelector<HTMLElement>('.mermaid-zoom')!;
    const stage = overlay.querySelector<HTMLElement>('.mermaid-zoom-stage')!;
    const before = stage.style.transform;
    overlay.dispatchEvent(new WheelEvent('wheel', { deltaY: -200, clientX: 10, clientY: 10, cancelable: true, bubbles: true }));
    expect(stage.style.transform).not.toBe(before);
    overlay.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }));
    overlay.dispatchEvent(new MouseEvent('pointerup', { bubbles: true }));
    expect(document.querySelector('.mermaid-zoom')).toBeNull();
  });
});
