//! Daemon lifecycle CLI commands.

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;

use crate::cli::DaemonClient;

/// `x0x start` — spawn x0xd as a background process.
pub async fn start(name: Option<&str>, config: Option<&Path>, foreground: bool) -> Result<()> {
    // Find x0xd binary: same directory as x0x, then PATH.
    let x0xd_path = find_x0xd()?;

    // Check if already running.
    let format = crate::cli::OutputFormat::Text;
    if let Ok(client) = DaemonClient::new(name, None, format) {
        if client.ensure_running().await.is_ok() {
            println!("Daemon already running at {}", client.base_url());
            return Ok(());
        }
    }

    let mut cmd = std::process::Command::new(&x0xd_path);
    if let Some(n) = name {
        cmd.arg("--name").arg(n);
    }
    if let Some(c) = config {
        cmd.arg("--config").arg(c);
    }

    if foreground {
        // Replace current process with x0xd.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = cmd.exec();
            anyhow::bail!("failed to exec x0xd: {err}");
        }
        #[cfg(not(unix))]
        {
            let status = cmd.status().context("failed to run x0xd")?;
            if !status.success() {
                anyhow::bail!("x0xd exited with {status}");
            }
            return Ok(());
        }
    }

    // Background: spawn and wait for health.
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let _child = cmd.spawn().context("failed to spawn x0xd")?;

    // Poll health for up to 5 seconds.
    let client = DaemonClient::new(name, None, format)?;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if client.ensure_running().await.is_ok() {
            println!("Daemon started at {}", client.base_url());
            return Ok(());
        }
    }

    println!(
        "Daemon spawned but not yet reachable at {}",
        client.base_url()
    );
    Ok(())
}

/// `x0x stop` — POST /shutdown
pub async fn stop(client: &DaemonClient) -> Result<()> {
    client.ensure_running().await?;
    match client.post_empty("/shutdown").await {
        Ok(_) => println!("Daemon shutting down."),
        Err(e) => {
            // Connection reset is expected when the server shuts down.
            let msg = format!("{e:#}");
            if msg.contains("connection") || msg.contains("reset") || msg.contains("closed") {
                println!("Daemon shutting down.");
            } else {
                return Err(e);
            }
        }
    }
    Ok(())
}

/// `x0x doctor` — run diagnostics against the daemon.
pub async fn doctor(client: &DaemonClient) -> Result<()> {
    println!("Running diagnostics...\n");

    // 1. Health check.
    print!("Health check: ");
    match client.ensure_running().await {
        Ok(()) => println!("OK"),
        Err(e) => {
            println!("FAIL — {e}");
            return Ok(());
        }
    }

    // 2. Agent identity.
    print!("Agent identity: ");
    match client.get("/agent").await {
        Ok(val) => {
            let agent_id = val
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("{agent_id}");
        }
        Err(e) => println!("FAIL — {e}"),
    }

    // 3. Network status.
    print!("Network: ");
    match client.get("/status").await {
        Ok(val) => {
            let peers = val.get("peers").and_then(|v| v.as_u64()).unwrap_or(0);
            let connectivity = val
                .get("connectivity")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("{peers} peers, {connectivity}");
        }
        Err(e) => println!("FAIL — {e}"),
    }

    // 4. Contacts.
    print!("Contacts: ");
    match client.get("/contacts").await {
        Ok(val) => {
            let count = val
                .get("contacts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            println!("{count} contacts");
        }
        Err(e) => println!("FAIL — {e}"),
    }

    println!("\nDiagnostics complete.");
    Ok(())
}

/// `x0x instances` — list running daemon instances.
pub async fn instances() -> Result<()> {
    let data_dir = dirs::data_dir().context("cannot determine data directory")?;

    let mut found = Vec::new();

    // Check default instance.
    let default_port = data_dir.join("x0x").join("api.port");
    if default_port.exists() {
        found.push(("(default)".to_string(), default_port));
    }

    // Check named instances.
    if let Ok(entries) = std::fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(instance) = name_str.strip_prefix("x0x-") {
                let port_file = entry.path().join("api.port");
                if port_file.exists() {
                    found.push((instance.to_string(), port_file));
                }
            }
        }
    }

    if found.is_empty() {
        println!("No running instances found.");
        return Ok(());
    }

    let name_width = found.iter().map(|(n, _)| n.len()).max().unwrap_or(4).max(4);
    println!("{:<name_width$}  {:<21}  {:<10}", "NAME", "API", "STATUS");

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    for (name, port_file) in &found {
        let addr = std::fs::read_to_string(port_file)
            .unwrap_or_default()
            .trim()
            .to_string();
        let status = if !addr.is_empty() {
            match http_client
                .get(format!("http://{addr}/health"))
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => "running",
                _ => "stale",
            }
        } else {
            "stale"
        };
        println!("{:<name_width$}  {:<21}  {:<10}", name, addr, status);
    }

    Ok(())
}

/// Find the x0xd binary.
fn find_x0xd() -> Result<std::path::PathBuf> {
    // Same directory as x0x binary.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("x0xd");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // Search PATH.
    if let Ok(path) = which::which("x0xd") {
        return Ok(path);
    }

    anyhow::bail!("x0xd not found. Install it or ensure it's in the same directory as x0x.")
}
