import { describe, it, expect } from 'vitest';
import { locateText } from '../src/format/locate';

describe('locateText', () => {
  it('finds plain text', () => {
    const doc = 'Hello brave new world.';
    expect(locateText(doc, 'brave new')).toEqual({ from: 6, to: 15 });
  });

  it('collapses whitespace and skips comment markers', () => {
    const doc = 'We should <!--c:k3f9-->ship on\nFriday<!--/c:k3f9--> regardless.';
    const r = locateText(doc, 'should ship  on Friday regardless')!;
    expect(doc.slice(r.from, r.to)).toBe('should <!--c:k3f9-->ship on\nFriday<!--/c:k3f9--> regardless');
  });

  it('returns null when missing or empty', () => {
    expect(locateText('abc', 'zzz')).toBeNull();
    expect(locateText('abc', '   ')).toBeNull();
  });
});
