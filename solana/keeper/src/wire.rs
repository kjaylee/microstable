use anyhow::{anyhow, Context, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::{
    hash::hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq, BorshDeserialize)]
pub struct ProtocolState {
    pub weights: [u64; 4],
    pub fee_rate: u64,
    pub mint_fee_rate: u64,
    pub redeem_fee_rate: u64,
    pub cr_target: u64,
    pub total_supply: u64,
    pub last_update_slot: u64,
    pub keeper_set: [Pubkey; 3],
    pub emergency_shutdown: bool,
    pub pending_rebalance_commit: [u8; 32],
    pub pending_rebalance_slot: u64,
    pub pending_rebalance_expiry: u64,
    pub pending_keeper_set: [[u8; 32]; 3],
    pub pending_keeper_activation_slot: u64,
    pub bump: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshDeserialize)]
pub struct CollateralVault {
    pub index: u8,
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub oracle: Pubkey,
    pub risk_score: u64,
    pub weight_cap: u64,
    pub base_weight_cap: u64,
    pub price: u64,
    pub confidence: u64,
    pub last_oracle_slot: u64,
    pub total_deposits: u64,
    pub bump: u8,
    pub pyth_price_feed: Pubkey,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshDeserialize)]
pub struct CircuitBreakerState {
    pub status: [u8; 4],
    pub activation_tick: [u64; 4],
    pub trigger_count: [u64; 4],
    pub cooldown_until: [u64; 4],
    pub last_trigger_tick: [u64; 4],
    pub recent_trigger_count: [u8; 4],
    pub recovery_tick: [u64; 4],
    pub cb1_collateral_index: u8,
    pub mint_rate_limit: u64,
    pub optimizer_enabled: bool,
    pub learning_rate_scale: u64,
    pub max_activation_duration: u64,
    pub bump: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshDeserialize)]
pub enum AgentRole {
    Optimizer,
    Monitor,
    Auditor,
    Liquidator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshDeserialize)]
pub enum AgentStatus {
    Active,
    Cooldown,
    Slashed,
    Deregistered,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshDeserialize)]
pub struct AgentRecord {
    pub agent: Pubkey,
    pub stake: u64,
    pub reputation: u64,
    pub role: AgentRole,
    pub tier: u8,
    pub status: AgentStatus,
    pub proposals_submitted: u64,
    pub proposals_accepted: u64,
    pub registered_at: i64,
    pub last_active_at: i64,
    pub agent_score: u64,
    pub bump: u8,
}

#[derive(BorshSerialize)]
struct UpdateOraclePythArgs {
    collateral_index: u8,
}

#[derive(BorshSerialize)]
struct CommitRebalanceArgs {
    commit_hash: [u8; 32],
    valid_for_slots: u64,
}

#[derive(BorshSerialize)]
struct RebalanceArgs {
    new_weights: [u64; 4],
    max_slippage_bps: u64,
    batch_slot: u64,
    reveal_salt: [u8; 32],
}

#[derive(BorshSerialize)]
pub struct UpdateProtocolParamsArgs {
    pub new_cr_target: u64,
    pub new_mint_fee: u64,
    pub new_redeem_fee: u64,
}

#[derive(BorshSerialize)]
struct UpdateAgentScoreArgs {
    agent: Pubkey,
    new_score: u64,
}

#[derive(BorshSerialize)]
struct PromoteAgentArgs {
    agent: Pubkey,
    new_tier: u8,
}

#[derive(BorshSerialize)]
struct DemoteAgentArgs {
    agent: Pubkey,
    new_tier: u8,
}

pub fn decode_account<T: BorshDeserialize>(data: &[u8], account_name: &str) -> Result<T> {
    if data.len() < 8 {
        return Err(anyhow!("account data too short for discriminator"));
    }

    let expected = anchor_discriminator("account", account_name);
    if data[..8] != expected {
        return Err(anyhow!(
            "account discriminator mismatch for {account_name}: expected {:02x?}, got {:02x?}",
            expected,
            &data[..8]
        ));
    }

    let mut payload = &data[8..];
    let decoded = T::deserialize(&mut payload).map_err(|e| anyhow!("borsh decode failed: {e}"))?;

    if !payload.is_empty() {
        warn!(
            account = account_name,
            trailing_bytes = payload.len(),
            "decoded account contains unexpected trailing bytes"
        );
    }

    Ok(decoded)
}

pub fn ix_update_oracle_pyth(
    program_id: Pubkey,
    protocol_state: Pubkey,
    circuit_breaker: Pubkey,
    vaults: [Pubkey; 4],
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    pyth_price_account: Pubkey,
    collateral_index: u8,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new(circuit_breaker, false),
            AccountMeta::new(vaults[0], false),
            AccountMeta::new(vaults[1], false),
            AccountMeta::new(vaults[2], false),
            AccountMeta::new(vaults[3], false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
            AccountMeta::new_readonly(pyth_price_account, false),
        ],
        data: instruction_data(
            "update_oracle_pyth",
            &UpdateOraclePythArgs { collateral_index },
        )?,
    })
}

pub fn ix_emergency_shutdown(
    program_id: Pubkey,
    protocol_state: Pubkey,
    circuit_breaker: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new(circuit_breaker, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
        ],
        data: instruction_data_unit("emergency_shutdown"),
    })
}

pub fn ix_commit_rebalance(
    program_id: Pubkey,
    protocol_state: Pubkey,
    agent_record: Pubkey,
    submitting_agent: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    commit_hash: [u8; 32],
    valid_for_slots: u64,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new(agent_record, false),
            AccountMeta::new_readonly(submitting_agent, true),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
        ],
        data: instruction_data(
            "commit_rebalance",
            &CommitRebalanceArgs {
                commit_hash,
                valid_for_slots,
            },
        )?,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn ix_rebalance(
    program_id: Pubkey,
    protocol_state: Pubkey,
    circuit_breaker: Pubkey,
    vaults: [Pubkey; 4],
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    new_weights: [u64; 4],
    max_slippage_bps: u64,
    batch_slot: u64,
    reveal_salt: [u8; 32],
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new(circuit_breaker, false),
            AccountMeta::new_readonly(vaults[0], false),
            AccountMeta::new_readonly(vaults[1], false),
            AccountMeta::new_readonly(vaults[2], false),
            AccountMeta::new_readonly(vaults[3], false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
        ],
        data: instruction_data(
            "rebalance",
            &RebalanceArgs {
                new_weights,
                max_slippage_bps,
                batch_slot,
                reveal_salt,
            },
        )?,
    })
}

pub fn ix_update_protocol_params(
    program_id: Pubkey,
    protocol_state: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    args: UpdateProtocolParamsArgs,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
        ],
        data: instruction_data("update_protocol_params", &args)?,
    })
}

pub fn ix_update_agent_score(
    program_id: Pubkey,
    protocol_state: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    agent_record: Pubkey,
    agent: Pubkey,
    new_score: u64,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(protocol_state, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
            AccountMeta::new(agent_record, false),
        ],
        data: instruction_data(
            "update_agent_score",
            &UpdateAgentScoreArgs { agent, new_score },
        )?,
    })
}

pub fn ix_promote_agent(
    program_id: Pubkey,
    protocol_state: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    agent_record: Pubkey,
    agent: Pubkey,
    new_tier: u8,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(protocol_state, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
            AccountMeta::new(agent_record, false),
        ],
        data: instruction_data("promote_agent", &PromoteAgentArgs { agent, new_tier })?,
    })
}

pub fn ix_demote_agent(
    program_id: Pubkey,
    protocol_state: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    agent_record: Pubkey,
    agent: Pubkey,
    new_tier: u8,
) -> Result<Instruction> {
    Ok(Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(protocol_state, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
            AccountMeta::new(agent_record, false),
        ],
        data: instruction_data("demote_agent", &DemoteAgentArgs { agent, new_tier })?,
    })
}

fn instruction_data<T: BorshSerialize>(name: &str, args: &T) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&anchor_discriminator("global", name));
    let mut payload = borsh::to_vec(args)
        .with_context(|| format!("failed to serialize instruction args for {name}"))?;
    out.append(&mut payload);
    Ok(out)
}

fn instruction_data_unit(name: &str) -> Vec<u8> {
    anchor_discriminator("global", name).to_vec()
}

fn anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let preimage = format!("{namespace}:{name}");
    let digest = hash(preimage.as_bytes()).to_bytes();
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}
