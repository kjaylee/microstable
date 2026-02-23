use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::{Keypair, Signature};

const SCALE: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Normal,
    Elevated,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedemptionPolicy {
    pub max_per_epoch: u64,
    pub delay_slots: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    HoldConservative,
    RelaxPartially,
    ResumeNormal,
}

#[derive(Debug, Clone)]
pub struct RiskManagerMemory {
    pub current_level: RiskLevel,
    pub recovery_epoch: u64,
    pub base_mint_fee_rate: Option<u64>,
    pub base_redeem_fee_rate: Option<u64>,
}

impl RiskManagerMemory {
    pub fn dynamic_fee_bases(&mut self, protocol: &wire::ProtocolState) -> (u64, u64) {
        let base_mint = *self
            .base_mint_fee_rate
            .get_or_insert(protocol.mint_fee_rate);
        let base_redeem = *self
            .base_redeem_fee_rate
            .get_or_insert(protocol.redeem_fee_rate);
        (base_mint, base_redeem)
    }
}

impl Default for RiskManagerMemory {
    fn default() -> Self {
        Self {
            current_level: RiskLevel::Normal,
            recovery_epoch: 0,
            base_mint_fee_rate: None,
            base_redeem_fee_rate: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiskManagerOutcome {
    pub risk_level: RiskLevel,
    pub previous_risk_level: Option<RiskLevel>,
    pub global_cr_bps: u64,
    pub throttle_redemptions: bool,
    pub redemption_policy: RedemptionPolicy,
    pub recovery_action: RecoveryAction,
    pub applied_param_signature: Option<Signature>,
}

pub fn run_risk_manager_cycle(
    rpc: &RpcClient,
    _secondary_rpc: Option<&RpcClient>,
    _secondary_mode: utils::SecondaryRpcMode,
    _cfg: &KeeperConfig,
    _keepers: &[Keypair],
    derived: &DerivedAccounts,
    memory: &mut RiskManagerMemory,
) -> Result<RiskManagerOutcome> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];

    let global_cr_bps = global_collateral_ratio_bps(&protocol, &vaults);
    let cr_ratio = global_cr_bps as f64 / 10_000.0;
    let cr_target = protocol.cr_target as f64 / SCALE as f64;

    let current_level = assess_risk_level(cr_ratio, cr_target);
    let previous_level = Some(memory.current_level);

    if current_level < memory.current_level {
        memory.recovery_epoch = memory.recovery_epoch.saturating_add(1);
    } else if current_level > memory.current_level {
        memory.recovery_epoch = 0;
    }

    let recovery_action =
        auto_recovery_step(current_level, memory.current_level, memory.recovery_epoch);
    memory.current_level = current_level;

    let throttle_redemptions = should_throttle_redemptions(current_level, 0);
    let redemption_policy = redemption_queue_policy(current_level);

    Ok(RiskManagerOutcome {
        risk_level: current_level,
        previous_risk_level: previous_level,
        global_cr_bps,
        throttle_redemptions,
        redemption_policy,
        recovery_action,
        applied_param_signature: None,
    })
}

pub fn assess_risk_level(cr_ratio: f64, cr_target: f64) -> RiskLevel {
    let current_pct = ratio_to_percent(cr_ratio);
    let _target_pct = ratio_to_percent(cr_target);

    if !current_pct.is_finite() || current_pct <= 0.0 {
        return RiskLevel::Critical;
    }

    if current_pct > 150.0 {
        RiskLevel::Normal
    } else if current_pct >= 120.0 {
        RiskLevel::Elevated
    } else if current_pct >= 110.0 {
        RiskLevel::High
    } else {
        RiskLevel::Critical
    }
}

pub fn compute_dynamic_fees(level: RiskLevel, base_mint: u32, base_redeem: u32) -> (u32, u32) {
    let (mint_mult_bps, redeem_mult_bps) = match level {
        RiskLevel::Normal => (10_000, 10_000),
        RiskLevel::Elevated => (11_000, 12_000),
        RiskLevel::High => (13_500, 15_000),
        RiskLevel::Critical => (17_500, 20_000),
    };

    (
        scale_fee(base_mint, mint_mult_bps),
        scale_fee(base_redeem, redeem_mult_bps),
    )
}

pub fn should_throttle_redemptions(level: RiskLevel, recent_volume: u64) -> bool {
    match level {
        RiskLevel::Normal => false,
        RiskLevel::Elevated => recent_volume >= 100_000,
        RiskLevel::High => recent_volume >= 50_000,
        RiskLevel::Critical => true,
    }
}

pub fn redemption_queue_policy(level: RiskLevel) -> RedemptionPolicy {
    match level {
        RiskLevel::Normal => RedemptionPolicy {
            max_per_epoch: u64::MAX,
            delay_slots: 0,
            enabled: false,
        },
        RiskLevel::Elevated => RedemptionPolicy {
            max_per_epoch: 750_000,
            delay_slots: 8,
            enabled: true,
        },
        RiskLevel::High => RedemptionPolicy {
            max_per_epoch: 300_000,
            delay_slots: 24,
            enabled: true,
        },
        RiskLevel::Critical => RedemptionPolicy {
            max_per_epoch: 100_000,
            delay_slots: 64,
            enabled: true,
        },
    }
}

pub fn auto_recovery_step(current: RiskLevel, previous: RiskLevel, epoch: u64) -> RecoveryAction {
    if matches!(current, RiskLevel::High | RiskLevel::Critical) {
        return RecoveryAction::HoldConservative;
    }

    match previous {
        RiskLevel::Critical => {
            if epoch < 2 {
                RecoveryAction::HoldConservative
            } else if epoch < 4 {
                RecoveryAction::RelaxPartially
            } else {
                RecoveryAction::ResumeNormal
            }
        }
        RiskLevel::High => {
            if epoch == 0 {
                RecoveryAction::RelaxPartially
            } else {
                RecoveryAction::ResumeNormal
            }
        }
        RiskLevel::Elevated => {
            if current == RiskLevel::Normal && epoch >= 1 {
                RecoveryAction::ResumeNormal
            } else {
                RecoveryAction::RelaxPartially
            }
        }
        RiskLevel::Normal => RecoveryAction::ResumeNormal,
    }
}

fn ratio_to_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return f64::NAN;
    }

    if value > 10.0 {
        value
    } else {
        value * 100.0
    }
}

fn scale_fee(base: u32, multiplier_bps: u32) -> u32 {
    ((base as u128)
        .saturating_mul(multiplier_bps as u128)
        .saturating_add(9_999)
        / 10_000) as u32
}

fn global_collateral_ratio_bps(
    protocol: &wire::ProtocolState,
    vaults: &[wire::CollateralVault; 4],
) -> u64 {
    if protocol.total_supply == 0 {
        return u64::MAX;
    }

    let total_value: u128 = vaults
        .iter()
        .map(|v| {
            (v.total_deposits as u128)
                .saturating_mul(v.price as u128)
                .saturating_div(SCALE as u128)
        })
        .sum();

    ((total_value.saturating_mul(10_000)) / protocol.total_supply as u128) as u64
}
