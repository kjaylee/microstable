use crate::optimizer::{self, LossFunction, ParamVector, ProtocolSnapshot, SafetyBounds};
use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Tournament {
    pub round: u64,
    pub snapshot: optimizer::ProtocolSnapshot,
    pub baseline_loss: f64,
    pub proposals: Vec<AgentProposal>,
    pub min_tier: u8,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AgentProposal {
    pub agent: solana_sdk::pubkey::Pubkey,
    pub params: optimizer::ParamVector,
    pub loss: f64,
    pub submitted_at: i64,
}

#[derive(Debug, Clone)]
pub struct TournamentResult {
    pub winner: Option<solana_sdk::pubkey::Pubkey>,
    pub winning_params: Option<optimizer::ParamVector>,
    pub winning_loss: f64,
    pub participants: usize,
    pub round: u64,
    pub score_adjustments: Vec<(solana_sdk::pubkey::Pubkey, i64)>,
}

pub fn create_tournament(snapshot: ProtocolSnapshot, round: u64, min_tier: u8) -> Tournament {
    let loss_fn = snapshot.loss_function.unwrap_or_default();
    let baseline_loss = loss_fn
        .compute(&snapshot)
        .map(|loss| loss.total_loss)
        .unwrap_or(f64::INFINITY);

    Tournament {
        round,
        snapshot,
        baseline_loss,
        proposals: Vec::new(),
        min_tier,
        created_at: unix_timestamp(),
    }
}

pub fn submit_proposal(
    tournament: &mut Tournament,
    agent: Pubkey,
    params: ParamVector,
    tier: u8,
) -> Result<()> {
    if tier < tournament.min_tier {
        return Err(anyhow!(
            "agent tier {} below minimum tournament tier {}",
            tier,
            tournament.min_tier
        ));
    }

    validate_proposal(&params, &SafetyBounds::default())?;

    let snapshot = tournament.snapshot.with_params(&params);
    let loss_fn: LossFunction = tournament.snapshot.loss_function.unwrap_or_default();
    let loss = loss_fn.compute(&snapshot)?.total_loss;

    tournament.proposals.push(AgentProposal {
        agent,
        params,
        loss,
        submitted_at: unix_timestamp(),
    });

    Ok(())
}

pub fn validate_proposal(params: &ParamVector, bounds: &SafetyBounds) -> Result<()> {
    if !params.is_finite() {
        return Err(anyhow!("proposal params contain non-finite values"));
    }

    bounds.validate()?;
    optimizer::validate_safety_set(params, bounds)?;
    Ok(())
}

pub fn evaluate_proposals(tournament: &Tournament) -> TournamentResult {
    if tournament.proposals.is_empty() {
        return TournamentResult {
            winner: None,
            winning_params: None,
            winning_loss: f64::INFINITY,
            participants: 0,
            round: tournament.round,
            score_adjustments: Vec::new(),
        };
    }

    let loss_fn = tournament.snapshot.loss_function.unwrap_or_default();

    let mut evaluated: Vec<(&AgentProposal, f64)> = Vec::with_capacity(tournament.proposals.len());
    for proposal in &tournament.proposals {
        let loss = loss_fn
            .compute(&tournament.snapshot.with_params(&proposal.params))
            .map(|result| result.total_loss)
            .unwrap_or(f64::INFINITY);
        evaluated.push((proposal, loss));
    }

    let mut winner_idx = 0usize;
    for idx in 1..evaluated.len() {
        let (current_proposal, current_loss) = evaluated[winner_idx];
        let (candidate_proposal, candidate_loss) = evaluated[idx];

        if candidate_loss.total_cmp(&current_loss).is_lt()
            || (candidate_loss.total_cmp(&current_loss).is_eq()
                && candidate_proposal.submitted_at < current_proposal.submitted_at)
        {
            winner_idx = idx;
        }
    }

    let winner_proposal = evaluated[winner_idx].0;
    let winning_loss = evaluated[winner_idx].1;

    let mut score_adjustments = Vec::with_capacity(evaluated.len());
    for (proposal, loss) in &evaluated {
        let adjustment = if proposal.agent == winner_proposal.agent {
            50_000
        } else {
            score_adjustment_for_non_winner(*loss, tournament.baseline_loss)
        };

        score_adjustments.push((proposal.agent, adjustment));
    }

    TournamentResult {
        winner: Some(winner_proposal.agent),
        winning_params: Some(winner_proposal.params),
        winning_loss,
        participants: tournament.proposals.len(),
        round: tournament.round,
        score_adjustments,
    }
}

pub fn tournament_summary(result: &TournamentResult) -> String {
    let winner = result
        .winner
        .map(|pk| pk.to_string())
        .unwrap_or_else(|| "None".to_string());

    format!(
        "round={}, participants={}, winner={}, winning_loss={:.6}",
        result.round, result.participants, winner, result.winning_loss
    )
}

fn score_adjustment_for_non_winner(loss: f64, baseline_loss: f64) -> i64 {
    if !loss.is_finite() {
        return -25_000;
    }

    if loss < baseline_loss {
        return 10_000;
    }

    if loss > baseline_loss * 5.0 {
        return -25_000;
    }

    if loss > baseline_loss * 2.0 {
        return -10_000;
    }

    0
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
