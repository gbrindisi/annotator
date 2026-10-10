import type { Selection } from './protocol';

/** Block-level elements carrying data-sourcepos (see spike/README.md). */
export const BLOCK_TAGS = ['p', 'li', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'td', 'th', 'pre', 'blockquote', 'ul', 'ol', 'table', 'tr'];
const BLOCK_SELECTOR = BLOCK_TAGS.map(t => `${t}[data-sourcepos]`).join(',');

export type PayloadResult = { ok: true; selection: Selection } | { ok: false; error: string };

/** Build the AddComment selection payload for `range` inside `root` (spike algorithm). */
export const selectionPayload = (range: Range | null, root: HTMLElement): PayloadResult | null => {
  if (!range || range.collapsed) return null;
  let node: Node | null = range.commonAncestorContainer;
  if (node.nodeType !== Node.ELEMENT_NODE) node = node.parentElement;
  const block = (node as Element | null)?.closest(BLOCK_SELECTOR) as HTMLElement | null;
  if (!block || !root.contains(block)) {
    if (!root.contains(range.commonAncestorContainer)) return null;
    return { ok: false, error: 'Select text within a single block to comment' };
  }
  const pre = block.ownerDocument.createRange();
  pre.setStart(block, 0);
  pre.setEnd(range.startContainer, range.startOffset);
  const text = range.toString();
  if (!text.trim()) return null;
  return {
    ok: true,
    selection: { sourcepos: block.getAttribute('data-sourcepos') ?? '', text, offset: pre.toString().length },
  };
};

export const currentRange = (doc: Document = document): Range | null => {
  const sel = doc.getSelection();
  if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return null;
  return sel.getRangeAt(0);
};
