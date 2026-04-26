# x0x — Continue C (DM over gossip)

## Where we are

Branch: **`claude/c-dm-over-gossip`** (pushed to origin).

Commits so far:
- `78016ba` — C phase 1: envelope + crypto + dedupe foundation (`src/dm.rs`, 1022 lines, 15 unit tests green).
- `fe618ae` — C phase 2: capability advertisement + AgentCard field (`src/dm_capability.rs`, 3 unit tests green; AgentCard additively gains `dm_capabilities: Option<DmCapabilities>`).

Total shipped in C so far: **18/18 unit tests passing**, `cargo fmt` + `cargo clippy --lib --all-features -- -D warnings` clean, builds against `ant-quic = "0.27.0"` + `saorsa-gossip-* = "0.5.17"` (both live on crates.io).

Design doc (authoritative, locked): `docs/design/dm-over-gossip.md`.

## What's left

Three phases remain to ship x0x 0.18.0:

### Phase 3 — runtime integration: capability advert service + DM inbox service

This is the heavy integration work. Two new background services need to be spawned from `Agent::join_network()`:

**`CapabilityAdvertService`** (new file suggested: `src/dm_capability_service.rs`):
- On startup: subscribe to the well-known `"x0x/caps/v1"` topic via `agent.gossip_runtime().pubsub().subscribe(...)`.
- Task 1 — **publisher**: every `ADVERT_PUBLISH_INTERVAL_SECS` (300s), build a fresh `CapabilityAdvert` (sign domain-separated bytes with the agent's ML-DSA-65 key via `ant_quic::crypto::raw_public_keys::pqc::sign_with_ml_dsa`), postcard-encode, publish on the topic.
- Task 2 — **subscriber**: consume the `Subscription` stream; for each incoming message, postcard-decode as `CapabilityAdvert`, verify signature (domain bytes), then insert into the shared `CapabilityStore`.
- `Agent` needs a new public getter: `fn capability_store(&self) -> Arc<CapabilityStore>`.

**`DmInboxService`** (new file suggested: `src/dm_inbox.rs`):
- On startup: subscribe to `format!("x0x/dm/v1/inbox/{}", hex::encode(self_agent_id))` via the existing gossip pub/sub.
- Consume incoming messages. Each is a postcard-encoded `DmEnvelope`. Apply the signature-first pipeline **exactly** as the design doc section "Signature-first rule" specifies:
  1. size check (≤ `MAX_ENVELOPE_BYTES`)
  2. postcard decode → `DmEnvelope`
  3. timestamp-window via `validate_timestamp_window()`
  4. dedupe check via `RecentDeliveryCache::lookup` — if hit, short-circuit with cached ACK, no further work
  5. ML-DSA-65 signature verify over `envelope.signed_bytes()`
  6. `recipient_agent_id` must equal self
  7. demux `envelope.body`:
     - `DmBody::Ack(a)` → `InFlightAcks::resolve(&a.acks_request_id, a.outcome)`. Done.
     - `DmBody::Payload(p)` → continue to step 8.
  8. trust-policy evaluation via `TrustEvaluator` (sender_agent_id + sender_machine_id) → `Accept/AcceptWithFlag/Unknown/RejectBlocked/RejectMachineMismatch`
  9. if blocked → cache `RejectedByPolicy{reason}`, publish ACK envelope to sender's inbox (unless `trust.silent_reject` config is true)
  10. KEM decapsulate + AEAD decrypt via `dm::decrypt_payload`. AAD is `envelope.aead_aad()`.
  11. dispatch to existing `DirectMessaging::handle_incoming(machine_id, sender_agent_id, decrypted.payload, verified=true, trust_decision)` — this is the handoff into the existing subscriber/SSE broadcast channel.
  12. insert into `RecentDeliveryCache` with outcome `Accepted`.
  13. publish `DmBody::Ack { outcome: Accepted }` envelope to sender's inbox.

Both services need a handle on `Arc<InFlightAcks>` and `Arc<RecentDeliveryCache>` shared with `DirectMessaging`.

### Phase 4 — `send_direct` rewrite

In `src/direct.rs`, add `send_direct_via_gossip()` that:
1. Lookup recipient in `CapabilityStore`.
2. If present and `gossip_inbox == true`: build envelope, publish, wait for ACK via `InFlightAcks::register(request_id) + timeout`, retry with same `request_id` on timeout. Return `DmReceipt { path: GossipInbox, retries_used }`.
3. If absent or `gossip_inbox == false`: return `DmReceipt { path: RawQuic }` after calling existing `NetworkNode::send_direct` (preserved unchanged).
4. If `DmSendConfig::require_gossip` and no capability → `DmError::RecipientKeyUnavailable`.

The public `send_direct(to, payload) -> Result<DmReceipt, DmError>` picks gossip vs raw based on capability; existing API shape for callers discarding `DmReceipt` is preserved via a thin wrapper.

In `src/bin/x0xd.rs`, the `/direct/send` REST endpoint returns `DmReceipt` (path + retries) so operators can see which transport was used.

### Phase 5 — tests + release

**Integration tests** (`tests/dm_over_gossip_integration.rs`):
1. 3-daemon A→B DM → receipt shows `GossipInbox`, `retries_used=0`.
2. Simulated first-publish loss → retry succeeds, `retries_used=1`.
3. Duplicate send (same `request_id`) → recipient delivers once, ACKs twice.
4. Blocked sender → `DmError::RecipientRejected`.
5. Pre-0.18 recipient (capability absent) → sender uses `RawQuic` path.
6. Expired envelope → dropped, no ACK, sender times out.

**e2e_full_audit.sh** adaptation: add a section asserting `DmReceipt::path == GossipInbox` for the standard DM probes.

**Release**:
- Bump `Cargo.toml` version → `0.18.0`, SKILL.md to match.
- `chore(release): 0.18.0` commit, tag `v0.18.0`, push.
- CI runs ~20 min, publishes to crates.io + GH releases.

## Quick-start for next session

```bash
cd /Users/davidirvine/Desktop/Devel/projects/x0x
git fetch origin
git checkout claude/c-dm-over-gossip
git pull
cargo nextest run --lib -E 'test(dm::) or test(dm_capability::)'   # should be 18/18
cat docs/design/dm-over-gossip.md   # authoritative spec
```

Then implement in order: `dm_capability_service.rs` → `dm_inbox.rs` → integrate into `Agent::join_network()` and `DirectMessaging` → `send_direct` rewrite → daemon `/direct/send` shape → integration tests → release.

## Open design decisions (from design doc, all deferred to next session)

1. **`trust.silent_reject` default** — design doc defaults to `false` (emit ACK on policy-reject so senders know why). VPS/privacy-sensitive may want `true`.
2. **Mixed-version fallback** — design doc says "no dual-send". The capability-driven path selection in phase 4 is the single source of truth.
3. **Cleanup of stale `InFlightAcks`** — if a sender abandons mid-retry, the registry entry is orphaned. Simple TTL sweep task (every 60s) recommended.

## Context from the session that shipped 0.27.0 / 0.5.17

- **ant-quic 0.27.0** landed D (connection lifecycle sync) per `docs/design/connection-lifecycle-sync.md` in that repo. Released by David's team.
- **saorsa-gossip 0.5.17** is just a dep bump (ant-quic 0.26.13 → 0.27.0). 416/416 tests.
- The VPS matrix NYC→Tokyo DM issue that launched this whole C work is still unresolved on the raw-QUIC path; C is the architectural fix. Once phase 4 lands, VPS nodes will advertise gossip capability, send each other via the inbox path, and the 30/30 matrix should go green with ACK-confirmed receipts.

## Acceptance bar for x0x 0.18.0

- 6 integration tests pass.
- `e2e_full_audit.sh` passes with DM probes using gossip path.
- `cargo fmt / clippy / nextest` all clean.
- VPS deploy on the 6-node mesh — every DM uses `DmPath::GossipInbox`, retries_used ≤ 1 at p95.
- Raw-QUIC direct path still functional as fallback (do not break existing `NetworkNode::send_direct`).

Good luck.
