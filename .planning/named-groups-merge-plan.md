# Named-groups merge / signoff commit plan

This plan assumes the current working tree is the desired merge target.

## Goal

Land the remaining named-groups work in reviewable commits that preserve the
story already established by the phase reports:

- D.4 strict apply-side commit wiring
- C.2 proof-hardening closure
- Phase E live receive proof
- final shell-suite cleanup / stable-id hardening
- final hostile signoff review + artifact policy

## Recommended commit boundaries

### Commit 1 — D.4 core implementation

**Suggested message**

```text
feat(groups): phase D.4 strict apply-side commit wiring
```

**Include**
- `src/bin/x0xd.rs`
- `src/groups/invite.rs`
- `src/groups/mod.rs`
- `src/groups/state_commit.rs`
- `tests/named_group_d4_apply.rs`
- `tests/harness/src/cluster.rs` (only the D.4/harness reliability pieces if separable)
- `tests/proof-reports/PHASE_D4_REPORT.md`
- `tests/proof-reports/named-groups-d4-nextest.log`
- `tests/proof-reports/named-groups-d4-apply.log` if referenced

**Narrative**
- every state-bearing metadata event now validates signed commits pre-mutation
- cloned state is mutated and finalized post-mutation
- invite-joined peers reconstruct the same genesis base
- MlsEncrypted ban ordering / binding gap closed

---

### Commit 2 — discovery/public-message hardening + shell cleanup

**Suggested message**

```text
fix(groups): close C.2/E live proof gaps and clean named-groups shell suite
```

**Include**
- `src/bin/x0xd.rs`
- `src/groups/directory.rs`
- `src/groups/discovery.rs`
- `src/groups/mod.rs`
- `src/groups/public_message.rs`
- `tests/e2e_named_groups.sh`
- `tests/named_group_c2_live.rs`
- `tests/named_group_e_live.rs`
- `tests/named_group_discovery.rs`
- `tests/named_group_integration.rs`
- `tests/proof-reports/PHASE_C2_REPORT.md`
- `tests/proof-reports/PHASE_E_REPORT.md`
- canonical logs:
  - `tests/proof-reports/named-groups-c2-hardening-run1.log`
  - `tests/proof-reports/named-groups-c2-hardening-run2.log`
  - `tests/proof-reports/named-groups-c2-hardening-run3.log`
  - `tests/proof-reports/named-groups-e-live-run1.log`
  - `tests/proof-reports/named-groups-e-live-run2.log`
  - `tests/proof-reports/named-groups-e-live-run3.log`
  - `tests/proof-reports/named-groups-e-live-nextest.log`
  - `tests/proof-reports/named-group-integration-hostile-targeted.log`
  - `tests/proof-reports/named-groups-phasef-clean.log`
  - `tests/proof-reports/named-groups-phasef-final-run1.log`
  - `tests/proof-reports/named-groups-phasef-final-run2.log`
  - `tests/proof-reports/named-groups-phasef-final-run3.log`

**Narrative**
- closes C.2 proof-hardening with live proof
- proves positive cross-daemon `ModeratedPublic` receive
- fixes stable-id / local-route-id mismatches in shell and GSS paths
- fixes stale group-card cache behavior after state changes
- makes `tests/e2e_named_groups.sh` repeatably clean

---

### Commit 3 — final signoff docs / artifact policy / hygiene

**Suggested message**

```text
docs(groups): finalize hostile signoff review and proof artifact policy
```

**Include**
- `docs/design/named-groups-full-model.md`
- `tests/proof-reports/PHASE_F_REVIEW.md`
- `tests/proof-reports/PHASE_F_FINAL_SIGNOFF_REVIEW.md`
- `tests/proof-reports/README.md`
- `.planning/c2-proof-hardening.md`
- `.planning/named-groups-merge-plan.md`
- `.gitignore`

**Narrative**
- aligns docs with current working-tree status
- records the final hostile signoff call
- defines which proof artifacts are canonical vs transient
- ignores local workflow artifacts (`test-results/`, `.claude/worktrees/`, etc.)

## Artifact policy for the merge

### Keep
Only keep proof logs that are explicitly referenced by the current phase/final
reports.

### Drop or leave uncommitted
Older exploratory reruns that are not referenced by the final reports, for
example:
- `named-groups-e-hardened-rerun.log`
- `named-groups-e-phasef-rerun.log`
- `named-groups-d4-phasef-rerun.log`
- `named-groups-d4-shell-rerun.log`
- `named-groups-c2a-rerun.log`
- `named-groups-c2ab-rerun.log`
- other superseded intermediate reruns that are no longer cited by the final
  reports

## Extra tree-hygiene decisions

### `.claude/skills/e2e-prove/SKILL.md`

`git status` currently shows `.claude/skills/` as untracked. Decide explicitly:

- **commit it** if the repo intends to version the strengthened named-groups /
  proof workflow skill, or
- **ignore it** if repo-local Claude/Pi skill content is considered local tooling
  rather than source-of-truth project content.

Do not leave this as an accidental untracked surprise at merge time.

## Final merge gate

Before actually merging, re-run:

```bash
cargo clippy --all-features --all-targets -- -D warnings
cargo nextest run --test named_group_state_commit --test named_group_public_messages --test named_group_discovery --test api_coverage
cargo nextest run --test named_group_d4_apply --run-ignored ignored-only
cargo test --test named_group_c2_live -- --ignored --nocapture
cargo test --test named_group_e_live -- --ignored --nocapture
bash tests/e2e_named_groups.sh
```

Expected shell result:
- `98 PASS / 0 FAIL`
