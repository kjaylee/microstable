# Microstable Security Audit — Purple Team v16 (FINAL Zero Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v16
- Target patch: Blue v12 (`f85d495`)
- Scope:
  - Keeper rebalance path: `solana/keeper/src/rebalance.rs`
  - On-chain commit/reveal verifier: `solana/programs/microstable/src/lib.rs`
  - Prior findings quick sweep (RV5/PV14/PV15 classes)

---

## Executive Result

Blue v12 is **NOT** a zero-finding release.

New verification found **2 liveness findings**:

1. **PV16-001 (HIGH)** — Post-commit slot “sync” rewrites `batch_slot` preimage, causing deterministic `CommitRevealMismatch` when commit execution slot drifts.
2. **PV16-002 (MEDIUM)** — `batch_window_has_room()` margin is insufficient for the documented ±2 drift tolerance; an edge dead zone remains.

**ZERO NEW FINDINGS: NO**

---

## Claim-by-Claim Verification of Blue v12

### 1) “Slot drift fix” (re-read `pending_rebalance_slot` and sync `batch_slot`)
- Implemented at keeper lines `385-406`.
- Commit hash preimage is built using pre-submit `batch_slot` at lines `344-351`.
- On-chain reveal verifies hash using caller-provided `batch_slot` (`lib.rs:1333-1337`, hash fn `lib.rs:3017-3023`).

**Verdict: INCOMPLETE / REGRESSION RISK.**
Rebinding local `batch_slot` after commit can desynchronize reveal preimage from committed hash.

### 2) “Batch window dead zone fix” (`batch_window_has_room()`)
- Implemented at keeper lines `331-342`, helper at `901-909`.

**Verdict: PARTIAL.**
Condition uses `remaining > delay + 1`, but drift model now tolerates up to ±2 (`911-922`). For +2 drift near boundary (slot%32==25, delay=5), commit can still become unrevealable.

### 3) “Drift tolerance in `deferred_reveal_ready()` ±2 + same window"
- Implemented at keeper lines `917-922`.

**Verdict: LOGIC EXISTS but operationally insufficient.**
Because stored `batch_slot` is overwritten to on-chain pending slot (`402-406`), this helper no longer protects the original commit preimage integrity.

### 4) “4 new tests”
- Added and passing: `tc_ow_13..15` (`1328-1389`).

**Verdict: PRESENT but coverage gap remains.**
No integration/unit test validates that post-commit slot sync preserves commit-hash preimage compatibility during reveal.

---

## New Findings

## PV16-001 — Post-commit `batch_slot` rebinding breaks commit/reveal preimage
- Severity: **HIGH**
- Category: DoS / Liveness
- Affected:
  - `solana/keeper/src/rebalance.rs:344-351, 385-406, 201-211`
  - `solana/programs/microstable/src/lib.rs:1333-1337, 3017-3023`

### Root Cause
1. Keeper computes `commit_hash = H(protocol, weights, batch_slot_local, salt)` before commit tx.
2. After commit lands, keeper overwrites stored `batch_slot` with `pending_rebalance_slot` from chain.
3. Deferred reveal later sends this overwritten `batch_slot`.
4. On-chain recomputes expected hash using reveal-provided `batch_slot` and compares to committed hash.

If tx execution slot != local observed slot (common 1-slot drift), hashes diverge and reveal fails with `CommitRevealMismatch`.

### Deterministic Evidence
- Keeper test already proves hash depends on `batch_slot` (`tc_ow_08`, lines `1248-1258`).
- Reproduction hash sample (same preimage schema):
  - `H(..., batch_slot=1000, ...) = 707cf2211f1eaaee7455eb41656e5ed5be3ea8907c59a74f41bf8f1aaf13360a`
  - `H(..., batch_slot=1001, ...) = ec51cac749f3204cace9399f17e63fa2edda5f9b70fda9035efd18119ddd08c1`
  - mismatch = `true`

### Impact
Default mode is deferred (`execute_rebalance_immediately=false` in keeper defaults/devnet), so commit cycles can repeatedly fail reveal under normal slot drift, degrading rebalancing liveness.

---

## PV16-002 — Residual edge dead zone under +2 drift (window-room off-by-one)
- Severity: **MEDIUM**
- Category: Liveness edge case
- Affected:
  - `solana/keeper/src/rebalance.rs:904-909`

### Root Cause
`batch_window_has_room()` enforces:
- `remaining > delay + 1`

But helper accepts up to ±2 drift (`917-919`). With +2 execution drift, this room condition is insufficient at boundary position 25 (delay=5).

### Deterministic Evidence
Using keeper’s own formulas:
- `slot=25, delay=5` → room check passes (`remaining=7 > 6`).
- If commit executes at `27` (+2 drift), earliest reveal `32` is next window, so `deferred_reveal_ready` fails same-window condition.

### Impact
Rare but deterministic boundary liveness failures remain under tolerated drift envelope.

---

## Deferred Reveal Reachability Assessment

- Reachable when **all** are true:
  - active pending commit,
  - delay elapsed,
  - same batch window,
  - reveal preimage (`weights`, `salt`, **original batch_slot**) matches committed hash.
- In Blue v12 current implementation, preimage integrity is not preserved under slot drift because `batch_slot` is rewritten post-commit.

**Conclusion:** deferred reveal is **not reachable in all normal scenarios**.

---

## Quick Sweep of Prior Findings

No regressions observed in previously closed classes outside the rebalance path above:
- RV5-005 dynamic fee separation still present (`mint_fee_rate`/`redeem_fee_rate`, legacy sync retained).
- RV5-006 sybil hardening controls remain (1 SOL min stake, `registered_slot`, tournament age/cap/entropy gates).
- RV5-007 4-feed coverage enforcement remains in keeper config validation.
- RV5-008 isolation fail-closed script behavior unchanged.
- RV5-013 unilateral slash controls unchanged.

---

## Commands Executed

- `cargo test --quiet` (keeper) → **PASS**
- `cargo test -p microstable --lib --quiet` (on-chain) → **PASS**
- Deterministic slot/window and commit-hash model checks (local Python) → reproduced PV16-001 and PV16-002 conditions

---

## Final Assessment

- **ZERO NEW FINDINGS:** **NO**
- New findings: **2**
  - **PV16-001 (HIGH)** — commit/reveal preimage broken by post-commit slot rebinding
  - **PV16-002 (MEDIUM)** — residual +2 drift boundary dead zone
