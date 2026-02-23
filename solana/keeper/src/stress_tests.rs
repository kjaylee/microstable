#![cfg(test)]

use crate::{
    aig::{
        evaluate_challenge_result, evaluate_challenge_result_for_tier, run_sandbox_trial,
        MAX_AIG_SCORE,
    },
    config::KeeperConfig,
    optimizer::{
        optimize_step, AdamOptimizer, LossFunction, ParamVector, ProtocolSnapshot, SafetyBounds,
    },
    oracle::{
        is_allowed_pyth_write_authority, validate_oracle_cross_rpc,
        validate_oracle_observation_consistency, OracleObservation,
    },
    rebalance::validate_rebalance_cross_rpc,
    risk_manager::{
        assess_risk_level, auto_recovery_step, redemption_queue_policy,
        should_throttle_redemptions, RecoveryAction, RiskLevel,
    },
    tournament::{create_tournament, evaluate_proposals, submit_proposal, AgentProposal},
    utils::{
        adaptive_secondary_confirm_window_secs, assess_tx_confirmation_outcome, load_keypairs,
        retry_with_backoff, SecondaryRpcMode, CROSS_RPC_MAX_ATTEMPTS, TX_CONFIRM_WINDOW_BASE_SECS,
        TX_CONFIRM_WINDOW_MAX_SECS,
    },
    wire,
};
use solana_sdk::pubkey::Pubkey;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn baseline_snapshot() -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 1.0,
        collateral_ratio: 1.2,
        nav_history: vec![1.0, 1.0, 1.0],
        current_weights: [0.25, 0.25, 0.25, 0.25],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: [1.0, 1.0, 1.0, 1.0],
        target_cr: 1.2,
        mint_fee: 0.001,
        redeem_fee: 0.001,
        loss_function: Some(LossFunction::default()),
    }
}

fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}_{nanos}.{extension}"))
}

fn write_temp(path: &Path, content: &str) {
    fs::write(path, content).expect("failed to write temp file")
}

fn sample_protocol_state() -> wire::ProtocolState {
    wire::ProtocolState {
        weights: [250_000; 4],
        fee_rate: 2_000,
        mint_fee_rate: 2_000,
        redeem_fee_rate: 2_000,
        cr_target: 1_200_000,
        total_supply: 1_000_000,
        last_update_slot: 42,
        keeper_set: [
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        ],
        emergency_shutdown: false,
        pending_rebalance_commit: [0u8; 32],
        pending_rebalance_slot: 0,
        pending_rebalance_expiry: 0,
        pending_keeper_set: [[0u8; 32]; 3],
        pending_keeper_activation_slot: 0,
        flow_control_slot: 42,
        minted_in_flow_slot: 0,
        redeemed_in_flow_slot: 0,
        max_mint_per_slot_ppm: 120_000,
        max_redeem_per_slot_ppm: 80_000,
        manual_oracle_mode_expiry_slot: 0,
        bump: 1,
    }
}

fn sample_vault(index: u8) -> wire::CollateralVault {
    wire::CollateralVault {
        index,
        mint: Pubkey::new_unique(),
        vault: Pubkey::new_unique(),
        oracle: Pubkey::new_unique(),
        risk_score: 1,
        weight_cap: 250_000,
        base_weight_cap: 250_000,
        price: 1_000_000,
        confidence: 100,
        last_oracle_slot: 100,
        total_deposits: 1_000_000,
        bump: 1,
        pyth_price_feed: Pubkey::new_unique(),
    }
}

fn sample_vaults() -> [wire::CollateralVault; 4] {
    [
        sample_vault(0),
        sample_vault(1),
        sample_vault(2),
        sample_vault(3),
    ]
}

fn sample_circuit_breaker(optimizer_enabled: bool) -> wire::CircuitBreakerState {
    wire::CircuitBreakerState {
        status: [0; 4],
        activation_tick: [0; 4],
        trigger_count: [0; 4],
        cooldown_until: [0; 4],
        last_trigger_tick: [0; 4],
        recent_trigger_count: [0; 4],
        recovery_tick: [0; 4],
        cb1_collateral_index: 0,
        mint_rate_limit: 0,
        optimizer_enabled,
        learning_rate_scale: 1_000_000,
        max_activation_duration: 100,
        bump: 1,
    }
}

fn minimal_valid_config_json() -> String {
    r#"{
  "rpc_url": "https://api.devnet.solana.com",
  "secondary_rpc_url": "https://rpc.ankr.com/solana_devnet",
  "program_id": "BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3",
  "keeper_keypairs": ["keeper/k1.json", "keeper/k2.json"],
  "pyth_feeds": [
    {
      "symbol": "USDC/USD",
      "collateral_index": 0,
      "price_account": "Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX"
    },
    {
      "symbol": "USDT/USD",
      "collateral_index": 1,
      "price_account": "HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM"
    },
    {
      "symbol": "DAI/USD",
      "collateral_index": 2,
      "price_account": "FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN"
    },
    {
      "symbol": "USDS/USD",
      "collateral_index": 3,
      "price_account": "9h4r3d4s8Jc8k5YfVY6Bnd3ETf6gVfGvSzj8Pzpo7aQw"
    }
  ],
  "tick_interval_secs": 30,
  "oracle_max_age_secs": 120,
  "min_collateral_ratio_bps": 10500,
  "rebalance_deviation_bps": 300,
  "max_rebalance_slippage_bps": 200,
  "commit_valid_for_slots": 200,
  "auto_emergency_shutdown": false,
  "send_watchdog_alert_tx": false,
  "execute_rebalance_immediately": false,
  "watchdog_supply_spike_bps": 2500,
  "watchdog_cr_drop_bps": 1500
}"#
    .to_string()
}

// ST-1: Numerical Edge Cases (optimizer.rs)

#[test]
fn st_1_1_all_zero_weights_no_division_by_zero() {
    let mut snapshot = baseline_snapshot();
    snapshot.current_weights = [0.0; 4];
    let out = LossFunction::default().compute(&snapshot).unwrap();
    assert!(out.total_loss.is_finite());
}

#[test]
fn st_1_2_denormal_weights_are_stable() {
    let mut snapshot = baseline_snapshot();
    snapshot.current_weights = [1e-300; 4];
    snapshot.previous_weights = [1e-300; 4];
    let out = LossFunction::default().compute(&snapshot).unwrap();
    assert!(out.total_loss.is_finite());
}

#[test]
fn st_1_3_nan_loss_input_is_rejected_without_propagation() {
    let mut snapshot = baseline_snapshot();
    snapshot.peg_price = f64::NAN;
    assert!(LossFunction::default().compute(&snapshot).is_err());
}

#[test]
fn st_1_4_infinite_loss_input_is_rejected_without_panic() {
    let mut snapshot = baseline_snapshot();
    snapshot.collateral_ratio = f64::INFINITY;
    assert!(LossFunction::default().compute(&snapshot).is_err());
}

#[test]
fn st_1_5_gradient_explosion_is_clipped() {
    let optimizer = AdamOptimizer {
        max_grad_norm: 1.0,
        ..AdamOptimizer::default()
    };

    let huge = ParamVector {
        weights: [1e12, 1e12, 1e12, 1e12],
        target_cr: 1e12,
        mint_fee: 1e12,
        redeem_fee: 1e12,
    };

    let clipped = optimizer.clip_gradients(&huge);
    assert!(clipped.l2_norm() <= 1.0 + 1e-9);
}

#[test]
fn st_1_6_adam_beta_extremes_remain_finite() {
    let start = ParamVector {
        weights: [0.25, 0.25, 0.25, 0.25],
        target_cr: 2.0,
        mint_fee: 0.01,
        redeem_fee: 0.01,
    };
    let gradient = ParamVector {
        weights: [0.0; 4],
        target_cr: 1.0,
        mint_fee: 1.0,
        redeem_fee: 1.0,
    };

    let mut beta_zero = AdamOptimizer {
        beta1: 0.0,
        beta2: 0.0,
        warmup_steps: 0,
        decay_steps: 0,
        min_learning_rate: 0.01,
        learning_rate: 0.01,
        ..AdamOptimizer::default()
    };
    let out_zero = beta_zero.step(&start, &gradient);
    assert!(out_zero.is_finite());

    let mut beta_one = AdamOptimizer {
        beta1: 1.0,
        beta2: 1.0,
        warmup_steps: 0,
        decay_steps: 0,
        min_learning_rate: 0.01,
        learning_rate: 0.01,
        ..AdamOptimizer::default()
    };
    let out_one = beta_one.step(&start, &gradient);
    assert!(out_one.is_finite());
}

#[test]
fn st_1_7_ten_thousand_consecutive_steps_stay_finite() {
    let mut params = ParamVector::default();
    let mut optimizer = AdamOptimizer {
        warmup_steps: 0,
        decay_steps: 0,
        learning_rate: 0.001,
        min_learning_rate: 0.001,
        ..AdamOptimizer::default()
    };

    let start = Instant::now();
    for _ in 0..10_000 {
        let grad = ParamVector {
            weights: [0.0; 4],
            target_cr: params.target_cr - 1.2,
            mint_fee: params.mint_fee,
            redeem_fee: params.redeem_fee,
        };
        params = optimizer.step(&params, &grad);
        assert!(params.is_finite());
    }

    assert_eq!(optimizer.state.t, 10_000);
    assert!(start.elapsed().as_secs() < 10);
}

#[test]
fn st_1_8_target_cr_zero_is_projected_to_safety_bounds() {
    let params = ParamVector {
        target_cr: 0.0,
        ..ParamVector::default()
    };

    let bounds = SafetyBounds {
        cr_min: 1.0,
        cr_max: 2.0,
        ..SafetyBounds::default()
    };

    let projected = crate::optimizer::project_to_safety_set(&params, &bounds);
    assert!(projected.target_cr >= 1.0);
}

#[test]
fn st_1_9_hundred_percent_fees_are_clamped_to_bounds() {
    let params = ParamVector {
        mint_fee: 1.0,
        redeem_fee: 1.0,
        ..ParamVector::default()
    };

    let bounds = SafetyBounds {
        fee_min: 0.0,
        fee_max: 0.05,
        ..SafetyBounds::default()
    };

    let projected = crate::optimizer::project_to_safety_set(&params, &bounds);
    assert!(projected.mint_fee <= 0.05 + 1e-12);
    assert!(projected.redeem_fee <= 0.05 + 1e-12);
}

#[test]
fn st_1_10_zero_oracle_quality_scores_loss_is_finite() {
    let mut snapshot = baseline_snapshot();
    snapshot.oracle_quality_scores = [0.0; 4];
    let out = LossFunction::default().compute(&snapshot).unwrap();
    assert!(out.total_loss.is_finite());
    assert!(out.terms.oracle_quality.is_finite());
}

// ST-2: Oracle Extreme Conditions

#[test]
fn st_2_1_hundred_stale_like_cycles_do_not_loop_forever() {
    let obs = OracleObservation {
        price: 1_000_000,
        confidence: 100,
        publish_time: 1,
        observed_slot: 1,
    };

    for _ in 0..100 {
        validate_oracle_observation_consistency(&obs, &obs).unwrap();
    }
}

#[test]
fn st_2_2_zero_price_observation_does_not_panic() {
    let obs = OracleObservation {
        price: 0,
        confidence: 0,
        publish_time: 0,
        observed_slot: 0,
    };
    validate_oracle_observation_consistency(&obs, &obs).unwrap();
}

#[test]
fn st_2_3_negative_price_rejection_guard_exists() {
    let source = include_str!("oracle.rs");
    assert!(source.contains("if update.price_message.price <= 0"));
}

#[test]
fn st_2_4_max_confidence_is_guarded_by_confidence_bps_check() {
    let source = include_str!("oracle.rs");
    assert!(source.contains("confidence_bps > cfg.oracle_confidence_max_bps"));
}

#[test]
fn st_2_5_future_publish_time_rejection_guard_exists() {
    let source = include_str!("oracle.rs");
    assert!(source.contains("if publish_time > now_unix_ts"));
}

#[test]
fn st_2_6_three_feeds_stale_path_is_graceful_continue() {
    let source = include_str!("oracle.rs");
    assert!(source.contains("oracle update skipped: stale publish time"));
    assert!(source.contains("status = \"unconfigured\""));
    assert!(source.contains("continue;"));
}

#[test]
fn st_2_cross_rpc_mismatch_is_rejected_without_panic() {
    let mut p = sample_protocol_state();
    let mut s = p.clone();
    s.total_supply = s.total_supply.saturating_add(10);

    let vaults = sample_vaults();
    let err = validate_oracle_cross_rpc(&p, &s, &vaults, &vaults).unwrap_err();
    assert!(err.to_string().contains("mismatch"));

    p.total_supply = p.total_supply.saturating_add(1);
}

// ST-3: AIG Extreme

#[test]
fn st_3_1_zero_epoch_challenge_returns_invalid_loss_and_zero_score() {
    let scenario = baseline_snapshot();
    let params = ParamVector::default();
    let loss = run_sandbox_trial(&params, &scenario, 0);
    assert!(loss >= 1e12);

    let result = evaluate_challenge_result(loss, 10.0);
    assert_eq!(result.score, 0);
}

#[test]
fn st_3_2_thousand_epoch_challenge_remains_finite() {
    let scenario = baseline_snapshot();
    let params = ParamVector::default();

    let start = Instant::now();
    let loss = run_sandbox_trial(&params, &scenario, 1000);
    assert!(loss.is_finite());
    assert!(start.elapsed().as_secs() < 10);
}

#[test]
fn st_3_3_baseline_loss_zero_yields_zero_score() {
    let result = evaluate_challenge_result(1.0, 0.0);
    assert_eq!(result.score, 0);
    assert!(!result.passed);
}

#[test]
fn st_3_4_trial_equal_to_baseline_is_boundary_case() {
    let result = evaluate_challenge_result_for_tier(10.0, 10.0, 1);
    assert_eq!(result.score, 0);
    assert!(!result.passed);
}

#[test]
fn st_3_5_score_is_clamped_to_max_aig_score() {
    let result = evaluate_challenge_result_for_tier(-100.0, 10.0, 1);
    assert_eq!(result.score, MAX_AIG_SCORE);
}

// ST-4: Tournament Extreme

#[test]
fn st_4_1_zero_proposals_returns_empty_result() {
    let tournament = create_tournament(baseline_snapshot(), 1, 1);
    let result = evaluate_proposals(&tournament);
    assert_eq!(result.participants, 0);
    assert!(result.winner.is_none());
}

#[test]
fn st_4_2_hundred_proposals_evaluate_without_panic() {
    let mut tournament = create_tournament(baseline_snapshot(), 2, 1);

    for i in 0..100u64 {
        let params = ParamVector {
            target_cr: 1.0 + (i as f64 % 20.0) * 0.01,
            ..ParamVector::default()
        };
        submit_proposal(&mut tournament, Pubkey::new_unique(), params, 1).unwrap();
    }

    let start = Instant::now();
    let result = evaluate_proposals(&tournament);
    assert_eq!(result.participants, 100);
    assert!(result.winner.is_some());
    assert!(start.elapsed().as_secs() < 5);
}

#[test]
fn st_4_3_identical_losses_use_tiebreaker() {
    let mut tournament = create_tournament(baseline_snapshot(), 3, 1);
    let first = Pubkey::new_unique();
    let second = Pubkey::new_unique();

    let params = ParamVector::default();
    submit_proposal(&mut tournament, first, params, 1).unwrap();
    submit_proposal(&mut tournament, second, params, 1).unwrap();

    tournament.proposals[0].submitted_at = 2_000;
    tournament.proposals[1].submitted_at = 1_000;

    let result = evaluate_proposals(&tournament);
    assert_eq!(result.winner, Some(second));
}

#[test]
fn st_4_4_nan_loss_proposal_is_isolated() {
    let mut tournament = create_tournament(baseline_snapshot(), 4, 1);
    let good = Pubkey::new_unique();
    let bad = Pubkey::new_unique();

    submit_proposal(&mut tournament, good, ParamVector::default(), 1).unwrap();
    tournament.proposals.push(AgentProposal {
        agent: bad,
        params: ParamVector {
            weights: [f64::NAN, 0.25, 0.25, 0.25],
            ..ParamVector::default()
        },
        loss: f64::NAN,
        submitted_at: 0,
    });

    let result = evaluate_proposals(&tournament);
    assert_eq!(result.winner, Some(good));
    assert_eq!(result.participants, 2);
}

#[test]
fn st_4_5_duplicate_agent_proposals_are_handled_deterministically() {
    let mut tournament = create_tournament(baseline_snapshot(), 5, 1);
    let same_agent = Pubkey::new_unique();

    submit_proposal(
        &mut tournament,
        same_agent,
        ParamVector {
            target_cr: 1.2,
            ..ParamVector::default()
        },
        1,
    )
    .unwrap();
    submit_proposal(
        &mut tournament,
        same_agent,
        ParamVector {
            target_cr: 1.3,
            ..ParamVector::default()
        },
        1,
    )
    .unwrap();

    let result = evaluate_proposals(&tournament);
    assert_eq!(result.participants, 2);
    assert_eq!(result.score_adjustments.len(), 2);
}

// ST-5: Risk Manager Boundaries

#[test]
fn st_5_1_cr_zero_is_critical_without_panic() {
    assert_eq!(assess_risk_level(0.0, 1.2), RiskLevel::Critical);
}

#[test]
fn st_5_2_extreme_cr_input_no_overflow() {
    let huge_ratio = (u64::MAX as f64) / 10_000.0;
    let level = assess_risk_level(huge_ratio, 1.2);
    assert_eq!(level, RiskLevel::Normal);
}

#[test]
fn st_5_3_max_consecutive_failed_cycles_limit_guarded() {
    let source = include_str!("main.rs");
    assert!(source.contains("consecutive_failed_cycles >= cfg.max_consecutive_failed_cycles"));

    let mut invalid = minimal_valid_config_json();
    invalid = invalid.replace(
        "\"watchdog_cr_drop_bps\": 1500",
        "\"watchdog_cr_drop_bps\": 1500,\n  \"max_consecutive_failed_cycles\": 101",
    );
    let path = unique_temp_path("keeper_invalid_max_failed_cycles", "json");
    write_temp(&path, &invalid);
    let err = KeeperConfig::load(Some(path.as_path())).unwrap_err();
    assert!(err.to_string().contains("max_consecutive_failed_cycles"));
    let _ = fs::remove_file(path);
}

#[test]
fn st_5_4_all_anomaly_style_signals_keep_conservative_policy() {
    let recovery = auto_recovery_step(RiskLevel::Critical, RiskLevel::High, 100);
    let throttle = should_throttle_redemptions(RiskLevel::Critical, u64::MAX);
    let policy = redemption_queue_policy(RiskLevel::Critical);

    assert_eq!(recovery, RecoveryAction::HoldConservative);
    assert!(throttle);
    assert!(policy.enabled);
}

#[test]
fn st_rebalance_cross_rpc_rejects_invalid_weight_caps() {
    let protocol = sample_protocol_state();
    let circuit = sample_circuit_breaker(true);
    let mut bad_vaults = sample_vaults();
    bad_vaults[0].weight_cap = 0;

    let err = validate_rebalance_cross_rpc(
        &protocol,
        &protocol,
        &circuit,
        &circuit,
        &bad_vaults,
        &bad_vaults,
    )
    .unwrap_err();

    assert!(err.to_string().contains("weight_cap out of range"));
}

#[test]
fn st_rebalance_cross_rpc_rejects_circuit_optimizer_flag_mismatch() {
    let protocol = sample_protocol_state();
    let vaults = sample_vaults();
    let primary = sample_circuit_breaker(true);
    let secondary = sample_circuit_breaker(false);

    let err =
        validate_rebalance_cross_rpc(&protocol, &protocol, &primary, &secondary, &vaults, &vaults)
            .unwrap_err();

    assert!(err.to_string().contains("optimizer_enabled mismatch"));
}

// ST-6: Config Edge Cases

#[test]
fn st_6_1_empty_config_json_returns_parse_error() {
    let path = unique_temp_path("keeper_empty_config", "json");
    write_temp(path.as_path(), "{}");

    let err = KeeperConfig::load(Some(path.as_path())).unwrap_err();
    assert!(err.to_string().contains("failed to parse config"));

    let _ = fs::remove_file(path);
}

#[test]
fn st_6_2_missing_optional_fields_use_defaults() {
    let path = unique_temp_path("keeper_minimal_config", "json");
    write_temp(path.as_path(), &minimal_valid_config_json());

    let cfg = KeeperConfig::load(Some(path.as_path())).unwrap();
    assert_eq!(cfg.oracle_publish_max_age_secs, 60);
    assert_eq!(cfg.oracle_confidence_max_bps, 500);
    assert_eq!(cfg.commit_reveal_delay_slots, 5);
    assert_eq!(cfg.max_consecutive_failed_cycles, 5);

    let _ = fs::remove_file(path);
}

#[test]
fn st_6_3_tick_interval_zero_is_rejected() {
    let path = unique_temp_path("keeper_tick_zero", "json");
    let body = minimal_valid_config_json()
        .replace("\"tick_interval_secs\": 30", "\"tick_interval_secs\": 0");
    write_temp(path.as_path(), &body);

    let err = KeeperConfig::load(Some(path.as_path())).unwrap_err();
    assert!(err.to_string().contains("tick_interval_secs"));

    let _ = fs::remove_file(path);
}

#[test]
fn st_6_4_tick_interval_u64_max_is_rejected() {
    let path = unique_temp_path("keeper_tick_max", "json");
    let body = minimal_valid_config_json().replace(
        "\"tick_interval_secs\": 30",
        &format!("\"tick_interval_secs\": {}", u64::MAX),
    );
    write_temp(path.as_path(), &body);

    let err = KeeperConfig::load(Some(path.as_path())).unwrap_err();
    assert!(err.to_string().contains("tick_interval_secs"));

    let _ = fs::remove_file(path);
}

#[test]
fn st_6_5_missing_keypair_file_has_clear_error() {
    let missing = unique_temp_path("keeper_missing_keypair", "json");
    let err = load_keypairs(&[missing.clone()]).unwrap_err();
    assert!(err.to_string().contains("failed to open keypair file"));
}

#[test]
fn st_6_6_invalid_keypair_format_has_clear_error() {
    let path = unique_temp_path("keeper_bad_keypair", "json");
    write_temp(path.as_path(), "[1,2,3]");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path.as_path()).unwrap().permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path.as_path(), perms).unwrap();
    }

    let err = load_keypairs(&[path.clone()]).unwrap_err();
    assert!(err.to_string().contains("failed to read keypair"));

    let _ = fs::remove_file(path);
}

#[test]
fn st_6_7_invalid_program_id_format_is_rejected() {
    let path = unique_temp_path("keeper_bad_program_id", "json");
    let body = minimal_valid_config_json().replace(
        "\"BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3\"",
        "\"not-a-valid-pubkey\"",
    );
    write_temp(path.as_path(), &body);

    let err = KeeperConfig::load(Some(path.as_path())).unwrap_err();
    assert!(err.to_string().contains("invalid program id"));

    let _ = fs::remove_file(path);
}

// ST-7: Concurrent/Timing

#[test]
fn st_7_1_sigterm_path_exists_for_graceful_shutdown() {
    let source = include_str!("main.rs");
    assert!(source.contains("tokio::signal::unix::SignalKind::terminate()"));
    assert!(source.contains("received SIGTERM, shutting down"));
}

#[test]
fn st_7_2_rpc_retry_logic_retries_then_succeeds() {
    let mut attempts = 0usize;
    let value = retry_with_backoff(CROSS_RPC_MAX_ATTEMPTS, 0, |_| {
        attempts += 1;
        if attempts < 3 {
            Err(anyhow::anyhow!("temporary RPC timeout"))
        } else {
            Ok(7u64)
        }
    })
    .unwrap();

    assert_eq!(value, 7);
    assert_eq!(attempts, 3);
}

#[test]
fn st_7_3_primary_and_secondary_failures_are_reported() {
    let normal_err =
        assess_tx_confirmation_outcome(false, false, SecondaryRpcMode::Normal, true).unwrap_err();
    assert!(normal_err
        .to_string()
        .contains("did not reach dual-RPC confirmation"));

    let degraded_err =
        assess_tx_confirmation_outcome(false, false, SecondaryRpcMode::Degraded, true).unwrap_err();
    assert!(degraded_err
        .to_string()
        .contains("not confirmed while running in degraded mode"));
}

#[test]
fn st_7_4_long_confirmation_window_paths_are_present() {
    assert_eq!(TX_CONFIRM_WINDOW_BASE_SECS, 30);
    assert_eq!(TX_CONFIRM_WINDOW_MAX_SECS, 60);
    assert_eq!(
        adaptive_secondary_confirm_window_secs(true, false),
        TX_CONFIRM_WINDOW_MAX_SECS
    );
    assert_eq!(
        adaptive_secondary_confirm_window_secs(true, true),
        TX_CONFIRM_WINDOW_BASE_SECS
    );
}

#[test]
fn st_1_rollback_on_non_finite_snapshot_returns_last_checkpoint() {
    let mut snapshot = baseline_snapshot();
    let mut params = ParamVector::default();
    let mut optimizer = AdamOptimizer::default();
    let bounds = SafetyBounds::default();
    let mut checkpoint = None;

    params = optimize_step(&snapshot, &params, &mut optimizer, &bounds, &mut checkpoint).unwrap();

    snapshot.peg_price = f64::NAN;
    let rolled = optimize_step(&snapshot, &params, &mut optimizer, &bounds, &mut checkpoint)
        .expect("should rollback to checkpoint");

    assert_eq!(rolled, params);
}

#[test]
fn st_2_write_authority_allowlist_works_for_self_or_trusted() {
    let pyth_account = Pubkey::new_unique();
    let trusted = Pubkey::new_unique();
    let other = Pubkey::new_unique();

    assert!(is_allowed_pyth_write_authority(
        pyth_account,
        pyth_account,
        trusted
    ));
    assert!(is_allowed_pyth_write_authority(
        trusted,
        pyth_account,
        trusted
    ));
    assert!(!is_allowed_pyth_write_authority(
        other,
        pyth_account,
        trusted
    ));
}
