# Microstable Crimson Team Report (Red+Purple Hybrid)

**Campaign date:** 2026-02-22  
**Code base:** `microstable` @ commit `dd20a9a`  
**PoC suite:** `tests/crimson-team/`  
**Total attempts:** 27 (20 successful, 7 blocked)

## Executive Summary
Blue v3 remediations block prior Red/Purple findings, but several **new** vulnerabilities remain in the Python simulation layer (economic/logic gates, reward plumbing, watchdog consensus, numeric inputs). A subset chain into **critical** outcomes (monitoring capture, unbacked withdrawals, negative-price redemption drain). Solana on-chain guards around oracle updates and ATA bindings hold in static review.

**Success distribution:** 5 CRITICAL, 7 HIGH, 7 MEDIUM, 1 LOW.  
**Blocked/defenses held:** 7 (see “Defenses That Held”).

## Methodology
- Targeted code review of Python simulation (`microstable.py`, `open_agent_economy.py`, `adversarial_agents.py`).
- Static Solana checks (`solana/programs/microstable/src/lib.rs`).
- Immediate PoC scripting for each hypothesis; every successful exploit is reproduced in `tests/crimson-team/*.py`.
- Cross-layer validation between Python simulator and Solana program semantics.

## Successful Exploits (20)
> Each entry: **Severity**, **Category**, **Affected files/lines**, **Narrative**, **PoC**, **Impact**, **Chains**.

### CT-C01 — False-resolve slash + stake desync + quorum collapse
- **Severity:** CRITICAL | **Category:** A/E/F
- **Affected:** `open_agent_economy.py` `FederatedWatchdog.resolve` (~1038-1070), `StakingEconomics.slash` (~482-510), `AgentRegistry.slash` (~248-272), `FederatedWatchdog.consensus` (~1003-1011)
- **Narrative:**
  1) Honest monitors report a true event.  
  2) Attacker calls `resolve(alert, is_true=False)` (no auth/consensus guard).  
  3) All reporters are slashed; **registry** stake is reduced by *ratio* semantics while **staking** balances are reduced by *amount* semantics.  
  4) Honest monitors drop below min stake → deregistered.  
  5) Attacker registers as sole monitor; `consensus()` now passes with 1 vote; attacker collects bounties unopposed.
- **PoC:** `tests/crimson-team/compound_exploits.py::c01_watchdog_false_resolve_kill_chain`
- **Impact:** Monitoring capture + false positive suppression; attacker earns ongoing rewards, disables honest watchdogs.
- **Chains:** Uses CT-S06 (false resolve) + slash desync to collapse quorum.

### CT-C02 — Registration identity squatting + ACP impersonation
- **Severity:** HIGH | **Category:** B/F
- **Affected:** `open_agent_economy.py` `AgentRegistry.register` (~171-219), `ACPMessage._select_verification_key` (~616-634)
- **Narrative:**
  1) Attacker pre-registers victim ID with attacker public key.  
  2) Victim cannot register the same ID later.  
  3) ACP verifies attacker-signed messages as the victim.
- **PoC:** `tests/crimson-team/compound_exploits.py::c02_identity_squat_governance_impersonation`
- **Impact:** Full impersonation of target agent and governance operations.
- **Chains:** Enables CT-S02/CT-C03 claim-id griefing and governance manipulation.

### CT-C03 — Claim-ID squatting denial via unsigned micro-claim
- **Severity:** HIGH | **Category:** A/B
- **Affected:** `open_agent_economy.py` `StakingEconomics.claim_reward` (~530-558)
- **Narrative:**
  1) Attacker submits unsigned micro-claim with a **global** `claim_id`.  
  2) Victim later submits a signed claim with same `claim_id`.  
  3) Victim claim is rejected due to global ID collision.
- **PoC:** `tests/crimson-team/compound_exploits.py::c03_claim_squatting_chain`
- **Impact:** Reward denial and claim griefing; can block legitimate rewards at will.
- **Chains:** Depends on CT-S02 (global claim_id namespace).

### CT-C04 — Queue output overwrite + negative-price residual drain
- **Severity:** CRITICAL | **Category:** A/C/D
- **Affected:** `microstable.py` `RedemptionQueue.settle` (~1655-1710), `redeem_by_value` (~1559-1608)
- **Narrative:**
  1) Attacker queues duplicate `account` entries; output map overwrites prior allocations.  
  2) Submit negative oracle price on last vault leg (no validation).  
  3) `redeem_by_value` allocates residual to final asset, resulting in full vault drain to attacker.
- **PoC:** `tests/crimson-team/compound_exploits.py::c04_redemption_oracle_account_chain`
- **Impact:** Critical redemption drain and queue integrity failure.
- **Chains:** Combines output-overwrite and negative-price residual.

### CT-C07 — Sybil boundary jitter bypasses 2dp fingerprint dampener
- **Severity:** MEDIUM | **Category:** A/B/F
- **Affected:** `open_agent_economy.py` `OptimizationTournament.evaluate` bucket rounding (~928-942)
- **Narrative:**
  1) Attacker submits many proposals with weights jittered around 2dp rounding boundaries.  
  2) Fingerprint buckets fragment → sybils appear unique.  
  3) Sybil group captures disproportionate participant rewards.
- **PoC:** `tests/crimson-team/compound_exploits.py::c07_sybil_rounding_boundary_capture`
- **Impact:** Rewards skewed toward sybil cluster under high participation.
- **Chains:** Related to CT-S04 (same root cause).

### CT-C08 — Python accepts negative staleness while Rust enforces monotonic slot
- **Severity:** MEDIUM | **Category:** C/E
- **Affected:** `microstable.py` `validated_oracle_price` (~1512-1520), `secure_mint_amount` (~1530-1554)
- **Narrative:**
  1) Provide `stale_seconds=-999` in simulator.  
  2) Python accepts and mints; Solana enforces `observed_slot <= slot` and staleness bounds.  
  3) Cross-layer divergence enables inaccurate sim results / policy assumptions.
- **PoC:** `tests/crimson-team/compound_exploits.py::c08_negative_stale_cross_layer_divergence`
- **Impact:** Simulator can mis-evaluate risk and allow impossible paths.
- **Chains:** Related to CT-N07 (negative staleness acceptance).

### CT-S01 — Unsigned micro-claims consume full epoch budget
- **Severity:** MEDIUM | **Category:** B/F
- **Affected:** `open_agent_economy.py` `StakingEconomics.claim_reward` (~530-558)
- **Narrative:**
  1) Attacker submits many unsigned micro-claims (`amount <= legacy_unsigned_claim_limit`).  
  2) Aggregates to full epoch cap without proof.  
  3) Exhausts reward pool for legitimate claimants.
- **PoC:** `tests/crimson-team/semantic_attacks.py::s01_unsigned_micro_claim_budget_exhaustion`
- **Impact:** Reward starvation and griefing at scale.
- **Chains:** Enables CT-C03 claim-id squatting.

### CT-S02 — Global claim_id namespace enables griefing
- **Severity:** HIGH | **Category:** B
- **Affected:** `open_agent_economy.py` `StakingEconomics.claim_reward` (~530-558)
- **Narrative:**
  1) Attacker uses shared `claim_id` before victim.  
  2) Victim’s signed claim is rejected (global claim_id collision).
- **PoC:** `tests/crimson-team/semantic_attacks.py::s02_claim_id_scope_griefing`
- **Impact:** Denial of legitimate rewards.
- **Chains:** Used by CT-C03.

### CT-S03 — -inf loss_estimate semantic injection wins tournament
- **Severity:** HIGH | **Category:** B
- **Affected:** `open_agent_economy.py` `OptimizationTournament._score` (~861-879), `evaluate` (~896-959)
- **Narrative:**
  1) Attacker submits proposal with `loss_estimate = -inf`.  
  2) `_score` uses `-proposal.loss_estimate`, so `-(-inf)` yields `+inf`.  
  3) Attacker wins and receives rewards; parameters pass validation.
- **PoC:** `tests/crimson-team/semantic_attacks.py::s03_tournament_loss_estimate_semantic_injection`
- **Impact:** Governance manipulation: attacker can force selection irrespective of real performance.
- **Chains:** Can combine with CT-S04 to farm rewards across epochs.

### CT-S04 — Fingerprint boundary evasion in participant pool
- **Severity:** MEDIUM | **Category:** A/B/F
- **Affected:** `open_agent_economy.py` `OptimizationTournament.evaluate` bucket rounding (~928-942)
- **Narrative:**
  1) Sybils submit weights jittered at rounding boundaries.  
  2) 2dp bucketing splits sybil cluster into many buckets.  
  3) Sybils capture excess participant rewards.
- **PoC:** `tests/crimson-team/semantic_attacks.py::s04_sybil_fingerprint_boundary_evasion`
- **Impact:** Reward distribution skew.
- **Chains:** Same root cause as CT-C07.

### CT-S05 — Collusion alert ordering bypasses semantic idempotency
- **Severity:** MEDIUM | **Category:** B/F
- **Affected:** `adversarial_agents.py` `ResponseEngine._idempotency_key` (~763-772)
- **Narrative:**
  1) Submit two collusion alerts with same agents in different order.  
  2) Idempotency key uses the **first** agent only, so order changes key.  
  3) Response executes twice (double quarantine / repeated actions).
- **PoC:** `tests/crimson-team/semantic_attacks.py::s05_response_engine_order_based_idempotency_bypass`
- **Impact:** Response engine can be spammed / over-triggered.
- **Chains:** Can be combined with CT-C01 to over-slash monitors.

### CT-S06 — False resolution can slash reporters without truth quorum
- **Severity:** HIGH | **Category:** B/E
- **Affected:** `open_agent_economy.py` `FederatedWatchdog.resolve` (~1038-1070)
- **Narrative:**
  1) Attackers call `resolve(alert, is_true=False)` with no authorization/consensus guard.  
  2) All reporters are slashed despite no truth quorum.  
  3) Creates a denial-of-service against monitors.
- **PoC:** `tests/crimson-team/semantic_attacks.py::s06_watchdog_false_resolution_without_consensus_guard`
- **Impact:** Monitor slashing abuse; reputation suppression.
- **Chains:** Core primitive for CT-C01.

### CT-N01 — NaN deposit enables unbacked large withdrawal
- **Severity:** CRITICAL | **Category:** D
- **Affected:** `open_agent_economy.py` `StakingEconomics.deposit` (~435-444), `request_withdrawal` (~455-464), `withdraw` (~467-476)
- **Narrative:**
  1) Deposit `NaN` as stake (no finite validation).  
  2) `available()` becomes NaN, so withdrawal requests always pass.  
  3) Withdraw arbitrary amount despite no real balance.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n01_nan_deposit_unbounded_withdraw`
- **Impact:** Unbacked token withdrawal from staking pool.
- **Chains:** Can be paired with CT-S01 to drain rewards and exit funds.

### CT-N02 — NaN deposit + inf withdrawal amount returns inf
- **Severity:** CRITICAL | **Category:** D
- **Affected:** Same as CT-N01
- **Narrative:** Same as CT-N01 but with `amount=inf`, returning infinite withdrawal.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n02_nan_deposit_infinite_withdraw`
- **Impact:** Catastrophic accounting corruption.
- **Chains:** Variant of CT-N01.

### CT-N03 — NaN epoch input crashes claim_reward
- **Severity:** MEDIUM | **Category:** D
- **Affected:** `open_agent_economy.py` `StakingEconomics._reward_claim_payload` (~517-526)
- **Narrative:**
  1) Submit `epoch=NaN` for `claim_reward`.  
  2) `int(epoch)` raises `ValueError`.  
  3) Crash propagates: denial-of-service.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n03_claim_reward_nan_epoch_crash`
- **Impact:** DoS on reward pipeline.
- **Chains:** Can be chained with CT-S01 to exhaust reward pool + DoS remainder.

### CT-N04 — Keeper path accepts NaN mint_fee
- **Severity:** HIGH | **Category:** D
- **Affected:** `microstable.py` `Keeper.submit_update_proposal` (~1260-1306)
- **Narrative:**
  1) Keeper proposal sets `mint_fee=NaN`.  
  2) `abs(fee - state.mint_fee)` returns NaN, bypassing delta checks.  
  3) Proposal applied; protocol state poisoned.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n04_keeper_nan_fee_poison`
- **Impact:** Fee becomes NaN → downstream math failures, pricing anomalies.
- **Chains:** Can be paired with CT-N05 (weights) for full state poisoning.

### CT-N05 — Keeper path accepts NaN in weights
- **Severity:** HIGH | **Category:** D
- **Affected:** `microstable.py` `Keeper.submit_update_proposal` (~1260-1306)
- **Narrative:**
  1) Submit weights containing NaN.  
  2) Sum/cap/delta comparisons do not reject NaN.  
  3) Protocol weights become NaN → nav/peg becomes NaN.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n05_keeper_nan_weight_poison`
- **Impact:** State corruption and invariant breakage.
- **Chains:** Couples with CT-N04 to poison fee + weights.

### CT-N06 — Negative oracle price drives residual into full-asset drain
- **Severity:** CRITICAL | **Category:** C/D
- **Affected:** `microstable.py` `redeem_by_value` (~1559-1608)
- **Narrative:**
  1) Provide negative oracle price for last asset.  
  2) Asset value becomes negative → total_value shrinks; residual allocated to final asset.  
  3) Attacker drains full vault units for that asset.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n06_negative_price_residual_drain`
- **Impact:** Redemption drain under malformed oracle inputs.
- **Chains:** Used inside CT-C04 chain.

### CT-N07 — Negative stale_seconds accepted by secure_mint_amount
- **Severity:** MEDIUM | **Category:** C/D/E
- **Affected:** `microstable.py` `validated_oracle_price` (~1512-1520)
- **Narrative:**
  1) Pass `stale_seconds=-100`.  
  2) Function only checks `stale_seconds > max` so negative values pass.  
  3) Mint proceeds on impossible staleness input.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n07_negative_staleness_accepted`
- **Impact:** Simulation accepts invalid oracle data; policy divergence.
- **Chains:** Same root cause as CT-C08.

### CT-N10 — Floating-point epsilon drift exceeds nominal epoch cap
- **Severity:** LOW | **Category:** D
- **Affected:** `open_agent_economy.py` `StakingEconomics.claim_reward` (~530-558)
- **Narrative:**
  1) Submit 10,000 claims of `0.1` without proof.  
  2) Floating-point accumulation results in `claimed_by_epoch` slightly **above** cap.  
  3) Minor budget overshoot beyond nominal cap.
- **PoC:** `tests/crimson-team/numeric_edge_cases.py::n10_reward_cap_epsilon_drift`
- **Impact:** Slight cap overshoot; low but measurable drift.

## Defenses That Held (Blocked Attempts)
These attempts are **blocked** by existing Blue v3 defenses (confirmed by PoCs):

- **CT-C05:** `update_oracle_pyth` requires keeper quorum and signer accounts (Solana).  
- **CT-C06:** mint account substitution blocked by canonical ATA constraints and runtime key checks.  
- **CT-S07:** `set_public_key` actor mismatch is rejected.  
- **CT-S08:** ACP nonce replay is rejected.  
- **CT-S09:** inactive monitor cannot report.  
- **CT-N08:** stale oracle rejected by `validated_oracle_price`.  
- **CT-N09:** toxic collateral mint blocked by risk threshold.

## Cross-Layer Divergences
- **Negative staleness handling:** Python accepts negative `stale_seconds` (CT-N07/CT-C08), while Solana enforces monotonic slot and staleness bounds in oracle updates. This divergence can invalidate simulator-based safety claims.

## Recommendations (High Priority)
1. **Watchdog resolve authorization:** require keeper quorum or monitor quorum for **both** `is_true` and `is_true=False` resolution paths.
2. **Normalize slash semantics:** align `StakingEconomics.slash` and `AgentRegistry.slash` (ratio vs amount) to avoid stake/balance desync.
3. **Strict numeric input validation:** reject non-finite values in `deposit`, `request_withdrawal`, `claim_reward` (epoch), `submit_update_proposal` (weights/fee).
4. **Claim ID scoping:** include `agent_id` in claim uniqueness and require proof for any claim with shared `claim_id` namespace.
5. **Oracle price validation in redemption:** require positive, finite `oracle_prices` in `redeem_by_value`.
6. **Sybil dampener hardening:** replace 2dp rounding with stronger clustering (cosine sim + owner linkage).

## PoC Suite
- `tests/crimson-team/compound_exploits.py`
- `tests/crimson-team/semantic_attacks.py`
- `tests/crimson-team/numeric_edge_cases.py`
- Aggregated results: `tests/crimson-team/results.json`

## Reproduction
```bash
PYTHONPATH=. python3 tests/crimson-team/compound_exploits.py
PYTHONPATH=. python3 tests/crimson-team/semantic_attacks.py
PYTHONPATH=. python3 tests/crimson-team/numeric_edge_cases.py
```

## Notes on Git
Per workspace rules, git operations are restricted outside this repo. Coordinate commit/push from the main agent if needed.
