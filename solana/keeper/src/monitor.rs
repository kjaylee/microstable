use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct MonitorOutcome {
    pub collateral_ratio_warnings: Vec<String>,
    pub circuit_breaker_triggered: bool,
    pub emergency_shutdown_signature: Option<Signature>,
}

pub fn run_monitor_cycle(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
) -> Result<MonitorOutcome> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    let circuit: wire::CircuitBreakerState =
        utils::fetch_account(rpc, &derived.circuit_breaker, "CircuitBreakerState")?;

    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];

    let mut warnings = Vec::new();
    for (i, vault) in vaults.iter().enumerate() {
        let cr_bps = collateral_ratio_bps(vault, &protocol);
        if cr_bps < cfg.min_collateral_ratio_bps {
            let msg = format!(
                "vault[{i}] collateral ratio low: {cr_bps} bps (< {} bps)",
                cfg.min_collateral_ratio_bps
            );
            warn!(%msg);
            warnings.push(msg);
        }
    }

    let circuit_breaker_triggered = circuit.status.iter().any(|s| *s != 0);
    if circuit_breaker_triggered {
        warn!(status = ?circuit.status, "circuit breaker active");
    }

    let emergency_shutdown_signature =
        if circuit_breaker_triggered && cfg.auto_emergency_shutdown && !protocol.emergency_shutdown
        {
            let ix = build_emergency_shutdown_ix(cfg, derived, keepers);
            let (k1, k2) = utils::keeper_quorum(keepers)?;
            let sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![ix])?;
            info!(signature = %sig, "emergency_shutdown sent");
            Some(sig)
        } else {
            None
        };

    Ok(MonitorOutcome {
        collateral_ratio_warnings: warnings,
        circuit_breaker_triggered,
        emergency_shutdown_signature,
    })
}

fn collateral_ratio_bps(vault: &wire::CollateralVault, protocol: &wire::ProtocolState) -> u64 {
    if protocol.total_supply == 0 {
        return u64::MAX;
    }

    let collateral_value = (vault.total_deposits as u128).saturating_mul(vault.price as u128);
    let denom = protocol.total_supply as u128;
    ((collateral_value.saturating_mul(10_000)) / denom) as u64
}

fn build_emergency_shutdown_ix(
    cfg: &KeeperConfig,
    derived: &DerivedAccounts,
    keepers: &[Keypair],
) -> Instruction {
    wire::ix_emergency_shutdown(
        cfg.program_id,
        derived.protocol_state,
        derived.circuit_breaker,
        keepers[0].pubkey(),
        keepers[1].pubkey(),
    )
}
