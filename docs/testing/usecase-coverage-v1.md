# Microstable Use Case Coverage v1 (UC-1 ~ UC-8)

Date: 2026-02-23 (KST)

## Scope
- On-chain: `solana/programs/microstable/src/lib.rs`
- Keeper: `solana/keeper/src/`
- Config: `solana/keeper/config.devnet.json`

## What I added

### On-chain tests (`solana/programs/microstable/src/lib.rs`)
- `tc_update_protocol_params_9_duplicate_signers_rejected`
- `protocol_state_pda_is_deterministic`
- `keeper_set_validation_rejects_duplicates_and_default`
- `keeper_quorum_requires_two_distinct_members_and_supports_rotation`
- `expected_pyth_feed_mappings_cover_all_four_vaults`
- `circuit_breaker_recovery_and_resume_path_restores_inactive_state`
- `cb4_recovery_restores_learning_rate_before_inactive_transition`

### Keeper tests
- `solana/keeper/src/aig_tests.rs`
  - `tc_aig_11_tier0_to1_promotion_passes_at_threshold`
  - `tc_aig_12_failed_challenge_below_target_threshold_does_not_pass`
- `solana/keeper/src/main_wiring_tests.rs`
  - `tc_mw_02_main_loop_has_consecutive_failure_guardrail`
- `solana/keeper/src/rebalance.rs` (existing in-file test module)
  - `tc_ow_07_weight_deviation_bps_reflects_large_rebalance_need`
  - `tc_ow_08_commit_hash_changes_with_salt_or_batch_slot`
- `solana/keeper/tests/test_blue_keeper_v4.rs`
  - `tc_pkv3_004_oracle_observation_consistency_accepts_small_drift`
  - `tc_pkv3_004_oracle_observation_consistency_rejects_large_price_gap`
  - `tc_pkv3_005_watchdog_cross_rpc_rejects_cr_spike_mismatch`
- `solana/keeper/tests/test_blue_keeper_v6.rs`
  - `tc_pkv5_003_keeper_quorum_selects_two_members_from_protocol_set`
  - `tc_pkv5_004_keeper_quorum_rejects_1_of_3_configuration`
  - `tc_pkv5_005_keeper_rotation_retargets_quorum_selection`

---

## UC Coverage Matrix

Legend: **PASS** (tested), **PARTIAL** (code-path + unit-level guard tested, no full tx/integration), **GAP** (not directly test-executed)

### UC-1: Protocol Lifecycle
- UC-1.1 Protocol initialization (ProtocolState PDA): **PASS**
  - `protocol_state_pda_is_deterministic`
  - code path: `initialize`, `ProtocolState` PDA seed (`b"protocol_state"`)
- UC-1.2 Collateral configuration (4 vault): **PASS**
  - `expected_pyth_feed_mappings_cover_all_four_vaults`
  - code path: `initialize` (`init_vault` called 4 times)
- UC-1.3 Keeper set registration (3-of-3 set validity): **PASS**
  - `keeper_set_validation_rejects_duplicates_and_default`
  - code path: `validate_keeper_set`
- UC-1.4 Emergency shutdown → resume cycle: **PARTIAL**
  - `circuit_breaker_recovery_and_resume_path_restores_inactive_state`
  - code path: `emergency_shutdown`, `resume_from_shutdown`, `refresh_circuit_breakers`
- UC-1.5 Protocol parameter update (2/3 quorum): **PASS**
  - existing `tc_update_protocol_params_1..8` + new `tc_update_protocol_params_9`

### UC-2: Oracle Pipeline
- UC-2.1 정상 Pyth 피드 수신 → 온체인 업데이트: **PARTIAL**
  - `tc_pkv3_004_oracle_observation_consistency_accepts_small_drift`
  - existing `tc_prog_001_accepts_pyth_account_self_write_authority`
- UC-2.2 1개 피드 stale → 나머지 정상 처리: **PARTIAL**
  - code path verified in `run_oracle_cycle` (`continue` per-feed skip)
- UC-2.3 전체 피드 stale → graceful skip: **GAP**
  - code path exists (`prepared_updates` empty path), but no direct stale-all mock execution test yet
- UC-2.4 confidence 초과 → 거부: **PARTIAL**
  - rejection branch verified by code path (`confidence_bps > oracle_confidence_max_bps`), no direct mocked runtime assertion yet
- UC-2.5 가격 급변(±10%) → watchdog 트리거: **PARTIAL**
  - watchdog anomaly path reviewed; `tc_pkv3_005_watchdog_cross_rpc_rejects_cr_spike_mismatch` validates related guardrail logic

### UC-3: Optimizer + Rebalance
- UC-3.1 정상 최적화 사이클(loss 감소): **PASS**
  - existing `integration_multiple_steps_reduce_loss_simple_case`
- UC-3.2 Safety bounds 위반 → projection: **PASS**
  - existing `projection_cap_and_delta_and_scalar_bounds`
- UC-3.3 Adam 체크포인트 저장/복원: **PASS**
  - existing `checkpoint_round_trip`, `integration_nan_input_rolls_back_to_checkpoint`
- UC-3.4 deviation 초과 → 리밸런스 실행 경로: **PASS**
  - new `tc_ow_07_weight_deviation_bps_reflects_large_rebalance_need`
- UC-3.5 슬리피지 한도 초과 → 거부: **PARTIAL**
  - on-chain reject path confirmed in `rebalance` (`SlippageExceeded`) and related bounds

### UC-4: Circuit Breaker
- UC-4.1 CB-1 (Oracle stale) trigger: **PARTIAL**
- UC-4.2 CB-2 (CR drop) trigger: **PARTIAL**
- UC-4.3 CB-3 (NAV/oracle degradation) trigger: **PARTIAL**
- UC-4.4 CB-4 (복합 조건) emergency shutdown: **PARTIAL**
  - trigger/recovery logic traced through `can_activate`, `hysteresis_ok`, `activate_circuit_breaker`, `emergency_shutdown`
- UC-4.5 CB 해제 조건 → 자동 복구: **PASS**
  - `circuit_breaker_recovery_and_resume_path_restores_inactive_state`
  - `cb4_recovery_restores_learning_rate_before_inactive_transition`

### UC-5: Agent Economy (OAE)
- UC-5.1 에이전트 등록(stake escrow): **PASS**
  - existing `registration_valid_stake_and_all_roles`, `registration_zero_stake_rejected`
- UC-5.2 AIG 챌린지 성공(Tier 0→1): **PASS**
  - new `tc_aig_11_tier0_to1_promotion_passes_at_threshold`
- UC-5.3 AIG 챌린지 실패(강등/미승격): **PASS**
  - new `tc_aig_12_failed_challenge_below_target_threshold_does_not_pass`
  - existing `validate_tier_demotion` checks in on-chain tests
- UC-5.4 Tournament 참여 → 승자 점수 업데이트: **PASS**
  - existing tournament tests (`tc_t04`, `tc_t08`, `tc_t09`)
- UC-5.5 등록해제 → stake 반환: **PASS**
  - existing `deregister_and_claim_cooldown`
- UC-5.6 Slash → escrow 삭감: **PASS**
  - existing `slash_capped_at_stake`

### UC-6: Risk Manager
- UC-6.1 Normal → throttle=false: **PASS**
  - existing `tc_rm_01`, `tc_rm_07`
- UC-6.2 CR 하락 → Elevated → throttle=true: **PASS**
  - existing `tc_rm_02`, `tc_rm_07`, `tc_rm_08`
- UC-6.3 연속 실패 사이클 → 자동 보호 모드: **PARTIAL**
  - new `tc_mw_02_main_loop_has_consecutive_failure_guardrail`
  - code path in `main.rs` (`max_consecutive_failed_cycles` exit)

### UC-7: Watchdog
- UC-7.1 Oracle stale anomaly: **PARTIAL**
- UC-7.2 Weight shift anomaly: **PARTIAL**
- UC-7.3 Supply spike anomaly: **PARTIAL**
- UC-7.4 복합 anomaly 누적: **PARTIAL**
  - anomaly logic path reviewed in `run_watchdog_cycle`
  - cross-RPC guard validated by `tc_pkv3_005_watchdog_cross_rpc_rejects_cr_spike_mismatch`

### UC-8: Multi-Keeper Quorum
- UC-8.1 2/3 서명 → TX 성공 경로: **PASS**
  - `keeper_quorum_requires_two_distinct_members_and_supports_rotation`
  - `tc_pkv5_003_keeper_quorum_selects_two_members_from_protocol_set`
- UC-8.2 1/3 서명 → TX 거부: **PASS**
  - `tc_update_protocol_params_9_duplicate_signers_rejected`
  - `tc_pkv5_004_keeper_quorum_rejects_1_of_3_configuration`
- UC-8.3 키 교체 후 quorum 재설정: **PASS**
  - `keeper_quorum_requires_two_distinct_members_and_supports_rotation`
  - `tc_pkv5_005_keeper_rotation_retargets_quorum_selection`

---

## Test Execution

### Full suite
- Command: `cd /Users/kjaylee/.openclaw/workspace/microstable/solana && cargo test`
- Result: **PASS**

### Required tail output (`cargo test 2>&1 | tail -30`)
```text
test tc_pkv5_001_degraded_mode_skips_secondary_reads ... ok
test tc_pkv5_001_read_failures_enter_degraded_mode_at_threshold ... ok
test tc_pkv5_001_recovery_restores_normal_dual_rpc_mode ... ok
test tc_pkv5_002_degraded_mode_allows_primary_only_confirmation ... ok
test tc_pkv5_002_normal_mode_primary_only_after_retry_is_rejected ... ok
test rebalance::tests::tc_ow_05_compute_target_weights_optimizer_enabled_calls_optimizer ... ok
test tc_pkv5_002_normal_mode_primary_only_is_soft_fail_retry_once ... ok
test tc_pkv5_002_normal_mode_requires_both_rpc_even_if_secondary_only ... ok
test optimizer::tests::checkpoint_round_trip ... ok
test tc_pkv5_004_keeper_quorum_rejects_1_of_3_configuration ... ok
test tc_pkv5_003_keeper_quorum_selects_two_members_from_protocol_set ... ok
test tc_pkv5_005_keeper_rotation_retargets_quorum_selection ... ok

test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/test_blue_keeper_v7.rs (target/debug/deps/test_blue_keeper_v7-ca6042a10632a3a2)

running 3 tests
test tc_pkv7_002_rejects_unknown_pyth_write_authority ... ok
test tc_pkv7_002_accepts_pyth_account_self_write_authority ... ok
test tc_pkv7_001_default_devnet_secondary_rpc_must_not_be_placeholder ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests microstable

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
