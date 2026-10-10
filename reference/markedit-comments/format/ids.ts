// Ported from kylemcd/obsidian-document-comments (MIT). See NOTICE.

const ALPHABET = 'abcdefghijklmnopqrstuvwxyz0123456789';

/** Generate a short id unique within `existing`. */
export const generateId = (existing: Iterable<string> = []): string => {
  const taken = existing instanceof Set ? existing : new Set(existing);
  for (let attempt = 0; attempt < 1000; attempt++) {
    const id = randomId(5);
    if (!taken.has(id)) return id;
  }
  return randomId(8);
};

const randomId = (len: number): string => {
  const bytes = new Uint8Array(len);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, b => ALPHABET[b % ALPHABET.length]).join('');
};
