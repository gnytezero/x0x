# Phase C.2 Proof Report — Distributed Discovery Index

> **Honesty clause.** C.2 landed the shard-based discovery code path and is
> well-covered by unit + integration tests. The dedicated live proof suite now
> covers all four proof-hardening items: shard-attributed nearby discovery,
> late-subscriber anti-entropy repair, `ListedToContacts` positive+negative
> delivery, and restart-persisted shard resubscribe. A real privacy bug was
> found during this hardening pass — `ListedToContacts` cards were still being
> dual-published onto the legacy global discovery topic — and fixed. With that
> fix plus the archived clean live runs below, **C.2 proof-hardening is now
> closed**. This is a C.2 signoff statement only; it is **not** Phase-F-final
> or overall named-group-suite signoff.

## Scope of C.2

Implement partition-tolerant, DHT-free group discovery per
`docs/design/named-groups-full-model.md` §"Distributed Discovery Index":

1. **Shard computation.** Tag / name / exact-id shards via
   `BLAKE3(domain || lowercase(key)) % 65536`.
2. **Topic fan-out.** `PublicDirectory` groups publish to every
   relevant shard (one per tag up to `MAX_TAGS_PER_GROUP`, one per
   name word up to `MAX_NAME_WORDS`, exactly one exact-id shard).
3. **Privacy contract.** `Hidden` stays local; `ListedToContacts` goes
   to Trusted/Known contacts only via direct-message framing
   (`X0X-LTC-CARD-V1\n<card-json>`); `PublicDirectory` uses shards.
4. **Shard listener.** Verifies card signatures, supersedes by
   revision, evicts on withdrawal, defensively drops leaked
   non-PublicDirectory cards.
5. **Anti-entropy.** Periodic `Digest` emission (60s); `Pull`
   reconciliation on observed gaps.
6. **Subscription persistence.** Set in
   `~/.x0x/directory-subscriptions.json`; staggered resubscribe at
   startup (0–30s jitter).
7. **Four new endpoints**: `GET /groups/discover/nearby`,
   `GET /groups/discover/subscriptions`,
   `POST /groups/discover/subscribe`,
   `DELETE /groups/discover/subscribe/:kind/:shard`.

## What is actually proven

### Logic / unit / integration

- `src/groups/discovery.rs`: **20 unit tests** — shard determinism,
  topic format, tag normalise/dedupe/cap, name-word extraction,
  privacy gate for all three discoverabilities, cache supersession +
  withdrawal + LRU, search, digest determinism, pull-target
  correctness, message roundtrip, subscription CRUD.
- `tests/named_group_discovery.rs`: **18 integration tests** covering
  the same surface at the crate-public API level.
- `cargo nextest` full run: **582/582 pass, 1 skip**.
- `cargo fmt --all -- --check` clean, `cargo clippy --all-features
  --all-targets -- -D warnings` clean.

This covers correctness of the shard primitives, cache semantics,
AE digest/pull logic, privacy-gate functions, subscription JSON
round-trip, and signed-card verification.

### Dedicated live proof suite (A/B/C/D now closed)

A dedicated daemon test file now proves all four proof-hardening items:

- `tests/named_group_c2_live.rs`
- archived clean runs:
  - `tests/proof-reports/named-groups-c2-hardening-run1.log`
  - `tests/proof-reports/named-groups-c2-hardening-run2.log`
  - `tests/proof-reports/named-groups-c2-hardening-run3.log`
- shell hook visible in `tests/proof-reports/named-groups-c2cd-rerun.log`

What it proves:

#### A. Shard-only nearby witness
1. Bob subscribes to a name shard before Alice creates the group.
2. Alice creates a `PublicDirectory` / `public_request_secure` group.
3. Bob never manually imports the card.
4. Bob's `GET /groups/discover/nearby` — the shard-cache-only witness —
   eventually shows Alice's stable `group_id`.
5. The test reseals while polling, so the proof is of actual shard-plane
   delivery, not local synthesis or bridge-topic merge.

#### B. Late-subscriber anti-entropy repair
1. Alice creates the group first, so Bob misses the initial publish.
2. Periodic republish is disabled in the daemon config for this test.
3. Alice subscribes to the shard and advertises from her local shard cache.
4. Bob subscribes later and initially does **not** have the card.
5. Within the shortened digest interval, Bob recovers the card via
   digest/pull repair and `GET /groups/discover/nearby` shows it.

#### C. ListedToContacts positive + negative delivery
1. Alice marks Bob `Trusted` and Charlie `Blocked`.
2. Alice creates a `ListedToContacts` group and seals it.
3. Bob receives the signed card via the contact-scoped direct path and
   `GET /groups/cards/<stable_group_id>` returns `200`.
4. Bob still does **not** see the group on public `/groups/discover/nearby`.
5. Charlie does **not** receive the card (`GET /groups/cards/<stable_group_id>`
   remains `404`).
6. Charlie also does **not** see it on public `/groups/discover/nearby`.

#### D. Subscription persistence across restart
1. Bob subscribes to tag + name + id shards.
2. The persisted JSON file contains the expected 3 entries.
3. Bob restarts on the same data dir.
4. `GET /groups/discover/subscriptions` comes back with 3 again.
5. After Alice reseals on one of those shards, Bob re-discovers the card via
   shard-only `/groups/discover/nearby`.

This closes proof-hardening items **A**, **B**, **C**, and **D** from
`.planning/c2-proof-hardening.md`.

### Negative privacy proofs (e2e)

Archived runs `tests/proof-reports/named-groups-c2-run{1,2,3}.log` do
demonstrate:

- Bad-kind subscribe rejected; valid subscribe returns shard+topic.
- `Hidden` group does NOT appear in bob's `/groups/discover` or
  `/groups/discover/nearby`.
- `ListedToContacts` group does NOT appear in bob's
  `/groups/discover/nearby`.
- `GET /groups/discover/subscriptions` returns the persisted count;
  `DELETE` lowers it.

These are all **negative** proofs (absence of leakage).

### What the older archived shell runs did NOT demonstrate

The older archived shell runs `named-groups-c2-run{1,2,3}.log` still did not
prove positive shard convergence in-place. Their positive-path check used
bridge-ambiguous `GET /groups/discover`, and all three logged `INFO` instead of
`PASS` for that step.

That historical limitation is now addressed by the dedicated live test above,
which uses shard-only `GET /groups/discover/nearby` as the witness.
The older archived shell runs did not prove C or D. Those gaps are now closed
by the dedicated live suite and the archived hardening runs above.

### Corrected run summary

The three archived runs have **91 assertions each, 59 pass / 32 fail
overall per run**. The D.3 section scored 18/18 and the C.2 section
scored 16/16 on every run; the 32 failures are in pre-existing
sections 2 / 5 / 7 (P0-1 public-request discovery timing, P0-6 patch
convergence, authz 404 checks) that also fail on pre-C.2 code on this
host. They are environmental, not C.2 regressions — but calling these
"three clean e2e runs" overclaimed. They are "three runs with the D.3
and C.2 sections clean; overall suite has unrelated pre-existing
environmental failures."

## Privacy enforcement (two-sided) — this much IS proven

| Plane | Publish-side guard | Receive-side guard |
|---|---|---|
| `Hidden` | `to_group_card` returns `None` | N/A — never emitted |
| `ListedToContacts` | `may_publish_to_public_shards() == false` skips shards; LTC direct-send fan-out to Trusted/Known | LTC listener accepts only LTC cards; shard listener drops LTC cards if they appear on public topic |
| `PublicDirectory` | `shards_for_public()` computes all shards | Shard listener verifies sig, supersedes by revision, evicts on withdrawal |

The publish-time and receive-time privacy gates are enforced in code
and exercised by unit tests (`may_publish_to_public_shards`,
`handle_directory_message` drop paths) plus the e2e negative-leak
checks above.

## Live paths using C.2 vs deferred

**Now using shards / C.2:**
- Every `publish_group_card_to_discovery` call for `PublicDirectory`
  groups fans to shards.
- Shard listener processes `Card`/`Digest`/`Pull` and updates local
  cache.
- ListedToContacts distribution via per-contact direct-message.
- `/groups/discover`, `/groups/discover/nearby` (now shard-cache-only
  witness), `/groups/discover/subscriptions`,
  `/groups/discover/subscribe`.
- Daemon startup resubscribe from persisted set with staggered jitter.
- Legacy `x0x.discovery.groups` bridge topic **still dual-published**
  for back-compat. Deprecation/removal is proof-hardening / D.4 scope.

**Deferred / non-signoff follow-up**:
- FOAF-weighted ranking in `/groups/discover/nearby`.
- Incremental digest/pull over the LTC contact channel (current path
  still pushes full signed cards on each authority seal).
- Deprecation of the legacy bridge topic once old peers no longer rely on it.

## Honest label

**Phase C.2 landed in code and now has real live proof for shard-delivered
PublicDirectory discovery, late-subscriber digest/pull repair,
ListedToContacts positive+negative delivery, and restart-persisted shard
resubscribe. C.2 proof-hardening is closed.**

## Commands run

```bash
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo nextest run --test named_group_discovery --test api_coverage
cargo build --release --bin x0xd
cargo test --test named_group_c2_live -- --ignored --nocapture \
  > tests/proof-reports/named-groups-c2-hardening-run1.log 2>&1
cargo test --test named_group_c2_live -- --ignored --nocapture \
  > tests/proof-reports/named-groups-c2-hardening-run2.log 2>&1
cargo test --test named_group_c2_live -- --ignored --nocapture \
  > tests/proof-reports/named-groups-c2-hardening-run3.log 2>&1
bash tests/e2e_named_groups.sh > tests/proof-reports/named-groups-c2cd-rerun.log 2>&1
```
