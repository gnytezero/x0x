# Phase F Review — hostile status after D.4 hardening

> **Superseded by the cleaner final pass:** see
> `tests/proof-reports/PHASE_F_FINAL_SIGNOFF_REVIEW.md` for the latest
> hostile review against the now-clean proof set.

## Verdict

**Do not auto-claim final named-group signoff from this file alone.**

The D.4 slice is now materially stronger and credible:

- apply-side metadata mutations are commit-wired
- invite-seeded peers now share the same genesis nonce/state base
- imported stubs can participate in join-request lifecycle proof
- the earlier MlsEncrypted ban ordering gap is fixed and live-tested
- D.4 now has real nextest-proofed daemon tests, not just a shell smoke

The previously tracked live-proof blockers are now cleared. This makes Phase F
an explicit **signoff candidate**, but this review document still stops short of
unilaterally declaring broader project completion.

## What is now strong

### D.4
- `cargo nextest run --test named_group_d4_apply --run-ignored ignored-only`
  now passes **3/3** with archived log:
  - `tests/proof-reports/named-groups-d4-nextest.log`
- those three tests cover:
  - metadata/roster convergence
  - join-request lifecycle over imported stubs
  - MlsEncrypted ban + epoch-binding convergence

### D.3 / D.2 / E unit+integration health
- `cargo nextest run --test named_group_state_commit --test named_group_public_messages --test named_group_discovery --test api_coverage` → **58/58 pass**
- targeted ignored integration reruns pass and are archived at
  `tests/proof-reports/named-group-integration-hostile-targeted.log`
- `cargo clippy --all-features --all-targets -- -D warnings` → clean
- `cargo fmt --all -- --check` → clean
- `bash tests/api-coverage.sh` → **113/113 routes covered**

## Previously open signoff blockers — now cleared

### 1. C.2 proof-hardening is now closed
C.2 is no longer a live-proof blocker for Phase F.

What is now live-proven:
- shard-only nearby discovery witness
- late-subscriber digest/pull repair
- `ListedToContacts` positive + negative delivery
- restart-persisted shard resubscribe

Archived clean runs:
- `tests/proof-reports/named-groups-c2-hardening-run1.log`
- `tests/proof-reports/named-groups-c2-hardening-run2.log`
- `tests/proof-reports/named-groups-c2-hardening-run3.log`

Important note: the hardening work also found and fixed a real privacy bug:
`ListedToContacts` cards were still leaking onto the legacy global discovery
bridge topic. That is now blocked on both publish and receive.

### 2. Positive cross-daemon `ModeratedPublic` receive is now proven
This is no longer a Phase F blocker.

What landed:
- creator-owned local groups now resolve by stable `group_id` inside the
  public-message listener
- the sender subscribes to the public topic before first publish to avoid
  fresh-topic races
- the ignored-test harness now starts daemons with `--no-hard-coded-bootstrap`
  so local proof is not contaminated by the ambient public mesh

Evidence:
- dedicated live test `tests/named_group_e_live.rs`
- archived runs:
  - `tests/proof-reports/named-groups-e-live-run1.log`
  - `tests/proof-reports/named-groups-e-live-run2.log`
  - `tests/proof-reports/named-groups-e-live-run3.log`
  - `tests/proof-reports/named-groups-e-live-nextest.log`
- shell rerun: `tests/proof-reports/named-groups-phasef-clean.log`
  now shows:
  - `E: alice receives bob's moderated_public message`

### 3. `GroupCard.metadata_topic` hardening moved forward
This was previously an open hardening gap. At HEAD:

- newly signed cards bind `metadata_topic` into the v2 card signature domain
- verification retains a legacy fallback only for cards that omit the field
- `POST /groups/cards/import` now rejects tampered/invalid signed cards
- local card cache and `GET /groups/cards/:id` now return signed local cards

This closes the earlier unsigned-bootstrap concern for current peers. It does
**not** by itself make Phase F complete.

### 4. Overall shell suite is now clean
The latest `bash tests/e2e_named_groups.sh` rerun now ends at:
- **98 PASS / 0 FAIL**
- archived at `tests/proof-reports/named-groups-phasef-clean.log`

What closed this blocker:
- the shell suite now uses stable group ids for cross-daemon/discovery paths
- GSS encrypt/decrypt/reseal now bind to the stable group id, not the local
  route key
- owner-side card surfaces now refresh after state changes instead of serving
  stale cached cards
- reject/cancel shell proof steps were separated onto fresh groups so they no
  longer overclaim third-party stub convergence after unrelated prior updates

This closes the earlier shell-cleanliness blocker.

## Specific call on the user's review comments

The hostile D.4 review was substantially correct.

### "MlsEncrypted ban ordering gap"
Correct at the time. Now fixed.

### "Invite signable-bytes dead machinery"
Correct at the time. Now clarified and version-tagged; still intentionally
vestigial rather than enforced.

### "Genesis nonce divergence"
Correct at the time. Now fixed for invite-joined peers by carrying the
`genesis_creation_nonce` in the invite.

### "Do not claim D.4 fully proven / C.2 closed / Phase F signoff"
Updated again:
- **C.2 closed** is correct.
- **Positive `ModeratedPublic` receive proved** is correct.
- **Latest full named-groups shell rerun clean** is now also correct.
- This report still does not auto-declare final signoff, but no remaining
  live-proof blocker is called out here.

## Recommended status line

Use this wording:

> **D.4 is now landed with credible live proof, C.2 proof-hardening is closed,
> positive `ModeratedPublic` receive is proven, and the latest named-groups
> shell rerun is clean (98 PASS / 0 FAIL). Phase F is now a signoff candidate;
> do not expand that into broader project-complete language without an explicit
> final review/signoff call.**
