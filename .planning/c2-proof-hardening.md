# Phase C.2 — Proof-Hardening Tracker

Status: **closed** — Phase C.2 code landed in commit `5cffeb6`
(`feat(groups): phase C.2 — distributed discovery index via shard gossip`),
and the four proof-hardening items below are now live-proven. See
`tests/proof-reports/PHASE_C2_REPORT.md` for the honest final-state
report.

A real privacy bug was found during this hardening pass —
`ListedToContacts` cards were still being published to the legacy global
bridge topic — and fixed before closure.

---

## A. Shard-specific positive proof — CLOSED

Completed via option 2.

What landed:
- `/groups/discover/nearby` is shard-cache-only
- dedicated live proof in `tests/named_group_c2_live.rs`
- shell hook in `tests/e2e_named_groups.sh`
- archived canonical live runs:
  - `tests/proof-reports/named-groups-c2-hardening-run1.log`
  - `tests/proof-reports/named-groups-c2-hardening-run2.log`
  - `tests/proof-reports/named-groups-c2-hardening-run3.log`

Acceptance satisfied:
- Bob subscribes to a relevant shard
- Alice creates a `PublicDirectory` group
- Bob does **not** manually import the card
- Bob's shard-only `/groups/discover/nearby` shows the stable `group_id`

This closes proof-hardening item A.

---

## B. Live anti-entropy repair proof — CLOSED

Completed with a dedicated pair-harness live test in
`tests/named_group_c2_live.rs`.

What landed to support it:
- test-only daemon config overrides for
  `directory_digest_interval_secs` and
  `group_card_republish_interval_secs`
- own published public cards now seed the local `directory_cache`
- `respond_to_pull(...)` resolves owned groups by stable `group_id`
  (not only local routing key)

Acceptance satisfied:
1. Alice publishes first.
2. Bob subscribes later and initially does **not** have the card.
3. Periodic republish is disabled for the test.
4. Alice advertises from the shard cache on a shortened digest interval.
5. Bob recovers the card via digest/pull and sees it on shard-only
   `/groups/discover/nearby`.

Archived under the same canonical hardening runs listed above.

---

## C. LTC positive + negative delivery proof — CLOSED

Completed in `tests/named_group_c2_live.rs`.

What landed:
- live proof that a `Trusted` contact receives the LTC card via
  `GET /groups/cards/<stable_group_id>`
- live proof that a `Blocked` contact does **not** receive it
- live proof that neither recipient sees the LTC card on public
  `/groups/discover/nearby`
- privacy fix: `ListedToContacts` cards no longer dual-publish onto the
  legacy global discovery topic, and the global listener now drops any
  such leaked cards defensively

Archived in:
- `tests/proof-reports/named-groups-c2-hardening-run1.log`
- `tests/proof-reports/named-groups-c2-hardening-run2.log`
- `tests/proof-reports/named-groups-c2-hardening-run3.log`

---

## D. Subscription persistence across restart — CLOSED

Completed in `tests/named_group_c2_live.rs`.

What landed:
- test-only config override `directory_resubscribe_jitter_ms`
- restartable test harness support in `tests/harness/src/cluster.rs`
- live proof for subscribe → persist file → restart → reload 3
  subscriptions → receive shard-delivered card after restart

Archived in:
- `tests/proof-reports/named-groups-c2-hardening-run1.log`
- `tests/proof-reports/named-groups-c2-hardening-run2.log`
- `tests/proof-reports/named-groups-c2-hardening-run3.log`

---

## Execution notes

- These four proofs are **e2e-shaped** and will live in a new section
  of `tests/e2e_named_groups.sh`, or preferably a dedicated
  `tests/e2e_c2_convergence.sh` that can run in isolation without the
  rest of the named-groups suite's pre-existing environmental noise.
- (C) requires either a contact-bootstrap step in the e2e or importing
  contact cards explicitly via `POST /contacts/add`.
- Proof-hardening should land as its own commit:
  `test(groups): phase C.2 proof-hardening — live shard convergence + LTC + restart`

## Signoff gate — satisfied

The dedicated live suite passed in **3 consecutive clean runs** archived as:
- `tests/proof-reports/named-groups-c2-hardening-run1.log`
- `tests/proof-reports/named-groups-c2-hardening-run2.log`
- `tests/proof-reports/named-groups-c2-hardening-run3.log`

This tracker is complete and remains only as closure evidence.
