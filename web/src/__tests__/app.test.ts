import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// jsdom cannot lay out SVG: stand in for mermaid.js (the default loader imports it lazily).
vi.mock('mermaid', () => ({
  default: {
    initialize: vi.fn(),
    render: vi.fn(async (id: string, code: string) => {
      if (code.includes('unclosed')) throw Object.assign(new Error('Parse error on line 4:\nExpecting something'), { hash: { line: 3 } });
      return { svg: `<svg id="${id}" width="100" height="50" viewBox="0 0 100 50"><text>${code.split('\n')[0]}</text></svg>` };
    }),
  },
}));

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
    expect(ids()).toEqual(['k3f9', 'a7b2', 'p7k2', 'mm01', 'orph1']);

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
    expect(document.querySelectorAll('.doc [data-change]').length).toBe(4);
    expect(document.querySelector('.pill-main')!.textContent).toBe('4 changes');
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

  it('shows the no-agent-waiting hint and banner wording', async () => {
    vi.resetModules();
    const { App } = await import('../app');
    const { mockFactory } = await import('../dev/mock');
    const mock = (window as unknown as { __mock: { waiters: (n: number) => void } }).__mock;
    new App(document.getElementById('app')!, 'ws://x/ws/dev', mockFactory);
    await vi.advanceTimersByTimeAsync(500);
    const hint = () => document.querySelector<HTMLElement>('.no-waiter')!;
    expect(hint().hidden).toBe(true);

    mock.waiters(0);
    await vi.advanceTimersByTimeAsync(100);
    expect(hint().hidden).toBe(false);
    expect(hint().textContent).toBe('No agent waiting: edits are saved, Done just closes the round');

    // An agent starts waiting: the tab notices live.
    mock.waiters(1);
    await vi.advanceTimersByTimeAsync(100);
    expect(hint().hidden).toBe(true);

    mock.waiters(0);
    await vi.advanceTimersByTimeAsync(100);
    let confirms = 0;
    window.confirm = () => (confirms++, true);
    (document.querySelector('.btn-primary') as HTMLButtonElement).click();
    await vi.advanceTimersByTimeAsync(300);
    expect(confirms).toBe(0);
    const banner = document.querySelector<HTMLElement>('.banner')!;
    expect(banner.hidden).toBe(false);
    expect(banner.textContent).toBe('✓Review closed. No agent was waiting; your edits and comments are saved in the file.');
    expect(hint().hidden).toBe(true);
    // No agent, so no next round.
    await vi.advanceTimersByTimeAsync(5000);
    expect(document.querySelector('.round-badge')!.textContent).toBe('Round 1');
  });

  it('keeps the sent-back wording when an agent is waiting', async () => {
    vi.resetModules();
    const { App } = await import('../app');
    const { mockFactory } = await import('../dev/mock');
    new App(document.getElementById('app')!, 'ws://x/ws/dev', mockFactory);
    await vi.advanceTimersByTimeAsync(500);
    (document.querySelector('.btn-primary') as HTMLButtonElement).click();
    await vi.advanceTimersByTimeAsync(300);
    const t = document.querySelector('.banner')!.textContent!;
    expect(t).toContain('Sent back to the agent.');
    expect(t).not.toContain('No agent was waiting');
  });
  it('renders the mock diagrams, comments on a whole diagram and shows the modified diagram in round 2', async () => {
    vi.resetModules();
    const { App } = await import('../app');
    const { mockFactory } = await import('../dev/mock');
    const sent: { type: string; selection?: { sourcepos: string; text: string; offset: number } }[] = [];
    const factory = (u: string) => {
      const sock = mockFactory(u);
      const orig = sock.send.bind(sock);
      sock.send = (d: string) => (sent.push(JSON.parse(d)), orig(d));
      return sock;
    };
    new App(document.getElementById('app')!, 'ws://x/ws/dev', factory);
    await vi.advanceTimersByTimeAsync(500);

    const blocks = () => [...document.querySelectorAll<HTMLElement>('.doc .mermaid-block[data-mermaid-for]')];
    expect(blocks().length).toBe(8);
    expect(blocks().filter(b => b.dataset.state === 'ok').length).toBe(7);
    const invalid = blocks()[7];
    expect(invalid.dataset.state).toBe('error');
    expect(invalid.querySelector('.mermaid-error-title')!.textContent).toBe('Mermaid diagram error · line 4');
    expect(invalid.nextElementSibling!.tagName).toBe('PRE');
    expect(document.querySelectorAll('.doc pre[hidden]').length).toBe(7);

    // Existing code comment on the first diagram: framed + badge; the badge focuses its card.
    const flow = blocks()[0];
    expect(flow.hasAttribute('data-commented')).toBe(true);
    const badge = flow.querySelector<HTMLElement>('.mermaid-badge')!;
    expect(badge.textContent).toBe('1');
    badge.click();
    expect(document.querySelector('[data-mc-id="mm01"]')!.classList.contains('is-active')).toBe(true);
    // Its card can jump to the block.
    expect(document.querySelector<HTMLButtonElement>('[data-mc-id="mm01"] .mc-quote')!.disabled).toBe(false);

    // Comment on the sequence diagram: payload covers the whole block.
    const seq = blocks()[1];
    const seqPre = document.querySelector<HTMLElement>(`pre[data-sourcepos="${seq.dataset.mermaidFor}"]`)!;
    seq.querySelector<HTMLElement>('.mermaid-btn-comment')!.click();
    const draft = document.querySelector<HTMLTextAreaElement>('.mc-card-draft textarea')!;
    draft.value = 'Add the agent here';
    draft.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await vi.advanceTimersByTimeAsync(300);
    const add = sent.find(m => m.type === 'add_comment')!;
    expect(add.selection).toEqual({ sourcepos: seq.dataset.mermaidFor, text: seqPre.textContent, offset: 0 });
    const seq2 = blocks()[1];
    expect(seq2.hasAttribute('data-commented')).toBe(true);
    expect(seq2.querySelector('.mermaid-badge')!.textContent).toBe('1');
    expect(blocks()[7].hasAttribute('data-commented')).toBe(false);

    // Done -> round 2: the sequence diagram was modified.
    (document.querySelector('.btn-primary') as HTMLButtonElement).click();
    await vi.advanceTimersByTimeAsync(4500);
    const mod = blocks()[1];
    expect(mod.getAttribute('data-change')).toBe('modified');
    expect(mod.previousElementSibling!.hasAttribute('data-change')).toBe(false);
    const prev = mod.querySelector<HTMLElement>('.mermaid-btn-prev')!;
    prev.click();
    await vi.advanceTimersByTimeAsync(50);
    expect(mod.querySelector('.mermaid-old svg')).not.toBeNull();
    // Hide changes: everything added for the diff disappears; diagrams stay.
    document.querySelector<HTMLElement>('.pill-toggle')!.click();
    expect(document.querySelectorAll('.doc [data-change], .doc .mermaid-old, .doc .mermaid-btn-prev').length).toBe(0);
    expect(blocks().length).toBe(8);
  });
  it('Edit mode: types in the rendered document, saves the block live, keeps comments; closed rounds are read-only', async () => {
    vi.resetModules();
    const { App } = await import('../app');
    const { mockFactory } = await import('../dev/mock');
    const sent: string[] = [];
    const factory = (u: string) => {
      const s = mockFactory(u);
      const orig = s.send.bind(s);
      s.send = (d: string) => (sent.push(d), orig(d));
      return s;
    };
    new App(document.getElementById('app')!, 'ws://x/ws/dev', factory);
    await vi.advanceTimersByTimeAsync(500);
    const seg = [...document.querySelectorAll('.topbar .seg-btn')].map(b => b.textContent);
    expect(seg).toEqual(['Read', 'Edit', 'Source']);
    expect(document.querySelector('.doc [contenteditable]')).toBeNull();
    (document.querySelectorAll('.topbar .seg-btn')[1] as HTMLButtonElement).click();
    expect(localStorage.getItem('annotator.mode')).toBe('edit');
    const p = () => document.querySelector<HTMLElement>('.doc p[data-sourcepos^="5:1-"]')!;
    expect(p().getAttribute('contenteditable')).toBe('true');
    expect(document.querySelector('.doc pre[contenteditable]')).toBeNull();

    // Type at the end of the paragraph that holds the k3f9 comment.
    p().focus();
    const last = p().lastChild as Text;
    last.data = last.data.replace('closes Monday.', 'closes Monday at noon.');
    p().dispatchEvent(new Event('input', { bubbles: true }));
    await vi.advanceTimersByTimeAsync(700);
    const msgs = sent.map(d => JSON.parse(d));
    const rb = msgs.find(m => m.type === 'replace_block');
    expect(rb).toMatchObject({ sourcepos: '5:1-5:97' });
    expect(rb.markdown).toBe('We should <!--c:k3f9-->ship on Friday<!--/c:k3f9--> regardless of the QA timeline, because the release window closes Monday at noon.');
    await vi.advanceTimersByTimeAsync(300);
    const { SAMPLE_RAW } = await import('../dev/fixture');
    const daemon = (window as unknown as { __mock: { daemon: { state: { raw: string } } } }).__mock.daemon;
    expect(daemon.state.raw).toBe(SAMPLE_RAW.replace('closes Monday.', 'closes Monday at noon.'));
    expect(p().textContent).toContain('at noon.');
    expect(p().querySelector('mark[data-cid="k3f9"]')).not.toBeNull();

    // Done: the round closes and Edit mode turns read-only.
    (document.querySelector('.btn-primary') as HTMLButtonElement).click();
    await vi.advanceTimersByTimeAsync(300);
    expect(document.querySelector('.doc [contenteditable]')).toBeNull();
    expect((document.querySelectorAll('.topbar .seg-btn')[1] as HTMLButtonElement).disabled).toBe(true);
  });
});
