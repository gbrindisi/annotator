import type { ParsedComment } from '../format/types';
import { isAnchored, isCodeComment, isOrphan } from '../format/parse';
import { buildComposer, type Composer } from './composer';
import { el } from './dom';
import { absoluteTime, relativeTime } from './time';

export type CardCallbacks = {
  reveal: (id: string) => void;
  reply: (id: string, text: string) => string | void;
  editEntry: (id: string, index: number, text: string) => string | void;
  deleteEntry: (id: string, index: number) => void;
  deleteThread: (id: string) => void;
};

/** Stable fingerprint of the rendered content so unrelated edits do not rebuild the card. */
export const cardSignature = (c: ParsedComment): string => {
  return JSON.stringify([
    c.status,
    c.quote,
    c.author,
    c.createdAt,
    c.open?.from,
    c.thread.map(e => [e.author, e.timestamp, e.text]),
    isAnchored(c),
    isCodeComment(c),
  ]);
};

export class Card {
  readonly el: HTMLElement;
  signature: string;
  private body: HTMLElement;
  private replyComposer: Composer | null = null;
  private editingIndex: number | null = null;
  private replyDraft = '';

  constructor(private comment: ParsedComment, private cb: CardCallbacks) {
    this.signature = cardSignature(comment);
    this.body = el('div', { className: 'mc-card-body' });
    this.el = el('article', { className: 'mc-card', 'data-mc-id': comment.id }, [this.body]);
    this.render();
  }

  get id(): string {
    return this.comment.id;
  }

  update(comment: ParsedComment): void {
    this.comment = comment;
    this.signature = cardSignature(comment);
    this.render();
  }

  setActive(active: boolean): void {
    this.el.classList.toggle('is-active', active);
  }

  private render(): void {
    const c = this.comment;
    const readOnly = false;
    this.el.classList.toggle('is-resolved', c.status === 'resolved');
    this.el.classList.toggle('is-readonly', readOnly);
    if (this.replyComposer) this.replyDraft = this.replyComposer.textarea.value;
    this.body.replaceChildren();

    const badges: HTMLElement[] = [];
    if (isOrphan(c)) badges.push(el('span', { className: 'mc-badge', title: 'The anchored text was edited or removed', text: 'orphaned' }));
    if (isCodeComment(c)) badges.push(el('span', { className: 'mc-badge', title: 'Anchored to lines inside a fenced code block', text: 'code' }));
    if (c.status === 'resolved') badges.push(el('span', { className: 'mc-badge', text: 'resolved' }));

    const quote = el('button', {
      className: 'mc-quote',
      type: 'button',
      title: isAnchored(c) ? 'Jump to the commented text' : 'No anchor to jump to',
      disabled: !isAnchored(c),
    }, [el('span', { text: c.quote ?? '(no quote)' })]);
    quote.addEventListener('click', () => this.cb.reveal(c.id));

    const menu = el('button', { className: 'mc-icon-btn', type: 'button', title: 'Delete comment thread', 'aria-label': 'Delete comment thread', text: '×' });
    menu.addEventListener('click', () => this.cb.deleteThread(c.id));

    this.body.append(el('header', { className: 'mc-card-head' }, [...badges, el('span', { className: 'mc-spacer' }), menu]));
    this.body.append(quote);

    const entries = el('div', { className: 'mc-entries' });
    if (c.thread.length === 0) {
      entries.append(el('div', { className: 'mc-entry mc-entry-empty', text: 'Empty comment' }));
    }
    c.thread.forEach((entry, index) => entries.append(this.renderEntry(index, entry.author, entry.timestamp ?? (index === 0 ? c.createdAt : undefined), entry.text, readOnly)));
    this.body.append(entries);

    if (!readOnly) {
      this.replyComposer = buildComposer({
        placeholder: 'Reply…',
        submitLabel: 'Reply',
        compact: true,
        initialValue: this.replyDraft,
        onSubmit: text => {
          const error = this.cb.reply(c.id, text);
          if (!error) this.replyDraft = '';
          return error;
        },
        onCancel: () => {
          if (this.replyComposer) {
            this.replyComposer.textarea.value = '';
            this.replyDraft = '';
          }
        },
      });
      this.body.append(this.replyComposer.el);
    } else {
      this.replyComposer = null;
    }
  }

  private renderEntry(index: number, author: string, timestamp: string | undefined, text: string, readOnly: boolean): HTMLElement {
    const root = el('div', { className: 'mc-entry' });
    const head = el('div', { className: 'mc-entry-head' }, [
      el('span', { className: 'mc-author', text: author || 'unknown' }),
      el('time', { className: 'mc-when', title: absoluteTime(timestamp), text: relativeTime(timestamp) }),
      el('span', { className: 'mc-spacer' }),
    ]);
    root.append(head);

    if (this.editingIndex === index) {
      const composer = buildComposer({
        placeholder: 'Edit…',
        submitLabel: 'Save',
        compact: true,
        initialValue: text,
        autofocus: true,
        onSubmit: value => {
          const error = this.cb.editEntry(this.comment.id, index, value);
          if (!error) {
            this.editingIndex = null;
          }
          return error;
        },
        onCancel: () => {
          this.editingIndex = null;
          this.render();
        },
      });
      root.append(composer.el);
      return root;
    }

    root.append(el('p', { className: 'mc-text', text }));
    if (!readOnly) {
      const edit = el('button', { className: 'mc-link', type: 'button', text: 'Edit' });
      edit.addEventListener('click', () => {
        this.editingIndex = index;
        this.render();
      });
      const remove = el('button', { className: 'mc-link', type: 'button', text: 'Delete' });
      remove.addEventListener('click', () => this.cb.deleteEntry(this.comment.id, index));
      head.append(edit, remove);
    }
    return root;
  }
}
