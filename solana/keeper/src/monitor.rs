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

#[derive(Debug, Default, Clone)]
pub struct MonitorMemory {
    pub consecutive_emergency_cycles: u64,
}

#[derive(Debug, Clone)]
pub struct MonitorOutcome {
    pub collateral_ratio_warnings: Vec<String>,
    pub circuit_breaker_triggered: bool,
    pub emergency_shutdown_signature: Option<Signature>,
    pub global_collateral_ratio_bps: u64,
}

pub fn run_monitor_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    memory: &mut MonitorMemory,
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

    let global_cr_bps = global_collateral_ratio_bps(&protocol, &vaults);

    if let Some(secondary) = secondary_rpc {
        let secondary_protocol: wire::ProtocolState =
            utils::fetch_account(secondary, &derived.protocol_state, "ProtocolState")?;
        let secondary_vaults = [
            utils::fetch_account::<wire::CollateralVault>(
                secondary,
                &derived.vaults[0],
                "CollateralVault",
            )?,
            utils::fetch_account::<wire::CollateralVault>(
                secondary,
                &derived.vaults[1],
                "CollateralVault",
            )?,
            utils::fetch_account::<wire::CollateralVault>(
                secondary,
                &derived.vaults[2],
                "CollateralVault",
            )?,
            utils::fetch_account::<wire::CollateralVault>(
                secondary,
                &derived.vaults[3],
                "CollateralVault",
            )?,
        ];
        let secondary_global_cr_bps = global_collateral_ratio_bps(&secondary_protocol, &secondary_vaults);

        if global_cr_bps != secondary_global_cr_bps {
            warn!(
                primary_global_cr_bps = global_cr_bps,
                secondary_global_cr_bps,
                "monitor cycle skipped: cross-RPC mismatch on collateral ratio"
            );
            return Ok(MonitorOutcome {
                collateral_ratio_warnings: Vec::new(),
                circuit_breaker_triggered: false,
                emergency_shutdown_signature: None,
                global_collateral_ratio_bps: global_cr_bps,
            });
        }
    }

    let mut warnings = Vec::new();

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

    let emergency_shutdown_signature = if emergency_condition {
        memory.consecutive_emergency_cycles = memory.consecutive_emergency_cycles.saturating_add(1);
        warn!(
            observed_global_cr_bps = global_cr_bps,
            emergency_threshold_bps = cfg.emergency_collateral_ratio_bps,
            consecutive_observations = memory.consecutive_emergency_cycles,
            required_observations = cfg.emergency_debounce_cycles,
            "emergency threshold breached"
        );

        if cfg.auto_emergency_shutdown
            && memory.consecutive_emergency_cycles >= cfg.emergency_debounce_cycles
        {
            let (k1, k2) = utils::keeper_quorum_for_protocol(keepers, &protocol.keeper_set)?;
            let ix = wire::ix_emergency_shutdown(
                cfg.program_id,
                derived.protocol_state,
                derived.circuit_breaker,
                k1.pubkey(),
                k2.pubkey(),
            )?;

            let sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![ix])?;
            warn!(
                signature = %sig,
                global_cr_bps,
                emergency_threshold_bps = cfg.emergency_collateral_ratio_bps,
                consecutive_observations = memory.consecutive_emergency_cycles,
                "emergency_shutdown sent after debounce threshold"
            );
            Some(sig)
        } else {
            if !cfg.auto_emergency_shutdown {
                warn!(
                    global_cr_bps,
                    emergency_threshold_bps = cfg.emergency_collateral_ratio_bps,
                    "emergency condition detected, but auto_emergency_shutdown is disabled"
                );
            }
            None
        }
    } else {
        if memory.consecutive_emergency_cycles > 0 {
            info!(
                previous_consecutive_observations = memory.consecutive_emergency_cycles,
                "emergency debounce counter reset"
            );
        }
        memory.consecutive_emergency_cycles = 0;
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
