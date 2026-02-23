# Microstable Security Audit — Purple Team v15 (Final Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v15
- Scope:
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper: all `solana/keeper/src/*.rs`
  - Ops script: `solana/keeper/scripts/verify-isolation.sh`
- Patch under verification: `39262ea` (Blue v11)

---

## Executive Result

Blue v11 **partially** remediates PV14-001 by making deferred reveal no longer *universally* unreachable.

However, this round found **1 HIGH liveness vulnerability** in the same rebalance commit/reveal path:

- **PV15-001 (HIGH)** — Deferred reveal remains brittle/unreachable under real slot timing and has a deterministic batch-window dead zone.

Therefore, **this is NOT a zero-finding round**.

---

## 1) PV14-001 Verification (Blue v11)

### What is fixed
Blue v11 changed:
- `select_batch_slot()` to return commit slot (`solana/keeper/src/rebalance.rs:867-869`)
- Added `deferred_reveal_ready()` with delay gating (`solana/keeper/src/rebalance.rs:871-879`)
- Deferred path now uses helper (`solana/keeper/src/rebalance.rs:192-199`)
- Added test `tc_ow_12_deferred_reveal_becomes_reachable_after_delay` (`solana/keeper/src/rebalance.rs:1266-1283`)

This resolves the v14 logic bug where `batch_slot` was `commit+delay` while keeper required `batch_slot == protocol.pending_rebalance_slot`.

### Why it is not fully solved
The new equality condition still relies on strict slot identity between local observed slot and on-chain commit execution slot, and it now interacts badly with the on-chain batch-window gate.

---

## 2) New Finding

## PV15-001 — Deferred reveal still stalls due to slot-equality brittleness + batch-window dead zone
- Severity: **HIGH**
- Category: **DoS / Liveness**
- Affected:
  - `solana/keeper/src/rebalance.rs`
  - `solana/programs/microstable/src/lib.rs`

### Root cause A — strict slot equality to on-chain commit slot
Keeper stores local `batch_slot` from pre-submit `rpc.get_slot()` and later requires exact equality with on-chain `pending_rebalance_slot`:
- `current_slot` read before commit: `solana/keeper/src/rebalance.rs:150`
- local `batch_slot = select_batch_slot(current_slot)`: `:331`
- commit sent: `:356-363`
- deferred reveal requires `batch_slot == protocol.pending_rebalance_slot`: helper at `:877`
- deferred branch filter uses helper: `:192-199`

On-chain pending slot is set at transaction execution time:
- `protocol.pending_rebalance_slot = slot`: `solana/programs/microstable/src/lib.rs:1265`

If tx lands even 1 slot later than keeper’s observed slot, deferred branch can never match, so reveal preimage is never used.

### Root cause B — deterministic dead zone near end of batch window
On-chain reveal always enforces:
- `validate_batch_window(slot, batch_slot)`: `solana/programs/microstable/src/lib.rs:1296`
- implementation: `slot/32 == batch_slot/32`: `:3027-3032`

Keeper now sets `batch_slot = commit slot` (`solana/keeper/src/rebalance.rs:867-869`) and deferred readiness requires reveal delay (`:878`).

For commits in the last `delay` slots of a 32-slot window (default delay=5), earliest reveal is already in the next window, so on-chain validation fails forever.

### Reproduction evidence (deterministic model)
Observed via local model run:
- slot drift case (`batch_slot=1000`, on-chain pending slot `1001`) => deferred readiness remains false even after delay.
- edge case (`commit_slot mod 32 in {27..31}` with delay 5) => deferred readiness true but on-chain `validate_batch_window` always false.

### Impact
In default config (`execute_rebalance_immediately: false` in `solana/keeper/src/config.rs:197`, `solana/keeper/config.devnet.json:51`), keeper relies on deferred reveal path.

This can lead to repeated commit-expiry cycles and delayed/non-executable rebalances under normal operation, degrading peg/risk response during stress.

### Exploitability
No privileged attacker required. This is a deterministic logic/liveness flaw triggered by normal slot timing and window position.

---

## 3) Regression / Prior-Class Sweep (Quick)

Rechecked prior closure classes; they remain closed at code level:

- **RV5-005 (dynamic fee bypass)**: mint uses `mint_fee_rate` (`lib.rs:815-817`), redeem uses `redeem_fee_rate` (`lib.rs:1119`), legacy sync kept (`lib.rs:2957-2960`).
- **RV5-006 (sybil hardening)**: min stake 1 SOL (`lib.rs:72`, `lib.rs:3348`), `registered_slot` tracked (`lib.rs:2331`, `lib.rs:3377`), tournament age/cap/entropy remain (`agent_loop.rs:32`, `:420-433`, `:460-487`).
- **RV5-007 (3-feed degradation)**: config enforces 4-feed coverage (`config.rs:40`, `:314-383`), devnet has USDS feed (`config.devnet.json:32`).
- **RV5-008 (isolation fail-open)**: strict path and fail-closed exit retained (`verify-isolation.sh:21`, `:126-130`); strict simulation still exits `1`.
- **RV5-013 (unilateral slash)**: keeper quorum + slash cap + cooldown retained (`lib.rs:458`, `:3385-3392`, field at `:2334`), wire schema includes `last_slashed_slot` (`keeper/src/wire.rs:93`).

No additional regressions from Blue v11 outside PV15-001 were found.

---

## 4) Validation Commands Executed

- `cd solana && cargo test -p microstable --lib --quiet` → passed (29 tests)
- `cd solana/keeper && cargo test --quiet` → passed
- `bash -n solana/keeper/scripts/verify-isolation.sh` → OK
- strict isolation simulation (mocked non-isolated PM2 jlist) → `exit_code=1` as expected
- deterministic slot/window model checks executed for deferred reveal path (shows slot-drift mismatch and end-window dead zone)

---

## Final Assessment

- **ZERO NEW FINDINGS:** **NO**
- Findings: **1 (HIGH)**
  - **PV15-001** — Deferred reveal remains liveness-fragile (slot-equality brittleness + batch-window dead zone)
