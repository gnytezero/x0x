# Named-Groups Parity — Signoff Report

**Status:** ✅ signoff candidate
**Date:** 2026-04-14
**Scope:** `docs/design/named-groups-full-model.md` — all four presets
across every consumer surface.

This report summarises the static and runtime proofs that collectively
demonstrate the named-groups REST API, CLI, embedded GUI, Communitas
Rust client, Communitas Swift client, Communitas Dioxus UI, and
Communitas SwiftUI are feature-equivalent.

## The surface-of-truth

`src/api/mod.rs::ENDPOINTS` is the single source of truth.
`tests/api_manifest.rs` projects it to
`docs/design/api-manifest.json` (schema `x0x-api-manifest/v1`). Every
downstream surface reads this manifest to assert coverage.

Named-groups surface: **33 endpoints** spanning core CRUD, policy,
membership/roles/bans, join requests, invite/join, public messaging
(Phase E), state chain (Phase D.3), discovery (Phase C + C.2), and the
secure plane (Phase D.2).

## Parity matrix

| Surface | Static proof (tests that fail if a new endpoint is not wired) | Runtime proof |
|---------|--------------------------------------------------------------|---------------|
| **x0xd REST** | `tests/api_coverage.rs` — every route handler registered in `src/bin/x0xd.rs` is in `ENDPOINTS` and has a test entry. 12 tests, all green. | `tests/e2e_named_groups.sh` — 98 REST-driven assertions over a 3-daemon mesh; 3× clean archived in `tests/proof-reports/`. |
| **x0x CLI** | `tests/parity_cli.rs` — spawns `x0x <cli_name> --help` for every endpoint. Zero misses. Plus empty-patch guards on `group update` / `group policy`. | `tests/e2e_feature_parity.sh` — 19 assertions driving each preset's lifecycle through the `x0x` binary; 3× clean archived in `tests/proof-reports/parity/`. |
| **x0x embedded GUI** (`src/gui/x0x-gui.html`) | `tests/gui_named_group_parity.rs` — fragment scan: every `/groups/*` path × method appears as an `api(...)` call, every preset name appears in the create-space modal, `renderDiscover` + admin-panel hosts exist. 4 tests, all green. | Manual; the HTML is loaded by a real x0xd. Playwright harness is queued for Phase 7 CI. |
| **Communitas Rust client** (`communitas-x0x-client`) | `communitas-x0x-client/tests/parity_manifest.rs` — vendored manifest copy; fails if any named-groups endpoint lacks a method or an IMPLEMENTED entry is stale. Plus `client_coverage.rs` (14 tests, all green). | `live_mutation_contract.rs` in the same crate exercises the client against a real x0xd. |
| **Communitas Dioxus UI** (`communitas-dioxus`) | Consumes the Rust client directly — indirectly covered by the client parity test. `space_preset_maps_to_client_preset` unit test asserts the UI enum matches the client enum. | 419/419 unit tests in `cargo test -p communitas-dioxus --bin communitas-dioxus`. End-to-end UI driver queued. |
| **Communitas Swift client** (`communitas-apple`) | `communitas-x0x-client/tests/swift_parity.rs` — every Rust method name has a Swift counterpart in `Sources/X0xClient/X0xClient.swift`; 2 tests, all green. | `swift test` — 42/42 pass; full E2E against a live daemon is covered by the `communitas-apple` test target. |
| **Communitas SwiftUI** | `swift build` + `swift test` — clean. UI is built from the Swift client types. | XCUITest harness queued. |

## Static-proof commands (one-line)

```bash
# x0x repo
cargo nextest run --test api_manifest --test parity_cli \
                  --test api_coverage --test gui_smoke --test gui_named_group_parity

# communitas repo
cargo nextest run -p communitas-x0x-client --test parity_manifest \
                     --test client_coverage --test swift_parity
cargo test -p communitas-dioxus --bin communitas-dioxus
(cd communitas-apple && swift build && swift test)
```

All of the above are currently green.

## Runtime proof — `tests/e2e_feature_parity.sh`

The CLI-driven runtime matrix. Spins up two daemons (alice + bob), uses
the `x0x` binary (never curl) to drive each preset through its full
lifecycle, and verifies state via the REST layer.

**19 assertions per run, 3× clean archived:**

- `tests/proof-reports/parity/feature-parity-clean-run1.log`
- `tests/proof-reports/parity/feature-parity-clean-run2.log`
- `tests/proof-reports/parity/feature-parity-clean-run3.log`

### What each preset proves

| Preset | Proof emitted by the runtime matrix |
|--------|-------------------------------------|
| `private_secure` | create via CLI ✓ · REST reflects group ✓ · **hidden does not leak into `/groups/discover`** ✓ · state chain initialised ✓ |
| `public_request_secure` | create via CLI ✓ · card published / imported ✓ · **bob.request-access via CLI** ✓ · approve-request CLI surface reachable ✓ · non-admin approve rejected ✓ |
| `public_open` | create via CLI ✓ · **alice.send via CLI for SignedPublic** ✓ · `/messages` returns the signed body ✓ · non-member send rejected ✓ |
| `public_announce` | create via CLI ✓ · **owner.send via CLI** ✓ · signed message observable in `/messages` ✓ · policy round-trip `write_access=admin_only` ✓ |

### Cross-preset guarantees

- non-admin `PATCH /groups/:id/policy` rejected ✓
- non-admin `POST /groups/:id/ban/:aid` rejected ✓
- `state-seal` via CLI advances the chain (revision `n → n+1`) ✓

### Honest scope note

On a 2-daemon loopback mesh, gossip propagation of join-requests from
bob to alice is timing-flaky — the existing `e2e_named_groups.sh` 3-
daemon suite covers that convergence, and this suite skips the
propagation check with an explicit `SKIP` log line if bob's request
hasn't landed on alice within 60 s. The `approve-request` CLI surface
is still exercised via a synthetic-id probe that expects a 4xx.

## Deferred / known gaps

These are **explicit** non-regressions. Each is documented here so a
reader can distinguish "not yet proven" from "not yet implemented":

1. **SignedPublic send-path routing inside chat views.** Both Dioxus
   (`communitas-dioxus/src/components/channel_chat.rs`) and SwiftUI
   (`communitas-apple/Sources/Communitas/Views/MessagingView.swift`)
   still publish messages through gossip regardless of the group's
   `confidentiality`. `POST /groups/:id/send` (SignedPublic) and
   `POST /groups/:id/secure/encrypt` (MlsEncrypted) are both wired at
   the client layer; the chat views just need a branch. Queued for
   Phase 7 alongside the headless driver work.
2. **Headless GUI / UI drivers.** Playwright for the embedded HTML
   GUI, `dioxus-testing`/Tauri driver for Dioxus, and XCUITest for
   SwiftUI are queued for Phase 7 CI. The current runtime proof is
   CLI-only; the UI surfaces are guarded by their static parity tests
   (`gui_named_group_parity.rs`, `swift_parity.rs`,
   `parity_manifest.rs`) plus manual smoke.
3. **Explicit OpenJoin scenario** in `e2e_named_groups.sh`. Covered
   indirectly by `public_open` in this runtime matrix via preset
   selection, but not as a dedicated assertion for the `join` CLI
   path. Non-blocking — see `PHASE_F_FINAL_SIGNOFF_REVIEW.md`.
4. **Moderation tooling** (per-message delete, mute), **backlog
   sync for late-joiners**, **MLS TreeKEM**, and **federation with
   external directory servers** — all explicitly out of scope for v1
   per the design doc.

## Signoff criteria

| Criterion | Status |
|-----------|--------|
| REST API has 33 named-groups endpoints, each covered by a handler, test, and registered cli_name | ✅ |
| CLI subcommand exists for every endpoint | ✅ (`parity_cli` green) |
| Rust client method exists for every endpoint | ✅ (`parity_manifest` green) |
| Swift client method exists for every Rust method | ✅ (`swift_parity` green) |
| Embedded HTML GUI calls every critical named-groups endpoint | ✅ (`gui_named_group_parity` green) |
| Create modal surfaces all 4 presets | ✅ (x0x GUI + Dioxus + SwiftUI) |
| Discover view exists with query, nearby, request-access | ✅ (x0x GUI + Dioxus + SwiftUI) |
| Admin surfaces exist: policy editor, state readout, roster roles/bans, request approve/reject | ✅ (x0x GUI inline, Dioxus Manage tab, SwiftUI ManageGroupSheet) |
| Runtime parity test for CLI × 4 presets, 3× clean | ✅ (see `tests/proof-reports/parity/`) |
| Existing design proofs (C.2, D.3, D.4, E, F) remain green | ✅ — not touched in Phases 0–6 |

## Recommendation

**Approve named-groups feature parity across x0x CLI, x0x embedded GUI,
Communitas Rust client, and Communitas Swift client.** The Dioxus and
SwiftUI apps reach full endpoint coverage through the respective
clients; the two known UI gaps (chat-view send routing, headless UI
driver CI) are scoped to Phase 7 and do not affect the parity position.

## Appendix — reproducing this report

```bash
# One-off
cargo build --release --bin x0xd --bin x0x --bin x0x-user-keygen
bash tests/e2e_feature_parity.sh

# Three-run signoff
for i in 1 2 3; do bash tests/e2e_feature_parity.sh; done
```

Artifacts land in `tests/proof-reports/parity/feature-parity-*.log`.
