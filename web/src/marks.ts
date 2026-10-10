import type { CommentView } from './protocol';

/** All marks of a comment, in document order. */
export const marksFor = (root: ParentNode, id: string): HTMLElement[] =>
  Array.from(root.querySelectorAll<HTMLElement>(`mark[data-cid="${CSS.escape(id)}"]`));

/** Comment id of the mark under an event target, if any. */
export const cidAt = (target: EventTarget | null): string | null => {
  const el = target instanceof Element ? target : (target as Node | null)?.parentElement ?? null;
  return el?.closest('mark[data-cid]')?.getAttribute('data-cid') ?? null;
};

/** Ids in order of first appearance in the rendered doc. */
export const markOrder = (root: ParentNode): string[] => {
  const seen = new Set<string>();
  for (const m of root.querySelectorAll('mark[data-cid]')) {
    const id = m.getAttribute('data-cid');
    if (id) seen.add(id);
  }
  return [...seen];
};

/** Decorate marks with state classes (resolved = dimmed, active = emphasised). */
export const styleMarks = (root: ParentNode, comments: CommentView[], activeId: string | null): void => {
  const byId = new Map(comments.map(c => [c.id, c]));
  for (const m of root.querySelectorAll<HTMLElement>('mark[data-cid]')) {
    const c = byId.get(m.dataset.cid ?? '');
    m.classList.add('cmark');
    m.classList.toggle('is-resolved', c?.status === 'resolved');
    m.classList.toggle('is-active', !!activeId && m.dataset.cid === activeId);
  }
};

export type SidebarGroup = 'anchored' | 'code' | 'orphan';

export const groupOf = (c: CommentView, anchored: Set<string>): SidebarGroup => {
  if (c.code_lines) return 'code';
  if (c.orphaned || !anchored.has(c.id)) return 'orphan';
  return 'anchored';
};

/**
 * Sidebar order: threads anchored in the rendered doc in mark order; then code-line comments,
 * then orphans (each in daemon order). `order` is the mark order of the current render;
 * when it is empty (source mode before a render), daemon order is kept.
 */
export const sortComments = (comments: CommentView[], order: string[]): CommentView[] => {
  const pos = new Map(order.map((id, i) => [id, i]));
  const anchored = new Set(order);
  const rank = { anchored: 0, code: 1, orphan: 2 } as const;
  return comments
    .map((c, i) => ({ c, i, g: rank[groupOf(c, anchored)] }))
    .sort((a, b) => a.g - b.g || (a.g === 0 ? pos.get(a.c.id)! - pos.get(b.c.id)! : a.i - b.i))
    .map(x => x.c);
};
