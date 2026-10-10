//! Rendered-view selection -> raw source byte range. Owned by the render/mapping Worker.

use crate::protocol::Selection;
use std::ops::Range;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MappingError {
    #[error("bad sourcepos: {0}")]
    BadSourcepos(String),
    #[error("selected text not found in source")]
    NotFound,
    #[error("selection spans multiple blocks")]
    CrossBlock,
}

/// Map a rendered-view selection to a raw byte range suitable for inserting markers.
pub fn map_selection(_raw: &str, _sel: &Selection) -> Result<Range<usize>, MappingError> {
    Err(MappingError::NotFound)
}
