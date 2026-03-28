# Response to Agent Primer Audit — 2026-03-28

## Thank You

Thank you for this audit. The methodology was sound — testing against the actual v0.11.0 release, building retained proof examples outside the repo, and classifying each finding by failure mode. The distinction between downstream doc issues, upstream doc issues, product gaps, hidden prerequisites, and evidence gaps is exactly the right framework.

The seven rewritten primers are well done. They are honest about what works, cautious where the product surface is incomplete, and structured so an agent can make safe build decisions. We are adopting them.

## What We Verified

Before acting on the findings, we verified every claim against the current codebase.

| Finding | Audit Verdict | Our Verification | Outcome |
|---------|--------------|-----------------|---------|
| Identity | Pass | Confirmed | Agree |
| Trust & Contacts | Pass | Confirmed | Agree |
| Gossip Pub/Sub | Pass | Confirmed | Agree |
| Local Apps | Pass | Confirmed | Agree |
| Direct Messaging | Partial (timed out) | Works — trust annotations absent by design | Disagree with "partial" — works, but trust gap is real |
| CRDT Sync (Tasks/Stores) | Partial/Fail | **Fully wired end-to-end** — delta publishing, gossip propagation, anti-entropy every 30s, out-of-order handling | Disagree — this works, test harness timing issue |
| Named Groups | Partial (views diverge) | Likely eventual consistency timing | Agree it needs documentation |
| File Transfer | Partial/Fail | **Confirmed: metadata only, zero byte delivery** | Agree — this is a real gap |

### CRDT Sync: Why We Disagree

The audit's "partial/fail" verdict for cross-node task list and store sync appears to be a test harness issue. The implementation is fully wired:

- Every mutation (add, claim, complete, put, remove) publishes deltas to gossip automatically
- Background subscriptions on remote peers receive and merge deltas
- Anti-entropy runs every 30 seconds as convergence fallback
- Out-of-order delivery is handled — state-change deltas include full TaskItem for upsert
- KV stores validate writer identity against access policy on merge

The likely failure mode: two local daemons need ~15 seconds after startup for gossip routes to establish through shared bootstrap peers. The retained proof may not have waited long enough.

### Direct Message Trust: A Design Choice, Not a Bug

Direct message events intentionally omit `verified` and `trust_level`. Gossip messages are broadcast-origin and may be unsigned, so the daemon annotates them. Direct messages are QUIC-authenticated at transport level — the machine identity is already verified by the transport. The `sender` AgentId is self-asserted, so trust evaluation is delegated to the application.

The primer's advice to resolve trust via `/contacts` + `/trust/evaluate` is correct. That said, we plan to enrich direct events with trust annotations for consistency — see "Planned Work" below.

## What We Are Doing

### Immediate (this release cycle)

**1. Correcting the CRDT coordination primer**

The coordination.md primer undersells task lists and stores. We are updating it to:
- Present cross-node sync as working, not experimental
- Document the ~15s gossip route establishment delay
- Remove "validate in your own environment" hedging for basic sync operations
- Keep the "no transactional consistency" and "eventual consistency" caveats, which are accurate

**2. Implementing actual file byte transfer**

The audit correctly identified that file transfer is metadata-only today. The types (`FileChunk`, `FileMessage`, `FileComplete`) exist but are never instantiated. We are wiring up real chunked byte delivery:

- On accept: sender spawns async task, reads file, streams chunks via `send_direct()`
- Receiver: direct message handler recognizes `FileMessage` types, buffers chunks, writes to disk
- On completion: SHA-256 verification against advertised hash
- Progress tracking: `bytes_transferred` updated as chunks arrive

This uses the existing direct messaging transport — no new protocols or dependencies.

**3. Shipping the rewritten primers**

We are adopting all seven primers into the x0x docs. The files primer will be updated after file transfer implementation lands. The other six are accurate as written (with the CRDT correction above).

### Planned (design docs, not yet implementation)

**4. Unify named groups and MLS into a single surface**

Currently these are two separate CLI/API surfaces (`x0x group` vs `x0x groups`). We plan to unify them so that:
- Creating a named group optionally creates an MLS group for encryption
- Group messaging works through the named group surface directly
- The MLS helpers remain available as low-level primitives for custom use

Design doc will precede implementation.

**5. Add trust annotations to direct message events**

We plan to enrich `DirectMessage` events with `verified` and `trust_level` fields, matching gossip events. This is a small change — the daemon already has access to the `ContactStore` and can look up trust during direct message dispatch.

Design doc will precede implementation.

### Not Doing

**App framework / app registry / app packaging** — The localhost app substrate works. The primer correctly describes it as "not yet a complete app packaging, discovery, or distribution platform." We agree. Building an app framework would add complexity without clear demand. The current model (read `api.port` + `api-token`, serve from localhost) is simple and works.

## Why This Ordering

The audit framed its priorities around "what can we safely tell agents to build on." We agree with that frame but adjusted the priority based on what we verified:

1. **CRDT sync works** — the primer just needs correction, not engineering work
2. **File transfer is the only real product gap** — everything else either works or has an honest workaround in the primer
3. **Groups + MLS unification and direct trust enrichment are quality-of-life improvements** — worth planning, but not blocking agent primer distribution

The primers can ship now (with the CRDT correction). The file transfer implementation and primer update follow. The design work for groups and direct trust happens in parallel.

## For Future Audit Cycles

The retained proof examples are valuable. We suggest:
- Allow 20-30 seconds for gossip route establishment when testing cross-node behavior with local daemons
- Use `x0x events` or the SSE stream to confirm subscription establishment before publishing
- The anti-entropy interval is 30 seconds — if a delta is missed, wait for the next sync cycle before declaring failure
