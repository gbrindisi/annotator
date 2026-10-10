//! Port of `reference/markedit-comments/format/ids.ts`.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Generate a short (5 char, `[a-z0-9]`) id not contained in `existing`.
pub fn new_id(existing: &[&str]) -> String {
    for _ in 0..1000 {
        let id = random_id(5);
        if !existing.contains(&id.as_str()) {
            return id;
        }
    }
    random_id(8)
}

fn random_id(len: usize) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    (0..len)
        .map(|_| {
            // RandomState is randomly keyed per process; mixing a counter gives fresh values.
            let mut h = RandomState::new().build_hasher();
            h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
            ALPHABET[(h.finish() % ALPHABET.len() as u64) as usize] as char
        })
        .collect()
}
