# Microstable Purple-Keeper v5 — Final Zero Verification Audit

Date: 2026-02-22 (KST)  
Target: Blue-Keeper v5 patch set

## Verdict
**ZERO FINDINGS 목표 미달성**

- Total findings: **2**
- Severity: **HIGH 1 / MEDIUM 1**

---

## Scope
Audited files:
- `solana/keeper/src/main.rs`
- `solana/keeper/src/oracle.rs`
- `solana/keeper/src/rebalance.rs`
- `solana/keeper/src/monitor.rs`
- `solana/keeper/src/watchdog.rs`
- `solana/keeper/src/config.rs`
- `solana/keeper/src/utils.rs`
- `solana/keeper/src/wire.rs`
- `solana/keeper/build.rs`

History references checked:
1. Purple-Keeper v1 (PK-001~010)
2. Red-Keeper (RK-001~006 exploited set 포함)
3. Purple-Keeper v2 (PKV2-001~003)
4. Purple-Keeper v3 (PKV3-001~003)
5. Purple-Keeper v4 (PKV4-001~002)

Validation run:
- `cd solana/keeper && cargo test` → **pass** (v2/v3/v4/v5 suites all pass)
- `cd solana/keeper && cargo run -- --once` → startup attestation pass confirmed; cycle path behavior observed

---

## PKV4 Patch Completeness Check

### PKV4-001 (Cargo.lock attestation / chicken-and-egg)
- **Result: Implemented as designed**
- `build.rs` embeds compile-time Cargo.lock SHA-256 (`KEEPER_CARGO_LOCK_HASH`) and runtime verifies lock bytes against embedded hash.
- Self-binary hash chicken-and-egg loop from v4 is removed.

### PKV4-002 (adaptive confirm + degraded mode DoS mitigation)
- **Result: Partially fixed / residual vulnerabilities remain**
- Adaptive confirm window + warning-only secondary failure path exists.
- However degraded-mode state transitions are not wired to cross-RPC **read-path** failures, so secondary instability still causes cycle failures and eventual keeper exit.

---

## Findings

## PKV5-001 — Degraded mode does not absorb read-path secondary failures (DoS persists)
- **Severity:** HIGH
- **File / Line:**
  - `solana/keeper/src/main.rs:148-180, 246-254, 258-347`
  - `solana/keeper/src/oracle.rs:97-124`
  - `solana/keeper/src/rebalance.rs:50-77`
  - `solana/keeper/src/monitor.rs:152-195`
  - `solana/keeper/src/watchdog.rs:48-77`
  - `solana/keeper/src/utils.rs:87-99, 771-946`
- **Attack scenario:**
  1. Attacker degrades/blocks secondary RPC read consistency (outage, partition, forced mismatch).
  2. Oracle/rebalance/monitor/watchdog all return `Err` after cross-RPC retries.
  3. `run_cycle()` aggregates module errors; main loop increments `consecutive_failed_cycles`.
  4. Keeper exits at configured threshold (`max_consecutive_failed_cycles`) → operator intervention required.
- **Impact:** Remote liveness DoS remains feasible despite v5 degraded-mode design goal.
- **Evidence:**
  - `register_secondary_rpc_failure()` is only called from startup checks and tx confirmation path (`main.rs:81,96`, `utils.rs:930`), not from module read cross-RPC failures.
  - Module read failures propagate as cycle failures (`oracle/rebalance/monitor/watchdog` sections above).
  - Runtime observation (`cargo run -- --once`) showed secondary health-check failure logs with `degraded=false`, followed by module-level cross-RPC failure chain and cycle failure.

## PKV5-002 — Primary-only tx confirmation acceptance reopens single-RPC trust on tx outcome
- **Severity:** MEDIUM
- **File / Line:**
  - `solana/keeper/src/utils.rs:125-142, 948-959`
  - `solana/keeper/tests/test_blue_keeper_v5.rs:66-69`
- **Attack scenario:**
  1. Primary RPC reports transaction confirmed, while secondary cannot confirm (lag/partition/adversarial behavior).
  2. `assess_tx_confirmation_outcome()` accepts success if **either** side confirms.
  3. Keeper proceeds as successful (`warning only`) on primary-only confirmation.
- **Impact:** Regression of PKV3-003 class risk (single-RPC trust on confirmation semantics): false-positive success state and protection/liveness drift under adversarial primary or split-brain conditions.
- **Evidence:**
  - Acceptance condition is `if primary_confirmed || secondary_confirmed { Ok(()) }`.
  - Secondary-missing path is explicitly warning-only (`"secondary confirmation missing (warning only)"`).
  - v5 test explicitly encodes policy: `tc_pkv4_002_primary_only_confirmation_is_accepted`.

---

## Focused Checks Requested in v5 Scope

1. **Cargo.lock attestation chicken-and-egg**
   - Fixed relative to v4 self-hash loop.

2. **Adaptive confirm + degraded mode DoS resistance**
   - Partial only. Write-path degraded handling exists, but read-path cross-RPC failure handling still fail-stop.

3. **Degraded mode race condition**
   - No direct mutex race found in current single-process state handling.
   - Main issue is logic coverage gap (state transition source incompleteness), not data race.

4. **Cargo.lock hash TOCTOU**
   - No direct hash-check TOCTOU bypass observed in runtime flow (single in-memory lockfile copy used for hash+source validation).

5. **Adaptive timeout leniency**
   - Bounded to 60s extension; however trust policy still accepts primary-only confirmation (Finding PKV5-002).

---

## Conclusion
Blue-Keeper v5 improved PKV4-001 implementation and added adaptive/degraded logic, but **final zero-verification is not met** due two residual security issues:
1. **HIGH:** read-path degraded handling gap enables keeper fail-stop DoS.
2. **MEDIUM:** primary-only confirmation acceptance reintroduces single-RPC trust for tx outcome.
