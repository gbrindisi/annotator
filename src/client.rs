//! CLI client: discover or spawn the daemon, register a session, long-poll until Done.
//! Owned by the daemon/CLI Worker.

use std::path::Path;

/// Returns the process exit code: 0 Done, 2 Abandoned, 1 error.
pub fn open(_file: &Path, _json: bool, _no_browser: bool) -> anyhow::Result<i32> {
    anyhow::bail!("not implemented")
}

pub fn status() -> anyhow::Result<()> {
    anyhow::bail!("not implemented")
}

pub fn stop() -> anyhow::Result<()> {
    anyhow::bail!("not implemented")
}
