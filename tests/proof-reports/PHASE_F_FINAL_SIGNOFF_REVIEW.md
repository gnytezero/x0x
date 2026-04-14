# Phase F Final Hostile Signoff Review

## Verdict

**Working-tree verdict: signoff-ready for the named-groups full model, with procedural follow-up still required.**

More precisely:

- **If reviewing the combined current working tree** (committed phases A/B/C/D.2/D.3/C.2/E plus uncommitted D.4/C.2 hardening/E live proof/Phase F work), the named-group implementation now clears the design doc's signoff gates.
- **If reviewing committed Git history only**, signoff must wait until the D.4 / hardening / final-review work is actually committed or merged, because the proof and docs now describe behavior that still partially lives only in the working tree.

So this is a **hostile approve of the working tree** and a **conditional approve for merge/signoff** pending commit-state alignment and proof-artifact hygiene.

---

## What was re-checked for this final review

### Core validation reruns

- `cargo clippy --all-features --all-targets -- -D warnings` ✅
- `cargo nextest run --test named_group_state_commit --test named_group_public_messages --test named_group_discovery --test api_coverage` ✅ **58/58**
- `cargo nextest run --test named_group_d4_apply --run-ignored ignored-only` ✅ **3/3**
- `cargo test --test named_group_c2_live -- --ignored --nocapture` ✅ **4/4**
- `cargo test --test named_group_e_live -- --ignored --nocapture` ✅ **1/1**
- `bash tests/api-coverage.sh` ✅ **113/113, 100%**

### Final shell proof reruns

Three consecutive clean named-groups shell runs were archived:

- `tests/proof-reports/named-groups-phasef-final-run1.log`
- `tests/proof-reports/named-groups-phasef-final-run2.log`
- `tests/proof-reports/named-groups-phasef-final-run3.log`

All three ended at:

- **98 PASS / 0 FAIL**

The earlier clean anchor is also retained:

- `tests/proof-reports/named-groups-phasef-clean.log`

### Targeted ignored integration reruns

The concurrency-sensitive ignored integration cases were rerun **individually** and all passed:

- `named_group_creator_delete_propagates_to_peer`
- `named_group_creator_removal_propagates_to_removed_peer`
- `named_group_import_rejects_tampered_metadata_topic`
- `invite_join_preserves_genesis_creation_nonce`

Archived at:

- `tests/proof-reports/named-group-integration-hostile-targeted.log`

Important note: the monolithic command
`cargo test --test named_group_integration -- --ignored --nocapture`
is **not** a reliable hostile-review oracle because it runs multiple daemon tests
concurrently inside one binary. The targeted reruns above are the authoritative
proof for those cases.

---

## Gate-by-gate hostile assessment against `docs/design/named-groups-full-model.md`

### 1. Stable identity + evolving validity — **PASS**

Evidence:
- `tests/named_group_state_commit.rs`
- `cargo nextest run --test named_group_state_commit ...` → included in **58/58**
- shell D.3 section clean in the final shell reruns

What is proven:
- stable `group_id` survives rename / roster changes
- signed commits supersede lower revisions
- withdrawal supersedes stale cards immediately
- public cards carry authority signature, revision, state hash, and withdrawal state

Hostile note:
- This is no longer “just code-shaped.” The apply path and card supersession now have both unit/integration proof and clean shell proof.

### 2. Real discovery — **PASS**

Evidence:
- `tests/named_group_c2_live.rs` → **4/4 pass**
- `tests/proof-reports/named-groups-c2-hardening-run{1,2,3}.log`
- clean shell reruns

What is proven:
- `PublicDirectory` discovery via shard-only nearby witness
- late-subscriber digest/pull repair
- `ListedToContacts` positive delivery to trusted/known contacts
- `ListedToContacts` negative path (`Blocked` / no public leak)
- restart-persisted shard subscriptions rehydrate and receive after restart
- `Hidden` and `ListedToContacts` do not leak to public discovery surfaces

Hostile note:
- The earlier `ListedToContacts` bridge leak was a real privacy defect, not reporting noise. It is now fixed on both publish and receive paths.

### 3. Policy fidelity — **PASS**

Evidence:
- shell preset coverage in `tests/e2e_named_groups.sh`
- policy round-trip checks through create / discover / import / card surfaces
- clean shell reruns

What is proven:
- `private_secure`
- `public_request_secure`
- `public_open`
- `public_announce`
- explicit policy axes survive card/export/import paths

Hostile note:
- This is now better than API smoke. The suite checks behavior, not only route hits.

### 4. Secure enforcement — **PASS (with honest v1 scope)**

Evidence:
- D.2 encrypt/decrypt shell proof clean
- `tests/proof-reports/PROOF_REPORT_d2-cross-daemon.md`
- cross-daemon D.2 adversarial envelope proof
- D.4 ban-binding test clean

What is proven:
- request approval gives actual cross-daemon secure access
- remove/ban revokes **future** secure access
- post-ban decrypt denial is real
- remaining member decrypt after rekey is real
- recipient-confidential envelope opening is real ML-KEM sealed behavior

Honesty clause:
- This is still the approved v1 GSS + ML-KEM secure plane, **not** full MLS TreeKEM forward secrecy. That is already documented and remains honest.

### 5. Apply-side validation — **PASS**

Evidence:
- `tests/named_group_d4_apply.rs` → **3/3 pass**
- `tests/proof-reports/PHASE_D4_REPORT.md`

What is proven:
- pre-mutation authorization checks
- post-mutation `state_hash` verification
- metadata / roster convergence
- join-request lifecycle convergence over imported stubs
- MlsEncrypted ban binding / epoch convergence

Hostile note:
- The D.4 structure is correct: validate against old roster, mutate clone, finalize against the signed commit. This is the right design, not just a passing implementation.

### 6. Metadata and card convergence — **PASS**

Evidence:
- clean shell reruns
- D.4 pair-harness convergence proof
- fixes landed for stale owner-side card cache

What is proven:
- metadata patch/card refresh converge
- owner-side cache refreshes after state change
- imported/discovered stubs track authority card state rather than stale local-only snapshots
- stable-id routing is used consistently enough for cross-daemon convergence

### 7. Strict API semantics — **PASS**

Evidence:
- authz and negative-path sections in `tests/e2e_named_groups.sh`
- `bash tests/api-coverage.sh` → **113/113**
- clean shell reruns

What is proven:
- missing targets return deterministic non-success
- authz paths reject correctly
- imported-stub request lifecycle no longer silently routes to wrong id domain
- public send path now fails honestly when publish fails

Hostile note:
- The biggest earlier shell failures were id-domain and stale-cache bugs, not random flake. Those were fixed, and the suite now proves the fix set.

### 8. Repeatable proof — **PASS**

Evidence:
- three consecutive clean shell reruns
- three archived clean C.2 live runs
- three archived clean E live runs
- D.4 nextest daemon suite clean

What is proven:
- this is no longer “one lucky run”
- the strongest shell proof is repeatably clean
- C.2 and E have dedicated live receive-side proof, not just broad-suite optimism

Hostile note:
- This gate is now satisfied. Earlier objections about shell cleanliness were valid then and obsolete now.

---

## Issues that are still real

### Procedural blocker 1: commit / merge-state alignment

The implementation and reports now describe a working tree that is ahead of committed history.

That means:
- the **working tree** is signoff-ready
- the **repository history** is not yet equally signoff-ready until the D.4 / hardening / final-review work is committed

This is not a design or proof blocker. It is a merge hygiene blocker.

### Procedural blocker 2: proof-artifact policy

There are many archived logs under `tests/proof-reports/` plus transient artifacts like `test-results/`.

A final merge should decide one of:
- commit the specifically referenced proof logs and ignore the rest, or
- commit no generated logs and make reports reproducible from commands alone, or
- use a hybrid policy with explicitly archived canonical logs only

Right now the artifact story is serviceable for review but untidy for long-term maintenance.

### Process caveat: ignored daemon tests need a documented runner

The all-at-once ignored integration command is concurrency-sensitive:

- `cargo test --test named_group_integration -- --ignored --nocapture`

This should not be used as a hostile signoff oracle unless serialized.

Recommended authoritative options:
- targeted single-test reruns for daemon-heavy ignored tests, or
- nextest serialization / per-test grouping, or
- an in-test suite mutex if the file is expected to run monolithically

This is a test-runner hygiene issue, not an implementation blocker.

---

## Non-blocking follow-ups

1. Extract D.4 apply boilerplate from `src/bin/x0xd.rs` into a smaller helper/module.
2. Note the `GroupInfo` clone cost in D.4 apply path for very large groups.
3. Keep the invite-signature “vestigial / future-facing” comments until enforcement is real.
4. Decide whether to serialize all ignored daemon tests centrally in harness code or only through nextest config.

---

## Bottom-line hostile conclusion

**I do not see a remaining architecture or proof blocker in the current working tree.**

The earlier objections have been closed by real code and real proof:
- C.2 privacy + live discovery proof: closed
- positive `ModeratedPublic` receive: closed
- overall named-groups shell cleanliness: closed with repeatable clean runs
- D.4 apply-side enforcement proof: closed enough for signoff
- invite nonce parity concern: now directly covered by ignored integration proof

### Final recommendation

> **Approve the combined working tree for named-groups signoff, contingent on committing/merging the outstanding D.4 / proof-hardening / final-review work and cleaning up proof-artifact policy.**

That is the strongest honest statement I can make after a hostile pass.
