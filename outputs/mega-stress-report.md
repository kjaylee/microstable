# Microstable Mega Stress Test Report

Generated: 2026-02-22T13:35:51.481589
Requested runs/scenario: 100
Actual runs/scenario: 100
Workers: 9
Runtime (sec): 546.65

## Gate criteria (relaxed)
- MAE < 0.05
- CR violation rate < 20%
- No crashes / NaN / Inf

## Overall: PASS
Critical scenarios (>50% run failures): 0

## Scenario Summary
| # | Scenario | Category | Status | MAE(mean) | CRv(mean) | Crashes | NaN/Inf |
|---:|---|---|---|---:|---:|---:|---:|
| 1 | single_usdc_depeg_095_mild | Depeg Variations | PASS | 0.000560 | 0.000000 | 0 | 0 |
| 2 | single_usdc_depeg_080_severe | Depeg Variations | PASS | 0.001213 | 0.000000 | 0 | 0 |
| 3 | single_usdt_depeg_050_extreme | Depeg Variations | PASS | 0.002102 | 0.000000 | 0 | 0 |
| 4 | dai_wobble_097_103 | Depeg Variations | PASS | 0.000340 | 0.000000 | 0 | 0 |
| 5 | usds_drop_070_instant_recovery | Depeg Variations | PASS | 0.000342 | 0.000000 | 0 | 0 |
| 6 | sequential_depegs_usdc_usdt_dai | Depeg Variations | PASS | 0.000619 | 0.000000 | 0 | 0 |
| 7 | all_depeg_090_simultaneous | Depeg Variations | PASS | 0.001793 | 0.000000 | 0 | 0 |
| 8 | all_depeg_060_catastrophic | Depeg Variations | PASS | 0.007036 | 0.000000 | 0 | 0 |
| 9 | inverse_depeg_premium_120 | Depeg Variations | PASS | 0.000874 | 0.000000 | 0 | 0 |
| 10 | slow_bleed_01pct_500ticks | Depeg Variations | PASS | 0.003329 | 0.000000 | 0 | 0 |
| 11 | low_spike_low_vol | Volatility Regimes | PASS | 0.000425 | 0.000000 | 0 | 0 |
| 12 | increasing_vol_ramp | Volatility Regimes | PASS | 0.001557 | 0.000000 | 0 | 0 |
| 13 | decreasing_vol_ramp | Volatility Regimes | PASS | 0.001437 | 0.000000 | 0 | 0 |
| 14 | bimodal_vol | Volatility Regimes | PASS | 0.000659 | 0.000000 | 0 | 0 |
| 15 | correlated_vol_all_assets | Volatility Regimes | PASS | 0.001487 | 0.000000 | 0 | 0 |
| 16 | anti_correlated_usdc_usdt | Volatility Regimes | PASS | 0.000361 | 0.000000 | 0 | 0 |
| 17 | fat_tail_cauchy_jumps | Volatility Regimes | PASS | 0.001270 | 0.000000 | 0 | 0 |
| 18 | microstructure_jitter | Volatility Regimes | PASS | 0.000333 | 0.000000 | 0 | 0 |
| 19 | weekend_effect | Volatility Regimes | PASS | 0.000455 | 0.000000 | 0 | 0 |
| 20 | vol_clustering_garch_like | Volatility Regimes | PASS | 0.000352 | 0.000000 | 0 | 0 |
| 21 | oracle_stale_20ticks | Oracle Attacks | PASS | 0.000425 | 0.000000 | 0 | 0 |
| 22 | oracle_stale_100ticks | Oracle Attacks | PASS | 0.000666 | 0.000000 | 0 | 0 |
| 23 | oracle_drift_plus1bps_200 | Oracle Attacks | PASS | 0.000263 | 0.000000 | 0 | 0 |
| 24 | oracle_drift_minus5bps_50 | Oracle Attacks | PASS | 0.000557 | 0.000000 | 0 | 0 |
| 25 | oracle_noise_injection_2pct | Oracle Attacks | PASS | 0.000585 | 0.000000 | 0 | 0 |
| 26 | oracle_front_running_1tick_lead | Oracle Attacks | PASS | 0.000511 | 0.000000 | 0 | 0 |
| 27 | oracle_sandwich_high_low | Oracle Attacks | PASS | 0.001027 | 0.000000 | 0 | 0 |
| 28 | multi_oracle_disagreement | Oracle Attacks | PASS | 0.000203 | 0.000000 | 0 | 0 |
| 29 | oracle_outage_50_then_recover | Oracle Attacks | PASS | 0.000559 | 0.000000 | 0 | 0 |
| 30 | oracle_intermittent_every_3rd | Oracle Attacks | PASS | 0.000740 | 0.000000 | 0 | 0 |
| 31 | cb1_rapid_toggle_attempt | Circuit Breaker Stress | PASS | 0.000677 | 0.000000 | 0 | 0 |
| 32 | all_cbs_simultaneous | Circuit Breaker Stress | PASS | 0.000538 | 0.000000 | 0 | 0 |
| 33 | cb_cascade_1_3_2_4 | Circuit Breaker Stress | PASS | 0.000414 | 0.000000 | 0 | 0 |
| 34 | cb_extended_mode_trigger | Circuit Breaker Stress | PASS | 0.000385 | 0.000000 | 0 | 0 |
| 35 | cb_recovery_race | Circuit Breaker Stress | PASS | 0.000344 | 0.000000 | 0 | 0 |
| 36 | cb_during_rebalance | Circuit Breaker Stress | PASS | 0.000470 | 0.000000 | 0 | 0 |
| 37 | cb_with_max_weight_concentration | Circuit Breaker Stress | PASS | 0.001087 | 0.000000 | 0 | 0 |
| 38 | cb_with_min_weight_collateral | Circuit Breaker Stress | PASS | 0.000342 | 0.000000 | 0 | 0 |
| 39 | cb_cooldown_boundary | Circuit Breaker Stress | PASS | 0.000367 | 0.000000 | 0 | 0 |
| 40 | cb_hysteresis_edge_threshold | Circuit Breaker Stress | PASS | 0.000558 | 0.000000 | 0 | 0 |
| 41 | gradient_explosion | Optimizer Adversarial | PASS | 0.000338 | 0.000000 | 0 | 0 |
| 42 | gradient_vanishing | Optimizer Adversarial | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 43 | saddle_point_zero_grad | Optimizer Adversarial | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 44 | oscillating_loss_direction | Optimizer Adversarial | PASS | 0.000401 | 0.000000 | 0 | 0 |
| 45 | adversarial_gradient_direction | Optimizer Adversarial | PASS | 0.000339 | 0.000000 | 0 | 0 |
| 46 | learning_rate_high_005 | Optimizer Adversarial | PASS | 0.000335 | 0.000000 | 0 | 0 |
| 47 | learning_rate_low_0001 | Optimizer Adversarial | PASS | 0.000336 | 0.000000 | 0 | 0 |
| 48 | weight_stuck_at_boundary | Optimizer Adversarial | PASS | 0.000243 | 0.000000 | 0 | 0 |
| 49 | simplex_projection_stress | Optimizer Adversarial | PASS | 0.000242 | 0.000000 | 0 | 0 |
| 50 | adam_momentum_trap_beta1_099 | Optimizer Adversarial | PASS | 0.000379 | 0.000000 | 0 | 0 |
| 51 | supply_near_zero | Liquidity & Supply | PASS | 0.000338 | 0.000000 | 0 | 0 |
| 52 | supply_huge_1b | Liquidity & Supply | PASS | 0.000336 | 0.000000 | 0 | 0 |
| 53 | rapid_mint_0_to_1m | Liquidity & Supply | PASS | 0.000336 | 0.000000 | 0 | 0 |
| 54 | rapid_redeem_1m_to_0 | Liquidity & Supply | PASS | 0.000338 | 0.000000 | 0 | 0 |
| 55 | oscillating_supply_every_tick | Liquidity & Supply | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 56 | one_sided_mints_200 | Liquidity & Supply | PASS | 0.000336 | 0.000000 | 0 | 0 |
| 57 | one_sided_redeems_200 | Liquidity & Supply | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 58 | supply_spike_then_plateau | Liquidity & Supply | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 59 | supply_growth_1pct_per_tick | Liquidity & Supply | PASS | 0.000335 | 0.000000 | 0 | 0 |
| 60 | supply_crash_90pct_3ticks | Liquidity & Supply | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 61 | depeg_plus_oracle_failure | Multi-Factor Stress | PASS | 0.001208 | 0.000000 | 0 | 0 |
| 62 | high_vol_plus_low_liquidity | Multi-Factor Stress | PASS | 0.000648 | 0.000000 | 0 | 0 |
| 63 | flash_crash_cb_cascade_oracle_stale | Multi-Factor Stress | PASS | 0.000936 | 0.000000 | 0 | 0 |
| 64 | triple_depeg_plus_gradient_attack | Multi-Factor Stress | PASS | 0.002770 | 0.000000 | 0 | 0 |
| 65 | max_concentration_then_depeg | Multi-Factor Stress | PASS | 0.002318 | 0.000000 | 0 | 0 |
| 66 | all_mild_compound_stress | Multi-Factor Stress | PASS | 0.001244 | 0.000000 | 0 | 0 |
| 67 | recovery_from_max_stress | Multi-Factor Stress | PASS | 0.005401 | 0.000000 | 0 | 0 |
| 68 | normal_crisis_oscillation_20ticks | Multi-Factor Stress | PASS | 0.001605 | 0.000000 | 0 | 0 |
| 69 | gradual_degradation_1000ticks | Multi-Factor Stress | PASS | 0.003025 | 0.000000 | 0 | 0 |
| 70 | sudden_improvement_after_500 | Multi-Factor Stress | PASS | 0.003622 | 0.000000 | 0 | 0 |
| 71 | perfect_peg_500ticks | Edge Cases & Boundary | PASS | 0.000324 | 0.000000 | 0 | 0 |
| 72 | equal_weights_025 | Edge Cases & Boundary | PASS | 0.000351 | 0.000000 | 0 | 0 |
| 73 | single_weight_at_max_cap | Edge Cases & Boundary | PASS | 0.000330 | 0.000000 | 0 | 0 |
| 74 | cr_exactly_target | Edge Cases & Boundary | PASS | 0.000338 | 0.000000 | 0 | 0 |
| 75 | fee_zero | Edge Cases & Boundary | PASS | 0.000334 | 0.000000 | 0 | 0 |
| 76 | fee_max_10pct | Edge Cases & Boundary | PASS | 0.000337 | 0.000000 | 0 | 0 |
| 77 | price_to_00001 | Edge Cases & Boundary | PASS | 0.004611 | 0.000000 | 0 | 0 |
| 78 | very_long_50000ticks | Edge Cases & Boundary | PASS | 0.000327 | 0.000000 | 0 | 0 |
| 79 | oracle_confidence_0_1_alternating | Edge Cases & Boundary | PASS | 0.000828 | 0.000000 | 0 | 0 |
| 80 | all_params_extreme_boundary | Edge Cases & Boundary | PASS | 0.026840 | 0.000000 | 0 | 0 |

## Critical Scenarios
- None

## Per-scenario details

### [1] single_usdc_depeg_095_mild — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000560, p95=0.000585, worst=0.000597
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.113s, p95=0.136s

### [2] single_usdc_depeg_080_severe — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001213, p95=0.001239, worst=0.001248
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.122s, p95=0.146s

### [3] single_usdt_depeg_050_extreme — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.002102, p95=0.002132, worst=0.002151
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.107s, p95=0.127s

### [4] dai_wobble_097_103 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000340, p95=0.000361, worst=0.000371
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.146s, p95=0.165s

### [5] usds_drop_070_instant_recovery — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000342, p95=0.000369, worst=0.000376
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.101s, p95=0.121s

### [6] sequential_depegs_usdc_usdt_dai — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000619, p95=0.000642, worst=0.000651
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.144s, p95=0.173s

### [7] all_depeg_090_simultaneous — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001793, p95=0.001821, worst=0.001830
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.108s, p95=0.129s

### [8] all_depeg_060_catastrophic — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.007036, p95=0.007062, worst=0.007073
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.110s, p95=0.145s

### [9] inverse_depeg_premium_120 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000874, p95=0.000901, worst=0.000909
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.122s, p95=0.147s

### [10] slow_bleed_01pct_500ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.003329, p95=0.003344, worst=0.003348
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.384s, p95=0.424s

### [11] low_spike_low_vol — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000425, p95=0.000492, worst=0.000519
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.138s, p95=0.173s

### [12] increasing_vol_ramp — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001557, p95=0.001889, worst=0.002041
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.135s, p95=0.167s

### [13] decreasing_vol_ramp — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001437, p95=0.001784, worst=0.002493
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.140s, p95=0.178s

### [14] bimodal_vol — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000659, p95=0.000804, worst=0.000966
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.137s, p95=0.176s

### [15] correlated_vol_all_assets — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001487, p95=0.001985, worst=0.002698
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.128s, p95=0.155s

### [16] anti_correlated_usdc_usdt — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000361, p95=0.000427, worst=0.000588
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.131s, p95=0.165s

### [17] fat_tail_cauchy_jumps — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001270, p95=0.001599, worst=0.001919
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.133s, p95=0.160s

### [18] microstructure_jitter — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000333, p95=0.000353, worst=0.000362
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.164s, p95=0.199s

### [19] weekend_effect — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000455, p95=0.000535, worst=0.000659
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.175s, p95=0.212s

### [20] vol_clustering_garch_like — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000352, p95=0.000409, worst=0.000438
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.166s, p95=0.205s

### [21] oracle_stale_20ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000425, p95=0.000463, worst=0.000490
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.129s, p95=0.163s

### [22] oracle_stale_100ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000666, p95=0.000733, worst=0.000742
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.174s, p95=0.220s

### [23] oracle_drift_plus1bps_200 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000263, p95=0.000287, worst=0.000297
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.186s, p95=0.220s

### [24] oracle_drift_minus5bps_50 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000557, p95=0.000589, worst=0.000608
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.120s, p95=0.153s

### [25] oracle_noise_injection_2pct — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000585, p95=0.000629, worst=0.000644
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.115s, p95=0.150s

### [26] oracle_front_running_1tick_lead — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000511, p95=0.000546, worst=0.000567
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.135s, p95=0.166s

### [27] oracle_sandwich_high_low — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001027, p95=0.001048, worst=0.001069
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.114s, p95=0.143s

### [28] multi_oracle_disagreement — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000203, p95=0.000226, worst=0.000234
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.106s, p95=0.135s

### [29] oracle_outage_50_then_recover — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000559, p95=0.000598, worst=0.000620
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.159s, p95=0.203s

### [30] oracle_intermittent_every_3rd — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000740, p95=0.000768, worst=0.000782
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.125s, p95=0.155s

### [31] cb1_rapid_toggle_attempt — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000677, p95=0.000696, worst=0.000705
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.151s, p95=0.181s

### [32] all_cbs_simultaneous — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000538, p95=0.000565, worst=0.000579
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.192s

### [33] cb_cascade_1_3_2_4 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000414, p95=0.000437, worst=0.000460
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.127s, p95=0.157s

### [34] cb_extended_mode_trigger — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000385, p95=0.000412, worst=0.000422
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.187s

### [35] cb_recovery_race — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000344, p95=0.000366, worst=0.000374
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.138s, p95=0.166s

### [36] cb_during_rebalance — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000470, p95=0.000529, worst=0.000570
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.111s, p95=0.135s

### [37] cb_with_max_weight_concentration — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001087, p95=0.001111, worst=0.001122
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.136s, p95=0.160s

### [38] cb_with_min_weight_collateral — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000342, p95=0.000366, worst=0.000385
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.135s, p95=0.164s

### [39] cb_cooldown_boundary — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000367, p95=0.000392, worst=0.000419
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.162s, p95=0.181s

### [40] cb_hysteresis_edge_threshold — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000558, p95=0.000583, worst=0.000610
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.136s, p95=0.159s

### [41] gradient_explosion — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000338, p95=0.000359, worst=0.000371
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.168s

### [42] gradient_vanishing — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000357, worst=0.000369
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.146s, p95=0.169s

### [43] saddle_point_zero_grad — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000362, worst=0.000375
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.147s, p95=0.173s

### [44] oscillating_loss_direction — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000401, p95=0.000424, worst=0.000436
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.173s

### [45] adversarial_gradient_direction — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000339, p95=0.000361, worst=0.000367
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.169s

### [46] learning_rate_high_005 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000335, p95=0.000357, worst=0.000367
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.150s, p95=0.180s

### [47] learning_rate_low_0001 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000336, p95=0.000358, worst=0.000376
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.176s

### [48] weight_stuck_at_boundary — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000243, p95=0.000268, worst=0.000274
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.150s, p95=0.179s

### [49] simplex_projection_stress — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000242, p95=0.000263, worst=0.000269
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.147s, p95=0.166s

### [50] adam_momentum_trap_beta1_099 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000379, p95=0.000404, worst=0.000428
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.176s

### [51] supply_near_zero — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000338, p95=0.000356, worst=0.000373
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.174s

### [52] supply_huge_1b — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000336, p95=0.000359, worst=0.000367
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.172s

### [53] rapid_mint_0_to_1m — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000336, p95=0.000358, worst=0.000368
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.174s

### [54] rapid_redeem_1m_to_0 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000338, p95=0.000362, worst=0.000375
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.147s, p95=0.171s

### [55] oscillating_supply_every_tick — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000360, worst=0.000384
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.180s

### [56] one_sided_mints_200 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000336, p95=0.000353, worst=0.000375
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.181s, p95=0.229s

### [57] one_sided_redeems_200 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000358, worst=0.000374
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.173s, p95=0.200s

### [58] supply_spike_then_plateau — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000355, worst=0.000369
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.175s

### [59] supply_growth_1pct_per_tick — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000335, p95=0.000356, worst=0.000369
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.181s, p95=0.223s

### [60] supply_crash_90pct_3ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000360, worst=0.000377
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.147s, p95=0.170s

### [61] depeg_plus_oracle_failure — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001208, p95=0.001241, worst=0.001278
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.145s, p95=0.168s

### [62] high_vol_plus_low_liquidity — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000648, p95=0.000748, worst=0.000835
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.137s, p95=0.163s

### [63] flash_crash_cb_cascade_oracle_stale — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000936, p95=0.000974, worst=0.000982
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.144s, p95=0.169s

### [64] triple_depeg_plus_gradient_attack — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.002770, p95=0.002796, worst=0.002812
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.143s, p95=0.167s

### [65] max_concentration_then_depeg — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.002318, p95=0.002347, worst=0.002368
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.144s, p95=0.171s

### [66] all_mild_compound_stress — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001244, p95=0.001274, worst=0.001283
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.176s, p95=0.232s

### [67] recovery_from_max_stress — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.005401, p95=0.005422, worst=0.005438
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.159s, p95=0.184s

### [68] normal_crisis_oscillation_20ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.001605, p95=0.001682, worst=0.001725
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.153s, p95=0.182s

### [69] gradual_degradation_1000ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.003025, p95=0.003066, worst=0.003090
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.597s, p95=0.649s

### [70] sudden_improvement_after_500 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.003622, p95=0.003667, worst=0.003684
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.436s, p95=0.495s

### [71] perfect_peg_500ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000324, p95=0.000324, worst=0.000324
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.349s, p95=0.407s

### [72] equal_weights_025 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000351, p95=0.000373, worst=0.000387
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.175s

### [73] single_weight_at_max_cap — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000330, p95=0.000350, worst=0.000367
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.148s, p95=0.173s

### [74] cr_exactly_target — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000338, p95=0.000364, worst=0.000373
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.149s, p95=0.171s

### [75] fee_zero — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000334, p95=0.000359, worst=0.000372
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.152s, p95=0.180s

### [76] fee_max_10pct — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000337, p95=0.000362, worst=0.000376
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.153s, p95=0.177s

### [77] price_to_00001 — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.004611, p95=0.004634, worst=0.004646
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.132s, p95=0.157s

### [78] very_long_50000ticks — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000327, p95=0.000328, worst=0.000328
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=34.169s, p95=35.456s

### [79] oracle_confidence_0_1_alternating — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.000828, p95=0.000851, worst=0.000861
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.169s, p95=0.211s

### [80] all_params_extreme_boundary — PASS
- runs=100 ok=100 crashed=0 nan_inf=0 failure_rate=0.00%
- MAE mean=0.026840, p95=0.026862, worst=0.026865
- CR violation mean=0.000000, p95=0.000000, worst=0.000000
- Runtime mean=0.169s, p95=0.216s
