## Summary

On public-IP bootstrap nodes running ant-quic 0.26.7 (via x0x 0.17.0), NAT
traversal / hole-punch machinery fires on connections between peers that are
**already directly reachable** (global v4 + global v6, `FullCone`,
`can_receive_direct: true`, `has_global_address: true`). The resulting
`hole_punch_success_rate` is catastrophically low (4–7.5 %) and failure
storms dominate the journal.

This is not a correctness bug in the cryptographic sense, but it is the
dominant source of jitter and added latency on the x0x VPS mesh and appears
to block the stream-level data flow for ~60–100 s on some pairs (see the
companion "message arrives late" symptom in issue B of this pair).

## Environment

- ant-quic 0.26.7 (workspace path dep from x0x at this commit)
- x0x 0.17.0, instrumented build with `RUST_LOG=ant_quic=info,x0x::*=debug`
- 3 bootstrap VPS nodes: `saorsa-1` (Helsinki/Hetzner), `saorsa-2` (NYC/DO),
  `saorsa-9` (Tokyo/Vultr)
- All three bind dual-stack `[::]:5483`, have public v4 + global v6

## Evidence

### Per-node end-state (from `/diagnostics/connectivity`)

| Node     | NAT type | can_receive_direct | has_global_address | hole_punch_success_rate | direct conns | relayed conns |
|----------|---------:|-------------------:|-------------------:|------------------------:|-------------:|--------------:|
| helsinki | FullCone | true               | true               | **0.075**               | 9            | 0             |
| nyc      | FullCone | true               | true               | **0.054**               | 16           | 0             |
| tokyo    | FullCone | true               | true               | **0.040**               | 12           | 0             |

Every node reports FullCone / globally reachable. They should not need
hole-punching to connect to each other at all.

### NAT-failure density (5-minute window, after a matrix probe)

| Node     | Total `nat_traversal_api` log lines | `NAT traversal failed` or `No successful punch` lines |
|----------|------------------------------------:|------------------------------------------------------:|
| helsinki | 21                                  | 13 (62 %)                                             |
| nyc      | 9                                   | 1 (11 %)                                              |
| tokyo    | 22                                  | 9 (41 %)                                              |

### Representative journal excerpt (Tokyo)

```
2026-04-15T20:23:47Z WARN ant_quic::nat_traversal_api: Phase Punching failed for peer PeerId([53, 92, 87, 144, 121, 250, 193, 236, 205, 143, 0, 0, ...0]): NoCandidatesFound, retrying (attempt 2) after 2.045s
2026-04-15T20:23:47Z WARN ant_quic::nat_traversal_api: No successful punch results for peer PeerId([135, 187, 44, 98, 109, 5, 35, 117, 92, 207, 0, 0, ...0])
2026-04-15T20:23:47Z WARN ant_quic::nat_traversal_api: Phase Punching failed for peer PeerId([135, 187, 44, 98, ...]): PunchingFailed("No successful punch"), retrying (attempt 3) after 4.021s
2026-04-15T20:23:49Z ERROR ant_quic::nat_traversal_api: NAT traversal failed for peer PeerId([64, 104, 237, 7, 22, 39, ...]) after 3 attempts
```

The "peer PeerId" has only 10 leading bytes non-zero, which suggests a
truncated / stub peer identity — worth investigating whether the traversal
path is being triggered from an incompletely-populated peer record.

### Error chain seen when a dial ultimately fails

When an end-to-end connection cannot be established:

```
Endpoint error: All connection strategies failed:
  DirectIPv4: Timeout;
  DirectIPv6: Happy Eyeballs timed out;
  HolePunch { round: 1 }: Timeout;
  HolePunch { round: 2 }: Timeout;
  HolePunch { round: 3 }: Timeout;
  HolePunch { round: 3 }: Timeout;
  Relay: Connection error: Relay did not provide socket
```

Six hole-punch attempts plus a relay attempt are consumed on peers that
should have succeeded on the very first direct dial.

## Expected behaviour

For peers whose `can_receive_direct == true` (or whose advertised addresses
include globally-routable entries):

1. Direct v6 AAAA should be tried first with a short per-address timeout
   (e.g. 3–5 s).
2. Direct v4 in parallel / after (Happy Eyeballs).
3. Hole-punch should only be triggered when direct has failed **and** at
   least one side's `can_receive_direct` is false (or `nat_type != FullCone`
   / `Unknown`).
4. On fast success, the hole-punch machinery should not be spun up at all.

## Actual behaviour

The hole-punch subsystem appears to fire on essentially every new peer
relationship, including between two nodes that are trivially reachable on
port 5483. Success rate 4–7.5 % suggests most hole-punch attempts never
produce a usable candidate.

## Hypothesis (non-authoritative)

- `nat_traversal_api::Synchronization` phase may be triggered regardless of
  whether the peer is globally reachable.
- The zero-padded peer IDs in the failure logs hint at a code path that
  kicks off traversal based on partial / stub peer records.
- The existing address-scope tracker (`socket_addr_scope`) is not being
  consulted as a short-circuit before phase start.

## Impact

- Wasted CPU / socket budget on every node (multiplied by the N² peer
  relationship space).
- Latency tail on x0x's `/direct/send` → `/direct/events`: observed
  send→recv gap of 60–100 s on VPS pairs (see issue B).
- Journal noise masks other real issues during operational triage.

## Reproduction

1. Check out ant-quic 0.26.7 and x0x at 0.17.0 with the instrumented logging
   patch (see attached `journals/` tarball for the current run).
2. Deploy x0xd to at least two public-IP nodes with
   `RUST_LOG=ant_quic=info`.
3. Have them learn each other via the default bootstrap or `announce`.
4. Observe `ant_quic::nat_traversal_api: Phase Punching failed` lines
   within minutes.
5. Query `/diagnostics/connectivity` (x0x REST) — see
   `connections.hole_punch_success_rate` below 0.1.

## Artefacts

Journal tarball and per-node diagnostic snapshots from 2026-04-15T20:26Z can
be provided on request (VPS journals filtered to `ant_quic::nat_traversal_api`
for a 5-minute window). They are currently in
`tests/proof-reports/matrix_20260415T202646Z/` in the x0x repository.

## Priority

This is the dominant source of connection-layer churn on public-IP mesh
peers and should, in our view, block a 100 %-connectivity promise for x0x
VPS deployments.
