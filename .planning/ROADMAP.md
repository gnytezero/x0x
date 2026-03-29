# Seedless Bootstrap & Gossip-Based NAT Coordination

## Problem

x0x hardcodes 6 VPS bootstrap IPs. If they go down, no new node can join. The `saorsa-gossip-coordinator` crate already implements SPEC2 coordinator adverts, FOAF discovery, and gossip cache integration — but x0x doesn't use any of it. The pieces exist, they're just not wired.

## Goal

Production-ready seedless bootstrap where new nodes can join the network without any hardcoded addresses, using gossip-based coordinator discovery and FOAF (friend-of-a-friend) propagation.

---

## Milestone 1: Wire Coordinator Into x0x

**Objective**: Connect the existing `saorsa-gossip-coordinator` crate into x0x's gossip runtime. Coordinator adverts flow, handlers process them, cache is enriched.

### Phase 1.1: GossipCacheAdapter Integration
- Wire `GossipCacheAdapter` to x0x's existing `Arc<BootstrapCache>`
- Store adapter in `Agent` struct
- Zero data duplication — wraps the same Arc

### Phase 1.2: Coordinator Advert Publishing
- When `node_status().is_coordinating == true`, use `CoordinatorPublisher` to create ML-DSA-65 signed adverts
- Publish to `coordinator_topic()` via existing pubsub
- Periodic re-publication (configurable interval, default 5 min)

### Phase 1.3: Coordinator Advert Subscription & Handling
- Subscribe to `coordinator_topic()` in `join_network()`
- Spawn handler task: deserialize CBOR adverts, call `CoordinatorHandler::handle_advert()`
- Verified adverts enrich the `GossipCacheAdapter` (and thus `BootstrapCache`)
- Message type discriminant byte for advert vs query vs response

---

## Milestone 2: FOAF Discovery & Seedless Bootstrap

**Objective**: New nodes can discover coordinators via friend-of-a-friend gossip queries, then bootstrap without any hardcoded addresses.

### Phase 2.1: FOAF Query/Response Routing
- Handle `FindCoordinatorQuery` messages on the coordinator topic
- Call `CoordinatorHandler::handle_find_query()` to produce responses
- Route `FindCoordinatorResponse` back to originator
- TTL=3 propagation with deduplication (30s query expiry)

### Phase 2.2: Seedless Bootstrap Path
- Integrate `Bootstrap::find_coordinator()` into `join_network()`
- Flow: warm cache -> connect cached coordinator -> FOAF query -> wait -> connect
- `BootstrapAction::Connect` -> `network.connect_addr()`
- `BootstrapAction::SendQuery` -> publish FOAF query, await response
- Hardcoded peers become fallback, not primary
- `AgentBuilder` default includes `NetworkConfig` (fix current None default)

### Phase 2.3: NAT Coordination via Discovered Coordinators
- When `needs_coordination()` is true, use discovered coordinators for hole-punch setup
- Connect to coordinator, request rendezvous for target agent
- Coordinator relays timing/address info for simultaneous hole-punch
- Falls back to direct connection attempts if no coordinator available

---

## Milestone 3: E2E Verification & Hardening

**Objective**: Prove seedless bootstrap and FOAF discovery work across the live VPS network. Make hardcoded peers truly optional.

### Phase 3.1: VPS Deployment & Integration Testing
- Deploy coordinator-enabled x0xd to all 6 VPS nodes
- Verify coordinator adverts propagate across the mesh
- Test FOAF discovery: new node joins with empty bootstrap list
- Test NAT coordination: node behind NAT joins via coordinator
- Measure: time-to-first-peer, advert propagation latency, FOAF success rate

### Phase 3.2: Hardening & Edge Cases
- Bootstrap with zero known peers (fully cold start)
- Bootstrap when some coordinators are unreachable
- Coordinator advert expiry and cache pruning
- Race conditions: simultaneous FOAF queries from multiple new nodes
- Replay protection for coordinator adverts (timestamp + signature)
- `NetworkConfig` option to disable hardcoded peers entirely

### Phase 3.3: Documentation & API Surface
- Update README with seedless bootstrap docs
- Update CLAUDE.md architecture section
- CLI: `x0x network coordinators` — list known coordinators
- GUI: show coordinator status in Network view
- API: `GET /network/coordinators` endpoint

---

## Architecture Notes

### Coordinator Discovery Flow (Seedless)

```
New Node (no peers)
  |
  +-- 1. Check BootstrapCache for cached coordinators
  |     +-- Hit? -> Connect directly
  |
  +-- 2. No cache? Send FOAF query (if any peer known from mDNS/local)
  |     +-- FindCoordinatorQuery { ttl: 3, expiry: 30s }
  |     +-- Propagates friend-of-a-friend, 3 hops deep
  |     +-- Response: list of CoordinatorAdverts
  |
  +-- 3. Still no peers? Fall back to hardcoded bootstrap
  |     +-- DEFAULT_BOOTSTRAP_PEERS (6 VPS nodes)
  |
  +-- 4. Connected -> join HyParView -> receive coordinator adverts via gossip
```

### Key Types (from saorsa-gossip-coordinator)

- `GossipCacheAdapter`: wraps `Arc<BootstrapCache>`, adds coordinator metadata
- `CoordinatorAdvert`: CBOR-encoded, ML-DSA-65 signed, has roles/addr_hints/nat_class/score
- `CoordinatorPublisher`: creates signed adverts from machine keypair
- `CoordinatorHandler`: verifies adverts, handles FOAF queries
- `Bootstrap`: implements find_coordinator() flow with BootstrapAction enum
- `FindCoordinatorQuery`: TTL=3, 30s expiry, random query_id
- `FindCoordinatorResponse`: list of verified coordinator adverts
- `coordinator_topic()`: BLAKE3("saorsa-coordinator-topic") well-known TopicId

### Compatibility

- `GossipCacheAdapter::new()` takes `Arc<ant_quic::BootstrapCache>` — x0x already holds one
- PeerId bytes identical between ant-quic and saorsa-gossip (32-byte SHA-256 of ML-DSA-65 pubkey)
- Adverts signed with ML-DSA-65 — same key type as x0x machine keys
- `coordinator_topic()` is a well-known TopicId subscribable via existing pubsub
