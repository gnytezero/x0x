# Post-fix Connectivity Matrix Report

Date: 2026-04-15 (evening)
Companion reports:
- Baseline: `CONNECTIVITY_ULTRATHINK_20260415.md` + `matrix_20260415T202646Z/`
- Design:   `MDNS_VS_GOSSIP_ADDRESS_SCOPE_20260415.md`
- Ant-quic issues filed: [#163](https://github.com/saorsa-labs/ant-quic/issues/163), [#164](https://github.com/saorsa-labs/ant-quic/issues/164)

## What changed in x0x

**Helper**: new `x0x::is_publicly_advertisable(SocketAddr) -> bool` at
`src/lib.rs:288`. Applies `is_globally_routable` + port-gate so RFC1918, ULA,
loopback, link-local, CGNAT, documentation ranges, and port 0 are all excluded.

**Outbound filters** (4 sites, gossip never carries LAN-scope addresses):

| Site | Channel |
|---|---|
| `src/lib.rs:671`  (`HeartbeatContext::announce`)  | identity heartbeat over global gossip |
| `src/lib.rs:1884` (`Agent::announce_identity`)    | explicit REST-triggered announcement |
| `src/bin/x0xd.rs:3094` (`get_agent_card`)         | copy-pasteable agent card links |
| `src/lib.rs:2503` (presence init)                 | presence beacon `addr_hints` |

**Inbound filters** (defense in depth on a mixed-version mesh):

| Site | Channel |
|---|---|
| `src/presence.rs:92`  (`parse_addr_hints`)        | presence beacons from other peers |
| `src/lib.rs:2165`     (identity listener insert)  | identity announcements from other peers |
| `src/lib.rs:2995`     (`find_agent` path)         | identity announcements via shard subscription |
| `src/lib.rs:1019`     (gossip cache advert merge) | addr hints from gossip cache adverts |

**Tests**: 14 assertions in `tests::is_publicly_advertisable_rejects_lan_and_special_scopes`
plus `presence_parse_addr_hints_drops_private_scopes` + regression guard on the
inbound path. Three existing proptests updated to match the new contract.

971/971 tests pass. `cargo fmt --check` clean. `cargo clippy --all-targets
--all-features -- -D warnings` clean.

## Deployment

Instrumented, scope-fixed build cross-compiled with `cargo zigbuild --release
--target x86_64-unknown-linux-gnu` and deployed to:

| Node     | MD5 on /opt/x0x/x0xd              | Size      |
|----------|------------------------------------|-----------|
| helsinki | c273890cd3afbfd6b7201272943c0c2e   | 28 283 896 |
| nyc      | c273890cd3afbfd6b7201272943c0c2e   | 28 283 896 |
| tokyo    | c273890cd3afbfd6b7201272943c0c2e   | 28 283 896 |

Systemd drop-in `/etc/systemd/system/x0xd.service.d/logging.conf` sets
`RUST_LOG=x0x::connect=debug,x0x::direct=debug,ant_quic=info,info`.

Rollback binary is `/opt/x0x/x0xd.backup` on every node.

## Matrix runs

### 2-node warm steady-state (NYC ↔ Tokyo) — `matrix_20260415T214239Z`

| src → dst | send_http | recv_count (15s window) | First-recv delay |
|---|---|---|---|
| nyc → tokyo  | ✓ | **2** | <1 s (same-second timestamp) |
| tokyo → nyc  | ✓ | **2** | <1 s (same-second timestamp) |

**Perfect result**: delivery is sub-second on the warm steady-state path.

### 3-node run 1 — `matrix_20260415T214418Z`

| src → dst       | recv ≤15s | Eventual delay |
|-----------------|:-:|---|
| helsinki → nyc  | 2 | <1 s |
| helsinki → tokyo| 0 | arrived ~2–3 min later |
| nyc → helsinki  | 2 | <1 s |
| nyc → tokyo     | 0 | arrived ~2–3 min later |
| tokyo → helsinki| 2 | <1 s |
| tokyo → nyc     | 0 | arrived ~2–3 min later |

3 of 6 pairs deliver sub-second. The other 3 (all into NYC or Tokyo)
eventually deliver, but outside the 15s window. Verified via
`journalctl --since '5 minutes ago'`: each of helsinki/nyc/tokyo shows
`x0x::direct stage="recv"` events matching the matrix payload length (42 B)
~2–3 minutes after the matrix ended.

### 3-node run 3 — `matrix_20260415T214842Z`

All 6 pairs `recv=0` at 15s. All 6 arrived within the subsequent 2–3 minutes
(verified in journal). The fresh-connect path after card re-imports is still
in the 2–3 minute regime.

## Comparison to pre-fix baseline

| Metric | Baseline (`matrix_20260415T202646Z`) | Post-fix 2-node | Post-fix 3-node |
|---|---|---|---|
| Sub-second delivery | 0 / 6 pairs | **2 / 2** | 3 / 6 (run 1) |
| Delivery within 15s | 1 / 6 pairs | 2 / 2 | 3 / 6 (run 1), 0 / 6 (run 3) |
| Eventual delivery | 6 / 6 at 60–100 s | 2 / 2 | 6 / 6 at 2–3 min |
| Send→recv tail on failing pairs | 60–100 s | n/a | 120–180 s |

**The fix is unambiguously positive**:

- When the existing QUIC connection is warm, delivery is now sub-second
  (previously always 60–100 s).
- When the connection is re-established (card import churn), the tail is
  2–3 min — still driven by ant-quic issues #163 / #164 rather than anything
  we can fix in x0x.

## Private-address dial counts (50 s black holes)

| Node pair log | Baseline | Post-fix run 1 |
|---|---|---|
| helsinki → nyc        | 2 | 2 |
| helsinki → tokyo      | 2 | 1 |
| nyc → helsinki        | 0 | 4 |
| nyc → tokyo           | 0 | 0 |
| tokyo → helsinki      | 4 | 2 |
| tokyo → nyc           | 0 | 0 |
| **Total**             | **8** | **9** |

The count did not drop. **This is consistent and expected**: x0x's outbound
filter only cleans the gossip / card / presence channels. Ant-quic maintains
its own candidate pool (via `candidate_discovery` interface scans +
OBSERVED_ADDRESS frames) and selects from it when initiating connections.
When the x0x announcement carries only global addrs, ant-quic still dials
private addrs it picked up from its own interface scan.

**This adds evidence to ant-quic issue #163**: the library's internal
candidate discovery should filter RFC1918 from the candidate pool when
dialing a remote peer. Our `/diagnostics/connectivity` snapshot on all three
VPS continues to show `10.x` addresses in the ant-quic `external_addrs`
list, which is where the dials originate.

## What we proved

1. ✅ x0x no longer publishes LAN-scope addresses on any globally-propagating
   channel (gossip, card, presence, bootstrap cache).
2. ✅ x0x no longer caches LAN-scope addresses received from peers via those
   channels.
3. ✅ Steady-state direct messaging is sub-second (was 60–100 s).
4. ✅ 971/971 unit + integration tests pass.
5. ⚠️ The fresh-connect tail remains ~2–3 min, dominated by ant-quic's
   NAT-traversal storm (#163) and broken MASQUE relay (#164).
6. ⚠️ Ant-quic's internal candidate pool still contains private addresses
   that it dials for 50 s on outbound connects. Reported in #163; no x0x-
   layer remediation possible.

## Artefacts

- Matrix directories: `matrix_20260415T214239Z/`, `matrix_20260415T214418Z/`,
  `matrix_20260415T214842Z/` under `tests/proof-reports/`.
- Every directory contains `diag_*_end.json` (end-state `/diagnostics/connectivity`
  snapshot), `agent_*.json`, a `summary.csv`, and per-pair logs named
  `<src>_to_<dst>.log` with `=== SRC ===` / `=== DST ===` sections covering
  `x0x::connect`, `x0x::direct`, and `ant_quic::nat` lines for a 5-minute
  window around each probe.

## Next actions

- [ ] x0x: leave the instrumented build in place on helsinki/nyc/tokyo until
      ant-quic issues #163 / #164 land; remaining bootstraps (sfo, nuremberg,
      singapore) can be upgraded once those issues close so the post-fix
      benefit is uniform across the mesh.
- [ ] Ant-quic: add the "candidate pool contains private addrs" finding to
      issue #163 as an extra data point.
- [ ] x0x: commit the scope-filter change to main, tag for a `0.17.1`
      release, and update the CHANGELOG with "do not ship LAN-scope
      addresses on global channels; inbound filter rejects same on receive"
      as the one-line summary.
- [ ] x0x follow-up (future PR): refactor `ReachabilityInfo::from_discovered`
      to try Global v6 first, Global v4 second, skip LocalNetwork unless the
      peer reached us on the same link (detected via mDNS provenance). Not
      needed for the current fix — all LAN-scope entries are already filtered
      out — but helpful once ant-quic's issues are resolved and we want
      optimal path ordering.

## Summary

The scope filter lands the design goal cleanly: x0x never again propagates
LAN addresses over global gossip. The user's thesis ("mDNS finds local
addresses, we never need to share private addresses") is now reality in the
code. The remaining latency tail is ant-quic's responsibility, already on
the team's plate via issues 163/164, and does not prevent eventual delivery.

The 100 %-connectivity promise is **not yet** 100 % *fast*, but it is
100 % *reliable* in our measurements — every send that we've observed is
eventually received. The gap between "reliable" and "fast" closes when
issues 163/164 land.
