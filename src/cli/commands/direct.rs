//! Direct messaging CLI commands.

use crate::cli::{print_value, DaemonClient, OutputFormat};
use anyhow::Result;
use base64::Engine;

/// `x0x direct connect` — POST /agents/connect
pub async fn connect(client: &DaemonClient, agent_id: &str) -> Result<()> {
    client.ensure_running().await?;
    let body = serde_json::json!({ "agent_id": agent_id });
    let resp = client.post("/agents/connect", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x direct send` — POST /direct/send
///
/// `require_ack_ms` opts into a post-send peer-liveness probe: after the
/// envelope has been handed to the DM path, x0xd calls ant-quic
/// `probe_peer` against the recipient's MachineId with the given timeout
/// and includes the RTT (or the failure reason) in the response under
/// `require_ack`. This does NOT prove the specific message was delivered;
/// it proves the peer's receive pipeline is live when the call returned.
pub async fn send(
    client: &DaemonClient,
    agent_id: &str,
    message: &str,
    require_ack_ms: Option<u64>,
) -> Result<()> {
    client.ensure_running().await?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(message.as_bytes());
    let mut body = serde_json::json!({
        "agent_id": agent_id,
        "payload": encoded,
    });
    if let Some(ms) = require_ack_ms {
        body["require_ack_ms"] = serde_json::json!(ms);
    }
    let resp = client.post("/direct/send", &body).await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x direct connections` — GET /direct/connections
pub async fn connections(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    let resp = client.get("/direct/connections").await?;
    print_value(client.format(), &resp);
    Ok(())
}

/// `x0x direct events` — stream GET /direct/events
pub async fn events(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    eprintln!("Streaming direct messages... (Ctrl+C to stop)");

    use futures::StreamExt;

    let resp = client.get_stream("/direct/events").await?;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

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
