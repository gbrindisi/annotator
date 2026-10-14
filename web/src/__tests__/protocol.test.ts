import { describe, expect, it } from 'vitest';
import { docFromMsg, msg, parseServerMsg } from '../protocol';

describe('client messages', () => {
  it('match the serde shapes of ClientMsg', () => {
    const sel = { sourcepos: '1:1-1:5', text: 'hi', offset: 0 };
    expect(JSON.parse(JSON.stringify(msg.addComment(sel, 'b')))).toEqual({ type: 'add_comment', selection: sel, body: 'b' });
    expect(msg.addCommentRaw(3, 7, 'b')).toEqual({ type: 'add_comment_raw', from: 3, to: 7, body: 'b' });
    expect(msg.reply('k', 'x')).toEqual({ type: 'reply', id: 'k', body: 'x' });
    expect(msg.editEntry('k', 1, 'x')).toEqual({ type: 'edit_entry', id: 'k', entry: 1, body: 'x' });
    expect(msg.deleteEntry('k', 0)).toEqual({ type: 'delete_entry', id: 'k', entry: 0 });
    expect(msg.resolve('k')).toEqual({ type: 'resolve', id: 'k' });
    expect(msg.reopen('k')).toEqual({ type: 'reopen', id: 'k' });
    expect(msg.deleteThread('k')).toEqual({ type: 'delete_thread', id: 'k' });
    expect(msg.replaceSource('t', 'h')).toEqual({ type: 'replace_source', text: 't', base_hash: 'h' });
    expect(msg.replaceBlock('1:1-1:5', 'md', 'h')).toEqual({ type: 'replace_block', sourcepos: '1:1-1:5', markdown: 'md', base_hash: 'h' });
    expect(msg.mergeBlocks('1:1-1:5', '3:1-3:2', 'md', 'h')).toEqual({ type: 'merge_blocks', first: '1:1-1:5', second: '3:1-3:2', markdown: 'md', base_hash: 'h' });
    expect(msg.done()).toEqual({ type: 'done' });
    expect(msg.abandon()).toEqual({ type: 'abandon' });
  });
});

describe('server messages', () => {
  it('parses doc, error and round_closed; rejects junk', () => {
    const doc = { type: 'doc', path: '/a.md', round: 2, hash: 'h', raw: 'r', html: '', comments: [], marker_ranges: [[0, 3]], changes: [], round_state: 'active', author: 'me' };
    const m = parseServerMsg(JSON.stringify(doc));
    expect(m?.type).toBe('doc');
    if (m?.type === 'doc') expect(docFromMsg(m)).not.toHaveProperty('type');
    expect(parseServerMsg('{"type":"error","message":"x","reloaded":true}')).toEqual({ type: 'error', message: 'x', reloaded: true });
    expect(parseServerMsg('{"type":"round_closed","round":1}')).toEqual({ type: 'round_closed', round: 1 });
    expect(parseServerMsg('{"type":"nope"}')).toBeNull();
    expect(parseServerMsg('not json')).toBeNull();
  });
});
