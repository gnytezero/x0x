# Proof report artifact policy

This directory contains both:

1. **canonical archived proof artifacts** that are referenced by reports/docs, and
2. **transient/generated rerun logs** used during investigation.

For merge/signoff, prefer keeping the files that are explicitly referenced by the
current phase reports and final hostile review.

## Canonical named-groups signoff artifacts

### Summary reports
- `PHASE_C2_REPORT.md`
- `PHASE_D4_REPORT.md`
- `PHASE_E_REPORT.md`
- `PHASE_F_REVIEW.md`
- `PHASE_F_FINAL_SIGNOFF_REVIEW.md`

### Canonical clean named-groups shell proof
- `named-groups-phasef-clean.log`
- `named-groups-phasef-final-run1.log`
- `named-groups-phasef-final-run2.log`
- `named-groups-phasef-final-run3.log`

### Canonical dedicated live-proof logs
- `named-groups-c2-hardening-run1.log`
- `named-groups-c2-hardening-run2.log`
- `named-groups-c2-hardening-run3.log`
- `named-groups-c2cd-rerun.log` (historical shell hook still cited by `PHASE_C2_REPORT.md`)
- `named-groups-e-live-run1.log`
- `named-groups-e-live-run2.log`
- `named-groups-e-live-run3.log`
- `named-groups-e-live-nextest.log`
- `named-groups-d4-nextest.log`
- `named-group-integration-hostile-targeted.log`

## Transient / historical rerun logs

Older reruns that are not referenced by the current final reports may be kept
for archaeology, but they are not required for the final named-groups signoff
story. If the repo wants a smaller artifact surface, prune unreferenced rerun
logs first.

## Runner artifacts outside this directory

- `test-results/` is transient and should remain ignored.
- `.claude/worktrees/` and `.claude/scheduled_tasks.lock` are local workflow
  artifacts and should remain ignored.
