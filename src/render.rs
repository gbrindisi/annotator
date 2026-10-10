//! Markdown -> HTML for the rendered view. Owned by the render/mapping Worker.
//!
//! Contract: comment markers and body blocks never reach comrak; anchors come back as
//! `<mark data-cid="ID">`, every block carries `data-sourcepos`, and the offset map lets
//! `mapping` translate positions in the rendered text back to raw byte offsets.

/// Output of [`render`].
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub html: String,
    /// The document with all markers and comment bodies removed (what comrak parsed).
    pub clean: String,
    /// `raw_offset[i]` = raw byte offset of clean byte `i` (len = clean.len() + 1).
    pub raw_offset: Vec<usize>,
}

/// Placeholder until the render Worker lands: escaped `<pre>` so the daemon can run.
pub fn render(raw: &str) -> Rendered {
    let escaped = raw
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    Rendered {
        html: format!("<pre data-sourcepos=\"1:1-1:1\">{escaped}</pre>"),
        clean: raw.to_string(),
        raw_offset: (0..=raw.len()).collect(),
    }
}
