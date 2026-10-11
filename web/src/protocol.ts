// Mirror of src/protocol.rs (browser <-> daemon part). Rust Option<T> = T | null.

export interface Selection {
  /** data-sourcepos of the innermost block element containing the selection. */
  sourcepos: string;
  text: string;
  /** UTF-16 offset of the selection start within the block's textContent. */
  offset: number;
}

export type ClientMsg =
  | { type: 'add_comment'; selection: Selection; body: string }
  | { type: 'add_comment_raw'; from: number; to: number; body: string }
  | { type: 'reply'; id: string; body: string }
  | { type: 'edit_entry'; id: string; entry: number; body: string }
  | { type: 'delete_entry'; id: string; entry: number }
  | { type: 'resolve'; id: string }
  | { type: 'reopen'; id: string }
  | { type: 'delete_thread'; id: string }
  | { type: 'replace_source'; text: string; base_hash: string }
  | { type: 'done' }
  | { type: 'abandon' };

export interface EntryView {
  author: string;
  timestamp: string | null;
  text: string;
}

export interface CommentView {
  id: string;
  status: string;
  quote: string | null;
  author: string | null;
  created_at: string | null;
  orphaned: boolean;
  /** 0-based inclusive lines inside a fenced code block. */
  code_lines: [number, number] | null;
  /** UTF-16 range of the anchored text in `raw`, when anchored. */
  anchor: [number, number] | null;
  entries: EntryView[];
  new_entries: number;
  new_thread: boolean;
}

export type ChangeKind = 'added' | 'modified' | 'removed';

export interface Change {
  kind: ChangeKind | string;
  sourcepos: string | null;
  old_text: string | null;
  after: string | null;
  /** Old block rendered by the daemon (modified/removed); absent from older daemons. */
  old_html?: string | null;
}

export interface DocState {
  path: string;
  round: number;
  hash: string;
  raw: string;
  html: string;
  comments: CommentView[];
  marker_ranges: [number, number][];
  changes: Change[];
  round_state: 'active' | 'closed' | string;
  author: string;
  /** Pending agent `wait` long-polls for this round; 0 = nobody waiting. Absent from older daemons. */
  waiters?: number;
}

export type ServerMsg =
  | ({ type: 'doc' } & DocState)
  | { type: 'error'; message: string; reloaded: boolean }
  | { type: 'round_closed'; round: number };

/** Intent constructors: the only way the UI builds outgoing messages. */
export const msg = {
  addComment: (selection: Selection, body: string): ClientMsg => ({ type: 'add_comment', selection, body }),
  addCommentRaw: (from: number, to: number, body: string): ClientMsg => ({ type: 'add_comment_raw', from, to, body }),
  reply: (id: string, body: string): ClientMsg => ({ type: 'reply', id, body }),
  editEntry: (id: string, entry: number, body: string): ClientMsg => ({ type: 'edit_entry', id, entry, body }),
  deleteEntry: (id: string, entry: number): ClientMsg => ({ type: 'delete_entry', id, entry }),
  resolve: (id: string): ClientMsg => ({ type: 'resolve', id }),
  reopen: (id: string): ClientMsg => ({ type: 'reopen', id }),
  deleteThread: (id: string): ClientMsg => ({ type: 'delete_thread', id }),
  replaceSource: (text: string, baseHash: string): ClientMsg => ({ type: 'replace_source', text, base_hash: baseHash }),
  done: (): ClientMsg => ({ type: 'done' }),
  abandon: (): ClientMsg => ({ type: 'abandon' }),
};

/** Parse a server frame; returns null for anything malformed or unknown. */
export const parseServerMsg = (data: string): ServerMsg | null => {
  let value: unknown;
  try {
    value = JSON.parse(data);
  } catch {
    return null;
  }
  if (!value || typeof value !== 'object') return null;
  const t = (value as { type?: unknown }).type;
  if (t === 'doc' || t === 'error' || t === 'round_closed') return value as ServerMsg;
  return null;
};

export const docFromMsg = (m: { type: 'doc' } & DocState): DocState => {
  const { type: _type, ...doc } = m;
  return doc;
};
