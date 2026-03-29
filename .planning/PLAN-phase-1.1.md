# Phase 1.1: GossipCacheAdapter Integration

## Tasks

### Task 1: Integration test file (TDD anchor)
**File**: `tests/gossip_cache_adapter_integration.rs` (new)
- test_adapter_absent_without_network_config
- test_adapter_present_with_network_config
- test_adapter_wraps_same_bootstrap_cache (insert advert, verify peer_count increases)
- test_adapter_clone_shares_state

### Task 2: Add gossip_cache_adapter field to Agent struct
**File**: `src/lib.rs`
- Add `gossip_cache_adapter: Option<GossipCacheAdapter>` field after bootstrap_cache
- Add `pub fn gossip_cache_adapter() -> Option<&GossipCacheAdapter>` accessor

### Task 3: Wire GossipCacheAdapter in AgentBuilder::build()
**File**: `src/lib.rs`
- After bootstrap_cache is created, construct GossipCacheAdapter from same Arc
- Add to Agent struct literal

### Task 4: Verify all tests pass
- cargo clippy --all-targets --all-features -- -D warnings
- cargo nextest run --all-features --workspace
- All existing tests unchanged
