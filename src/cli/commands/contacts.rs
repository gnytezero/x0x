//! Contact and trust management CLI commands.

use crate::cli::{print_value, DaemonClient};
use anyhow::Result;

/// `x0x contacts [list]` — GET /contacts
pub async fn list(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/contacts").await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x contacts add` — POST /contacts
pub async fn add(
    client: &DaemonClient,
    agent_id: &str,
    trust: &str,
    label: Option<&str>,
) -> Result<()> {
    client.ensure_running().await?;
    let mut body = serde_json::json!({
        "agent_id": agent_id,
        "trust_level": trust,
    });
    if let Some(l) = label {
        body["label"] = serde_json::Value::String(l.to_string());
    }
    let resp = client.post("/contacts", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x contacts update` — PATCH /contacts/:agent_id
pub async fn update(
    client: &DaemonClient,
    agent_id: &str,
    trust: Option<&str>,
    identity_type: Option<&str>,
) -> Result<()> {
    client.ensure_running().await?;
    let mut body = serde_json::Map::new();
    if let Some(t) = trust {
        body.insert(
            "trust_level".to_string(),
            serde_json::Value::String(t.to_string()),
        );
    }
    if let Some(it) = identity_type {
        body.insert(
            "identity_type".to_string(),
            serde_json::Value::String(it.to_string()),
        );
    }
    let resp = client
        .patch(&format!("/contacts/{agent_id}"), &body)
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x contacts remove` — DELETE /contacts/:agent_id
pub async fn remove(client: &DaemonClient, agent_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.delete(&format!("/contacts/{agent_id}")).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x contacts revoke` — POST /contacts/:agent_id/revoke
pub async fn revoke(client: &DaemonClient, agent_id: &str, reason: &str) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({ "reason": reason });
    let resp = client
        .post(&format!("/contacts/{agent_id}/revoke"), &body)
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x contacts revocations` — GET /contacts/:agent_id/revocations
pub async fn revocations(client: &DaemonClient, agent_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client
        .get(&format!("/contacts/{agent_id}/revocations"))
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x trust set` — POST /contacts/trust
pub async fn trust_set(client: &DaemonClient, agent_id: &str, level: &str) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({
        "agent_id": agent_id,
        "level": level,
    });
    let resp = client.post("/contacts/trust", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x trust evaluate` — POST /trust/evaluate
pub async fn trust_evaluate(client: &DaemonClient, agent_id: &str, machine_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({
        "agent_id": agent_id,
        "machine_id": machine_id,
    });
    let resp = client.post("/trust/evaluate", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}
