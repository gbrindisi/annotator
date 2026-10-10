//! Frontend assets. Release builds embed `web/dist`; debug builds read it from disk.

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
pub struct Assets;
