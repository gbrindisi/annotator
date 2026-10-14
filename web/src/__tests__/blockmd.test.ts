import { describe, expect, it } from 'vitest';
import { blockText, serializeBlock } from '../blockmd';
import { applyWordDiff, wordDiff } from '../worddiff';
import { editableHosts, prepareEditable, spOf } from '../editable';
import cases from './blockmd_cases.json';

type Case = { name: string; html: string; blocks: { sourcepos: string; markdown: string }[] };

const mount = (html: string): HTMLElement => {
  const root = document.createElement('article');
  root.innerHTML = html;
  document.body.replaceChildren(root);
  return root;
};

const one = (html: string): HTMLElement => {
  const root = mount(html);
  prepareEditable(root);
  return editableHosts(root)[0];
};

describe('blockmd round-trips the daemon render (golden cases from tests/blockedit_web_cases.rs)', () => {
  for (const c of cases as Case[]) {
    it(c.name, () => {
      const root = mount(c.html);
      const hosts = prepareEditable(root);
      const got = hosts.map(h => ({ sourcepos: spOf(h), markdown: serializeBlock(h) }));
      expect(got).toEqual(c.blocks);
    });
  }
});

describe('blockmd', () => {
  it('re-emits one marker pair per comment across several marks and inside code', () => {
    const h = one('<p data-sourcepos="1:1-1:9">a <mark data-cid="x1">b </mark><strong><mark data-cid="x1">c</mark></strong> <code><mark data-cid="y2">k</mark></code></p>');
    expect(serializeBlock(h)).toBe('a <!--c:x1-->b **c<!--/c:x1-->** <!--c:y2-->`k`<!--/c:y2-->');
  });

  it('drops the markers of a mark whose text was deleted', () => {
    const h = one('<p data-sourcepos="1:1-1:9">a <mark data-cid="x1"></mark>b</p>');
    expect(serializeBlock(h)).toBe('a b');
  });

  it('never serializes diff decorations', () => {
    const h = one('<p data-sourcepos="1:1-1:9">old new text</p>');
    expect(applyWordDiff(h, wordDiff('gone text', 'old new text'))).toBe(true);
    expect(h.querySelector('[data-diff="del"]')).not.toBeNull();
    expect(serializeBlock(h)).toBe('old new text');
    expect(blockText(h)).toBe('old new text');
  });

  it('strips unknown markup to text and escapes what would change the rendering', () => {
    const h = one('<p data-sourcepos="1:1-1:9"><span style="color:red">red</span> <font>1. x</font><script>bad()</script> <u>*a*</u></p>');
    expect(serializeBlock(h)).toBe('red 1. x \\*a\\*');
    const h2 = one('<p data-sourcepos="1:1-1:9">- item\n# head\n&lt;!--c:zz--&gt; [ ] x</p>');
    expect(serializeBlock(h2)).toBe('\\- item\n\\# head\n&lt;!--c:zz--> [ ] x');
  });

  it('hard breaks, trailing browser <br>, nbsp and blank lines', () => {
    const h = one('<p data-sourcepos="1:1-1:9">a<br>b\u00a0\u00a0c\n\n\nd<br></p>');
    expect(serializeBlock(h)).toBe('a\\\nb  c\nd');
  });

  it('headings and cells are single-line; cells escape pipes', () => {
    expect(serializeBlock(one('<h2 data-sourcepos="1:1-1:9">a<br>b\nc ##</h2>'))).toBe('a b c \\##');
    expect(serializeBlock(one('<table><tr><td data-sourcepos="1:1-1:9">a|b <code>c|d</code></td></tr></table>'))).toBe('a\\|b `c\\|d`');
  });

  it('task items: checkbox state becomes the prefix', () => {
    const root = mount('<ul><li data-sourcepos="1:1-1:9"><input type="checkbox" disabled="" /> todo</li></ul>');
    const [h] = prepareEditable(root);
    const box = h.querySelector('input')!;
    expect(box.disabled).toBe(false);
    expect(serializeBlock(h)).toBe('[ ] todo');
    box.click();
    expect(serializeBlock(h)).toBe('[x] todo');
    expect(serializeBlock(h, { task: false })).toBe('todo');
  });

  it('code spans pick a safe fence', () => {
    expect(serializeBlock(one('<p data-sourcepos="1:1-1:9"><code>a``b</code> <code>`x</code></p>'))).toBe('```a``b``` `` `x ``');
  });

  it('links with odd destinations and titles', () => {
    const h = one('<p data-sourcepos="1:1-1:9"><a href="a b(c" title="say &quot;hi&quot;">t</a> <a href="mailto:me@x.io">me@x.io</a></p>');
    expect(serializeBlock(h)).toBe('[t](<a b(c> "say \\"hi\\"") me@x.io');
  });

  it('blocks with raw HTML are not editable', () => {
    const root = mount('<p data-sourcepos="1:1-1:9">a <!-- raw HTML omitted -->k</p><p data-sourcepos="3:1-3:2">ok</p>');
    expect(prepareEditable(root).map(spOf)).toEqual(['3:1-3:2']);
  });
});
