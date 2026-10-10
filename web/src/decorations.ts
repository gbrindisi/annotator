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

/**
 * Anchored text spans: the visible text between an opening anchor marker range and its
 * closing one. Only the first characters of each hidden range are inspected to tell opening
 * from closing anchors; the comment format itself stays the daemon's business.
 */
export const anchorSpans = (raw: string, ranges: [number, number][]): (Span & { id: string })[] => {
  const spans = hiddenSpans(ranges, raw.length);
  const out: (Span & { id: string })[] = [];
  const open = new Map<string, number>();
  for (const s of spans) {
    const text = raw.slice(s.from, s.to);
    // A merged range may contain several markers; scan all of them.
    const re = /<!--(\/?)c:([^\s>-][^\s>]*?)-->/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text))) {
      const id = m[2];
      if (!m[1]) open.set(id, s.from + m.index + m[0].length);
      else if (open.has(id)) {
        const from = open.get(id)!;
        const to = s.from + m.index;
        open.delete(id);
        if (to > from) out.push({ id, from, to });
      }
    }
  }
  return out.sort((a, b) => a.from - b.from);
};
