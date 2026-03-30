# Task Specification Review
**Date**: 2026-03-30
**Task**: Phase 1.4 - Cache Enrichment & Adaptive Detection (All 5 tasks)

## Spec Compliance

### Task 1: Bootstrap Cache Enrichment
- [x] bootstrap_cache field added to PresenceWrapper
- [x] PresenceWrapper::new() accepts optional BootstrapCache
- [x] Event loop calls bc.add_from_connection() on AgentOnline
- [x] lib.rs updated to pass bootstrap_cache.clone()
- [x] Graceful when bootstrap_cache is None

### Task 2: Per-Peer Inter-Arrival Tracking
- [x] PeerBeaconStats struct with VecDeque<u64>
- [x] INTER_ARRIVAL_WINDOW = 10
- [x] record() method caps at window size
- [x] inter_arrival_stats() returns None when < 2 samples
- [x] peer_stats: Arc<RwLock<HashMap<PeerId, PeerBeaconStats>>> added to PresenceWrapper
- [x] Event loop calls stats.record(now_secs) on new peers

### Task 3: Adaptive Timeout (Phi-Accrual Lite)
- [x] adaptive_timeout_secs(fallback) implemented
- [x] Formula: mean + 3*stddev, clamped to [180, 600]
- [x] PresenceConfig::adaptive_timeout_fallback_secs added (default 300)
- [x] Event loop uses adaptive timeout for offline detection
- [x] Fallback used when < 2 samples

### Task 4: Quality-Weighted FOAF Peer Selection
- [x] foaf_peer_score(stats) pure function implemented
- [x] Score formula: 1/(1+stddev), range [0,1]
- [x] No stats → 0.5 neutral score
- [x] foaf_peer_candidates() returns sorted Vec<(PeerId, f64)>

### Task 5: Legacy Coexistence + Integration Tests
- [x] PresenceConfig::legacy_coexistence_mode added (default true)
- [x] 11 new unit tests covering all Phase 1.4 components
- [x] All existing 668 tests continue to pass
- [x] PresenceConfig defaults backward-compatible

## Grade: A
