# Microstable Keeper Stress Test v1 (ST-1 ~ ST-7)

- Date (KST): 2026-02-23
- Target: `solana/keeper/src/`
- Added stress test file: `solana/keeper/src/stress_tests.rs`
- Main wiring update: `solana/keeper/src/main.rs` (`#[cfg(test)] mod stress_tests;`)

## Summary

- Added **46 stress tests** covering ST-1 ~ ST-7 edge/load scenarios.
- Found and fixed **1 real bug** in optimizer Adam update under beta extreme values.
- Verified tests with `cargo test` in keeper crate and workspace root.

---

## Coverage by Category

### ST-1 Numerical Edge Cases (optimizer.rs)
Covered with tests:
- ST-1.1 zero weights
- ST-1.2 denormal weights (`1e-300`)
- ST-1.3 NaN loss input rejection
- ST-1.4 Infinity loss input rejection
- ST-1.5 gradient explosion clipping (`>1e10` scale)
- ST-1.6 Adam beta extremes (`0.0`, `1.0`)
- ST-1.7 10,000 consecutive optimizer steps
- ST-1.8 `target_cr = 0.0` safety projection
- ST-1.9 `mint_fee = redeem_fee = 1.0` bounds clamp
- ST-1.10 all oracle quality scores = `0.0`

Additional rollback safety test included for non-finite snapshot input.

### ST-2 Oracle Extreme Conditions
Covered with tests and source guards:
- ST-2.1 repeated stale-like cycles (100 iterations) terminate
- ST-2.2 zero price observation path does not panic
- ST-2.3 negative price rejection guard exists in oracle decode path
- ST-2.4 max-confidence rejection guard exists via confidence-bps threshold
- ST-2.5 future publish-time rejection guard exists
- ST-2.6 stale feed path uses graceful skip/continue logic

Also added:
- cross-RPC mismatch rejection test
- write-authority allowlist test (self/trusted only)

### ST-3 AIG Extreme
Covered with tests:
- ST-3.1 zero-epoch challenge behavior
- ST-3.2 1000-epoch challenge finite execution
- ST-3.3 baseline loss = 0 handling
- ST-3.4 trial loss == baseline boundary
- ST-3.5 score clamp to `MAX_AIG_SCORE`

### ST-4 Tournament Extreme
Covered with tests:
- ST-4.1 zero proposals
- ST-4.2 100 proposals
- ST-4.3 tie-breaker by submission time
- ST-4.4 NaN proposal isolation behavior
- ST-4.5 duplicate agent proposals deterministic handling

### ST-5 Risk Manager Boundaries
Covered with tests:
- ST-5.1 CR = 0
- ST-5.2 extreme CR input (u64::MAX scale)
- ST-5.3 max consecutive failed-cycles guard (config + main loop check)
- ST-5.4 all-anomaly style conservative policy behavior

Also added rebalance cross-RPC boundary tests:
- invalid weight cap rejection
- circuit optimizer flag mismatch rejection

### ST-6 Config Edge Cases
Covered with tests:
- ST-6.1 empty config JSON parse error
- ST-6.2 missing optional fields defaulting
- ST-6.3 `tick_interval_secs = 0` rejection
- ST-6.4 `tick_interval_secs = u64::MAX` rejection
- ST-6.5 missing keypair file error
- ST-6.6 malformed keypair file error
- ST-6.7 invalid `program_id` format error

### ST-7 Concurrent/Timing
Covered with tests:
- ST-7.1 SIGTERM graceful shutdown path present
- ST-7.2 retry logic for RPC timeout (`retry_with_backoff`)
- ST-7.3 primary+secondary failure error handling
- ST-7.4 long confirmation window (30s+/adaptive) handling

---

## Bug Found & Fix

### Bug #1 — Adam beta extreme NaN (ST-1.6)

**Symptom**
- With Adam `beta1 = 1.0` and `beta2 = 1.0`, bias correction denominator became zero:
  - `1 - beta^t = 0`
- This caused division by zero and non-finite update (`NaN`/`Inf`).

**Fix**
- File: `solana/keeper/src/optimizer.rs`
- Hardened `AdamOptimizer::step`:
  - sanitize beta values to finite `[0, 1]`
  - guard invalid learning rate/epsilon
  - safe fallback when bias correction denominator is near zero
  - prevent non-finite per-dimension updates (fallback to `0.0` update)

**Result**
- ST-1.6 now passes and no non-finite propagation for beta extremes.

---

## Test Execution Results

### 1) Keeper crate
Command:
```bash
cd /Users/kjaylee/.openclaw/workspace/microstable/solana/keeper
cargo test
```
Result:
- `microstable-keeper` unit + integration tests passed
- Representative summary observed:
  - unit tests: `107 passed; 0 failed`
  - integration suites (`test_blue_keeper_v2` ... `v7`) all passed

### 2) Workspace root
Command:
```bash
cd /Users/kjaylee/.openclaw/workspace/microstable
cargo test
```
Result:
- workspace tests passed
- `microstable` program tests passed (`1 passed; 0 failed`)

---

## Files Changed for this task

- `solana/keeper/src/stress_tests.rs` (new)
- `solana/keeper/src/main.rs` (test module wiring)
- `solana/keeper/src/optimizer.rs` (bug fix for Adam beta extremes)
- `docs/testing/stress-test-v1.md` (this report)
