// Mermaid diagrams in the rendered view.
//
// The daemon renders a ```mermaid fence as `<pre data-sourcepos><code class="language-mermaid">`.
// That <pre> stays in the DOM (hidden) as the source of truth for selections, comments and diffs;
// a `.mermaid-block[data-mermaid-for=SOURCEPOS]` container with the SVG is inserted right after it.
// mermaid.js itself (and the theme) is loaded with a dynamic import() the first time a diagram
// has to be drawn, so documents without diagrams never fetch it.
import type { CommentView, Selection } from './protocol';
import { currentRange } from './selection';
import { el } from './ui/dom';
import type { Scheme } from './mermaidTheme';

export interface MermaidApi {
  initialize(config: Record<string, unknown>): void;
  render(id: string, text: string): Promise<{ svg: string }>;
}
export interface Engine {
  api: MermaidApi;
  config(scheme: Scheme): Record<string, unknown>;
}
export type Loader = () => Promise<Engine>;

const defaultLoader: Loader = async () => {
  const [lib, theme] = await Promise.all([import('mermaid'), import('./mermaidTheme')]);
  return { api: lib.default as unknown as MermaidApi, config: theme.mermaidConfig };
};

export interface MermaidHost {
  /** Open the comment composer for this selection payload (whole code block). */
  comment(selection: Selection): void;
  /** Focus the thread card of a comment. */
  focusThread(id: string): void;
}

export type RenderResult = { ok: true; svg: string } | { ok: false; message: string; line: number | null; transient?: boolean };

/** Attribute on the toolbar button / badge that tells the delegated click handler what to do. */
const ACT = 'data-mermaid-act';
/** Marks UI added by the round-changes view; `clearChanges` removes it again. */
export const CHANGE_UI_ATTR = 'data-mermaid-change-ui';
/** Old diagram source (modified block) kept on the container while changes are shown. */
export const OLD_CODE_ATTR = 'data-mermaid-old';
export const SHOW_PREV_CLASS = 'show-prev';
export const MERMAID_SELECTOR = 'pre[data-sourcepos] > code.language-mermaid';

const CACHE_MAX = 80;
const TMP_ID_PREFIX = 'amm';

type BlockState = 'loading' | 'ok' | 'error';

export const prefersDark = (): boolean => !!window.matchMedia?.('(prefers-color-scheme: dark)').matches;

/** Source of a mermaid code block inside some HTML (the first one), or null. */
export const mermaidSourceIn = (html: string): string | null => {
  const tmp = document.createElement('div');
  tmp.innerHTML = html;
  const code = tmp.querySelector(MERMAID_SELECTOR) ?? tmp.querySelector('pre > code.language-mermaid');
  return code ? code.textContent ?? '' : null;
};

const toError = (e: unknown): RenderResult => {
  const o = e as { message?: unknown; hash?: { line?: unknown } } | null;
  const message = String(o?.message ?? e ?? 'Unknown error').trim() || 'Unknown error';
  let line: number | null = null;
  const m = /\bline (\d+)/i.exec(message);
  if (m) line = Number(m[1]);
  else if (typeof o?.hash?.line === 'number') line = o.hash.line + 1; // jison lines are 0-based
  return { ok: false, message, line };
};

/**
 * mermaid appends temp elements (and, on errors, its "bomb" SVG) to <body>, and leaves them there
 * when it throws. Renders are serialized, so whatever appeared in <body> during one is ours to
 * remove (finished SVGs are returned as strings and inserted elsewhere, never as body children).
 */
const bodyChildren = (): Set<Element> => new Set(Array.from(document.body.children));
const cleanBody = (before: Set<Element>): void => {
  for (const n of Array.from(document.body.children)) {
    if (before.has(n) || n.classList.contains('mermaid-zoom')) continue;
    n.remove();
  }
};

export class MermaidView {
  private cache = new Map<string, RenderResult>();
  private pending = new Map<string, Promise<RenderResult>>();
  private engine: Promise<Engine> | null = null;
  private queue: Promise<unknown> = Promise.resolve();
  private seq = 0;
  private root: HTMLElement | null = null;
  private bound = new WeakSet<HTMLElement>();
  /** view element -> diagram source it should currently show (stale-render guard). */
  private wanted = new WeakMap<HTMLElement, string>();
  private pres = new WeakMap<HTMLElement, HTMLElement>();
  private states = new WeakMap<HTMLElement, BlockState>();
  /** Per block (sourcepos) UI state, kept across Doc updates. */
  private sourceShown = new Set<string>();
  private prevShown = new Set<string>();
  private threads = new Map<string, CommentView[]>();
  private cursor = new Map<string, number>();
  scheme: Scheme;

  constructor(
    private host: MermaidHost,
    private load: Loader = defaultLoader,
  ) {
    this.scheme = prefersDark() ? 'dark' : 'light';
    const mq = window.matchMedia?.('(prefers-color-scheme: dark)');
    mq?.addEventListener?.('change', () => this.setScheme(mq.matches ? 'dark' : 'light'));
  }

  // ---------- public API ----------

  /** Call right after `root.innerHTML = html`: wrap every mermaid block. */
  attach(root: HTMLElement): void {
    this.root = root;
    this.bind(root);
    for (const code of root.querySelectorAll<HTMLElement>(MERMAID_SELECTOR)) {
      const pre = code.parentElement as HTMLElement;
      if (pre.closest('[data-diff]')) continue; // old (removed) blocks: see decorateChanges
      const sp = pre.getAttribute('data-sourcepos') ?? '';
      if (this.containerOf(root, sp)) continue;
      this.build(pre, sp, code.textContent ?? '');
    }
  }

  /** Mark diagrams whose code block has comments (border + count badge). Idempotent. */
  decorateComments(root: HTMLElement, comments: CommentView[]): void {
    this.threads = new Map();
    for (const c of comments) {
      if (!c.code_lines || !c.block_sourcepos) continue;
      const list = this.threads.get(c.block_sourcepos) ?? [];
      list.push(c);
      this.threads.set(c.block_sourcepos, list);
    }
    for (const container of root.querySelectorAll<HTMLElement>('.mermaid-block[data-mermaid-for]')) {
      const list = this.threads.get(container.dataset.mermaidFor ?? '') ?? [];
      container.toggleAttribute('data-commented', list.length > 0);
      const badge = container.querySelector<HTMLElement>('.mermaid-badge');
      if (!badge) continue;
      badge.hidden = list.length === 0;
      badge.textContent = String(list.length);
      const open = list.filter(c => c.status !== 'resolved').length;
      badge.classList.toggle('is-resolved', list.length > 0 && open === 0);
      badge.title = list.length === 1 ? '1 comment on this diagram' : `${list.length} comments on this diagram`;
    }
  }

  /**
   * After `applyChanges`: add the "Previous version" toggle to modified diagrams and render the
   * old diagram inside removed/replaced block boxes. Everything added here is removed again by
   * `clearChanges` (changes.ts).
   */
  decorateChanges(root: HTMLElement): void {
    for (const container of root.querySelectorAll<HTMLElement>(`.mermaid-block[data-mermaid-for][${OLD_CODE_ATTR}]`)) {
      const sp = container.dataset.mermaidFor ?? '';
      const toolbar = container.querySelector('.mermaid-toolbar');
      if (toolbar && !toolbar.querySelector('.mermaid-btn-prev')) {
        const btn = el('button', { className: 'mermaid-btn mermaid-btn-prev', type: 'button', [ACT]: 'prev', [CHANGE_UI_ATTR]: '', title: 'Show the diagram as it was in your last review' });
        toolbar.prepend(btn);
      }
      this.syncPrev(container, this.prevShown.has(sp));
    }
    for (const code of root.querySelectorAll<HTMLElement>('.diff-del-block pre > code.language-mermaid')) {
      const pre = code.parentElement as HTMLElement;
      if (pre.hidden) continue;
      pre.hidden = true;
      const view = el('div', { className: 'mermaid-view mmd mermaid-old-view' });
      const box = el('div', { className: 'mermaid-block is-old', 'data-view': 'diagram', 'data-state': 'loading' }, [
        el('span', { className: 'mermaid-old-label', text: 'removed' }),
        view,
      ]);
      pre.after(box);
      this.fill(view, code.textContent ?? '', box);
    }
  }

  setScheme(s: Scheme): void {
    if (s === this.scheme) return;
    this.scheme = s;
    if (!this.root) return;
    for (const view of this.root.querySelectorAll<HTMLElement>('.mermaid-view')) {
      const code = this.wanted.get(view);
      if (code !== undefined) this.fill(view, code, view.closest<HTMLElement>('.mermaid-old, .mermaid-block'));
    }
  }

  /** Element to scroll to for a code block (the diagram if shown, else the code block). */
  revealTarget(root: HTMLElement, sp: string): HTMLElement | null {
    const container = this.containerOf(root, sp);
    const pre = root.querySelector<HTMLElement>(`pre[data-sourcepos="${CSS.escape(sp)}"]`);
    if (container && (!pre || pre.hidden)) return container;
    return pre;
  }

  // ---------- DOM construction ----------

  private containerOf(root: ParentNode, sp: string): HTMLElement | null {
    return root.querySelector<HTMLElement>(`.mermaid-block[data-mermaid-for="${CSS.escape(sp)}"]`);
  }

  private build(pre: HTMLElement, sp: string, code: string): void {
    pre.hidden = true;
    const badge = el('button', { className: 'mermaid-badge', type: 'button', [ACT]: 'badge', hidden: true, 'aria-label': 'Show comments on this diagram' });
    const toolbar = el('div', { className: 'mermaid-toolbar' }, [
      badge,
      el('button', { className: 'mermaid-btn mermaid-btn-comment', type: 'button', [ACT]: 'comment', text: 'Comment', title: 'Comment on this whole diagram' }),
      el('button', { className: 'mermaid-btn mermaid-btn-source', type: 'button', [ACT]: 'source', text: 'Source', title: 'Show the Mermaid source instead of the diagram' }),
    ]);
    const view = el('div', { className: 'mermaid-view mmd' });
    const container = el('div', { className: 'mermaid-block', 'data-mermaid-for': sp, 'data-view': 'diagram', 'data-state': 'loading' }, [toolbar, view]);
    this.pres.set(container, pre);
    pre.after(container);
    this.fill(view, code, container);
    this.layout(container);
  }

  /** Where the container sits and whether the code block is visible, from state + toggles. */
  private layout(container: HTMLElement): void {
    const pre = this.pres.get(container);
    if (!pre) return;
    const sp = container.dataset.mermaidFor ?? '';
    const state = this.states.get(container) ?? 'loading';
    const source = state === 'error' || this.sourceShown.has(sp);
    pre.hidden = !source;
    container.dataset.view = source ? 'source' : 'diagram';
    container.dataset.state = state;
    // Source view: toolbar / error box above the code block; diagram view: diagram after it.
    if (source) {
      if (container.nextElementSibling !== pre) pre.before(container);
    } else if (pre.nextElementSibling !== container) pre.after(container);
    const btn = container.querySelector<HTMLElement>('.mermaid-btn-source');
    if (btn) {
      const showing = this.sourceShown.has(sp);
      btn.textContent = showing ? 'Diagram' : 'Source';
      btn.title = showing ? 'Show the rendered diagram' : 'Show the Mermaid source instead of the diagram';
      btn.setAttribute('aria-pressed', String(showing));
      btn.hidden = state === 'error';
    }
  }

  private syncPrev(container: HTMLElement, on: boolean): void {
    const btn = container.querySelector<HTMLElement>('.mermaid-btn-prev');
    if (btn) {
      btn.textContent = on ? 'Current version' : 'Previous version';
      btn.setAttribute('aria-pressed', String(on));
    }
    let old = container.querySelector<HTMLElement>('.mermaid-old');
    if (!on) {
      container.classList.remove(SHOW_PREV_CLASS);
      old?.remove();
      return;
    }
    container.classList.add(SHOW_PREV_CLASS);
    const code = container.getAttribute(OLD_CODE_ATTR) ?? '';
    if (old && this.wanted.get(old.querySelector<HTMLElement>('.mermaid-view')!) === code) return;
    old?.remove();
    const view = el('div', { className: 'mermaid-view mmd mermaid-old-view' });
    old = el('div', { className: 'mermaid-old', [CHANGE_UI_ATTR]: '', 'data-state': 'loading' }, [el('span', { className: 'mermaid-old-label', text: 'previous' }), view]);
    container.querySelector('.mermaid-view')?.before(old);
    this.fill(view, code, old);
  }

  // ---------- rendering ----------

  private key(code: string): string {
    return `${this.scheme}\u0000${code}`;
  }

  /** Show `code` in `view`: from the cache synchronously, else after an async render. */
  private fill(view: HTMLElement, code: string, owner: HTMLElement | null): void {
    this.wanted.set(view, code);
    const key = this.key(code);
    const hit = this.cache.get(key);
    if (hit) {
      this.show(view, hit, owner);
      return;
    }
    view.replaceChildren(el('div', { className: 'mermaid-loading', text: 'Rendering diagram…' }));
    this.setState(owner, 'loading');
    void this.request(code).then(res => {
      // Stale: the DOM was rebuilt (new Doc), the block changed, or the colour scheme flipped.
      if (!this.root?.contains(view) || this.wanted.get(view) !== code || this.key(code) !== key) return;
      this.show(view, res, owner);
    });
  }

  private setState(owner: HTMLElement | null, state: BlockState): void {
    if (!owner) return;
    this.states.set(owner, state);
    if (owner.dataset.mermaidFor !== undefined) this.layout(owner);
    else owner.dataset.state = state;
  }

  private show(view: HTMLElement, res: RenderResult, owner: HTMLElement | null): void {
    if (res.ok) view.innerHTML = res.svg;
    else view.replaceChildren(errorBox(res));
    this.setState(owner, res.ok ? 'ok' : 'error');
  }

  private request(code: string): Promise<RenderResult> {
    const key = this.key(code);
    const inflight = this.pending.get(key);
    if (inflight) return inflight;
    const scheme = this.scheme;
    const p = this.enqueue(() => this.renderNow(code, scheme)).then(res => {
      this.pending.delete(key);
      if (!(!res.ok && res.transient)) this.cache.set(key, res);
      if (this.cache.size > CACHE_MAX) this.cache.delete(this.cache.keys().next().value as string);
      return res;
    });
    this.pending.set(key, p);
    return p;
  }

  /** mermaid.render is not re-entrant friendly (shared temp elements): one at a time. */
  private enqueue<T>(fn: () => Promise<T>): Promise<T> {
    const run = this.queue.then(fn, fn);
    this.queue = run.catch(() => undefined);
    return run;
  }

  private async renderNow(code: string, scheme: Scheme): Promise<RenderResult> {
    let engine: Engine;
    try {
      this.engine ??= this.load();
      engine = await this.engine;
    } catch (e) {
      this.engine = null; // retry on the next request
      return { ok: false, message: `Could not load the diagram renderer: ${(e as Error)?.message ?? e}`, line: null, transient: true };
    }
    const id = `${TMP_ID_PREFIX}-${++this.seq}`;
    const before = bodyChildren();
    try {
      engine.api.initialize(engine.config(scheme));
      const { svg } = await engine.api.render(id, code);
      return { ok: true, svg };
    } catch (e) {
      return toError(e);
    } finally {
      cleanBody(before);
    }
  }

  // ---------- interaction ----------

  private bind(root: HTMLElement): void {
    if (this.bound.has(root)) return;
    this.bound.add(root);
    root.addEventListener('click', e => {
      const target = e.target instanceof Element ? e.target : null;
      if (!target) return;
      const container = target.closest<HTMLElement>('.mermaid-block');
      if (!container) return;
      const act = target.closest<HTMLElement>(`[${ACT}]`);
      if (act) {
        e.preventDefault();
        e.stopPropagation();
        this.act(act.getAttribute(ACT) ?? '', container);
        return;
      }
      const view = target.closest('.mermaid-view');
      const svg = view?.querySelector('svg');
      // Zoom is rarely used: loaded on first click (keeps the initial bundle small).
      if (svg && currentRange() === null) void import('./zoom').then(z => z.openZoom(svg as unknown as SVGElement));
    });
  }

  private act(kind: string, container: HTMLElement): void {
    const sp = container.dataset.mermaidFor ?? '';
    if (kind === 'comment') {
      const pre = this.pres.get(container);
      if (pre) this.host.comment({ sourcepos: sp, text: pre.textContent ?? '', offset: 0 });
    } else if (kind === 'source') {
      if (this.sourceShown.has(sp)) this.sourceShown.delete(sp);
      else this.sourceShown.add(sp);
      this.layout(container);
    } else if (kind === 'prev') {
      if (this.prevShown.has(sp)) this.prevShown.delete(sp);
      else this.prevShown.add(sp);
      this.syncPrev(container, this.prevShown.has(sp));
    } else if (kind === 'badge') {
      const list = this.threads.get(sp) ?? [];
      if (!list.length) return;
      // Several threads: successive clicks step through them (open ones first).
      const ordered = [...list.filter(c => c.status !== 'resolved'), ...list.filter(c => c.status === 'resolved')];
      const i = (this.cursor.get(sp) ?? -1) + 1;
      this.cursor.set(sp, i % ordered.length);
      this.host.focusThread(ordered[i % ordered.length].id);
    }
  }
}

const errorBox = (res: Extract<RenderResult, { ok: false }>): HTMLElement =>
  el('div', { className: 'mermaid-error', role: 'alert' }, [
    el('div', { className: 'mermaid-error-title', text: res.line ? `Mermaid diagram error · line ${res.line}` : 'Mermaid diagram error' }),
    el('div', { className: 'mermaid-error-msg', text: res.message }),
  ]);
