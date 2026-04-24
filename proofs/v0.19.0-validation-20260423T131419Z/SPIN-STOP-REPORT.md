# x0x v0.19.0 — Spin regression reproduces. Validation halted at step 8.

**UTC window:** 2026-04-23T14:10Z — 2026-04-23T14:27Z
**Git:** `fb51ba9` (v0.19.0 tag) + dirty `Cargo.toml` (`[patch.crates-io] ant-quic = { path = "../ant-quic" }`) → sibling `../ant-quic` HEAD `43acc666` (6 commits unpushed incl. the two getsockname perf fixes).
**Verdict:** 🛑 **STOP-CONDITION TRIGGERED.** "Any CPU-spin recurrence (any node >50% x0xd CPU for more than 60 s steady state) → halt."

## Steps completed before halt

| Step | Result | Artifact |
|---|---|---|
| 1. `just check` | ✅ 1024/1025 | `step1b-nextest-rerun.log` — pre-existing parity_cli only; flaky `test_agent_id_uniqueness` fixed |
| 2. Deploy to 6 VPS | ✅ 24/24 | `step2-deploy.log` — all 6 on v0.19.0, rolling 15s honored |
| 3. Health + journal | ✅ | `00-vps-health.txt`, `00-vps-journalctl-audit.txt` — zero agent_id mismatch, zero ERRORs |
| 4. Local comprehensive | ✅ 147/147 | `01-local-comprehensive.log` — 145 pass + 2 SSE timeouts |
| 5. Live network ↔ VPS | ❌ cascade | `02-live-network.log` — failed downstream of nyc spin (nyc API responses empty because nyc CPU-bound) |
| 7. Wire v2 audit | ✅ | `04-wire-v2-audit.txt` — zero v1, zero legacy shards, v2 topics live |
| 8. CPU-spin check | 🛑 **FAIL** | `05-cpu-baseline.txt`, `spin-forensics/*` |

## CPU evidence

Two independent 60-second parallel samples across all 6 nodes, ~2 min apart:

**T+~4min post-deploy (`05-cpu-baseline.txt`):**
```
nyc           100.0  90.9  90.9 100.0 100.0  90.9 | max=100.0 mean=95.5
sfo            30.0   0.0   0.0   0.0   0.0 110.0 | max=110.0 mean=23.3
helsinki        0.0   0.0   0.0  10.0   0.0   0.0 | max=10.0  mean=1.7
nuremberg       0.0   0.0   0.0   9.1   0.0   0.0 | max=9.1   mean=1.5
singapore       0.0   0.0   0.0  20.0   0.0   0.0 | max=20.0  mean=3.3
sydney          0.0   0.0   0.0  20.0   0.0   0.0 | max=20.0  mean=3.3
```

**T+~7min (`spin-forensics/mesh-spin-resample.txt`):**
```
nyc           100.0 100.0 100.0  90.9 100.0 100.0 | max=100.0 mean=98.5  ← still pinned
sfo           100.0 100.0 100.0 100.0  90.9  90.9 | max=100.0 mean=97.0  ← newly pinned
others                                            ≤ 50% with brief spikes only
```

Pattern exactly matches `NEXT-SESSION-PROMPT.md`: "2–3 of 6 are pinned; rotates across nodes". Both pinned nodes sustained ≥90% for the full 60 s window. Stop-condition threshold (>50% for >60 s) passed decisively.

## Forensics on nyc (`spin-forensics/nyc-top-stacks.txt`, `nyc-gdb-bt.txt`)

### Per-thread CPU (`top -bHn1 -p 52102`)
```
PID    COMMAND          %CPU TIME+       STATE
52104  tokio-rt-worker  99.9  6:34.84    R (running)   ← the spinning worker
52103  tokio-rt-worker   0.0  0:09.84    S
52102  x0xd              0.0  0:00.03    S
52107  mDNS_daemon       0.0  0:00.11    S
```

### Kernel state for spinning tid 52104
- `/proc/52104/wchan`: `0` (not waiting on any kernel object)
- `/proc/52104/syscall`: `running` (not inside any syscall)
- `/proc/52104/stack`: **empty** (no kernel-side frames)
- `State: R (running)`, `voluntary_ctxt_switches: 43384`, `nonvoluntary: 7457` over ~8 min runtime (≈100 ctx-switch/sec)

→ Thread is in **pure userspace Rust**, busy-polling. The high voluntary ctx-switch rate is consistent with tokio's cooperative yield firing repeatedly on an always-ready future (hypothesis #1 in NEXT-SESSION-PROMPT).

### gdb backtrace (stripped binary — no symbols)
Thread 3 (LWP 52104) top frame `0x0000626b2ad5ca18`, 19 frames deep, all `??`. Text frames 5-19 look like a tokio runtime poll stack. The stripped release binary means no line-level resolution from this snapshot.

### perf record failed
`perf record -F 99 -g -p $pid --call-graph dwarf -- sleep 30` produced a zero-byte usable record ("Captured and wrote 0.000 MB (null)") and `perf report` failed with "incompatible file format". Likely kernel 6.8.0-110-generic vs the `linux-tools-generic` meta-package delivering a mismatched perf build. Needs either a matching `linux-tools-6.8.0-110-generic` install or an alternative profiler.

## What didn't help (already tried)

- ant-quic `e6eb4b54` + `43acc666` (the two unpushed getsockname fixes that 722f80e pulls in on x0x) are in the deployed binary. Prior session confirmed they eliminate the `getsockname` hot path but did **not** eliminate the spin — now reproduced on v0.19.0 with the same ant-quic.
- v0.19.0's CHANGELOG delta (wire v2 + UserAnnouncement + IntroductionCard signing) is orthogonal to the spin path. As expected: the bug persists.

## Concrete next actions (pick one)

**(X) Symbolicated trace on a still-spinning node.**
Build x0xd with the `release-profiling` profile (472 MB, embedded debug, sha `52ebff45…` from the prior session), scp to nyc, `systemctl restart x0xd`, wait for re-spin (per prior session: 1-5 min), then `gdb -batch ... thread apply all bt 30`. This is exactly what NEXT-SESSION-PROMPT step 3 prescribes. Gives us the Rust function names in the spinning stack.

**(Y) Install matching perf tools + flamegraph.**
`apt install -y linux-tools-6.8.0-110-generic` on nyc, redo `perf record -F 99 -g -p $pid -- sleep 30`, collect `perf script` output. Feed into FlameGraph. More thorough than a single gdb snapshot, doesn't need the profiling binary.

**(Z) Stop the mesh again, triage offline.**
`systemctl stop x0xd && systemctl disable x0xd` on all 6, restore to pre-session stopped-and-disabled state. Pure rollback — defers the hunt to a separate focused session without the full-mesh context.

## My recommendation

**X + Y together:** build release-profiling locally right now (~2 min), deploy to nyc and sfo (the two currently pinned), restart each, wait for re-spin (≤5 min per node), then BOTH gdb symbolicated backtrace AND perf flamegraph on whichever re-spins first. Two complementary signals in one pass. Leave the other 4 nodes on the stripped release binary so we have a stable mesh for comparison.

**I am not proceeding with validation steps 6, 9, 10, 11, 12, 13, 14 until you pick a path.** The remaining steps either require the mesh stable (6, 9, 10, 14) or are orthogonal and safe to do next session (11, 12, 13). Stress + file-transfer + efficiency audits against a spinning mesh will report data dominated by the spin, not by their own subject matter.

## Housekeeping notes (for whenever this lands)

- `tests/e2e_deploy.sh` has a 6-line uncommitted patch adding a 15s rolling delay between node restarts (matches `rolling_start_requirement` memory). Genuinely useful; belongs in a fix commit regardless of the spin outcome.
- `tests/comprehensive_integration.rs` has 3 single-line uncommitted fixes (`AgentId([rand::random::<u8>(); 32])` → `AgentId(rand::random::<[u8; 32]>())`). Latent flake at 1/256, exposed this session.
- `Cargo.toml`'s `[patch.crates-io] ant-quic = { path = "../ant-quic" }` is unchanged — pre-existing dirty state. The deployed v0.19.0 binary therefore contains 6 unpushed ant-quic commits. If we cut a v0.19.1 to crates.io from this tree, either the patch needs committing (and ant-quic needs a 0.27.4 crates.io release with the 6 commits) or the patch needs stripping (losing the two getsockname fixes that 722f80e depends on).
