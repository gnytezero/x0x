//! Gossip messaging CLI commands.

use crate::cli::{print_value, DaemonClient, OutputFormat};
use anyhow::Result;
use base64::Engine;

/// `x0x publish` — POST /publish
pub async fn publish(client: &DaemonClient, topic: &str, payload: &str) -> Result<()> {
    client.ensure_running().await?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
    let body = serde_json::json!({
        "topic": topic,
        "payload": encoded,
    });
    let resp = client.post("/publish", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x subscribe` — POST /subscribe + stream /events
pub async fn subscribe(client: &DaemonClient, topic: &str) -> Result<()> {
    client.ensure_running().await?;

    // Create subscription.
    let body = serde_json::json!({ "topic": topic });
    let sub_resp = client.post("/subscribe", &body).await?;
    let sub_id = sub_resp
        .get("subscription_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    eprintln!("Subscribed to '{topic}' (id: {sub_id}). Streaming events... (Ctrl+C to stop)");

    // Stream SSE events.
    stream_sse(client, "/events").await?;

    // Cleanup: unsubscribe.
    let _ = client.delete(&format!("/subscribe/{sub_id}")).await;
    Ok(())
}

/// `x0x unsubscribe` — DELETE /subscribe/:id
pub async fn unsubscribe(client: &DaemonClient, id: &str) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.delete(&format!("/subscribe/{id}")).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x events` — stream GET /events
pub async fn events(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    eprintln!("Streaming events... (Ctrl+C to stop)");
    stream_sse(client, "/events").await
}

/// Stream SSE events from a path, printing each data line to stdout.
async fn stream_sse(client: &DaemonClient, path: &str) -> Result<()> {
    use futures::StreamExt;

    let resp = client.get_stream(path).await?;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Parse SSE frames: split on double newline.
        while let Some(pos) = buffer.find("\n\n") {
            let frame = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    match client.format() {
                        OutputFormat::Json => println!("{data}"),
                        OutputFormat::Text => {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                print_value(OutputFormat::Text, &val);
                            } else {
                                println!("{data}");
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
