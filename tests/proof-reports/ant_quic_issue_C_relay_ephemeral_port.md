## Summary

In ant-quic 0.26.9, the MASQUE relay fallback path is attempting CONNECT-UDP
to the peer's **ephemeral outbound traversal port** (e.g. `:37616`) instead
of the peer's listening socket (`:5483`). The peer never bound that
ephemeral port as a listener, so relay initiation fails with:

```
Relay: Connection error: Failed to initiate relay connection:
  invalid remote address: [2001:19f0:4401:346:5400:5ff:fed9:9735]:37616
```

This is (we believe) a new regression or at least newly-visible symptom in
0.26.9. The prior 0.26.7 error was the opaque
`Relay: Connection error: Relay did not provide socket` — your 0.26.9
improvements around relay state reporting (tracked in #164) now surface a
specific and debuggable failure, which is good — but the underlying target
selection is wrong.

## Environment

- ant-quic 0.26.9
- x0x 0.17.0 (instrumented build with
  `RUST_LOG=x0x::connect=debug,x0x::direct=debug,ant_quic=info`)
- 6 bootstrap VPS nodes: helsinki / nyc / tokyo / sfo / nuremberg /
  singapore, all with public v4 + global v6, bound `[::]:5483`
- Matrix probe issued 30 ordered-pair `/direct/send` requests across the
  mesh with ~15 s between pairs

## Evidence

### Full error chain (tokyo journal, 20260416T11:15:25Z)

```
Endpoint error: All connection strategies failed:
  DirectIPv4: Timeout;
  DirectIPv6: Happy Eyeballs timed out;
  HolePunch { round: 1 }: Operation timed out;
  HolePunch { round: 2 }: Operation timed out;
  HolePunch { round: 3 }: Operation timed out;
  HolePunch { round: 3 }: Operation timed out;
  Relay: Connection error: Failed to initiate relay connection:
    invalid remote address: [2001:19f0:4401:346:5400:5ff:fed9:9735]:37616
```

### Why `:37616` is the wrong target

- The peer's listening socket is `:5483` (the x0x well-known port; also
  the address they publish in their identity announcement).
- `:37616` is an ephemeral port — almost certainly an OBSERVED_ADDRESS
  frame value from an outbound connection attempt the peer made earlier
  (their NAT-mapped source port, not their listener).
- No peer-level listener exists on that port on the remote side, so the
  relay's CONNECT-UDP request is rejected as an invalid remote.

### Operational impact

With this bug present, the 30-pair matrix saw only 6 eventual recipients
(within 25 min of the sends) out of 30 sends. Before this regression
surfaced (0.26.7 + x0x scope filter), the 3-node matrix delivered 6/6 at
a 2–3 min tail. So in 0.26.9, once a fresh connect-after-churn scenario
escalates past DirectIPv4 + Happy Eyeballs + HolePunch, relay is unable
to rescue it.

### Positive signal (worth noting)

Relay **is** forwarding bytes when the target is correct. Per our
`/diagnostics/connectivity`:

| Node | bytes_forwarded |
|------|-----------------|
| singapore | 771 567 |
| sfo | 291 600 |
| tokyo | 156 000 |
| nyc | 13 200 |

So the relay subsystem is healthy end-to-end; this issue is scoped to
**target-address selection**.

## Expected behaviour

The relay's CONNECT-UDP target for a peer should be:
1. The peer's advertised listening address (from their identity
   announcement / agent card / gossip cache), **not** their ephemeral
   OBSERVED_ADDRESS port.
2. The listening port (`5483` in our deployment) should take precedence
   over any ephemeral ports learned during NAT traversal.

## Hypothesis

When ant-quic builds the relay's peer address list, it may be taking from
a pool that includes OBSERVED_ADDRESS candidates (used correctly for hole
punching) without distinguishing "listener" (valid relay target) from
"ephemeral source" (valid only for the short-lived punch). The filter
at `candidate_discovery` → `relay_target_addr` likely needs to restrict
to candidates that were advertised by the peer as listeners.

## Reproduction

1. Deploy ant-quic 0.26.9 to ≥ 3 public-IP nodes with relay enabled.
2. Connect all to the global bootstrap mesh.
3. From node A, import node B's card, then have A `/direct/send` to B
   **after** the gossip runtime has churned (e.g. from multiple card
   imports or peer additions).
4. Wait for DirectIPv4 + IPv6 + HolePunch rounds to time out.
5. Observe `Relay: Connection error: Failed to initiate relay connection:
   invalid remote address: [<peer v6>]:<ephemeral>` in the journal.

## Priority

Moderate-to-high. In the steady-state user path (client joins bootstrap,
talks to mesh) the direct connection stays warm and this code path is
never exercised. But under churn it dominates failures and prevents the
"100 % connectivity" guarantee at scale.

## Companion evidence

Per-node diagnostics JSON snapshots, 30 per-pair journal excerpts, and
the matrix summary CSV are in
`x0x/tests/proof-reports/matrix_20260416T105717Z/`. Journal window was
UTC 11:15 – 11:25 on 2026-04-16.
