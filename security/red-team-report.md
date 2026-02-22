# Microstable Red Team Adversarial Security Audit

Date: 2026-02-22 (KST)  
Auditor stance: **adversarial / break-first**

## Scope
- Python simulator: `microstable/microstable.py`
- Solana program: `microstable/solana/programs/microstable/src/lib.rs`
- Agent interfaces:
  - `microstable/agents/keeper.py`
  - `microstable/agents/watchdog.py`
  - `microstable/agents/auditor.py`
  - `microstable/agents/consensus.py`
- Spec: `specs/microstable/spec.md`

## Deliverables produced
- PoC exploit code: `microstable/security/red_team_exploits.py`
- This report: `microstable/security/red-team-report.md`

---

## Executive Summary

I found multiple protocol-breaking issues, including one **CRITICAL** exploit path that enables near-total reserve drain in the Solana design model.

### Top findings
1. **CRITICAL — Unbacked mint + oracle manipulation drain path** (Solana logic)
   - `mint`/`redeem` update internal counters but do not perform token transfers.
   - Privileged oracle updater can set extreme prices.
   - Combined, attacker can mint enormous µSD and redeem nearly all vault balances.
2. **HIGH — Circuit-breaker griefing DoS** (Python simulator)
   - Keeping two assets barely depegged holds CB-2 active and keeps `mint_limit=0` indefinitely.
3. **HIGH — Circuit-breaker priority mismatch vs spec** (Python simulator)
   - Code priority `[4,2,3,1]` contradicts spec `[4,3,2,1]`.
   - CB-2 can recover while CB-3 still active, reopening mint path under oracle degradation.
4. **HIGH — Single key/agent liveness and safety fragility** (agents + Solana)
   - 1 compromised keeper key = full privileged control.
   - 1 of 3 consensus “no” vote can permanently block governance.

---

## CRITICAL Reproduction (exact)

### CR-01: Free mint + manipulated oracle => near-total redeem drain

**Files:**
- `solana/programs/microstable/src/lib.rs` lines ~126-268 (`mint`), ~271-360 (`redeem`), ~87-113 (`update_oracle`)

**Why it breaks:**
- `mint` increments `vault.total_deposits` by user input amount but never enforces actual token transfer.
- `update_oracle` permits any positive price from keeper.
- `redeem` distributes vault balances pro-rata to minted µSD.

**Reproduce:**
```bash
cd /Users/kjaylee/.openclaw/workspace/microstable
python3 security/red_team_exploits.py
```
Check finding: `B4/B8 free mint + oracle-manipulated redeem drain`

Observed PoC output:
- `free_mint_without_transfer`: `831666`
- `manipulated_mint_amount`: `831666666666`
- `redeem_share_of_total_supply`: `0.9999963927985831`

This models an attacker redeeming ~99.9996% of supply share after forged collateral accounting.

---

## Detailed findings by requested attack category

## A) Economic attacks (Python simulator)

### A1. Sandwich attack
- **Status:** **VULNERABLE**
- **Vector:** Forced CB-1 cap repair causes deterministic one-tick weight jump (`12.5%` on one asset), far above `DELTA_W_MAX=2%`.
- **Exploit code:** `exploit_a1_sandwich_cb1_weight_jump()`
- **Severity:** HIGH
- **Exploitability:** Easy
- **Fix:** In `CircuitBreaker.update` cap-repair branch, enforce bounded transition using same per-step delta as optimizer (project with `[w-DELTA_W_MAX, w+DELTA_W_MAX]` and staged multi-tick repair).

### A2. Oracle price manipulation
- **Status:** **VULNERABLE (model-level)**
- **Vector:** Simulator trusts supplied prices if integrated with external feed; no robust sanity bound / TWAP median.
- **Exploit code:** `red_team_exploits.py` (price/CB poisoning scenarios)
- **Severity:** HIGH
- **Exploitability:** Medium
- **Fix:** Add bounded oracle adapter for simulator inputs: max per-tick price move, multi-source median, and confidence gates before loss update.

### A3. Mint/redeem arbitrage
- **Status:** **VULNERABLE (implemented in Solana path, not Python runner)**
- **Vector:** Unbacked mint then redeem path (see CR-01).
- **Exploit code:** `exploit_b4_b8_state_only_mint_redeem()`
- **Severity:** CRITICAL
- **Exploitability:** Easy
- **Fix:** Require real token transfer CPI in mint/redeem before state mutation.

### A4. Weight concentration attack
- **Status:** **PARTIAL**
- **Vector:** Adversary-triggered CB actions can abruptly reallocate weights and increase concentration in non-target assets.
- **Exploit code:** A1 PoC demonstrates abrupt redistribution.
- **Severity:** MEDIUM
- **Exploitability:** Medium
- **Fix:** add concentration-aware repair objective; do not redistribute purely by box-simplex with abrupt cap snap.

### A5. Griefing via circuit breaker
- **Status:** **VULNERABLE**
- **Vector:** Sustained 2-asset slight depeg keeps CB-2 in HOLDING/RECOVERY loop and minting paused permanently.
- **Exploit code:** `exploit_a5_cb_griefing_dos()` (`paused_ticks=120/120`)
- **Severity:** HIGH
- **Exploitability:** Easy
- **Fix:** add anti-grief windowing, adaptive thresholds, and fail-open limited mint path under persistent but shallow stress.

### A6. Gradient poisoning
- **Status:** **VULNERABLE (control poisoning)**
- **Vector:** Crafted market/oracle inputs can dominate controller behavior; CB priority bug worsens this by reopening mint while oracle still degraded.
- **Exploit code:** `exploit_a6_priority_mismatch_recovery_bypass()`
- **Severity:** HIGH
- **Exploitability:** Easy
- **Fix:** align CB priority with spec and freeze mint whenever CB-3 active regardless of CB-2 status.

### A7. Death spiral induction
- **Status:** **VULNERABLE**
- **Vector:** Temporary stress permanently ratchets `cr_target` upward (`1.20 -> 1.35`), reducing system liveness and increasing contraction risk.
- **Exploit code:** `exploit_a7_death_spiral_cr_target_ratchet()`
- **Severity:** MEDIUM
- **Exploitability:** Easy
- **Fix:** store baseline CR target and implement time-based downward restoration after healthy streak.

---

## B) Smart contract attacks (Solana/Anchor)

### B1. Reentrancy
- **Status:** No direct reentrancy path observed (no external token CPI in current code).
- **Severity:** LOW
- **Exploitability:** Theoretical
- **Fix:** keep CEI pattern when token CPI is added.

### B2. Integer overflow/underflow
- **Status:** Mostly checked arithmetic; no immediate overflow exploit found.
- **Severity:** LOW
- **Exploitability:** Hard
- **Fix:** keep `checked_*`; add fuzz/property tests for boundary u64/u128 transitions.

### B3. PDA seed collision
- **Status:** No collision found for fixed seeds.
- **Severity:** LOW
- **Exploitability:** Hard
- **Fix:** retain explicit seed domains and bump checks.

### B4. Missing signer/authorization controls (privileged scope)
- **Status:** **VULNERABLE (operationally)**
- **Vector:** Single keeper signer controls oracle updates, rebalancing, breaker management.
- **Severity:** HIGH
- **Exploitability:** Easy (if key compromised)
- **Fix:** migrate to multisig/threshold authority and key rotation instruction.

### B5. Account confusion
- **Status:** No major account-type confusion exploit observed due Anchor seeds+types.
- **Severity:** LOW
- **Exploitability:** Hard

### B6. Rent exemption / close-drain exploit
- **Status:** No close path found.
- **Severity:** LOW
- **Exploitability:** Theoretical

### B7. Front-running
- **Status:** PARTIAL
- **Vector:** Rebalance and breaker transitions are predictable; A1-style jumps can be anticipated.
- **Severity:** MEDIUM
- **Exploitability:** Medium
- **Fix:** commit-reveal updates or delayed randomized activation windows.

### B8. Flash-loan attack
- **Status:** **VULNERABLE by stronger primitive**
- **Vector:** Flash loan unnecessary because unbacked mint path exists; if transfer checks are later added without anti-flash controls, risk remains.
- **Severity:** CRITICAL (current), HIGH (post-transfer without anti-flash)
- **Exploitability:** Easy
- **Fix:** enforce token transfers + same-tx anti-flash checks (min holding time / oracle snapshot guard).

### Additional B-policy weakness
- **Finding:** CB-1 cap reduction can be bypassed when current target weight already exceeds half-cap (`max(base/2, target_weight)`).
- **Exploit code:** `exploit_b1_cb1_cap_bypass_math()`
- **Severity:** MEDIUM
- **Exploitability:** Easy
- **Fix:** enforce actual reduction trajectory and then gradual migration to feasible weights.

---

## C) Agent security

### C1. Keeper manipulation
- **Status:** **VULNERABLE**
- **Vector:** Keeper single point of failure (oracle + rebalance + breaker control).
- **Severity:** HIGH
- **Exploitability:** Easy
- **Fix:** multisig keeper set + role separation (oracle updater distinct from rebalancer).

### C2. Watchdog bypass
- **Status:** **VULNERABLE**
- **Vector:** Watchdog outputs are unauthenticated JSON; not cryptographically bound or enforced on-chain.
- **Severity:** HIGH
- **Exploitability:** Easy
- **Fix:** signed watchdog attestations, quorum rules, and on-chain verification.

### C3. Consensus gaming (1 of 3 block)
- **Status:** **VULNERABLE**
- **Exploit code:** `exploit_c3_consensus_single_veto()`
- **Severity:** HIGH (liveness)
- **Exploitability:** Easy
- **Fix:** use 2-of-3 for non-critical updates, 3-of-3 only for destructive actions, add timeout fallback.

### C4. Agent key theft
- **Status:** **VULNERABLE**
- **Vector:** Stolen keeper key grants full privileged path.
- **Severity:** CRITICAL
- **Exploitability:** Easy
- **Fix:** HSM/remote signer, rotation, timelocked privileged actions, anomaly rate limits.

---

## D) Implementation bugs

### D1. NaN/Inf propagation
- **Status:** Mostly guarded in `Value`; failures trigger CB-4 path.
- **Severity:** LOW
- **Exploitability:** Medium
- **Fix:** add explicit finite checks at all external input boundaries.

### D2. Division by zero
- **Status:** guarded by EPS/requires.
- **Severity:** LOW
- **Exploitability:** Hard

### D3. Floating-point precision accumulation
- **Status:** Present by design (float simulation), can cause drift over long horizons.
- **Severity:** LOW
- **Exploitability:** Theoretical
- **Fix:** fixed-point arithmetic for critical policy paths.

### D4. CB state-machine bug (spec mismatch)
- **Status:** **VULNERABLE**
- **Vector:** Priority mismatch (`[4,2,3,1]`) enables unsafe recovery ordering.
- **Exploit code:** `exploit_a6_priority_mismatch_recovery_bypass()`
- **Severity:** HIGH
- **Exploitability:** Easy
- **Fix:** set priority to `[4,3,2,1]` and block lower-priority recovery while higher-priority CB remains active.

---

## Severity × Exploitability Matrix

| ID | Finding | Severity | Exploitability |
|---|---|---|---|
| F-01 | Free mint + oracle-manipulated redeem drain | CRITICAL | Easy |
| F-02 | Keeper key theft blast radius | CRITICAL | Easy |
| F-03 | CB griefing mint DoS | HIGH | Easy |
| F-04 | CB priority mismatch (CB2 recovers before CB3) | HIGH | Easy |
| F-05 | Sandwich window via forced CB1 rebalance jump | HIGH | Easy |
| F-06 | Consensus 1-of-3 veto liveness failure | HIGH | Easy |
| F-07 | Watchdog bypass / non-binding agent outputs | HIGH | Easy |
| F-08 | CR-target ratchet (death spiral pressure) | MEDIUM | Easy |
| F-09 | CB1 cap reduction bypass (`max(base/2, target)`) | MEDIUM | Easy |
| F-10 | Weight concentration via forced redistribution | MEDIUM | Medium |
| F-11 | Front-running of deterministic safety transitions | MEDIUM | Medium |
| F-12 | Numeric edge-case drift (float model) | LOW | Theoretical |

---

## Concrete code-change recommendations

### 1) Solana `mint`/`redeem`: enforce real token movement (CRITICAL)
- In `Mint` accounts, add user token account + vault token account + token program accounts.
- Before incrementing `vault.total_deposits`, execute SPL token transfer CPI from user ATA to vault ATA.
- In `Redeem`, transfer tokens out from vault ATA to user ATA; only then mutate accounting.

**Minimal shape (Anchor):**
```rust
#[account(mut)]
pub user_collateral_ata: InterfaceAccount<'info, TokenAccount>,
#[account(mut)]
pub vault_collateral_ata: InterfaceAccount<'info, TokenAccount>,
pub token_program: Interface<'info, TokenInterface>,
```
Then call `token_interface::transfer_checked(...)` in both directions.

### 2) Solana oracle hardening (CRITICAL/HIGH)
- Replace keeper-supplied raw `price` with verified oracle account reads (Pyth/Switchboard).
- Add bounds:
```rust
require!(price >= MIN_PRICE && price <= MAX_PRICE, ErrorCode::InvalidPrice);
require!(abs_diff(price, prev_price) <= MAX_PRICE_JUMP, ErrorCode::PriceJumpTooLarge);
```
- Store and check TWAP / medianized price.

### 3) Python breaker ordering bug (HIGH)
Change:
```python
PRIORITY = [4, 2, 3, 1]
```
To:
```python
PRIORITY = [4, 3, 2, 1]
```
Also enforce `if cb3 active: mint_limit = 0.0` when oracle is degraded.

### 4) Python forced cap-repair jump (HIGH)
In `CircuitBreaker.update` cap-repair branch, replace direct projection with bounded transition:
```python
lo2 = [max(0.0, state.weights[i] - DELTA_W_MAX) for i in range(n)]
hi2 = [min(state.w_caps[i], state.weights[i] + DELTA_W_MAX) for i in range(n)]
state.weights = AdamOptimizer.project_box_simplex(state.weights, lo2, hi2, target=1.0)
```
Iterate over ticks until feasible.

### 5) CR target ratchet (MEDIUM)
- Add `base_cr_target` to `ProtocolState`.
- On full breaker recovery streak, decay `cr_target` back toward baseline:
```python
state.cr_target = max(state.base_cr_target, state.cr_target - 0.005)
```

## Prioritized Fix Plan

### P0 (Immediate, before any real funds)
1. Add token transfer CPI enforcement in `mint`/`redeem` and bind vault mint accounts.
2. Replace single-keeper authority with multisig + key rotation.
3. Harden oracle ingestion: signed feeds, median/TWAP, bounded price moves.

### P1
4. Fix CB priority order to spec `[4,3,2,1]`.
5. Enforce bounded/staged weight repair during CB-driven cap changes.
6. Restore `cr_target` downward after sustained healthy period.

### P2
7. Add signed agent attestations and enforceable quorum on-chain.
8. Improve governance liveness (2-of-3 + timeout fallback).

---

## Notes
- PoCs were executed via `python3 security/red_team_exploits.py`.
- Major simulator-side findings are directly reproducible against `microstable.py` classes.
- Solana CRITICAL finding is demonstrated via arithmetic/state-path reproduction of current on-chain logic (no transfer validation path exists in target code).
