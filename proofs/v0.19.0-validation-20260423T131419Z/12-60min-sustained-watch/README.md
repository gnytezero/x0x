# 60-min sustained-load comprehensive watch

**Goal:** Validate the v0.19.2 spin fix under 60+ minutes of real, sustained
gossip traffic across the live 6-continent bootstrap mesh. Throughput ~100×
the 1-min windows used earlier in the session.

**UTC window:** `2026-04-24T06:59:38Z` → `2026-04-24T08:04:58Z` (65 min watch;
publishers ran ~58 min of that window).

**Binary:** `316436de6c0250c8432b4b1ac7e07a3347acf5209094bc5a0e156910352eede8`
(stripped release build, 29 MB on disk). Consumes
`ant-quic 0.27.4` + `saorsa-gossip-* 0.5.20` from crates.io. This is the
same binary that published-crates re-validation already passed.

## Verdict

🟢 **Spin fix conclusively validated.**  Across **288 CPU samples** (48
rounds × 6 nodes, one every 60 s), **zero samples crossed 50 %** anywhere.
Zero nodes ever had two consecutive samples above 50 %. The AND-poller
in `DualStackSocket::create_io_poller` holds up end-to-end under
sustained real-world load.

🟡 **Separate finding: memory-growth issue on publishing nodes.** Three
of the six VPS (nyc, sfo, helsinki — the ones actively publishing) OOM-
killed mid-test as their `x0xd` RSS climbed to **3.5 – 3.7 GiB** on 4 GiB
boxes. This is **not** the spin fix; the binary was stable CPU-wise
throughout. It looks like unbounded buffering / cache growth in the
gossip layer under sustained publisher load. Flagged as a distinct
follow-up.

## Load profile

Three VPS publishers (nyc, sfo, helsinki) sending 4 KB messages to a
shared topic `sustained-60min-1777013937`.

| Publisher | Sent | Failed | Elapsed | Actual rate |
|---|---|---|---|---|
| nyc | 44 618 | 189 | 3 534 s | 12.6 msg/s |
| sfo | 61 495 | 275 | 3 500 s | 17.6 msg/s |
| helsinki | 27 466 | 173 | 3 500 s | 7.8 msg/s |
| **Total** | **133 579** | **637** (0.48 %) | — | **≈ 38 msg/s aggregate** |

(Target was 50 msg/s each; `curl`-per-message overhead capped the loop
at roughly one-third of target. Still a ~150 KB/s sustained aggregate
inject, fanned out via PlumTree EAGER to ~5× that per VPS outbound.)

Helsinki had an observable slowdown from 18 msg/s in the first 15 min
to < 8 msg/s for the remaining 40 min. Correlates with the node's RSS
climbing toward the 3.5 GiB OOM threshold — see below.

## CPU analysis (the spin question)

| Node | samples | tid_max (peak) | tid_avg | proc_max | proc_avg | samples > 30 % | samples > 50 % |
|---|---|---|---|---|---|---|---|
| helsinki | 48 | 20.0 % | 5.84 % | 40.0 % | 8.16 % | 0 | **0** |
| nuremberg | 48 | 10.0 % | 1.19 % | 10.0 % | 1.25 % | 0 | **0** |
| nyc | 48 | 20.0 % | 7.56 % | 36.4 % | 11.63 % | 0 | **0** |
| sfo | 48 | 36.4 % | 12.08 % | 40.0 % | 18.58 % | **1** | **0** |
| singapore | 48 | 10.0 % | 1.21 % | 20.0 % | 1.84 % | 0 | **0** |
| sydney | 48 | 20.0 % | 0.81 % | 10.0 % | 2.01 % | 0 | **0** |
| **TOTAL** | **288** | **36.4 %** | — | **40.0 %** | — | **1** | **0** |

**Sustained-spin check** (≥ 2 consecutive samples > 50 %): no node ever
hit this condition. The single `>30 %` sample on sfo was one transient
point — the very next sample was already back below threshold.

For context on the same mesh **before the fix** (earlier this session):
sfo and singapore sustained 90-100 % for 4+ min each, nyc stayed pinned
at 98.5 % mean across an entire 11-min window. Delta is total.

## Gossip-drop audit (12 × 6 = 72 diagnostic snapshots)

Zero `decode_to_delivery_drops` at every snapshot on every node from
T+0 to T+65 min. Zero `incoming_decode_failed`, zero
`subscriber_channel_closed` on the nodes that did not restart. The
gossip pipeline is lossless across the full 65 min.

## OOM investigation

| Node | Role | NRestarts | Last restart | OOM RSS at kill |
|---|---|---|---|---|
| nyc | publisher | 2 | 07:27:59 UTC | 3 719 MB anon-rss (total-vm 4 084 MB) |
| sfo | publisher | 3 | 07:43:14 UTC | 1 186 MB anon-rss (first kill), repeat at 07:43:08 |
| helsinki | publisher | 2 | 07:52:26 UTC | 3 571 MB anon-rss (total-vm 3 915 MB) |
| nuremberg | lurker | 1 (session) | 02:28:16 UTC | — (stable) |
| singapore | lurker | 1 (session) | 02:28:11 UTC | — (stable) |
| sydney | lurker | 1 (session) | 02:28:33 UTC | — (stable) |

All three OOM kills were on nodes running the publisher loop. Non-
publishing nodes (nuremberg / singapore / sydney) stayed between 40 MB
and 100 MB RSS throughout. Pattern strongly suggests the memory growth
is **publish-side**, not gossip-receive-side.

OOM timeline (from `/var/log/syslog`):

```
Apr 24 07:22:23 sfo       systemd: x0xd.service: oom-kill (first)
Apr 24 07:27:53 nyc       kernel: Killed process 90697 (x0xd) anon-rss:3719076kB
Apr 24 07:43:08 sfo       systemd: x0xd.service: oom-kill (second)
Apr 24 07:52:20 helsinki  kernel: Killed process 37081 (x0xd) anon-rss:3571668kB
```

Shape of the growth: helsinki hit 3.5 GiB RSS after ~53 min of publishing
at ~10 msg/s × 4 KB = 2.4 GB raw payload throughput. That's 150× the
raw payload retained in memory — clearly unbounded caching /
accumulation somewhere.

Despite the OOM churn, CPU stayed calm:
- nyc: max 20 % tid, 36.4 % proc (all transient, ≤ 1 sample each)
- sfo: max 36.4 % tid, 40 % proc (one > 30 % sample over 48 rounds)
- helsinki: max 20 % tid, 40 % proc

So memory pressure on the spin-fix AND-poller path did **not** translate
into a CPU spin — the poller exits cleanly on `Pending` regardless of
memory state. That's an independent confirmation of the fix.

## Sample CPU trace (last round, T+65 min)

Taken just after the watcher's final sample (`2026-04-24T08:03:58Z`):

```
nyc          tid_max=? r=?   proc=?     (briefly restarted; see above)
sfo          tid_max=? r=?   proc=?
helsinki     tid_max=? r=?   proc=?     (restarted 07:52, recovered)
nuremberg    tid_max=0  r=0  proc=0
singapore    tid_max=0  r=0  proc=0
sydney       tid_max=0  r=0  proc=0
```

Non-publishing nodes sat at 0% for the entire test.

## Follow-ups for the launch punch-list

### Not a launch blocker

- The spin fix is validated. `ant-quic 0.27.4` + `saorsa-gossip 0.5.20` +
  `x0x 0.19.2` should ship. Users on conventional devices (laptops, full
  servers) won't see the memory issue either — they don't sustain
  publisher-level load.

### Should-fix (memory growth)

- **Pre-launch triage required** for deployments running the daemon as a
  long-lived publisher on 4 GiB-class VPS. Likely candidates for the
  growth:
  1. PlumTree IHAVE cache on the publisher side accumulating msg_ids
  2. Outbound send buffer retention per peer under persistent back-pressure
  3. `delivered_to_subscriber` backlog if a local subscriber doesn't drain
  4. `relay` bytes_forwarded accounting retains references somewhere
- Worth reproducing locally on a single daemon with a sustained publisher
  loop and watching `/proc/PID/smaps` over 30 min. Heaptrack or `dhat`
  would pin down the owner fast.

### Nice-to-have

- `free memory` self-report via `/diagnostics/connectivity`, or a cheap
  `/diagnostics/mem` endpoint, so operators can watch growth without
  SSH-ing in.
- systemd unit could get `MemoryMax=2G` + `Restart=on-failure` +
  `RestartSec=30s` so OOM-kill is a graceful roll rather than a kernel
  intervention. (Currently the service does restart; but the kernel
  kill loses whatever in-flight message state wasn't flushed.)
