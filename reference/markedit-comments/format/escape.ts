// Ported from kylemcd/obsidian-document-comments (MIT). See NOTICE.
// Byte compatibility of the on-disk format depends on this file; do not "improve" it.

/** Thread entries are one per physical line, so newlines and backslashes are escaped. */
export const escapeText = (s: string): string => {
  return s.replace(/\\/g, '\\\\').replace(/\r/g, '\\r').replace(/\n/g, '\\n');
};

export const unescapeText = (s: string): string => {
  return s.replace(/\\(.)/g, (match, c: string) => {
    if (c === 'n') return '\n';
    if (c === 'r') return '\r';
    if (c === '\\') return '\\';
    return match;
  });
};

/** Reaction authors are comma-joined on one line; escape commas in names. */
export const escapeReactionAuthor = (s: string): string => {
  return s.replace(/\\/g, '\\\\').replace(/,/g, '\\,');
};

export const splitReactionAuthors = (s: string): string[] => {
  return s
    .split(/(?<!\\),/)
    .map(author => author.trim().replace(/\\(.)/g, (match, c: string) => (c === ',' ? ',' : c === '\\' ? '\\' : match)))
    .filter(Boolean);
};

/** A code comment's quote must round-trip byte-for-byte inside the header's `quote:"…"`. */
export const encodeCodeQuote = (s: string): string => {
  return s
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\q')
    .replace(/\r/g, '\\r')
    .replace(/\n/g, '\\n')
    .replace(/-->/g, '--\\g>');
};

export const decodeCodeQuote = (s: string): string => {
  return s.replace(/\\(.)/g, (match, c: string) => {
    if (c === 'n') return '\n';
    if (c === 'r') return '\r';
    if (c === 'q') return '"';
    if (c === 'g') return '';
    if (c === '\\') return '\\';
    return match;
  });
};

/** `-->` must never appear literally inside the body; break it with a zero-width space. */
export const breakTerminator = (s: string): string => {
  return s.replace(/-->/g, '--​>');
};

/** Author handles are single tokens: whitespace becomes `_`. */
export const canonicalAuthorKey = (author: string): string => {
  return breakTerminator(author.trim()).replace(/\s+/g, '_');
};
