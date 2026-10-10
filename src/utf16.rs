//! UTF-16 code unit <-> UTF-8 byte offset conversion (wire offsets <-> Rust offsets).

/// Byte offset of UTF-16 offset `u` in `s`. Clamped to `s.len()`; an offset inside a surrogate
/// pair snaps to the start of that char.
pub fn to_byte(s: &str, u: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units + c.len_utf16() > u {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// UTF-16 offset of byte offset `b` in `s` (snapped down to a char boundary, clamped).
pub fn from_byte(s: &str, b: usize) -> usize {
    let mut b = b.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    s[..b].encode_utf16().count()
}

/// Converts many ascending-or-not byte offsets efficiently via a prefix table.
pub struct Utf16Index {
    /// `table[i]` = UTF-16 offset of byte `i` (for char boundaries), len = s.len() + 1.
    table: Vec<usize>,
}

impl Utf16Index {
    pub fn new(s: &str) -> Self {
        let mut table = vec![0; s.len() + 1];
        let mut u = 0;
        for (i, c) in s.char_indices() {
            for t in &mut table[i..i + c.len_utf8()] {
                *t = u;
            }
            u += c.len_utf16();
        }
        table[s.len()] = u;
        Utf16Index { table }
    }

    pub fn from_byte(&self, b: usize) -> usize {
        self.table[b.min(self.table.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_accented_emoji() {
        let s = "aé🎉b";
        // bytes: a=0, é=1..3, 🎉=3..7, b=7
        assert_eq!(from_byte(s, 0), 0);
        assert_eq!(from_byte(s, 1), 1);
        assert_eq!(from_byte(s, 3), 2);
        assert_eq!(from_byte(s, 7), 4);
        assert_eq!(from_byte(s, 8), 5);
        assert_eq!(from_byte(s, 5), 2); // inside the emoji: snaps down
        assert_eq!(to_byte(s, 0), 0);
        assert_eq!(to_byte(s, 1), 1);
        assert_eq!(to_byte(s, 2), 3);
        assert_eq!(to_byte(s, 3), 3); // middle of surrogate pair
        assert_eq!(to_byte(s, 4), 7);
        assert_eq!(to_byte(s, 5), 8);
        assert_eq!(to_byte(s, 99), 8);
        let idx = Utf16Index::new(s);
        for b in [0, 1, 3, 7, 8] {
            assert_eq!(idx.from_byte(b), from_byte(s, b));
        }
        for u in 0..=5 {
            assert_eq!(from_byte(s, to_byte(s, u)), if u == 3 { 2 } else { u });
        }
    }
}
