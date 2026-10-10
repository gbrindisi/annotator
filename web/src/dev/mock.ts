// In-browser fake daemon for `npm run dev`. Dev-only: it is the one place in web/ that knows
// the comment format, purely to fake what the daemon would push.
import type { SocketFactory, SocketLike } from '../connection';
import type { ClientMsg, CommentView, DocState, ServerMsg } from '../protocol';
import { SAMPLE_HTML, SAMPLE_RAW } from './fixture';

const hash = (s: string): string => {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return (h >>> 0).toString(16).padStart(8, '0');
};

/** Marker/body ranges (UTF-16). Good enough for the fixture (skips the example fence). */
export const fakeMarkerRanges = (raw: string): [number, number][] => {
  const out: [number, number][] = [];
  const re = /<!--\/?c:[^\s>]+?-->|\n?<!--co:[\s\S]*?-->/g;
  for (let m; (m = re.exec(raw)); ) {
    if (/ex01/.test(m[0])) continue;
    out.push([m.index, m.index + m[0].length]);
  }
  return out;
};

const now = () => new Date().toISOString();

const initialComments = (): CommentView[] => [
  {
    id: 'k3f9', status: 'open', quote: 'ship on Friday', author: 'kyle', created_at: '2026-06-17T10:00:00.000Z', orphaned: false, code_lines: null,
    entries: [
      { author: 'kyle', timestamp: '2026-06-17T10:00:00.000Z', text: 'I thought we agreed Thursday?' },
      { author: 'sam', timestamp: '2026-06-17T10:05:00.000Z', text: 'Thursday is better for QA.' },
    ],
    new_entries: 0, new_thread: false,
  },
  {
    id: 'a7b2', status: 'open', quote: 'another highlighted span', author: 'sam', created_at: '2026-06-17T10:05:00.000Z', orphaned: false, code_lines: null,
    entries: [{ author: 'sam', timestamp: '2026-06-17T10:05:00.000Z', text: 'Two anchors near each other.' }], new_entries: 0, new_thread: false,
  },
  {
    id: 'zz1q', status: 'resolved', quote: 'resolved', author: 'kyle', created_at: '2026-06-17T09:00:00.000Z', orphaned: false, code_lines: null,
    entries: [{ author: 'kyle', timestamp: '2026-06-17T09:00:00.000Z', text: 'Handled.' }], new_entries: 0, new_thread: false,
  },
  {
    id: 'p7k2', status: 'open', quote: '    validate(order)', author: 'agent', created_at: null, orphaned: false, code_lines: [1, 1],
    entries: [{ author: 'agent', timestamp: null, text: 'should this run before logging?' }], new_entries: 0, new_thread: false,
  },
  {
    id: 'orph1', status: 'open', quote: 'deleted text', author: 'me', created_at: '2026-06-17T12:00:00.000Z', orphaned: true, code_lines: null,
    entries: [{ author: 'me', timestamp: '2026-06-17T12:00:00.000Z', text: 'orphaned thread' }], new_entries: 0, new_thread: false,
  },
];

/** Wrap [offset, offset+len) of `block`'s textContent in <mark data-cid> (one per text node). */
export const wrapRange = (block: Element, offset: number, len: number, id: string): void => {
  const doc = block.ownerDocument;
  const walker = doc.createTreeWalker(block, NodeFilter.SHOW_TEXT);
  const targets: [Text, number, number][] = [];
  let pos = 0;
  for (let n = walker.nextNode() as Text | null; n; n = walker.nextNode() as Text | null) {
    const start = pos;
    const end = pos + n.data.length;
    pos = end;
    const a = Math.max(offset, start);
    const b = Math.min(offset + len, end);
    if (b > a) targets.push([n, a - start, b - start]);
  }
  for (const [node, a, b] of targets) {
    const mid = node.splitText(a);
    mid.splitText(b - a);
    const mark = doc.createElement('mark');
    mark.setAttribute('data-cid', id);
    mid.replaceWith(mark);
    mark.append(mid);
  }
};

class FakeDaemon {
  state: DocState;
  sockets = new Set<FakeSocket>();
  private seq = 0;

  constructor() {
    this.state = {
      path: '/home/bomber/project/fixtures/sample.md', round: 1, hash: hash(SAMPLE_RAW), raw: SAMPLE_RAW, html: SAMPLE_HTML,
      comments: initialComments(), marker_ranges: fakeMarkerRanges(SAMPLE_RAW), changes: [], round_state: 'active', author: 'bomber',
    };
  }

  push(m: ServerMsg): void {
    for (const s of this.sockets) s.deliver(m);
  }

  pushDoc(): void {
    this.push({ type: 'doc', ...structuredClone(this.state) });
  }

  error(message: string, reloaded = false): void {
    this.push({ type: 'error', message, reloaded });
    if (reloaded) this.pushDoc();
  }

  private editHtml(fn: (root: HTMLElement) => void): void {
    const root = document.createElement('div');
    root.innerHTML = this.state.html;
    fn(root);
    this.state.html = root.innerHTML;
  }

  private setRaw(raw: string): void {
    this.state.raw = raw;
    this.state.hash = hash(raw);
    this.state.marker_ranges = fakeMarkerRanges(raw);
  }

  private thread(id: string): CommentView | undefined {
    return this.state.comments.find(c => c.id === id);
  }

  handle(m: ClientMsg): void {
    const s = this.state;
    if (s.round_state !== 'active' && m.type !== 'done') return this.error('The round is closed');
    const author = s.author;
    const newId = () => `n${(++this.seq).toString(36)}${Math.random().toString(36).slice(2, 4)}`;
    switch (m.type) {
      case 'add_comment': {
        const id = newId();
        let ok = false;
        this.editHtml(root => {
          const block = root.querySelector(`[data-sourcepos="${m.selection.sourcepos}"]`);
          if (!block) return;
          if (!block.textContent?.slice(m.selection.offset).startsWith(m.selection.text)) return;
          wrapRange(block, m.selection.offset, m.selection.text.length, id);
          ok = true;
        });
        if (!ok) return this.error('Could not anchor the selection (cross-block or stale)');
        s.comments.push({ id, status: 'open', quote: m.selection.text, author, created_at: now(), orphaned: false, code_lines: null, entries: [{ author, timestamp: now(), text: m.body }], new_entries: 0, new_thread: s.round > 1 });
        break;
      }
      case 'add_comment_raw': {
        const id = newId();
        const raw = s.raw;
        const lineEnd = raw.indexOf('\n', m.to) === -1 ? raw.length : raw.indexOf('\n', m.to);
        const quote = raw.slice(m.from, m.to);
        const next = raw.slice(0, m.from) + `<!--c:${id}-->` + quote + `<!--/c:${id}-->` + raw.slice(m.to, lineEnd) +
          `\n<!--co:${id} by:${author} at:${now()} status:open quote:${JSON.stringify(quote)}\n${author}: ${m.body}\n-->` + raw.slice(lineEnd);
        this.setRaw(next);
        // The fake daemon does not re-render: the thread shows up as unanchored in rendered mode.
        s.comments.push({ id, status: 'open', quote, author, created_at: now(), orphaned: false, code_lines: null, entries: [{ author, timestamp: now(), text: m.body }], new_entries: 0, new_thread: s.round > 1 });
        break;
      }
      case 'reply': {
        const t = this.thread(m.id);
        if (!t) return this.error(`No thread ${m.id}`);
        t.entries.push({ author, timestamp: now(), text: m.body });
        break;
      }
      case 'edit_entry': {
        const e = this.thread(m.id)?.entries[m.entry];
        if (!e) return this.error('No such entry');
        e.text = m.body;
        break;
      }
      case 'delete_entry': {
        const t = this.thread(m.id);
        if (!t || !t.entries[m.entry]) return this.error('No such entry');
        t.entries.splice(m.entry, 1);
        t.new_entries = Math.min(t.new_entries, t.entries.length);
        break;
      }
      case 'resolve':
      case 'reopen': {
        const t = this.thread(m.id);
        if (!t) return this.error(`No thread ${m.id}`);
        t.status = m.type === 'resolve' ? 'resolved' : 'open';
        break;
      }
      case 'delete_thread': {
        s.comments = s.comments.filter(c => c.id !== m.id);
        this.editHtml(root => root.querySelectorAll(`mark[data-cid="${m.id}"]`).forEach(mk => mk.replaceWith(...mk.childNodes)));
        break;
      }
      case 'replace_source': {
        if (m.base_hash !== s.hash) return this.error('The file changed on disk; your edit was not saved', true);
        this.setRaw(m.text);
        break;
      }
      case 'done': {
        if (s.round_state !== 'active') return;
        s.round_state = 'closed';
        this.push({ type: 'round_closed', round: s.round });
        this.pushDoc();
        window.setTimeout(() => this.nextRound(), 4000);
        return;
      }
      case 'abandon': {
        s.round_state = 'closed';
        this.push({ type: 'round_closed', round: s.round });
        return;
      }
    }
    this.pushDoc();
  }

  /** Simulate the agent's round 2: edits, a removed block, a reply and a new thread. */
  nextRound(): void {
    const s = this.state;
    s.round += 1;
    s.round_state = 'active';
    const oldP = 'Here is a second paragraph with another highlighted span placed close to the first.';
    this.editHtml(root => {
      const p = root.querySelector('[data-sourcepos="7:1-7:83"]');
      if (p) p.innerHTML = 'Here is a <strong data-sourcepos="7:11-7:24">rewritten</strong> second paragraph with <mark data-cid="a7b2">another highlighted span</mark>, now further away.';
      const added = document.createElement('p');
      added.setAttribute('data-sourcepos', '10:1-10:61');
      added.innerHTML = 'QA signed off on <mark data-cid="ag01">Thursday as the release day</mark> after the sync.';
      root.querySelector('[data-sourcepos="9:1-9:43"]')?.after(added);
      root.querySelector('[data-sourcepos="18:1-18:33"]')?.remove();
    });
    s.changes = [
      { kind: 'modified', sourcepos: '7:1-7:83', old_text: oldP, after: null },
      { kind: 'added', sourcepos: '10:1-10:61', old_text: null, after: null },
      { kind: 'removed', sourcepos: null, old_text: 'This body has no anchor any more.', after: '13:1-16:3' },
    ];
    for (const c of s.comments) {
      c.new_entries = 0;
      c.new_thread = false;
    }
    const k = this.thread('k3f9');
    if (k) {
      k.entries.push({ author: 'agent', timestamp: now(), text: 'Moved to Thursday, see the new paragraph below.' });
      k.new_entries = 1;
    }
    s.comments.push({ id: 'ag01', status: 'open', quote: 'Thursday as the release day', author: 'agent', created_at: now(), orphaned: false, code_lines: null, entries: [{ author: 'agent', timestamp: now(), text: 'Added this; please confirm.' }], new_entries: 1, new_thread: true });
    this.setRaw(s.raw.replace('Here is a second', 'Here is a **rewritten** second'));
    this.pushDoc();
  }

  /** Simulate an external edit to the file (for the "changed on disk" notice). */
  touch(): void {
    this.setRaw(this.state.raw + `\nEdited externally at ${now()}.\n`);
    this.pushDoc();
  }
}

class FakeSocket implements SocketLike {
  readyState = 0;
  onopen: ((ev: Event) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;

  constructor(private daemon: FakeDaemon) {
    window.setTimeout(() => {
      if (!daemon.up) return this.close();
      this.readyState = 1;
      daemon.sockets.add(this);
      this.onopen?.(new Event('open'));
      daemon.pushDoc();
    }, 150);
  }

  deliver(m: ServerMsg): void {
    const data = JSON.stringify(m);
    window.setTimeout(() => this.onmessage?.(new MessageEvent('message', { data })), 30);
  }

  send(data: string): void {
    const m = JSON.parse(data) as ClientMsg;
    console.debug('[mock] <-', m);
    window.setTimeout(() => this.daemon.handle(m), 60);
  }

  close(): void {
    this.readyState = 3;
    this.daemon.sockets.delete(this);
    this.onclose?.(new CloseEvent('close'));
  }
}

interface FakeDaemon {
  up: boolean;
}
FakeDaemon.prototype.up = true;

const daemon = new FakeDaemon();

/** Console helpers: __mock.drop(), __mock.down(ms), __mock.touch(), __mock.nextRound(). */
(window as unknown as { __mock: unknown }).__mock = {
  daemon,
  drop: () => [...daemon.sockets].forEach(s => s.close()),
  down: (ms = 5000) => {
    daemon.up = false;
    [...daemon.sockets].forEach(s => s.close());
    window.setTimeout(() => (daemon.up = true), ms);
  },
  touch: () => daemon.touch(),
  nextRound: () => daemon.nextRound(),
  error: (msg = 'Something went wrong') => daemon.error(msg),
};
console.info('[annotator] running against the in-browser mock daemon; see window.__mock');

export const mockFactory: SocketFactory = () => new FakeSocket(daemon);
