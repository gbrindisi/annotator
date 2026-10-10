import type { Change } from './protocol';

export const CHANGE_ATTR = 'data-change';

/** Find an element with exactly this data-sourcepos. */
export const bySourcepos = (root: ParentNode, sp: string): HTMLElement | null =>
  root.querySelector<HTMLElement>(`[data-sourcepos="${CSS.escape(sp)}"]`);

/**
 * Outline changed blocks in the rendered doc. Returns the decorated elements in document
 * order (removed markers included) for the "N changes" stepper.
 */
export const applyChanges = (root: HTMLElement, changes: Change[]): HTMLElement[] => {
  clearChanges(root);
  const out: HTMLElement[] = [];
  for (const ch of changes) {
    if (ch.kind === 'added' || ch.kind === 'modified') {
      const el = ch.sourcepos ? bySourcepos(root, ch.sourcepos) : null;
      if (!el) continue;
      el.setAttribute(CHANGE_ATTR, ch.kind);
      if (ch.kind === 'modified' && ch.old_text != null) el.title = `Before:\n${ch.old_text}`;
      out.push(el);
    } else if (ch.kind === 'removed') {
      const marker = root.ownerDocument.createElement('div');
      marker.className = 'removed-marker';
      marker.setAttribute(CHANGE_ATTR, 'removed');
      marker.title = `Removed:\n${ch.old_text ?? ''}`;
      marker.setAttribute('aria-label', 'Removed block');
      const after = ch.after ? bySourcepos(root, ch.after) : null;
      if (after) {
        // Insert after the top-level block (the sourcepos may belong to a nested element).
        let top: HTMLElement = after;
        while (top.parentElement && top.parentElement !== root) top = top.parentElement;
        if (top.parentElement === root) top.after(marker);
        else continue;
      } else if (ch.after == null) {
        root.prepend(marker);
      } else continue;
      out.push(marker);
    }
  }
  return sortByDocument(out);
};

export const clearChanges = (root: HTMLElement): void => {
  root.querySelectorAll('.removed-marker').forEach(n => n.remove());
  root.querySelectorAll<HTMLElement>(`[${CHANGE_ATTR}]`).forEach(n => {
    n.removeAttribute(CHANGE_ATTR);
    n.removeAttribute('title');
  });
};

const sortByDocument = (els: HTMLElement[]): HTMLElement[] =>
  [...new Set(els)].sort((a, b) => (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1));
