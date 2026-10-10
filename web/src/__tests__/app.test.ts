import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// End-to-end smoke test: the real App against the dev mock daemon.
describe('App with mock daemon', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="app"></div>';
    localStorage.clear();
    window.confirm = () => true;
    Element.prototype.scrollIntoView = () => {};
  });
  afterEach(() => vi.useRealTimers());

  it('renders the doc, sidebar, round close and round 2 changes', async () => {
    const { App } = await import('../app');
    const { mockFactory } = await import('../dev/mock');
    new App(document.getElementById('app')!, 'ws://x/ws/dev', mockFactory);
    await vi.advanceTimersByTimeAsync(500);

    expect(document.querySelector('.file-name')!.textContent).toBe('sample.md');
    expect(document.title).toBe('sample.md · round 1');
    expect(document.querySelectorAll('.doc mark[data-cid]').length).toBe(3);
    expect(document.querySelector('mark[data-cid="zz1q"]')!.classList.contains('is-resolved')).toBe(true);
    // Open filter hides the resolved thread; order: anchored, code, orphan.
    const ids = () => [...document.querySelectorAll('.mc-list .mc-card')].map(c => c.getAttribute('data-mc-id'));
    expect(ids()).toEqual(['k3f9', 'a7b2', 'p7k2', 'orph1']);

    // Reply flows through the daemon and back.
    const ta = document.querySelector<HTMLTextAreaElement>('[data-mc-id="a7b2"] textarea')!;
    ta.value = 'sounds good';
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await vi.advanceTimersByTimeAsync(300);
    expect(document.querySelector('[data-mc-id="a7b2"]')!.textContent).toContain('sounds good');

    // Done -> closed banner, read-only.
    (document.querySelector('.btn-primary') as HTMLButtonElement).click();
    await vi.advanceTimersByTimeAsync(300);
    expect(document.querySelector<HTMLElement>('.banner')!.hidden).toBe(false);
    expect(document.querySelector('.banner')!.textContent).toContain('Waiting for round 2');
    expect(document.querySelectorAll('.mc-list textarea').length).toBe(0);

    // Round 2 arrives.
    await vi.advanceTimersByTimeAsync(4500);
    expect(document.querySelector<HTMLElement>('.banner')!.hidden).toBe(true);
    expect(document.querySelector('.round-badge')!.textContent).toBe('Round 2');
    expect(document.querySelectorAll('.doc [data-change]').length).toBe(3);
    expect(document.querySelector('.pill-main')!.textContent).toBe('3 changes');
    expect(document.querySelector('[data-mc-id="ag01"] .mc-badge.is-new')).not.toBeNull();
    expect(document.querySelector('[data-mc-id="k3f9"] .mc-entry.is-new')!.textContent).toContain('Moved to Thursday');
  });

  it('autosaves source edits with the last hash and hides markers', async () => {
    const { App } = await import('../app');
    const { mockFactory } = await import('../dev/mock');
    const { EditorView } = await import('@codemirror/view');
    localStorage.setItem('annotator.mode', 'source');
    const sent: string[] = [];
    const factory = (u: string) => {
      const s = mockFactory(u);
      const orig = s.send.bind(s);
      s.send = (d: string) => (sent.push(d), orig(d));
      return s;
    };
    new App(document.getElementById('app')!, 'ws://x/ws/dev', factory);
    await vi.advanceTimersByTimeAsync(500);
    const view = EditorView.findFromDOM(document.querySelector('.cm-editor') as HTMLElement)!;
    expect(view.state.doc.toString()).toContain('<!--c:k3f9-->');
    expect(document.querySelector('.cm-content')!.textContent).not.toContain('<!--c:k3f9-->');
    expect(document.querySelector('.cm-anchor')).not.toBeNull();
    view.dispatch({ changes: { from: 0, insert: 'X' } });
    view.dispatch({ changes: { from: 1, insert: 'Y' } });
    await vi.advanceTimersByTimeAsync(400);
    expect(sent.length).toBe(0);
    await vi.advanceTimersByTimeAsync(200);
    expect(sent.length).toBe(1);
    const m = JSON.parse(sent[0]);
    expect(m.type).toBe('replace_source');
    expect(m.text.startsWith('XY# Release plan')).toBe(true);
    expect(m.base_hash).toMatch(/^[0-9a-f]{8}$/);
    await vi.advanceTimersByTimeAsync(300);
    expect(document.querySelector<HTMLElement>('.notice')!.hidden).toBe(true);
  });
});
