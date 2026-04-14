# Phase D.4 Proof Report — strict apply-side commit wiring

> **Honesty clause.** This report covers the D.4 slice exercised at current
> working-tree HEAD. It does **not** by itself claim broader project-complete
> status. C.2 is now closed separately, and the named-groups shell suite is now
> clean, but final signoff wording lives in `PHASE_F_FINAL_SIGNOFF_REVIEW.md`.

## What D.4 now means at HEAD

D.4 moves the remaining named-group metadata mutations onto the D.3 signed
state-commit chain so receivers no longer trust only ad-hoc per-field
revision counters.

At HEAD, these metadata-plane events carry a signed `GroupStateCommit` and
apply through the D.4 validate → mutate → finalize flow:

- `MemberAdded`
- `MemberRemoved`
- `GroupDeleted`
- `PolicyUpdated`
- `MemberRoleUpdated`
- `MemberBanned`
- `MemberUnbanned`
- `JoinRequestCreated`
- `JoinRequestApproved`
- `JoinRequestRejected`
- `JoinRequestCancelled`
- `GroupMetadataUpdated`

## Code landed

### 1. Apply-side two-phase validation
`src/bin/x0xd.rs` uses `apply_stateful_event_to_group(...)`:

1. validate the signed commit against the **pre-mutation** view
2. clone `GroupInfo`
3. apply the event-specific mutation to the clone
4. call `GroupInfo::finalize_applied_commit(...)`
5. only swap the new state in on success

This is the right structure for actions like self-leave, where signer
authority must be checked against the old roster before the mutation removes
that signer.

### 2. Invite seeding is now chain-compatible
`src/groups/invite.rs` and `src/bin/x0xd.rs::join_group_via_invite(...)` now
carry enough authority metadata for invite-joined peers to reconstruct the
same genesis/state base:

- `stable_group_id`
- `group_created_at`
- `group_description`
- `policy`
- `genesis_creation_nonce`

This closes the earlier invite-join mismatch where peers could share the same
stable `group_id` but expose different `genesis.creation_nonce` values.

### 3. MlsEncrypted ban ordering gap fixed
The earlier review concern was real: `MemberBanned` committed the new
`security_binding` (`gss:epoch=N+1`) but the receiver-side mutation closure did
not update that binding before `finalize_applied_commit(...)`, making the ban
apply depend on `SecureShareDelivered` arriving first.

At HEAD:

- `NamedGroupMetadataEvent::MemberBanned` carries `secret_epoch`
- the receiver-side ban closure updates `secret_epoch` + `security_binding`
  before finalization
- if the epoch advanced and the actual secret has not arrived yet, the stale
  `shared_secret` is cleared so the later envelope is still accepted
- `SecureShareDelivered` now accepts equal-epoch delivery iff the daemon knows
  the epoch but does not yet hold the secret material, and stores the matching
  `security_binding`

This removes the earlier apply-side ordering dependency.

### 4. Stable-id event identity for imported stubs
A real D.4-adjacent bug surfaced while adding join-request live proof:
owner-authored events were still publishing `group_id: id.clone()` from the
owner's local routing key, which could be the local `mls_group_id` path rather
than the D.3 stable `group_id`. Imported/discovered stubs are keyed by stable
`group_id`, so reject/approve/unban-style events could miss them.

At HEAD, state-bearing metadata events now publish the **stable**
`group_id` from `info.stable_group_id()`.

### 5. Imported-stub bootstrap is now authority-bound
Another real product gap surfaced while proving join requests live:
imported stubs need the authority's metadata topic to publish request events.

At HEAD:

- `GroupCard` carries `metadata_topic: Option<String>`
- newly signed cards bind it in the v2 signature domain
- verification retains a legacy fallback only for cards with no
  `metadata_topic`
- `GET /groups/cards/:id` and local cache refresh now produce signed local cards
- `POST /groups/cards/import` rejects tampered/invalid signed cards
- `import_group_card(...)` copies the verified topic into the local stub

This is enough for honest live join-request proof across imported stubs.

## Review concerns from the hostile pass — status

### Concern 1: MlsEncrypted ban ordering dependency
**Resolved in code and live proof.** See section 3 above.

### Concern 2: invite signable-bytes silent wire break / dead machinery
**Addressed.** `SignedInvite::signable_bytes()` now has an explicit
`x0x.invite.v2|` domain/version prefix, and comments now clearly state invite
signatures are currently vestigial/future-facing rather than enforced by the
join flow.

### Concern 3: `genesis.creation_nonce` diverges across peers
**Resolved for invite-joined peers.** The nonce now rides in the invite and is
reconstructed on the joiner, so `/groups/:id/state` no longer exposes that
particular divergence after invite join.

## Live proof added

A single ignored integration binary now contains three real pair-harness tests:

- `tests/named_group_d4_apply.rs`
  - `d4_stateful_events_converge_via_signed_commits`
  - `d4_join_request_events_converge_via_signed_commits`
  - `d4_mls_ban_commit_advances_binding_and_converges`

### What they prove

#### A. Metadata / roster convergence
Two real daemons prove convergence after:

1. metadata patch
2. policy patch
3. role update (owner promotes bob to admin)
4. member add
5. ban
6. admin-authored unban back to the owner
7. member remove

This remains the strongest positive D.4 proof for the core metadata/roster
plane.

#### B. Join-request lifecycle over imported stable-id stubs
Two real daemons prove the request lifecycle across a non-member imported stub:

1. bob imports the creator's discoverable card
2. bob submits a request; alice observes `pending`
3. duplicate request is rejected locally on bob
4. bob cancels; alice observes `cancelled`
5. bob re-requests; alice rejects; alice observes `rejected`
6. bob re-requests again; alice approves
7. once approved, alice and bob converge on `/groups/:id/state`
8. alice and bob both show bob as an active member
9. bob can no longer submit another request because he is already a member

#### C. MlsEncrypted ban path
Two real daemons prove the rekey-bound ban path converges with the new D.4
binding behavior:

1. add third member
2. ban third member on an `MlsEncrypted` group
3. alice + bob converge on the new signed state
4. `security_binding` advances to `gss:epoch=1` on both sides
5. banned member state is visible as `banned` on the peer

## Validation run

### Code health
- `cargo fmt --all -- --check` ✅
- `cargo clippy --all-features --all-targets -- -D warnings` ✅
- `cargo check --all-features` ✅

### Targeted proof / regression tests
- `cargo nextest run --test named_group_state_commit --test named_group_public_messages --test named_group_discovery --test api_coverage` → **58/58 pass**
- targeted ignored integration reruns all pass:
  - `named_group_creator_delete_propagates_to_peer`
  - `named_group_creator_removal_propagates_to_removed_peer`
  - `named_group_import_rejects_tampered_metadata_topic`
  - `invite_join_preserves_genesis_creation_nonce`
  - archived: `tests/proof-reports/named-group-integration-hostile-targeted.log`
  - note: the full `cargo test --test named_group_integration -- --ignored` command
    is not authoritative for hostile review because it runs multiple daemon tests
    concurrently inside one binary
- `cargo nextest run --test named_group_d4_apply --run-ignored ignored-only` → **3/3 pass**
  - archived: `tests/proof-reports/named-groups-d4-nextest.log`
- `bash tests/api-coverage.sh` → **113/113, 100.0%**
- `bash tests/e2e_named_groups.sh` → **98 PASS / 0 FAIL**
  - archived clean runs:
    - `tests/proof-reports/named-groups-phasef-clean.log`
    - `tests/proof-reports/named-groups-phasef-final-run1.log`
    - `tests/proof-reports/named-groups-phasef-final-run2.log`
    - `tests/proof-reports/named-groups-phasef-final-run3.log`

### Harness hardening
`tests/harness/src/cluster.rs` now allocates ephemeral TCP/UDP ports for pair
and cluster daemons instead of relying on a narrow fixed range. This makes the
ignored D.4 daemon tests much more reliable.

`.config/nextest.toml` now serializes `d4_*` tests into the existing
`quic-localhost` test group so nextest does not run multiple localhost
multi-daemon D.4 cases concurrently.

## Explicit non-claims

- This report does **not** by itself claim final Phase F signoff.
- C.2 proof-hardening is now closed, and positive cross-daemon
  `ModeratedPublic` receive is now separately proven.
- The overall named-groups shell suite is now clean, but the final hostile
  signoff call lives in `PHASE_F_FINAL_SIGNOFF_REVIEW.md` rather than here.

## Current label

**D.4 is now a credible working-tree-landed slice with live proof for
metadata/roster, join-request lifecycle, and the MlsEncrypted ban binding
path. The downstream hostile review gates now depend primarily on final
signoff wording / merge hygiene, not on missing D.4 proof.**
