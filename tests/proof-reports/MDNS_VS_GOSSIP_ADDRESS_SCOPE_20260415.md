# mDNS vs Gossip: Address-Scope Analysis

Date: 2026-04-15
Author: connectivity investigation (x0x @ 0.17.0 instrumented build)
Companion reports: `CONNECTIVITY_ULTRATHINK_20260415.md`, `matrix_20260415T202646Z/`

## Thesis the user proposed (verbatim)

> we do have mdns but that finds local addresses, so we never have a need to
> share private addresses AFAIK. Please analyse this part before we fix.

Verdict: **The thesis is correct.** Gossip announcements must carry only
globally-routable addresses. mDNS (owned by ant-quic) already handles LAN-scope
discovery by its own link-local delivery. We currently leak LAN/private
addresses on three outbound channels and dial them on receive — this is the
sole cause of the 50 s direct-dial black holes observed in the VPS matrix.

---

## The two address channels and what each one should carry

| Channel | Transport | Who sees it | Should advertise |
|---|---|---|---|
| **mDNS** (ant-quic owned) | multicast `224.0.0.251:5353` / `ff02::fb` | Same link (same LAN / same VPC) | **Everything** (loopback optional, link-local, private, global) — addresses are only useful to LAN peers, and the delivery channel is link-scoped |
| **Gossip identity announcement / agent card / presence beacon** | x0x pub/sub over QUIC, propagated globally | Every agent on the network, anywhere | **Globally routable only** (public v4, global v6, UPnP-mapped, QUIC-observed external) |

Why the split:
- A peer 5 000 km away cannot reach our `10.200.0.1`. Advertising it to them
  creates a dead-end dial that x0x currently spends 50 s timing out on.
- A peer on our LAN doesn't need gossip to find us — mDNS already did.
- Identity (AgentId / PeerId) is derived from ML-DSA-65 public keys, not from
  addresses. Filtering addresses does not weaken trust or verification.

## What ant-quic gives us for free

`../ant-quic/src/reachability.rs` exports:

```rust
pub enum ReachabilityScope { Loopback, LocalNetwork, Global }
pub fn socket_addr_scope(addr: SocketAddr) -> Option<ReachabilityScope>;
```

with full IPv4 + IPv6 coverage:
- **Loopback**: 127/8, ::1
- **LocalNetwork**: RFC1918 (10/8, 172.16/12, 192.168/16), link-local
  (169.254/16 + fe80::/10), ULA (fc00::/7, inc. fd00::/8)
- **Global**: everything else, including CGNAT (100.64/10) — note CGNAT is
  *routable* but not *globally reachable*; see caveat below

This is exactly the primitive we need. We do not have to write our own.

### Small gap to note

`socket_addr_scope` classifies CGNAT (100.64/10) as **Global**. ant-quic's
scope enum does not distinguish "private" from "carrier-grade NAT'd". x0x's
existing `is_globally_routable()` (src/lib.rs:265) does catch CGNAT.
Recommendation: filter using **both** — `socket_addr_scope == Global` **and**
`is_globally_routable` — so we retain the CGNAT exclusion.

---

## The leaks in x0x today (grounded in code)

Three outbound channels copy `ant-quic NodeStatus.external_addrs` verbatim.
ant-quic's `external_addrs` currently includes RFC1918 entries (confirmed in
Tokyo's live snapshot: `"10.200.0.1:5483"` sitting next to the public v4 and
global v6). All three x0x channels then propagate that privately to every
agent in the global gossip mesh.

### Leak 1 — Identity heartbeat (every 5 min, gossiped globally)

**File**: `src/lib.rs:609–670` — `HeartbeatContext::announce()`.

```rust
let mut addresses = match self.network.node_status().await {
    Some(status) if !status.external_addrs.is_empty() => status.external_addrs,
    ...
};
// ...augmentation with collect_local_interface_addrs() and UDP-trick v6 probe...

// Strip only truly unusable addresses (port 0, unspecified, loopback).
// Private/LAN addresses are kept for same-network connectivity.        <-- design comment
addresses.retain(|a| a.port() > 0 && !a.ip().is_unspecified() && !a.ip().is_loopback());
```

The comment at lines 669–670 **explicitly states** private addresses are kept
intentionally. That design predates ant-quic's first-party mDNS and is now
harmful.

### Leak 2 — Explicit `announce_identity` (REST-triggered)

**File**: `src/lib.rs:1815–1883` — `Agent::announce_identity()`. Identical
pattern to Leak 1, including the same retained-by-design comment at 1882–1883.

### Leak 3 — Agent card generation (GET /agent/card)

**File**: `src/bin/x0xd.rs:3092`:

```rust
card.addresses = ns.external_addrs.iter().map(|a| a.to_string()).collect();
discover_local_card_addresses(ns.local_addr.port(), &mut card.addresses);
```

Cards are copy-pasteable links people share out of band (email, Slack,
`x0x://agent/...`). A card generated inside a Vultr VPC can embed
`10.200.0.1:5483` and then be sent to someone in London. They'll dial it for
50 s before giving up.

### Non-leak but worth normalising — `/status` endpoint

**File**: `src/bin/x0xd.rs:2660–2707`. Manual filters exclude loopback and
link-local but keep RFC1918. This output is locally diagnostic, so shipping
the unfiltered list is defensible — but for consistency and to avoid
operators mistaking it for propagation data, filtering makes sense here too.

### Diagnostics endpoint (intentional pass-through)

**File**: `src/bin/x0xd.rs:10507` — `/diagnostics/connectivity` (added this
session). Deliberately returns the raw unfiltered list so we can **see** what
ant-quic presents. This is the right behaviour for diagnostics and should
stay as-is.

## The inbound side: dialing dead addresses

When a remote announcement arrives, `IdentityAnnouncement.addresses` is
written straight into `identity_discovery_cache` in at least these sites:

- `src/lib.rs:766` (DiscoveredAgent construction in `announce()`)
- `src/lib.rs:1925` (post-`announce_identity` self-insert)
- `src/lib.rs:2145, 2966, 2998` (listener + import paths)

Then `Agent::connect_to_agent` (lib.rs:1081) iterates `info.addresses` and
dials each one. Its outer `tokio::time::timeout(8s, network.connect_addr)`
does not propagate to ant-quic's internal 50 s per-candidate timeout, which
is why we saw `dur_ms=50006` in the matrix journals.

Today, nothing filters inbound addresses. A node running a future "fixed"
build that only advertises Global will still be force-fed LAN addresses from
older nodes on the mesh until every node is upgraded.

---

## The mDNS side is already correct — no action needed

From the live diagnostics snapshot on every deployed node:

```json
"mdns": { "browsing": true, "advertising": true, "discovered_peers": N }
```

ant-quic is running the full mDNS stack and auto-connecting to discovered
peers (per its `auto_connect` policy). LAN discovery does not require help
from x0x, and mDNS's link-local delivery is the correct scope for publishing
LAN addresses.

On VPS nodes, `discovered_peers: 0` is expected (a single x0x per datacenter
in our testnet). If two x0x nodes later run in the same Vultr VPC, ant-quic's
mDNS should discover them and prefer the 10.x path — without any gossip
involvement.

## Does this break anything?

Audit of scenarios where filtering private addrs out of gossip might hurt:

- **Two agents on the same LAN** → mDNS handles discovery; LAN path is kept.
- **Two agents in the same VPC (e.g. both Vultr Tokyo)** → mDNS over the VPC
  internal multicast (if reachable). Fallback: public addresses, minor latency
  cost (still sub-100ms in-region), zero connectivity cost.
- **Two agents in Docker containers on the same bridge** → mDNS typically
  doesn't cross bridges; these agents find each other via the daemon's
  published public address. No regression vs today (private addrs were
  already useless there).
- **Developer running two daemons on localhost** → loopback is a separate
  scope; today's filter already drops loopback. Unchanged.
- **Identity verification / trust** → unaffected. Identity is cryptographic
  (ML-DSA-65 public key), not address-based. Filtering addresses only changes
  *where* we try to connect, not *who* we verify.

Conclusion: **no loss of functionality**, measurable latency improvement.

---

## Design proposal

### New invariant

> Only `ReachabilityScope::Global` addresses that also pass
> `is_globally_routable()` (CGNAT exclusion) may appear in an
> `IdentityAnnouncement`, `AgentCard`, or presence beacon.

### Where to enforce

| Location | What | Priority |
|---|---|---|
| `src/lib.rs:669–670` (heartbeat) | replace the `retain` with a scope filter | P0 |
| `src/lib.rs:1882–1883` (announce_identity) | same | P0 |
| `src/bin/x0xd.rs:3092` (agent card) | filter `external_addrs` before assigning | P0 |
| `src/lib.rs` inbound listener (insertion sites 766, 1925, 2145, 2966, 2998) | drop non-Global entries before caching | P1 (defense in depth) |
| `src/connectivity.rs::ReachabilityInfo::from_discovered` | sort Global v6, Global v4, skip LocalNetwork unless we are on a matching link | P1 |
| `src/lib.rs::collect_local_interface_addrs` | keep LAN entries for mDNS / local hints, but tag them so callers can split | P2 |

### Re-use, don't reimplement

Plan:

```rust
// new: src/lib.rs or src/connectivity.rs
use ant_quic::reachability::{socket_addr_scope, ReachabilityScope};

/// True if `addr` is safe to publish over the global gossip mesh.
pub fn is_publicly_advertisable(addr: SocketAddr) -> bool {
    matches!(socket_addr_scope(addr), Some(ReachabilityScope::Global))
        && is_globally_routable(addr.ip()) // retains CGNAT exclusion
        && addr.port() > 0
}
```

Then the three leak sites become a one-liner filter each.

### Dialing order (P1 refinement of `ReachabilityInfo`)

1. Global v6 (AAAA wins — fastest path, no NAT mangling)
2. Global v4
3. LocalNetwork addresses **only** if `info.mdns_discovered` is true (i.e.
   this peer reached us on the same link)
4. Short per-address timeout (3 s is enough; ant-quic's own fallback ladder
   handles the rest)

### Test plan

Unit:
- `test_announcement_excludes_private_addresses` — mixed input, only Global
  entries in the announcement.
- `test_agent_card_excludes_private_addresses` — same for `AgentCard`.
- `test_heartbeat_addresses_filter_cgnat` — 100.64.x.x in input → not in output.

Integration:
- Redeploy instrumented build (now with the filter).
- Re-run `bash tests/proof-reports/matrix_probe.sh helsinki nyc tokyo`.
- Success criteria:
  - Zero `direct dial failed ... 10.x.x.x ... dur_ms=50` lines in any journal.
  - `send→recv` latency drops from ~70 s to <2 s on all 6 ordered pairs.

## What this does NOT fix (for clarity)

1. **ant-quic hole-punch firing on already-reachable public peers**
   (`hole_punch_success_rate: 0.04–0.075`). This stays an ant-quic issue.
2. **MASQUE relay `did not provide socket`** and `bytes_forwarded: 0` despite
   `is_relaying: true`. Also an ant-quic issue.
3. **50 s per-candidate dial timeout in ant-quic**. The x0x-layer filter
   removes *most* cases where this matters; an outer x0x-side short timeout
   would mitigate the remainder. Root fix is in ant-quic.

These three ant-quic issues warrant separate upstream bug reports with the
journals from `matrix_20260415T202646Z/` attached.

## Summary

The analysis confirms the user's thesis without caveat. We have a clean
separation of responsibilities:

- **mDNS (in ant-quic)** handles LAN-scope discovery and should keep
  publishing LAN/loopback addresses on its link-local multicast channel.
- **Gossip / cards / presence (in x0x)** must advertise only global addresses
  so remote peers never waste dial budget on unreachable RFC1918 entries.

Three specific code sites in x0x leak private addresses into the global
channel today, because of a 2024-era design choice that predated ant-quic's
first-party mDNS. The primitive we need is already exported from ant-quic
(`ReachabilityScope` + `socket_addr_scope`). Fix is a one-line filter at each
of the three leak sites, plus a defensive inbound filter for robustness on a
mixed-version mesh.

No security or functionality regression. Expected result: send→recv latency
drops from ~70 s to <2 s on the VPS matrix.
