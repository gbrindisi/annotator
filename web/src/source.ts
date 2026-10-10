import { EditorState, StateEffect, StateField, Compartment, RangeSetBuilder, type Range } from '@codemirror/state';
import { Decoration, EditorView, keymap, drawSelection, highlightActiveLine, type DecorationSet } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { markdown } from '@codemirror/lang-markdown';
import { syntaxHighlighting, defaultHighlightStyle, HighlightStyle } from '@codemirror/language';
import { tags } from '@lezer/highlight';
import { anchorSpans, hiddenSpans } from './decorations';
import type { CommentView } from './protocol';

const setMarkers = StateEffect.define<{ raw: string; ranges: [number, number][]; comments: CommentView[] }>();

const hideDeco = Decoration.replace({});

export const buildDecorations = (raw: string, ranges: [number, number][], comments: CommentView[] = []): { hidden: DecorationSet; anchors: DecorationSet } => {
  const b = new RangeSetBuilder<Decoration>();
  for (const s of hiddenSpans(ranges, raw.length)) b.add(s.from, s.to, hideDeco);
  const marks: Range<Decoration>[] = anchorSpans(comments, raw.length).map(s =>
    Decoration.mark({ class: s.resolved ? 'cm-anchor is-resolved' : 'cm-anchor', attributes: { 'data-cid': s.id } }).range(s.from, s.to),
  );
  return { hidden: b.finish(), anchors: Decoration.set(marks, true) };
};

const markerField = StateField.define<{ hidden: DecorationSet; anchors: DecorationSet }>({
  create: () => ({ hidden: Decoration.none, anchors: Decoration.none }),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMarkers)) return buildDecorations(e.value.raw, e.value.ranges, e.value.comments);
    if (!tr.docChanged) return value;
    return { hidden: value.hidden.map(tr.changes), anchors: value.anchors.map(tr.changes) };
  },
  provide: f => [
    EditorView.decorations.from(f, v => v.hidden),
    EditorView.decorations.from(f, v => v.anchors),
    EditorView.atomicRanges.of(view => view.state.field(f).hidden),
  ],
});

const mdHighlight = HighlightStyle.define([
  { tag: tags.heading1, fontSize: '1.35em', fontWeight: '700' },
  { tag: tags.heading2, fontSize: '1.2em', fontWeight: '700' },
  { tag: [tags.heading3, tags.heading4, tags.heading5, tags.heading6], fontWeight: '700' },
  { tag: tags.strong, fontWeight: '700' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.link, color: 'var(--accent-text)' },
  { tag: tags.monospace, fontFamily: 'var(--mono)' },
]);

export type SourceEditorOptions = {
  onChange: (text: string) => void;
  onSelectionChange?: (empty: boolean) => void;
  onCommentShortcut?: () => void;
};

export class SourceEditor {
  readonly view: EditorView;
  private readOnly = new Compartment();
  private applying = false;

  constructor(parent: HTMLElement, opts: SourceEditorOptions) {
    this.view = new EditorView({
      parent,
      state: EditorState.create({
        doc: '',
        extensions: [
          history(),
          drawSelection(),
          highlightActiveLine(),
          EditorView.lineWrapping,
          markdown(),
          syntaxHighlighting(mdHighlight),
          syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
          markerField,
          this.readOnly.of([]),
          keymap.of([
            { key: 'Mod-Alt-m', run: () => (opts.onCommentShortcut?.(), true) },
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          EditorView.updateListener.of(u => {
            if (u.docChanged && !this.applying) opts.onChange(u.state.doc.toString());
            if (u.selectionSet) opts.onSelectionChange?.(u.state.selection.main.empty);
          }),
        ],
      }),
    });
  }

  text(): string {
    return this.view.state.doc.toString();
  }

  /** Replace the document keeping cursor and scroll where possible. */
  setDoc(raw: string, ranges: [number, number][], comments: CommentView[]): void {
    const cur = this.view.state;
    const sel = cur.selection.main;
    const scroller = this.view.scrollDOM;
    const top = scroller.scrollTop;
    this.applying = true;
    try {
      const same = cur.doc.toString() === raw;
      const clamp = (n: number) => Math.min(n, raw.length);
      this.view.dispatch({
        changes: same ? undefined : { from: 0, to: cur.doc.length, insert: raw },
        selection: { anchor: clamp(sel.anchor), head: clamp(sel.head) },
        effects: setMarkers.of({ raw, ranges, comments }),
      });
    } finally {
      this.applying = false;
    }
    scroller.scrollTop = top;
  }

  /** Main selection as UTF-16 offsets (CodeMirror positions are UTF-16). */
  selection(): { from: number; to: number } | null {
    const s = this.view.state.selection.main;
    return s.empty ? null : { from: s.from, to: s.to };
  }

  setReadOnly(ro: boolean): void {
    this.view.dispatch({ effects: this.readOnly.reconfigure(ro ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : []) });
  }

  scrollToCid(id: string): void {
    const set = this.view.state.field(markerField).anchors;
    const it = set.iter();
    while (it.value) {
      if (it.value.spec.attributes?.['data-cid'] === id) {
        this.view.dispatch({ selection: { anchor: it.from }, effects: EditorView.scrollIntoView(it.from, { y: 'center' }) });
        return;
      }
      it.next();
    }
  }
}
