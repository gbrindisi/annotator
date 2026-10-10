import type { TextRange } from './types';

const MARKER_RE = /<!--\/?c:[A-Za-z0-9]+-->/g;

/**
 * Find rendered (preview) text back in the Markdown source. Whitespace is collapsed on both
 * sides and comment markers in the source are skipped, since neither is visible in a preview.
 * Returns the first match, or null when the text cannot be found.
 */
export const locateText = (doc: string, rendered: string): TextRange | null => {
  const needle = normalize(rendered);
  if (!needle) return null;

  // Build the normalized haystack with a map back to source offsets.
  let hay = '';
  const map: number[] = [];
  let lastWasSpace = true;
  let i = 0;
  while (i < doc.length) {
    MARKER_RE.lastIndex = i;
    const m = MARKER_RE.exec(doc);
    if (m && m.index === i) {
      i += m[0].length;
      continue;
    }
    const ch = doc.charAt(i);
    if (/\s/.test(ch)) {
      if (!lastWasSpace) {
        hay += ' ';
        map.push(i);
        lastWasSpace = true;
      }
    } else {
      hay += ch;
      map.push(i);
      lastWasSpace = false;
    }
    i++;
  }

  const at = hay.indexOf(needle);
  if (at < 0) return null;
  const from = map[at];
  const lastIdx = map[at + needle.length - 1];
  if (from === undefined || lastIdx === undefined) return null;
  return { from, to: lastIdx + 1 };
};

const normalize = (s: string): string => s.replace(/\s+/g, ' ').trim();
