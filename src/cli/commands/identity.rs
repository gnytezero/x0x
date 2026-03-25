//! Identity CLI commands.

use crate::cli::{print_value, DaemonClient};
use anyhow::Result;

/// `x0x agent` — GET /agent
pub async fn agent(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/agent").await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x agent user-id` — GET /agent/user-id
pub async fn user_id(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/agent/user-id").await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x announce` — POST /announce
pub async fn announce(client: &DaemonClient, include_user: bool, consent: bool) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({
        "include_user": include_user,
        "human_consent": consent,
    });
    let resp = client.post("/announce", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}
