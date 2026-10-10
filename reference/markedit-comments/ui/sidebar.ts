import type { EditorView } from '@codemirror/view';
import type { ParsedComment } from '../format/types';
import { persisted } from '../editor/settings';
import type { Draft } from '../editor/commands';
import { Card, type CardCallbacks } from './card';
import { buildComposer, type Composer } from './composer';
import { el, isolateEvents } from './dom';

export type SidebarCallbacks = CardCallbacks & {
  addComment: (draft: Draft, text: string) => string | void;
  onDraftCancelled: () => void;
  onOpenChanged: (open: boolean) => void;
};

const WIDTH_VAR = '--mc-sidebar-width';
const MIN_WIDTH = 220;

/**
 * Fixed right-hand column listing comment threads. It reserves space by padding `body`
 * rather than laying `body` out as a grid, so it coexists with MarkEdit-preview's split view.
 */
export class Sidebar {
  readonly el: HTMLElement;
  private list: HTMLElement;
  private empty: HTMLElement;
  private count: HTMLElement;
  private composerHost: HTMLElement;
  private cards = new Map<string, Card>();
  private order: string[] = [];
  private draft: { data: Draft; composer: Composer } | null = null;
  private activeId: string | null = null;
  private previewObserver: MutationObserver | null = null;
  private open = false;

  constructor(private view: EditorView, private cb: SidebarCallbacks) {
    this.count = el('span', { className: 'mc-count', text: '0' });
    const close = el('button', { className: 'mc-icon-btn', type: 'button', title: 'Hide comments', 'aria-label': 'Hide comments', text: '×' });
    close.addEventListener('click', () => this.setOpen(false));
    const head = el('header', { className: 'mc-head' }, [el('h2', { className: 'mc-title', text: 'Comments' }), this.count, el('span', { className: 'mc-spacer' }), close]);

    this.composerHost = el('div', { className: 'mc-draft-host' });
    this.list = el('div', { className: 'mc-list' });
    this.empty = el('div', { className: 'mc-empty', text: 'No comments yet. Select text and press the Add Comment shortcut.' });
    const handle = el('div', { className: 'mc-resize-handle', title: 'Drag to resize' });
    this.el = el('aside', { className: 'mc-pane', role: 'complementary', 'aria-label': 'Comments' }, [handle, head, this.composerHost, this.list, this.empty]);
    isolateEvents(this.el);
    this.installResize(handle);
    document.body.appendChild(this.el);

    document.documentElement.style.setProperty(WIDTH_VAR, `${Math.max(MIN_WIDTH, persisted.sidebarWidth())}px`);
    this.watchPreviewOverlay();
  }

  isOpen(): boolean {
    return this.open;
  }

  setOpen(open: boolean): void {
    if (this.open === open) return;
    this.open = open;
    document.body.classList.toggle('mc-sidebar-open', open);
    persisted.setSidebarOpen(open);
    this.view.requestMeasure();
    if (!open && this.draft) this.cancelDraft();
    this.cb.onOpenChanged(open);
  }

  toggle(): void {
    this.setOpen(!this.open);
  }

  /** Rebuild the card list from the current comments, reusing cards whose content did not change. */
  render(comments: ParsedComment[]): void {
    const visible = comments.filter(c => c.body !== null);
    visible.sort((a, b) => (a.open?.from ?? a.body!.from) - (b.open?.from ?? b.body!.from));
    const ids = visible.map(c => c.id);

    for (const [id, card] of this.cards) {
      if (!ids.includes(id)) {
        card.el.remove();
        this.cards.delete(id);
        if (this.activeId === id) this.activeId = null;
      }
    }
    for (const c of visible) {
      const existing = this.cards.get(c.id);
      if (!existing) {
        const card = new Card(c, this.cb);
        card.setActive(c.id === this.activeId);
        this.cards.set(c.id, card);
      } else if (existing.signature !== cardSignatureOf(c)) {
        existing.update(c);
      }
    }
    if (ids.join('\n') !== this.order.join('\n')) {
      this.order = ids;
      this.list.replaceChildren(...ids.map(id => this.cards.get(id)!.el));
    }
    this.count.textContent = String(visible.length);
    this.empty.hidden = visible.length > 0 || this.draft !== null;
  }

  setActive(id: string | null, scroll = false): void {
    if (this.activeId === id) return;
    if (this.activeId) this.cards.get(this.activeId)?.setActive(false);
    this.activeId = id;
    if (id) {
      const card = this.cards.get(id);
      card?.setActive(true);
      if (scroll) card?.el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  }

  /** Show the new-comment composer for a validated selection. */
  openDraft(draft: Draft): void {
    this.setOpen(true);
    this.cancelDraft(false);
    const quote = el('div', { className: 'mc-quote is-static' }, [el('span', { text: draft.quote })]);
    const composer = buildComposer({
      placeholder: 'Write a comment…',
      submitLabel: 'Comment',
      autofocus: true,
      onSubmit: text => {
        const current = this.draft;
        if (!current) return 'The draft was cancelled.';
        const error = this.cb.addComment(current.data, text);
        if (!error) this.cancelDraft(false);
        return error;
      },
      onCancel: () => this.cancelDraft(),
    });
    const card = el('article', { className: 'mc-card mc-card-draft' }, [
      el('header', { className: 'mc-card-head' }, [el('span', { className: 'mc-badge is-accent', text: 'new comment' })]),
      quote,
      composer.el,
    ]);
    this.composerHost.replaceChildren(card);
    this.draft = { data: draft, composer };
    this.empty.hidden = true;
  }

  currentDraft(): Draft | null {
    return this.draft?.data ?? null;
  }

  updateDraft(draft: Draft): void {
    if (this.draft) this.draft.data = draft;
  }

  cancelDraft(notify = true): void {
    if (!this.draft) return;
    this.draft = null;
    this.composerHost.replaceChildren();
    this.empty.hidden = this.cards.size > 0;
    if (notify) this.cb.onDraftCancelled();
  }

  focusDraft(): void {
    this.draft?.composer.focus();
  }

  private installResize(handle: HTMLElement): void {
    let dragging = false;
    handle.addEventListener('pointerdown', event => {
      dragging = true;
      handle.setPointerCapture(event.pointerId);
      document.body.classList.add('mc-resizing');
      event.preventDefault();
    });
    handle.addEventListener('pointermove', event => {
      if (!dragging) return;
      const max = Math.floor(window.innerWidth * 0.6);
      const width = Math.min(max, Math.max(MIN_WIDTH, window.innerWidth - event.clientX));
      document.documentElement.style.setProperty(WIDTH_VAR, `${width}px`);
    });
    const stop = () => {
      if (!dragging) return;
      dragging = false;
      document.body.classList.remove('mc-resizing');
      const width = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(WIDTH_VAR));
      if (Number.isFinite(width)) persisted.setSidebarWidth(width);
      this.view.requestMeasure();
    };
    handle.addEventListener('pointerup', stop);
    handle.addEventListener('pointercancel', stop);
  }

  /** Hide the pane while MarkEdit-preview's full-window preview overlay is showing. */
  private watchPreviewOverlay(): void {
    const check = () => {
      const overlay = document.querySelector('.markdown-body.overlay') !== null;
      document.body.classList.toggle('mc-suppressed', overlay);
    };
    this.previewObserver = new MutationObserver(check);
    this.previewObserver.observe(document.body, { attributes: true, attributeFilter: ['class'], childList: true, subtree: true });
    check();
  }
}

// Re-exported here so the sidebar can compare without importing Card internals twice.
import { cardSignature as cardSignatureOf } from './card';
