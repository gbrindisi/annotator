// Ported from kylemcd/obsidian-document-comments (MIT). See NOTICE.

import type { CommentData, ThreadEntry } from './types';
import { breakTerminator, canonicalAuthorKey, encodeCodeQuote, escapeReactionAuthor, escapeText } from './escape';

export const openMarker = (id: string): string => `<!--c:${id}-->`;
export const closeMarker = (id: string): string => `<!--/c:${id}-->`;

/** Serialize a comment body block: `<!--co:ID header\n thread\n-->`. */
export const serializeBody = (id: string, data: CommentData): string => {
  const head: string[] = [`co:${id}`];
  if (data.author) head.push(`by:${canonicalAuthorKey(data.author)}`);
  if (data.createdAt) head.push(`at:${sanitizeToken(data.createdAt)}`);
  head.push(`status:${data.status}`);
  if (data.quote) {
    const quote = data.codeLines ? encodeCodeQuote(data.quote) : sanitizeQuote(data.quote);
    head.push(`quote:"${quote}"`);
  }
  if (data.codeLines) {
    const { from, to } = data.codeLines;
    head.push(`line:${from === to ? from : `${from}-${to}`}`);
  }

  const lines = data.thread.map(serializeEntry);
  const reactionLines = (data.reactions ?? [])
    .filter(r => r.authors.length > 0)
    .map(r => {
      const target = r.entry !== undefined && r.entry > 0 ? `@${r.entry} ` : '';
      return `+${target}${r.emoji} ${r.authors.map(escapeReactionAuthor).join(', ')}`;
    });
  const body = [...lines, ...reactionLines];
  const block = body.length ? body.join('\n') + '\n' : '';
  return `<!--${head.join(' ')}\n${block}-->`;
};

const serializeEntry = (e: ThreadEntry): string => {
  const who = e.timestamp ? `${e.author} (${e.timestamp})` : e.author;
  return `${breakTerminator(who)}: ${escapeText(breakTerminator(e.text))}`;
};

const sanitizeToken = (s: string): string => breakTerminator(s).replace(/\s+/g, '_');

const sanitizeQuote = (s: string): string => breakTerminator(s.replace(/\s+/g, ' ').replace(/"/g, "'")).trim();
