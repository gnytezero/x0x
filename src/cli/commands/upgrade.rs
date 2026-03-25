//! Upgrade CLI commands.

use crate::cli::{print_value, DaemonClient};
use anyhow::Result;

/// `x0x upgrade` — GET /upgrade
pub async fn check(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/upgrade").await?;
    print_value(client.format(), &resp);
    Ok(())
}
