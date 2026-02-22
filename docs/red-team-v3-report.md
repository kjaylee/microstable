# Microstable Red Team v3 Report

## Executive Summary
- Total exploit attempts: **36**
- Successful exploits: **16**
- Blocked by defenses: **20**
- Empirical exploit success rate: **44.44%**

Primary confirmed issues are concentrated in Python-layer economic/security controls (NaN handling, authorization gaps, legacy compatibility defaults, and weak commit-proof semantics). Solana account-substitution and migration guard checks held in static review; however `update_oracle_pyth` remains permissionless by design and should be threat-modeled as an MEV/grief surface.

## PoC Artifacts
- Harness: `tests/red-team-v3/exploit_campaign.py`
- Raw results JSON: `tests/red-team-v3/results.json`
- Raw markdown table: `tests/red-team-v3/results.md`
- Reproduce: `PYTHONPATH=. python3 tests/red-team-v3/exploit_campaign.py`

## Successful Exploits (Confirmed)
### A01 — Reward proof forgery to unregistered ghost agent
- Severity: **CRITICAL**
- Target: `OAE staking`
- Bypasses previous fix: **PT-001**
- Estimated financial impact: **Up to reward_epoch_cap per epoch; ~100% incentive pool drain**
- Attack narrative: Forge signed reward claims for non-registered identity and mint rewards to ghost balance.
- PoC evidence: `claim_ok=True, ghost_balance=900.0`

### A02 — NaN reward-claim poisoning bypasses epoch cap
- Severity: **CRITICAL**
- Target: `OAE staking`
- Bypasses previous fix: **PT-001**
- Estimated financial impact: **Unlimited rewards after poisoning; theoretical full treasury drain**
- Attack narrative: Inject NaN claim to poison epoch accounting then claim beyond cap.
- PoC evidence: `nan_claim=True, post_nan_large_claim=True, epoch_used=nan`

### A03 — Negative slash parameter increases attacker balance
- Severity: **CRITICAL**
- Target: `OAE staking`
- Bypasses previous fix: **No direct PT mapping**
- Estimated financial impact: **Direct 2x stake inflation per call**
- Attack narrative: Call slash with negative ratio to turn penalty into mint.
- PoC evidence: `before=100.0, slash_amt=-100.0, after=200.0`

### A08 — NaN proposal passes winner invariants
- Severity: **HIGH**
- Target: `Tournament evaluate`
- Bypasses previous fix: **PT-006**
- Estimated financial impact: **Corrupt protocol params; possible cascading failures**
- Attack narrative: Use NaN weights/fee to pass validation and win.
- PoC evidence: `winner=evil, mint_fee=nan, weights=[nan, 0.3, 0.3, 0.4]`

### A10 — Multi-identity sybil captures participant pool
- Severity: **HIGH**
- Target: `Tournament economics`
- Bypasses previous fix: **PT-007**
- Estimated financial impact: **Large share of participant rewards; 5-20%/epoch pool capture**
- Attack narrative: Use many funded optimizer IDs to capture participant rewards.
- PoC evidence: `sybil_rewards=14.7619, honest_rewards=30.2381`

### A12 — Watchdog resolve without consensus
- Severity: **HIGH**
- Target: `Watchdog settlement`
- Bypasses previous fix: **PT-008**
- Estimated financial impact: **Unilateral bounty extraction each epoch**
- Attack narrative: Call resolve(true) despite consensus false.
- PoC evidence: `consensus=False, before=5.0, after=6.0`

### A14 — NaN timestamp crash (DoS)
- Severity: **MEDIUM**
- Target: `Watchdog evidence validation`
- Bypasses previous fix: **PT-009**
- Estimated financial impact: **Monitoring loop crash / request-level DoS**
- Attack narrative: Submit NaN timestamp to trigger unhandled exception.
- PoC evidence: `ValueError:cannot convert float NaN to integer`

### A16 — Legacy ACP replay accepted by default
- Severity: **HIGH**
- Target: `ACP auth`
- Bypasses previous fix: **PT-011**
- Estimated financial impact: **Replay signed control messages**
- Attack narrative: Replay legacy message when allow_legacy default remains true.
- PoC evidence: `verify1=True, verify2=True`

### A17 — Public-key registry hijack enables impersonation
- Severity: **CRITICAL**
- Target: `ACP auth / registry`
- Bypasses previous fix: **PT-012**
- Estimated financial impact: **Full impersonation of victim agent**
- Attack narrative: Overwrite victim public key then sign as victim.
- PoC evidence: `impersonation_verify=True`

### A20 — NaN burn amount enqueue crash
- Severity: **MEDIUM**
- Target: `Redemption queue`
- Bypasses previous fix: **PT-014**
- Estimated financial impact: **Queue DoS**
- Attack narrative: Pass NaN burn amount into enqueue path.
- PoC evidence: `ValueError:cannot convert float NaN to integer`

### A24 — Predictable commit_proof bypass for split rebalances
- Severity: **HIGH**
- Target: `Keeper rebalance guard`
- Bypasses previous fix: **PT-018**
- Estimated financial impact: **Policy manipulation; potential multi-% TVL adverse allocation**
- Attack narrative: Satisfy commit proof with public epoch/hash string (no pre-commit).
- PoC evidence: `result={'status': 'APPLIED', 'weights': [0.46, 0.24, 0.2, 0.1], 'mint_fee': 0.002}, turnover_window=[0.039999999999999925, 0.040000000000000036, 0.040000000000000036]`

### A25 — Toxic collateral still mints nontrivial amount
- Severity: **MEDIUM**
- Target: `Mint risk gate`
- Bypasses previous fix: **PT-019**
- Estimated financial impact: **Undercollateralized mint pressure; 1-10% TVL in stress**
- Attack narrative: Set extreme risk score and verify mint remains >0.
- PoC evidence: `minted=767691`

### A31 — Safe-mode release without health context
- Severity: **MEDIUM**
- Target: `Response safety gate`
- Bypasses previous fix: **PT-025**
- Estimated financial impact: **Premature recovery under unhealthy state**
- Attack narrative: Invoke recovery with health=None after cooldown.
- PoC evidence: `result={'safe_mode': False, 'registration_frozen': False, 'rate_limit_enabled': False}`

### A32 — Case/format signature bypass
- Severity: **MEDIUM**
- Target: `Forensics ↔ executor blocklist`
- Bypasses previous fix: **PT-026**
- Estimated financial impact: **Bypass blocked signatures**
- Attack narrative: Insert uppercase blocked signature and execute lower-case signature path.
- PoC evidence: `status=partial_success, success=True, blocked_entry=uppercase`

### A33 — Chain-content morph collision
- Severity: **MEDIUM**
- Target: `Forensic signature robustness`
- Bypasses previous fix: **PT-027**
- Estimated financial impact: **Variant attacks evade blocklist despite same signature**
- Attack narrative: Alter chain internals while preserving chain depth.
- PoC evidence: `sig_a=82d26edc9519934771eb, sig_b=82d26edc9519934771eb`

### A34 — Permissionless Pyth oracle update surface
- Severity: **MEDIUM**
- Target: `Solana on-chain oracle path`
- Bypasses previous fix: **No direct PT mapping**
- Estimated financial impact: **Potential grief/MEV surface if feed account compromised**
- Attack narrative: Static check: update_oracle_pyth lacks keeper quorum + signer accounts.
- PoC evidence: `has_quorum_call=False, has_keeper_signers=False`

## Blocked Attempts (Defense Held)
- **A04 Classic withdraw-overdraw after slash** | Target: `OAE staking` | PT: `PT-002` | Evidence: `request_ok=True, withdrawn=10.0`
- **A05 Duplicate reveal replay** | Target: `Tournament commit/reveal` | PT: `PT-003` | Evidence: `commit=True, reveal1=True, reveal2=False`
- **A06 Direct-submit path in ops-disabled mode** | Target: `Tournament submission` | PT: `PT-004` | Evidence: `direct_submit=False`
- **A07 Risk-zero score explosion** | Target: `Tournament scoring` | PT: `PT-005` | Evidence: `score=10.0`
- **A09 One-agent multiple proposals same epoch** | Target: `Tournament anti-sybil` | PT: `PT-007` | Evidence: `first=True, second=False`
- **A11 Duplicate watchdog resolve replay** | Target: `Watchdog settlement` | PT: `PT-008` | Evidence: `balance_first=6.0, balance_second=6.0`
- **A13 Future timestamp spoof** | Target: `Watchdog evidence validation` | PT: `PT-009` | Evidence: `report_ok=False`
- **A15 Lexicographic first-reporter bounty** | Target: `Watchdog bounty ordering` | PT: `PT-010` | Evidence: `zzz=6.0, aaa=5.0`
- **A18 Caller epoch spoof against limiter** | Target: `Rate limiting` | PT: `PT-013` | Evidence: `allow=[True,True,False]`
- **A19 Tiny-order discount poisoning** | Target: `Redemption queue` | PT: `PT-014` | Evidence: `victim_units=1333329, attacker_units=0`
- **A21 Last-user residual theft** | Target: `Redemption queue` | PT: `PT-015` | Evidence: `u3=1, treasury_residual=2`
- **A22 CB3/CB4 rollback inconsistency regression** | Target: `Circuit-breaker policy` | PT: `PT-016` | Evidence: `pt16_markers_present=True`
- **A23 Commit overwrite** | Target: `Tournament commit` | PT: `PT-017` | Evidence: `commit1=True, commit2=False`
- **A26 Global mint DoS from single degraded vault** | Target: `Oracle degradation scope` | PT: `PT-020` | Evidence: `enabled_assets=[True, False, True, True]`
- **A27 Attack-id grinding outcome manipulation** | Target: `Adversarial executor` | PT: `PT-021` | Evidence: `r1=failed, r2=failed`
- **A28 Signature bucket collision** | Target: `Adversarial executor` | PT: `PT-022` | Evidence: `sig_a=b493e196a13d76fa, sig_b=53aada4958229af5`
- **A29 Schema mismatch collusion evasion** | Target: `Anomaly detector` | PT: `PT-023` | Evidence: `clusters=[{'type': 'collusion', 'agents': ['a', 'b'], 'similarity': 1.0, 'epoch': 1}]`
- **A30 Random alert-id idempotency bypass** | Target: `Response engine` | PT: `PT-024` | Evidence: `first=rate_limit, second=noop`
- **A35 migrate_legacy_state unauthorized reinit** | Target: `Solana migration` | PT: `-` | Evidence: `trusted_initializer_guard=True`
- **A36 Mint account-substitution attack** | Target: `Solana mint CPI path` | PT: `-` | Evidence: `guard_checks_present=[True, True, True, True]`

## Highest-Risk Findings to Patch First
1. **A01/A02/A03/A17 (CRITICAL)**: reward/slash/auth pathways allow direct economic forgery and impersonation.
2. **A08 (HIGH)**: NaN proposal values pass invariant gates and corrupt control parameters.
3. **A12 (HIGH)**: watchdog `resolve()` can finalize rewards without consensus check.
4. **A24 (HIGH)**: PT-018 commit-proof is predictable and does not enforce true commit-reveal.
5. **A32/A33 (MEDIUM-HIGH)**: signature normalization/content coverage gaps permit forensic blocklist evasion.

## Notes
- This campaign was executed as an exploit-oriented red-team pass: each attempt was executed in code, producing machine-readable evidence.
- Several blocked vectors demonstrate meaningful hardening from PT-002/3/4/5/7/8/9/10/13/14/15/16/17/20/21/22/23/24.
- Git push was **not** executed from this subagent run.