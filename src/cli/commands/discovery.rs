//! Agent discovery CLI commands.

use crate::cli::{print_value, DaemonClient};
use anyhow::Result;

/// `x0x agents [list]` — GET /agents/discovered
pub async fn list(client: &DaemonClient, unfiltered: bool) -> Result<()> {
    client.ensure_running().await?;
    let resp = if unfiltered {
        client
            .get_query("/agents/discovered", &[("unfiltered", "true")])
            .await?
    } else {
        client.get("/agents/discovered").await?
    };
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x agents get` — GET /agents/discovered/:agent_id
pub async fn get(client: &DaemonClient, agent_id: &str, wait: Option<u64>) -> Result<()> {
    client.ensure_running().await?;
    let path = format!("/agents/discovered/{agent_id}");
    let resp = if let Some(secs) = wait {
        client
            .get_query(&path, &[("wait", &secs.to_string())])
            .await?
    } else {
        client.get(&path).await?
    };
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x agents find` — POST /agents/find/:agent_id
pub async fn find(client: &DaemonClient, agent_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client
        .post_empty(&format!("/agents/find/{agent_id}"))
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x agents reachability` — GET /agents/reachability/:agent_id
pub async fn reachability(client: &DaemonClient, agent_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client
        .get(&format!("/agents/reachability/{agent_id}"))
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x agents by-user` — GET /users/:user_id/agents
pub async fn by_user(client: &DaemonClient, user_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get(&format!("/users/{user_id}/agents")).await?;
    print_value(client.format(), &resp);
    Ok(())
}
