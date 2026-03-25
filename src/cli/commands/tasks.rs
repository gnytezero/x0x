//! Collaborative task list CLI commands.

use crate::cli::{print_value, DaemonClient};
use anyhow::Result;

/// `x0x tasks [list]` — GET /task-lists
pub async fn list(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/task-lists").await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x tasks create` — POST /task-lists
pub async fn create(client: &DaemonClient, name: &str, topic: &str) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({
        "name": name,
        "topic": topic,
    });
    let resp = client.post("/task-lists", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x tasks show` — GET /task-lists/:id/tasks
pub async fn show(client: &DaemonClient, list_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get(&format!("/task-lists/{list_id}/tasks")).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x tasks add` — POST /task-lists/:id/tasks
pub async fn add(
    client: &DaemonClient,
    list_id: &str,
    title: &str,
    description: Option<&str>,
) -> Result<()> {
    client.ensure_running().await?;
    let mut body = serde_json::json!({ "title": title });
    if let Some(desc) = description {
        body["description"] = serde_json::Value::String(desc.to_string());
    }
    let resp = client
        .post(&format!("/task-lists/{list_id}/tasks"), &body)
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x tasks claim/complete` — PATCH /task-lists/:id/tasks/:tid
pub async fn update(
    client: &DaemonClient,
    list_id: &str,
    task_id: &str,
    action: &str,
) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({ "action": action });
    let resp = client
        .patch(&format!("/task-lists/{list_id}/tasks/{task_id}"), &body)
        .await?;
    print_value(client.format(), &resp);
    Ok(())
}
