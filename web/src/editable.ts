// Edit mode DOM: which rendered blocks are editable, and helpers on them.
import { clearWordDiff } from './worddiff';

/** Attribute on every contenteditable block ("host"). */
export const HOST_ATTR = 'data-edit-host';
/** A host created in the browser (Enter) that is not in the file yet. */
export const LOCAL_ATTR = 'data-local';
export const HOST_SELECTOR = `[${HOST_ATTR}]`;

const TEXT_BLOCKS = 'p[data-sourcepos], h1[data-sourcepos], h2[data-sourcepos], h3[data-sourcepos], h4[data-sourcepos], h5[data-sourcepos], h6[data-sourcepos], td[data-sourcepos], th[data-sourcepos]';
const BLOCK_CHILD = new Set(['UL', 'OL', 'P', 'PRE', 'BLOCKQUOTE', 'TABLE', 'DIV', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'HR']);

/** Blocks whose source has raw HTML (rendered as `<!-- raw HTML omitted -->`) can't round-trip. */
const hasRawHtml = (el: Element): boolean => {
  const walker = el.ownerDocument.createTreeWalker(el, NodeFilter.SHOW_COMMENT);
  return !!walker.nextNode();
};

const markHost = (h: HTMLElement): void => {
  h.setAttribute(HOST_ATTR, '');
  h.setAttribute('contenteditable', 'true');
  h.setAttribute('spellcheck', 'true');
  for (const box of h.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) enableBox(box);
};

const enableBox = (box: HTMLInputElement): void => {
  box.removeAttribute('disabled');
  box.setAttribute('contenteditable', 'false');
  box.classList.add('task-box');
  box.title = 'Toggle task';
};

/** A new, empty host of the same kind as `like`, placed after it by the caller. */
export const newHostLike = (like: HTMLElement): { outer: HTMLElement; host: HTMLElement } => {
  const doc = like.ownerDocument;
  let outer: HTMLElement;
  let host: HTMLElement;
  if (like.classList.contains('li-body')) {
    outer = doc.createElement('li');
    host = doc.createElement('div');
    host.className = 'li-body';
    outer.append(host);
  } else {
    host = outer = doc.createElement(/^H[1-6]$|^P$/.test(like.tagName) ? 'p' : 'p');
  }
  markHost(host);
  host.setAttribute(LOCAL_ATTR, '');
  return { outer, host };
};

/** The element that is a sibling in the block flow (the `li` of a list item text). */
export const outerOf = (host: HTMLElement): HTMLElement => (host.classList.contains('li-body') && host.parentElement ? host.parentElement : host);

/**
 * Make the text blocks under `root` editable: paragraphs, headings, table cells, and list item
 * text (tight items get their inline content wrapped in `div.li-body`). Returns the hosts.
 */
export const prepareEditable = (root: HTMLElement): HTMLElement[] => {
  const doc = root.ownerDocument;
  for (const li of root.querySelectorAll<HTMLElement>('li[data-sourcepos]')) {
    if (li.closest('[data-diff]') || li.querySelector(':scope > .li-body')) continue;
    const inline: Node[] = [];
    for (const c of Array.from(li.childNodes)) {
      if (c instanceof Element && BLOCK_CHILD.has(c.tagName)) break;
      inline.push(c);
    }
    if (!inline.some(n => (n instanceof Element && n.tagName !== 'INPUT') || (n.nodeType === Node.TEXT_NODE && n.textContent!.trim()))) {
      // Loose item: its paragraphs are hosts; a checkbox sits before the first one.
      const box = li.querySelector<HTMLInputElement>(':scope > input[type="checkbox"]');
      if (box && li.querySelector(':scope > p')) enableBox(box);
      continue;
    }
    const body = doc.createElement('div');
    body.className = 'li-body';
    li.insertBefore(body, inline[0]);
    body.append(...inline);
  }
  const hosts: HTMLElement[] = [];
  const candidates = root.querySelectorAll<HTMLElement>(`${TEXT_BLOCKS}, li[data-sourcepos] > .li-body`);
  for (const h of candidates) {
    if (h.closest('[data-diff], pre, .mermaid-block')) continue;
    if (hasRawHtml(h)) {
      h.setAttribute('data-edit-locked', '');
      h.title = 'Contains raw HTML: edit this block in Source mode';
      continue;
    }
    markHost(h);
    hosts.push(h);
  }
  return hosts;
};

/** Hosts in document order; `withLocal` includes blocks not yet in the file. */
export const editableHosts = (root: ParentNode, withLocal = false): HTMLElement[] =>
  Array.from(root.querySelectorAll<HTMLElement>(HOST_SELECTOR)).filter(h => withLocal || !h.hasAttribute(LOCAL_ATTR));

export const hostOf = (root: HTMLElement, target: EventTarget | null): HTMLElement | null => {
  const el = target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
  const h = el?.closest<HTMLElement>(HOST_SELECTOR) ?? null;
  return h && root.contains(h) ? h : null;
};

/** The `data-sourcepos` the daemon knows the host by (the `li` for list item text). */
export const spOf = (host: HTMLElement): string | null =>
  host.getAttribute('data-sourcepos') ?? (host.classList.contains('li-body') ? host.parentElement?.getAttribute('data-sourcepos') ?? null : null);

/** Remove round-change decorations from a block that enters editing. */
export const undecorate = (host: HTMLElement): void => {
  if (host.querySelector('[data-diff]')) clearWordDiff(host);
};

// ---------- caret by text offset ----------

const textNodes = (host: Node): Text[] => {
  const walker = host.ownerDocument!.createTreeWalker(host, NodeFilter.SHOW_TEXT);
  const out: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) out.push(n as Text);
  return out;
};

/** Caret (selection focus start) as a UTF-16 offset into the host's text, or null. */
export const caretOffset = (host: HTMLElement): number | null => {
  const sel = host.ownerDocument.getSelection();
  if (!sel || !sel.rangeCount) return null;
  const r = sel.getRangeAt(0);
  if (!host.contains(r.startContainer)) return null;
  const pre = host.ownerDocument.createRange();
  pre.selectNodeContents(host);
  pre.setEnd(r.startContainer, r.startOffset);
  return pre.toString().length;
};

export const setCaret = (host: HTMLElement, offset: number): void => {
  const doc = host.ownerDocument;
  const sel = doc.getSelection();
  if (!sel) return;
  const r = doc.createRange();
  let left = offset;
  let placed = false;
  for (const t of textNodes(host)) {
    if (left <= t.data.length) {
      r.setStart(t, left);
      placed = true;
      break;
    }
    left -= t.data.length;
  }
  if (!placed) {
    r.selectNodeContents(host);
    r.collapse(false);
  } else r.collapse(true);
  sel.removeAllRanges();
  sel.addRange(r);
};

/** The host's text length (for "caret at end" checks). */
export const textLength = (host: HTMLElement): number => textNodes(host).reduce((n, t) => n + t.data.length, 0);
