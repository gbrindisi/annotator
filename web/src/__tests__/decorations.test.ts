import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { anchorSpans, hiddenSpans } from '../decorations';
import { buildDecorations } from '../source';

const raw = 'We 👍 <!--c:k3-->ship it<!--/c:k3--> now.\n<!--co:k3 status:open\nme: hi\n-->\nend';
const anchorFrom = raw.indexOf('<!--c:k3-->') + 11;
const comments = (status = 'open') => [{ id: 'k3', status, anchor: [anchorFrom, anchorFrom + 7] as [number, number] }] as any;
const ranges = (): [number, number][] => {
  const open = raw.indexOf('<!--c:k3-->');
  const close = raw.indexOf('<!--/c:k3-->');
  const body = raw.indexOf('\n<!--co:');
  return [[close, close + 12], [open, open + 11], [body, raw.indexOf('-->', body) + 3]];
};

describe('marker ranges -> decorations', () => {
  it('sorts, clamps and merges ranges', () => {
    expect(hiddenSpans([[5, 8], [0, 2], [2, 3], [7, 10], [12, 99], [4, 4]], 20)).toEqual([
      { from: 0, to: 3 }, { from: 5, to: 10 }, { from: 12, to: 20 },
    ]);
  });

  it('uses daemon-provided anchor ranges (UTF-16 positions)', () => {
    const spans = anchorSpans(comments(), raw.length);
    expect(spans).toHaveLength(1);
    expect(raw.slice(spans[0].from, spans[0].to)).toBe('ship it');
    expect(spans[0].id).toBe('k3');
  });

  it('builds replace + mark decorations usable by CodeMirror', () => {
    const { hidden, anchors } = buildDecorations(raw, ranges(), comments('resolved'));
    const state = EditorState.create({ doc: raw });
    expect(state.doc.length).toBe(raw.length); // CodeMirror positions are UTF-16 too
    const h: [number, number][] = [];
    hidden.between(0, raw.length, (f, t) => void h.push([f, t]));
    const r = ranges().sort((a, b) => a[0] - b[0]);
    expect(h).toEqual(r);
    const a: string[] = [];
    anchors.between(0, raw.length, (f, t, d) => void a.push(`${raw.slice(f, t)}|${d.spec.class}`));
    expect(a).toEqual(['ship it|cm-anchor is-resolved']);
  });
});
