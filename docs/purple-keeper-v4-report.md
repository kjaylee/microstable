# Microstable Purple-Keeper v4 — Zero Verification Audit

Date: 2026-02-22 (KST)  
Target: `microstable` @ `e12632f` (working tree includes Blue-Keeper v4 patch set)

## Verdict
**ZERO FINDINGS 목표 미달성**

- Total findings: **2**
- Severity: **HIGH 2**

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
- `solana/keeper/build.rs` (new)

Audit references:
1. `docs/purple-keeper-report.md` (PK-001~010)
2. `docs/red-keeper-report.md` (RK-001~006 exploited set 포함)
3. `docs/blue-keeper-v2-tc.md`
4. `docs/purple-keeper-v2-report.md` (PKV2-001~003)
5. `docs/blue-keeper-v3-tc.md`
6. `docs/purple-keeper-v3-report.md` (PKV3-001~003)
7. `docs/blue-keeper-v4-tc.md`

Verification runs:
- `cd solana/keeper && cargo test` → pass (v2/v3/v4 suites all pass)
- `cd solana/keeper && cargo run -- --once` → **fails at startup attestation** (evidence below)

---

## PKV3 Patch Completeness Recheck

- **PKV3-001 (strict cross-RPC equality DoS):** Partially improved (tolerance + retry/backoff added).
- **PKV3-002 (runtime-overridable trust anchor):** New embedded hash path introduced, but implementation is non-bootstrappable in current form (Finding PKV4-001).
- **PKV3-003 (single-RPC submit trust):** Dual-submit/dual-confirm path introduced, but confirmation policy creates race/split-brain liveness failure surface (Finding PKV4-002).

Regression check for prior PK/RK/PKV2 sets:
- No direct re-open of PK-004/005/006/007/009 class bugs found in v4 patch area.
- Residual risk now concentrated in **supply-chain attestation implementation** and **dual-RPC submit/confirm semantics**.

---

## Findings

## PKV4-001 — Compile-time embedded binary hash attestation is non-bootstrappable (startup fail-stop DoS)
- **Severity:** HIGH
- **File / Line:**
  - `solana/keeper/build.rs:1-20`
  - `solana/keeper/src/utils.rs:728-746, 763-795`
  - `solana/keeper/src/main.rs:45`
- **Attack scenario:**
  1. Build pipeline sets `KEEPER_BUILD_HASH` (or leaves default zero hash).
  2. Runtime always verifies binary SHA-256 against compile-time embedded value.
  3. Because embedding the expected hash changes the binary, a simple one-pass build cannot satisfy self-hash equality.
  4. Keeper exits before any keeper cycle (`main()` hard-calls `enforce_supply_chain_controls()`), causing operational halt.
- **Impact:** Keeper daemon startup DoS / liveness loss. A CI/env tampering actor can force persistent non-start by controlling `KEEPER_BUILD_HASH` input.
- **Evidence:**
  - Runtime hard gate:
    - `utils::enforce_supply_chain_controls()?` in `main.rs:45`
    - `verify_binary_attestation_for_bytes(...)?` in `utils.rs:745-746`
  - Repro 1 (default build):
    - `cargo run -- --once` → `binary sha256 mismatch: expected 000...000, got <actual>`
  - Repro 2 (iterative embed attempts):
    - Build with unset hash → `H1=1b2b...`
    - Rebuild with `KEEPER_BUILD_HASH=H1` → `H2=0397...` (`H1 != H2`)
    - Rebuild with `KEEPER_BUILD_HASH=H2` → `H3=f479...` (`H2 != H3`)
    - Subsequent run still mismatched (`expected H3, got different actual`).

## PKV4-002 — Dual-RPC confirmation policy has race/split-brain liveness failure (secondary veto + short confirm window)
- **Severity:** HIGH
- **File / Line:**
  - `solana/keeper/src/utils.rs:25-28, 579-597, 607-694, 696-723`
  - `solana/keeper/src/rebalance.rs:212-213`
  - `solana/keeper/src/main.rs:145-156`
- **Attack scenario:**
  1. Keeper submits tx to both RPCs (`send_transaction_with_config`), then immediately confirms each side.
  2. Confirmation retry budget is only 3 attempts with 40/80ms backoff (`~120ms` sleep budget).
  3. If primary confirms but secondary is delayed/partitioned/malicious and reports unconfirmed, policy rejects (`primary=true, secondary=false` => Err).
  4. Rebalance path propagates `?` on send failure; repeated cycle errors hit `max_consecutive_failed_cycles` and daemon exits.
- **Impact:** Remote liveness DoS via secondary RPC lag/manipulation; split-brain confirmation semantics can fail closed even when tx is actually accepted on-chain via primary.
- **Evidence:**
  - Asymmetric confirmation policy in `assess_dual_rpc_confirmation()`:
    - Accepts `(false,true)` and `(true,true)`
    - Rejects `(true,false)`
  - Short confirmation loop in `confirm_signature_with_retry()` with constants:
    - `CROSS_RPC_MAX_ATTEMPTS=3`, `CROSS_RPC_BACKOFF_BASE_MS=40`
  - Error propagation chain:
    - `rebalance.rs:212-213` uses `utils::send_instructions(...)?`
    - `main.rs:145-156` increments consecutive failures and exits at threshold.

---

## Focused Checks Requested in v4 Scope

1. **Tolerance-based comparison 공격 가능성**
   - Current tolerance is narrow (`±1` numeric, `±1s` time). Broad spoofing bypass는 어려움.
   - 다만 decision path가 primary snapshot을 기준으로 진행되므로 경계값(±1) 근처에서 동작 차이가 날 수 있음.

2. **build.rs 해시 embed 우회 가능성**
   - 현 구현은 우회보다 먼저 **운영 불가(bootstrapping failure)** 리스크가 지배적 (PKV4-001).

3. **Dual-RPC TX submit race/split-brain**
   - 확인 로직이 비대칭 + 짧은 confirm window로 인해 liveness 취약점 존재 (PKV4-002).

---

## Conclusion
Blue-Keeper v4는 PKV3 대응을 추가했지만, **최종 zero-vulnerability 상태는 아님**.

우선순위 수정 권고:
1. **Attestation trust model 재설계**: self-hash 고정점 요구 제거(예: signed manifest / external immutable provenance).
2. **TX confirmation policy 개선**: 충분한 confirm horizon + 비대칭 veto 제거 + canonical/finality-aware reconciliation.
