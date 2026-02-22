# Microstable Red Team v2 Report — 26 Additional Attack Vectors

Date: 2026-02-22 13:24:05

## Scope
- `microstable.py`
- `solana/programs/microstable/src/lib.rs`
- `agents/keeper.py`, `watchdog.py`, `auditor.py`, `consensus.py`
- `stress_test.py`
- `specs/microstable/spec.md`
- Prior report: `security/red-team-report.md` (v1 baseline)

## Execution Proof (all PoCs run)
```bash
cd /Users/kjaylee/.openclaw/workspace/microstable
python3 security/red_team_v2_exploits.py > security/red-team-v2-exploit-output.json
```

Observed summary:
- total_attacks=26
- vulnerable=21
- severity_counts={'CRITICAL': 3, 'HIGH': 11, 'MEDIUM': 7}

Output artifact: `security/red-team-v2-exploit-output.json` (full JSON evidence + summary).

## New Findings (E-I)

### E1. Time-weighted manipulation over 1000 ticks
- **Category:** Advanced Economic
- **Vector:** Slow deterministic oracle drift nudges optimizer into exploitable allocation before reversal.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_e1_time_weighted_manipulation`
- **Rating:** MEDIUM
- **Exploitability:** Medium
- **Status:** NOT REPRODUCED / THEORETICAL
- **Evidence:** weights_final=[0.40002244717024804, 0.30001139487894163, 0.19999710045587638, 0.09996905749493405]; weights_baseline=[0.4, 0.3, 0.2, 0.1]; l1_weight_drift=6.76840983792476e-05; nav_baseline_on_reversal=0.979979
- **Fix:** Use drift-resistant TWAP/median anchors and penalize cumulative directional weight drift.

### E2. Collateral substitution (high-quality redemption vs degraded deposit)
- **Category:** Advanced Economic
- **Vector:** Deposit degraded collateral priced at stale/par oracle, redeem diversified basket by unit share.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_e2_collateral_substitution`
- **Rating:** CRITICAL
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** true_prices=[1.0, 1.0, 1.0, 0.72]; oracle_prices=[1.0, 1.0, 1.0, 1.0]; minted_musd=831666; payout_units=[243473, 243473, 243473, 365210]
- **Fix:** Redeem by value, not raw units; enforce oracle freshness/medianization and collateral quality gates.

### E3. Fee extraction loop via rounding dust
- **Category:** Advanced Economic
- **Vector:** Brute-force micro transaction sizes to locate positive rounding loop.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_e3_fee_extraction_loop`
- **Rating:** INFO
- **Exploitability:** Theoretical
- **Status:** NOT REPRODUCED / THEORETICAL
- **Evidence:** best_micro_amount=6; best_per_cycle_pnl_units=-2; simulated_loops=200000; cumulative_pnl_units=-400000
- **Fix:** Keep ceil/floor asymmetry audited; add min trade size and anti-loop fee floor if profit emerges.

### E4. Liquidity crunch via coordinated mass redemptions
- **Category:** Advanced Economic
- **Vector:** Redeem wave just before oracle-degraded penalty creates first-mover extraction and accelerates crunch.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_e4_liquidity_crunch`
- **Rating:** HIGH
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** early_redeem_value=1333332; late_redeem_value=1266664; first_mover_advantage_units=66668; early_payout=[333333, 333333, 333333, 333333]
- **Fix:** Introduce redemption queue + time-smoothing so equal burns get equal treatment across boundary slots.

### E5. Cross-collateral correlated depeg blind spot
- **Category:** Advanced Economic
- **Vector:** Two major assets drift below per-asset trigger but jointly degrade basket value without CB-2 activation.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_e5_cross_collateral_correlation`
- **Rating:** HIGH
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** cb2_activations=0; avg_nav=0.9787045; min_nav=0.9787045; price_pattern=[0.981, 0.981, 1.0, 1.0]
- **Fix:** Add portfolio-level correlation/dependency stress trigger, not only per-asset depeg thresholds.

### E6. Governance parameter attack (CR target gradient push)
- **Category:** Advanced Economic
- **Vector:** Sequential small parameter changes remain valid individually but cumulatively push CR target to extreme.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_e6_governance_parameter_gradient`
- **Rating:** HIGH
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** steps_tested=21; all_steps_valid=True; mint_at_cr_1_20=831666; mint_at_cr_1_00=998000
- **Fix:** Add rate-of-change governance guardrails and cumulative risk budget per epoch.

### F7. Account resurrection attempt (PDA close/recreate)
- **Category:** Advanced Smart Contract
- **Vector:** Attempt to close and recreate PDA state with altered data.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_f7_account_resurrection`
- **Rating:** LOW
- **Exploitability:** Theoretical
- **Status:** NOT REPRODUCED / THEORETICAL
- **Evidence:** close_path_detected_in_source=False; realloc_detected_in_source=False; result=No direct resurrection path found in current source.
- **Fix:** Keep no-close policy for core PDAs and explicitly deny realloc/close in critical account handlers.

### F8. Instruction ordering bug (CB-4 recovery keeps LR degraded)
- **Category:** Advanced Smart Contract
- **Vector:** State transitions set CB-4 to Inactive before learning-rate restoration branch can run.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_f8_instruction_ordering`
- **Rating:** MEDIUM
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** final_status_cb4=0; learning_rate_scale_final=500000; expected_after_recovery=1000000
- **Fix:** Restore learning_rate_scale before Recovery->Inactive transition or upon entering Inactive post-cooldown.

### F9. Compute budget exhaustion / keeper inclusion starvation
- **Category:** Advanced Smart Contract
- **Vector:** High-CU spam transactions consume block budget and exclude safety/keeper instructions.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_f9_compute_budget_exhaustion`
- **Rating:** MEDIUM
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** block_compute_limit=48000000; attacker_total_cu=48400000; remaining_cu=0; keeper_required_txs=3
- **Fix:** Use priority fees/reserved keeper lanes and bundle critical instructions with QoS guarantees.

### F10. Anchor discriminator collision forging
- **Category:** Advanced Smart Contract
- **Vector:** Brute-force instruction discriminator collision against 8-byte Anchor selector.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_f10_anchor_discriminator_collision`
- **Rating:** INFO
- **Exploitability:** Theoretical
- **Status:** NOT REPRODUCED / THEORETICAL
- **Evidence:** discriminators={'initialize': 'afaf6d1f0d989bed', 'update_oracle': '7029d112f8e2fcbc', 'mint': '3339e12fb69289a6', 'redeem': 'b80c569546c461e1', 'rebalance': '6c9e4d09d234583e', 'activate_circuit_breaker': '43f09f71078af7ae', 'recover_circuit_breaker': 'a2af4b78576cc461'}; attempts=150000; collision_found=None; target=3339e12fb69289a6
- **Fix:** Keep 8-byte discriminators + strict account constraints; optional runtime allowlist of instruction IDs.

### F11. Upgradeable program authority exploitability
- **Category:** Advanced Smart Contract
- **Vector:** If upgrade authority is retained/compromised, protocol logic can be swapped without user opt-in.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_f11_upgrade_authority`
- **Rating:** HIGH
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** program_id_present=True; immutability_proof_file_present=False; proof_path_checked=/Users/kjaylee/.openclaw/workspace/microstable/security/program-immutability-proof.txt
- **Fix:** Publish verifiable ProgramData authority status (None) and pin immutable release artifacts.

### F12. Cross-program invocation chain keeper takeover
- **Category:** Advanced Smart Contract
- **Vector:** Initialization race + PDA signer via CPI can permanently assign keeper to attacker-controlled program.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_f12_cpi_chain_takeover`
- **Rating:** CRITICAL
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** keeper_after_init=PDA::malicious_program::keeper; attacker_privileged_call_success=True; note=Model reproduces authority assignment semantics; mitigate with trusted initializer + governance handoff.
- **Fix:** Restrict initialize to trusted deploy authority and require one-time governance-set keeper multisig.

### G13. Sybil agent attack (fake keeper/watchdog/auditor quorum)
- **Category:** Agent Coordination
- **Vector:** Consensus accepts raw CLI votes without identity, attestation, or stake-weighted authentication.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_g13_sybil_agents`
- **Rating:** CRITICAL
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** votes={'keeper': True, 'watchdog': True, 'auditor': True}; queued=True; required_yes=2
- **Fix:** Require cryptographic signatures for each agent identity and on-chain quorum verification.

### G14. Timing attack on consensus timelock boundary
- **Category:** Agent Coordination
- **Vector:** Timelock exists in output metadata but no enforceable pre-execution state machine.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_g14_timelock_boundary`
- **Rating:** HIGH
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** queued_immediately=True; eta_unix=1771907044; decision={'action': 'QUEUE_GOVERNANCE_ACTION', 'queued': True}
- **Fix:** Persist proposal state and enforce execute-only-after-ETA with immutable proposal hash.

### G15. Agent state desynchronization
- **Category:** Agent Coordination
- **Vector:** Agents operate on local ephemeral snapshots with no shared signed state root.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_g15_agent_state_desync`
- **Rating:** HIGH
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** keeper_tick=40; watchdog_tick=39; keeper_prices=[0.9999360299278881, 1.0001278578781292, 0.9999434759588042, 0.9999212328944173]; watchdog_prices=[1.000093353792406, 1.0006332696970153, 1.0002738331869097, 1.0002784516566379]
- **Fix:** Use canonical state hash + round id; reject votes/recommendations from mismatched epochs.

### G16. Replay old keeper recommendation in new market regime
- **Category:** Agent Coordination
- **Vector:** Proposal acceptance checks bounds only; no proposal nonce, expiry, or market-state binding.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_g16_replay_attack`
- **Rating:** HIGH
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** stale_proposal={'weights': [0.42, 0.28, 0.2, 0.1], 'mint_fee': 0.002}; replay_apply_status=APPLIED; loss_before=0.45443970751687207; loss_after_replay=0.4820869152448319
- **Fix:** Attach signed epoch+state hash+expiry to proposals and reject stale submissions.

### G17. Agent starvation under transaction queue congestion
- **Category:** Agent Coordination
- **Vector:** No reserved throughput or priority lane for keeper/watchdog/auditor transactions.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_g17_agent_starvation`
- **Rating:** MEDIUM
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** block_tx_capacity=100; attacker_spam_txs=100; critical_agent_txs=3; admitted_agent_txs=0
- **Fix:** Reserve protocol-critical tx bandwidth and implement priority fee policy for safety agents.

### H18. Seed predictability in simulation RNG
- **Category:** Cryptographic/System
- **Vector:** Pseudo-random market path is deterministic for known seed (default seed=0 used widely).
- **Exploit code:** `security/red_team_v2_exploits.py::attack_h18_seed_predictability`
- **Rating:** MEDIUM
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** seed=0; first_path_tick0=[1.0002354288511701, 0.9996508554738247, 0.9998300713879804, 1.0000926258918652]; paths_identical_over_20_ticks=True
- **Fix:** Use unpredictable entropy for production-like testing and hide/rotate seeds for adversarial evaluations.

### H19. Deterministic exploit windows from fixed seed
- **Category:** Cryptographic/System
- **Vector:** Attacker can precompute exact depeg/vulnerable windows and schedule adversarial actions.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_h19_deterministic_exploit_window`
- **Rating:** MEDIUM
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** window_ticks_run1=[20, 21, 22, 23, 24, 25, 26, 27, 28, 29]; window_ticks_run2=[20, 21, 22, 23, 24, 25, 26, 27, 28, 29]; identical=True
- **Fix:** Randomize operational scheduling and evaluate policies under hidden-seed Monte Carlo ensembles.

### H20. Memory exhaustion via deeply nested Value graph
- **Category:** Cryptographic/System
- **Vector:** Unbounded computation graph growth allows attacker to force large memory consumption.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_h20_memory_exhaustion`
- **Rating:** HIGH
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** graph_depth=40000; tracemalloc_current_bytes=110061864; tracemalloc_peak_bytes=110062032
- **Fix:** Cap graph depth/node count per tick and hard-fail before memory amplification.

### H21. Topological-sort cycle injection
- **Category:** Cryptographic/System
- **Vector:** Autograd internals are mutable; cycle can be injected without explicit DAG validation.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_h21_topological_sort_cycle`
- **Rating:** MEDIUM
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** cycle_injected=True; backward_completed_without_cycle_error=True; error_if_any=None; x_grad=4.0
- **Fix:** Make graph links immutable/private and add explicit cycle detection before backward pass.

### H22. Precision cascading over 100K ticks
- **Category:** Cryptographic/System
- **Vector:** Accumulate floating-point update errors across long horizons.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_h22_precision_cascading`
- **Rating:** INFO
- **Exploitability:** Theoretical
- **Status:** NOT REPRODUCED / THEORETICAL
- **Evidence:** cr_float=1.199999978378379; cr_decimal=1.1999999783783784; absolute_drift=6.661338147750939e-16; ticks=100000
- **Fix:** Use fixed-point/decimal arithmetic for critical accounting paths if long-horizon drift becomes material.

### I23. Regulatory arbitrage in asset listing
- **Category:** Protocol Design
- **Vector:** Asset listing path has no legal/compliance metadata gate despite spec-level regulatory constraints.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_i23_regulatory_arbitrage`
- **Rating:** HIGH
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** asset=SANCTIONED_USD_PROXY; queued=True; validation={'ok': True, 'reason': 'ok'}
- **Fix:** Add mandatory compliance oracle and jurisdiction risk checks before listing queueing.

### I24. Black swan cascade across all collateral assets
- **Category:** Protocol Design
- **Vector:** Simultaneous deep shock drives sustained peg deviation even with breaker activation.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_i24_black_swan_cascade`
- **Rating:** HIGH
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** max_peg_error=0.017822011903716906; mean_peg_error=0.003817043404987887; cb2_active_ticks=35; final_cr=1.2463934686503504
- **Fix:** Stress hardening: dynamic haircut escalation + redemption queue + emergency recapitalization policy.

### I25. MEV sandwich around predictable rebalance transaction
- **Category:** Protocol Design
- **Vector:** Predictable keeper trade direction allows front-run/back-run extraction.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_i25_mev_sandwich_rebalance`
- **Rating:** HIGH
- **Exploitability:** Easy
- **Status:** VULNERABLE
- **Evidence:** front_run_input_musd=500000.0; keeper_trade_musd=2000000.0; back_run_output_musd=697554.8340518456; attacker_pnl_musd=197554.8340518456
- **Fix:** Use commit-reveal/batched auctions for rebalances to reduce mempool directional leakage.

### I26. Insurance fund drain via repeated micro-claims
- **Category:** Protocol Design
- **Vector:** Repeated low-value claims can deplete treasury when replenishment is throttled/disabled.
- **Exploit code:** `security/red_team_v2_exploits.py::attack_i26_insurance_fund_drain`
- **Rating:** MEDIUM
- **Exploitability:** Medium
- **Status:** VULNERABLE
- **Evidence:** starting_treasury=1000000.0; claim_size=75.0; loops_attempted=20000; ending_treasury=-50.0
- **Fix:** Implement claim cooldowns, deductibles, per-epoch caps, and dynamic insurance pricing.

## Updated Summary Matrix (v1 + v2)

### v1 Baseline Findings (from `red-team-report.md`)
| ID | Finding | Severity | Exploitability |
|---|---|---|---|
| F-01 | Free mint + oracle-manipulated redeem drain | CRITICAL | Easy |
| F-02 | Keeper key theft blast radius | CRITICAL | Easy |
| F-03 | CB griefing mint DoS | HIGH | Easy |
| F-04 | CB priority mismatch | HIGH | Easy |
| F-05 | Sandwich via forced CB1 rebalance jump | HIGH | Easy |
| F-06 | Consensus veto liveness failure | HIGH | Easy |
| F-07 | Watchdog bypass / non-binding outputs | HIGH | Easy |
| F-08 | CR-target ratchet pressure | MEDIUM | Easy |
| F-09 | CB1 cap reduction bypass math | MEDIUM | Easy |
| F-10 | Weight concentration via forced redistribution | MEDIUM | Medium |
| F-11 | Front-running deterministic safety transitions | MEDIUM | Medium |
| F-12 | Numeric edge-case drift | LOW | Theoretical |

### v2 Findings (this report)
| ID | Finding | Vulnerable | Severity | Exploitability |
|---|---|---|---|---|
| E1 | Time-weighted manipulation over 1000 ticks | No | MEDIUM | Medium |
| E2 | Collateral substitution (high-quality redemption vs degraded deposit) | Yes | CRITICAL | Easy |
| E3 | Fee extraction loop via rounding dust | No | INFO | Theoretical |
| E4 | Liquidity crunch via coordinated mass redemptions | Yes | HIGH | Easy |
| E5 | Cross-collateral correlated depeg blind spot | Yes | HIGH | Medium |
| E6 | Governance parameter attack (CR target gradient push) | Yes | HIGH | Easy |
| F7 | Account resurrection attempt (PDA close/recreate) | No | LOW | Theoretical |
| F8 | Instruction ordering bug (CB-4 recovery keeps LR degraded) | Yes | MEDIUM | Easy |
| F9 | Compute budget exhaustion / keeper inclusion starvation | Yes | MEDIUM | Medium |
| F10 | Anchor discriminator collision forging | No | INFO | Theoretical |
| F11 | Upgradeable program authority exploitability | Yes | HIGH | Medium |
| F12 | Cross-program invocation chain keeper takeover | Yes | CRITICAL | Medium |
| G13 | Sybil agent attack (fake keeper/watchdog/auditor quorum) | Yes | CRITICAL | Easy |
| G14 | Timing attack on consensus timelock boundary | Yes | HIGH | Easy |
| G15 | Agent state desynchronization | Yes | HIGH | Easy |
| G16 | Replay old keeper recommendation in new market regime | Yes | HIGH | Medium |
| G17 | Agent starvation under transaction queue congestion | Yes | MEDIUM | Easy |
| H18 | Seed predictability in simulation RNG | Yes | MEDIUM | Easy |
| H19 | Deterministic exploit windows from fixed seed | Yes | MEDIUM | Easy |
| H20 | Memory exhaustion via deeply nested Value graph | Yes | HIGH | Medium |
| H21 | Topological-sort cycle injection | Yes | MEDIUM | Medium |
| H22 | Precision cascading over 100K ticks | No | INFO | Theoretical |
| I23 | Regulatory arbitrage in asset listing | Yes | HIGH | Easy |
| I24 | Black swan cascade across all collateral assets | Yes | HIGH | Medium |
| I25 | MEV sandwich around predictable rebalance transaction | Yes | HIGH | Easy |
| I26 | Insurance fund drain via repeated micro-claims | Yes | MEDIUM | Medium |

### Consolidated Vulnerability Count (v1 + v2 vulnerable only)
| Severity | v1 | v2 | Total |
|---|---:|---:|---:|
| CRITICAL | 2 | 3 | 5 |
| HIGH | 5 | 11 | 16 |
| MEDIUM | 4 | 7 | 11 |
| LOW | 1 | 0 | 1 |
| INFO | 0 | 0 | 0 |

## Priority Remediation (new in v2)
1. **Block collateral substitution path (E2)**: redeem-by-value, stale-oracle hard fails, asset quality gating.
2. **Harden initialization/authority model (F12/F11)**: trusted one-time initializer, immutable deployment proof, multisig keeper.
3. **Agent auth + timelock enforcement (G13/G14/G16)**: signed votes, nonce/expiry, on-chain stateful timelock executor.
4. **Tail-risk controls (E4/E5/I24/I25)**: correlation trigger, redemption queue, anti-MEV rebalance auctioning.
5. **Runtime hardening (H20/H21)**: cap autograd graph depth/memory and enforce cycle detection.
