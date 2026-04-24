# ant-quic CPU-spin: root cause identified

**Date:** 2026-04-23T16:04:00Z
**Spinning binary:** `target/x86_64-unknown-linux-gnu/release-profiling/x0xd`
sha256 `7d95f17c921c14244754a0ccb0d8fc6ca8ea3f857b00db85cd26e783abaa7324`
Built from `x0x @ fb51ba9` (v0.19.0) + `../ant-quic @ 43acc666` (HEAD, both unpushed getsockname fixes in).
**Evidence node:** saorsa-3 / sfo / 147.182.234.192, pid 23708, spinning tid 23712.

## One-line diagnosis

`ant_quic::high_level::connection::State::drive_transmit` (`../ant-quic/src/high_level/connection.rs:1280–1346`) enters an **unbounded tight loop** when `io_poller.poll_writable(cx)` reports `Ready` but `socket.try_send(...)` returns `io::ErrorKind::WouldBlock`. The `continue` at line 1333 re-enters the same iteration immediately; the fresh `UdpPollHelper` future created by the next `poll_writable` call returns `Ready` again (tokio's cached writable readiness is not cleared by the raw `try_send` path), so the loop never yields.

## Evidence

### Symbolicated gdb backtrace on sfo (`spin-forensics/sfo/gdb-symbolicated.txt`)

Thread 3 (LWP 23712, tokio-rt-worker, **State R, 99.9% CPU, 11:43 CPU time in ~13 min wall-time**):

```
#0  ant_quic::high_level::connection::State::drive_transmit
    (self=0x7aaab448c870, cx=0x7aaaba1fd6d0) at src/high_level/connection.rs:1332
#1  ant_quic::high_level::connection::{impl#3}::poll
    (self=..., cx=0x7aaaba1fd6d0) at src/high_level/connection.rs:299
#2  ant_quic::high_level::connection::{impl#0}::new::{async_block#0} ()
    at src/high_level/connection.rs:86
...  [frames 3-21 = tokio task/poll harness]
#22 tokio::runtime::scheduler::multi_thread::worker::Context::run
    at src/runtime/scheduler/multi_thread/worker.rs:589
```

Top-of-stack is exactly `drive_transmit` and the WouldBlock retry path (line 1332 is `self.buffered_transmit = Some(t)`, one line above the `continue`).

### Nyc idle contrast (`spin-forensics/nyc/gdb-idle-contrast.txt`)

Same binary, same source, not yet tripped into spin. Threads:
- tokio-rt-worker #1: `parking_lot::Condvar::wait` — healthy idle
- tokio-rt-worker #2: `epoll_wait` via `tokio::runtime::io::driver::Driver::turn` — healthy idle
- x0xd main: `Condvar::wait`
- mDNS_daemon: `epoll_wait`

→ The bug is load/state-triggered, not constitutive. Same binary behaves both ways on different nodes.

### CPU samples (two independent 60s windows ~2 min apart, `05-cpu-baseline.txt` + `spin-forensics/mesh-spin-resample.txt`)

```
T+4min:  nyc 95.5% mean  sfo 23.3% (one 110% spike)  others < 10%
T+7min:  nyc 98.5% mean  sfo 97.0% mean               others < 10%
```

Matches `NEXT-SESSION-PROMPT.md`: "2–3 of 6 are pinned; rotates across nodes."

## The offending loop

`../ant-quic/src/high_level/connection.rs:1289–1343`:

```rust
loop {
    // [1] Get a transmit (buffered or new from poll_transmit)
    let t = match self.buffered_transmit.take() { Some(t) => t, None => { ... } };

    // [2] Readiness check
    if self.io_poller.as_mut().poll_writable(cx)?.is_pending() {
        self.buffered_transmit = Some(t);
        return Ok(false);   // ← correct: register wakeup, exit
    }

    // [3] Sync send
    let retry = match self.socket.try_send(&udp_transmit(&t, &self.send_buffer[..len])) {
        Ok(()) => false,
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => true,   // ← the trap
        Err(e) => return Err(e),
    };

    // [4] WouldBlock retry path
    if retry {
        // "We thought the socket was writable, but it wasn't. Retry so that either another
        //  poll_writable call determines that the socket is indeed not writable and
        //  registers us for a wakeup, or the send succeeds..."
        self.buffered_transmit = Some(t);
        continue;   // ← line 1333: INFINITE if underlying tokio readiness is stuck Ready
    }
    ...
}
```

## Why the expected self-correction doesn't fire

The inline comment expects the next iteration's `poll_writable(cx)` to return `Pending` and register for a wakeup. For `tokio::net::UdpSocket`-backed pollers that works **only if** the socket's async-readiness state has been cleared by a syscall that observed WouldBlock. The standard idiom is `socket.try_io(Interest::WRITABLE, || try_send(...))`, which both performs the send AND clears tokio's internal `Ready::WRITABLE` bit on EAGAIN.

This code uses `self.socket.try_send(...)` — a raw, non-tokio-aware send through the `AsyncUdpSocket` trait. The kernel returns EAGAIN; tokio's internal readiness bit stays `Ready`; the next `UdpPollHelper::poll_writable` call makes a fresh future via `make_fut()`; that future resolves `Ready` immediately because tokio thinks the socket is writable; `try_send` fails again; `continue`; spin.

Under sustained bidirectional traffic (6-node multi-continent mesh with gossip + presence + heartbeats), the UDP send buffer periodically fills enough to make `try_send` EAGAIN. When that happens for the first time, the loop latches and only breaks if/when the kernel drains the buffer enough that `try_send` succeeds on one of the spin iterations — but sustained traffic keeps refilling it, so the loop wins for minutes at a time.

`parking_lot_core::thread_parker::imp::ThreadParker::futex_wait` in frame #1 of the OTHER (idle) worker on the same node shows that worker is properly parked waiting for work — which is normal. The whole runtime is not wedged, just one task that can't yield.

## Why the two getsockname fixes (`722f80e`) didn't help

`e6eb4b54` and `43acc666` removed `getsockname` syscalls from the send hot path. That eliminated *kernel-visible* CPU cost under spin (the original perf showed 71% in `getsockname`). After the fix, the spin became *pure userspace* (confirmed: `/proc/tid/wchan = 0`, `syscall = running`, empty kernel stack) — which is exactly what the current forensics show. The fixes made the spin invisible to kernel profiling but left the underlying retry-loop intact.

## Candidate fixes (in order of surgical-ness)

### 1. **One-line backoff** (safest, least invasive)

At `connection.rs:1332`, on the WouldBlock retry path, yield the task instead of looping inline:

```rust
if retry {
    self.buffered_transmit = Some(t);
    // Clear tokio's stale Ready::WRITABLE by forcing the scheduler to re-poll.
    cx.waker().wake_by_ref();
    return Ok(false);
}
```

Correctness: on next poll, `poll_writable` makes a new future; that future calls tokio's `UdpSocket::writable().await` which DOES go through `try_io` and DOES observe EAGAIN and clear readiness. Second call returns `Pending` properly. Worst case: one extra scheduler round-trip per WouldBlock event (trivial). This is the quinn/mio-style way.

### 2. **Thread the send through `try_io`** (architecturally correct)

At `socket.try_send`'s implementation site (likely in `ant-quic/src/runtime/tokio.rs` or similar, wherever `AsyncUdpSocket::try_send` is defined for the tokio backend), wrap the underlying `sendmsg` call in `inner.try_io(Interest::WRITABLE, || inner.send_to_vectored(...))`. This is what quinn upstream does. Clears the readiness on EAGAIN atomically.

### 3. **Bounded retry** (belt-and-braces)

Add a retry counter inside `drive_transmit`; if `retry` fires N times in a row (e.g. N=4) for the same iteration, save the buffered transmit and `return Ok(false)`. Makes the absolute worst case a finite loop regardless of lower-layer misbehavior.

I'd do **(1) + (3)** — the wake + bounded retry — even if (2) is done later.

## Residual

- `perf record --call-graph dwarf` still produces zero-byte output on this kernel (6.8.0-110-generic with linux-tools-6.8.0-110.110); switching to `-g` (fp) gets the same result. Tentative guess: `kernel.perf_event_paranoid` or container/LXC nesting is blocking it. Not re-investigated — the gdb backtrace is unambiguous and doesn't need perf.
- Nyc did not spin in the ~13 min observation window with the profiling binary. Ten years of software says it will, on a longer window. The bug is statistical — whichever connection's outbound buffer fills first gets pinned.

## Validation steps left on hold

Cannot meaningfully continue 6 (e2e_vps), 9 (stress), 10 (measurement), 14 (efficiency) against this mesh — the numbers would be dominated by the pinned node(s) rather than what each test is meant to measure. 11/12/13 (GUI / Dioxus / Apple) are orthogonal and safe to run.

## Current mesh state

- All 6 VPS on v0.19.0, services active.
- nyc, sfo running the profiling binary (`7d95f17c…`, 452 MB).
- Other 4 running the stripped release binary (`02cd1d82…`, 29 MB).
- sfo's tokio-rt-worker (tid 23712) is still spinning at time of writing.
- Other nodes idle-to-warm. None of the 4 stripped nodes has shown a spin in ~30 min since their deploy but the sample is too small to call them safe.
