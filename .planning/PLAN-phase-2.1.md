# Phase 2.1 Plan: Connectivity Module

## Overview

Create `src/connectivity.rs` with `ReachabilityInfo` and
`connect_to_agent()` on `Agent`. This implements the connectivity
strategy: try direct → coordinated NAT traversal → relay, for 100%
connectivity as required by the milestone success criteria.

Also add `status()` and `connect_addr()` to `NetworkNode`, and enrich
the bootstrap cache from identity announcements.

## Files

- `src/connectivity.rs` (new) — `ReachabilityInfo`, connectivity types
- `src/lib.rs` — `Agent::connect_to_agent()`, enriched bootstrap cache
- `src/network.rs` — `status()` accessor, `connect_addr()` already exists

---

## Tasks

### Task 1: Define ReachabilityInfo and connectivity types

**Files**: `src/connectivity.rs` (new)

Create types that summarize how reachable an agent is:

```rust
/// Summarises the connectivity properties of a discovered agent.
#[derive(Debug, Clone)]
pub struct ReachabilityInfo {
    /// Agent's known addresses.
    pub addresses: Vec<std::net::SocketAddr>,
    /// NAT type reported by the agent.
    pub nat_type: Option<String>,
    /// Whether the agent can receive direct inbound connections.
    pub can_receive_direct: Option<bool>,
    /// Whether the agent is acting as a relay.
    pub is_relay: Option<bool>,
    /// Whether the agent is coordinating NAT traversal.
    pub is_coordinator: Option<bool>,
}

impl ReachabilityInfo {
    /// Build from a DiscoveredAgent.
    pub fn from_discovered(agent: &crate::DiscoveredAgent) -> Self;
    /// Returns true if a direct connection attempt is likely to succeed.
    pub fn likely_direct(&self) -> bool;
    /// Returns true if coordinated NAT traversal may be needed.
    pub fn needs_coordination(&self) -> bool;
}

/// Outcome of a connect_to_agent() attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectOutcome {
    /// Connected directly without NAT traversal.
    Direct(std::net::SocketAddr),
    /// Connected via coordinated hole-punch.
    Coordinated(std::net::SocketAddr),
    /// Agent not reachable with available information.
    Unreachable,
    /// Agent not found in discovery cache.
    NotFound,
}
```

**Estimated Lines**: ~90

---

### Task 2: Add Agent::connect_to_agent()

**Files**: `src/lib.rs`

Add a method on `Agent` that looks up a discovered agent and attempts to
connect to it:

```rust
pub async fn connect_to_agent(
    &self,
    agent_id: &identity::AgentId,
) -> error::Result<connectivity::ConnectOutcome>;
```

Logic:
1. Look up `agent_id` in `identity_discovery_cache`
2. If not found → return `ConnectOutcome::NotFound`
3. Build `ReachabilityInfo` from discovered agent
4. If `likely_direct()` → try `network.connect_addr()` for each address
5. If direct connect fails or `needs_coordination()` → return `Coordinated` or `Unreachable`
6. On success → return `ConnectOutcome::Direct(addr)`

**Estimated Lines**: ~60

---

### Task 3: Bootstrap cache enrichment from announcements

**Files**: `src/lib.rs`

The identity listener already adds addresses from announcements to the
bootstrap cache when `machine_id == ant-quic PeerId`. Verify this is
correct and also update a `last_seen` in the bootstrap cache using the
`announced_at` timestamp for quality scoring.

Also add addresses to the bootstrap cache from `connect_to_agent()`
when a successful connection is made.

**Estimated Lines**: ~20

---

### Task 4: Expose ReachabilityInfo via Agent::reachability()

**Files**: `src/lib.rs`

```rust
pub async fn reachability(
    &self,
    agent_id: &identity::AgentId,
) -> Option<connectivity::ReachabilityInfo>;
```

Returns `None` if the agent is not in the discovery cache.

**Estimated Lines**: ~15

---

### Task 5: Tests for connectivity module

**Files**: `src/connectivity.rs`

Unit tests:
- `likely_direct()` returns true for agents with `can_receive_direct: Some(true)`
- `needs_coordination()` returns true for agents with symmetric NAT
- `from_discovered()` correctly copies all fields
- `ConnectOutcome` equality

**Estimated Lines**: ~60

---

## Summary

| Task | File(s) | Lines | Status |
|------|---------|-------|--------|
| 1 | connectivity.rs (new) | ~90 | TODO |
| 2 | lib.rs | ~60 | TODO |
| 3 | lib.rs | ~20 | TODO |
| 4 | lib.rs | ~15 | TODO |
| 5 | connectivity.rs | ~60 | TODO |

**Total Estimated Lines**: ~245
