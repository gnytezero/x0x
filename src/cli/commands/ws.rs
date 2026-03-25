//! WebSocket CLI commands.

use crate::cli::{print_value, DaemonClient};
use anyhow::Result;

/// `x0x ws sessions` — GET /ws/sessions
pub async fn sessions(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/ws/sessions").await?;
    print_value(client.format(), &resp);
    Ok(())
}
