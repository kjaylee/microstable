use crate::{
    aig,
    config::KeeperConfig,
    optimizer::{ParamVector, ProtocolSnapshot},
    tournament,
};
use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::info;

pub struct AgentLoopState {
    pub last_aig_run: Option<Instant>,
    pub last_tournament_run: Option<Instant>,
}

impl Default for AgentLoopState {
    fn default() -> Self {
        Self {
            last_aig_run: None,
            last_tournament_run: None,
        }
    }
}

pub fn maybe_run_aig_cycle(cfg: &KeeperConfig, state: &mut AgentLoopState) -> Result<()> {
    if !cfg.aig_enabled {
        return Ok(());
    }

    let now = Instant::now();
    if !interval_elapsed(state.last_aig_run, cfg.aig_interval_secs, now) {
        return Ok(());
    }

    let current_tier = 0;
    let target_tier = 1;
    let candidate = ParamVector::default();
    let baseline = ParamVector {
        weights: [0.40, 0.30, 0.20, 0.10],
        target_cr: 1.30,
        mint_fee: 0.004,
        redeem_fee: 0.002,
    };

    let challenges = aig::generate_challenges(current_tier, target_tier);
    let mut results = Vec::with_capacity(challenges.len());

    for challenge in &challenges {
        let baseline_loss = aig::run_sandbox_trial(&baseline, &challenge.scenario, challenge.epochs)
            .max(f64::EPSILON);
        let trial_loss = aig::run_sandbox_trial(&candidate, &challenge.scenario, challenge.epochs);
        let result = aig::evaluate_challenge_result_for_tier(trial_loss, baseline_loss, target_tier);

        info!(
            kind = ?challenge.kind,
            epochs = challenge.epochs,
            baseline_loss,
            trial_loss,
            score = result.score,
            passed = result.passed,
            "aig challenge evaluated"
        );

        results.push(result);
    }

    let aggregate_score = aig::aggregate_scores(&results);
    let passed_count = results.iter().filter(|result| result.passed).count();

    info!(
        challenges = results.len(),
        passed_count,
        aggregate_score,
        "aig cycle complete"
    );

    state.last_aig_run = Some(now);
    Ok(())
}

pub fn maybe_run_tournament_cycle(cfg: &KeeperConfig, state: &mut AgentLoopState) -> Result<()> {
    if !cfg.tournament_enabled {
        return Ok(());
    }

    let now = Instant::now();
    if !interval_elapsed(state.last_tournament_run, cfg.tournament_interval_secs, now) {
        return Ok(());
    }

    let round = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    let snapshot = ProtocolSnapshot::default();
    let mut tournament = tournament::create_tournament(snapshot, round, 1);

    let base_proposal = ParamVector::default();
    let challenger_proposal = ParamVector {
        weights: [0.30, 0.30, 0.20, 0.20],
        target_cr: 1.25,
        mint_fee: 0.002,
        redeem_fee: 0.002,
    };

    tournament::submit_proposal(&mut tournament, Pubkey::new_unique(), base_proposal, 1)?;
    tournament::submit_proposal(&mut tournament, Pubkey::new_unique(), challenger_proposal, 1)?;

    let result = tournament::evaluate_proposals(&tournament);
    let summary = tournament::tournament_summary(&result);

    info!(
        round = result.round,
        participants = result.participants,
        winner = ?result.winner,
        winning_loss = result.winning_loss,
        summary = %summary,
        "tournament cycle complete"
    );

    state.last_tournament_run = Some(now);
    Ok(())
}

fn interval_elapsed(last_run: Option<Instant>, interval_secs: u64, now: Instant) -> bool {
    let interval = Duration::from_secs(interval_secs);
    match last_run {
        None => true,
        Some(previous) => now
            .checked_duration_since(previous)
            .map(|elapsed| elapsed >= interval)
            .unwrap_or(false),
    }
}
