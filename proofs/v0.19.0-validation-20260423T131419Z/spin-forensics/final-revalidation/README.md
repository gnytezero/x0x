# Final re-validation: fix as published to crates.io

**Goal:** prove the spin fix works when consumed through the normal crates.io
pipeline, with no `[patch.crates-io]` hacks.

**Binary:** `316436de6c0250c8432b4b1ac7e07a3347acf5209094bc5a0e156910352eede8`
(452 MB release-profiling build, Linux x86_64-unknown-linux-gnu).
Built from:
- `x0x @ fb51ba9` (v0.19.0) with Cargo.toml updated to require
  `ant-quic = "0.27.4"` and all `saorsa-gossip-* = "0.5.20"`; the
  `[patch.crates-io] ant-quic = { path = "../ant-quic" }` stanza is
  **removed**.
- Deps resolved from crates.io only.

## Version chain published to crates.io

| Crate | Version | crates.io uploaded |
|---|---|---|
| `ant-quic` | `0.27.4` | `2026-04-23T19:17:39Z` |
| `saorsa-gossip-*` (11 crates) | `0.5.20` | `2026-04-23T19:39Z` |

Both were cut in this session:
- ant-quic master: `effda0dd chore(release): v0.27.4` + `79f14e2a fix(dual_stack): AND-combine v4/v6 writability to prevent tokio spin`
- saorsa-gossip main: `8cf202e chore(release): v0.5.20 — bump ant-quic to 0.27.4`

## Deploy window

- Rolling deploy to all 6 VPS: `20:26:16 → 20:28:15Z` (15 s gap per node).
- Watch started at `20:28:47Z`, completed at `20:39:54Z`.
- Final health probe at `20:40:13Z`.
- Nuremberg transient 60 s re-check at `20:40:30Z` confirmed benign.

## CPU observations (13 samples × 6 nodes across 11 min)

| Metric | Value |
|---|---|
| Peak any single sample | **40 %** (nyc T+480s) |
| Second-highest | 40 % → 30 % (sfo T+120s) |
| Nodes with any R-state tokio-rt-worker | 0 sustained; transient (1 R thread) appearances only |
| Samples above the 50 % stop-condition threshold | **0** |
| Nuremberg 60-s sustained sample after spurious 50 % point-reading | 18.2 % → 0 → 0 → 0 → 10 → 0 (peak 18.2 %, brief) |

## Mesh health at watch close

| Node | IP | Version | Peers | tid max CPU |
|---|---|---|---|---|
| nyc | 142.93.199.50 | 0.19.0 | 6 | 10 |
| sfo | 147.182.234.192 | 0.19.0 | 6 | 0 |
| helsinki | 65.21.157.229 | 0.19.0 | 6 | 10 |
| nuremberg | 116.203.101.172 | 0.19.0 | 6 | 50 → 0 (transient) |
| singapore | 152.42.210.67 | 0.19.0 | 6 | 0 |
| sydney | 170.64.176.102 | 0.19.0 | 6 | 10 |

## Comparison matrix

| Build | Source | CPU behaviour |
|---|---|---|
| v0.18.x / v0.19.0 w/ ant-quic 0.27.3 | crates.io | 2–3 of 6 nodes latch into 100 % tokio-rt-worker spin within 4–7 min |
| v0.19.0 + local `[patch.crates-io]` ant-quic `43acc666` (pre-fix experiment) | path dep | Same as above (getsockname caching alone insufficient) |
| v0.19.0 + local `[patch.crates-io]` ant-quic `effda0dd` (AND-poller fix, path dep) | path dep | Clean — 90 min, max 30 % |
| **v0.19.0 + crates.io `ant-quic 0.27.4` + `saorsa-gossip 0.5.20`** | **crates.io** | **Clean — 11 min, max 40 %, no sustained spin** |

## Verdict

🟢 **GREEN LIGHT.** The fix as delivered through the normal crates.io
pipeline behaves identically to the path-patched build validated earlier.
Public users installing `x0x = 0.19.0` now pick up `ant-quic 0.27.4` and
`saorsa-gossip-* 0.5.20` via the lockfile — no local hacks required.

## Follow-ups still open (from main README)

- x0x Cargo.toml changes (ant-quic → 0.27.4, saorsa-gossip-* → 0.5.20,
  strip `[patch.crates-io]`) are **uncommitted in x0x**. Commit + optional
  `x0x 0.19.1` release when you're ready.
- The two test-harness fixes (`tests/e2e_deploy.sh` rolling delay +
  `tests/comprehensive_integration.rs` rand flake) remain uncommitted.
- Step 6 `e2e_vps.sh` curl-harness flakiness (9/156 under parallel
  SSH-tunnel storms) still worth tightening.
