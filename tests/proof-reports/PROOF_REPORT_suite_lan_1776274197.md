# PROOF REPORT — LAN Suite

| Field | Value |
|-------|-------|
| Suite | LAN E2E Test Suite v2 — studio1.local + studio2.local Mac Studios |
| Run ID | `suite_lan_1776274197` |
| Timestamp | 2026-04-15T17:38:14Z (log line 241) |
| Version | `0.17.0` (log lines 22, 24: both studios) |
| Proof Token | `proof-1776274249-67163` (log lines 3, 240) |
| Studio1 agent_id | `fbf3ddcc6fae718fa2e1e809c96ba877...` (log line 26) |
| Studio2 agent_id | `1c7f260468e0884e738692f6fb731dbf...` (log line 29) |

---

## Summary

| Metric | Count |
|--------|-------|
| Total checks | 130 |
| PASS | 124 |
| FAIL | 6 |
| SKIP | 0 |

**Overall verdict: PARTIAL**

---

## Interface Proof Matrix

| Interface | Send Proven | Receive Proven | Evidence (log lines) |
|-----------|-------------|----------------|----------------------|
| REST | YES | YES — all CRUD round-trips proven | Lines 85–219 throughout |
| CLI | YES | YES — studio1 CLI agent_id matches REST (exact) | Line 65: exact match proof |
| GUI | YES (studio1 /gui serves HTML) | N/A — no GUI send test in LAN suite | Lines 53–56 |
| SSE | Not tested in LAN suite | N/A | — |
| WebSocket | Not tested in LAN suite | N/A | — |
| Direct Messaging | YES (send both directions) | PARTIAL — sends confirmed, SSE receive not tested | Lines 88–93 |
| File Transfer | YES (full lifecycle) | YES — sha256 match confirmed | Lines 211–219 |
| CRDT (KV) | YES | YES — cross-node CRDT sync proven | Lines 150–163 |
| CRDT (Kanban) | YES | YES — CRDT state converged to Done | Lines 166–181 |
| Named Groups | YES | YES — propagation proven including authoritative remove | Lines 127–147 |
| MLS | YES | YES — encrypt→decrypt round-trip | Lines 115–124 |
| Presence | YES | YES — studio2 found in studio1 presence (2 agents) | Lines 185–192 |

---

## Failures

### LAN-F01 — mDNS LAN discovery: studio1 did not discover studio2 (line 74)
- **Test**: `studio1 did not discover studio2 within 90s`
- **Expected**: studio1's discovery cache includes studio2 after 90s wait with no bootstrap
- **Actual**: FAIL — discovery did not complete
- **Category**: mDNS / LAN discovery
- **Root cause**: ant-quic's built-in LAN discovery (which replaced the x0x-owned mdns-sd runtime in v0.16.0) is not discovering peers on this macOS LAN within the 90s timeout. The `can_receive_direct: False` reported by both studios (lines 41, 47) may be contributing — both studios believe they cannot receive inbound direct connections, so neither initiates a connection even after mDNS announces. This is likely a macOS mDNS multicast socket issue with ant-quic's new first-party implementation.

### LAN-F02 — mDNS LAN discovery: studio2 did not discover studio1 (line 75)
- **Test**: `studio2 did not discover studio1 within 90s`
- **Category**: mDNS / LAN discovery
- **Root cause**: Same as LAN-F01. Bidirectional failure confirms it is not asymmetric network filtering.

### LAN-F03 — studio1 cannot GET discovered studio2 by ID (line 78)
- **Test**: `studio1 get discovered studio2 — curl_failed (non-2xx)`
- **Expected**: `GET /agents/discovered/:studio2_id` returns studio2's data
- **Actual**: curl failed (non-2xx) — studio2 ID not in studio1's discovery cache
- **Category**: discovery / API
- **Root cause**: Cascades from LAN-F01 — studio2 was never discovered, so its ID is not in the cache. The test attempts to GET a cache entry that does not exist.

### LAN-F04 — studio1 cannot GET reachability of studio2 (line 80)
- **Test**: `studio1 reachability of studio2 — curl_failed (non-2xx)`
- **Expected**: Reachability info returned for studio2
- **Actual**: curl failed (non-2xx)
- **Category**: discovery / reachability API
- **Root cause**: Cascades from LAN-F01.

### LAN-F05 — Seedless bootstrap: studio2-b did not discover studio1 (line 226)
- **Test**: `studio2-b did not discover studio1 within 60s`
- **Expected**: Third agent (studio2-b) running with no bootstrap seeds discovers studio1 via mDNS within 60s
- **Actual**: FAIL
- **Category**: mDNS / seedless bootstrap
- **Root cause**: Same root as LAN-F01/F02 — ant-quic's built-in mDNS does not complete discovery within 60s on macOS. The seedless bootstrap path depends entirely on mDNS working.

### LAN-F06 — Swarm: studio2-b missing studio1 (line 232)
- **Test**: `swarm: studio2-b missing studio1`
- **Expected**: 3-node mesh; studio2-b is peer of studio1
- **Actual**: `studio1 peers=1, studio2 peers=1` — studio2-b has not joined the swarm
- **Category**: gossip mesh / 3-node swarm
- **Root cause**: Cascades from LAN-F05 — studio2-b never joined the network, so it cannot be a swarm peer.

---

## Positive Highlights

The following were **proven fully on LAN** (direct physical hardware, not loopback):

- **File transfer full lifecycle**: offer → accept → complete → sha256 match → body contains proof token (lines 211–219). This is a critical contrast to the LOCAL suite where file transfer fails entirely. File transfer works when the direct channel is established via explicit card import + connect, but not from a cold mDNS start.
- **Named group authoritative remove propagation**: `studio2 authoritative removal propagated` PASS (line 144) and `studio2 space removed from group list after authoritative remove` PASS (line 147). This group propagation succeeds on LAN but fails in the LOCAL suite — confirming the LOCAL failure is a timing/gossip issue rather than a fundamental CRDT bug.
- **KV CRDT cross-node sync**: proven with exact value match across studio1→studio2 (line 162).
- **Kanban CRDT convergence**: task claimed and completed by studio2; studio1 sees converged state (lines 177–180).
- **Direct connections outcome=Coordinated**: studio1 connects to studio2 via coordination path (line 87). Both studios report `can_receive_direct: False`, so coordination is used correctly.

---

## API Coverage

| Suite contribution | Routes |
|--------------------|--------|
| LAN suite | 61/113 |
| Combined all suites | 113/113 (100%) |

---

## Proof Artifacts

| Token / Evidence | Log Line |
|------------------|----------|
| Proof token: `proof-1776274249-67163` | Lines 3, 240 |
| Studio1 agent_id: `fbf3ddcc6fae718fa2e1e809c96ba877...` | Lines 26, 31 |
| Studio2 agent_id: `1c7f260468e0884e738692f6fb731dbf...` | Lines 29, 31 |
| Version: `0.17.0` both nodes | Lines 22, 24 |
| MLS encrypt→decrypt exact match | Line 123 |
| KV CRDT sync exact match | Line 162 |
| Kanban CRDT converged: `state=done:1c7f260468e0884e...` | Line 180 |
| File transfer sha256 match | Line 218 |
| File transfer body contains proof token | Line 219 |
| Named group invite: `x0x://invite/eyJncm91cF9pZCI6ImY4ZmY5NWY...` | Lines 130–131 |
| Direct send s1→s2: `proof-1776274249-67163-direct-s1-to-s2` | Lines 89 |
| CLI agent_id == REST agent_id (exact) | Line 65 |

---

## Recommendations

**Retry recommended for LAN-F01–F06, with one code fix escalation.**

All 6 failures trace to a single root: **ant-quic's built-in mDNS LAN discovery does not work on macOS within the test timeout windows (90s / 60s).** This is a systematic code issue, not a transient.

However, note that:
- Both studios successfully connect once cards are imported manually (outcome=Coordinated). The transport, direct messaging, file transfer, CRDT sync, and group propagation all work correctly once connectivity is bootstrapped.
- The mDNS failure is isolated to the zero-bootstrap / seedless-discovery path.

**Escalate to Opus: ant-quic mDNS on macOS.**

Specific fix needed:
- Verify that ant-quic's first-party mDNS implementation correctly binds multicast sockets on macOS (interface selection, `IP_ADD_MEMBERSHIP` on the correct interface). macOS requires explicit interface selection for mDNS multicast — binding to `0.0.0.0` or `[::]` is insufficient.
- Both studios report `nat_type: Unknown` and `can_receive_direct: False` (lines 39–42, 46–48), which is unexpected on a local LAN. The NAT detection in ant-quic may be incorrectly classifying a direct LAN link as non-routable, which could suppress mDNS-initiated connection attempts.
- The seedless bootstrap test timeout of 60s may also need to be raised to 120s as a parallel mitigation while the mDNS fix is developed.
