/** Pure helpers turning DocState marker ranges into CodeMirror-ready ranges (UTF-16). */

export type Span = { from: number; to: number };

/** Clamp to [0, len], drop empty ranges, sort and merge overlapping/adjacent ranges. */
export const hiddenSpans = (ranges: [number, number][], len: number): Span[] => {
  const spans = ranges
    .map(([a, b]) => ({ from: Math.max(0, Math.min(a, b, len)), to: Math.min(len, Math.max(a, b)) }))
    .filter(s => s.to > s.from)
    .sort((x, y) => x.from - y.from || x.to - y.to);
  const out: Span[] = [];
  for (const s of spans) {
    const last = out[out.length - 1];
    if (last && s.from <= last.to) last.to = Math.max(last.to, s.to);
    else out.push({ ...s });
  }
  return out;
};

/** Anchored text spans (UTF-16), as computed by the daemon (`CommentView.anchor`). */
export const anchorSpans = (
  comments: { id: string; status: string; anchor: [number, number] | null }[],
  len: number,
): (Span & { id: string; resolved: boolean })[] =>
  comments
    .filter(c => c.anchor !== null)
    .map(c => ({ id: c.id, resolved: c.status === 'resolved', from: Math.max(0, c.anchor![0]), to: Math.min(len, c.anchor![1]) }))
    .filter(s => s.to > s.from)
    .sort((a, b) => a.from - b.from);
