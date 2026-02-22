# Purple Team v3 Report — Post Blue v3 Vulnerability Hunt

Date: 2026-02-22 (KST)
Repo: `/Users/kjaylee/.openclaw/workspace/microstable/`
Baseline patch commit reviewed: `dd20a9a`

## Executive Summary

- **New findings:** **23**
- **Severity mix:** **CRITICAL 2 / HIGH 13 / MEDIUM 8**
- Focus areas hit:
  - Blue v3 patch gaps and boundary bypasses
  - New attack surfaces introduced by Blue v3 controls
  - Python simulation ↔ Solana on-chain divergence
  - Economic griefing/drain vectors under tightened constraints

---

## Findings

### PTV3-001 — Reward proof forgery via hardcoded signing key
- **Severity:** CRITICAL
- **File+Lines:** `open_agent_economy.py:417-424, 526-530, 548-549`
- **Attack scenario:**
  1. Attacker reads default signing key (`OAE_REWARD_AUTHORITY_V1`) from source.
  2. Attacker locally computes a “valid” proof for any active agent.
  3. `claim_reward()` accepts forged proof and mints rewards.
- **PoC evidence:**
  - Observed: `claim_ok True balance 999.0`
- **Impact:** Arbitrary reward minting despite Blue v3 proof requirement.

### PTV3-002 — Unsigned micro-claims still accepted by default
- **Severity:** HIGH
- **File+Lines:** `open_agent_economy.py:418, 551-553`
- **Attack scenario:**
  1. Use `proof=None` with `amount <= legacy_unsigned_claim_limit` (default 1.0).
  2. Repeat with unique claim IDs.
  3. Drain epoch budget without cryptographic proof.
- **PoC evidence:**
  - Observed: `all_ok True claimed_epoch 5.0 balance 5.0` for 5 unsigned claims.
- **Impact:** Budget siphoning and anti-forgery policy bypass at low denomination.

### PTV3-003 — Global epoch cap allows first-claimer griefing
- **Severity:** MEDIUM
- **File+Lines:** `open_agent_economy.py:540-546, 556`
- **Attack scenario:**
  1. One attacker consumes full epoch cap early.
  2. Honest agents’ valid claims fail afterward.
- **PoC evidence:**
  - Observed: `attacker drains True`, `honest blocked False`.
- **Impact:** Reward denial/griefing without protocol compromise.

### PTV3-004 — NaN stake registration bypasses numeric hardening
- **Severity:** HIGH
- **File+Lines:** `open_agent_economy.py:171-203`
- **Attack scenario:**
  1. Register with `stake=NaN`.
  2. Comparison `stake < min_stake` does not reject NaN.
  3. NaN enters agent state and downstream economics.
- **PoC evidence:**
  - Observed: `ok True stake_is_nan True`.
- **Impact:** State poisoning and undefined behavior in slashing/reward logic.

### PTV3-005 — NaN withdrawal request poisons lock/pending accounting
- **Severity:** HIGH
- **File+Lines:** `open_agent_economy.py:455-465, 474-479`
- **Attack scenario:**
  1. Submit `request_withdrawal(..., amount=NaN)`.
  2. Request passes guards; lock and pending become NaN.
  3. Subsequent accounting behavior becomes non-deterministic.
- **PoC evidence:**
  - Observed: `request_ok True locked nan pending (nan, 5)`.
- **Impact:** Withdrawal/accounting corruption and potential localized DoS.

### PTV3-006 — Score manipulation persists via unbounded `loss_estimate`
- **Severity:** HIGH
- **File+Lines:** `open_agent_economy.py:861-877, 896-906`
- **Attack scenario:**
  1. Submit valid params but set `loss_estimate` extremely negative.
  2. `_score` uses `loss_score = -proposal.loss_estimate` with no bounds.
  3. Attacker dominates ranking and becomes winner.
- **PoC evidence:**
  - Observed: `winner evil` with `loss_estimate=-1e9`.
- **Impact:** Parameter-selection capture despite Blue v3 risk/return clamping.

### PTV3-007 — ACP expiry not enforced when `now_epoch` is omitted
- **Severity:** HIGH
- **File+Lines:** `open_agent_economy.py:678-683`
- **Attack scenario:**
  1. Craft message with old/expired epoch.
  2. Call `ACPMessage.verify(..., now_epoch=None)` (default).
  3. Expiry/future checks never execute.
- **PoC evidence:**
  - Observed: `verify_without_now_epoch True` on expired payload.
- **Impact:** Long-lived replay window in common caller configuration.

### PTV3-008 — Nonce replay set is unbounded (memory DoS)
- **Severity:** MEDIUM
- **File+Lines:** `open_agent_economy.py:583, 686-690`
- **Attack scenario:**
  1. Send many valid messages with unique nonces.
  2. `_seen_nonces` grows forever; no pruning/TTL.
- **PoC evidence:**
  - Observed: `seen_nonce_size 5000` after 5000 verifies.
- **Impact:** Memory pressure and process-level DoS risk.

### PTV3-009 — Public-key rotation bypass via spoofed `actor_id` + derivable rotate token
- **Severity:** CRITICAL
- **File+Lines:** `open_agent_economy.py:296-333`
- **Attack scenario:**
  1. Attacker supplies `actor_id="victim"` (string check only).
  2. Rotation token is HMAC under current “public_key” string.
  3. If key is observable, attacker derives token and rotates victim key.
- **PoC evidence:**
  - Observed: `rotate_by_spoofed_actor True`, `new_key EVILKEY`.
- **Impact:** Post-patch identity takeover / ACP impersonation.

### PTV3-010 — False alert resolution slashes reporters without consensus gate
- **Severity:** HIGH
- **File+Lines:** `open_agent_economy.py:1045-1070`
- **Attack scenario:**
  1. Single report exists (below consensus).
  2. Call `resolve(..., is_true=False)`.
  3. Reporter is slashed even with no quorum.
- **PoC evidence:**
  - Observed: `consensus_before False`, `balance_before 5.0`, `balance_after 4.75`.
- **Impact:** Griefing slashes against honest monitors.

### PTV3-011 — Python keeper path has no keeper auth/quorum (cross-layer divergence)
- **Severity:** HIGH
- **File+Lines:** `microstable.py:1260-1304` vs `solana/programs/microstable/src/lib.rs:996-1001`
- **Attack scenario:**
  1. Any caller invokes `submit_update_proposal()` with shape-valid payload.
  2. Proposal applies without signer/quorum checks in simulation.
  3. Security posture diverges from on-chain 2-of-3 enforcement.
- **PoC evidence:**
  - Observed: `result {'status': 'APPLIED', ...}` for non-keeper crafted payload.
- **Impact:** Off-chain control-plane assumptions can be dangerously optimistic.

### PTV3-012 — Commit threshold equality bypass (`>` instead of `>=`)
- **Severity:** MEDIUM
- **File+Lines:** `microstable.py:1292`
- **Attack scenario:**
  1. Craft proposal at exact cumulative threshold (~0.05 L1).
  2. Commit proof path is skipped because check is strict `>`.
- **PoC evidence:**
  - Observed: `delta 0.049999999999999906` and proposal `APPLIED` without commit proof.
- **Impact:** Boundary bypass on Blue v3 split-rebalance control.

### PTV3-013 — Oracle freshness accepts negative staleness values
- **Severity:** MEDIUM
- **File+Lines:** `microstable.py:1512-1520`
- **Attack scenario:**
  1. Provide `stale_seconds < 0` (future timestamp semantics).
  2. Freshness check only rejects `stale_seconds > max`.
- **PoC evidence:**
  - Observed: `validated_oracle_price(..., -999) -> 1.0`.
- **Impact:** Time/freshness gate bypass under malformed clock inputs.

### PTV3-014 — `secure_mint_amount` accepts single-source oracle input
- **Severity:** HIGH
- **File+Lines:** `microstable.py:1512-1543`
- **Attack scenario:**
  1. Submit one manipulated oracle sample.
  2. Mint amount is computed directly from that sample median.
- **PoC evidence:**
  - Observed: `normal 831666`, `single_source_manip 1247500`.
- **Impact:** Over-minting under thin/manipulated oracle input.

### PTV3-015 — `redeem_by_value` crashes on NaN oracle prices
- **Severity:** MEDIUM
- **File+Lines:** `microstable.py:1581-1602`
- **Attack scenario:**
  1. Inject NaN into `oracle_prices`.
  2. Path reaches integer conversion from NaN-derived residual.
- **PoC evidence:**
  - Observed exception: `cannot convert float NaN to integer`.
- **Impact:** Redemption-path DoS with malformed numeric input.

### PTV3-016 — RedemptionQueue accepts zero/negative burn entries
- **Severity:** MEDIUM
- **File+Lines:** `microstable.py:1630-1646, 1661-1667`
- **Attack scenario:**
  1. Enqueue many `burn_amount<=0` requests.
  2. Requests are accepted and consume smoothing window capacity.
- **PoC evidence:**
  - Observed: `enq0 True enqNeg True pending 2`.
- **Impact:** Queue spam / settlement latency griefing.

### PTV3-017 — Duplicate account keys overwrite prior payouts
- **Severity:** HIGH
- **File+Lines:** `microstable.py:1683-1694`
- **Attack scenario:**
  1. Same account appears multiple times in a batch.
  2. `out[req.account] = allocs` overwrites previous allocation.
- **PoC evidence:**
  - Observed:
    - duplicate batch: `{'x': [50]}` total 50
    - split accounts: `{'a': [100], 'b': [50]}` total 150
- **Impact:** Silent payout loss/misattribution in settlement output.

### PTV3-018 — Insurance cooldown/cap bypass via caller-controlled `tick`
- **Severity:** HIGH
- **File+Lines:** `microstable.py:1798-1816`
- **Attack scenario:**
  1. Submit claims with artificially jumped ticks (e.g., 0,20,40).
  2. Cooldown and epoch windows are bypassed by input spoofing.
- **PoC evidence:**
  - Observed three sequential approvals at ticks `0`, `20`, `40`.
- **Impact:** Accelerated fund drain and pacing-policy bypass.

### PTV3-019 — Rejected insurance claims can still trigger auto-refill
- **Severity:** HIGH
- **File+Lines:** `microstable.py:1792-1805`
- **Attack scenario:**
  1. Treasury is below refill trigger.
  2. Send claim below `min_claim`.
  3. `_auto_refill()` runs before validation and increases treasury.
- **PoC evidence:**
  - Observed: `treasury_before 200000.0` → rejected claim → `treasury_after 250000.0`.
- **Impact:** Unbacked treasury inflation path from invalid requests.

### PTV3-020 — Global insurance cooldown enables cheap lockout griefing
- **Severity:** MEDIUM
- **File+Lines:** `microstable.py:1807-1809`
- **Attack scenario:**
  1. Attacker submits minimal valid claim first.
  2. Victim claims immediately after are denied by global cooldown.
- **PoC evidence:**
  - Observed: `attacker_claim approved`, `victim_blocked {'reason': 'global_cooldown'}`.
- **Impact:** Denial of coverage to legitimate claimants.

### PTV3-021 — On-chain `resume_from_shutdown` has no health gating
- **Severity:** HIGH
- **File+Lines:** `solana/programs/microstable/src/lib.rs:1305-1319`
- **Attack scenario:**
  1. Keeper quorum calls resume immediately after shutdown.
  2. Function unconditionally clears shutdown and re-enables mint lane.
- **PoC evidence:**
  - Static review: no CR/oracle/depeg checks in function body.
- **Impact:** Unsafe restart window under unresolved market/oracle stress.

### PTV3-022 — On-chain keeper rotation is immediate and timelock-free
- **Severity:** HIGH
- **File+Lines:** `solana/programs/microstable/src/lib.rs:1322-1334`
- **Attack scenario:**
  1. Compromised current quorum rotates to attacker-controlled or dead keys.
  2. Rotation applies instantly, no delay/ratification barrier.
- **PoC evidence:**
  - Static review: direct assignment `protocol_state.keeper_set = new_keeper_set`.
- **Impact:** Fast governance capture / potential permanent control loss.

### PTV3-023 — Oracle trust anchor pinned to single deployer key
- **Severity:** MEDIUM
- **File+Lines:** `solana/programs/microstable/src/lib.rs:24, 39, 2128-2132`
- **Attack scenario:**
  1. Pyth write authority rotates legitimately or deployer key is unavailable.
  2. Updates fail because equality check is hard-pinned to one key.
- **PoC evidence:**
  - Static review: `const PYTH_TRUSTED_WRITE_AUTHORITY: Pubkey = TRUSTED_INITIALIZER` and strict `require_keys_eq!`.
- **Impact:** Oracle update liveness fragility and concentrated trust risk.

---

## Notes on Reproducibility

Primary PoCs were executed directly against repository Python modules (`open_agent_economy.py`, `microstable.py`) with deterministic inputs. Solana findings PTV3-021~023 are static code-path findings with exact line references.
