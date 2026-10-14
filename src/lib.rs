//! annotator: local review sessions for agent-written Markdown.
//!
//! Module ownership (see the plan): the Rust side owns parsing, rendering, writes, diffs and
//! summaries; the browser is a thin view that sends intents (`protocol::ClientMsg`).

pub mod assets;
pub mod blockedit;
pub mod client;
pub mod config;
pub mod daemon;
pub mod diff;
pub mod format;
pub mod mapping;
pub mod protocol;
pub mod render;
pub mod session;
pub mod summary;
pub mod time;
pub mod utf16;
