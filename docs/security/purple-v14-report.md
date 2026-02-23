# Microstable Security Audit — Purple Team v14 (Post-Red-v5 Remediation Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v14
- Scope:
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper: all `solana/keeper/src/*.rs`
  - Ops script: `solana/keeper/scripts/verify-isolation.sh`
  - Prior reports reviewed:
    - `docs/security/red-v5-report.md`
    - `docs/security/purple-v13-report.md`
  - Remediation commit verified: `3c54de2`

---

## Executive Result

Blue v10 successfully closes the five Red v5 target exploits (**RV5-005/006/007/008/013**) at code level.

However, this round identified **1 HIGH-severity remaining liveness vulnerability** outside those exact five items:

- **PV14-001 (HIGH)** — Rebalance commit/reveal deferred path is logically unreachable (default config path), causing rebalance execution to stall until commits expire.

So this round is **NOT zero-finding**.

---

## Verification of Blue v10 Red-v5 Patches

## 1) RV5-005 — Dynamic fee bypass (mint/redeem fee params)
**Status: CLOSED**

Evidence:
- Mint path charges `mint_fee_rate` (not legacy `fee_rate`):
  - `solana/programs/microstable/src/lib.rs:815-817`
- Redeem path deducts `redeem_fee_rate` before payout:
  - `solana/programs/microstable/src/lib.rs:1119`
- Legacy alias preserved/synchronized for compatibility:
  - `solana/programs/microstable/src/lib.rs:2957-2960`

## 2) RV5-006 — Sybil capture via low min stake + predictable sampling
**Status: CLOSED (for stated patch targets)**

Evidence:
- Min registration stake raised to 1 SOL:
  - `solana/programs/microstable/src/lib.rs:72`, enforced at `:3348`
- `registered_slot` added/tracked:
  - struct field `:2331`, set at `:3377`
- Tournament min registration age gate:
  - `solana/keeper/src/agent_loop.rs:32`, `:412-421`
- Entropy mixing (slot + blockhash + protocol nonce):
  - `solana/keeper/src/agent_loop.rs:460-487`
- Tournament participant cap:
  - `solana/keeper/src/agent_loop.rs:33`, `:426-433`

## 3) RV5-007 — 3-feed config causing 4-vault oracle degradation
**Status: CLOSED**

Evidence:
- Config enforces exactly 4 feeds and full index coverage:
  - `solana/keeper/src/config.rs:40`, `:314-331`, `:363-383`
- Devnet config includes USDS feed (`collateral_index=3`):
  - `solana/keeper/config.devnet.json:32-33`
- Oracle cycle logs unconfigured vaults explicitly:
  - `solana/keeper/src/oracle.rs:168-179`

## 4) RV5-008 — Isolation verification fail-open
**Status: CLOSED**

Evidence:
- `--strict` flag implemented:
  - `solana/keeper/scripts/verify-isolation.sh:7`, `:22`
- Warnings and non-isolated state tracked:
  - `:37-42`, `:93-94`
- Strict mode exits non-zero on warnings/not-isolated:
  - `:125-130`
- Runtime check reproduced:
  - strict run with mocked non-isolated PM2 domain returned `exit_code=1`.

## 5) RV5-013 — Unilateral slashing by trusted initializer
**Status: CLOSED**

Evidence:
- `slash_agent` now requires keeper quorum:
  - `solana/programs/microstable/src/lib.rs:458` (+ `require_keeper_quorum` at `:2923`)
- 50% slash cap implemented:
  - `:3385-3388`
- 100-slot slash cooldown implemented:
  - constant `:74`, check `:475`, helper `:3390-3392`
- `last_slashed_slot` added and updated:
  - field `:2334`, assignment `:501`
- Keeper wire schema updated accordingly:
  - `solana/keeper/src/wire.rs:90`, `:93`

---

## New / Remaining Finding

### PV14-001 — Deferred rebalance reveal path is unreachable (default keeper mode)
- Severity: **HIGH**
- Category: **DoS / Liveness**
- Affected:
  - `solana/programs/microstable/src/lib.rs`
  - `solana/keeper/src/rebalance.rs`
  - `solana/keeper/config.devnet.json`

#### Description
Keeper stores local reveal metadata with `batch_slot = current_slot + commit_reveal_delay_slots`, but the deferred reveal branch later requires `batch_slot == protocol.pending_rebalance_slot`. On-chain, `pending_rebalance_slot` is set to the **commit slot** (`current_slot`), not `batch_slot`.

With `commit_reveal_delay_slots > 0`, equality cannot hold. Therefore deferred reveal is never selected.

#### Code Evidence
- On-chain commit stores commit slot:
  - `protocol.pending_rebalance_slot = slot` at `solana/programs/microstable/src/lib.rs:1265`
- Keeper creates deferred `batch_slot = current_slot + delay`:
  - `solana/keeper/src/rebalance.rs:326`, `:862-863`
- Deferred reveal requires impossible equality:
  - `pending.batch_slot == protocol.pending_rebalance_slot` at `solana/keeper/src/rebalance.rs:194`
- Config enforces non-zero delay and default non-immediate reveal mode:
  - `commit_reveal_delay_slots must be > 0` at `solana/keeper/src/config.rs:454-455`
  - default `execute_rebalance_immediately: false` at `solana/keeper/src/config.rs:197`
  - devnet config also sets `execute_rebalance_immediately: false` + delay 5 at `solana/keeper/config.devnet.json:48,51`

#### Impact
- In default mode, keeper can submit commits but cannot execute deferred reveals.
- Active pending commit blocks new commits until expiry, reducing rebalance to commit-expire loops.
- Rebalance control can become effectively non-functional, degrading peg/risk response under stress.

#### Exploitability
- No privileged attacker capability required; this is a deterministic logic flaw in normal operations.
- Any market condition requiring rebalance can be amplified by inability to execute deferred reveals.

---

## Regression Sweep (v8–v13 + Red v5)

- Red v5 target items (005/006/007/008/013): **verified closed** as above.
- Prior closure classes from v13 were rechecked in current code and remain generally intact (keeper quorum gating, feed validation, PM2 strict option path, ABI updates).
- **Exception found this round:** PV14-001 liveness defect in rebalance deferred reveal flow.

---

## Validation Commands Executed

- `cd solana && cargo test -p microstable --lib --quiet` → passed (29 tests)
- `cd solana/keeper && cargo test --quiet` → passed
- `bash -n solana/keeper/scripts/verify-isolation.sh` → syntax OK
- Strict isolation simulation with mocked PM2 non-isolated state:
  - `verify-isolation.sh --strict` returned `exit_code=1` (expected fail-closed behavior)

---

## Final Assessment

- **ZERO NEW FINDINGS:** **NO**
- Findings: **1 (HIGH)**
  - PV14-001 (deferred rebalance reveal unreachable/default liveness failure)
