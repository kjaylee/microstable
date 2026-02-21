use anchor_lang::prelude::*;

pub const MAX_ASSETS: usize = 8;
pub const CIRCUIT_BREAKER_COUNT: usize = 4;
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const CR_HARD_MIN_BPS: u64 = 10_500;
pub const DELTA_MAX_BPS: u16 = 200;
pub const FEE_DELTA_MAX_BPS: u16 = 10;
pub const TARGET_CR_DELTA_MAX_BPS: u64 = 500;

pub const GLOBAL_STATE_SEED: &[u8] = b"global-state";
pub const BASKET_CONFIG_SEED: &[u8] = b"basket-config";
pub const CIRCUIT_STATE_SEED: &[u8] = b"circuit-state";
pub const UPDATE_PROPOSAL_SEED: &[u8] = b"update-proposal";

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProtocolMode {
    Normal,
    SafeMode,
    Frozen,
    RedeemOnly,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CBStatus {
    Normal,
    Active,
    Cooldown,
    ExtendedActive,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AssetConfig {
    pub mint: Pubkey,
    pub weight_bps: u16,
    pub weight_cap_bps: u16,
    pub risk_score: u16,
    pub oracle: Pubkey,
    pub vault: Pubkey,
}

impl AssetConfig {
    pub const LEN: usize = 32 + 2 + 2 + 2 + 32 + 32;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct CBEntry {
    pub status: CBStatus,
    pub activated_at: i64,
    pub activation_count_30: u8,
    pub last_recovery_at: i64,
}

impl Default for CBEntry {
    fn default() -> Self {
        Self {
            status: CBStatus::Normal,
            activated_at: 0,
            activation_count_30: 0,
            last_recovery_at: 0,
        }
    }
}

impl CBEntry {
    pub const LEN: usize = 1 + 8 + 1 + 8;

    pub fn is_active(&self) -> bool {
        matches!(self.status, CBStatus::Active | CBStatus::ExtendedActive)
    }
}

#[account]
pub struct GlobalState {
    pub authority: Pubkey,
    pub mode: ProtocolMode,
    pub target_cr: u64,
    pub mint_fee_bps: u16,
    pub redeem_fee_bps: u16,
    pub total_supply: u64,
    pub last_update_epoch: i64,
    pub version: u8,
    pub bump: u8,
}

impl GlobalState {
    pub const LEN: usize = 32 + 1 + 8 + 2 + 2 + 8 + 8 + 1 + 1;
}

#[account]
pub struct BasketConfig {
    pub assets: Vec<AssetConfig>,
    pub bump: u8,
}

impl BasketConfig {
    pub const LEN: usize = 4 + (MAX_ASSETS * AssetConfig::LEN) + 1;

    pub fn total_weight_bps(&self) -> u64 {
        self.assets.iter().map(|a| a.weight_bps as u64).sum::<u64>()
    }
}

#[account]
pub struct CircuitState {
    pub cb_states: [CBEntry; CIRCUIT_BREAKER_COUNT],
    pub bump: u8,
}

impl CircuitState {
    pub const LEN: usize = (CBEntry::LEN * CIRCUIT_BREAKER_COUNT) + 1;

    pub fn any_active(&self) -> bool {
        self.cb_states.iter().any(CBEntry::is_active)
    }

    pub fn is_active(&self, index: usize) -> bool {
        self.cb_states
            .get(index)
            .map(CBEntry::is_active)
            .unwrap_or(false)
    }
}

#[account]
pub struct UpdateProposal {
    pub proposer: Pubkey,
    pub new_weights_bps: Vec<u16>,
    pub new_target_cr: u64,
    pub new_mint_fee_bps: u16,
    pub new_redeem_fee_bps: u16,
    pub proposed_at: i64,
    pub bump: u8,
}

impl UpdateProposal {
    pub const LEN: usize = 32 + 4 + (MAX_ASSETS * 2) + 8 + 2 + 2 + 8 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AssetConfigInput {
    pub mint: Pubkey,
    pub weight_bps: u16,
    pub weight_cap_bps: u16,
    pub risk_score: u16,
    pub oracle: Pubkey,
    pub vault: Pubkey,
}

impl From<AssetConfigInput> for AssetConfig {
    fn from(value: AssetConfigInput) -> Self {
        Self {
            mint: value.mint,
            weight_bps: value.weight_bps,
            weight_cap_bps: value.weight_cap_bps,
            risk_score: value.risk_score,
            oracle: value.oracle,
            vault: value.vault,
        }
    }
}
