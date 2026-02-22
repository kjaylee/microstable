#![cfg(test)]

use crate::{
    optimizer::{LossFunction, ParamVector, ProtocolSnapshot, SafetyBounds},
    tournament::{
        create_tournament, evaluate_proposals, submit_proposal, tournament_summary,
        validate_proposal,
    },
};
use solana_sdk::pubkey::Pubkey;

fn cr_only_snapshot() -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 1.0,
        collateral_ratio: 1.0,
        nav_history: vec![1.0, 1.0],
        current_weights: [0.25, 0.25, 0.25, 0.25],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: [1.0, 1.0, 1.0, 1.0],
        target_cr: 1.2,
        mint_fee: 0.001,
        redeem_fee: 0.001,
        loss_function: Some(LossFunction {
            lambda_price: 0.0,
            lambda_cr: 1.0,
            lambda_vol: 0.0,
            lambda_turn: 0.0,
            lambda_conc: 0.0,
            lambda_oracle: 0.0,
        }),
    }
}

fn proposal_with_target_cr(target_cr: f64) -> ParamVector {
    ParamVector {
        weights: [0.25, 0.25, 0.25, 0.25],
        target_cr,
        mint_fee: 0.001,
        redeem_fee: 0.001,
    }
}

#[test]
fn tc_t01_create_tournament_returns_valid_tournament_with_snapshot() {
    let snapshot = cr_only_snapshot();
    let tournament = create_tournament(snapshot.clone(), 42, 2);

    assert_eq!(tournament.round, 42);
    assert_eq!(tournament.min_tier, 2);
    assert!(tournament.proposals.is_empty());
    assert_eq!(tournament.snapshot.peg_price, snapshot.peg_price);
    assert!(tournament.baseline_loss.is_finite());
}

#[test]
fn tc_t02_submit_proposal_adds_proposal_to_tournament() {
    let mut tournament = create_tournament(cr_only_snapshot(), 1, 1);
    let agent = Pubkey::new_unique();

    submit_proposal(&mut tournament, agent, proposal_with_target_cr(1.1), 1).unwrap();

    assert_eq!(tournament.proposals.len(), 1);
    assert_eq!(tournament.proposals[0].agent, agent);
}

#[test]
fn tc_t03_submit_proposal_rejects_agent_below_min_tier() {
    let mut tournament = create_tournament(cr_only_snapshot(), 1, 2);

    let result = submit_proposal(
        &mut tournament,
        Pubkey::new_unique(),
        proposal_with_target_cr(1.1),
        1,
    );

    assert!(result.is_err());
    assert!(tournament.proposals.is_empty());
}

#[test]
fn tc_t04_evaluate_proposals_picks_lowest_loss_proposal_as_winner() {
    let mut tournament = create_tournament(cr_only_snapshot(), 7, 1);
    let winner = Pubkey::new_unique();
    let loser = Pubkey::new_unique();

    submit_proposal(&mut tournament, winner, proposal_with_target_cr(1.1), 1).unwrap();
    submit_proposal(&mut tournament, loser, proposal_with_target_cr(1.3), 1).unwrap();

    let result = evaluate_proposals(&tournament);

    assert_eq!(result.winner, Some(winner));
    assert_eq!(result.participants, 2);
}

#[test]
fn tc_t05_evaluate_proposals_with_single_proposal_returns_that_proposal() {
    let mut tournament = create_tournament(cr_only_snapshot(), 8, 1);
    let only = Pubkey::new_unique();

    submit_proposal(&mut tournament, only, proposal_with_target_cr(1.1), 1).unwrap();

    let result = evaluate_proposals(&tournament);

    assert_eq!(result.winner, Some(only));
    assert!(result.winning_params.is_some());
    assert!(result.winning_loss.is_finite());
}

#[test]
fn tc_t06_evaluate_proposals_with_empty_proposals_returns_none() {
    let tournament = create_tournament(cr_only_snapshot(), 9, 1);

    let result = evaluate_proposals(&tournament);

    assert_eq!(result.winner, None);
    assert_eq!(result.participants, 0);
    assert!(result.winning_params.is_none());
}

#[test]
fn tc_t07_evaluate_proposals_breaks_ties_by_earlier_submission_time() {
    let mut tournament = create_tournament(cr_only_snapshot(), 10, 1);
    let later_submitted = Pubkey::new_unique();
    let earlier_submitted = Pubkey::new_unique();

    submit_proposal(
        &mut tournament,
        later_submitted,
        proposal_with_target_cr(1.1),
        1,
    )
    .unwrap();
    submit_proposal(
        &mut tournament,
        earlier_submitted,
        proposal_with_target_cr(1.1),
        1,
    )
    .unwrap();

    tournament.proposals[0].submitted_at = 2_000;
    tournament.proposals[1].submitted_at = 1_000;

    let result = evaluate_proposals(&tournament);

    assert_eq!(result.winner, Some(earlier_submitted));
}

#[test]
fn tc_t08_score_adjustment_gives_winner_plus_50_000_score_boost() {
    let mut tournament = create_tournament(cr_only_snapshot(), 11, 1);
    let winner = Pubkey::new_unique();
    let other = Pubkey::new_unique();

    submit_proposal(&mut tournament, winner, proposal_with_target_cr(1.1), 1).unwrap();
    submit_proposal(&mut tournament, other, proposal_with_target_cr(1.3), 1).unwrap();

    let result = evaluate_proposals(&tournament);
    let winner_adjustment = result
        .score_adjustments
        .iter()
        .find(|(agent, _)| *agent == winner)
        .map(|(_, delta)| *delta)
        .unwrap();

    assert_eq!(winner_adjustment, 50_000);
}

#[test]
fn tc_t09_score_adjustment_penalizes_proposals_worse_than_baseline_by_minus_10_000() {
    let mut snapshot = cr_only_snapshot();
    snapshot.target_cr = 1.1; // baseline loss = 0.01

    let mut tournament = create_tournament(snapshot, 12, 1);
    let winner = Pubkey::new_unique();
    let penalized = Pubkey::new_unique();

    submit_proposal(&mut tournament, winner, proposal_with_target_cr(1.1), 1).unwrap();
    submit_proposal(&mut tournament, penalized, proposal_with_target_cr(1.2), 1).unwrap();

    let result = evaluate_proposals(&tournament);
    let penalized_adjustment = result
        .score_adjustments
        .iter()
        .find(|(agent, _)| *agent == penalized)
        .map(|(_, delta)| *delta)
        .unwrap();

    assert_eq!(penalized_adjustment, -10_000);
}

#[test]
fn tc_t10_tournament_summary_correctly_reports_participant_count_and_winner() {
    let mut tournament = create_tournament(cr_only_snapshot(), 13, 1);
    let winner = Pubkey::new_unique();

    submit_proposal(&mut tournament, winner, proposal_with_target_cr(1.1), 1).unwrap();
    submit_proposal(
        &mut tournament,
        Pubkey::new_unique(),
        proposal_with_target_cr(1.3),
        1,
    )
    .unwrap();

    let result = evaluate_proposals(&tournament);
    let summary = tournament_summary(&result);

    assert!(summary.contains("participants=2"));
    assert!(summary.contains(&winner.to_string()));
}

#[test]
fn tc_t11_validate_proposal_rejects_non_finite_params() {
    let bounds = SafetyBounds::default();

    let result = validate_proposal(&ParamVector::infinities(), &bounds);

    assert!(result.is_err());
}

#[test]
fn tc_t12_validate_proposal_rejects_params_outside_safety_bounds() {
    let bounds = SafetyBounds::default();
    let invalid = ParamVector {
        weights: [0.9, 0.9, 0.9, 0.9],
        target_cr: 9.0,
        mint_fee: -0.1,
        redeem_fee: 2.0,
    };

    let result = validate_proposal(&invalid, &bounds);

    assert!(result.is_err());
}
