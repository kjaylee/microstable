use crate::{
    aig,
    config::KeeperConfig,
    optimizer::{ParamVector, ProtocolSnapshot},
    tournament,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcProgramAccountsConfig,
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{
    hash::{hash, hashv, Hash},
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use std::{
    collections::HashSet,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};

const AIG_CURRENT_TIER: u8 = 1;
const AIG_TARGET_TIER: u8 = 2;
const TOURNAMENT_BASE_AGENT_SCORE: u64 = 500_000;
const TOURNAMENT_TOP_BOOST: i64 = 50_000;
const TOURNAMENT_BOTTOM_REDUCTION: i64 = -25_000;
const TOURNAMENT_MIN_REGISTRATION_AGE_SLOTS: u64 = 100;
const TOURNAMENT_PARTICIPANT_DIVISOR: usize = 10;
const TOURNAMENT_MIN_PARTICIPANTS: usize = 2;
const AGENT_RECORD_ACCOUNT_DATA_SIZE: u64 = 8 + 160;

type TxRuntime<'a> = (
    &'a RpcClient,
    Option<&'a RpcClient>,
    utils::SecondaryRpcMode,
    &'a [Keypair],
    &'a DerivedAccounts,
);

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

#[cfg(test)]
pub fn maybe_run_aig_cycle(cfg: &KeeperConfig, state: &mut AgentLoopState) -> Result<()> {
    maybe_run_aig_cycle_inner(cfg, state, None)
}

#[allow(clippy::too_many_arguments)]
pub fn maybe_run_aig_cycle_with_tx(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    state: &mut AgentLoopState,
) -> Result<()> {
    maybe_run_aig_cycle_inner(
        cfg,
        state,
        Some((rpc, secondary_rpc, secondary_mode, keepers, derived)),
    )
}

fn maybe_run_aig_cycle_inner(
    cfg: &KeeperConfig,
    state: &mut AgentLoopState,
    tx_runtime: Option<TxRuntime<'_>>,
) -> Result<()> {
    if !cfg.aig_enabled {
        return Ok(());
    }

    let now = Instant::now();
    if !interval_elapsed(state.last_aig_run, cfg.aig_interval_secs, now) {
        return Ok(());
    }

    let candidate = ParamVector::default();
    let baseline = ParamVector {
        weights: [0.40, 0.30, 0.20, 0.10],
        target_cr: 1.30,
        mint_fee: 0.004,
        redeem_fee: 0.002,
    };

    let challenges = aig::generate_challenges(AIG_CURRENT_TIER, AIG_TARGET_TIER);
    let mut results = Vec::with_capacity(challenges.len());

    for challenge in &challenges {
        let baseline_loss =
            aig::run_sandbox_trial(&baseline, &challenge.scenario, challenge.epochs)
                .max(f64::EPSILON);
        let trial_loss = aig::run_sandbox_trial(&candidate, &challenge.scenario, challenge.epochs);
        let result =
            aig::evaluate_challenge_result_for_tier(trial_loss, baseline_loss, AIG_TARGET_TIER);

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
        passed_count, aggregate_score, "aig cycle complete"
    );

    if let Some((rpc, secondary_rpc, secondary_mode, keepers, derived)) = tx_runtime {
        let registered_agents = match fetch_registered_agents(rpc, cfg.program_id, keepers) {
            Ok(agents) => agents,
            Err(err) => {
                warn!(
                    error = %err,
                    "aig tx submission skipped: failed to fetch registered agent registry"
                );
                Vec::new()
            }
        };

        let selection_slot = match rpc.get_slot() {
            Ok(slot) => slot,
            Err(err) => {
                warn!(
                    error = %err,
                    "aig tx submission skipped: failed to fetch current slot for weighted selection"
                );
                state.last_aig_run = Some(now);
                return Ok(());
            }
        };
        let selection_seed = selection_seed_with_entropy(rpc, derived, selection_slot);

        if let Some(agent) = select_candidate_agent(&registered_agents, selection_seed) {
            match resolve_keeper_quorum_signers(rpc, keepers, derived) {
                Ok((keeper_one, keeper_two)) => {
                    let actions = aig_actions_for_outcome(
                        agent,
                        AIG_CURRENT_TIER,
                        AIG_TARGET_TIER,
                        aggregate_score,
                    );

                    let mut sender = |instructions: Vec<Instruction>| -> Result<Signature> {
                        utils::send_instructions(
                            rpc,
                            secondary_rpc,
                            secondary_mode,
                            keeper_one,
                            &[keeper_one, keeper_two],
                            instructions,
                        )
                    };

                    submit_agent_actions(
                        cfg,
                        derived,
                        keeper_one.pubkey(),
                        keeper_two.pubkey(),
                        &actions,
                        &mut sender,
                    );
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        "aig tx submission skipped: failed to resolve keeper quorum signers"
                    );
                }
            }
        } else {
            warn!("aig tx submission skipped: no eligible registered agent available");
        }
    }

    state.last_aig_run = Some(now);
    Ok(())
}

#[cfg(test)]
pub fn maybe_run_tournament_cycle(cfg: &KeeperConfig, state: &mut AgentLoopState) -> Result<()> {
    maybe_run_tournament_cycle_inner(cfg, state, None)
}

#[allow(clippy::too_many_arguments)]
pub fn maybe_run_tournament_cycle_with_tx(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    state: &mut AgentLoopState,
) -> Result<()> {
    maybe_run_tournament_cycle_inner(
        cfg,
        state,
        Some((rpc, secondary_rpc, secondary_mode, keepers, derived)),
    )
}

fn maybe_run_tournament_cycle_inner(
    cfg: &KeeperConfig,
    state: &mut AgentLoopState,
    tx_runtime: Option<TxRuntime<'_>>,
) -> Result<()> {
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

    let registered_agents: Vec<RegisteredAgent> = if let Some((rpc, _, _, keepers, _)) = tx_runtime
    {
        match fetch_registered_agents(rpc, cfg.program_id, keepers) {
            Ok(agents) => agents,
            Err(err) => {
                warn!(
                    error = %err,
                    "tournament cycle skipped: failed to fetch registered agent registry"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    if registered_agents.len() < TOURNAMENT_MIN_PARTICIPANTS {
        warn!(
            participants = registered_agents.len(),
            "tournament cycle skipped: not enough participant agents"
        );
        state.last_tournament_run = Some(now);
        return Ok(());
    }

    let (selection_slot, selection_seed) = if let Some((rpc, _, _, _, derived)) = tx_runtime {
        let slot = match rpc.get_slot() {
            Ok(slot) => slot,
            Err(err) => {
                warn!(
                    error = %err,
                    "tournament cycle skipped: failed to fetch current slot for weighted selection"
                );
                state.last_tournament_run = Some(now);
                return Ok(());
            }
        };
        (slot, selection_seed_with_entropy(rpc, derived, slot))
    } else {
        (round, round)
    };

    let eligible_agents = filter_tournament_eligible_agents(&registered_agents, selection_slot);
    if eligible_agents.len() < TOURNAMENT_MIN_PARTICIPANTS {
        warn!(
            registered = registered_agents.len(),
            eligible = eligible_agents.len(),
            min_age_slots = TOURNAMENT_MIN_REGISTRATION_AGE_SLOTS,
            "tournament cycle skipped: not enough agents satisfy registration-age gate"
        );
        state.last_tournament_run = Some(now);
        return Ok(());
    }

    let participant_cap = tournament_participant_cap(registered_agents.len());
    let participant_target = participant_cap.min(eligible_agents.len());
    let participants =
        select_tournament_participants(&eligible_agents, selection_seed, participant_target);
    if participants.len() < TOURNAMENT_MIN_PARTICIPANTS {
        warn!("tournament cycle skipped: weighted sampling returned insufficient participants");
        state.last_tournament_run = Some(now);
        return Ok(());
    }

    for (idx, agent) in participants.iter().enumerate() {
        tournament::submit_proposal(&mut tournament, *agent, tournament_proposal_for_index(idx), 1)?;
    }

    let result = tournament::evaluate_proposals(&tournament);
    let summary = tournament::tournament_summary(&result);

    info!(
        round = result.round,
        participants = result.participants,
        selection_slot,
        selection_seed,
        winner = ?result.winner,
        winning_loss = result.winning_loss,
        summary = %summary,
        "tournament cycle complete"
    );

    if let Some((rpc, secondary_rpc, secondary_mode, keepers, derived)) = tx_runtime {
        match resolve_keeper_quorum_signers(rpc, keepers, derived) {
            Ok((keeper_one, keeper_two)) => {
                let actions = tournament_actions_from_rankings(
                    &tournament.proposals,
                    TOURNAMENT_BASE_AGENT_SCORE,
                );

                let mut sender = |instructions: Vec<Instruction>| -> Result<Signature> {
                    utils::send_instructions(
                        rpc,
                        secondary_rpc,
                        secondary_mode,
                        keeper_one,
                        &[keeper_one, keeper_two],
                        instructions,
                    )
                };

                submit_agent_actions(
                    cfg,
                    derived,
                    keeper_one.pubkey(),
                    keeper_two.pubkey(),
                    &actions,
                    &mut sender,
                );
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "tournament tx submission skipped: failed to resolve keeper quorum signers"
                );
            }
        }
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegisteredAgent {
    agent: Pubkey,
    stake: u64,
    registered_slot: u64,
}

fn select_candidate_agent(registered_agents: &[RegisteredAgent], slot_seed: u64) -> Option<Pubkey> {
    weighted_random_index(registered_agents, slot_seed, 0).map(|idx| registered_agents[idx].agent)
}

fn select_tournament_participants(
    registered_agents: &[RegisteredAgent],
    slot_seed: u64,
    count: usize,
) -> Vec<Pubkey> {
    let mut pool = registered_agents.to_vec();
    let mut selected = Vec::with_capacity(count);

    for nonce in 0..count {
        let Some(idx) = weighted_random_index(&pool, slot_seed, nonce as u64 + 1) else {
            break;
        };

        selected.push(pool.swap_remove(idx).agent);
    }

    selected
}

fn filter_tournament_eligible_agents(
    registered_agents: &[RegisteredAgent],
    current_slot: u64,
) -> Vec<RegisteredAgent> {
    registered_agents
        .iter()
        .copied()
        .filter(|candidate| {
            current_slot.saturating_sub(candidate.registered_slot)
                >= TOURNAMENT_MIN_REGISTRATION_AGE_SLOTS
        })
        .collect()
}

fn tournament_participant_cap(registered_agent_count: usize) -> usize {
    if registered_agent_count == 0 {
        return 0;
    }

    (registered_agent_count / TOURNAMENT_PARTICIPANT_DIVISOR)
        .max(TOURNAMENT_MIN_PARTICIPANTS)
        .min(registered_agent_count)
}

fn tournament_proposal_for_index(index: usize) -> ParamVector {
    match index % 4 {
        0 => ParamVector::default(),
        1 => ParamVector {
            weights: [0.30, 0.30, 0.20, 0.20],
            target_cr: 1.25,
            mint_fee: 0.002,
            redeem_fee: 0.002,
        },
        2 => ParamVector {
            weights: [0.35, 0.25, 0.20, 0.20],
            target_cr: 1.22,
            mint_fee: 0.003,
            redeem_fee: 0.0015,
        },
        _ => ParamVector {
            weights: [0.28, 0.32, 0.20, 0.20],
            target_cr: 1.28,
            mint_fee: 0.0015,
            redeem_fee: 0.0025,
        },
    }
}

fn selection_seed_with_entropy(rpc: &RpcClient, derived: &DerivedAccounts, slot_seed: u64) -> u64 {
    let recent_blockhash = rpc.get_latest_blockhash().ok();
    let protocol_nonce =
        utils::fetch_account::<wire::ProtocolState>(rpc, &derived.protocol_state, "ProtocolState")
            .ok()
            .map(|protocol| protocol.last_update_slot);

    derive_selection_seed(slot_seed, recent_blockhash, protocol_nonce)
}

fn derive_selection_seed(
    slot_seed: u64,
    recent_blockhash: Option<Hash>,
    protocol_nonce: Option<u64>,
) -> u64 {
    let blockhash_bytes = recent_blockhash
        .map(|blockhash| blockhash.to_bytes())
        .unwrap_or_default();
    let nonce_bytes = protocol_nonce.unwrap_or_default().to_le_bytes();

    let digest = hashv(&[
        b"microstable-agent-selection-v3",
        &slot_seed.to_le_bytes(),
        &blockhash_bytes,
        &nonce_bytes,
    ])
    .to_bytes();

    let mut seed_bytes = [0u8; 8];
    seed_bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(seed_bytes)
}

fn weighted_random_index(
    candidates: &[RegisteredAgent],
    slot_seed: u64,
    nonce: u64,
) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }

    let total_weight: u128 = candidates
        .iter()
        .map(|candidate| u128::from(candidate.stake.max(1)))
        .sum();

    if total_weight == 0 {
        return None;
    }

    let digest = hashv(&[
        b"microstable-agent-selection-v2",
        &slot_seed.to_le_bytes(),
        &nonce.to_le_bytes(),
    ])
    .to_bytes();

    let mut draw_bytes = [0u8; 16];
    draw_bytes.copy_from_slice(&digest[..16]);
    let draw = u128::from_le_bytes(draw_bytes) % total_weight;

    let mut cursor = 0u128;
    for (idx, candidate) in candidates.iter().enumerate() {
        cursor = cursor.saturating_add(u128::from(candidate.stake.max(1)));
        if draw < cursor {
            return Some(idx);
        }
    }

    Some(candidates.len().saturating_sub(1))
}

fn fetch_registered_agents(
    rpc: &RpcClient,
    program_id: Pubkey,
    keepers: &[Keypair],
) -> Result<Vec<RegisteredAgent>> {
    let keeper_pubkeys: HashSet<Pubkey> = keepers.iter().map(Keypair::pubkey).collect();

    let mut agents = Vec::new();
    for (_, account) in rpc.get_program_accounts_with_config(
        &program_id,
        RpcProgramAccountsConfig {
            filters: Some(agent_record_scan_filters()),
            ..RpcProgramAccountsConfig::default()
        },
    )? {
        let data = account.data;

        let record = match wire::decode_account::<wire::AgentRecord>(&data, "AgentRecord") {
            Ok(record) => record,
            Err(_) => continue,
        };

        if record.status != wire::AgentStatus::Active {
            continue;
        }
        if record.stake == 0 {
            continue;
        }
        if keeper_pubkeys.contains(&record.agent) {
            continue;
        }

        agents.push(RegisteredAgent {
            agent: record.agent,
            stake: record.stake,
            registered_slot: record.registered_slot,
        });
    }

    agents.sort_by_key(|candidate| candidate.agent);
    agents.dedup_by(|lhs, rhs| lhs.agent == rhs.agent);
    Ok(agents)
}

fn agent_record_scan_filters() -> Vec<RpcFilterType> {
    vec![
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            0,
            agent_record_discriminator().to_vec(),
        )),
        RpcFilterType::DataSize(AGENT_RECORD_ACCOUNT_DATA_SIZE),
    ]
}

fn agent_record_discriminator() -> [u8; 8] {
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash(b"account:AgentRecord").to_bytes()[..8]);
    discriminator
}

fn resolve_keeper_quorum_signers<'a>(
    rpc: &RpcClient,
    keepers: &'a [Keypair],
    derived: &DerivedAccounts,
) -> Result<(&'a Keypair, &'a Keypair)> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    utils::keeper_quorum_for_protocol(keepers, &protocol.keeper_set)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentTxAction {
    UpdateScore { agent: Pubkey, new_score: u64 },
    Promote { agent: Pubkey, new_tier: u8 },
    Demote { agent: Pubkey, new_tier: u8 },
}

fn aig_actions_for_outcome(
    agent: Pubkey,
    current_tier: u8,
    target_tier: u8,
    aggregate_score: u64,
) -> Vec<AgentTxAction> {
    let mut actions = vec![AgentTxAction::UpdateScore {
        agent,
        new_score: aggregate_score.min(aig::MAX_AIG_SCORE),
    }];

    if aggregate_score >= aig::tier_promotion_threshold(target_tier) {
        if target_tier > current_tier {
            actions.push(AgentTxAction::Promote {
                agent,
                new_tier: target_tier,
            });
        }
    } else if current_tier > 0 {
        actions.push(AgentTxAction::Demote {
            agent,
            new_tier: current_tier - 1,
        });
    }

    actions
}

fn tournament_actions_from_rankings(
    proposals: &[tournament::AgentProposal],
    base_score: u64,
) -> Vec<AgentTxAction> {
    if proposals.is_empty() {
        return Vec::new();
    }

    let mut ranked = proposals.to_vec();
    ranked.sort_by(|lhs, rhs| {
        lhs.loss
            .total_cmp(&rhs.loss)
            .then_with(|| lhs.submitted_at.cmp(&rhs.submitted_at))
    });

    let top_count = (ranked.len() / 3).max(1);
    let bottom_count = (ranked.len() / 3).max(1);

    ranked
        .iter()
        .enumerate()
        .map(|(rank, proposal)| {
            let adjustment = if rank < top_count {
                TOURNAMENT_TOP_BOOST
            } else if rank >= ranked.len().saturating_sub(bottom_count) {
                TOURNAMENT_BOTTOM_REDUCTION
            } else {
                0
            };

            AgentTxAction::UpdateScore {
                agent: proposal.agent,
                new_score: apply_score_adjustment(base_score, adjustment),
            }
        })
        .collect()
}

fn apply_score_adjustment(base_score: u64, adjustment: i64) -> u64 {
    let adjusted = if adjustment >= 0 {
        base_score.saturating_add(adjustment as u64)
    } else {
        base_score.saturating_sub(adjustment.unsigned_abs())
    };

    adjusted.min(aig::MAX_AIG_SCORE)
}

fn derive_agent_record_pda(program_id: Pubkey, agent: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"agent", agent.as_ref()], &program_id).0
}

fn submit_agent_actions<F>(
    cfg: &KeeperConfig,
    derived: &DerivedAccounts,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    actions: &[AgentTxAction],
    sender: &mut F,
) where
    F: FnMut(Vec<Instruction>) -> Result<Signature>,
{
    for action in actions {
        let (agent, label, build_ix) = match action {
            AgentTxAction::UpdateScore { agent, new_score } => (
                *agent,
                "update_agent_score",
                wire::ix_update_agent_score(
                    cfg.program_id,
                    derived.protocol_state,
                    keeper_one,
                    keeper_two,
                    derive_agent_record_pda(cfg.program_id, *agent),
                    *agent,
                    (*new_score).min(aig::MAX_AIG_SCORE),
                ),
            ),
            AgentTxAction::Promote { agent, new_tier } => (
                *agent,
                "promote_agent",
                wire::ix_promote_agent(
                    cfg.program_id,
                    derived.protocol_state,
                    keeper_one,
                    keeper_two,
                    derive_agent_record_pda(cfg.program_id, *agent),
                    *agent,
                    *new_tier,
                ),
            ),
            AgentTxAction::Demote { agent, new_tier } => (
                *agent,
                "demote_agent",
                wire::ix_demote_agent(
                    cfg.program_id,
                    derived.protocol_state,
                    keeper_one,
                    keeper_two,
                    derive_agent_record_pda(cfg.program_id, *agent),
                    *agent,
                    *new_tier,
                ),
            ),
        };

        let ix = match build_ix {
            Ok(ix) => ix,
            Err(err) => {
                warn!(
                    action = label,
                    agent = %agent,
                    error = %err,
                    "agent tx build failed"
                );
                continue;
            }
        };

        match sender(vec![ix]) {
            Ok(sig) => {
                info!(
                    action = label,
                    agent = %agent,
                    signature = %sig,
                    "agent tx submitted"
                );
            }
            Err(err) => {
                warn!(
                    action = label,
                    agent = %agent,
                    error = %err,
                    "agent tx submission failed"
                );
            }
        }
    }
}

#[cfg(test)]
mod wiring_tests {
    use super::*;

    fn test_cfg_and_derived() -> (KeeperConfig, DerivedAccounts) {
        let cfg = KeeperConfig::default_devnet();
        let derived = DerivedAccounts::derive(&cfg.program_id);
        (cfg, derived)
    }

    #[test]
    fn tc_alw_01_aig_tx_path_submits_score_and_promotion() {
        let (cfg, derived) = test_cfg_and_derived();
        let keeper_one = Pubkey::new_unique();
        let keeper_two = Pubkey::new_unique();
        let agent = Pubkey::new_unique();

        let actions = aig_actions_for_outcome(agent, 1, 2, aig::TIER2_PROMOTION_THRESHOLD + 1);
        assert_eq!(actions.len(), 2);

        let mut captured = Vec::<Instruction>::new();
        let mut sender = |instructions: Vec<Instruction>| -> Result<Signature> {
            captured.push(instructions[0].clone());
            Ok(Signature::default())
        };

        submit_agent_actions(
            &cfg,
            &derived,
            keeper_one,
            keeper_two,
            &actions,
            &mut sender,
        );

        assert_eq!(captured.len(), 2);

        let agent_record = derive_agent_record_pda(cfg.program_id, agent);
        let expected_update = wire::ix_update_agent_score(
            cfg.program_id,
            derived.protocol_state,
            keeper_one,
            keeper_two,
            agent_record,
            agent,
            aig::TIER2_PROMOTION_THRESHOLD + 1,
        )
        .unwrap();
        let expected_promote = wire::ix_promote_agent(
            cfg.program_id,
            derived.protocol_state,
            keeper_one,
            keeper_two,
            agent_record,
            agent,
            2,
        )
        .unwrap();

        assert_eq!(captured[0].data, expected_update.data);
        assert_eq!(captured[1].data, expected_promote.data);
    }

    #[test]
    fn tc_alw_02_tournament_tx_path_submits_score_updates_for_rankings() {
        let (cfg, derived) = test_cfg_and_derived();
        let keeper_one = Pubkey::new_unique();
        let keeper_two = Pubkey::new_unique();

        let best = Pubkey::new_unique();
        let middle = Pubkey::new_unique();
        let worst = Pubkey::new_unique();

        let proposals = vec![
            tournament::AgentProposal {
                agent: middle,
                params: ParamVector::default(),
                loss: 2.0,
                submitted_at: 1_000,
            },
            tournament::AgentProposal {
                agent: worst,
                params: ParamVector::default(),
                loss: 3.0,
                submitted_at: 2_000,
            },
            tournament::AgentProposal {
                agent: best,
                params: ParamVector::default(),
                loss: 1.0,
                submitted_at: 3_000,
            },
        ];

        let actions = tournament_actions_from_rankings(&proposals, TOURNAMENT_BASE_AGENT_SCORE);
        assert_eq!(actions.len(), 3);

        let mut best_score = None;
        let mut worst_score = None;
        for action in &actions {
            if let AgentTxAction::UpdateScore { agent, new_score } = action {
                if *agent == best {
                    best_score = Some(*new_score);
                }
                if *agent == worst {
                    worst_score = Some(*new_score);
                }
            }
        }

        assert_eq!(best_score, Some(550_000));
        assert_eq!(worst_score, Some(475_000));

        let mut captured = Vec::<Instruction>::new();
        let mut sender = |instructions: Vec<Instruction>| -> Result<Signature> {
            captured.push(instructions[0].clone());
            Ok(Signature::default())
        };

        submit_agent_actions(
            &cfg,
            &derived,
            keeper_one,
            keeper_two,
            &actions,
            &mut sender,
        );
        assert_eq!(captured.len(), 3);

        let expected_discriminator = wire::ix_update_agent_score(
            cfg.program_id,
            derived.protocol_state,
            keeper_one,
            keeper_two,
            derive_agent_record_pda(cfg.program_id, best),
            best,
            550_000,
        )
        .unwrap()
        .data[..8]
            .to_vec();

        for ix in captured {
            assert_eq!(&ix.data[..8], expected_discriminator.as_slice());
        }
    }

    #[test]
    fn tc_alw_03_aig_candidate_selection_varies_with_slot_seed() {
        let agents = vec![
            RegisteredAgent {
                agent: Pubkey::new_unique(),
                stake: 100,
                registered_slot: 0,
            },
            RegisteredAgent {
                agent: Pubkey::new_unique(),
                stake: 100,
                registered_slot: 0,
            },
            RegisteredAgent {
                agent: Pubkey::new_unique(),
                stake: 100,
                registered_slot: 0,
            },
        ];

        let first = select_candidate_agent(&agents, 0).expect("candidate should exist");
        let mut observed_alternative = false;
        for slot in 1..256 {
            if select_candidate_agent(&agents, slot).expect("candidate should exist") != first {
                observed_alternative = true;
                break;
            }
        }

        assert!(
            observed_alternative,
            "weighted selection should vary across slot seeds"
        );
    }

    #[test]
    fn tc_alw_04_tournament_sampling_returns_unique_participants() {
        let agents = vec![
            RegisteredAgent {
                agent: Pubkey::new_unique(),
                stake: 500,
                registered_slot: 0,
            },
            RegisteredAgent {
                agent: Pubkey::new_unique(),
                stake: 250,
                registered_slot: 0,
            },
            RegisteredAgent {
                agent: Pubkey::new_unique(),
                stake: 100,
                registered_slot: 0,
            },
        ];

        let sampled = select_tournament_participants(&agents, 42, 2);
        assert_eq!(sampled.len(), 2);
        assert_ne!(sampled[0], sampled[1]);
        assert!(agents.iter().any(|candidate| candidate.agent == sampled[0]));
        assert!(agents.iter().any(|candidate| candidate.agent == sampled[1]));
    }

    #[test]
    fn tc_alw_05_registration_age_filter_blocks_fresh_agents() {
        let mature = RegisteredAgent {
            agent: Pubkey::new_unique(),
            stake: 500,
            registered_slot: 100,
        };
        let fresh = RegisteredAgent {
            agent: Pubkey::new_unique(),
            stake: 500,
            registered_slot: 190,
        };

        let filtered = filter_tournament_eligible_agents(&[mature, fresh], 200);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].agent, mature.agent);
    }

    #[test]
    fn tc_alw_06_tournament_participant_cap_scales_with_registry_size() {
        assert_eq!(tournament_participant_cap(0), 0);
        assert_eq!(tournament_participant_cap(2), 2);
        assert_eq!(tournament_participant_cap(9), 2);
        assert_eq!(tournament_participant_cap(20), 2);
        assert_eq!(tournament_participant_cap(30), 3);
        assert_eq!(tournament_participant_cap(100), 10);
    }

    #[test]
    fn tc_alw_07_entropy_seed_mixes_slot_blockhash_and_nonce() {
        let slot_seed = 42;
        let blockhash_a = Hash::new_unique();
        let blockhash_b = Hash::new_unique();

        let seed_a = derive_selection_seed(slot_seed, Some(blockhash_a), Some(7));
        let seed_b = derive_selection_seed(slot_seed, Some(blockhash_b), Some(7));
        let seed_c = derive_selection_seed(slot_seed, Some(blockhash_a), Some(8));

        assert_ne!(seed_a, slot_seed);
        assert_ne!(seed_a, seed_b);
        assert_ne!(seed_a, seed_c);
    }

    #[test]
    fn tc_alw_08_agent_record_scan_filters_include_discriminator_and_size() {
        let filters = agent_record_scan_filters();
        assert_eq!(filters.len(), 2);

        match &filters[0] {
            RpcFilterType::Memcmp(memcmp) => {
                assert_eq!(memcmp.offset(), 0);
                let bytes = memcmp.bytes().expect("memcmp bytes should decode");
                assert_eq!(bytes.as_ref(), &agent_record_discriminator());
            }
            _ => panic!("first filter should be memcmp discriminator"),
        }

        match filters[1] {
            RpcFilterType::DataSize(size) => assert_eq!(size, AGENT_RECORD_ACCOUNT_DATA_SIZE),
            _ => panic!("second filter should be dataSize"),
        }
    }
}
