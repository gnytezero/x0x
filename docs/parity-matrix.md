# x0x Capability Parity Matrix

**Target:** every capability in x0x is reachable — and behaves identically —
from **every surface**. The REST API on `x0xd` is the source of truth; every
other surface is a client of it.

| # | Surface | Transport | Coverage source |
|---|---------|-----------|-----------------|
| 1 | REST API (`x0xd`) | HTTP/JSON + WS + SSE | `tests/api_coverage.rs`, `tests/daemon_api_integration.rs` |
| 2 | CLI (`x0x`) | Wraps REST | `tests/parity_cli.rs` (every endpoint has a CLI command) |
| 3 | Embedded HTML GUI (`src/gui/x0x-gui.html`) | Wraps REST via fetch | `tests/gui_smoke.rs`, `tests/gui_named_group_parity.rs`, `tests/e2e_gui_chrome.mjs` (this release) |
| 4 | `communitas-x0x-client` (Rust) | Wraps REST + WS + SSE | `communitas/communitas-x0x-client/tests/` |
| 5 | `communitas-core` (Rust library) | Wraps `communitas-x0x-client` | `communitas/communitas-core/tests/` |
| 6 | `communitas-ui-api` (Tauri / IPC) | JSON over Tauri bridge | `communitas/communitas-ui-api/tests/` |
| 7 | `communitas-ui-service` (WebRTC signaling etc.) | Wraps `x0x-client` | `communitas/communitas-ui-service/tests/` |
| 8 | `communitas-dioxus` (desktop GUI) | Uses `communitas-ui-service` | `communitas/communitas-dioxus/tests/e2e/` (this release) |
| 9 | `communitas-kanban` (task view) | Uses `communitas-x0x-client` task lists | `communitas/communitas-kanban/tests/` |
| 10 | `communitas-bench` (perf harness) | `communitas-x0x-client` | `communitas/communitas-bench/` |
| 11 | `communitas-apple` (Swift app) | Wraps REST through `X0xClient` Swift lib | `communitas/communitas-apple/Tests/X0xClientTests/`, `communitas/communitas-apple/Tests/CommunitasUITests/` (this release) |

> **Note.** Previous releases shipped first-party Python (PyO3) and Node.js
> (napi-rs) bindings; both were retired in favour of the daemon + REST model
> so that there is exactly one supported surface per host. Non-Rust
> applications consume `x0xd` over HTTP/WebSocket — see
> [`docs/local-apps.md`](local-apps.md).

---

## Capability → surface matrix

Legend: ✅ implemented & tested · 🟡 implemented, test gap · ❌ not yet wired ·
`—` not applicable for this surface.

### Identity
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Get agent id / card | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | ✅ | — |
| Import agent card | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |
| Export/backup keypairs | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | — |
| User (human) identity (opt-in) | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | — |
| Agent certificate verify | ✅ | ✅ | — | ✅ | ✅ | ✅ | — | — | — |

### Trust & contacts
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Add / block / trust contact | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |
| Machine-pinning enforcement | ✅ | ✅ | 🟡 | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |
| Trust evaluator decision read | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | — |

### Connectivity / discovery
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Connect to agent (direct / coordinated) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | — |
| Probe peer liveness (**0.27.2 new**) | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | — |
| Connection health snapshot (**0.27.1 new**) | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | — |
| Peer lifecycle subscription (**0.27.1 new**) | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | — |
| Discover agents (cache / FOAF) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |
| `GET /diagnostics/connectivity` | ✅ | ✅ | ✅ | — | — | ✅ | — | — | — |
| `GET /diagnostics/gossip` (this release) | ✅ | ✅ | ✅ | — | — | ✅ | — | — | — |
| Four-word network bootstrap | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |

### Messaging — pub/sub
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Publish | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Subscribe | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Unsubscribe | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| WebSocket live feed | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | — |

### Messaging — direct (DM-over-gossip)
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Send direct | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Receive direct (annotated) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Epidemic rebroadcast on caps topic | ✅ | — | — | — | — | ✅ | — | — | — |
| Send + receive-ACK (**0.27.1 new**) | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | — |
| File transfer (offer/accept) | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | 🟡 | — |

### Groups
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Create named group | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Invite / join / leave | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Policy (roles, bans) | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | 🟡 | 🟡 | — |
| Discover groups (tag / nearby) | ✅ | ✅ | 🟡 | 🟡 | 🟡 | ✅ | 🟡 | 🟡 | — |
| MLS encryption | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | — |

### KV store
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Create / list stores | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |
| PUT / GET / DELETE key | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |
| Access-policy enforcement | ✅ | ✅ | 🟡 | ✅ | ✅ | ✅ | 🟡 | 🟡 | — |

### Task lists (CRDT)
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Create / join task list | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Add / update item | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Claim / done transitions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Concurrent-merge correctness | ✅ | — | — | ✅ | ✅ | ✅ | — | — | — |

### Presence
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Online list | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| FOAF walk | ✅ | ✅ | 🟡 | 🟡 | 🟡 | ✅ | 🟡 | 🟡 | — |
| Find specific agent | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Status / reachability | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | — |
| Events SSE | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | — |

### Upgrade / self-update
| Capability | REST | CLI | GUI | Py | Node | x0x-client | Dioxus | Apple | Kanban |
|---|---|---|---|---|---|---|---|---|---|
| Check updates | ✅ | ✅ | ✅ | — | — | ✅ | 🟡 | ✅ (Sparkle) | — |
| Apply update | ✅ | ✅ | 🟡 | — | — | 🟡 | 🟡 | ✅ (Sparkle) | — |
| Gossip manifest propagation | ✅ | — | — | — | — | 🟡 | — | — | — |

---

## Red-cell ticket list (gaps to close in this release)

1. ~~**Probe-peer / connection-health / lifecycle subscription**~~ — closed in
   v0.19.6. REST handlers (`POST /peers/:id/probe`, `GET /peers/:id/health`,
   `GET /peers/events` SSE) + CLI commands (`x0x peers probe|health|events`)
   + x0x-client (`probe_peer`, `peer_health`, `connect_peer_events`) +
   GUI panels (live peer-events feed, probe button on each peer row) all
   wired and round-trip-tested via `tests/peer_lifecycle_integration.rs`.
   **Known wart**: `/peers/:id/health` returns `format!("{health:?}")` — the
   ant-quic `ConnectionHealth` Debug rendering. Functional today (substring
   match on `connected: true` is stable), but a structured-JSON shape would
   let clients act programmatically. Tracked for a future release.
2. ~~**`send_with_receive_ack`**~~ — closed in v0.19.6. `POST /direct/send`
   accepts opt-in `require_ack_ms`; CLI exposes `--require-ack-ms`;
   `communitas-x0x-client::send_direct` accepts the option; GUI DM composer
   has an "ACK" toggle that surfaces the round-trip RTT inline. Round-trip
   tested via `direct_send_with_require_ack_round_trips_to_live_peer`.
3. ~~**`/diagnostics/gossip`**~~ — closed in v0.19.6. GUI panel renders the
   per-stream dispatcher stats; `communitas-x0x-client::gossip_stats` ships.
4. **Communitas Dioxus & Apple** — broad identity/trust/kv surface is
   "implemented" via the Rust client but test coverage is thin. XCUITest
   target + Dioxus WebDriver harness (planned for the next session) start
   closing those cells.
5. **Bench / kanban** — historical parity gaps; tracked but out of scope
   until usage warrants.

---

## Proof artefacts

Per-run artefacts land in `./proofs/YYYY-MM-DD-HHMM/`:

- `proof-report.json` — machine-readable capability → surface pass/fail
- `logs/` — one JSON log per daemon process (`X0X_LOG_DIR=./proofs/.../logs`)
- `gossip-stats-*.json` — pre/post snapshots of `GET /diagnostics/gossip`
- `connectivity-*.json` — pre/post snapshots of `GET /diagnostics/connectivity`
- `xcuitest.xcresult` — Apple UI tests bundle
- `dioxus-e2e.log` — Dioxus driver transcript
- `chrome-gui.har` — network HAR for GUI run
- `chrome-gui.console.jsonl` — console logs for GUI run

The acceptance gate is `proof-report.json`: every ✅ cell in the matrix
must have `status: "pass"` and every 🟡 cell must have `status: "pending"`
with a follow-up ticket id.
