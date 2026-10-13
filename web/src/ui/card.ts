import type { CommentView } from '../protocol';
import { buildComposer, type Composer } from './composer';
import { el } from './dom';
import { absoluteTime, relativeTime } from './time';

export type CardCallbacks = {
  reveal: (id: string) => void;
  focus: (id: string) => void;
  reply: (id: string, text: string) => void;
  editEntry: (id: string, index: number, text: string) => void;
  deleteEntry: (id: string, index: number) => void;
  deleteThread: (id: string) => void;
  resolve: (id: string) => void;
  reopen: (id: string) => void;
};

export type CardProps = { comment: CommentView; readOnly: boolean; anchored: boolean };

export const cardSignature = (p: CardProps): string => JSON.stringify([p.comment, p.readOnly, p.anchored]);

export class Card {
  readonly el: HTMLElement;
  signature: string;
  private body: HTMLElement;
  private replyComposer: Composer | null = null;
  private editingIndex: number | null = null;
  private replyDraft = '';

  constructor(private props: CardProps, private cb: CardCallbacks) {
    this.signature = cardSignature(props);
    this.body = el('div', { className: 'mc-card-body' });
    this.el = el('article', { className: 'mc-card', 'data-mc-id': props.comment.id }, [this.body]);
    this.el.addEventListener('mouseenter', () => this.cb.focus(this.id));
    this.render();
  }

  get id(): string {
    return this.props.comment.id;
  }

  /** Text typed into a composer of this card and not sent yet. */
  hasDraft(): boolean {
    return Array.from(this.el.querySelectorAll('textarea')).some(t => t.value.trim() !== '');
  }

  update(props: CardProps): void {
    const sig = cardSignature(props);
    if (sig === this.signature) return;
    this.props = props;
    this.signature = sig;
    this.render();
  }

  setActive(active: boolean): void {
    this.el.classList.toggle('is-active', active);
  }

  private render(): void {
    const { comment: c, readOnly, anchored } = this.props;
    this.el.classList.toggle('is-resolved', c.status === 'resolved');
    this.el.classList.toggle('is-readonly', readOnly);
    if (this.replyComposer) this.replyDraft = this.replyComposer.textarea.value;
    if (readOnly) this.editingIndex = null;
    this.body.replaceChildren();

    const badges: HTMLElement[] = [];
    if (c.new_thread) badges.push(el('span', { className: 'mc-badge is-new', text: 'new' }));
    if (c.code_lines) {
      const [a, b] = c.code_lines;
      badges.push(el('span', { className: 'mc-badge', title: 'Anchored to lines inside a fenced code block', text: a === b ? `code L${a + 1}` : `code L${a + 1}–${b + 1}` }));
    } else if (c.orphaned || !anchored) {
      badges.push(el('span', { className: 'mc-badge is-warn', title: 'The anchored text was edited or removed', text: 'orphaned' }));
    }
    if (c.status === 'resolved') badges.push(el('span', { className: 'mc-badge', text: 'resolved' }));

    const head = el('header', { className: 'mc-card-head' }, [...badges, el('span', { className: 'mc-spacer' })]);
    if (!readOnly) {
      const resolved = c.status === 'resolved';
      const toggle = el('button', { className: 'mc-chip', type: 'button', text: resolved ? 'Reopen' : 'Resolve', title: resolved ? 'Reopen thread' : 'Mark thread resolved' });
      toggle.addEventListener('click', () => (resolved ? this.cb.reopen(c.id) : this.cb.resolve(c.id)));
      const del = el('button', { className: 'mc-icon-btn', type: 'button', title: 'Delete comment thread', 'aria-label': 'Delete comment thread', text: '×' });
      del.addEventListener('click', () => {
        if (window.confirm('Delete this comment thread?')) this.cb.deleteThread(c.id);
      });
      head.append(toggle, del);
    }
    this.body.append(head);

    const quote = el('button', {
      className: 'mc-quote',
      type: 'button',
      title: anchored ? (c.code_lines ? 'Jump to the code block' : 'Jump to the commented text') : 'No anchor to jump to',
      disabled: !anchored,
    }, [el('span', { text: c.quote ?? '(no quote)' })]);
    quote.addEventListener('click', () => this.cb.reveal(c.id));
    this.body.append(quote);

    const entries = el('div', { className: 'mc-entries' });
    if (c.entries.length === 0) entries.append(el('div', { className: 'mc-entry mc-entry-empty', text: 'Empty comment' }));
    const firstNew = c.entries.length - c.new_entries;
    c.entries.forEach((entry, index) =>
      entries.append(this.renderEntry(index, entry.author, entry.timestamp ?? (index === 0 ? c.created_at : null), entry.text, readOnly, !c.new_thread && c.new_entries > 0 && index >= firstNew)),
    );
    this.body.append(entries);

    if (!readOnly) {
      this.replyComposer = buildComposer({
        placeholder: 'Reply…',
        submitLabel: 'Reply',
        compact: true,
        initialValue: this.replyDraft,
        onSubmit: text => {
          this.cb.reply(c.id, text);
          this.replyDraft = '';
          if (this.replyComposer) this.replyComposer.textarea.value = '';
        },
        onCancel: () => {
          if (this.replyComposer) this.replyComposer.textarea.value = '';
          this.replyDraft = '';
        },
      });
      this.body.append(this.replyComposer.el);
    } else {
      this.replyComposer = null;
    }
  }

  private renderEntry(index: number, author: string, timestamp: string | null, text: string, readOnly: boolean, isNew: boolean): HTMLElement {
    const root = el('div', { className: `mc-entry${isNew ? ' is-new' : ''}` });
    const head = el('div', { className: 'mc-entry-head' }, [
      el('span', { className: 'mc-author', text: author || 'unknown' }),
      el('time', { className: 'mc-when', title: absoluteTime(timestamp ?? undefined), text: relativeTime(timestamp ?? undefined) }),
      isNew ? el('span', { className: 'mc-badge is-new', text: 'new' }) : null,
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
          this.editingIndex = null;
          if (value !== text) this.cb.editEntry(this.id, index, value);
          this.render();
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
      remove.addEventListener('click', () => {
        if (window.confirm('Delete this entry?')) this.cb.deleteEntry(this.id, index);
      });
      head.append(edit, remove);
    }
    return root;
  }
}
