import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { BlockEditor, SAVE_DELAY, type EditCallbacks } from '../edit';
import { caretOffset, setCaret } from '../editable';
import type { ClientMsg, DocState } from '../protocol';
import { applyWordDiff, wordDiff } from '../worddiff';

const doc = (html: string, hash: string): DocState => ({
  path: '/a.md', round: 1, hash, raw: '', html, comments: [], marker_ranges: [], changes: [], round_state: 'active', author: 'me',
});

const P1 = '<p data-sourcepos="1:1-1:5">Hello</p>';
const P2 = '<p data-sourcepos="3:1-3:5">World</p>';

let root: HTMLElement;
let sent: ClientMsg[];
let cb: EditCallbacks & { toast: Mock<EditCallbacks['toast']>; conflict: Mock<EditCallbacks['conflict']> };
let ed: BlockEditor;

const ps = () => Array.from(root.querySelectorAll<HTMLElement>('p'));
const type = (h: HTMLElement, text: string, caret = text.length) => {
  h.focus();
  h.textContent = text;
  setCaret(h, caret);
  h.dispatchEvent(new Event('input', { bubbles: true }));
};
const key = (h: HTMLElement, k: string, opts: KeyboardEventInit = {}) =>
  h.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...opts }));

beforeEach(() => {
  vi.useFakeTimers();
  root = document.createElement('article');
  document.body.replaceChildren(root);
  sent = [];
  cb = {
    send: m => (sent.push(m), true),
    toast: vi.fn<EditCallbacks['toast']>(),
    conflict: vi.fn<EditCallbacks['conflict']>(),
    beforeRender: () => {},
    afterRender: () => {},
  };
  ed = new BlockEditor(root, cb);
  ed.setEnabled(true);
  ed.render(doc(P1 + '\n' + P2, 'h1'));
});
afterEach(() => vi.useRealTimers());

describe('Edit mode', () => {
  it('makes text blocks editable and saves after the debounce with the last hash', () => {
    expect(ps().every(p => p.getAttribute('contenteditable') === 'true')).toBe(true);
    type(ps()[0], 'Hello there');
    vi.advanceTimersByTime(SAVE_DELAY - 10);
    expect(sent).toEqual([]);
    vi.advanceTimersByTime(20);
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:5', markdown: 'Hello there', base_hash: 'h1' }]);
  });

  it('re-syncs the saved block from the echo and restores the caret', () => {
    const live = ps()[0];
    type(live, 'Hello there', 5);
    vi.advanceTimersByTime(SAVE_DELAY);
    ed.render(doc('<p data-sourcepos="1:1-1:11">Hello there</p>\n' + P2, 'h2'));
    const p = ps()[0];
    expect(p).toBe(live); // renders identically: the live element (and its undo history) stays
    expect(p.getAttribute('data-sourcepos')).toBe('1:1-1:11');
    expect(document.activeElement).toBe(p);
    expect(caretOffset(p)).toBe(5);
    type(p, 'Hello there!');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent[1]).toMatchObject({ sourcepos: '1:1-1:11', base_hash: 'h2' });
  });

  it('a clean focused block re-syncs when the Doc changes it (e.g. a new comment) and keeps the caret', () => {
    const live = ps()[0];
    live.focus();
    setCaret(live, 4);
    ed.render(doc('<p data-sourcepos="1:1-1:5">He<mark data-cid="n1">ll</mark>o</p>\n' + P2, 'h2'));
    const p = ps()[0];
    expect(p).not.toBe(live);
    expect(p.querySelector('mark')).not.toBeNull();
    expect(document.activeElement).toBe(p);
    expect(caretOffset(p)).toBe(4);
  });

  it('never clobbers a dirty block: other blocks update, the edit is re-targeted', () => {
    type(ps()[0], 'Hello local');
    // External edit elsewhere (block 2 changed, block 1 unchanged in the file): silent rebase.
    ed.render(doc(P1 + '\n<p data-sourcepos="3:1-3:9">World new</p>', 'h2'));
    expect(ps()[0].textContent).toBe('Hello local');
    expect(ps()[1].textContent).toBe('World new');
    expect(document.activeElement).toBe(ps()[0]);
    expect(cb.conflict).not.toHaveBeenCalled();
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:5', markdown: 'Hello local', base_hash: 'h2' }]);
  });

  it('a Doc changing the dirty block itself opens the conflict notice; Keep mine retries on the fresh hash', () => {
    type(ps()[0], 'Hello local');
    ed.render(doc('<p data-sourcepos="1:1-1:8">Hello!!!</p>\n' + P2, 'h2'));
    expect(cb.conflict).toHaveBeenCalledWith(true);
    expect(ps()[0].textContent).toBe('Hello local');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([]); // paused while the notice is open
    ed.keepMine();
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:8', markdown: 'Hello local', base_hash: 'h2' }]);
  });

  it('hash conflict from the daemon keeps the local text; Reload shows the file', () => {
    type(ps()[0], 'Hello local');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(ed.onError('file changed on disk; reloaded', true)).toBe(true);
    expect(cb.conflict).toHaveBeenCalledWith(true);
    ed.render(doc('<p data-sourcepos="1:1-1:5">Howdy</p>\n' + P2, 'h3'));
    expect(ps()[0].textContent).toBe('Hello local');
    ed.reload();
    expect(ps()[0].textContent).toBe('Howdy');
    expect(ed.dirty).toBe(false);
  });

  it('a rejected edit is reported and the block shows the file again', () => {
    type(ps()[0], 'Hello bad');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(ed.onError('Comment markers would be unbalanced', false)).toBe(true);
    expect(cb.toast).toHaveBeenCalledWith(expect.stringContaining('unbalanced'), 'error');
    expect(ps()[0].textContent).toBe('Hello');
  });

  it('holds back Docs while a save is in flight (no flicker of the old text)', () => {
    type(ps()[0], 'Hello there');
    vi.advanceTimersByTime(SAVE_DELAY);
    ed.render(doc(P1 + '\n' + P2, 'h1')); // e.g. a waiters update with the old content
    expect(ps()[0].textContent).toBe('Hello there');
    ed.render(doc('<p data-sourcepos="1:1-1:11">Hello there</p>\n' + P2, 'h2'));
    expect(ed.dirty).toBe(false);
  });

  it('Enter splits a block; the tail is saved with it as a new paragraph', () => {
    const p = ps()[0];
    p.focus();
    setCaret(p, 3);
    key(p, 'Enter');
    expect(ps().map(x => x.textContent)).toEqual(['Hel', 'lo', 'World']);
    expect(document.activeElement).toBe(ps()[1]);
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:5', markdown: 'Hel\n\nlo', base_hash: 'h1' }]);
    ed.render(doc('<p data-sourcepos="1:1-1:3">Hel</p>\n<p data-sourcepos="3:1-3:2">lo</p>\n<p data-sourcepos="5:1-5:5">World</p>', 'h2'));
    expect(document.activeElement).toBe(ps()[1]);
    expect(ps()[1].getAttribute('data-sourcepos')).toBe('3:1-3:2');
    expect(ed.dirty).toBe(false);
  });

  it('Enter at the end makes an empty local paragraph that is only saved once typed into', () => {
    const p = ps()[0];
    p.focus();
    setCaret(p, 5);
    key(p, 'Enter');
    const fresh = ps()[1];
    expect(fresh.hasAttribute('data-local')).toBe(true);
    vi.advanceTimersByTime(SAVE_DELAY * 2);
    expect(sent).toEqual([]);
    type(fresh, 'New one');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:5', markdown: 'Hello\n\nNew one', base_hash: 'h1' }]);
  });

  it('Enter in a list item makes a new item; a comment stays with the first part', () => {
    ed.render(doc('<ul data-sourcepos="1:1-2:5">\n<li data-sourcepos="1:1-1:13"><mark data-cid="c1">one two</mark></li>\n<li data-sourcepos="2:1-2:5">three</li>\n</ul>', 'h8'));
    const body = root.querySelector<HTMLElement>('.li-body')!;
    body.focus();
    setCaret(body, 3);
    key(body, 'Enter');
    expect([...root.querySelectorAll('li')].map(li => li.textContent)).toEqual(['one', ' two', 'three']);
    expect(root.querySelectorAll('mark').length).toBe(1);
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:13', markdown: '<!--c:c1-->one<!--/c:c1-->\n\ntwo', base_hash: 'h8' }]);
  });

  it('an empty new block survives unrelated Docs and focus can go back to its predecessor', () => {
    const p = ps()[0];
    p.focus();
    setCaret(p, 5);
    key(p, 'Enter');
    ed.render(doc(P1 + '\n<p data-sourcepos="3:1-3:6">World!</p>', 'h2'));
    expect(ps().map(x => x.textContent)).toEqual(['Hello', '', 'World!']);
    expect(document.activeElement).toBe(ps()[1]);
    ps()[0].focus();
    ed.render(doc(P1 + '\n<p data-sourcepos="3:1-3:7">World!!</p>', 'h3'));
    expect(document.activeElement).toBe(ps()[0]);
  });

  it('Backspace at the start of a block merges it into the previous one', () => {
    const p2 = ps()[1];
    p2.focus();
    setCaret(p2, 0);
    key(p2, 'Backspace');
    expect(ps().map(x => x.textContent)).toEqual(['HelloWorld']);
    expect(caretOffset(ps()[0])).toBe(5);
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([{ type: 'merge_blocks', first: '1:1-1:5', second: '3:1-3:5', markdown: 'HelloWorld', base_hash: 'h1' }]);
  });

  it('pastes plain text only', () => {
    const p = ps()[0];
    p.focus();
    setCaret(p, 5);
    const ev = new Event('paste', { bubbles: true, cancelable: true });
    Object.defineProperty(ev, 'clipboardData', { value: { getData: (t: string) => (t === 'text/plain' ? ' <b>x</b> *y*' : '<b>bold</b><img src=x>') } });
    p.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    expect(p.querySelector('b, img')).toBeNull();
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent[0]).toMatchObject({ markdown: 'Hello \\<b>x\\</b> \\*y\\*' });
  });

  it('removes round-change decorations from a block entering editing; never sends them', () => {
    const p = ps()[0];
    applyWordDiff(p, wordDiff('Bye', 'Hello'));
    expect(p.querySelector('[data-diff]')).not.toBeNull();
    p.focus();
    expect(p.querySelector('[data-diff]')).toBeNull();
    type(p, 'Hello you');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent[0]).toMatchObject({ markdown: 'Hello you' });
  });

  it('comment marks survive an edit around them', () => {
    ed.render(doc('<p data-sourcepos="1:1-1:9">a <mark data-cid="k1">bc</mark> d</p>', 'h5'));
    const p = ps()[0];
    p.focus();
    (p.lastChild as Text).data = ' dd';
    p.dispatchEvent(new Event('input', { bubbles: true }));
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent[0]).toMatchObject({ markdown: 'a <!--c:k1-->bc<!--/c:k1--> dd', base_hash: 'h5' });
  });

  it('Cmd/Ctrl-B wraps the selection in bold; a second press removes it', () => {
    const p = ps()[0];
    p.focus();
    const r = document.createRange();
    r.setStart(p.firstChild!, 1);
    r.setEnd(p.firstChild!, 4);
    getSelection()!.removeAllRanges();
    getSelection()!.addRange(r);
    key(p, 'b', { ctrlKey: true });
    expect(p.innerHTML).toBe('H<strong>ell</strong>o');
    key(p, 'b', { ctrlKey: true });
    expect(p.innerHTML).toBe('Hello');
    vi.advanceTimersByTime(SAVE_DELAY);
    expect(sent).toEqual([]); // net no-op: nothing written
  });

  it('task checkboxes are clickable and saved immediately', () => {
    ed.render(doc('<ul data-sourcepos="1:1-1:10">\n<li data-sourcepos="1:1-1:10"><input type="checkbox" disabled="" /> todo</li>\n</ul>', 'h6'));
    const box = root.querySelector<HTMLInputElement>('input.task-box')!;
    box.click();
    expect(sent).toEqual([{ type: 'replace_block', sourcepos: '1:1-1:10', markdown: '[x] todo', base_hash: 'h6' }]);
  });

  it('Esc leaves the block and saves', () => {
    type(ps()[0], 'Hello esc');
    key(ps()[0], 'Escape');
    expect(document.activeElement).not.toBe(ps()[0]);
    expect(sent).toHaveLength(1);
  });

  it('code blocks are not editable; clicking one hints at Source mode', () => {
    ed.render(doc('<pre data-sourcepos="1:1-3:3"><code>x</code></pre>', 'h7'));
    expect(root.querySelector('[contenteditable]')).toBeNull();
    root.querySelector('code')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(cb.toast).toHaveBeenCalledWith(expect.stringContaining('Source mode'));
  });

  it('disabled (Read mode / closed round): plain render, nothing editable', () => {
    ed.setEnabled(false);
    expect(root.querySelector('[contenteditable]')).toBeNull();
    expect(ps()[0].outerHTML).toBe(P1);
  });

  it('whenSaved waits for the echo', () => {
    const done = vi.fn();
    type(ps()[0], 'Hello there');
    ed.whenSaved(done);
    expect(sent).toHaveLength(1);
    expect(done).not.toHaveBeenCalled();
    ed.render(doc('<p data-sourcepos="1:1-1:11">Hello there</p>\n' + P2, 'h2'));
    expect(done).toHaveBeenCalledOnce();
  });
});
