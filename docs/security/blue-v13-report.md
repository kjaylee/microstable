# Microstable Security Patch Report — Blue Team v13

- Date: 2026-02-23 (KST)
- Target report: `docs/security/purple-v16-report.md`
- Base commit: `0615edc`
- Scope:
  - `solana/keeper/src/rebalance.rs`
  - verification review of `solana/programs/microstable/src/lib.rs` (no code change required)

## Executive Summary

Blue v13 applies minimal liveness patches for both Purple v16 findings:

- ✅ **PV16-001 (HIGH)** fixed by freezing `batch_slot` at commit-time and reusing that frozen value at reveal.
- ✅ **PV16-002 (MEDIUM)** fixed by increasing batch-window safety margin from `delay + 1` to `delay + 2`.

Protocol semantics were not expanded; changes are keeper-side guard/consistency fixes only.

## Patch Details

### PV16-001 (HIGH) — Post-commit slot sync rewrote reveal preimage slot

**Root issue:** Keeper computed commit hash with local `batch_slot`, but later replaced stored `batch_slot` with on-chain `pending_rebalance_slot`, causing reveal preimage mismatch.

**Fix applied (keeper):**
- Removed post-commit `pending_rebalance_slot` resync-and-rebind behavior.
- `PendingReveal.batch_slot` now always stores the same `batch_slot` used to compute `commit_hash`.

**File:** `solana/keeper/src/rebalance.rs`

### PV16-002 (MEDIUM) — Insufficient window margin under ±2 drift tolerance

**Root issue:** `batch_window_has_room()` used `remaining > delay + 1`, leaving a boundary dead zone while reveal logic tolerates ±2 drift.

**Fix applied (keeper):**
- Updated margin condition to `remaining > delay + 2`.
- Updated keeper unit test `tc_ow_15_batch_window_has_room_prevents_dead_zone_commits` to reflect the corrected boundary.

**File:** `solana/keeper/src/rebalance.rs`

## Validation

### 1) Keeper tests
```bash
cd solana/keeper && cargo test --quiet
```
Result: **PASS**

- test suites passed: 132 + 8 + 8 + 28 + 25 + 27 + 3
- failed: 0

### 2) On-chain program library tests
```bash
cd solana && cargo test -p microstable --lib --quiet
```
Result: **PASS**

- tests passed: 29
- failed: 0

## Changed Files

- `solana/keeper/src/rebalance.rs`
- `docs/security/blue-v13-report.md`

## Final Status

- Purple v16 findings addressed in Blue v13 patch set:
  - **PV16-001:** fixed
  - **PV16-002:** fixed
- Regression tests and existing library tests pass.