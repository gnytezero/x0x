# Next-session prompt — x0xd CPU spin, partial fix, more hunt needed

**Read this before doing anything. State of the network: all 6 VPS bootstrap
`x0xd` services are stopped + disabled. Do not restart them without a plan.**

---

## The one-sentence problem

On the live 6-node public bootstrap mesh, `x0xd` workers latch into a
sustained CPU spin (100–200 % per process, both tokio workers running,
no kernel work, `journalctl` stops emitting) after a few minutes of
normal gossip traffic. The spin rotates across nodes: at any probe,
2–3 of 6 are pinned; `systemctl restart x0xd` clears the one you hit
but another takes its place. Mesh converges only to a 3–4 peer subset
before some node drops into spin again.

If this isn't nailed, the public network is effectively unusable once
any real traffic is on it. Treat it as a blocker.

---

## What's already been fixed (do not re-investigate)

Two real `getsockname(2)` hot paths found with `perf record -g --call-graph
dwarf` on a spinning SFO daemon and fixed upstream in `ant-quic`
(sibling path dep, repo `../ant-quic`, branch `master`):

- **ant-quic `e6eb4b54`** — `perf(dual_stack): cache socket family instead
  of per-packet getsockname`. `DualStackSocket::try_send` called
  `socket.local_addr()?.is_ipv6()` on every outgoing UDP datagram.
  `select_socket` now returns `(&socket, is_v6: bool)` and `convert_dest`
  takes the boolean.
- **ant-quic `43acc666`** — `perf(dual_stack): cache local_addr so
  AsyncUdpSocket::local_addr is syscall-free`. quinn's
  `ConnectionDriver::poll` calls `AsyncUdpSocket::local_addr` every
  iteration; the impl used to delegate to `tokio::net::UdpSocket::local_addr()`
  (another `getsockname`). It now serves `v4_addr`/`v6_addr` from fields
  captured at `new()`.

Effect (pre-fix vs post-fix symbolicated perf on a spinning VPS):

| Stack location                                          | Pre  | Post-1 | Post-2 |
|---------------------------------------------------------|------|--------|--------|
| `getsockname` under `try_send → convert_dest`           | 71 % | gone   | gone   |
| `getsockname` under `ConnectionDriver → local_addr`     | –    | 34 %   | gone   |
| `parking_lot::Condvar::wait` (workers idle)             | no   | no     | yes (on freshly restarted nodes) |

`gdb -batch -p $pid -ex "thread apply all bt"` after both fixes, on a
freshly-restarted node, shows both `tokio-rt-worker` threads parked in
`parking_lot::Condvar::wait` — healthy idle.

Both fixes ship together as part of the release build
`target/x86_64-unknown-linux-gnu/release/x0xd`
(sha256 prefix `f9694c53aa42b542`).

On the `x0x` side: commit `722f80e` (`perf: pick up ant-quic getsockname
fixes + add release-profiling profile`). Adds a `[profile.release-profiling]`
with `debug = "full"` + `strip = "none"` + `split-debuginfo = "off"`
so gdb on VPS doesn't need `.dwo` files. Proof artefacts in
`proofs/spin-fix-20260422-v0185/`.

---

## The residual bug (the real task)

Even with both fixes deployed everywhere (verified by sha256 on each
node before services were stopped), sustained 6-node mesh load still
latches 2–3 nodes into spin:

- `top -bHn1 -p $(pidof x0xd)` on a pinned daemon shows **2 tokio
  workers at ~100 % each**, main thread idle, `mDNS_daemon` idle.
- `/proc/$pid/task/$tid/stack` is a few frames of interrupt-return /
  timer interrupt — **threads are in user-space Rust**, *not* in any
  syscall.
- `gdb -batch -p $pid -ex "thread apply all bt"` on a pinned worker in
  this post-fix state was not captured — every gdb attempt that caught
  a spinning worker showed a stack in the OLD (pre-fix) profiling
  binary, which was stale due to Cargo's profile-inheritance cache
  (see "traps" below).
- No new `journalctl` entries once a node is in the spin state.
- `systemctl restart x0xd` clears one node for a few minutes before
  another drops in.
- The 5-minute sustained watch in
  `proofs/spin-fix-20260422-v0185/mesh-watch.log` shows the rotating
  2-of-6 / 3-of-6 pattern.

**This is the next bug to find.** Candidate hypotheses, roughly in
priority order:

1. A busy-polling tokio task whose future is always `Poll::Ready`
   (e.g. a `Notify` that's notified every time any packet arrives,
   an `UnboundedReceiver::poll_recv` paired with an upstream that
   always has something queued, or a broadcast channel lagging).
   Profile showed `ant_quic::high_level::endpoint::RecvState::poll_socket
   → mpsc::UnboundedSender::send → wake_by_val → eventfd_write` hitting
   `tokio::yield_now` in earlier captures — this chain is suspicious
   under steady mesh rate.
2. ML-DSA-65 verification on every inbound announcement / gossip
   message saturates 2 vCPUs on s-2vcpu-4gb droplets when the
   6-node mesh has ~7 machines announcing. This would be *legitimate*
   work, but if announcements aren't deduped aggressively enough it
   becomes a cost-accumulation spiral.
3. A CRDT OR-Set / PlumTree message amplification where each inbound
   message generates N outbound messages without settling.
4. The `observed_address_watch_task` spawned in
   `ant-quic/src/nat_traversal_api.rs:5543` — it's `loop { iterate
   all_observed_addresses; select! { observed_change, closed } }`.
   If `observed_address_updated()` (a `Notify::notified()`) is notified
   by every packet, the loop iterates at packet rate. Worth reading
   closely.
5. ant-quic connection-supersede race under IPv6 / IPv4 dual-stack
   mapping (we are on dual-stack `[::]:5483`, all 6 bootstrap nodes
   have global IPv6, and gossip runs across both families).

---

## Environment

### 6 bootstrap nodes (all STOPPED now)

| Name | IP | IPv6 | Provider | Size | Status | Binary (sha256 pfx) |
|------|----|----|----|----|----|----|
| NYC (saorsa-2)       | 142.93.199.50  | 2604:a880:400:d1:0:3:7db3:f001 | DigitalOcean NYC1   | s-2vcpu-4gb | stopped/disabled | `f9694c53…` (release) |
| SFO (saorsa-3)       | 147.182.234.192 | 2604:a880:4:1d0:0:1:6ba1:f000  | DigitalOcean SFO3  | s-2vcpu-4gb | stopped/disabled | `f9694c53…` (release) |
| HEL (saorsa-6)       | 65.21.157.229   | 2a01:4f9:c012:684b::1          | Hetzner Helsinki   | (original)  | stopped/disabled | `f9694c53…` (release) |
| NUR (saorsa-7)       | 116.203.101.172 | 2a01:4f8:1c1a:31e6::1          | Hetzner Nuremberg  | (original)  | stopped/disabled | `f9694c53…` (release) |
| SGP (saorsa-8, NEW)  | 152.42.210.67   | 2400:6180:0:d2:0:2:d30b:d000   | DigitalOcean SGP1  | s-2vcpu-4gb | stopped/disabled | `f9694c53…` (release) |
| SYD (saorsa-9, NEW)  | 170.64.176.102  | 2400:6180:10:200::ba69:b000    | DigitalOcean SYD1  | s-2vcpu-4gb | stopped/disabled | `f9694c53…` (release) |

Vultr is **out** — Singapore was auto-null-routed by Vultr's DDoS
heuristic under sustained gossip UDP, so saorsa-8 + saorsa-9 were
moved to DO. That migration is the `585a8e8 infra(vps): retire Vultr
nodes — migrate Singapore + Tokyo to DigitalOcean` commit. Tokens are
in `tests/.vps-tokens.env`. SSH as `root@$IP`.

The four DO droplets were resized `s-1vcpu-2gb → s-2vcpu-4gb` during
this session because the old 1-vCPU was starving x0xd under mesh
load. Hetzner HEL + NUR were *not* resized and need the same treatment
if we retry at the old size (`hcloud` CLI isn't installed locally —
use the Hetzner web console). They've held up better than the DO
nodes did, but the sample is tainted by the spin.

### Legacy Communitas — gone

NYC / SFO / HEL / NUR were previously running 4–5 legacy
`communitas-headless` / `communitas-bootstrap` / `communitas-mcp`
daemons that competed for the 1-vCPU. Those were purged in this
session because Communitas now bootstraps on x0x. Only x0xd is
installed (and now stopped) on the bootstrap nodes.

### Local build state

`rustc` 1.95.0, `cargo zigbuild 0.15.2`, target
`x86_64-unknown-linux-gnu`.

- `target/x86_64-unknown-linux-gnu/release/x0xd`
  (29 MB, stripped, `f9694c53aa42b542`) — both fixes compiled in.
- `target/x86_64-unknown-linux-gnu/release-profiling/x0xd`
  (472 MB, debuginfo, `52ebff45aedb765e`) — same source, same fixes,
  embedded debuginfo.

`Cargo.toml` has the new `[profile.release-profiling]` for future
perf/gdb work. Dep `ant-quic = { path = "../ant-quic" }` — both
upstream fixes are on `../ant-quic` HEAD (`43acc666`).

---

## Traps already stepped in (don't repeat)

1. **Cargo profile cache lied**. `[profile.release-profiling] inherits
   = "release"` does NOT rebuild the `ant-quic` rlib when you change
   the *profile* without also touching a source file. Earlier
   symbolicated profiles were of the stale pre-fix binary even though
   my local source was patched. Trusted signal:
   `cargo clean --target x86_64-unknown-linux-gnu --profile release-profiling`
   before every reprofile, or verify `Compiling ant-quic v0.27.3`
   appears in the build log.
2. **`split-debuginfo = "unpacked"`** is the default and ships `.dwo`
   files next to the binary. You need the `.dwo` files on the VPS for
   gdb / addr2line to resolve symbols, which is awful to manage. The
   profile in this commit uses `split-debuginfo = "off"` so the
   debuginfo is embedded — ~470 MB binary but gdb works.
3. **`perf top -p $pid --stdio`** is *interactive* and only works on
   a real terminal; use `perf record … -- sleep N && perf report
   --stdio --no-children` for non-interactive captures.
4. **Vultr auto-null-routes** sustained UDP traffic from non-DDoS-
   protected IPs for 1–4 h. A dead-silent drop (ICMP + SYN both fail
   while UDP nc reports "succeeded" — the latter is a false positive
   since nc declares UDP success on no ICMP unreachable). Don't try
   to put a new bootstrap on Vultr without DDoS Protection enabled.
5. **Provider throttle vs our bug**: we verified the 4 providers
   we deal with (DO, Hetzner, Vultr, Linode-as-backup). Hetzner had a
   one-time Feb 2026 UDP over-scrub incident, resolved. DO only
   blocks `udp/11211`. Linode moved off null-routing post-Akamai.
   None of them are shaping our traffic — the spin is ours.

---

## First moves for the next session

1. **Don't restart the mesh yet.** Start one single node fresh
   (e.g. `systemctl start x0xd` on `NUR`, one of the Hetzner nodes
   that doesn't have a history of spinning in this session). With
   nothing to gossip to, it should be quiet — confirms `x0xd` is OK
   at rest.
2. **Bring up two nodes** (NUR + NYC). If they spin with just a
   2-node mesh, the bug's amplification factor is 2. If they stay
   clean until we add more, the bug is load-dependent and the
   threshold tells us something.
3. **When a node spins, capture all four of these from the spinning
   PID before you do anything else**:
   - `top -bHn1 -p $pid` (per-thread CPU)
   - `for tid in /proc/$pid/task/*/; do cat $tid/stack; done`
     (kernel stacks — expect "running" if user-space only)
   - `gdb -batch -p $pid -ex "set pagination off" -ex "thread apply
     all bt 25"` with the **profiling binary** so Rust symbols resolve
   - `perf record -F 999 -g -p $pid --call-graph dwarf -- sleep 10`,
     then `perf report --stdio --no-children --percent-limit 0.5`
4. **Look specifically** for the call chain through
   `ant_quic::high_level::endpoint::RecvState::poll_socket` and for
   `saorsa_gossip_pubsub::PubSubManager` / `PlumTree` entries — that's
   where I'd put my money for the residual spin.
5. **`tokio-console`** on a local reproducer. Attach the console
   subscriber, join the local daemon to the public mesh (with VPS
   stopped, bring up 1–2 VPS and the local + console), and watch
   for tasks with `Polls: <rising fast>` and `Time busy: ~100%`.
   That names the offender directly.
6. Cross-check **`../ant-quic/src/nat_traversal_api.rs:5543
   spawn_observed_address_watch_task_parts`** — the `loop { …;
   tokio::select! { observed_change, closed } }` pattern relies on
   `Notify::notified()` being edge-triggered. If the connection state
   calls `notify_one()` on every packet (not just on observed-set
   changes) then this task spins at packet rate.

---

## Repo state

Branch `main`, 3 local commits ahead of `origin/main`:

```
722f80e perf: pick up ant-quic getsockname fixes + add release-profiling profile
585a8e8 infra(vps): retire Vultr nodes — migrate Singapore + Tokyo to DigitalOcean
c07af96 feat(discovery): machine-centric endpoint announcements + v0.18.5 proofs
```

Not pushed. `../ant-quic` `master` has 2 local commits ahead of
`origin/master` (`e6eb4b54`, `43acc666`). Also not pushed.

Push only after the residual spin is understood — a half-fix release
is worse than the current holding-pattern of stopped services.
