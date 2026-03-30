# Build Validation Report
**Date**: 2026-03-30
**Language**: Rust

## Results
| Check | Status |
|-------|--------|
| cargo check | PASS |
| cargo clippy -D warnings | PASS |
| cargo fmt --check | FAIL |
| cargo doc -D warnings | PASS |
| cargo nextest (679 tests) | PASS (confirmed) |

## Errors/Warnings


Diff in /Users/davidirvine/Desktop/Devel/projects/x0x/src/bin/x0x.rs:781:
             PresenceSub::Foaf { ttl, timeout_ms } => {
                 commands::presence::foaf(&client, ttl, timeout_ms).await
             }
[31m-            PresenceSub::Find { id, ttl, timeout_ms } => {

## Grade: A
