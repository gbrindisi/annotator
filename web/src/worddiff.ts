/** Word-level diff (LCS on whitespace-delimited tokens) and its application to a live DOM block. */

export type Op = { type: 'eq' | 'ins' | 'del'; text: string };

/** Split into alternating word / whitespace tokens (whitespace kept). */
export const tokenize = (s: string): string[] => s.match(/\s+|\S+/gu) ?? [];

/** Max DP cells before giving up (treated as "all different"). */
const MAX_CELLS = 4_000_000;

/** Token diff from `a` (old) to `b` (new). Adjacent ops of one type are merged. */
export const wordDiff = (a: string, b: string): Op[] => {
  const x = tokenize(a);
  const y = tokenize(b);
  // Trim common prefix/suffix to keep the table small.
  let pre = 0;
  while (pre < x.length && pre < y.length && x[pre] === y[pre]) pre++;
  let suf = 0;
  while (suf < x.length - pre && suf < y.length - pre && x[x.length - 1 - suf] === y[y.length - 1 - suf]) suf++;
  const xs = x.slice(pre, x.length - suf);
  const ys = y.slice(pre, y.length - suf);
  const raw: Op[] = x.slice(0, pre).map(text => ({ type: 'eq', text }));
  const n = xs.length;
  const m = ys.length;
  if ((n + 1) * (m + 1) > MAX_CELLS) {
    xs.forEach(text => raw.push({ type: 'del', text }));
    ys.forEach(text => raw.push({ type: 'ins', text }));
  } else {
    // lcs[i][j] = LCS length of xs[i..] and ys[j..]
    const w = m + 1;
    const lcs = new Uint32Array((n + 1) * w);
    for (let i = n - 1; i >= 0; i--)
      for (let j = m - 1; j >= 0; j--)
        lcs[i * w + j] = xs[i] === ys[j] ? lcs[(i + 1) * w + j + 1] + 1 : Math.max(lcs[(i + 1) * w + j], lcs[i * w + j + 1]);
    let i = 0;
    let j = 0;
    while (i < n || j < m) {
      if (i < n && j < m && xs[i] === ys[j]) {
        raw.push({ type: 'eq', text: xs[i++] });
        j++;
      } else if (j < m && (i === n || lcs[i * w + j + 1] >= lcs[(i + 1) * w + j])) raw.push({ type: 'ins', text: ys[j++] });
      else raw.push({ type: 'del', text: xs[i++] });
    }
  }
  x.slice(x.length - suf).forEach(text => raw.push({ type: 'eq', text }));
  return normalizeOps(raw);
};

/**
 * Absorb whitespace-only equal tokens sandwiched between changes into both sides (less
 * choppy "del a | del b" interleaving), then group each change run as del-then-ins and merge.
 */
const normalizeOps = (ops: Op[]): Op[] => {
  const absorbed: Op[] = [];
  ops.forEach((op, k) => {
    const prev = ops[k - 1];
    const next = ops[k + 1];
    if (op.type === 'eq' && !op.text.trim() && prev && next && prev.type !== 'eq' && next.type !== 'eq') {
      absorbed.push({ type: 'del', text: op.text }, { type: 'ins', text: op.text });
    } else absorbed.push(op);
  });
  const out: Op[] = [];
  let del = '';
  let ins = '';
  const flush = () => {
    if (del) out.push({ type: 'del', text: del });
    if (ins) out.push({ type: 'ins', text: ins });
    del = ins = '';
  };
  for (const op of absorbed) {
    if (op.type === 'del') del += op.text;
    else if (op.type === 'ins') ins += op.text;
    else {
      flush();
      const last = out[out.length - 1];
      if (last?.type === 'eq') last.text += op.text;
      else out.push({ ...op });
    }
  }
  flush();
  return out;
};

/** Fraction of characters changed (0 = identical, 1 = nothing in common). */
export const changedRatio = (ops: Op[]): number => {
  let changed = 0;
  let total = 0;
  for (const op of ops) {
    const len = op.text.length;
    total += op.type === 'eq' ? 2 * len : len;
    if (op.type !== 'eq') changed += len;
  }
  return total ? changed / total : 0;
};

export const DIFF_ATTR = 'data-diff';
/** Elements whose text is not part of the current document (deleted text). */
export const DELETED_SELECTOR = `[${DIFF_ATTR}="del"], [${DIFF_ATTR}="del-block"]`;

export const isDeletedText = (n: Node): boolean => !!n.parentElement?.closest(DELETED_SELECTOR);

/** Text nodes under `root` in document order, excluding deleted diff text. */
export const liveTextNodes = (root: Node): Text[] => {
  const doc = root.ownerDocument ?? (root as Document);
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const out: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) if (!isDeletedText(n)) out.push(n as Text);
  return out;
};

const diffEl = (doc: Document, tag: string, kind: string, cls: string): HTMLElement => {
  const e = doc.createElement(tag);
  e.className = cls;
  e.setAttribute(DIFF_ATTR, kind);
  e.setAttribute('contenteditable', 'false');
  return e;
};

/**
 * Apply `ops` (diff old → block.textContent) to the live `block`: inserted text is wrapped in
 * `<ins class="diff-ins">` per text node, deleted text is inserted as `<del class="diff-del">`.
 * Returns false (and does nothing) if the ops don't match the block's text.
 */
export const applyWordDiff = (block: HTMLElement, ops: Op[]): boolean => {
  const cur = ops.filter(o => o.type !== 'del').map(o => o.text).join('');
  if (cur !== liveTextNodes(block).map(t => t.data).join('')) return false;
  type Edit = { kind: 'ins'; from: number; to: number } | { kind: 'del'; at: number; text: string };
  const edits: Edit[] = [];
  let pos = 0;
  for (const op of ops) {
    if (op.type === 'eq') pos += op.text.length;
    else if (op.type === 'ins') {
      edits.push({ kind: 'ins', from: pos, to: pos + op.text.length });
      pos += op.text.length;
    } else edits.push({ kind: 'del', at: pos, text: op.text });
  }
  // Wrap insertions first (they only split text nodes), then place deletions.
  for (const e of edits) if (e.kind === 'ins') wrapRange(block, e.from, e.to);
  for (const e of edits) if (e.kind === 'del') insertDel(block, e.at, e.text);
  return true;
};

const wrapRange = (block: HTMLElement, from: number, to: number): void => {
  const doc = block.ownerDocument;
  let start = 0;
  for (const t of liveTextNodes(block)) {
    const end = start + t.data.length;
    const a = Math.max(from, start);
    const b = Math.min(to, end);
    if (a < b) {
      let node = t;
      if (a > start) node = node.splitText(a - start);
      if (b < end) node.splitText(b - a);
      const ins = diffEl(doc, 'ins', 'ins', 'diff-ins');
      node.replaceWith(ins);
      ins.append(node);
    }
    start = end;
    if (start >= to) break;
  }
};

const insertDel = (block: HTMLElement, at: number, text: string): void => {
  const del = diffEl(block.ownerDocument, 'del', 'del', 'diff-del');
  del.textContent = text;
  const nodes = liveTextNodes(block);
  let start = 0;
  for (const t of nodes) {
    const end = start + t.data.length;
    if (at <= end && at >= start) {
      // Prefer attaching to the end of the previous text (outside a following <ins>).
      if (at === start) {
        let ref: Node = t;
        while (ref.parentElement && ref.parentElement !== block && ref.parentElement.getAttribute(DIFF_ATTR) === 'ins' && ref.parentElement.firstChild === ref) ref = ref.parentElement;
        ref.parentNode!.insertBefore(del, ref);
      } else if (at === end) {
        let ref: Node = t;
        while (ref.parentElement && ref.parentElement !== block && ref.parentElement.getAttribute(DIFF_ATTR) === 'ins' && ref.parentElement.lastChild === ref) ref = ref.parentElement;
        ref.parentNode!.insertBefore(del, ref.nextSibling);
      } else {
        const rest = t.splitText(at - start);
        rest.parentNode!.insertBefore(del, rest);
      }
      return;
    }
    start = end;
  }
  block.append(del);
};

/** Undo everything `applyWordDiff` / del-blocks did under `root`. */
export const clearWordDiff = (root: HTMLElement): void => {
  root.querySelectorAll(`[${DIFF_ATTR}="del"], [${DIFF_ATTR}="del-block"]`).forEach(n => n.remove());
  root.querySelectorAll(`[${DIFF_ATTR}="ins"]`).forEach(n => {
    n.replaceWith(...Array.from(n.childNodes));
  });
  root.normalize();
};
