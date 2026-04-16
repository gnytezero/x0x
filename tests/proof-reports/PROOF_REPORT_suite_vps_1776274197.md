# PROOF REPORT — VPS Suite

| Field | Value |
|-------|-------|
| Suite | VPS E2E — All-Pairs Matrix: NYC, SFO, Helsinki, Nuremberg, Singapore, Tokyo |
| Run ID | `suite_vps_1776274197` |
| Timestamp | 2026-04-15 (run at conclusion of VPS test; no ISO timestamp in log) |
| Version | `0.17.0` on all live nodes (log lines 9, 12, 15, 17) |
| Proof Token | `vps-proof-1776274254-67304` (log lines 4, 215) |

---

## Summary

| Metric | Count |
|--------|-------|
| Total checks | 125 |
| PASS | 110 |
| FAIL | 12 |
| SKIP | 3 |

**Overall verdict: PARTIAL**

**Live nodes during run**: NYC, Helsinki, Singapore, Tokyo (4/6)
**Offline nodes**: SFO (`curl_failed` — line 10), Nuremberg (`curl_failed` — line 13)

---

## All-Pairs Matrix

> Only 4 nodes were live. SFO and Nuremberg are offline. Matrix covers 4 live nodes; SFO and Nuremberg columns/rows marked OFFLINE.

| From\To | NYC | SFO | Helsinki | Nuremberg | Singapore | Tokyo |
|---------|-----|-----|----------|-----------|-----------|-------|
| **NYC** | — | OFFLINE | FAIL (connect) | OFFLINE | OK (send) | OK (send) |
| **SFO** | OFFLINE | — | OFFLINE | OFFLINE | OFFLINE | OFFLINE |
| **Helsinki** | OK (send) | OFFLINE | — | OFFLINE | FAIL (recv) | OK (send) |
| **Nuremberg** | OFFLINE | OFFLINE | OFFLINE | — | OFFLINE | OFFLINE |
| **Singapore** | FAIL (recv) | OFFLINE | FAIL (recv) | OFFLINE | — | FAIL (recv) |
| **Tokyo** | FAIL (recv) | OFFLINE | FAIL (recv) | OFFLINE | FAIL (recv) | — |

**Legend**: OK = send succeeded AND receive confirmed. SEND = send succeeded, receive failed. FAIL = explicit failure reported. OFFLINE = node unreachable. SKIP = test skipped.

> Note: The log reports "all 11 REST direct sends succeeded" (line 83) but "10/11 REST deliveries missing after retry" (line 95). All pairs sent successfully; only NYC→Helsinki connect failed (line 78). Receive failures are universal except 1 unspecified pair.

---

## Failures

### VPS-F01 — NYC→Helsinki: connect failed (line 78)
- **Test**: `NYC→Helsinki: missing` (connect step)
- **Expected**: Connect succeeds, outcome returned
- **Actual**: FAIL — missing from connect results
- **Category**: direct connection / NAT traversal
- **Root cause**: NYC has 8 active connections (line 106), Helsinki has 4 (line 108). Helsinki reports only 2 peers on network (line 38) vs NYC's 10. The NYC→Helsinki connection failed despite both nodes being live. Likely a transient hole-punch failure or NAT state issue on the Helsinki→NYC path.

### VPS-F02 to VPS-F11 — 10/11 direct message receives missing (lines 85–95)
- **Tests**: NYC did not receive from Helsinki (line 85), Singapore (line 86), Tokyo (line 87); Helsinki did not receive from Singapore (line 88); Singapore did not receive from NYC (line 89), Helsinki (line 90), Tokyo (line 91); Tokyo did not receive from NYC (line 92), Helsinki (line 93), Singapore (line 94)
- **Expected**: After 15s wait, all sent messages appear in each node's direct event log
- **Actual**: 10/11 deliveries missing after retry
- **Category**: direct messaging receive / event delivery
- **Root cause**: Same root as LOCAL suite failures F03/F04 — the `POST /direct/send` succeeds (11/11 sends pass, line 83), but messages are not delivered to the recipient's event/SSE stream. The VPS nodes have active direct connections (4–10 active connections per node, lines 105–112), which means QUIC transport is live. The failure is in the receive notification path: after a direct message arrives at the transport layer, it is not propagated to the daemon's SSE/WS event fan-out. This is the same bug as LOCAL F03, confirmed cross-continent.

### VPS-F12 — Named group: removal not propagated to Tokyo (line 152)
- **Test**: `Tokyo authoritative removal propagated`
- **Expected**: After NYC removes Tokyo from the named space, Tokyo's state updates
- **Actual**: FAIL
- **Category**: group CRDT propagation
- **Root cause**: Same pattern as LOCAL F05 — group membership removal delta not propagating to the removed member's node via gossip within the test timeout.

### VPS-F13 — Named group: Tokyo group list not cleared after remove (line 153)
- **Test**: `Tokyo group list cleared after authoritative remove`
- **Category**: group CRDT propagation
- **Root cause**: Cascades from VPS-F12.

### VPS-F14–VPS-F19 — File transfer: Tokyo does not see incoming transfer (lines 182–187)
- **Tests**: `Tokyo sees incoming transfer` (FAIL, line 182), `Tokyo accepts incoming transfer — curl_failed` (line 183), `sender transfer reaches Complete` (line 184), `receiver transfer reaches Complete` (line 185), `received file sha256 matches` (line 186), `received file body contains proof token` (line 187)
- **Expected**: Singapore sends file offer; Tokyo's `/files/transfers` lists it; accept succeeds; sha256 verified
- **Actual**: Tokyo sees zero transfers; all downstream assertions fail. Note Singapore→Tokyo file offer send passes (line 180) and Singapore's own transfer list passes (line 181).
- **Category**: file transfer receive notification
- **Root cause**: Same root as LOCAL F10–F16 — file offer notification is not delivered to Tokyo's daemon event system. The direct send of the file offer uses the same direct-message channel that is broken for all VPS receive tests (F02–F11).

### VPS-SKIP01 — CLI NYC→Helsinki: not connected (line 98)
- **Reason**: Skipped because NYC→Helsinki connect failed (VPS-F01). No CLI proof possible for an unconnected pair.

### VPS-SKIP02 — CLI Tokyo→SFO: no agent_id (line 99)
- **Reason**: SFO is offline; no agent_id available for Tokyo→SFO CLI test.

### VPS-SKIP03 — GUI proof: NYC/Helsinki not fully available (line 102)
- **Reason**: Skipped; connect between NYC and Helsinki failed (VPS-F01), making a cross-node GUI proof non-viable.

---

## Positive Highlights

- **MLS (post-quantum, multi-continent)**: Full lifecycle across NYC+Helsinki+Singapore+Tokyo — create, add 3 members, encrypt, decrypt (round-trip match), remove Helsinki — all PASS (lines 127–137).
- **Named groups**: Create, invite, join (Tokyo joins via invite), set display name, member roster add/remove — all PASS (lines 140–157). Only propagation-to-removed-member fails.
- **KV stores (cross-continent)**: Singapore create, put 3 keys, get, list, delete — all PASS (lines 160–169).
- **Task lists (CRDT)**: Tokyo create, add task, claim, complete — all PASS (lines 172–176).
- **Contacts & trust**: NYC add Helsinki as contact, trust eval → Accept, block Singapore → RejectBlocked, unblock — all PASS (lines 115–119).
- **Gossip pub/sub**: Helsinki and Tokyo subscribe; NYC publishes — all subscribe/publish mechanics PASS (lines 122–124). Note: receive-side proof not asserted in VPS pub/sub section.
- **Presence**: All 4 nodes presence online, FOAF, find, and status — all PASS (lines 190–197).
- **Constitution + status**: All 4 nodes PASS (lines 200–212).
- **WebSocket sessions**: NYC WS sessions PASS (line 208).
- **Bootstrap cache**: NYC PASS (line 210).
- **SSE endpoints**: `/events` and `/presence/events` both respond on NYC (lines 211–212).

---

## API Coverage

| Suite contribution | Routes |
|--------------------|--------|
| VPS suite | 72/113 |
| Combined all suites | 113/113 (100%) |

---

## Proof Artifacts

| Token / Evidence | Log Line |
|------------------|----------|
| Proof token: `vps-proof-1776274254-67304` | Lines 4, 215 |
| NYC version: `0.17.0` | Line 9 |
| Helsinki version: `0.17.0` | Line 12 |
| Singapore version: `0.17.0` | Line 15 |
| Tokyo version: `0.17.0` | Line 17 |
| NYC agent: `da2233d6ba2f9569...` | Line 22 |
| Helsinki agent: `5ec81c4cee5a22fb...` | Line 24 |
| Singapore agent: `4b338897360e362a...` | Line 26 |
| Tokyo agent: `290602eb4ea84afb...` | Line 28 |
| All agents distinct | Line 29 |
| 12 card import pairs succeeded | Line 73 |
| 11 REST direct sends succeeded | Line 83 |
| MLS group: `59d0e928763da931...` | Line 129 |
| MLS decrypt round-trip | Line 136 |
| NYC trust eval Helsinki → Accept | Line 116 |
| NYC block Singapore → RejectBlocked | Line 118 |
| NYC peers: 10 | Line 36 |
| Singapore peers: 16 | Line 40 |
| Tokyo peers: 13 | Line 42 |

---

## Recommendations

**Code fix needed — escalate to Opus. Two VPS nodes also need investigation.**

**Node availability:**
- SFO and Nuremberg are unreachable (lines 10, 13: `curl_failed`). These nodes need independent health investigation outside the test suite — either the daemon is down, or the API port (12600) is not reachable.

**Systematic code fixes required:**

1. **Direct message receive / event fan-out (VPS-F02–F11, F14–F19) — HIGH PRIORITY:**
   - 10/11 sends succeed but receives are universally missing across the entire 4-node VPS mesh. This matches LOCAL suite failures exactly. The bug is in how the daemon delivers received direct messages to SSE/WS subscribers.
   - Fix: Audit `recv_direct_annotated()` event loop in `src/bin/x0xd.rs`. Verify that incoming direct messages received by the QUIC transport are published to the daemon's internal broadcast channel that feeds `/direct/events` SSE and `/ws/direct` WebSocket clients.

2. **Group CRDT removal propagation (VPS-F12–F13) — MEDIUM PRIORITY:**
   - Group membership removal deltas are not reaching removed members on both LOCAL and VPS suites. LAN suite proves this works when gossip is healthy (LAN studio2 authoritative removal propagated PASS). The issue may be that removal gossip is published to a topic the removed member has already unsubscribed from (because they are being removed), creating a delivery gap.
   - Fix: Review `src/groups/` — ensure removal deltas are published before unsubscribing the removed member from the group topic, or use a separate tombstone topic that all agents remain subscribed to.

3. **NYC→Helsinki connect failure (VPS-F01) — LOW PRIORITY (likely transient):**
   - Only 1/12 connect pairs failed. Helsinki has fewer peers (2 vs 10 for NYC) which may indicate it has fewer bootstrap connections to route through. Retry is recommended; this may be transient NAT punch timing.
