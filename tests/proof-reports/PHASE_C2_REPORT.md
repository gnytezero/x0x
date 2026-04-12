# Phase C.2 Proof Report — Distributed Discovery Index

> **Honesty clause.** C.2 delivers the shard-based discovery plane and
> the privacy contract (Hidden / ListedToContacts / PublicDirectory).
> It does NOT claim full named-group support — that requires D.4 + E + F.

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
7. **Four new endpoints**:
   - `GET /groups/discover/nearby`
   - `GET /groups/discover/subscriptions`
   - `POST /groups/discover/subscribe`
   - `DELETE /groups/discover/subscribe/:kind/:shard`

## Explicit claims

1. **Deterministic sharding.** `shard_of(kind, key)` is pure-BLAKE3 and
   reproducible across peers; topic format
   `x0x.directory.{tag|name|id}.{N}` is stable.
2. **Hidden no-leak.** `Hidden` groups never reach any directory topic —
   proven by unit test `hidden_must_not_publish_to_public_shards` and by
   defensive guard in `publish_group_card_to_discovery_inner`.
3. **ListedToContacts no-leak.** `ListedToContacts` groups never reach
   public shards — proven by unit test
   `listed_to_contacts_must_not_publish_to_public_shards` and by the
   `may_publish_to_public_shards` gate at publish.
4. **PublicDirectory fan-out.** Tags + name words + exact-id all emit
   `DirectoryMessage::Card` to their shards.
5. **Signed-card verification on receive.** Unsigned cards or cards
   with bad signatures are dropped. Covered by `verify_signature()` +
   `handle_directory_message`.
6. **Revision supersession.** `DirectoryShardCache::insert` rejects
   lower revisions, replaces on higher, evicts on withdrawal.
7. **Anti-entropy correctness.** `pull_targets` returns the correct set
   of `group_id`s given a peer's digest (missing + stale), proven by
   unit test `pull_targets_finds_missing_and_stale`.
8. **Persistence.** `SubscriptionSet` round-trips via JSON
   (test `subscription_set_json_roundtrip`); daemon reloads the set on
   startup and resubscribes with jitter.

## Defensive privacy enforcement (two-sided)

| Plane | Publish-side guard | Receive-side guard |
|---|---|---|
| `Hidden` | `to_group_card` returns `None` (no card produced) | N/A — never emitted |
| `ListedToContacts` | `may_publish_to_public_shards() == false` skips shards; LTC direct-send fan-out to Trusted/Known | LTC listener accepts only ListedToContacts cards; shard listener drops LTC cards if they appear on public topic |
| `PublicDirectory` | `shards_for_public()` computes all shards | Shard listener verifies sig, supersedes by revision |

## Live paths using C.2 vs deferred

**Now using shards / C.2:**
- Every `publish_group_card_to_discovery` call for
  `PublicDirectory` groups fans to shards.
- Shard listener processes `Card`/`Digest`/`Pull` and updates local
  cache.
- ListedToContacts distribution via per-contact direct-message.
- `/groups/discover`, `/groups/discover/nearby`,
  `/groups/discover/subscriptions`, `/groups/discover/subscribe`.
- Daemon startup resubscribe from persisted set.
- Legacy `x0x.discovery.groups` bridge topic still dual-published for
  back-compat (deprecated; removal planned for D.4).

**Deferred:**
- FOAF-weighted ranking in `/groups/discover/nearby` — current returns
  unweighted reachable set.
- Incremental digest/pull on the LTC contact channel — current pushes
  full signed cards on each authority seal.
- Deprecation/removal of the legacy bridge topic.

## Commands run

```bash
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo nextest run --lib --test named_group_state_commit \
  --test named_group_discovery --test api_coverage
cargo build --release --bin x0xd --bin x0x
bash tests/e2e_named_groups.sh > tests/proof-reports/named-groups-c2-run{1,2,3}.log 2>&1
```

## Unit + integration evidence

- `src/groups/discovery.rs`: **20 unit tests** (shard determinism,
  topic format, tag normalise/dedupe/cap, name-word extraction,
  privacy rules, cache supersession/withdrawal/LRU, search,
  digest determinism, pull-target logic, message roundtrip,
  subscription CRUD).
- `tests/named_group_discovery.rs`: **18 integration tests** (topic
  format, shard determinism, fan-out coverage, name-shard matching,
  privacy guarantees for all 3 discoverabilities, max-tags/name-words
  caps, exactly-one id shard, cache behaviour, AE pull targets,
  signed-card roundtrip, message variants, subscription JSON
  round-trip, search, distinct-id-shard routing).
- `cargo nextest`: all lib + D.3 + C.2 + api_coverage tests pass.
- `cargo clippy -D warnings`: clean.

## E2E evidence

`tests/e2e_named_groups.sh` new **C.2 section** on three fresh daemons:
- Hidden group creation + privacy check (never in bob's
  `/groups/discover` or `/groups/discover/nearby`).
- `POST /groups/discover/subscribe` rejects bad kind; accepts valid
  tag/name/id with key; returns shard + topic.
- `GET /groups/discover/subscriptions` lists active subs.
- PublicDirectory creation + seal + cross-peer visibility via shards
  (best-effort given pre-existing host-dependent gossip timing).
- ListedToContacts group does NOT appear in bob's
  `/groups/discover/nearby`.
- `DELETE /groups/discover/subscribe/:kind/:shard` removes entry and
  lowers subscription count.

Three archived clean runs: `tests/proof-reports/named-groups-c2-run{1,2,3}.log`.

## What's next

Phase **E** — public group behavior (open join, public read, moderated
write, admin-only announce write, banned-peer rejection). Per approved
plan: D.3 → **C.2 (now landed)** → E → D.4 → F.
