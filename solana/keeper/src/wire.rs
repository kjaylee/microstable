use anyhow::{anyhow, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::{
    hash::hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

#[derive(Debug, Clone, BorshDeserialize)]
pub struct ProtocolState {
    pub weights: [u64; 4],
    pub fee_rate: u64,
    pub cr_target: u64,
    pub total_supply: u64,
    pub last_update_slot: u64,
    pub keeper_set: [Pubkey; 3],
    pub emergency_shutdown: bool,
    pub pending_rebalance_commit: [u8; 32],
    pub pending_rebalance_slot: u64,
    pub pending_rebalance_expiry: u64,
    pub bump: u8,
}

#[derive(Debug, Clone, BorshDeserialize)]
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

#[derive(Debug, Clone, BorshDeserialize)]
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
    T::deserialize(&mut payload).map_err(|e| anyhow!("borsh decode failed: {e}"))
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
) -> Instruction {
    Instruction {
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
        ),
    }
}

pub fn ix_emergency_shutdown(
    program_id: Pubkey,
    protocol_state: Pubkey,
    circuit_breaker: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new(circuit_breaker, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
        ],
        data: instruction_data_unit("emergency_shutdown"),
    }
}

pub fn ix_commit_rebalance(
    program_id: Pubkey,
    protocol_state: Pubkey,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    commit_hash: [u8; 32],
    valid_for_slots: u64,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(protocol_state, false),
            AccountMeta::new_readonly(keeper_one, true),
            AccountMeta::new_readonly(keeper_two, true),
        ],
        data: instruction_data(
            "commit_rebalance",
            &CommitRebalanceArgs {
                commit_hash,
                valid_for_slots,
            },
        ),
    }
}

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
) -> Instruction {
    Instruction {
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
        ),
    }
}

fn instruction_data<T: BorshSerialize>(name: &str, args: &T) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&anchor_discriminator("global", name));
    let mut payload = borsh::to_vec(args).expect("instruction args serializable");
    out.append(&mut payload);
    out
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
