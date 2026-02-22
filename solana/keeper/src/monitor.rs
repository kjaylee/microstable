use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::{Keypair, Signature, Signer};
use tracing::{info, warn};

const SCALE: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct MonitorOutcome {
    pub collateral_ratio_warnings: Vec<String>,
    pub circuit_breaker_triggered: bool,
    pub emergency_shutdown_signature: Option<Signature>,
    pub global_collateral_ratio_bps: u64,
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

    let global_cr_bps = global_collateral_ratio_bps(&protocol, &vaults);
    if protocol.total_supply > 0 && global_cr_bps < cfg.min_collateral_ratio_bps {
        let msg = format!(
            "global collateral ratio low: {global_cr_bps} bps (< {} bps)",
            cfg.min_collateral_ratio_bps
        );
        warn!(%msg);
        warnings.push(msg);
    }

    for (i, vault) in vaults.iter().enumerate() {
        let vault_cr_bps = collateral_ratio_bps(vault, &protocol);
        if vault_cr_bps < cfg.min_collateral_ratio_bps {
            let msg = format!(
                "vault[{i}] collateral ratio low: {vault_cr_bps} bps (< {} bps)",
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

    let emergency_condition = protocol.total_supply > 0
        && global_cr_bps < cfg.emergency_collateral_ratio_bps
        && !protocol.emergency_shutdown;

    let emergency_shutdown_signature = if emergency_condition && cfg.auto_emergency_shutdown {
        let (k1, k2) = utils::keeper_quorum(keepers)?;
        let ix = wire::ix_emergency_shutdown(
            cfg.program_id,
            derived.protocol_state,
            derived.circuit_breaker,
            k1.pubkey(),
            k2.pubkey(),
        );

        let sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![ix])?;
        warn!(
            signature = %sig,
            global_cr_bps,
            emergency_threshold_bps = cfg.emergency_collateral_ratio_bps,
            "emergency_shutdown sent due to low collateral ratio"
        );
        Some(sig)
    } else {
        if emergency_condition {
            warn!(
                global_cr_bps,
                emergency_threshold_bps = cfg.emergency_collateral_ratio_bps,
                "emergency condition detected, but auto_emergency_shutdown is disabled"
            );
        }
        None
    };

    if !warnings.is_empty() {
        info!(
            count = warnings.len(),
            global_cr_bps, "monitor collected warnings"
        );
    }

    Ok(MonitorOutcome {
        collateral_ratio_warnings: warnings,
        circuit_breaker_triggered,
        emergency_shutdown_signature,
        global_collateral_ratio_bps: global_cr_bps,
    })
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

fn collateral_ratio_bps(vault: &wire::CollateralVault, protocol: &wire::ProtocolState) -> u64 {
    if protocol.total_supply == 0 {
        return u64::MAX;
    }

    let collateral_value = (vault.total_deposits as u128)
        .saturating_mul(vault.price as u128)
        .saturating_div(SCALE as u128);

    ((collateral_value.saturating_mul(10_000)) / protocol.total_supply as u128) as u64
}
