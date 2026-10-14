// Edit mode: WYSIWYG editing of the rendered document, one block at a time.
//
// Text blocks (paragraphs, headings, list item text, table cells) are contenteditable. An
// edited block is serialized on its own (`blockmd.ts`) and sent as `replace_block` (debounced,
// and on blur) with the last known hash; Backspace at the start of a block sends `merge_blocks`,
// Enter splits a block (the new block is local until it is saved with its predecessor as
// `replace_block` with a blank line). The daemon is the only writer and pushes a fresh Doc.
//
// Reconciling Docs: a dirty block (unsent edits) is never clobbered. Each render rebuilds the
// view from the Doc and puts the live dirty blocks back in place of their counterparts (same
// position in the list of editable blocks). While a save is in flight, Docs are held back until
// the echo of the save arrives (recognized by the saved blocks serializing identically). A Doc
// that changes the file under a dirty block opens the "file changed on disk" notice (Keep mine
// re-targets the block in the fresh Doc and retries; Reload drops the local edit).
import { blockKind, blockText, serializeBlock, taskBox } from './blockmd';
import {
  caretOffset,
  editableHosts,
  hostOf,
  LOCAL_ATTR,
  newHostLike,
  outerOf,
  prepareEditable,
  setCaret,
  spOf,
  textLength,
  undecorate,
} from './editable';
import { msg, type ClientMsg, type DocState } from './protocol';

export const SAVE_DELAY = 600;
const ECHO_TIMEOUT = 3000;
const HINT_EVERY = 5000;

export interface EditCallbacks {
  send(m: ClientMsg): boolean;
  toast(text: string, kind?: 'info' | 'error'): void;
  /** Show (true) or hide the "file changed on disk" notice for Edit mode. */
  conflict(open: boolean): void;
  beforeRender(): void;
  /** The rendered view was rebuilt (attach diagrams, decorations, restore scroll). */
  afterRender(resetScroll: boolean): void;
}

interface Merge {
  sp: string;
  /** Serialization of the merged block when it was last synced (rebase check). */
  synced: string;
}

/** A dirty block with the local blocks split off after it and a pending merge. */
interface Unit {
  base: HTMLElement;
  extras: HTMLElement[];
  merge: Merge | null;
  /** false = only holds an empty new block (nothing to save yet). */
  dirty: boolean;
}

interface Inflight {
  unit: Unit;
  members: HTMLElement[];
  idx: number;
  sent: string[];
}

const ser = (h: HTMLElement, k: number): string => serializeBlock(h, { task: k === 0 });

export class BlockEditor {
  private enabled = false;
  private closed = false;
  private doc: DocState | null = null;
  private baseHash = '';
  private units: Unit[] = [];
  private inflight: Inflight | null = null;
  private pendingNow = false;
  /** Latest Doc held back while a save is in flight. */
  private stale: DocState | null = null;
  /** Latest Doc while the conflict notice is open. */
  private conflictDoc: DocState | null = null;
  private timer: number | undefined;
  private echoTimer: number | undefined;
  private synced = new WeakMap<HTMLElement, string>();
  private quiet = false;
  private active: HTMLElement | null = null;
  private waiters: (() => void)[] = [];
  private lastHint = 0;
  private resetNext = false;

  constructor(
    private root: HTMLElement,
    private cb: EditCallbacks,
  ) {
    this.bind();
  }

  // ---------- public API ----------

  get isEnabled(): boolean {
    return this.enabled;
  }

  /** Unsaved or in-flight edits. */
  get dirty(): boolean {
    return !!this.inflight || this.units.some(u => u.dirty);
  }

  get inConflict(): boolean {
    return !!this.conflictDoc;
  }

  setEnabled(on: boolean): void {
    if (on === this.enabled) return;
    this.enabled = on;
    if (!on) {
      window.clearTimeout(this.timer);
      this.units = [];
      this.active = null;
    }
    if (this.doc) this.rebuild(this.doc, () => null);
  }

  setClosed(closed: boolean): void {
    this.closed = closed;
    if (closed) window.clearTimeout(this.timer);
  }

  /** Render a Doc from the daemon (any mode). */
  render(doc: DocState, resetScroll = false): void {
    this.doc = doc;
    this.resetNext = this.resetNext || resetScroll;
    if (!this.enabled) {
      this.baseHash = doc.hash;
      this.rebuild(doc, () => null);
      return;
    }
    if (this.inflight) {
      if (this.isEcho(doc)) this.resolve(doc);
      else this.stale = doc;
      return;
    }
    if (this.conflictDoc) {
      this.conflictDoc = doc;
      this.rebuild(doc, this.plainIndex());
      return;
    }
    if (this.dirty && doc.hash !== this.baseHash) {
      if (this.canRebase(doc)) this.baseHash = doc.hash;
      else {
        this.conflictDoc = doc;
        this.cb.conflict(true);
      }
      this.rebuild(doc, this.plainIndex());
      return;
    }
    this.baseHash = doc.hash;
    this.rebuild(doc, this.plainIndex());
  }

  /** Send the first dirty block now (queued behind a save in flight). */
  flush(): void {
    window.clearTimeout(this.timer);
    if (this.conflictDoc || this.closed || !this.enabled) return;
    if (this.inflight) {
      this.pendingNow = this.units.some(u => u.dirty);
      return;
    }
    const u = this.units.find(x => x.dirty);
    if (!u) return this.drain();
    const send = u.extras.filter(x => blockText(x) !== '');
    const keep = u.extras.filter(x => !send.includes(x) && x === this.active);
    for (const x of u.extras) if (!send.includes(x) && !keep.includes(x)) outerOf(x).remove();
    const members = [u.base, ...send];
    const mds = members.map(ser);
    const md = mds.join('\n\n');
    const sp = spOf(u.base);
    const drop = () => (this.units = this.units.filter(x => x !== u));
    if (!send.length && !u.merge && md === this.synced.get(u.base)) {
      // No-op (e.g. formatting toggled twice): nothing to write.
      drop();
      if (keep.length) this.units.push({ base: u.base, extras: keep, merge: null, dirty: false });
      return this.flush();
    }
    if (!sp) {
      drop();
      this.cb.toast('This block can no longer be saved; reload the page.', 'error');
      return;
    }
    const m = u.merge ? msg.mergeBlocks(sp, u.merge.sp, md, this.baseHash) : msg.replaceBlock(sp, md, this.baseHash);
    const idx = editableHosts(this.root).indexOf(u.base);
    if (idx < 0 || !this.cb.send(m)) {
      this.timer = window.setTimeout(() => this.flush(), 2000);
      return;
    }
    drop();
    if (keep.length) this.units.push({ base: u.base, extras: keep, merge: null, dirty: false });
    this.inflight = { unit: { ...u, extras: send }, members, idx, sent: mds };
    this.pendingNow = false;
    window.clearTimeout(this.echoTimer);
    this.echoTimer = window.setTimeout(() => this.echoTimeout(), ECHO_TIMEOUT);
  }

  /** Run `fn` once every edit is saved (immediately when nothing is pending). */
  whenSaved(fn: () => void): void {
    if (!this.dirty) return fn();
    this.waiters.push(fn);
    this.flush();
  }

  /** An `error` from the daemon. Returns true when it concerned a block save. */
  onError(message: string, reloaded: boolean): boolean {
    const inf = this.inflight;
    if (!inf) return false;
    this.inflight = null;
    window.clearTimeout(this.echoTimer);
    if (reloaded) {
      this.restore(inf);
      this.conflictDoc = this.stale ?? this.doc;
      this.stale = null;
      this.cb.conflict(true);
      return true;
    }
    // Rejected (the file is untouched): drop the edit and show the file's version.
    this.cb.toast(`Edit not saved: ${message}`, 'error');
    this.units = this.units.filter(u => !inf.members.includes(u.base));
    const d = this.stale ?? this.doc;
    this.stale = null;
    if (d) {
      this.baseHash = d.hash;
      this.rebuild(d, this.plainIndex());
    }
    this.flush();
    return true;
  }

  /** Conflict notice: keep the local text and save it against the fresh file. */
  keepMine(): void {
    const d = this.conflictDoc;
    if (!d) return;
    this.conflictDoc = null;
    this.cb.conflict(false);
    this.baseHash = d.hash;
    this.flush();
  }

  /** Conflict notice: drop local edits and show the file. */
  reload(): void {
    const d = this.conflictDoc ?? this.doc;
    this.conflictDoc = null;
    this.cb.conflict(false);
    this.units = [];
    if (!d) return;
    this.baseHash = d.hash;
    this.root.querySelectorAll(`[${LOCAL_ATTR}]`).forEach(h => outerOf(h as HTMLElement).remove());
    this.rebuild(d, this.plainIndex());
    this.drain();
  }

  /** After decorations were (re)applied: keep them out of the block being edited. */
  afterDecorate(): void {
    if (this.active) undecorate(this.active);
    for (const u of this.units) undecorate(u.base);
  }

  /** The host containing `node`, if any. */
  hostAt(node: Node | null): HTMLElement | null {
    return this.enabled ? hostOf(this.root, node) : null;
  }

  /** Index of a host among the file's editable blocks (stable across a save of that block). */
  indexOf(h: HTMLElement): number {
    return editableHosts(this.root).indexOf(h);
  }

  hostByIndex(i: number): HTMLElement | null {
    return editableHosts(this.root)[i] ?? null;
  }

  // ---------- rendering ----------

  private isPreserved(h: HTMLElement): boolean {
    return this.units.some(u => u.base === h || u.extras.includes(h));
  }

  /** Index of a host of the current view in the next Doc's editable blocks (no structural change pending). */
  private plainIndex(): (el: HTMLElement) => number | null {
    const old = editableHosts(this.root);
    const merges = this.units.filter(u => u.merge).map(u => old.indexOf(u.base));
    return el => {
      let j = old.indexOf(el);
      if (j < 0) return null;
      for (const b of merges) if (b >= 0 && j > b) j++; // the merged block is still in the file
      return j;
    };
  }

  private echoIndex(inf: Inflight): (el: HTMLElement) => number | null {
    const old = editableHosts(this.root);
    const added = inf.members.length - 1;
    return el => {
      const k = inf.members.indexOf(el);
      if (k >= 0) return inf.idx + k;
      const j = old.indexOf(el);
      if (j < 0) return null;
      return j > inf.idx ? j + added : j;
    };
  }

  private rebuild(doc: DocState, idxOf: (el: HTMLElement) => number | null): void {
    this.cb.beforeRender();
    const active = this.active?.isConnected ? this.active : null;
    const caret = active ? caretOffset(active) : null;
    const plans = this.units.map(u => ({ u, i: idxOf(u.base) }));
    const activeIdx = active && !this.isPreserved(active) ? idxOf(active) : null;
    const activeMd = activeIdx !== null && active ? serializeBlock(active) : null;
    this.quiet = true;
    try {
      // Daemon-generated, sanitized HTML (comrak safe mode); inserted, never evaluated.
      this.root.innerHTML = doc.html;
      if (!this.enabled) {
        this.active = null;
        return;
      }
      const hosts = prepareEditable(this.root);
      for (const h of hosts) this.synced.set(h, serializeBlock(h));
      for (const { u, i } of plans) {
        const cp = i === null ? undefined : hosts[i];
        if (!cp || cp.tagName !== u.base.tagName) {
          this.units = this.units.filter(x => x !== u);
          if (u.dirty) this.cb.toast('An unsaved edit could not be kept: the block moved in the file.', 'error');
          continue;
        }
        if (!u.dirty) {
          // Only holds a new empty block: adopt the fresh render of its predecessor.
          u.base = cp;
        } else if (u.merge) {
          const second = hosts[(i as number) + 1];
          if (second) {
            u.merge.sp = spOf(second) ?? u.merge.sp;
            removeHost(second);
          }
        }
        if (u.dirty) swapIn(cp, u.base);
        let after = outerOf(u.base);
        for (const x of u.extras) {
          const o = outerOf(x);
          after.after(o);
          after = o;
        }
      }
      let target = active && this.isPreserved(active) ? active : activeIdx !== null ? hosts[activeIdx] : null;
      // The focused block renders exactly as it is: keep the live element (caret, native undo).
      if (active && target && target !== active && target.tagName === active.tagName && this.synced.get(target) === activeMd) {
        swapIn(target, active);
        this.synced.set(active, activeMd!);
        target = active;
      }
      this.active = null;
      if (target?.isConnected && this.root.contains(target)) {
        this.active = target;
        target.focus({ preventScroll: true });
        undecorate(target);
        if (caret !== null) setCaret(target, caret);
      }
    } finally {
      this.quiet = false;
      const reset = this.resetNext;
      this.resetNext = false;
      this.cb.afterRender(reset);
    }
  }

  private detachedHosts(doc: DocState): HTMLElement[] {
    const tmp = this.root.ownerDocument.createElement('div');
    tmp.innerHTML = doc.html;
    return prepareEditable(tmp);
  }

  private isEcho(doc: DocState): boolean {
    const inf = this.inflight;
    if (!inf || doc.hash === this.baseHash) return false;
    const hosts = this.detachedHosts(doc);
    if (inf.sent.length === 1 && inf.sent[0] === '') return true; // the block was emptied (removed)
    return inf.sent.every((md, k) => {
      const h = hosts[inf.idx + k];
      return !!h && ser(h, k) === md;
    });
  }

  /** Can dirty blocks be re-targeted to `doc` silently (their blocks are unchanged there)? */
  private canRebase(doc: DocState): boolean {
    const hosts = this.detachedHosts(doc);
    const idx = this.plainIndex();
    return this.units.every(u => {
      const i = idx(u.base);
      const cp = i === null ? undefined : hosts[i];
      if (!cp || serializeBlock(cp) !== this.synced.get(u.base)) return false;
      return !u.merge || serializeBlock(hosts[(i as number) + 1] ?? cp) === u.merge.synced;
    });
  }

  private resolve(doc: DocState): void {
    const inf = this.inflight!;
    window.clearTimeout(this.echoTimer);
    const idxOf = this.echoIndex(inf);
    this.inflight = null;
    this.stale = null;
    this.baseHash = doc.hash;
    // Saved blocks typed into meanwhile stay (as units): their synced text is what was sent.
    inf.members.forEach((el, k) => {
      this.synced.set(el, inf.sent[k]);
      el.removeAttribute(LOCAL_ATTR);
    });
    this.rebuild(doc, idxOf);
    if (this.pendingNow) this.flush();
    else if (this.units.some(u => u.dirty)) this.schedule();
    else this.drain();
  }

  private echoTimeout(): void {
    const inf = this.inflight;
    if (!inf) return;
    if (this.stale && this.stale.hash !== this.baseHash) {
      // The echo did not serialize identically (non-canonical block); accept the new file.
      this.resolve(this.stale);
      return;
    }
    // Nothing came back (lost message): retry.
    this.inflight = null;
    this.restore(inf);
    this.flush();
  }

  /** A failed save goes back to the dirty units (merged with edits made meanwhile). */
  private restore(inf: Inflight): void {
    const extras = [...inf.unit.extras];
    for (const u of this.units.filter(x => inf.members.includes(x.base))) {
      for (const x of u.extras) if (!extras.includes(x)) extras.push(x);
    }
    this.units = this.units.filter(x => !inf.members.includes(x.base));
    for (const x of extras) x.setAttribute(LOCAL_ATTR, '');
    this.units.unshift({ base: inf.unit.base, extras, merge: inf.unit.merge, dirty: true });
  }

  private drain(): void {
    if (this.dirty) return;
    const w = this.waiters;
    this.waiters = [];
    for (const fn of w) fn();
  }

  private schedule(): void {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => this.flush(), SAVE_DELAY);
  }

  private unitOf(h: HTMLElement): Unit | undefined {
    return this.units.find(u => u.base === h || u.extras.includes(h));
  }

  private markDirty(h: HTMLElement): void {
    if (this.closed) return;
    let u = this.unitOf(h);
    if (!u) {
      u = { base: h, extras: [], merge: null, dirty: true };
      this.units.push(u);
    }
    u.dirty = true;
    this.schedule();
  }

  // ---------- events ----------

  private bind(): void {
    const r = this.root;
    r.addEventListener('focusin', e => {
      if (this.quiet || !this.enabled) return;
      const h = hostOf(r, e.target);
      if (!h) return;
      this.active = h;
      undecorate(h);
    });
    r.addEventListener('focusout', e => {
      if (this.quiet || !this.enabled) return;
      const h = hostOf(r, e.target);
      if (!h) return;
      const next = hostOf(r, (e as FocusEvent).relatedTarget);
      const u = this.unitOf(h);
      if (u && h.hasAttribute(LOCAL_ATTR) && !blockText(h) && next !== h && !this.inflight?.members.includes(h)) {
        outerOf(h).remove();
        u.extras = u.extras.filter(x => x !== h);
        if (!u.dirty && !u.extras.length) this.units = this.units.filter(x => x !== u);
      }
      if (this.active === h) this.active = null;
      if (!next || !u || this.unitOf(next) !== u) this.flush();
    });
    r.addEventListener('input', e => {
      if (!this.enabled) return;
      const h = hostOf(r, e.target);
      if (h) this.markDirty(h);
    });
    r.addEventListener('beforeinput', e => {
      if (!this.enabled) return;
      const h = hostOf(r, e.target);
      if (!h) return;
      const t = (e as InputEvent).inputType ?? '';
      if (this.closed || t.startsWith('format') || t === 'insertFromDrop') e.preventDefault();
      else if (t === 'insertParagraph') {
        e.preventDefault();
        this.split(h);
      }
    });
    r.addEventListener('keydown', e => this.onKey(e));
    r.addEventListener('paste', e => {
      const h = this.enabled ? hostOf(r, e.target) : null;
      if (!h) return;
      e.preventDefault();
      if (this.closed) return;
      // Plain text only: pasted HTML never reaches the document.
      const text = (e as ClipboardEvent).clipboardData?.getData('text/plain') ?? '';
      this.insertText(h, blockKind(h) === 'para' ? text.replace(/\r\n?/g, '\n') : text.replace(/\s*\r?\n\s*/g, ' '));
    });
    r.addEventListener('drop', e => {
      if (this.enabled && hostOf(r, e.target)) e.preventDefault();
    });
    r.addEventListener('click', e => {
      if (!this.enabled) return;
      const t = e.target instanceof Element ? e.target : null;
      if (t?.matches('input.task-box')) {
        const h = boxHost(t as HTMLInputElement);
        if (!h || this.closed) {
          e.preventDefault();
          return;
        }
        this.markDirty(h);
        this.flush();
        return;
      }
      if (t?.closest('pre, .mermaid-block') && !hostOf(r, t) && Date.now() - this.lastHint > HINT_EVERY) {
        this.lastHint = Date.now();
        this.cb.toast('Code blocks and diagrams are not editable here: use Source mode to change them.');
      }
    });
  }

  private onKey(e: KeyboardEvent): void {
    if (!this.enabled) return;
    const h = hostOf(this.root, e.target);
    if (!h) return;
    const mod = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey;
    const k = e.key.toLowerCase();
    if (e.key === 'Escape') {
      e.preventDefault();
      h.blur();
    } else if (e.key === 'Enter' && !e.shiftKey && !mod) {
      e.preventDefault();
      this.split(h);
    } else if (e.key === 'Enter' && e.shiftKey && blockKind(h) !== 'para') {
      e.preventDefault(); // headings and cells are single-line
    } else if (e.key === 'Backspace' && !mod && !e.shiftKey && !e.altKey) {
      const sel = this.root.ownerDocument.getSelection();
      if (sel?.isCollapsed && caretOffset(h) === 0) this.mergeBack(h, e);
    } else if (mod && (k === 'b' || k === 'i' || k === 'e' || k === 'k')) {
      e.preventDefault();
      if (this.closed) return;
      if (k === 'b') this.toggle(h, 'strong, b', 'strong');
      else if (k === 'i') this.toggle(h, 'em, i', 'em');
      else if (k === 'e') this.toggle(h, 'code', 'code');
      else this.link(h);
    }
  }

  private range(h: HTMLElement): Range | null {
    const sel = this.root.ownerDocument.getSelection();
    if (!sel || !sel.rangeCount) return null;
    const r = sel.getRangeAt(0);
    return h.contains(r.startContainer) && h.contains(r.endContainer) ? r : null;
  }

  private insertText(h: HTMLElement, text: string): void {
    const r = this.range(h);
    if (!r || !text) return;
    r.deleteContents();
    const t = this.root.ownerDocument.createTextNode(text);
    r.insertNode(t);
    r.setStartAfter(t);
    r.collapse(true);
    const sel = this.root.ownerDocument.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(r);
    this.markDirty(h);
  }

  /** Cmd/Ctrl-B/I/E: unwrap the enclosing element, or wrap the selection. */
  private toggle(h: HTMLElement, match: string, tag: string): void {
    const r = this.range(h);
    if (!r) return;
    const start = r.startContainer instanceof Element ? r.startContainer : r.startContainer.parentElement;
    const inside = start?.closest<HTMLElement>(match);
    if (inside && h.contains(inside) && inside !== h) {
      inside.replaceWith(...Array.from(inside.childNodes));
      h.normalize();
      this.markDirty(h);
      return;
    }
    if (r.collapsed) return;
    const el = this.root.ownerDocument.createElement(tag);
    el.append(r.extractContents());
    r.insertNode(el);
    const sel = this.root.ownerDocument.getSelection()!;
    const nr = this.root.ownerDocument.createRange();
    nr.selectNodeContents(el);
    sel.removeAllRanges();
    sel.addRange(nr);
    this.markDirty(h);
  }

  private link(h: HTMLElement): void {
    const r = this.range(h);
    if (!r) return;
    const start = r.startContainer instanceof Element ? r.startContainer : r.startContainer.parentElement;
    const a = start?.closest('a');
    if (a && h.contains(a)) {
      a.replaceWith(...Array.from(a.childNodes));
      this.markDirty(h);
      return;
    }
    const url = window.prompt('Link URL', 'https://')?.trim();
    if (!url) return;
    const el = this.root.ownerDocument.createElement('a');
    el.setAttribute('href', url);
    if (r.collapsed) el.textContent = url;
    else el.append(r.extractContents());
    r.insertNode(el);
    r.setStartAfter(el);
    r.collapse(true);
    this.markDirty(h);
  }

  /** Enter: split the block at the caret; the tail becomes a new (local) block after it. */
  private split(h: HTMLElement): void {
    if (this.inflight || this.conflictDoc || this.closed || blockKind(h) === 'cell') return;
    const own = this.unitOf(h);
    if (this.units.some(u => u !== own && u.dirty)) return this.flush();
    const r = this.range(h);
    if (!r) return;
    if (!r.collapsed) r.deleteContents();
    if (!blockText(h) || caretOffset(h) === 0) return; // nothing before the caret: no empty paragraph in Markdown
    const tail = this.root.ownerDocument.createRange();
    tail.setStart(r.startContainer, r.startOffset);
    tail.setEnd(h, h.childNodes.length);
    const frag = tail.extractContents();
    const { outer, host } = newHostLike(h);
    host.append(frag);
    host.querySelectorAll('input').forEach(i => i.remove());
    // A comment can't span two blocks: it stays with the first part.
    const kept = new Set(Array.from(h.querySelectorAll('mark[data-cid]')).filter(m => m.textContent).map(m => m.getAttribute('data-cid')));
    host.querySelectorAll('mark[data-cid]').forEach(m => {
      if (kept.has(m.getAttribute('data-cid'))) m.replaceWith(...Array.from(m.childNodes));
    });
    outerOf(h).after(outer);
    const u = own ?? { base: h, extras: [], merge: null, dirty: false };
    if (!own) this.units.push(u);
    const at = u.base === h ? 0 : u.extras.indexOf(h) + 1;
    u.extras.splice(at, 0, host);
    const moved = blockText(host) !== '';
    this.focus(host, 0);
    if (moved) this.markDirty(host);
  }

  /** Backspace at the start of a block: merge it into the previous one. */
  private mergeBack(h: HTMLElement, e: KeyboardEvent): void {
    const all = editableHosts(this.root, true);
    const prev = all[all.indexOf(h) - 1];
    if (!prev) return;
    e.preventDefault();
    if (this.inflight || this.conflictDoc || this.closed) return;
    if (blockKind(h) === 'cell' || blockKind(prev) === 'cell' || !adjacent(this.root, prev, h)) return;
    const hu = this.unitOf(h);
    const pu = this.unitOf(prev);
    if (h.hasAttribute(LOCAL_ATTR)) {
      if (!hu || hu !== pu) return;
      const had = blockText(h) !== '';
      const junction = textLength(prev);
      moveInto(h, prev);
      removeHost(h);
      hu.extras = hu.extras.filter(x => x !== h);
      this.focus(prev, junction);
      if (had) this.markDirty(prev);
      return;
    }
    if (prev.hasAttribute(LOCAL_ATTR)) return; // save the new block first
    if (this.units.some(u => u !== hu && u !== pu && u.dirty)) return this.flush();
    if (pu?.extras.length || pu?.merge || hu?.merge) return this.flush();
    const sp = spOf(h);
    if (!sp) return;
    const merge = { sp, synced: this.synced.get(h) ?? serializeBlock(h) };
    const junction = textLength(prev);
    moveInto(h, prev);
    removeHost(h);
    this.units = this.units.filter(u => u !== hu && u !== pu);
    this.units.push({ base: prev, extras: hu?.extras ?? [], merge, dirty: true });
    this.focus(prev, junction);
    this.schedule();
  }

  private focus(h: HTMLElement, offset: number): void {
    this.quiet = true;
    try {
      h.focus({ preventScroll: true });
      this.active = h;
      setCaret(h, offset);
    } finally {
      this.quiet = false;
    }
  }
}

/** The host a task checkbox belongs to (tight: its li-body; loose: the item's first paragraph). */
const boxHost = (box: HTMLInputElement): HTMLElement | null => {
  const body = box.closest<HTMLElement>('.li-body');
  if (body) return body;
  const p = box.parentElement?.querySelector<HTMLElement>(':scope > p');
  return p && taskBox(p) === box ? p : null;
};

/** Put the live element `live` where the freshly rendered `cp` is (keeping the new sourcepos). */
const swapIn = (cp: HTMLElement, live: HTMLElement): void => {
  const sp = cp.getAttribute('data-sourcepos');
  cp.replaceWith(live);
  if (sp) live.setAttribute('data-sourcepos', sp);
};

const removeHost = (h: HTMLElement): void => {
  const li = h.classList.contains('li-body') ? h.parentElement : null;
  h.remove();
  if (li && !li.querySelector('*')) li.remove();
};

/** Move a block's content to the end of `into` (checkbox and edge newlines dropped). */
const moveInto = (from: HTMLElement, into: HTMLElement): void => {
  const last = into.lastChild;
  if (last?.nodeType === Node.TEXT_NODE) (last as Text).data = (last as Text).data.replace(/\n+$/, '');
  const nodes = Array.from(from.childNodes).filter(n => !(n instanceof HTMLInputElement));
  const first = nodes[0];
  if (first?.nodeType === Node.TEXT_NODE) (first as Text).data = (first as Text).data.replace(/^\s+/, '');
  into.append(...nodes);
  into.normalize();
};

/** No code block, rule, table or locked block between two hosts. */
const adjacent = (root: HTMLElement, a: HTMLElement, b: HTMLElement): boolean => {
  const walker = root.ownerDocument.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
  walker.currentNode = a;
  for (let n = walker.nextNode() as Element | null; n && n !== b; n = walker.nextNode() as Element | null) {
    if (a.contains(n) || n.contains(b)) continue;
    if (n.matches('pre, hr, table, img, .mermaid-block, [data-edit-locked]')) return false;
  }
  return true;
};
