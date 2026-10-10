import type { EditorView } from '@codemirror/view';
import { anchorRange, isCodeComment } from '../format/parse';
import { commentField, getComments } from '../editor/state';

const GROUPS = ['mc-comment', 'mc-comment-alt'] as const;
const HIGHLIGHT_ACTIVE = 'mc-comment-active';
const PANE_SELECTOR = '.markdown-body';

type TextPos = { node: Text; offset: number };

/**
 * Paints comment anchors inside MarkEdit-preview's rendered pane using the CSS Custom Highlight
 * API, so the preview DOM is never modified. Each comment's quoted text is searched in the
 * pane's text, in document order, with whitespace collapsed on both sides.
 */
export class PreviewHighlights {
  private observer: MutationObserver;
  private timer: number | null = null;
  private ranges = new Map<string, Range>();
  private supported = typeof CSS !== 'undefined' && 'highlights' in CSS && typeof Highlight !== 'undefined';

  constructor(private view: EditorView, private onPick: (id: string) => void) {
    this.observer = new MutationObserver(() => this.schedule());
    this.observer.observe(document.body, { childList: true, subtree: true, characterData: true });
    document.addEventListener('mousedown', this.onMouseDown, true);
    this.schedule();
  }

  destroy(): void {
    this.observer.disconnect();
    document.removeEventListener('mousedown', this.onMouseDown, true);
    if (this.supported) {
      for (const g of GROUPS) CSS.highlights.delete(g);
      CSS.highlights.delete(HIGHLIGHT_ACTIVE);
    }
  }

  schedule(): void {
    if (!this.supported) return;
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      this.timer = null;
      this.repaint();
    }, 80);
  }

  repaint(): void {
    if (!this.supported) return;
    const pane = document.querySelector<HTMLElement>(PANE_SELECTOR);
    const field = this.view.state.field(commentField, false);
    this.ranges.clear();
    if (!pane || !field || !field.highlights || pane.childNodes.length === 0) {
      for (const g of GROUPS) CSS.highlights.delete(g);
      CSS.highlights.delete(HIGHLIGHT_ACTIVE);
      return;
    }

    const { text, positions } = collectText(pane);
    const doc = this.view.state.doc.toString();
    const comments = getComments(this.view.state)
      .filter(c => !isCodeComment(c) && anchorRange(c) !== null)
      .sort((a, b) => a.open!.from - b.open!.from);

    // Nested comments alternate between two groups so their overlap paints darker instead of merging.
    const groups = GROUPS.map(() => new Highlight());
    const active = new Highlight();
    let cursor = 0;
    let depthToggle = 0;
    let lastEnd = -1;
    for (const c of comments) {
      const r = anchorRange(c)!;
      const needle = normalize(stripMarkers(doc.slice(r.from, r.to)));
      if (!needle) continue;
      let at = text.indexOf(needle, cursor);
      if (at < 0) at = text.indexOf(needle);
      if (at < 0) continue;
      const start = positions[at];
      const end = positions[at + needle.length - 1];
      if (!start || !end) continue;
      const range = new Range();
      range.setStart(start.node, start.offset);
      range.setEnd(end.node, end.offset + 1);
      this.ranges.set(c.id, range);
      depthToggle = at < lastEnd ? depthToggle ^ 1 : 0;
      lastEnd = Math.max(lastEnd, at + needle.length);
      (c.id === field.activeId ? active : groups[depthToggle]!).add(range);
      // Advance one character only: a nested comment starts inside the previous match, while a
      // repeated identical quote still resolves to the next occurrence.
      cursor = at + 1;
    }
    GROUPS.forEach((name, i) => CSS.highlights.set(name, groups[i]!));
    CSS.highlights.set(HIGHLIGHT_ACTIVE, active);
  }

  private onMouseDown = (event: MouseEvent) => {
    if (event.button !== 0 || this.ranges.size === 0) return;
    const target = event.target instanceof Element ? event.target : null;
    if (!target?.closest(PANE_SELECTOR)) return;
    const caret = document.caretRangeFromPoint?.(event.clientX, event.clientY);
    if (!caret) return;
    for (const [id, range] of this.ranges) {
      if (range.comparePoint(caret.startContainer, caret.startOffset) === 0) {
        this.onPick(id);
        return;
      }
    }
  };
}

/** Concatenate the pane's visible text with whitespace collapsed, mapping every character back to its node. */
const collectText = (root: HTMLElement): { text: string; positions: TextPos[] } => {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: node => {
      const parent = node.parentElement;
      if (!parent || parent.closest('script, style, pre code')) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let text = '';
  const positions: TextPos[] = [];
  let lastWasSpace = true;
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    const data = node.data;
    for (let i = 0; i < data.length; i++) {
      const ch = data.charAt(i);
      if (/\s/.test(ch)) {
        if (!lastWasSpace) {
          text += ' ';
          positions.push({ node, offset: i });
          lastWasSpace = true;
        }
      } else {
        text += ch;
        positions.push({ node, offset: i });
        lastWasSpace = false;
      }
    }
    // Block boundaries between text nodes count as whitespace in the rendered output.
    if (!lastWasSpace) {
      text += ' ';
      positions.push({ node, offset: Math.max(0, data.length - 1) });
      lastWasSpace = true;
    }
  }
  return { text, positions };
};

const stripMarkers = (s: string): string => s.replace(/<!--\/?c:[A-Za-z0-9]+-->/g, '');
const normalize = (s: string): string => s.replace(/\s+/g, ' ').trim();
