## Summary

MASQUE relay in ant-quic 0.26.7 is wired and reports itself active
(`is_relaying: true`, 4–5 open `relay.sessions` across three independent
public-IP nodes), but **`bytes_forwarded: 0` on all of them** — no payload
has ever been forwarded by any of these relays.

In parallel, the relay server's advertised public address oscillates at
~1 Hz between the node's public v4, its global v6, and (on Vultr) its
RFC1918 interior v4 — never stabilising. Callers that reach the end of the
connection-strategy ladder receive the error
`Relay: Connection error: Relay did not provide socket`.

Net effect: the last safety net in the direct → hole-punch → relay chain
does not exist in practice, even though the node believes it is relaying.

## Environment

- ant-quic 0.26.7
- x0x 0.17.0 (instrumented build with `RUST_LOG=ant_quic=info`)
- 3 bootstrap VPS nodes: Helsinki (Hetzner, eu-central), NYC (DO,
  us-east), Tokyo (Vultr, ap-northeast)
- Dual-stack `[::]:5483` on all three

## Evidence

### End-state relay snapshot (from x0x `/diagnostics/connectivity`)

| Node     | is_relaying | sessions | **bytes_forwarded** | external_addrs                                                                                   |
|----------|------------:|---------:|--------------------:|--------------------------------------------------------------------------------------------------|
| helsinki | true        | 5        | **0**               | `65.21.157.229:5483`, `[2a01:4f9:c012:684b::1]:5483`, `10.60.3.1:5483`                           |
| nyc      | true        | 4        | **0**               | `142.93.199.50:5483`, `[2604:a880:400:d1:0:3:7db3:f001]:5483`                                    |
| tokyo    | true        | 4        | **0**               | `45.77.176.184:5483`, `[2401:c080:1000:4c32:5400:5ff:fed9:9737]:5483`, `10.200.0.1:5483`         |

Every node reports active relay sessions but has never forwarded a byte.

### Symptom 1 — public-address oscillation at ~1 Hz

`ant_quic::masque::relay_server` rewrites its own advertised public address
on a ~1-second cadence, cycling through v4 / v6 / (on Vultr) the RFC1918
interior address. Representative excerpt (Tokyo):

```
2026-04-15T20:29:47Z INFO ant_quic::masque::relay_server: Relay server public address updated old=10.200.0.1:5483 new=[2401:c080:1000:4c32:5400:5ff:fed9:9737]:5483
2026-04-15T20:29:48Z INFO ant_quic::masque::relay_server: Relay server public address updated old=[2401:c080:1000:4c32:5400:5ff:fed9:9737]:5483 new=45.77.176.184:5483
2026-04-15T20:29:48Z INFO ant_quic::masque::relay_server: Relay server public address updated old=45.77.176.184:5483 new=[2401:c080:1000:4c32:5400:5ff:fed9:9737]:5483
2026-04-15T20:29:48Z INFO ant_quic::masque::relay_server: Relay server public address updated old=[2401:c080:1000:4c32:5400:5ff:fed9:9737]:5483 new=10.200.0.1:5483
2026-04-15T20:29:48Z INFO ant_quic::masque::relay_server: Relay server public address updated old=10.200.0.1:5483 new=[2401:c080:1000:4c32:5400:5ff:fed9:9737]:5483
```

NYC (no Vultr-style interior) toggles v4 ↔ v6 at the same cadence:

```
2026-04-15T20:29:56Z INFO ant_quic::masque::relay_server: Relay server public address updated old=142.93.199.50:5483 new=[2604:a880:400:d1:0:3:7db3:f001]:5483
2026-04-15T20:29:56Z INFO ant_quic::masque::relay_server: Relay server public address updated old=[2604:a880:400:d1:0:3:7db3:f001]:5483 new=142.93.199.50:5483
... (continues every ~1s)
```

Two consequences:

- The relay never stabilises on a single advertised address, so clients
  racing to CONNECT-UDP have a moving target.
- It suggests the relay server is being fed an unordered stream of
  `external_addrs` snapshots and treating each as authoritative, rather
  than picking a preferred one (e.g. Global v6 > Global v4 > LocalNetwork).

Note that `10.200.0.1` being surfaced here at all is itself questionable —
an RFC1918 address is not a useful "public" relay endpoint by definition.

### Symptom 2 — "Relay did not provide socket" in the strategy chain

When x0x's `/direct/send` → `connect_to_agent` exhausts direct and
hole-punch, ant-quic returns:

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

The `Relay did not provide socket` error appears consistently whenever the
fallback ladder reaches the relay step. Correlated with Symptom 1, our
reading is that the client attempts CONNECT-UDP against a relay address
that has already been overwritten by the time the stream opens.

## Expected behaviour

- `bytes_forwarded` should be non-zero when there are active sessions and
  at least one peer is relaying traffic (or the session should be closed).
- The relay server should publish a **stable** public address, choosing the
  best scope (Global v6 → Global v4 → mapped-external → …) and *only*
  updating when that selection demonstrably changes (e.g. a new
  OBSERVED_ADDRESS frame invalidates the current choice).
- RFC1918 / link-local / ULA addresses should be excluded from the relay
  server's advertised public address entirely — they cannot serve as a
  globally-reachable relay endpoint.
- The CONNECT-UDP path should return a socket (or a concrete "no upstream"
  error) deterministically, not a generic "did not provide socket" symptom
  of an internal race.

## Hypothesis (non-authoritative)

- `masque::relay_server` may be subscribed to the full
  `NodeStatus::external_addrs` change stream and rewriting its public
  address on every entry (rather than selecting a preferred one).
- The 1 Hz oscillation appears linked to `ant_quic::candidate_discovery`
  periodic interface scans and/or OBSERVED_ADDRESS frame arrivals from
  multiple peers.
- `Relay did not provide socket` is likely a race where a CONNECT-UDP
  session's socket binding is torn down mid-establishment because the
  advertised address has already been replaced.

## Impact

- Removes the third rung of ant-quic's advertised
  direct → hole-punch → relay ladder.
- Combined with the elevated hole-punch failure rate (see companion issue
  A), this is why x0x VPS nodes see ~60–100 s send→recv latency tails
  despite all peers having global IPs — there is no working fallback.

## Reproduction

1. Run ant-quic 0.26.7 with relay enabled on two or more public-IP nodes.
2. Observe `ant_quic::masque::relay_server: Relay server public address
   updated` lines on ~1 Hz cadence.
3. Trigger a scenario where direct + hole-punch fail (for example, by
   pointing a client at a known-unreachable candidate list) and watch the
   resulting error — it will be `Relay: Connection error: Relay did not
   provide socket` despite the server's own `is_relaying: true`.

## Artefacts

Journal windows from 2026-04-15T20:25Z–20:30Z and per-node diagnostic
snapshots (from x0x `/diagnostics/connectivity`) available in
`tests/proof-reports/matrix_20260415T202646Z/` of the x0x repo.

## Priority

High. Without a working relay, ant-quic cannot guarantee 100 %
connectivity in any case where both sides' direct/hole-punch paths fail —
which, per the companion issue A, is currently frequent.
