import { applyChanges, clearChanges } from './changes';
import { Connection, type ConnState, type SocketFactory } from './connection';
import { cidAt, markOrder, marksFor, styleMarks } from './marks';
import { MermaidView, type Loader } from './mermaid';
import { BlockEditor } from './edit';
import { docFromMsg, msg, type ClientMsg, type DocState, type Selection, type ServerMsg } from './protocol';
import { currentRange, selectionPayload } from './selection';
import { SourceEditor } from './source';
import { el } from './ui/dom';
import { Sidebar } from './ui/sidebar';

/** `rendered` = Read (the stored value predates Edit mode). */
type Mode = 'rendered' | 'edit' | 'source';
const MODE_KEY = 'annotator.mode';
const CHANGES_KEY = 'annotator.showChanges';
const SAVE_DEBOUNCE = 500;

const parseMode = (v: string | null): Mode => (v === 'source' || v === 'edit' ? v : 'rendered');

export const baseName = (p: string): string => p.split(/[\\/]/).pop() || p;

export class App {
  private conn: Connection;
  private doc: DocState | null = null;
  private mode: Mode = parseMode(localStorage.getItem(MODE_KEY));
  private closed = false;
  private closedRound = 0;
  /** Whether nobody was waiting when Done was clicked here (null = unknown / not via this tab). */
  private doneNoWaiter: boolean | null = null;
  private showChanges = localStorage.getItem(CHANGES_KEY) !== 'off';
  private changeEls: HTMLElement[] = [];
  private changeIdx = -1;
  private unseenRound = false;

  // Source-mode autosave state.
  private saveTimer: number | undefined;
  private dirty = false; // local edits not sent yet
  private lastSent: string | null = null;
  private baseHash = '';
  private stale: DocState | null = null; // Doc received while dirty

  // DOM
  private root: HTMLElement;
  private fileEl = el('span', { className: 'file-name' });
  private roundEl = el('span', { className: 'round-badge' });
  private connEl = el('button', { className: 'conn', type: 'button' });
  private changesPill = el('div', { className: 'changes-pill', hidden: true });
  private changesStep = el('button', { className: 'pill-main', type: 'button', title: 'Next change (n)' });
  private changesToggle = el('button', { className: 'pill-toggle', type: 'button' });
  private changesLegend = el('span', { className: 'changes-legend', hidden: true });
  private modeBtns: Record<Mode, HTMLButtonElement>;
  private doneBtn = el('button', { className: 'btn btn-primary', type: 'button', text: 'Done', title: 'Send the review back to the agent' });
  private abandonBtn = el('button', { className: 'btn btn-ghost', type: 'button', text: 'Abandon' });
  private noWaiterEl = el('span', {
    className: 'no-waiter',
    text: 'No agent waiting: edits are saved, Done just closes the round',
    title: 'Opened without an agent waiting (e.g. from Finder). Your edits and comments are written to the file as you go.',
    hidden: true,
  });
  private banner = el('div', { className: 'banner', hidden: true });
  private notice = el('div', { className: 'notice', hidden: true });
  private pane = el('main', { className: 'doc-pane' });
  private rendered = el('article', { className: 'doc markdown-body' });
  private sourceHost = el('div', { className: 'source-host' });
  private floatBtn = el('button', { className: 'float-comment', type: 'button', text: 'Comment', title: 'Comment on selection (Ctrl/Cmd-Alt-M)', hidden: true });
  private toasts = el('div', { className: 'toasts', 'aria-live': 'polite' });
  private flashEl = el('div', { className: 'round-flash', hidden: true });
  private sidebar: Sidebar;
  private editor: SourceEditor;
  private mermaid: MermaidView;
  private blockEdit: BlockEditor;
  private savedTop = 0;

  constructor(mount: HTMLElement, url: string, factory?: SocketFactory, opts: { mermaidLoader?: Loader } = {}) {
    this.root = mount;
    this.mermaid = new MermaidView(
      { comment: sel => this.commentOnBlock(sel), focusThread: id => this.setActive(id, true) },
      opts.mermaidLoader,
    );
    const mk = (m: Mode, label: string) => {
      const b = el('button', { className: 'seg-btn', type: 'button', text: label });
      b.addEventListener('click', () => this.setMode(m));
      return b;
    };
    this.modeBtns = { rendered: mk('rendered', 'Read'), edit: mk('edit', 'Edit'), source: mk('source', 'Source') };
    this.modeBtns.edit.title = 'Type directly in the document; edits are saved as you go';
    this.changesPill.append(this.changesStep, this.changesToggle);
    this.changesLegend.append(
      el('span', { className: 'legend-ins', text: 'added' }),
      el('span', { className: 'legend-del', text: 'removed' }),
      el('span', { className: 'legend-note', text: 'since your last review' }),
    );

    this.sidebar = new Sidebar({
      reveal: id => this.reveal(id),
      focus: id => this.setActive(id, false),
      reply: (id, body) => this.send(msg.reply(id, body)),
      editEntry: (id, i, body) => this.send(msg.editEntry(id, i, body)),
      deleteEntry: (id, i) => this.send(msg.deleteEntry(id, i)),
      deleteThread: id => this.send(msg.deleteThread(id)),
      resolve: id => this.send(msg.resolve(id)),
      reopen: id => this.send(msg.reopen(id)),
    });

    const top = el('header', { className: 'topbar' }, [
      el('div', { className: 'title-group' }, [el('span', { className: 'logo', text: '✎' }), this.fileEl, this.roundEl, this.connEl]),
      el('span', { className: 'spacer' }),
      this.noWaiterEl,
      this.changesPill,
      this.changesLegend,
      el('div', { className: 'seg' }, [this.modeBtns.rendered, this.modeBtns.edit, this.modeBtns.source]),
      this.abandonBtn,
      this.doneBtn,
    ]);
    this.pane.append(this.notice, this.rendered, this.sourceHost);
    mount.replaceChildren(top, this.banner, el('div', { className: 'layout' }, [this.pane, this.sidebar.el]), this.floatBtn, this.toasts, this.flashEl);

    this.editor = new SourceEditor(this.sourceHost, {
      onChange: text => this.onSourceEdit(text),
      onSelectionChange: () => this.updateFloat(),
      onCommentShortcut: () => this.startComment(),
    });

    this.blockEdit = new BlockEditor(this.rendered, {
      send: m => this.conn.send(m),
      toast: (t, k) => this.toast(t, k),
      conflict: open => (open ? this.showNotice('edit') : (this.notice.hidden = true)),
      beforeRender: () => (this.savedTop = this.pane.scrollTop),
      afterRender: reset => this.afterRender(reset),
    });

    this.bind();
    this.applyMode();
    this.renderTop();
    this.conn = new Connection(url, { onMessage: m => this.onMessage(m), onState: (s, r) => this.onConnState(s, r) }, factory);
    this.conn.connect();
  }

  // ---------- wiring ----------

  private bind(): void {
    this.connEl.addEventListener('click', () => this.conn.retryNow());
    this.changesStep.addEventListener('click', () => this.stepChange(1));
    this.changesToggle.addEventListener('click', () => {
      this.showChanges = !this.showChanges;
      localStorage.setItem(CHANGES_KEY, this.showChanges ? 'on' : 'off');
      this.decorateRendered();
      this.renderTop();
    });
    this.doneBtn.addEventListener('click', () => this.done());
    this.abandonBtn.addEventListener('click', () => {
      if (window.confirm('Abandon this review? The agent will be told you abandoned it.')) this.send(msg.abandon());
    });

    this.rendered.addEventListener('mouseover', e => {
      const id = cidAt(e.target);
      if (id) this.setActive(id, true);
    });
    this.rendered.addEventListener('click', e => {
      const id = cidAt(e.target);
      if (id && currentRange() === null) this.setActive(id, true);
    });
    document.addEventListener('selectionchange', () => this.updateFloat());
    this.pane.addEventListener('scroll', () => this.updateFloat(), { passive: true });
    this.floatBtn.addEventListener('mousedown', e => e.preventDefault());
    this.floatBtn.addEventListener('click', () => this.startComment());
    document.addEventListener('keydown', e => {
      if ((e.ctrlKey || e.metaKey) && e.altKey && (e.key === 'm' || e.key === 'M' || e.code === 'KeyM')) {
        e.preventDefault();
        this.startComment();
      } else if (e.key === 'n' && !isTyping(e.target) && this.changeEls.length) {
        this.stepChange(e.shiftKey ? -1 : 1);
      }
    });
    window.addEventListener('focus', () => this.clearUnseen());
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && document.hasFocus()) this.clearUnseen();
    });
    window.addEventListener('beforeunload', e => {
      if (this.dirty || this.blockEdit.dirty || this.sidebar.hasUnsentText()) e.preventDefault();
    });
  }

  private send(m: ClientMsg): void {
    if (!this.conn.send(m)) this.toast('Not connected to the annotator daemon; retrying…', 'error');
  }

  // ---------- server messages ----------

  onMessage(m: ServerMsg): void {
    if (m.type === 'doc') this.onDoc(docFromMsg(m));
    else if (m.type === 'error') {
      if (this.blockEdit.onError(m.message, m.reloaded)) return;
      this.toast(m.message + (m.reloaded ? ' (reloaded)' : ''), 'error');
    }
    else if (m.type === 'round_closed') {
      if (this.doneNoWaiter === null && this.doc && this.closedRound < m.round) this.doneNoWaiter = this.noWaiter(this.doc);
      this.closed = true;
      this.closedRound = Math.max(this.closedRound, m.round);
      this.applyReadOnly();
    }
  }

  private onDoc(doc: DocState): void {
    const prev = this.doc;
    const newRound = !!prev && doc.round > prev.round;
    this.doc = doc;
    const wasClosed = this.closed;
    this.closed = doc.round_state === 'closed';
    if (!this.closed && newRound) this.closedRound = 0;
    if (!this.closed) this.doneNoWaiter = null;
    if (newRound) this.changeIdx = -1;

    this.renderRendered(!prev || newRound);
    this.syncSource(doc, newRound);
    this.renderSidebar();
    this.applyReadOnly();
    this.renderTop();
    if (newRound && doc.round_state === 'active' && (wasClosed || prev)) this.announceRound(doc.round);
  }

  private renderRendered(resetScroll: boolean): void {
    if (!this.doc) return;
    // The block editor owns the rendered DOM: it keeps blocks with unsaved edits in place.
    this.blockEdit.render(this.doc, resetScroll);
  }

  private afterRender(resetScroll: boolean): void {
    this.mermaid.attach(this.rendered); // async: never blocks the rest of the render
    this.decorateRendered();
    if (this.mode !== 'source') this.pane.scrollTop = resetScroll ? 0 : this.savedTop;
  }

  private decorateRendered(): void {
    if (!this.doc) return;
    styleMarks(this.rendered, this.doc.comments, this.sidebarActive);
    this.changeEls = this.showChanges ? applyChanges(this.rendered, this.doc.changes) : (clearChanges(this.rendered), []);
    this.mermaid.decorateChanges(this.rendered);
    this.mermaid.decorateComments(this.rendered, this.doc.comments);
    if (this.changeIdx >= this.changeEls.length) this.changeIdx = -1;
    this.blockEdit.afterDecorate();
  }

  private syncSource(doc: DocState, force: boolean): void {
    const local = this.editor.text();
    if (this.lastSent !== null && doc.raw === this.lastSent) {
      // Echo of our own autosave.
      this.baseHash = doc.hash;
      this.lastSent = null;
      if (!this.dirty && local === doc.raw) this.editor.setDoc(doc.raw, doc.marker_ranges, doc.comments);
      return;
    }
    if ((this.dirty || this.lastSent !== null) && local !== doc.raw && !force) {
      this.stale = doc;
      this.showNotice();
      return;
    }
    this.acceptDoc(doc);
  }

  private acceptDoc(doc: DocState): void {
    window.clearTimeout(this.saveTimer);
    this.dirty = false;
    this.lastSent = null;
    this.stale = null;
    this.baseHash = doc.hash;
    this.editor.setDoc(doc.raw, doc.marker_ranges, doc.comments);
    this.notice.hidden = true;
  }

  /** "File changed on disk" notice, for Source mode autosave or Edit mode block saves. */
  private showNotice(kind: 'source' | 'edit' = 'source'): void {
    const reload = el('button', { className: 'btn btn-small', type: 'button', text: 'Reload' });
    const keep = el('button', { className: 'btn btn-small btn-ghost', type: 'button', text: 'Keep mine' });
    if (kind === 'edit') {
      reload.addEventListener('click', () => this.blockEdit.reload());
      keep.addEventListener('click', () => this.blockEdit.keepMine());
    } else {
      reload.addEventListener('click', () => this.doc && this.acceptDoc(this.doc));
      keep.addEventListener('click', () => {
        if (!this.doc) return;
        this.baseHash = this.doc.hash;
        this.stale = null;
        this.notice.hidden = true;
        this.flushSave();
      });
    }
    this.notice.replaceChildren(el('span', { text: 'The file changed on disk while you were editing.' }), el('span', { className: 'spacer' }), keep, reload);
    this.notice.hidden = false;
  }

  // ---------- source editing ----------

  private onSourceEdit(_text: string): void {
    if (this.closed) return;
    this.dirty = true;
    window.clearTimeout(this.saveTimer);
    if (this.stale) return; // wait for the user to resolve the conflict notice
    this.saveTimer = window.setTimeout(() => this.flushSave(), SAVE_DEBOUNCE);
  }

  private flushSave(): void {
    window.clearTimeout(this.saveTimer);
    if (!this.dirty || this.stale || !this.doc) return;
    const text = this.editor.text();
    if (this.conn.send(msg.replaceSource(text, this.baseHash))) {
      this.dirty = false;
      this.lastSent = text;
    } else {
      this.saveTimer = window.setTimeout(() => this.flushSave(), 2000);
    }
  }

  // ---------- comments / selection ----------

  private updateFloat(): void {
    if (this.closed) {
      this.floatBtn.hidden = true;
      return;
    }
    let rect: { left: number; top: number; bottom: number } | null = null;
    if (this.mode !== 'source') {
      const range = currentRange();
      const p = range ? selectionPayload(range, this.rendered) : null;
      if (range && p) {
        const rects = range.getClientRects();
        const r = rects.length ? rects[rects.length - 1] : range.getBoundingClientRect();
        rect = { left: r.right, top: r.top, bottom: r.bottom };
      }
    } else {
      const s = this.editor.selection();
      if (s && this.editor.view.hasFocus) {
        const c = this.editor.view.coordsAtPos(s.to);
        if (c) rect = { left: c.right, top: c.top, bottom: c.bottom };
      }
    }
    if (!rect) {
      this.floatBtn.hidden = true;
      return;
    }
    this.floatBtn.hidden = false;
    const w = this.floatBtn.offsetWidth || 90;
    this.floatBtn.style.left = `${Math.max(8, Math.min(window.innerWidth - w - 8, rect.left - w / 2))}px`;
    this.floatBtn.style.top = `${rect.bottom + 8}px`;
  }

  private startComment(): void {
    if (this.closed || !this.doc) return;
    if (this.mode !== 'source') {
      const range = currentRange();
      const p = selectionPayload(range, this.rendered);
      if (!p) return;
      if (!p.ok) {
        this.toast(p.error, 'error');
        return;
      }
      const selection = p.selection;
      // Edit mode: the block may have unsaved edits. Save first, then refer to the block by its
      // (possibly updated) sourcepos.
      const host = this.blockEdit.hostAt(range?.commonAncestorContainer ?? null);
      const idx = host ? this.blockEdit.indexOf(host) : -1;
      this.sidebar.openDraft({
        quote: selection.text,
        submit: body =>
          this.blockEdit.whenSaved(() => {
            const h = idx >= 0 ? this.blockEdit.hostByIndex(idx) : null;
            const sp = h ? (h.closest('[data-sourcepos]')?.getAttribute('data-sourcepos') ?? null) : null;
            this.send(msg.addComment(sp ? { ...selection, sourcepos: sp } : selection, body));
          }),
      });
    } else {
      const s = this.editor.selection();
      if (!s) return;
      const quote = this.editor.view.state.sliceDoc(s.from, s.to);
      this.sidebar.openDraft({
        quote,
        submit: body => {
          this.flushSave(); // offsets refer to the editor text; make sure the daemon has it
          this.send(msg.addCommentRaw(s.from, s.to, body));
        },
      });
    }
    this.floatBtn.hidden = true;
  }

  /** "Comment" on a rendered diagram: the selection payload covers the whole code block. */
  private commentOnBlock(selection: Selection): void {
    if (this.closed || !this.doc) return;
    if (!selection.text.trim()) {
      this.toast('This diagram has no source to comment on', 'error');
      return;
    }
    this.sidebar.openDraft({ quote: selection.text, submit: body => this.send(msg.addComment(selection, body)) });
  }

  private sidebarActive: string | null = null;

  private setActive(id: string | null, scrollCard: boolean): void {
    if (this.sidebarActive === id) return;
    this.sidebarActive = id;
    if (this.doc) styleMarks(this.rendered, this.doc.comments, id);
    this.sidebar.setActive(id, scrollCard);
  }

  private reveal(id: string): void {
    this.setActive(id, false);
    if (this.mode === 'source') {
      this.editor.scrollToCid(id);
      return;
    }
    const marks = marksFor(this.rendered, id);
    if (!marks.length) {
      // Code-line comment: scroll to its diagram / code block.
      const sp = this.doc?.comments.find(c => c.id === id)?.block_sourcepos;
      const block = sp ? this.mermaid.revealTarget(this.rendered, sp) : null;
      if (block) {
        block.scrollIntoView({ block: 'center', behavior: 'smooth' });
        flash(block);
      }
      return;
    }
    marks[0].scrollIntoView({ block: 'center', behavior: 'smooth' });
    for (const m of marks) flash(m);
  }

  private renderSidebar(): void {
    if (!this.doc) return;
    // Code-line comments can jump to their code block / diagram when it is in the rendered doc.
    const jumpable = new Set(
      this.doc.comments.filter(c => c.code_lines && c.block_sourcepos && this.mermaid.revealTarget(this.rendered, c.block_sourcepos)).map(c => c.id),
    );
    this.sidebar.render(this.doc.comments, markOrder(this.rendered), this.closed, jumpable);
  }

  // ---------- modes / state ----------

  private setMode(m: Mode): void {
    if (m === this.mode) return;
    if (this.mode === 'source') this.flushSave();
    if (this.mode === 'edit' && this.blockEdit.inConflict) {
      this.toast('Resolve the "file changed on disk" notice first (Keep mine or Reload).', 'error');
      return;
    }
    const go = () => {
      this.mode = m;
      localStorage.setItem(MODE_KEY, m);
      this.sidebar.closeDraft();
      this.applyMode();
    };
    // Leaving Edit mode: let pending block saves land first (usually immediate).
    if (this.mode === 'edit') this.blockEdit.whenSaved(go);
    else go();
  }

  private applyMode(): void {
    this.rendered.hidden = this.mode === 'source';
    this.sourceHost.hidden = this.mode !== 'source';
    this.root.classList.toggle('mode-source', this.mode === 'source');
    this.root.classList.toggle('mode-edit', this.mode === 'edit' && !this.closed);
    this.blockEdit.setEnabled(this.mode === 'edit' && !this.closed);
    for (const [k, b] of Object.entries(this.modeBtns)) b.classList.toggle('is-on', k === this.mode);
    this.floatBtn.hidden = true;
    if (this.mode === 'source') requestAnimationFrame(() => this.editor.view.requestMeasure());
    this.renderTop();
  }

  private applyReadOnly(): void {
    const ro = this.closed;
    this.root.classList.toggle('is-closed', ro);
    this.editor.setReadOnly(ro);
    // A closed round is read-only: Edit mode shows the document as in Read mode.
    this.blockEdit.setClosed(ro);
    this.blockEdit.setEnabled(this.mode === 'edit' && !ro);
    this.root.classList.toggle('mode-edit', this.mode === 'edit' && !ro);
    this.modeBtns.edit.disabled = ro;
    this.modeBtns.edit.title = ro ? 'Editing is off while the round is closed' : 'Type directly in the document; edits are saved as you go';
    this.doneBtn.disabled = ro;
    this.abandonBtn.disabled = ro;
    if (ro) {
      this.floatBtn.hidden = true;
      this.sidebar.closeDraft();
      const next = Math.max(this.doc?.round ?? 0, this.closedRound) + 1;
      if (this.doneNoWaiter)
        this.banner.replaceChildren(
          el('span', { className: 'banner-icon', text: '✓' }),
          el('strong', { text: 'Review closed.' }),
          el('span', { text: ' No agent was waiting; your edits and comments are saved in the file.' }),
        );
      else
        this.banner.replaceChildren(
          el('span', { className: 'banner-icon', text: '↩' }),
          el('strong', { text: 'Sent back to the agent.' }),
          el('span', { text: ` Waiting for round ${next}…` }),
        );
    }
    this.banner.hidden = !ro;
    this.renderSidebar();
    this.renderTop();
  }

  private done(): void {
    if (this.sidebar.hasUnsentText() && !window.confirm('You have an unsent comment. Send the review back anyway?')) return;
    this.flushSave();
    // Decide the closed-banner wording now: the waiter count drops right after round_closed.
    if (this.doc) this.doneNoWaiter = this.noWaiter(this.doc);
    this.blockEdit.whenSaved(() => this.send(msg.done()));
  }

  /** Active round and no agent blocked on `annotator open` (older daemons: assume waiting). */
  private noWaiter(doc: DocState): boolean {
    return doc.round_state === 'active' && (doc.waiters ?? 1) === 0;
  }

  private stepChange(dir: number): void {
    if (!this.changeEls.length) return;
    if (this.mode === 'source') this.setMode('rendered');
    this.changeIdx = (this.changeIdx + dir + this.changeEls.length) % this.changeEls.length;
    const target = this.changeEls[this.changeIdx];
    target.scrollIntoView({ block: 'center', behavior: 'smooth' });
    flash(target);
    this.renderTop();
  }

  private announceRound(round: number): void {
    this.flashEl.textContent = `Round ${round}`;
    this.flashEl.hidden = false;
    this.flashEl.classList.remove('is-anim');
    void this.flashEl.offsetWidth;
    this.flashEl.classList.add('is-anim');
    window.setTimeout(() => (this.flashEl.hidden = true), 2200);
    if (!document.hasFocus()) this.unseenRound = true;
    this.renderTop();
  }

  private clearUnseen(): void {
    if (!this.unseenRound) return;
    this.unseenRound = false;
    this.renderTop();
  }

  private renderTop(): void {
    const d = this.doc;
    const name = d ? baseName(d.path) : 'annotator';
    this.fileEl.textContent = name;
    this.fileEl.title = d?.path ?? '';
    this.roundEl.textContent = d ? `Round ${d.round}` : '';
    this.roundEl.hidden = !d;
    const nobody = !!d && !this.closed && this.noWaiter(d);
    this.noWaiterEl.hidden = !nobody;
    this.doneBtn.title = nobody ? 'Close this review round (no agent is waiting)' : 'Send the review back to the agent';
    document.title = `${this.unseenRound ? '● ' : ''}${name}${d ? ` · round ${d.round}` : ''}`;

    const n = d?.changes.length ?? 0;
    this.changesPill.hidden = n === 0;
    this.changesLegend.hidden = n === 0 || !this.showChanges;
    this.changesStep.textContent = this.showChanges
      ? this.changeIdx >= 0 ? `${this.changeIdx + 1} / ${this.changeEls.length} changes` : `${n} change${n === 1 ? '' : 's'}`
      : `${n} change${n === 1 ? '' : 's'} hidden`;
    this.changesStep.disabled = !this.showChanges;
    this.changesToggle.textContent = this.showChanges ? 'Hide' : 'Show';
    this.changesToggle.title = this.showChanges ? 'Hide changes since your last review' : 'Show changes since your last review';
  }

  private onConnState(s: ConnState, retryIn?: number): void {
    this.connEl.dataset.state = s;
    this.connEl.textContent = s === 'open' ? 'Live' : s === 'connecting' ? 'Connecting…' : `Offline · retry in ${Math.round((retryIn ?? 0) / 1000)}s`;
    this.connEl.title = s === 'closed' ? 'Click to reconnect now' : s === 'open' ? 'Connected to the annotator daemon' : '';
    if (s === 'open' && this.dirty) this.flushSave();
    if (s === 'open' && this.blockEdit.dirty) this.blockEdit.flush();
  }

  toast(text: string, kind: 'info' | 'error' = 'info'): void {
    const t = el('div', { className: `toast is-${kind}`, role: 'status', text });
    t.addEventListener('click', () => t.remove());
    this.toasts.append(t);
    window.setTimeout(() => t.remove(), 6000);
  }
}

const flash = (node: HTMLElement): void => {
  node.classList.remove('is-flash');
  void node.offsetWidth;
  node.classList.add('is-flash');
  window.setTimeout(() => node.classList.remove('is-flash'), 1300);
};

const isTyping = (t: EventTarget | null): boolean =>
  t instanceof HTMLElement && (t.isContentEditable || t.tagName === 'TEXTAREA' || t.tagName === 'INPUT' || !!t.closest('.cm-editor'));
