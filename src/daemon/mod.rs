//! The daemon: axum HTTP + WebSocket server, session registry, file watching, idle shutdown.
//! Owned by the daemon/CLI Worker.

pub fn serve(_foreground: bool) -> anyhow::Result<()> {
    anyhow::bail!("not implemented")
}
