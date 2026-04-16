# PROOF REPORT — LOCAL Suite

| Field | Value |
|-------|-------|
| Suite | LOCAL — x0x Complete API Audit (112 endpoints + CLI + GUI + SSE + WS) |
| Run ID | `suite_local_1776274197` |
| Timestamp | 2026-04-15T18:31:05Z |
| Version | `0.17.0` (log line 13: `version=0.17.0 peers=1`) |
| Proof Token | `full-audit-20260415_183105_67784` (log line 5) |
| Agent Alice | `c87da7bfa7b9b45d7ea224e58d58c7e4...` (log line 6) |
| Agent Bob | `1462e3da197dd40e75d3a1dd...` (log line 6) |

---

## Summary

| Metric | Count |
|--------|-------|
| Total checks | 277 |
| PASS | 255 |
| FAIL | 22 |
| SKIP | 0 |

**Overall verdict: PARTIAL**

---

## Interface Proof Matrix

| Interface | Send Proven | Receive Proven | Evidence (log lines) |
|-----------|-------------|----------------|----------------------|
| REST | YES | YES — all CRUD round-trips confirmed | Lines 11–382 throughout all sections |
| CLI | YES | YES — CLI agent_id == REST agent_id | Lines 276–348; line 348: exact match proof |
| GUI | YES (send) | NO — GUI-driven direct send did NOT reach bob | Lines 272–273: `GUI sends direct message via real browser` PASS; `GUI-driven direct send reached bob` FAIL |
| SSE (`/events`) | YES (200 ok) | NO — published payload not received via SSE | Lines 99–102: SSE 200 OK; line 101 FAIL: `GET /events receives published payload` |
| SSE (`/direct/events`) | YES (200 ok) | NO — alice→bob and bob→alice not received | Lines 105–113: both directions FAIL |
| SSE (`/presence/events`) | YES | YES — charlie online+offline emitted | Lines 350–356 |
| WebSocket (`/ws`) | YES | YES — pubsub round-trip and payload matched | Lines 259–265 |
| WebSocket (`/ws/direct`) | YES (send) | NO — direct_message frame not received; payload empty | Lines 266–269: receive timeout |

---

## Failures

### F01 — Version string expectation
- **Test**: `GET /health → version 0.16.x` (line 12)
- **Expected**: string contains `0.16`
- **Actual**: `0.17.0`
- **Category**: version / stale test expectation
- **Root cause**: Test hardcodes `0.16` version string; daemon is now `0.17.0`. Test script needs updating.

### F02 — SSE: gossip published payload not received
- **Test**: `GET /events receives published payload` (line 101)
- **Expected**: SSE stream delivers published gossip message
- **Actual**: message not received within timeout
- **Category**: SSE receive / gossip delivery to local SSE stream
- **Root cause**: SSE `/events` stream does not deliver self-published gossip messages in the test window, or the test's SSE consumer is timing out before delivery. Shared with WS direct receive failure — suggests a single-process loopback delivery issue.

### F03 — SSE: direct message alice→bob not received (log line 108)
- **Test**: `GET /direct/events receives verified alice→bob message`
- **Expected**: `direct/events` SSE stream delivers message with proof token
- **Actual**: Not received within timeout
- **Category**: direct messaging / SSE receive
- **Root cause**: Direct message send succeeds (line 107 PASS), but SSE delivery on `/direct/events` fails. Same underlying issue as F02 — SSE event loop not delivering within test timeout, or message delivery to SSE listeners is broken in `recv_direct_annotated()`.

### F04 — SSE: direct message bob→alice not received (line 113)
- **Test**: `GET /direct/events receives bob→alice message`
- **Expected**: SSE delivers bob→alice message
- **Actual**: Not received
- **Category**: direct messaging / SSE receive
- **Root cause**: Same as F03.

### F05 — Named group: removal not propagated to bob (line 147)
- **Test**: `named-group removal propagated to bob`
- **Expected**: Bob's view clears after alice removes him
- **Actual**: Bob's group still shows `chat_topic`, `created_at`, `creator` fields intact
- **Category**: group CRDT propagation
- **Root cause**: Removal delta not propagated to bob's daemon via gossip within test timeout, or CRDT merge does not clear membership on remote node.

### F06 — Named group (public_request_secure): alice does not see bob's pending request (line 177)
- **Test**: `alice sees bob's pending request`
- **Expected**: `want='1'`
- **Actual**: `got='0'` — request list empty
- **Category**: group join-request gossip
- **Root cause**: Bob's join request submitted (line 174–175 PASS), but gossip propagation of the request to alice's daemon did not complete within the test wait window.

### F07 — Named group (public_request_secure): approve endpoint fails (line 178)
- **Test**: `POST /groups/:id/requests/:rid/approve`
- **Expected**: HTTP 2xx
- **Actual**: `{"error":"curl_fail"}`
- **Category**: REST API / group approval
- **Root cause**: Cascades from F06 — alice does not have the request ID in state, so the approve endpoint returns an error (unknown request ID). Root is F06's propagation failure.

### F08 — Named group (public_request_secure): bob not active member after approval (line 179)
- **Test**: `bob is now active member after approval`
- **Expected**: `want='yes'`
- **Actual**: `got='no'`
- **Category**: group membership
- **Root cause**: Cascades from F07.

### F09 — Named group (ban/unban): delete convergence not propagated to bob (line 215)
- **Test**: `delete convergence: bob's view cleared`
- **Expected**: Bob's group list clears after alice deletes group
- **Actual**: Bob still sees group with `chat_topic`, `created_at`, `creator`
- **Category**: group CRDT propagation / deletion
- **Root cause**: Same pattern as F05 — group deletion delta not propagating to remote member within test timeout.

### F10–F16 — File transfer: recipient never sees incoming transfer (lines 246–256)
- **Tests**: `recipient sees pending incoming transfer` (line 246), `POST /files/accept/:id` (line 247), `sender transfer reaches Complete` (line 248), `receiver transfer reaches Complete` (line 249), `received file sha256 matches` (line 251), `received file body contains proof token` (line 252), `recipient sees second pending transfer` (line 254), `POST /files/reject/:id` (line 255), `sender sees rejected transfer` (line 256)
- **Expected**: Recipient bob's `/files/transfers` lists incoming transfer; accept/reject endpoints succeed; file completes with sha256 match
- **Actual**: `{"ok":true,"transfers":[]}` — bob sees zero transfers. All downstream file transfer assertions fail. `shasum: : No such file or directory` (line 250) confirms no file was received.
- **Category**: file transfer / direct messaging receive
- **Root cause**: File offer notification is not reaching bob. This is the same root cause as the direct messaging SSE receive failures (F03/F04) — the direct channel event does not arrive on bob's side, so the file transfer offer is never surfaced.

### F17 — WS/direct: receive frame not delivered (line 268)
- **Test**: `GET /ws/direct receives direct_message frame`
- **Expected**: WebSocket receives `direct_message` frame with proof payload
- **Actual**: `{"ok":false,"mode":"direct-receive","error":"websocket receive timeout"}`
- **Category**: WebSocket / direct messaging
- **Root cause**: Same as F03 — direct message delivery to local event consumers (whether SSE or WS) is not working within test timeout. The send path works; the receive/notify path is broken or too slow.

### F18 — WS/direct: payload empty (line 269)
- **Test**: `GET /ws/direct payload matched`
- **Category**: WebSocket / direct messaging
- **Root cause**: Cascades from F17.

### F19 — GUI: direct send did not reach bob (line 273)
- **Test**: `GUI-driven direct send reached bob`
- **Category**: GUI / direct messaging receive
- **Root cause**: Cascades from F03/F17 — bob's direct receive channel is not delivering messages regardless of send path (REST, WS, GUI).

### F20 — Swarm: reply from bob not delivered to alice (line 370)
- **Test**: `swarm reply from bob delivered to alice`
- **Expected**: Alice receives bob's gossip reply on SSE stream
- **Actual**: SSE stream delivers `message` type but with wrong payload; alice does not match expected reply
- **Category**: gossip SSE / swarm pub/sub parse
- **Root cause**: The SSE event is received (line 370 shows event data with base64 payload), but the test's payload comparison fails — likely the SSE swarm-reply message has a different structure or envelope than expected (type=`message` rather than a direct payload match). This is the "SSE swarm-reply parse issue" noted in the preliminary.

---

## API Coverage

| Metric | Value |
|--------|-------|
| Routes in x0xd.rs | 113 |
| Tested in this suite (full audit) | 102 |
| Tested in ANY suite | 113 |
| Untested | 0 |
| Coverage | **100%** |

This suite contributed coverage for 102/113 routes. Remaining 11 routes covered by other suites (comprehensive, VPS, LAN, named-groups, stress).

---

## Proof Artifacts

| Token | Log Line |
|-------|----------|
| `full-audit-20260415_183105_67784` | Line 5 (header), line 219 (task-list id), line 235 (KV round-trip), line 126 (MLS decrypt), line 243 (file transfer_id), line 114 (direct tokens) |
| Alice agent_id: `c87da7bfa7b9b45d7ea224e58d58c7e4...` | Line 6, 31 |
| Bob agent_id: `1462e3da197dd40e75d3a1dd...` | Line 6 |
| Version confirmed: `0.17.0` | Line 13, 46 |
| MLS decrypt: `proof-mls-20260415_183105_67784` | Line 126 |
| KV round-trip exact match | Line 235 |
| Task round-trip: `proof-task-20260415_183105_67784` | Line 224 |
| WS pubsub round-trip matched | Line 264–265 |
| Presence online+offline lifecycle proven | Lines 351–356 |

---

## Recommendations

**Code fix needed — escalate to Opus.**

The 22 failures fall into three categories:

**1. Stale version test (1 failure — trivial, no escalation needed):**
- F01: Update test script to expect `0.17` instead of `0.16`. One-line change in `tests/e2e_full_audit.sh`.

**2. Direct message receive / SSE delivery broken (14 failures — escalate):**
- F03, F04, F17, F18, F19, F10–F16 all trace to a single root: after a successful `POST /direct/send`, the recipient daemon does not deliver the message to its SSE (`/direct/events`) or WebSocket (`/ws/direct`) listeners within the test timeout. File transfer offer delivery (which depends on the same channel) also fails.
- Fix needed: Audit `recv_direct_annotated()` and the direct-message event fan-out in `src/bin/x0xd.rs`. Verify that the daemon is pushing received direct messages to all active SSE/WS subscribers on `/direct/events`.
- Also audit gossip pub/sub SSE delivery (F02, F20) — the `/events` SSE stream is not delivering self-published or peer-published gossip messages. Check event fan-out from `GossipRuntime` to SSE clients.

**3. Group CRDT propagation failures (5 failures — escalate):**
- F05, F06, F07, F08, F09: Group membership changes (removal, join-request submission, deletion) are not propagating from one local daemon to another within the test window. Direct messaging between alice and bob is proven (send passes), so the QUIC transport is live. The CRDT delta gossip for group state may be using a different fanout path that is not reaching the peer quickly enough, or the merge/apply logic on the receiving side is not updating the local group state.
- Fix needed: Review group CRDT delta publication in `src/groups/` — specifically the gossip topic for group state changes and the merge handler that applies incoming deltas to local group state.
