Warning: no stdin data received in 3s, proceeding without it. If piping from a slow command, redirect stdin explicitly: < /dev/null to skip, or wait longer.
## Review: Phase 1.4 — Cache Enrichment & Adaptive Detection

### Grade: B+

**Strengths:**

- Excellent documentation — module-level, per-struct, per-method, with formulas and ranges
- Good test coverage of `PeerBeaconStats` — single sample, two samples, high-jitter ceiling, steady-beacon floor
- Clamp bounds (`ADAPTIVE_TIMEOUT_FLOOR_SECS` / `CEILING_SECS`) prevent degenerate timeouts
- `saturating_sub` on absent_secs prevents underflow
- `unwrap_or(0)` on system time is the safer failure mode (suppresses false offline events)
- `#[must_use]` on pure functions is correct

**Issues to address:**

1. **Double read-lock in offline detection (consistency)** — `peer_stats` is read-locked twice with separate acquisitions (lines ~603 and ~611 in the new code). A beacon arrival between the two reads could update stats, making the timeout stale relative to `last_seen`. Not a security issue (presence is a soft signal), but it's a needless inconsistency. Combine into a single lock:

   ```rust
   let (timeout, last_seen) = {
       let g = peer_stats.read().await;
       let s = g.get(&peer);
       let timeout = s.map(|s| s.adaptive_timeout_secs(adaptive_fallback))
           .unwrap_or(adaptive_fallback);
       let last_seen = s.and_then(|s| s.last_seen.back().copied()).unwrap_or(0);
       (timeout, last_seen)
   };
   ```

2. **Unbounded `peer_stats` growth** — entries are added for every new peer but never removed. A `HashMap` with no eviction. Add cleanup when `AgentOffline` is emitted:

   ```rust
   peer_stats.write().await.remove(&peer);
   ```

3. **`last_seen` field accessed from outside the struct** — the offline detection reaches into `s.last_seen.back()` directly. Add a `pub fn last_seen(&self) -> Option<u64>` accessor to preserve encapsulation.

4. **`foaf_peer_candidates` includes offline peers** — the method returns all peers ever seen, not just currently-online ones. Consider filtering against the current online set, or documenting that callers must do so.

5. **Minor: `adaptive_timeout_fallback_secs` has no validation** — the doc says "no clamp is enforced here." A `u64::MAX` fallback would effectively disable offline detection. Consider asserting it's within bounds in `Default` or providing a builder.

**Security:** No issues. Address enrichment uses cryptographically-authenticated PeerIds. No injection vectors, no unsafe code.

**Verdict:** Ship after fixing items 1 and 2 (quick fixes). Items 3-5 are nice-to-haves.
