import type { CommentView } from '../protocol';
import { sortComments } from '../marks';
import { Card, type CardCallbacks } from './card';
import { buildComposer, type Composer } from './composer';
import { el } from './dom';

export type Filter = 'open' | 'all';
const WIDTH_KEY = 'annotator.sidebarWidth';
const FILTER_KEY = 'annotator.filter';
const MIN_W = 240;
const MAX_W = 720;

export type DraftTarget = { quote: string; submit: (body: string) => void };

export class Sidebar {
  readonly el: HTMLElement;
  private list: HTMLElement;
  private draftHost: HTMLElement;
  private count: HTMLElement;
  private filterBtns: Record<Filter, HTMLButtonElement>;
  private cards = new Map<string, Card>();
  private draft: Composer | null = null;
  private activeId: string | null = null;
  filter: Filter;
  private last: { comments: CommentView[]; order: string[]; readOnly: boolean; jumpable: Set<string> } = { comments: [], order: [], readOnly: false, jumpable: new Set() };

  constructor(private cb: CardCallbacks) {
    this.filter = (localStorage.getItem(FILTER_KEY) as Filter) === 'all' ? 'all' : 'open';
    this.count = el('span', { className: 'mc-count', text: '0' });
    const mk = (f: Filter, label: string) => {
      const b = el('button', { className: 'seg-btn', type: 'button', text: label });
      b.addEventListener('click', () => this.setFilter(f));
      return b;
    };
    this.filterBtns = { open: mk('open', 'Open'), all: mk('all', 'All') };
    const handle = el('div', { className: 'mc-resize-handle', title: 'Drag to resize', role: 'separator', 'aria-orientation': 'vertical' });
    this.draftHost = el('div', { className: 'mc-draft-host' });
    this.list = el('div', { className: 'mc-list' });
    this.el = el('aside', { className: 'mc-pane' }, [
      handle,
      el('div', { className: 'mc-head' }, [
        el('h2', { className: 'mc-title', text: 'Comments' }),
        this.count,
        el('span', { className: 'mc-spacer' }),
        el('div', { className: 'seg seg-small' }, [this.filterBtns.open, this.filterBtns.all]),
      ]),
      this.draftHost,
      this.list,
    ]);
    this.syncFilterButtons();
    this.initResize(handle);
  }

  private initResize(handle: HTMLElement): void {
    const stored = Number(localStorage.getItem(WIDTH_KEY));
    if (stored >= MIN_W && stored <= MAX_W) document.documentElement.style.setProperty('--sidebar-width', `${stored}px`);
    handle.addEventListener('pointerdown', ev => {
      ev.preventDefault();
      handle.setPointerCapture(ev.pointerId);
      document.body.classList.add('is-resizing');
      const move = (e: PointerEvent) => {
        const w = Math.round(Math.min(MAX_W, Math.max(MIN_W, window.innerWidth - e.clientX)));
        document.documentElement.style.setProperty('--sidebar-width', `${w}px`);
        localStorage.setItem(WIDTH_KEY, String(w));
      };
      const up = () => {
        document.body.classList.remove('is-resizing');
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', up);
      };
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', up);
    });
  }

  setFilter(f: Filter): void {
    this.filter = f;
    localStorage.setItem(FILTER_KEY, f);
    this.syncFilterButtons();
    this.render(this.last.comments, this.last.order, this.last.readOnly, this.last.jumpable);
  }

  private syncFilterButtons(): void {
    for (const [f, b] of Object.entries(this.filterBtns)) b.classList.toggle('is-on', f === this.filter);
  }

  /** `order` = mark order in the rendered doc (ids anchored there); `jumpable` = code comments whose block is in the doc. */
  render(comments: CommentView[], order: string[], readOnly: boolean, jumpable: Set<string> = new Set()): void {
    this.last = { comments, order, readOnly, jumpable };
    const anchored = new Set(order);
    const open = comments.filter(c => c.status !== 'resolved').length;
    this.count.textContent = String(open);
    this.count.title = `${open} open, ${comments.length} total`;
    const visible = sortComments(comments, order).filter(c => this.filter === 'all' || c.status !== 'resolved' || c.id === this.activeId);
    const keep = new Set(visible.map(c => c.id));
    for (const [id, card] of this.cards) {
      if (!comments.some(c => c.id === id)) this.cards.delete(id);
      else if (!keep.has(id)) card.el.remove();
    }
    const nodes: HTMLElement[] = [];
    for (const c of visible) {
      const props = { comment: c, readOnly, anchored: anchored.has(c.id) || jumpable.has(c.id) };
      let card = this.cards.get(c.id);
      if (card) card.update(props);
      else {
        card = new Card(props, this.cb);
        this.cards.set(c.id, card);
      }
      card.setActive(c.id === this.activeId);
      nodes.push(card.el);
    }
    // Reorder without detaching nodes that are already in place (keeps focus in composers).
    nodes.forEach((n, i) => {
      if (this.list.children[i] !== n) this.list.insertBefore(n, this.list.children[i] ?? null);
    });
    while (this.list.children.length > nodes.length) this.list.lastElementChild!.remove();
    if (nodes.length === 0) {
      this.list.append(el('div', { className: 'mc-empty', text: comments.length ? 'No open comments. Switch to “All” to see resolved threads.' : 'No comments yet. Select text in the document to add one.' }));
    }
    if (readOnly) this.closeDraft();
  }

  setActive(id: string | null, scroll = false): void {
    this.activeId = id;
    for (const [cid, card] of this.cards) card.setActive(cid === id);
    if (!id) return;
    let card = this.cards.get(id);
    if (!card?.el.isConnected) {
      // A resolved thread hidden by the filter: show it while active.
      this.render(this.last.comments, this.last.order, this.last.readOnly, this.last.jumpable);
      card = this.cards.get(id);
    }
    if (scroll && card) card.el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  openDraft(target: DraftTarget): void {
    this.closeDraft();
    this.draft = buildComposer({
      placeholder: 'Add a comment…',
      submitLabel: 'Comment',
      autofocus: true,
      onSubmit: body => {
        target.submit(body);
        this.closeDraft();
      },
      onCancel: () => this.closeDraft(),
    });
    const card = el('article', { className: 'mc-card mc-card-draft' }, [
      el('div', { className: 'mc-quote is-static' }, [el('span', { text: target.quote })]),
      this.draft.el,
    ]);
    this.draftHost.append(card);
    this.draft.focus();
  }

  closeDraft(): void {
    this.draft = null;
    this.draftHost.replaceChildren();
  }

  hasUnsentText(): boolean {
    if (this.draft && this.draft.textarea.value.trim()) return true;
    return [...this.cards.values()].some(c => c.hasDraft());
  }
}
