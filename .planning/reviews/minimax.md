Warning: no stdin data received in 3s, proceeding without it. If piping from a slow command, redirect stdin explicitly: < /dev/null to skip, or wait longer.
Looking at this diff for Phase 1.4 (Cache Enrichment & Adaptive Detection), I'll analyze security, correctness, and quality.

## Analysis

### Security Issues

**1. Bootstrap Cache Enrichment from Untrusted Sources (Medium Severity)**

```rust
if let Some(ref bc) = bootstrap_cache {
    if !socket_addrs.is_empty() {
        let ant_peer_id = ant_quic::PeerId(*peer.as_bytes());
        bc.add_from_connection(ant_peer_id, socket_addrs.clone(), None).await;
    }
}
```

Addresses from presence beacons (which come from the network) are fed into the bootstrap cache without validation. An attacker could advertise spoofed addresses to pollute bootstrap state, potentially enabling eclipse attacks. The code trusts `socket_addrs` from the identity discovery cache without verification.

**2. Floating Point in Timing/Critical Path**

```rust
let raw = mean + 3.0 * stddev;
raw.clamp(ADAPTIVE_TIMEOUT_FLOOR_SECS, ADAPTIVE_TIMEOUT_CEILING_SECS) as u64
```

Using `f64` for timeout calculations introduces potential precision edge cases. While unlikely to cause issues in practice, timeout logic should ideally use integer arithmetic or fixed-point math.

### Correctness Issues

**3. Unused Configuration Field**

```rust
pub legacy_coexistence_mode: bool,
```

Set in config but never used in the visible diff. Dead code or incomplete implementation.

**4. No Validation of Config Bounds**

```rust
/// Defaults to 300 s (5 minutes). Must be within
/// `[ADAPTIVE_TIMEOUT_FLOOR_SECS, ADAPTIVE_TIMEOUT_CEILING_SECS]` for
/// consistent behaviour, though no clamp is enforced here.
pub adaptive_timeout_fallback_secs: u64,
```

The documentation explicitly states values should be clamped but no enforcement exists.

**5. Sorting with `partial_cmp`**

```rust
candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
```

While the formula guarantees no NaN, using `unwrap_or` here masks a potential latent bug if the formula ever changes.

### Quality Issues

**6. `last_seen: VecDeque<u64>` - Unbounded While Loop**

```rust
while self.last_seen.len() > INTER_ARRIVAL_WINDOW {
    self.last_seen.pop_front();
}
```

This is correct (bounded to 10), but the evict logic happens on every `record()` call. Minor efficiency concern.

---

## Rating: **C+**

| Category | Grade |
|----------|-------|
| Security | C (address enrichment concern) |
| Correctness | C+ (NaN risk, unused field) |
| Code Quality | B- (mostly clean) |
| **Overall** | **C+** |

### Key Concerns:
1. **Address spoofing risk** in bootstrap cache enrichment
2. **Floating point in security-critical timeout** code  
3. `legacy_coexistence_mode` appears dead/incomplete

### Positives:
- Correct Phi-Accrual-lite formula implementation
- Proper bounded VecDeque with `INTER_ARRIVAL_WINDOW = 10`
- Good documentation and math
- Safe `saturating_sub` for time calculations
- Reasonable clamp bounds (180-600s)

The diff is reasonable but has enough security and completeness concerns to warrant review before merging.
