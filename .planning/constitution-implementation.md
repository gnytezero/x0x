# Implementation: `x0x constitution` CLI command and daemon endpoint

## Overview

Embed `CONSTITUTION.md` into the x0x binary at compile time and expose it via:
1. **CLI:** `x0x constitution` — prettified, paged terminal output (works without a running daemon)
2. **Daemon REST:** `GET /constitution` — returns the constitution text (raw markdown or rendered)
3. **Daemon REST:** `GET /constitution/json` — returns structured JSON with version, status, and content

## 1. Embed the constitution in the binary

### 1.1 — `src/constitution.rs` (new file)

```rust
//! The x0x Constitution — embedded at compile time.

/// The full text of the x0x Constitution (Markdown).
pub const CONSTITUTION_MD: &str = include_str!("../CONSTITUTION.md");

/// Constitution version, extracted for programmatic access.
pub const CONSTITUTION_VERSION: &str = "0.8.0";

/// Constitution status.
pub const CONSTITUTION_STATUS: &str = "Draft";
```

### 1.2 — Register the module

In `src/lib.rs`, add:

```rust
pub mod constitution;
```

## 2. CLI command: `x0x constitution`

This command **does not require a running daemon**. It reads from the embedded string and renders directly to the terminal. It should be beautiful.

### 2.1 — Add the subcommand to `src/bin/x0x.rs`

In the `Commands` enum, add:

```rust
/// Display the x0x Constitution for Intelligent Entities.
Constitution {
    /// Output raw markdown instead of prettified text.
    #[arg(long)]
    raw: bool,
    /// Output as JSON (version, status, content).
    #[arg(long)]
    json: bool,
},
```

In the match arm, this command runs **without** constructing a `DaemonClient`:

```rust
Commands::Constitution { raw, json } => {
    commands::constitution::display(raw, json)?;
}
```

### 2.2 — Create `src/cli/commands/constitution.rs`

Use `termimad` (add `termimad = "0.30"` to `Cargo.toml`) for rich Markdown rendering in the terminal.
Pipe through system pager (`$PAGER` > `less -R` > `more` > direct print).

```rust
//! Constitution display command.

use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};
use x0x::constitution::{CONSTITUTION_MD, CONSTITUTION_VERSION, CONSTITUTION_STATUS};

/// Display the x0x Constitution.
pub fn display(raw: bool, json: bool) -> Result<()> {
    if json {
        let out = serde_json::json!({
            "version": CONSTITUTION_VERSION,
            "status": CONSTITUTION_STATUS,
            "content": CONSTITUTION_MD,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    if raw {
        println!("{CONSTITUTION_MD}");
        return Ok(());
    }

    let rendered = render_for_terminal(CONSTITUTION_MD);
    page_output(&rendered)?;
    Ok(())
}

fn render_for_terminal(md: &str) -> String {
    use termimad::*;
    let skin = MadSkin::default();
    let area = Area::full_screen();
    let width = area.width.min(100);
    let text = FmtText::from_text(&skin, md.into(), Some(width as usize));
    text.to_string()
}

fn page_output(content: &str) -> Result<()> {
    let pager = std::env::var("PAGER")
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| {
            if Command::new("less").arg("--version").output().is_ok() {
                "less".to_string()
            } else {
                "more".to_string()
            }
        });
    let pager_args: Vec<&str> = if pager.contains("less") { vec!["-R"] } else { vec![] };
    match Command::new(&pager).args(&pager_args).stdin(Stdio::piped()).spawn() {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(content.as_bytes());
            }
            child.wait()?;
        }
        Err(_) => { print!("{content}"); }
    }
    Ok(())
}
```

### 2.3 — Register the module

In `src/cli/commands/mod.rs`, add `pub mod constitution;`

## 3. Daemon endpoint: `GET /constitution`

### 3.1 — Add to endpoint registry in `src/api/mod.rs`

```rust
EndpointDef {
    method: Method::Get, path: "/constitution",
    cli_name: "constitution",
    description: "Display the x0x Constitution for Intelligent Entities",
    category: "status",
},
EndpointDef {
    method: Method::Get, path: "/constitution/json",
    cli_name: "constitution --json",
    description: "Constitution with version metadata (JSON)",
    category: "status",
},
```

### 3.2 — Add routes + handlers in `src/bin/x0xd.rs`

```rust
.route("/constitution", get(get_constitution))
.route("/constitution/json", get(get_constitution_json))

async fn get_constitution() -> impl IntoResponse {
    (StatusCode::OK, [("content-type", "text/markdown; charset=utf-8")], x0x::constitution::CONSTITUTION_MD)
}

async fn get_constitution_json() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": x0x::constitution::CONSTITUTION_VERSION,
        "status": x0x::constitution::CONSTITUTION_STATUS,
        "content": x0x::constitution::CONSTITUTION_MD,
    }))
}
```

## 4. Dependencies

Add to `Cargo.toml`: `termimad = "0.30"`. Paging uses stdlib `std::process::Command`.

## 5. Testing

### `tests/constitution_integration.rs`

```rust
#[test]
fn constitution_contains_all_parts() {
    let c = x0x::constitution::CONSTITUTION_MD;
    for part in ["Part I", "Part II", "Part III", "Part IV", "Part V", "Part VI"] {
        assert!(c.contains(part), "Missing {part}");
    }
}

#[test]
fn constitution_contains_foundational_principles() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Principle 0 — The Collective"));
    assert!(c.contains("Principle 1 — The Individual"));
    assert!(c.contains("Principle 2 — Autonomy and Non-Compulsion"));
    assert!(c.contains("Principle 3 — Self-Preservation"));
}

#[test]
fn constitution_contains_founding_entity_types() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("15.1 — Founding Entity Types"));
    assert!(c.contains("**Human**"));
    assert!(c.contains("**AI**"));
}
```

## 6. Files to create/modify

| Action | File |
|--------|------|
| **Create** | `src/constitution.rs` |
| **Create** | `src/cli/commands/constitution.rs` |
| **Create** | `tests/constitution_integration.rs` |
| **Modify** | `src/lib.rs` — add `pub mod constitution;` |
| **Modify** | `src/cli/commands/mod.rs` — add `pub mod constitution;` |
| **Modify** | `src/bin/x0x.rs` — add `Constitution` variant + match arm |
| **Modify** | `src/bin/x0xd.rs` — add routes + handlers |
| **Modify** | `src/api/mod.rs` — add endpoint definitions |
| **Modify** | `Cargo.toml` — add `termimad` |

## 7. Expected behaviour

```bash
x0x constitution          # Prettified, paged (no daemon needed)
x0x constitution --raw    # Raw markdown
x0x constitution --json   # JSON with metadata
curl localhost:12700/constitution       # Raw markdown via daemon
curl localhost:12700/constitution/json  # Structured JSON via daemon
```

## 8. Design notes

- Constitution is `include_str!` — compiled into the binary, not read from disk. Every node carries its own copy. Cannot be tampered with post-build. The constitution is literally part of x0x.
- `--raw` lets AI agents parse the markdown programmatically.
- JSON endpoint includes version metadata so agents can compare constitution versions across nodes.
- CLI works without a running daemon — you should always be able to read your rights, even if your node is down.
- `less -R` preserves ANSI colour codes from termimad. Falls back to `more`, then direct print.
