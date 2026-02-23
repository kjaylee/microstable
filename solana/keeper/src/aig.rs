use crate::optimizer::{self, LossFunction, ParamVector, ProtocolSnapshot};

pub const MAX_AIG_SCORE: u64 = 1_000_000;
pub const TIER1_PROMOTION_THRESHOLD: u64 = 600_000;
pub const TIER2_PROMOTION_THRESHOLD: u64 = 750_000;
pub const TIER3_PROMOTION_THRESHOLD: u64 = 850_000;

const INVALID_TRIAL_LOSS: f64 = 1_000_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeKind {
    PegStability,
    StressTest,
    Optimization,
    Adversarial,
}

#[derive(Debug, Clone)]
pub struct AigChallenge {
    pub kind: ChallengeKind,
    pub scenario: optimizer::ProtocolSnapshot,
    pub epochs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChallengeResult {
    pub score: u64,
    pub loss: f64,
    pub passed: bool,
}

pub fn generate_challenges(current_tier: u8, target_tier: u8) -> Vec<AigChallenge> {
    match (current_tier, target_tier) {
        (0, 1) => (0..3)
            .map(|i| AigChallenge {
                kind: ChallengeKind::PegStability,
                scenario: peg_stability_scenario(i),
                epochs: 12,
            })
            .collect(),
        (1, 2) => {
            let mut challenges = Vec::with_capacity(5);
            for i in 0..3 {
                challenges.push(AigChallenge {
                    kind: ChallengeKind::StressTest,
                    scenario: stress_scenario(i),
                    epochs: 16,
                });
            }
            for i in 0..2 {
                challenges.push(AigChallenge {
                    kind: ChallengeKind::Optimization,
                    scenario: optimization_scenario(i),
                    epochs: 14,
                });
            }
            challenges
        }
        (2, 3) => {
            let mut challenges = Vec::with_capacity(8);
            for i in 0..5 {
                challenges.push(AigChallenge {
                    kind: ChallengeKind::Adversarial,
                    scenario: adversarial_scenario(i),
                    epochs: 20,
                });
            }
            for i in 0..3 {
                challenges.push(AigChallenge {
                    kind: ChallengeKind::Optimization,
                    scenario: optimization_scenario(i + 2),
                    epochs: 16,
                });
            }
            challenges
        }
        _ => Vec::new(),
    }
}

pub fn run_sandbox_trial(params: &ParamVector, scenario: &ProtocolSnapshot, epochs: usize) -> f64 {
    if !params.is_finite() || epochs == 0 {
        return INVALID_TRIAL_LOSS;
    }

    let mut state = scenario.clone();
    if state.validate().is_err() {
        return INVALID_TRIAL_LOSS;
    }

    let loss_fn = state.loss_function.unwrap_or_else(LossFunction::default);
    let mut cumulative_loss = 0.0;

    for epoch in 0..epochs {
        state.current_weights = params.weights;
        state.target_cr = params.target_cr;
        state.mint_fee = params.mint_fee;
        state.redeem_fee = params.redeem_fee;

        let step_loss = match loss_fn.compute(&state) {
            Ok(v) if v.total_loss.is_finite() => v.total_loss.max(0.0),
            _ => return INVALID_TRIAL_LOSS,
        };

        cumulative_loss += step_loss;
        if !cumulative_loss.is_finite() {
            return INVALID_TRIAL_LOSS;
        }

        state = evolve_state(&state, epoch);
    }

    cumulative_loss
}

#[cfg(test)]
pub fn evaluate_challenge_result(loss: f64, baseline_loss: f64) -> ChallengeResult {
    evaluate_challenge_result_for_tier(loss, baseline_loss, 1)
}

pub fn evaluate_challenge_result_for_tier(
    loss: f64,
    baseline_loss: f64,
    target_tier: u8,
) -> ChallengeResult {
    let score = if !loss.is_finite() || !baseline_loss.is_finite() || baseline_loss <= 0.0 {
        0
    } else {
        let normalized = 1.0 - (loss / baseline_loss);
        (MAX_AIG_SCORE as f64 * normalized)
            .round()
            .max(0.0)
            .min(MAX_AIG_SCORE as f64) as u64
    };

    ChallengeResult {
        score,
        loss,
        passed: score >= tier_promotion_threshold(target_tier),
    }
}

pub fn tier_promotion_threshold(tier: u8) -> u64 {
    match tier {
        1 => TIER1_PROMOTION_THRESHOLD,
        2 => TIER2_PROMOTION_THRESHOLD,
        3 => TIER3_PROMOTION_THRESHOLD,
        _ => MAX_AIG_SCORE + 1,
    }
}

pub fn aggregate_scores(results: &[ChallengeResult]) -> u64 {
    if results.is_empty() {
        return 0;
    }

    let total: u128 = results.iter().map(|r| r.score as u128).sum();
    (total / results.len() as u128) as u64
}

fn peg_stability_scenario(seed: usize) -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 1.0 + seed as f64 * 0.002,
        collateral_ratio: 1.25,
        nav_history: vec![1.0, 1.001, 0.999, 1.0],
        current_weights: [0.25, 0.25, 0.25, 0.25],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: [0.98, 0.97, 0.99, 0.98],
        target_cr: 1.2,
        mint_fee: 0.001,
        redeem_fee: 0.001,
        loss_function: Some(LossFunction::default()),
    }
}

fn stress_scenario(seed: usize) -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 0.94 - seed as f64 * 0.01,
        collateral_ratio: 1.02 - seed as f64 * 0.02,
        nav_history: vec![1.0, 0.93, 0.9, 0.86, 0.84],
        current_weights: [0.35, 0.30, 0.20, 0.15],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: [0.75, 0.62, 0.55, 0.48],
        target_cr: 1.35,
        mint_fee: 0.004,
        redeem_fee: 0.002,
        loss_function: Some(LossFunction::default()),
    }
}

fn optimization_scenario(seed: usize) -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 0.98 - seed as f64 * 0.004,
        collateral_ratio: 1.16,
        nav_history: vec![1.0, 1.02, 0.98, 1.01, 0.99],
        current_weights: [0.40, 0.30, 0.20, 0.10],
        previous_weights: [0.34, 0.26, 0.24, 0.16],
        oracle_quality_scores: [0.90, 0.84, 0.78, 0.72],
        target_cr: 1.24,
        mint_fee: 0.003,
        redeem_fee: 0.001,
        loss_function: Some(LossFunction::default()),
    }
}

fn adversarial_scenario(seed: usize) -> ProtocolSnapshot {
    let oracle = if seed % 2 == 0 {
        [0.55, 0.4, 0.35, 0.2]
    } else {
        [0.45, 0.3, 0.25, 0.15]
    };

    ProtocolSnapshot {
        peg_price: 0.82 - seed as f64 * 0.015,
        collateral_ratio: 0.92 - seed as f64 * 0.02,
        nav_history: vec![1.0, 0.88, 0.74, 0.69, 0.62, 0.57],
        current_weights: [0.55, 0.25, 0.15, 0.05],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: oracle,
        target_cr: 1.45,
        mint_fee: 0.008,
        redeem_fee: 0.001,
        loss_function: Some(LossFunction::default()),
    }
}

fn evolve_state(state: &ProtocolSnapshot, epoch: usize) -> ProtocolSnapshot {
    let mut next = state.clone();
    next.previous_weights = state.current_weights;

    let oracle_mean = state.oracle_quality_scores.iter().sum::<f64>() / 4.0;
    let stress = (1.0 - oracle_mean).clamp(0.0, 1.0);
    let periodic_shock = ((epoch as f64 + 1.0) * 0.7).sin() * 0.01;

    next.peg_price = (state.peg_price + (1.0 - state.peg_price) * 0.2 - stress * 0.05
        + periodic_shock * (1.0 + stress))
        .clamp(0.5, 1.5);

    next.collateral_ratio = (state.collateral_ratio
        + (state.target_cr - state.collateral_ratio) * 0.08
        - stress * 0.03)
        .clamp(0.5, 3.0);

    let nav_last = state.nav_history.last().copied().unwrap_or(1.0);
    let nav_step = (next.peg_price - 1.0) * 0.15 - stress * 0.02;
    let nav_next = (nav_last * (1.0 + nav_step)).max(0.01);

    next.nav_history.push(nav_next);
    if next.nav_history.len() > 48 {
        let overflow = next.nav_history.len() - 48;
        next.nav_history.drain(0..overflow);
    }

    for q in &mut next.oracle_quality_scores {
        *q = (*q - stress * 0.005).clamp(0.0, 1.0);
    }

    next
}
