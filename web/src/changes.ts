import type { Change } from './protocol';
import { applyWordDiff, changedRatio, clearWordDiff, DIFF_ATTR, liveTextNodes, wordDiff } from './worddiff';

export const CHANGE_ATTR = 'data-change';
/** Above this fraction of changed characters, show the whole old block instead of interleaving. */
export const BLOCK_DIFF_THRESHOLD = 0.6;

/** Find an element with exactly this data-sourcepos. */
export const bySourcepos = (root: ParentNode, sp: string): HTMLElement | null =>
  root.querySelector<HTMLElement>(`[data-sourcepos="${CSS.escape(sp)}"]`);

/** Struck-through container with the old block's HTML (sourcepos stripped so lookups ignore it). */
const delBlock = (doc: Document, html: string | null, fallbackText: string | null): HTMLElement => {
  const div = doc.createElement('div');
  div.className = 'diff-del-block';
  div.setAttribute(DIFF_ATTR, 'del-block');
  div.setAttribute('contenteditable', 'false');
  div.setAttribute('aria-label', 'Removed since your last review');
  if (html != null) {
    div.innerHTML = html;
    div.querySelectorAll('[data-sourcepos]').forEach(n => n.removeAttribute('data-sourcepos'));
    div.querySelectorAll('mark[data-cid]').forEach(n => n.replaceWith(...Array.from(n.childNodes)));
  } else div.textContent = fallbackText ?? '';
  return div;
};

const topLevel = (root: HTMLElement, el: HTMLElement): HTMLElement | null => {
  let top: HTMLElement = el;
  while (top.parentElement && top.parentElement !== root) top = top.parentElement;
  return top.parentElement === root ? top : null;
};

/** Old text as the browser renders it (textContent of the parsed old HTML). */
const oldTextContent = (doc: Document, html: string): string => {
  const tmp = doc.createElement('div');
  tmp.innerHTML = html;
  return tmp.textContent ?? '';
};

/**
 * Show changes inline (track-changes style). Returns the decorated elements in document
 * order (removed blocks included) for the "N changes" stepper.
 */
export const applyChanges = (root: HTMLElement, changes: Change[]): HTMLElement[] => {
  clearChanges(root);
  const doc = root.ownerDocument;
  const out: HTMLElement[] = [];
  for (const ch of changes) {
    if (ch.kind === 'added' || ch.kind === 'modified') {
      const el = ch.sourcepos ? bySourcepos(root, ch.sourcepos) : null;
      if (!el) continue;
      el.setAttribute(CHANGE_ATTR, ch.kind);
      out.push(el);
      if (ch.kind !== 'modified' || ch.old_html == null) continue;
      const cur = liveTextNodes(el).map(t => t.data).join('');
      const ops = wordDiff(oldTextContent(doc, ch.old_html), cur);
      if (changedRatio(ops) > BLOCK_DIFF_THRESHOLD || !applyWordDiff(el, ops)) {
        const top = topLevel(root, el);
        if (top) {
          const d = delBlock(doc, ch.old_html, ch.old_text);
          top.before(d);
          el.setAttribute(CHANGE_ATTR, 'replaced');
        }
      }
    } else if (ch.kind === 'removed') {
      const block = delBlock(doc, ch.old_html ?? null, ch.old_text);
      const after = ch.after ? bySourcepos(root, ch.after) : null;
      if (after) {
        // Insert after the top-level block (the sourcepos may belong to a nested element).
        const top = topLevel(root, after);
        if (!top) continue;
        // Keep several removed blocks after one anchor in their original order.
        let ref: Element = top;
        while (ref.nextElementSibling?.getAttribute(DIFF_ATTR) === 'del-block' && ref.nextElementSibling.getAttribute(CHANGE_ATTR) === 'removed') ref = ref.nextElementSibling;
        ref.after(block);
      } else if (ch.after == null) {
        let ref: Element | null = root.firstElementChild;
        while (ref?.getAttribute(CHANGE_ATTR) === 'removed') ref = ref.nextElementSibling;
        if (ref) ref.before(block);
        else root.append(block);
      } else continue;
      block.setAttribute(CHANGE_ATTR, 'removed');
      out.push(block);
    }
  }
  return sortByDocument(out);
};

export const clearChanges = (root: HTMLElement): void => {
  clearWordDiff(root);
  root.querySelectorAll<HTMLElement>(`[${CHANGE_ATTR}]`).forEach(n => n.removeAttribute(CHANGE_ATTR));
};

const sortByDocument = (els: HTMLElement[]): HTMLElement[] =>
  [...new Set(els)].sort((a, b) => (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1));
