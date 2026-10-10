//! The comment format (byte-compatible with markedit-comments / Obsidian Document Comments).
//! Port of `reference/markedit-comments/format/*`. Owned by the format Worker.
//! All offsets are byte offsets into the document `&str`.

pub mod types;

pub use types::*;
