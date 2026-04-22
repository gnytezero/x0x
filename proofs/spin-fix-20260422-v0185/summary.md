# x0xd CPU-spin hunt — 2026-04-22 (partial fix)

## Symptom

Under sustained six-node bootstrap-mesh traffic, at any given probe
moment 1–3 of 6 VPS nodes had `x0xd` pinned at 100–200% CPU with
`/health` timing out. `journalctl --since='1 minute ago'` returned
"No entries" — the daemon was *doing work* but making no observable
progress. A `systemctl restart x0xd` on the affected node cleared
the state for a while, but the next node in the mesh would eventually
drop into the same spin.

## What was fixed (shipped to `ant-quic`)

Two `getsockname(2)` syscalls per packet / per poll identified via
`perf record -g --call-graph dwarf` on a live VPS. Both fixes in
`ant-quic/src/high_level/runtime/dual_stack.rs`:

1. **`perf(dual_stack): cache socket family instead of per-packet getsockname`**
   (ant-quic `e6eb4b54`).
   `DualStackSocket::try_send` was calling `socket.local_addr()?.is_ipv6()`
   on every outgoing datagram to decide whether to map the destination
   to an IPv4-mapped-IPv6 address. `select_socket` already knew which
   of `self.v4` / `self.v6` it was returning, so it now returns
   `(&socket, is_v6: bool)` and `try_send` passes the boolean to
   `convert_dest` directly. Pre-fix profile showed ~71% CPU inside
   `getsockname` → `inet_getname` → `release_sock` / `_raw_spin_lock_bh`
   on the spinning worker.

2. **`perf(dual_stack): cache local_addr so AsyncUdpSocket::local_addr is syscall-free`**
   (ant-quic `43acc666`).
   quinn's `ConnectionDriver::poll` invokes `AsyncUdpSocket::local_addr`
   every iteration; the previous impl delegated to
   `tokio::net::UdpSocket::local_addr()` (another `getsockname`).
   Since the sockets never rebind, the bound addresses are now cached
   in `DualStackSocket::{v4_addr, v6_addr}` at construction and every
   `local_addr*` / `Debug` path reads the cached values.

Both patches: 28/28 existing `dual_stack` + `masque` + `transport::udp`
tests pass.

The symbolicated "latched spin" stack that made both fixes obvious:

```
#0  __GI_getsockname
#1  std::sys::net::…::socket_addr
#2  std::sys::net::…::sockname
#3  std::sys::net::…::TcpStream::socket_addr        (std shares with UdpSocket)
#4  std::net::tcp::TcpStream::local_addr
#5  tokio::net::udp::UdpSocket::local_addr
#6  ant_quic::…::DualStackSocket::convert_dest       ← bug 1
#7  ant_quic::…::DualStackSocket::try_send
#8  ant_quic::…::connection::State::drive_transmit
#9  ant_quic::…::connection::ConnectionDriver::poll  ← calls this every iteration
```

## What's still open

After both fixes deployed to every node, sustained 5–7 minute
6-node mesh observation still shows a fraction of the nodes pinning
CPU at 90–200%. `gdb -batch -p $pid -ex "thread apply all bt"` on a
spinning worker now shows the thread in `parking_lot::Condvar::wait`
(healthy idle) — the bursts are no longer an infinite `getsockname`
loop. The remaining CPU is plausibly legitimate ML-DSA-65 verification
over the accumulated gossip / announcement traffic on a 2-vCPU
droplet, but that has not yet been definitively separated from a
third spin candidate (e.g. a busy tokio task that wakes itself every
poll). That is the hunt to pick up next session.

## Proof artefacts

- `pre-fix-perf.txt` — first symbolicated capture showing 71% `getsockname`
  under `try_send`.
- `mid-fix-perf.txt` — capture after try_send patch, showing 34%
  `getsockname` now under `local_addr`.
- `post-fix-gdb-healthy.txt` — `thread apply all bt` from a freshly-
  restarted node running the fully-patched binary; tokio workers in
  `Condvar::wait` (idle).
- `mesh-watch.log` — 5-minute 6-node CPU/peer watch after both fixes.

## Gotchas

Cargo's `[profile.release-profiling] inherits = "release"` does NOT
rebuild the `ant-quic` rlib when the *profile* changes but the source
doesn't — only when you also `touch` a source file on top. Stale
`release-profiling` artefacts burned hours earlier in this hunt.
Fix: `cargo clean --target x86_64-unknown-linux-gnu --profile release-profiling`
plus the `split-debuginfo = "off"` + `debug = "full"` profile change
so gdb works without chasing `.dwo` files.
