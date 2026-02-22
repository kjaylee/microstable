#![cfg(test)]

use crate::{
    aig::{
        aggregate_scores, evaluate_challenge_result, generate_challenges, run_sandbox_trial,
        tier_promotion_threshold, ChallengeKind, ChallengeResult,
    },
    optimizer::{ParamVector, ProtocolSnapshot},
};

fn normal_snapshot() -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 1.0,
        collateral_ratio: 1.25,
        nav_history: vec![1.0, 1.0, 1.0, 1.0],
        current_weights: [0.25, 0.25, 0.25, 0.25],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: [1.0, 1.0, 1.0, 1.0],
        target_cr: 1.2,
        mint_fee: 0.001,
        redeem_fee: 0.001,
        loss_function: None,
    }
}

#[test]
fn tc_aig_01_generate_challenges_0_to_1_returns_three_peg_stability() {
    let challenges = generate_challenges(0, 1);
    assert_eq!(challenges.len(), 3);
    assert!(challenges
        .iter()
        .all(|c| matches!(c.kind, ChallengeKind::PegStability)));
}

#[test]
fn tc_aig_02_generate_challenges_1_to_2_returns_three_stress_two_optimization() {
    let challenges = generate_challenges(1, 2);
    assert_eq!(challenges.len(), 5);

    let stress = challenges
        .iter()
        .filter(|c| matches!(c.kind, ChallengeKind::StressTest))
        .count();
    let optimization = challenges
        .iter()
        .filter(|c| matches!(c.kind, ChallengeKind::Optimization))
        .count();

    assert_eq!(stress, 3);
    assert_eq!(optimization, 2);
}

#[test]
fn tc_aig_03_generate_challenges_2_to_3_returns_five_adversarial_three_optimization() {
    let challenges = generate_challenges(2, 3);
    assert_eq!(challenges.len(), 8);

    let adversarial = challenges
        .iter()
        .filter(|c| matches!(c.kind, ChallengeKind::Adversarial))
        .count();
    let optimization = challenges
        .iter()
        .filter(|c| matches!(c.kind, ChallengeKind::Optimization))
        .count();

    assert_eq!(adversarial, 5);
    assert_eq!(optimization, 3);
}

#[test]
fn tc_aig_04_run_sandbox_trial_optimal_params_score_above_800k() {
    let scenario = normal_snapshot();
    let params = ParamVector {
        weights: [0.25, 0.25, 0.25, 0.25],
        target_cr: 1.15,
        mint_fee: 0.001,
        redeem_fee: 0.001,
    };

    let loss = run_sandbox_trial(&params, &scenario, 12);
    let result = evaluate_challenge_result(loss, 10.0);

    assert!(result.score > 800_000, "score was {}", result.score);
}

#[test]
fn tc_aig_05_run_sandbox_trial_terrible_params_score_below_200k() {
    let scenario = normal_snapshot();
    let params = ParamVector {
        weights: [1.0, 0.0, 0.0, 0.0],
        target_cr: 3.0,
        mint_fee: 0.05,
        redeem_fee: 0.0,
    };

    let loss = run_sandbox_trial(&params, &scenario, 12);
    let result = evaluate_challenge_result(loss, 10.0);

    assert!(result.score < 200_000, "score was {}", result.score);
}

#[test]
fn tc_aig_06_run_sandbox_trial_nan_params_score_zero() {
    let scenario = normal_snapshot();
    let params = ParamVector {
        weights: [f64::NAN, 0.25, 0.25, 0.25],
        target_cr: 1.2,
        mint_fee: 0.001,
        redeem_fee: 0.001,
    };

    let loss = run_sandbox_trial(&params, &scenario, 12);
    let result = evaluate_challenge_result(loss, 10.0);

    assert_eq!(result.score, 0);
}

#[test]
fn tc_aig_07_evaluate_challenge_result_maps_loss_to_score() {
    let low_loss = evaluate_challenge_result(2.0, 10.0);
    let high_loss = evaluate_challenge_result(8.0, 10.0);

    assert!(low_loss.score > high_loss.score);
    assert_eq!(low_loss.score, 800_000);
    assert_eq!(high_loss.score, 200_000);
}

#[test]
fn tc_aig_08_tier_promotion_threshold_values() {
    assert_eq!(tier_promotion_threshold(1), 600_000);
    assert_eq!(tier_promotion_threshold(2), 750_000);
    assert_eq!(tier_promotion_threshold(3), 850_000);
}

#[test]
fn tc_aig_09_aggregate_scores_averages_results() {
    let results = vec![
        ChallengeResult {
            score: 900_000,
            loss: 1.0,
            passed: true,
        },
        ChallengeResult {
            score: 600_000,
            loss: 4.0,
            passed: true,
        },
        ChallengeResult {
            score: 300_000,
            loss: 7.0,
            passed: false,
        },
    ];

    assert_eq!(aggregate_scores(&results), 600_000);
}

#[test]
fn tc_aig_10_stress_oracle_failure_non_panic_valid_result() {
    let mut scenario = normal_snapshot();
    scenario.peg_price = 0.88;
    scenario.collateral_ratio = 0.95;
    scenario.nav_history = vec![1.0, 0.9, 0.8, 0.85, 0.78, 0.82];
    scenario.oracle_quality_scores = [0.0, 0.0, 0.0, 0.0];

    let params = ParamVector {
        weights: [0.4, 0.3, 0.2, 0.1],
        target_cr: 1.3,
        mint_fee: 0.004,
        redeem_fee: 0.002,
    };

    let loss = run_sandbox_trial(&params, &scenario, 12);
    assert!(loss.is_finite(), "loss must be finite, got {loss}");
}
