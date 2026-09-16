# Fleet CPU metrics (issue #656)

> Status: docs-only guidance. Product envelope work owns upstream
> [saorsa-gossip#76](https://github.com/saorsa-labs/saorsa-gossip/issues/76).

## Do not use co-tenant `%CPU` as acceptance

Bootstrap hosts that run **three** `x0xd` daemons on **two** vCPUs (prod `:5483`,
prod `:443`, testnet) cannot yield a meaningful per-daemon `ps`/`pcpu`
acceptance number:

- When one daemon sheds work, the other two absorb the freed CPU.
- `schedstat` wait/run ratios on those hosts show contention between co-tenants,
  not intrinsic daemon cost.
- Wall-clock `pubsub_stages` timers inflate under the same contention and are
  not a pure CPU cost meter.

Prefer **bytes/s**, **verify/s**, delivery matrices, and isolated-host profiles
when judging load. Track the gossip frame floor (double ML-DSA-65 envelope ≈
10.31 KB before payload) via saorsa-gossip#76; do not collapse the x0x V2
envelope without a later ADR GO.

See [#656](https://github.com/saorsa-labs/x0x/issues/656).
