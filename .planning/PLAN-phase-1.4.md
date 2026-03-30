# Phase 1.4: Cache Enrichment & Adaptive Detection

**Phase**: 1.4
**Name**: Cache Enrichment & Adaptive Detection
**Status**: Planning Complete
**Created**: 2026-03-30
**Estimated Tasks**: 5

---

## Overview

Build the feedback loop from presence beacons back into the bootstrap cache, and replace the fixed 900-second TTL with an adaptive Phi-Accrual-lite failure detector. This gives quality-weighted FOAF routing and eliminates the 15-min offline blind spot.

**Key Technologies:**
- `bootstrap_cache.add_from_connection()` — existing ant-quic method
- `parse_addr_hints()` — already in `src/presence.rs`
- Per-peer inter-arrival window (VecDeque of unix timestamps)
- Mean + 3×stddev formula, floor 180 s, ceiling 600 s
- `network.node_status()` for NAT/relay/coordinator flags

---

## Task Breakdown

### Task 1: Bootstrap Cache Enrichment from Presence Beacons

**File**: `src/presence.rs`

When the presence event loop receives a beacon from a peer that has address hints,
call `bootstrap_cache.add_from_connection()` so those addresses are available for
future NAT traversal even before the peer is directly connected.

**Implementation**:
- Add `bootstrap_cache: Option<Arc<ant_quic::BootstrapCache>>` field to `PresenceWrapper`.
- Update `PresenceWrapper::new()` to accept an optional `BootstrapCache`.
- In `start_event_loop()`, on each `AgentOnline` event, if we have addresses and a
  `bootstrap_cache`, call `bc.add_from_connection(peer_id, addrs, None)`.
- Update `lib.rs` to pass `bootstrap_cache` when constructing `PresenceWrapper`.

**Requirements**:
- No `.unwrap()` / `.expect()` in production paths
- If `bootstrap_cache` is `None`, skip enrichment silently
- Existing tests must still pass
- Add doc comment to new field and updated constructor

**Files**:
- `src/presence.rs` — `PresenceWrapper` struct + `new()` + event loop
- `src/lib.rs` — constructor call site (~line 3180 bootstrap_cache field)

---

### Task 2: Per-Peer Inter-Arrival Tracking

**File**: `src/presence.rs`

Track the last N beacon inter-arrival intervals per peer so the adaptive detector
in Task 3 has data to work with.

**Implementation**:
```rust
/// Sliding window of the last N beacon inter-arrival durations (seconds) for a peer.
const INTER_ARRIVAL_WINDOW: usize = 10;

struct PeerBeaconStats {
    /// Wall-clock timestamps (unix secs) of the last INTER_ARRIVAL_WINDOW arrivals.
    last_seen: VecDeque<u64>,
}

impl PeerBeaconStats {
    fn record(&mut self, now_secs: u64);
    /// Returns (mean, stddev) of inter-arrival intervals, or None if < 2 samples.
    fn inter_arrival_stats(&self) -> Option<(f64, f64)>;
}
```

- Add `peer_stats: Arc<RwLock<HashMap<PeerId, PeerBeaconStats>>>` to `PresenceWrapper`.
- Update the event loop to call `stats.record(now_secs)` whenever a peer beacon arrives.

**Requirements**:
- `VecDeque` capped at `INTER_ARRIVAL_WINDOW`
- `inter_arrival_stats()` returns `None` when fewer than 2 observations
- Unit tests for `record()` and `inter_arrival_stats()`

**Files**:
- `src/presence.rs` — new `PeerBeaconStats` type + `PresenceWrapper` field

---

### Task 3: Adaptive Timeout (Phi-Accrual Lite)

**File**: `src/presence.rs`

Replace the hard-coded `expires` TTL with an adaptive timeout per peer:

```
adaptive_timeout = clamp(mean + 3 * stddev, 180, 600)  // seconds
```

When fewer than 2 observations: fall back to `config.beacon_interval_secs * 3` (or 300 s).

**Implementation**:
```rust
impl PeerBeaconStats {
    /// Compute the adaptive offline timeout for this peer (seconds).
    fn adaptive_timeout_secs(&self, fallback_secs: u64) -> u64 {
        match self.inter_arrival_stats() {
            Some((mean, stddev)) => {
                let raw = mean + 3.0 * stddev;
                raw.clamp(180.0, 600.0) as u64
            }
            None => fallback_secs,
        }
    }
}
```

- Add `PresenceConfig::adaptive_timeout_fallback_secs` (default: 300).
- In the event loop, use `adaptive_timeout_secs()` when computing whether a peer is
  considered offline (time since last beacon > adaptive_timeout).
- The legacy `PresenceRecord::expires` field is still used as a secondary guard.

**Requirements**:
- No floating-point panics (stddev formula handles edge cases)
- Unit tests: single-sample fallback, steady beacon, high-jitter beacon
- `PresenceConfig::adaptive_timeout_fallback_secs` has doc comment

**Files**:
- `src/presence.rs` — `PeerBeaconStats::adaptive_timeout_secs()` + `PresenceConfig` field

---

### Task 4: Quality-Weighted FOAF Peer Selection

**File**: `src/presence.rs`

When `PresenceWrapper` selects peers to forward FOAF random-walk queries to,
prefer peers with more stable beacon intervals (lower stddev = higher quality).

**Implementation**:
```rust
/// Score a peer for FOAF forwarding priority.
/// Higher score = more stable = preferred.
/// Score = 1.0 / (1.0 + stddev)  (0..1 range)
fn foaf_peer_score(stats: &PeerBeaconStats) -> f64 {
    match stats.inter_arrival_stats() {
        Some((_, stddev)) => 1.0 / (1.0 + stddev),
        None => 0.5, // Unknown stability: neutral score
    }
}
```

- Add `Agent::foaf_peer_candidates()` → `Vec<(PeerId, f64)>` (peer + score) as a
  pub(crate) helper used by the discover_agents_foaf() path.
- Sort candidates descending by score before forwarding.

**Requirements**:
- Score always in [0, 1] range
- Peers with no stats get score 0.5 (neutral)
- Unit tests: empty stats, one sample, multiple samples
- No change to public API surface

**Files**:
- `src/presence.rs` — `foaf_peer_score()` fn + `foaf_peer_candidates()` on `PresenceWrapper`

---

### Task 5: Legacy Coexistence + Integration Tests

**File**: `src/presence.rs`, `tests/presence_foaf_integration.rs`

Ensure both the existing heartbeat (identity announcements) and the new adaptive
detection run simultaneously without conflict.

**Implementation**:
- Add `PresenceConfig::legacy_coexistence_mode: bool` (default: true).
  When true, the old 300-second identity heartbeat continues unchanged.
- When false (future deprecation path, not yet default), identity heartbeat stops.
- Write unit tests for the new `PeerBeaconStats` + adaptive timeout logic.
- Verify the existing integration tests in `tests/presence_foaf_integration.rs` still pass.

**Tests to add in `src/presence.rs` `#[cfg(test)]` block**:
1. `test_peer_beacon_stats_single_sample` — adaptive_timeout returns fallback
2. `test_peer_beacon_stats_two_samples` — stats computed, timeout clamped
3. `test_peer_beacon_stats_high_jitter` — ceiling clamp at 600
4. `test_peer_beacon_stats_steady` — floor behaviour
5. `test_foaf_peer_score_no_stats` — returns 0.5
6. `test_foaf_peer_score_stable` — score close to 1.0
7. `test_presence_config_adaptive_fallback_default` — 300 s

**Requirements**:
- Zero warnings, zero `.unwrap()` in production
- All existing tests continue to pass
- `PresenceConfig` default stays backward-compatible

**Files**:
- `src/presence.rs` — `PresenceConfig` new field + unit tests
- `tests/presence_foaf_integration.rs` — verify no regressions

---

## Module Structure

All changes go into the existing `src/presence.rs`. No new files are created.

```
src/presence.rs  (modified)
  ├── PeerBeaconStats (new private type)
  ├── PresenceConfig (add 2 fields)
  └── PresenceWrapper (add 2 fields: bootstrap_cache + peer_stats)
```

---

## Success Criteria

- [ ] Presence beacons enrich bootstrap cache (Task 1)
- [ ] Per-peer inter-arrival window tracked (Task 2)
- [ ] Adaptive timeout replaces fixed 900 s (Task 3)
- [ ] FOAF candidates sorted by quality score (Task 4)
- [ ] ≥7 new unit tests covering adaptive detection (Task 5)
- [ ] All existing tests pass (cargo nextest run)
- [ ] Zero warnings, zero `.unwrap()` in production paths
- [ ] Full doc coverage on new public items

---

**Plan Created**: 2026-03-30
**Total Tasks**: 5
**Estimated Completion**: Phase 1.4 complete after all tasks pass review
