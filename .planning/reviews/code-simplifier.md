# Code Simplification Review
**Date**: 2026-03-30
**Mode**: gsd-task (Phase 1.4)

## Scope
src/presence.rs - Phase 1.4 additions

## Findings
- [LOW] src/presence.rs start_event_loop: two separate RwLock reads for peer_stats in the offline detection branch (one for timeout, one for last_seen). Could be merged into a single guard:
  ```rust
  // Current: two reads
  let timeout = { peer_stats.read().await.get(&peer).map(|s| s.adaptive_timeout_secs(fallback)).unwrap_or(fallback) };
  let last_seen = { peer_stats.read().await.get(&peer).and_then(|s| s.last_seen.back().copied()).unwrap_or(0) };
  // Could be: one read
  let (timeout, last_seen) = {
      let g = peer_stats.read().await;
      let s = g.get(&peer);
      (s.map(|s| s.adaptive_timeout_secs(fallback)).unwrap_or(fallback), s.and_then(|s| s.last_seen.back().copied()).unwrap_or(0))
  };
  ```
  Minor optimization, not a blocker.
- [OK] PeerBeaconStats methods are appropriately small
- [OK] foaf_peer_score is clean and readable
- [OK] Constants well-named, no magic numbers

## Grade: A-
