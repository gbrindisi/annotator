//! The comment format (byte-compatible with markedit-comments / Obsidian Document Comments).
//! Port of markedit-comments `src/format/*` (derived from kylemcd/obsidian-document-comments, MIT; see NOTICE).
//! All offsets are byte offsets into the document `&str`.

pub mod edits;
pub mod escape;
pub mod ids;
pub mod locate;
pub mod parse;
pub mod serialize;
pub mod types;

pub use edits::*;
pub use ids::new_id;
pub use locate::locate_text;
pub use parse::{existing_ids, fenced_ranges, is_in_fenced_code, masked_ranges, parse_comments};
pub use serialize::{close_marker, open_marker, serialize_body};
pub use types::*;
