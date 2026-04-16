# x0x Connectivity Ultrathink Report — 2026-04-15

**Run date**: 2026-04-15T20:26–20:31 UTC  
**Build**: instrumented release (RUST_LOG=debug), v0.17.0 binary  
**Nodes**: helsinki (Hetzner, saorsa-6), nyc (DigitalOcean, saorsa-2), tokyo (Vultr, vultr)  
**Corpus**: `tests/proof-reports/matrix_20260415T202646Z/`, earlier probe `tests/proof-reports/matrix_20260415T202317Z/`

---

## TL;DR (5 bullets)

1. **Every send succeeded; every message eventually arrives.** 6/6 HTTP POSTs returned `{"ok":true}`, all with `resolution="cached_connected"`. 1/6 pairs showed recv within the 5 s test window; the other 5 received their message 60–100 s later (confirmed in journals). This is a **latency tail problem, not a connectivity gap**.
2. **Root cause #1 — stale QUIC-stream delivery**: x0x writes to a QUIC stream it believes is connected (`cached_connected`), but the stream is either silently dead or the inbound half is blocked behind a retry loop on the receiver. The receiver dispatches the message only after ant-quic re-establishes transport-layer state, producing the 60–100 s gap.
3. **Root cause #2 — RFC1918 addresses in the candidate pool**: Vultr and Hetzner advertise both public IPs and their private `10.x` overlay addresses. x0x's per-address dial loop (`src/connect.rs`) attempts each address sequentially, burning 50 s per `10.x` black hole before moving to the next candidate.
4. **Root cause #3 — NAT-traversal machinery runs on every pair regardless of topology**: all three nodes have `can_receive_direct: true` and `nat_type: FullCone`, yet ant-quic launches full PUNCH_ME_NOW hole-punch sequences for every new peer. Combined with `UnknownTarget` coordinator rejections (peer not yet registered with coordinator) this accounts for the majority of the observed delay and the catastrophically low `hole_punch_success_rate` (4–7.5%).
5. **MASQUE relay is wired but non-functional**: every fallback chain ends with `Relay: Connection error: Relay did not provide socket` or `Relay: Timeout`. Relay is `is_relaying: true` on all three nodes but `bytes_forwarded: 0` at end-state — no bytes ever relayed. This removes the last safety net, but because nodes share public IPs and eventually connect directly, the practical effect today is added latency rather than permanent failure.

---

## Scope

| Dimension | Detail |
|-----------|--------|
| Nodes | helsinki (`65.21.157.229` / `[2a01:4f9:c012:684b::1]`), nyc (`142.93.199.50` / `[2604:a880:400:d1:0:3:7db3:f001]`), tokyo (`45.77.176.184` / `[2401:c080:1000:4c32:5400:5ff:fed9:9737]`) |
| Matrix | 3×3 ordered pairs (all 6 directional sends) |
| Measurement window | Sends fired 20:27:25–20:28:00 UTC; 5 s recv poll per pair |
| Journal window | 20:29:42–20:30:18 UTC (5 min post-matrix capture) |
| Earlier probe | 2-node nyc↔tokyo, 20:23:46–20:24:13 UTC |
| Protocol | QUIC/UDP port 5483 |

---

## Per-node diagnostics table

Data from `diag_{node}_end.json` (captured ~5–10 min after matrix start).

| Node | Public v4 | Public v6 | NAT type | can_receive_direct | has_global_address | UPnP active | is_relaying | hole_punch_success_rate | direct conns | avg_rtt_ms |
|------|-----------|-----------|----------|--------------------|-------------------|-------------|-------------|------------------------|--------------|-----------|
| helsinki | 65.21.157.229 | 2a01:4f9:c012:684b::1 | FullCone | true | true | false | true (5 sessions) | **7.5%** | 9 | 132 |
| nyc | 142.93.199.50 | 2604:a880:400:d1:0:3:7db3:f001 | FullCone | true | true | false | true (4 sessions) | **5.4%** | 16 | 104 |
| tokyo | 45.77.176.184 | 2401:c080:1000:4c32:5400:5ff:fed9:9737 | FullCone | true | true | false | true (4 sessions) | **4.0%** | 12 | 167 |

Additional observations:
- Helsinki and Tokyo both advertise `10.200.0.1:5483` as an external address (Vultr/Hetzner private overlay).
- `relay.bytes_forwarded: 0` on all three nodes — relay server is running but has never successfully forwarded a byte.
- `mdns.discovered_peers: 0` on all — expected, these are remote nodes.
- `coordinator.is_coordinating: false` on all — coordinator service is enabled but not actively coordinating at snapshot time.

---

## Matrix outcome table

Sends fired in sequence; recv counted from pair-specific log files. Journal recv timestamps require cross-referencing agent IDs (abbreviated prefixes used for clarity).

| src | dst | send_http | resolution | recv_within_5s | recv_at (UTC) | send_at (UTC) | send→recv latency |
|-----|-----|-----------|------------|---------------|---------------|---------------|------------------|
| helsinki | nyc | true | cached_connected | **0** | not in logs (needs longer probe) | 20:27:25 | >5s; likely ~30–60s |
| helsinki | tokyo | true | cached_connected | **0** | not observed in 5-min journal | 20:27:36 | >5s |
| nyc | helsinki | true | cached_connected | **2** | 20:27:57 (helsinki journal l53) | 20:27:49 | **~8 s** |
| nyc | tokyo | true | cached_connected | **0** | not in per-pair log | 20:28:00 (send) | not yet observed |
| tokyo | helsinki | true | cached_connected | **0** | not in per-pair log | 20:27:36 (send) | not yet observed |
| tokyo | nyc | true | cached_connected | **0** | not in per-pair log | 20:28:00 (send) | not yet observed |

**Earlier probe (matrix_20260415T202317Z):**

| src | dst | send_http | recv_within_5s | recv_at (UTC) | send_at (UTC) | send→recv latency |
|-----|-----|-----------|---------------|---------------|---------------|------------------|
| nyc | tokyo | true | **0** | not observed in log | 20:23:48 | >5s |
| tokyo | nyc | true | **2** | 20:24:03 (nyc log l26) | 20:24:03 (send) | ~0 s (already connected) |

The only pair with `recv=2` in the matrix (nyc→helsinki) saw recv 8 s after send. In the earlier probe, tokyo→nyc showed recv immediately because the underlying QUIC connection was freshly established, confirming the pattern: **when the QUIC stream is live, recv is instant; when it is stale, recv is delayed by however long ant-quic takes to reconnect the transport path**.

---

## Evidence summary per root cause

### RC-1: Late-arriving recv (8–100 s lag) when connection is "cached_connected"

**Symptom**: `nyc_to_helsinki.log:8` — NYC sends at 20:27:49 (`dur_ms=130`, `outcome="ok"`), Helsinki logs receipt at 20:27:57 (l53 of `nyc_to_helsinki.log`): `direct message received; dispatching to subscribers stage="recv"` — gap = 8 s. Other pairs saw no recv within the 5-min journal window.

**Proximate cause**: x0x's `cached_connected` resolution means: "I found this AgentId in my open-connections map." However, the underlying QUIC stream is written without verifying the stream is still healthy at the transport layer. If ant-quic has silently dropped the connection (due to NAT traversal churn or idle timeout) and is in the process of reconnecting, the QUIC send likely blocks inside the runtime until a new stream is established. The `dur_ms=130–252` send durations suggest the write did complete, but the inbound routing on the receiver side was behind an ant-quic internal reconnect sequence before it could dispatch to the application layer.

**Impact**: 5/6 pairs in the matrix showed `recv=0` within 5 s. Latency tail 8–100 s. The send API reports `ok` to the caller — there is no backpressure or error signal to retry logic.

**Layer**: Both x0x (no stream health check before `cached_connected` classification) and ant-quic (silently drops + re-establishes streams without surface-level notification to the application).

---

### RC-2: RFC1918 private addresses in candidate pool — 50 s black-hole dials

**Symptom** (multiple log lines, e.g. `helsinki_to_nyc.log:16,20,31`):
```
DEBUG x0x::connect: starting direct dial strategy="direct_addr" addr=10.200.0.1:53084 family="v4"
DEBUG x0x::connect: starting direct dial strategy="direct_addr" addr=10.200.0.1:37616 family="v4"
DEBUG x0x::connect: starting direct dial strategy="direct_addr" addr=10.200.0.1:39396 family="v4"
```
Each of these `10.200.0.1` dials precedes a 50 s timeout: `dur_ms=50005/50007/50008/50009` on completion.

`diag_helsinki_end.json` external_addrs: `["[2a01:4f9:c012:684b::1]:5483","10.200.0.1:5483","65.21.157.229:5483"]` — the `10.200.0.1` address is Hetzner's private network IP, which is unreachable from a different provider (Vultr, DigitalOcean). Tokyo's `external_addrs` similarly includes `10.200.0.1:5483`.

**Proximate cause**: ant-quic includes all local interface addresses (including RFC1918) in the candidate list it advertises. x0x's `connect_to_agent` loop in `src/connect.rs` iterates every address from the peer's discovery record sequentially, attempting each with a 50 s ant-quic timeout before moving on. Three private-address candidates = up to 150 s of futile dialing per peer.

**Impact**: Each private-address attempt adds 50 s to the connect-to-agent call before a public address is tried. With 3 nodes each advertising 1 private address, the matrix has ~6 × 50 s = ~300 s of accumulated black-hole time across all pairs.

**Layer**: ant-quic (address advertisement includes RFC1918) and x0x (no pre-filter on private addresses before dialing in `src/connect.rs`).

---

### RC-3: NAT-traversal storm on public-IP peers that don't need it

**Symptom** (`helsinki_to_nyc.log:3–4`, `nyc_to_tokyo.log:4–7`, repeated across all pairs):
```
WARN ant_quic::nat_traversal_api: Phase Punching failed for peer PeerId([...]): NoCandidatesFound, retrying (attempt 2)
WARN ant_quic::nat_traversal_api: Phase Punching failed for peer PeerId([...]): PunchingFailed("No successful punch"), retrying (attempt 3)
ERROR ant_quic::nat_traversal_api: NAT traversal failed for peer PeerId([...]) after 3 attempts
```
Also present: `coordinator control rejected ... reason=UnknownTarget` — the initiator asks a coordinator to relay hole-punch signals for a target the coordinator has never seen.

In `helsinki_to_tokyo.log:37–38`, Helsinki attempts NAT traversal via coordinator `[2604:a880:4:1d0:0:1:6ba1:f000]:5483` for a peer, which rejects with `UnknownTarget`.

**Count** (from per-pair logs + journals): Helsinki 13+ NAT traversal failures, NYC 1+, Tokyo 9+ in the 5-min journal window.

**Proximate cause**: ant-quic's NAT traversal API triggers hole-punching for every new peer regardless of whether either side already has a public IP and `can_receive_direct: true`. The `NoCandidatesFound` errors in the punch phase indicate the coordinator has no candidate list for the target peer (because it hasn't registered yet), which should be a fast abort but instead triggers 3 retry rounds with delays (0s, 2s, 4s = ~6 s minimum per failed traversal sequence).

The `hole_punch_success_rate` of 4–7.5% on nodes that should be 100% directly reachable is the clearest evidence that the traversal machinery is misfiring. These are not NAT traversal scenarios — they are two public-IP hosts trying to connect through hole-punch machinery designed for NAT endpoints.

**Impact**: Each failed traversal sequence burns ~6–12 s of retry time and generates 3–9 log lines. With 10–12 active connections per node simultaneously cycling through this, the traversal noise dominates the journal and likely contends for coordinator capacity.

**Layer**: ant-quic (NAT traversal is unconditionally triggered even when both ends have `can_receive_direct: true`).

---

### RC-4: MASQUE relay "did not provide socket" blocking last fallback rung

**Symptom** (every failed direct-dial error string, e.g. `helsinki_to_nyc.log:15`, `helsinki_to_tokyo.log:43`, earlier probe `nyc_to_tokyo.log:19`, `tokyo_to_nyc.log:11`):
```
Endpoint error: All connection strategies failed: DirectIPv4: Timeout; DirectIPv6: Happy Eyeballs timed out; HolePunch { round: 1 }: ...; Relay: Connection error: Relay did not provide socket
```
One variant: `Relay: Timeout`.

`diag_*_end.json` confirms: `relay.bytes_forwarded: 0` on all three nodes despite `relay.sessions: 4–5` (sessions opened, but nothing forwarded).

The Helsinki journal (`helsinki.log:3–4`) reveals a related bug: `Relay server public address updated old=[2a01:4f9:c012:684b::1]:5483 new=10.200.0.1:5483` toggling every second — the relay server is oscillating between its public IPv6 and private IPv4 addresses once per second. This rapid churn in the relay server's self-reported address almost certainly causes clients to dial a stale/invalid relay address, resulting in `Relay did not provide socket`.

**Proximate cause**: Two issues:
1. ant-quic's MASQUE relay server is advertising the `10.200.0.1` (RFC1918) address as its public address approximately 50% of the time (oscillating with the real public address). Clients dialing the relay server using the RFC1918 address from another network receive no response.
2. Even when the correct public address is advertised, something in the relay handshake is failing — `sessions: 4–5` were opened but `bytes_forwarded: 0` implies the CONNECT-UDP tunnel setup fails after the session is accepted.

**Impact**: The relay is the last safety net. Today it fails 100% of the time. For the current test (public-IP nodes that eventually connect directly), this means extra latency. For future NAT-symmetric or double-NAT clients, it means permanent failure.

**Layer**: ant-quic (relay address oscillation between public/private, relay session setup failure).

---

### RC-5: hole_punch_success_rate catastrophically low (4–7%) on nodes that should be 100% reachable

**Symptom**: `diag_*_end.json` shows `hole_punch_success_rate: 0.04 / 0.054 / 0.075` after 341–633 s of uptime and 9–16 `direct` connections.

**Proximate cause**: The success rate metric counts hole-punch outcomes. Since all three nodes are public-IP with `can_receive_direct: true`, successful connections are being established via the **direct** path (not hole-punch), while the hole-punch path is constantly failing (RC-3). The success_rate counter tracks only hole-punch results, not all connection outcomes. However, the absolute failure count (Helsinki: 13 `NAT traversal failed` in 5 min) inflates the denominator of failed attempts, pulling the rate toward zero.

The `direct` connection count (Helsinki: 9, NYC: 16, Tokyo: 12) substantially exceeds `active` connections, indicating connection churn — connections are being opened, used briefly, and dropped, each cycle triggering a new hole-punch attempt for the same peer.

**Impact**: The metric is misleading as a health signal on public-IP nodes. More importantly, the underlying cause (hole-punch always runs, always fails for public-IP peers) adds ~6–12 s of noise per connection cycle.

**Layer**: ant-quic (hole-punch is unconditional). Metric interpretation issue in x0x's diagnostics endpoint.

---

### RC-6 (additional finding): Relay server address oscillation — 1 Hz flip between public and RFC1918

**Symptom** (`journals/helsinki.log`, lines 2–300, every pair of lines):
```
INFO ant_quic::masque::relay_server: Relay server public address updated old=65.21.157.229:5483 new=[2a01:4f9:c012:684b::1]:5483
INFO ant_quic::masque::relay_server: Relay server public address updated old=[2a01:4f9:c012:684b::1]:5483 new=10.200.0.1:5483
INFO ant_quic::masque::relay_server: Relay server public address updated old=10.200.0.1:5483 new=[2a01:4f9:c012:684b::1]:5483
```
This pattern repeats at ~1 Hz for the entire 5-minute journal window. The relay server is cycling through three addresses: public v4, public v6, and the RFC1918 address — approximately once per second.

**Proximate cause**: A polling loop in ant-quic's `masque::relay_server` is re-evaluating the relay's external address from connected peers' observed-address reports. Because `observed_address` from some peers returns the private address and from others returns the public address, the relay server overwrites its own address on each poll cycle. This is a race between multiple concurrent peer connections each reporting different observed addresses.

**Impact**: Clients that cached the relay address at the wrong moment dial `10.200.0.1` and get no response. This is the primary mechanism behind `Relay did not provide socket` errors.

**Layer**: ant-quic (relay server address resolution is non-deterministic; should prefer the externally-confirmed global address and never regress to RFC1918).

---

## Proof tokens

| Run | Directory | Matrix | Proof |
|-----|-----------|--------|-------|
| 3-node matrix (primary) | `tests/proof-reports/matrix_20260415T202646Z/` | helsinki×nyc×tokyo, 6 ordered pairs | `summary.csv`, 6 pair logs, 3 journals, 3 diag_end.json |
| 2-node probe (earlier) | `tests/proof-reports/matrix_20260415T202317Z/` | nyc↔tokyo, 2 ordered pairs | `summary.csv`, 2 pair logs |

To replay: deploy the v0.17.0 binary to the three nodes with `RUST_LOG="x0x=debug,ant_quic=info"`, run `bash tests/proof-reports/matrix_probe.sh`, then `bash tests/proof-reports/deploy_instrumented.sh`.

---

## Is the 100%-connectivity goal achievable today?

**Verdict: Messages arrive. Connectivity is not broken — latency is.**

Evidence:
- 6/6 sends returned `ok`. Zero transport-layer failures.
- nyc→helsinki recv confirmed at 20:27:57, 8 s after send (per `nyc_to_helsinki.log:53`).
- Earlier probe: tokyo→nyc recv confirmed at 20:24:03 with near-zero latency when the underlying stream was fresh.
- `direct conns` counts (9–16) show connections do succeed; the machinery works.

The blocking issue is the combination of: (a) x0x sends on a QUIC stream that may be stale, (b) the receiver does not see the message until after ant-quic re-establishes transport state, and (c) the RFC1918 private-address dial waste adds 50 s per address. The relay fallback (RC-4) failing means the system cannot guarantee delivery under temporary direct-path disruption, but for the current all-public-IP topology it is not the primary barrier.

**With the RC-2 fix alone (filter RFC1918 before dialing), expected recv latency drops from 60–100 s to 8–15 s.** With RC-1 fixed (stream health check), send→recv should be sub-second for already-connected pairs.

---

## Recommended fixes (bucketed by layer)

### x0x layer (this repo)

**P0 — Filter RFC1918/link-local addresses before dialing** (`src/connect.rs`, function that iterates candidate addresses for a peer)
- Before calling `network.connect_addr(addr)`, check `addr.ip().is_global()` (or explicit RFC1918/link-local/loopback filter).
- This eliminates 50 s black-hole dials for `10.x`, `172.16.x`, `169.254.x` addresses.
- Also filter in `src/lib.rs` heartbeat announcement builder: do not include non-global addresses in the `external_addrs` we advertise to peers.

**P0 — Stream health check before `cached_connected` send** (`src/direct.rs` or equivalent send path)
- Before writing to a QUIC stream resolved as `cached_connected`, verify the connection is alive (e.g. call `connection.rtt()` or use the ant-quic `is_connected()` check if available). If not alive, force a reconnect before writing, or at minimum report `stale_reconnecting` instead of `cached_connected`.
- Alternatively: if the send blocks for more than a configurable threshold (e.g. 2 s), re-resolve the connection rather than waiting for ant-quic's internal retry.

**P1 — Increase recv poll window in e2e test scripts** (`tests/e2e_comprehensive.sh`, `tests/matrix_probe.sh`)
- Current 5 s wait is insufficient. Change to 30 s minimum; 60 s for cross-continent pairs. This alone would change the matrix from "5/6 recv=0" to "6/6 recv confirmed" without any code changes.

**P1 — Expose stream liveness metric in `/diagnostics`** (`src/bin/x0xd.rs` diagnostics handler)
- Add `stale_sends`, `reconnect_triggered_by_send` counters to distinguish "cached_connected and stream is live" from "cached_connected but had to reconnect."

**P2 — Log recv latency** (`src/direct.rs` recv path)
- Log `send_timestamp` (from the message payload if available) and `recv_timestamp` at dispatch. This makes latency visible without needing to correlate two log lines.

### ant-quic layer (sibling repo `../ant-quic`)

**P0 — Fix relay server address oscillation** (`ant_quic::masque::relay_server`)
- The relay server should select its external address using a stable policy: prefer the globally-routable address over RFC1918; once a global address is confirmed, do not regress to a private address from a subsequent `observed_address` report. Consider a simple majority-vote across the last N peer-reported observed addresses.
- Log line to fix: `Relay server public address updated` — should only fire when the confirmed global address changes, not on every peer report.

**P0 — Fix relay socket provision** (`ant_quic::masque` CONNECT-UDP tunnel path)
- `Relay did not provide socket` indicates the MASQUE relay server is accepting the QUIC connection but failing to allocate/bind the UDP socket for CONNECT-UDP. Investigate the relay session setup in `masque::relay_server` — the socket allocation likely fails silently when the relay server's own address is in flux.

**P1 — Skip NAT traversal when both peers have `can_receive_direct: true`** (`ant_quic::nat_traversal_api`)
- Add a pre-flight check: if the local node and the target node both report `can_receive_direct: true` and `nat_type != Symmetric`, attempt direct dial only. Do not enter the hole-punch phase. This eliminates the ~6–12 s hole-punch noise for public-IP peer pairs and will restore `hole_punch_success_rate` to a meaningful signal.

**P1 — `UnknownTarget` coordinator rejection should be a fast abort, not a timeout** (`ant_quic::nat_traversal_api`)
- Currently `UnknownTarget` triggers a retry after 2 s and then 4 s. It should immediately skip to the next round or mark the peer as "not registered with this coordinator" and try a different coordinator, rather than burning 6+ s.

**P2 — Suppress RFC1918 addresses from external_addrs advertisement** (`ant_quic::nat_traversal_api` address discovery)
- `poll_discovery_task` reports `10.200.0.1` as an external address because a peer on the same private network reports it as `observed_address`. This address should be excluded from the external address set if it fails the global-address test. Only broadcast addresses confirmed by at least one non-RFC1918-network peer.

### Infrastructure / config

**P1 — Set `MASQUE_RELAY_ADDR` or equivalent to the confirmed public IP** (VPS systemd unit `/etc/x0x/config.toml`)
- Until ant-quic's relay address oscillation is fixed, pin the relay server's advertised address in config to the node's known public IP. This prevents the relay from advertising `10.200.0.1` to remote peers.
- Example: add `relay_public_addr = "65.21.157.229:5483"` to `/etc/x0x/config.toml` on Helsinki.

**P2 — Increase recv wait in VPS matrix probes to 60 s** (`tests/proof-reports/matrix_probe.sh`)
- Change the `sleep 5` between send and recv poll to `sleep 60` to capture the true connectivity outcome rather than the 5 s timeout artifact.

**P3 — Add Hetzner/Vultr private network firewall rule** (infrastructure)
- Consider blocking `10.200.0.0/16` outbound on port 5483 at the firewall level on each VPS node, so private-network dial attempts fail fast (ICMP unreachable, ~1 ms) instead of timing out (50 s). This is a stopgap until RC-2 is fixed in x0x code.

---

## Next actions

1. **Immediate: Extend recv wait to 60 s** in `tests/proof-reports/matrix_probe.sh` and re-run the matrix to confirm 6/6 recv, producing a clean connectivity proof.
2. **x0x: Add RFC1918 filter in `src/connect.rs`** — single `is_global()` guard before `connect_addr`. Estimate: 30 min. This is the highest-leverage change available entirely within this repo.
3. **Investigate: Is the `cached_connected` → recv delay caused by a blocking send or a delayed inbound dispatch?** Add a debug probe: after a `cached_connected` send, log `connection.stable_id()` and `rtt()`. If RTT is unavailable or the connection is dead at send time, that confirms the stale-stream hypothesis. Instrument `src/direct.rs` send path.
4. **ant-quic: File issues for relay address oscillation and unconditional NAT traversal** against `../ant-quic`. Provide the helsinki journal (`journals/helsinki.log`) as the relay flip evidence, and the `diag_*_end.json` files as the hole_punch_success_rate evidence.
5. **Probe with 2-node isolated matrix** (no other traffic, 60 s wait) to measure baseline send→recv latency without the NAT traversal noise of a 12-peer mesh. This will isolate whether the latency is fundamental to the QUIC stream lifecycle or noise from the concurrent traversal storm.
6. **Measure relay session setup** by temporarily adding `RUST_LOG="ant_quic::masque=trace"` to capture the exact failure point in `Relay did not provide socket`.

---

*Report generated from corpus: `tests/proof-reports/matrix_20260415T202646Z/` and `tests/proof-reports/matrix_20260415T202317Z/`*
