// Block serializer for Edit mode: one edited block's DOM -> Markdown for *that block only*
// (inline content; the daemon keeps the block's prefix: heading #s, list marker, `>`).
//
// Supported: text, **strong**/<b>, *em*/<i>, `code` (backtick-safe), [text](href "title"),
// bare autolinks, ![alt](src), ~~strike~~, <br> as a hard break (`\` + newline), soft breaks,
// task checkboxes (`[ ] ` / `[x] ` prefix), `<mark data-cid>` re-emitted as comment markers.
// Anything else is reduced to its text. Markdown-significant characters are escaped only where
// they would otherwise change the rendering; diff decorations (`data-diff`) are never emitted.
import { DIFF_ATTR } from './worddiff';

export type BlockKind = 'para' | 'heading' | 'cell';

export const blockKind = (host: Element): BlockKind => {
  const t = host.tagName;
  if (/^H[1-6]$/.test(t)) return 'heading';
  if (t === 'TD' || t === 'TH') return 'cell';
  return 'para';
};

/** The task checkbox of a list item host: inside it (tight item) or before it (loose item). */
export const taskBox = (host: Element): HTMLInputElement | null => {
  const own = host.querySelector<HTMLInputElement>(':scope > input[type="checkbox"]');
  if (own) return own;
  const li = host.parentElement;
  if (host.tagName === 'P' && li?.tagName === 'LI' && li.querySelector(':scope > p') === host)
    return li.querySelector<HTMLInputElement>(':scope > input[type="checkbox"]');
  return null;
};

/** Whether `host` is the text of a list item (a `[ ] ` at its start would become a checkbox). */
const isItemText = (host: Element): boolean =>
  host.classList.contains('li-body') || (host.tagName === 'P' && host.parentElement?.tagName === 'LI' && host.parentElement.querySelector(':scope > p') === host);

const isDeleted = (n: Node): boolean => {
  const e = n instanceof Element ? n : n.parentElement;
  const d = e?.closest(`[${DIFF_ATTR}]`)?.getAttribute(DIFF_ATTR);
  return d === 'del' || d === 'del-block';
};

interface State {
  kind: BlockKind;
  itemText: boolean;
  out: string;
  /** Some visible content was emitted on the current line (comment markers don't count). */
  lineHas: boolean;
  /** Plain text of the block, for neighbour-aware escaping. */
  plain: string;
  offsets: Map<Text, number>;
  opens: Map<Element, string[]>;
  closes: Map<Element, string[]>;
  need: { star: boolean; under: boolean; tilde: boolean; tick: boolean };
  /** Emit text verbatim (bare autolinks). */
  verbatim: boolean;
}

const ws = (c: string | undefined): boolean => c === undefined || c === '' || /\s/.test(c);
const alnum = (c: string | undefined): boolean => !!c && /[\p{L}\p{N}]/u.test(c);
const punct = (c: string | undefined): boolean => !!c && /[!-/:-@[-`{-~]/.test(c);
const ENTITY = /^&(?:#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,31});/;

const liveTexts = (root: Node): Text[] => {
  const walker = (root.ownerDocument ?? (root as Document)).createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const out: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) if (!isDeleted(n)) out.push(n as Text);
  return out;
};

/** Comment markers: open before the first non-empty mark of an id, close after its last one.
 * A mark inside a code span attaches to the code element (markers inside backticks would be
 * literal text). */
const planMarks = (host: Element): { opens: Map<Element, string[]>; closes: Map<Element, string[]> } => {
  const first = new Map<string, Element>();
  const last = new Map<string, Element>();
  for (const m of host.querySelectorAll('mark[data-cid]')) {
    if (isDeleted(m)) continue;
    const id = m.getAttribute('data-cid') ?? '';
    if (!/^[A-Za-z0-9]+$/.test(id) || !liveTexts(m).some(t => t.data.length > 0)) continue;
    const code = m.closest('code');
    const at = code && host.contains(code) ? code : m;
    if (!first.has(id)) first.set(id, at);
    last.set(id, at);
  }
  const order = [...first.keys()];
  const opens = new Map<Element, string[]>();
  const closes = new Map<Element, string[]>();
  for (const id of order) {
    const a = first.get(id)!;
    opens.set(a, [...(opens.get(a) ?? []), id]);
  }
  for (const id of [...order].reverse()) {
    const b = last.get(id)!;
    closes.set(b, [...(closes.get(b) ?? []), id]);
  }
  return { opens, closes };
};

const emit = (st: State, s: string, visible = true): void => {
  st.out += s;
  if (visible && s) st.lineHas = true;
};

/** Escape one text node's data. `base` = offset of the node in `st.plain`. */
const text = (data: string, base: number, st: State): void => {
  if (st.verbatim) return emit(st, data.replace(/\u00a0/g, ' '));
  const oneLine = st.kind !== 'para';
  let s = '';
  const flush = () => {
    if (s) emit(st, s);
    s = '';
  };
  for (let i = 0; i < data.length; i++) {
    let c = data[i];
    if (c === '\u00a0') c = ' ';
    const g = base + i;
    const prev = st.plain[g - 1];
    const next = st.plain[g + 1];
    if (c === '\n' || c === '\r') {
      if (c === '\r') continue;
      if (oneLine) c = ' ';
      else {
        flush();
        if (!st.out.endsWith('\n') && st.out !== '') st.out += '\n';
        st.lineHas = false;
        continue;
      }
    }
    if (!st.lineHas && !s && (c === ' ' || c === '\t')) continue; // leading whitespace
    const rest = data.slice(i);
    if (!st.lineHas && !s) {
      // Line-start constructs: headings, quotes, list markers, setext/thematic lines, tasks.
      if (c === '#' && /^#{1,6}(\s|$)/.test(rest)) { s += '\\#'; continue; }
      if (c === '>') { s += '\\>'; continue; }
      if ((c === '-' || c === '+' || c === '*') && (ws(next) || next === '\n')) { s += '\\' + c; continue; }
      if ((c === '-' || c === '=') && /^(-+|=+)[ \t]*(\n|$)/.test(rest)) { s += '\\' + c; continue; }
      const num = /^(\d{1,9})([.)])(\s|$)/.exec(rest);
      if (num) {
        s += num[1] + '\\' + num[2];
        i += num[1].length + num[2].length - 1;
        continue;
      }
      if (c === '[' && ((st.itemText && /^\[[ xX]\](\s|$)/.test(rest)) || /^\[[^\]]*\]:/.test(rest))) { s += '\\['; continue; }
    }
    switch (c) {
      case '\\':
        s += i === data.length - 1 || punct(next) || next === '\n' ? '\\\\' : '\\';
        break;
      case '`':
        s += st.need.tick ? '\\`' : '`';
        break;
      case '*':
        s += st.need.star && !(ws(prev) && ws(next)) ? '\\*' : '*';
        break;
      case '_':
        s += st.need.under && !(alnum(prev) && alnum(next)) && !(ws(prev) && ws(next)) ? '\\_' : '_';
        break;
      case '~':
        s += st.need.tilde && !(ws(prev) && ws(next)) ? '\\~' : '~';
        break;
      case '<':
        s += next === '!' ? '&lt;' : next && /[A-Za-z/?]/.test(next) ? '\\<' : '<';
        break;
      case '&':
        s += ENTITY.test(rest) ? '\\&' : '&';
        break;
      case ']':
        s += next === '(' || next === '[' ? '\\]' : ']';
        break;
      case '|':
        s += st.kind === 'cell' ? '\\|' : '|';
        break;
      case '#':
        // A trailing `#` run after a space would be read as a closing sequence.
        s += st.kind === 'heading' && ws(prev) && /^#+\s*$/.test(st.plain.slice(g)) ? '\\#' : '#';
        break;
      default:
        s += c;
    }
  }
  flush();
};

/** Wrap serialized children in a delimiter, keeping edge whitespace outside it. */
const wrapped = (el: Element, delim: string, st: State): void => {
  const saved = st.out;
  const savedLine = st.lineHas;
  st.out = '';
  st.lineHas = true;
  children(el, st);
  const inner = st.out;
  st.out = saved;
  const m = /^(\s*)([\s\S]*?)(\s*)$/.exec(inner)!;
  const visible = m[2].replace(/<!--\/?c:[A-Za-z0-9]+-->/g, '');
  if (!visible) {
    st.out += inner;
    st.lineHas = savedLine || /\S/.test(inner.replace(/<!--\/?c:[A-Za-z0-9]+-->/g, ''));
    return;
  }
  st.out += m[1] + delim + m[2] + delim + m[3];
  st.lineHas = !m[3].includes('\n');
};

const codeSpan = (el: Element, st: State): void => {
  let content = liveTexts(el).map(t => t.data).join('').replace(/\u00a0/g, ' ').replace(/\n/g, ' ');
  if (!content) return;
  if (st.kind === 'cell') content = content.replace(/\|/g, '\\|');
  const runs = content.match(/`+/g) ?? [];
  const n = Math.max(0, ...runs.map(r => r.length)) + 1;
  const fence = '`'.repeat(n);
  const pad = content.startsWith('`') || content.endsWith('`') || (/^ .*[^ ].* $/.test(content));
  emit(st, fence + (pad ? ' ' : '') + content + (pad ? ' ' : '') + fence);
};

const destination = (url: string): string => (/[\s<>]/.test(url) || !balanced(url) ? `<${url.replace(/[<>]/g, c => encodeURIComponent(c))}>` : url);
const balanced = (s: string): boolean => {
  let d = 0;
  for (const c of s) {
    if (c === '(') d++;
    else if (c === ')' && --d < 0) return false;
  }
  return d === 0;
};
const title = (t: string | null): string => (t ? ` "${t.replace(/(["\\])/g, '\\$1')}"` : '');

const link = (el: Element, st: State): void => {
  const href = el.getAttribute('href') ?? '';
  const txt = liveTexts(el).map(t => t.data).join('');
  if (!txt.trim()) return;
  const bare = !el.getAttribute('title') && (txt === href || 'mailto:' + txt === href || 'http://' + txt === href) && /^(https?:\/\/|www\.|[^\s@]+@[^\s@]+\.[^\s@]+$)/.test(txt);
  if (bare) {
    st.verbatim = true;
    children(el, st);
    st.verbatim = false;
    return;
  }
  const saved = st.out;
  st.out = '';
  emit(st, '[');
  children(el, st);
  const inner = st.out;
  st.out = saved + inner + '](' + destination(href) + title(el.getAttribute('title')) + ')';
  st.lineHas = true;
};

const image = (el: Element, st: State): void => {
  const alt = (el.getAttribute('alt') ?? '').replace(/([[\]\\])/g, '\\$1');
  emit(st, `![${alt}](${destination(el.getAttribute('src') ?? '')}${title(el.getAttribute('title'))})`);
};

const BLOCKISH = new Set(['DIV', 'P', 'LI', 'UL', 'OL', 'BLOCKQUOTE', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'PRE', 'TABLE', 'TR']);

const node = (n: Node, st: State): void => {
  if (n.nodeType === Node.TEXT_NODE) {
    if (isDeleted(n)) return;
    return text((n as Text).data, st.offsets.get(n as Text) ?? 0, st);
  }
  if (n.nodeType !== Node.ELEMENT_NODE) return; // comments, processing instructions
  const el = n as Element;
  const diff = el.getAttribute(DIFF_ATTR);
  if (diff === 'del' || diff === 'del-block') return;
  for (const id of st.opens.get(el) ?? []) emit(st, `<!--c:${id}-->`, false);
  switch (el.tagName) {
    case 'STRONG':
    case 'B':
      wrapped(el, '**', st);
      break;
    case 'EM':
    case 'I':
      wrapped(el, '*', st);
      break;
    case 'DEL':
    case 'S':
    case 'STRIKE':
      wrapped(el, '~~', st);
      break;
    case 'CODE':
      codeSpan(el, st);
      break;
    case 'A':
      link(el, st);
      break;
    case 'IMG':
      image(el, st);
      break;
    case 'BR':
      if (st.kind !== 'para') emit(st, ' ');
      else if (st.lineHas) {
        st.out += '\\\n';
        st.lineHas = false;
      }
      break;
    case 'INPUT':
    case 'SCRIPT':
    case 'STYLE':
    case 'svg':
    case 'SVG':
      break;
    default:
      if (BLOCKISH.has(el.tagName) && st.kind === 'para' && st.out && !st.out.endsWith('\n')) {
        st.out += '\n';
        st.lineHas = false;
      }
      children(el, st);
  }
  for (const id of st.closes.get(el) ?? []) emit(st, `<!--/c:${id}-->`, false);
};

const children = (el: Node, st: State): void => {
  for (const c of Array.from(el.childNodes)) node(c, st);
};

/** Serialize an edited block (paragraph, heading, list item text, table cell) to Markdown. */
export const serializeBlock = (host: Element, opts: { task?: boolean } = {}): string => {
  const kind = blockKind(host);
  const texts = liveTexts(host);
  const offsets = new Map<Text, number>();
  let plain = '';
  for (const t of texts) {
    offsets.set(t, plain.length);
    plain += t.data;
  }
  const has = (sel: string) => Array.from(host.querySelectorAll(sel)).some(e => !isDeleted(e) && (e.textContent ?? '').trim() !== '');
  const count = (c: string) => plain.split(c).length - 1;
  const { opens, closes } = planMarks(host);
  const st: State = {
    kind,
    itemText: isItemText(host),
    out: '',
    lineHas: false,
    plain,
    offsets,
    opens,
    closes,
    need: {
      star: count('*') >= 2 || has('strong, b, em, i'),
      under: count('_') >= 2,
      tilde: count('~') >= 2 || has('del:not([data-diff]), s, strike'),
      tick: count('`') >= 2 || has('code'),
    },
    verbatim: false,
  };
  children(host, st);
  let md = st.out
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n[ \t]+/g, '\n')
    .replace(/\n{2,}/g, '\n')
    .replace(/^[ \t\n]+/, '')
    .replace(/(?:\\\n|\s)+$/, '');
  const box = opts.task === false ? null : taskBox(host);
  if (box) md = (box.checked ? '[x] ' : '[ ] ') + md;
  return md;
};

/** Normalized visible text of a block (diff deletions excluded), to compare renders. */
export const blockText = (host: Element): string =>
  liveTexts(host)
    .map(t => t.data)
    .join('')
    .replace(/[\s\u00a0]+/g, ' ')
    .trim();
