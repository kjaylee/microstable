use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use tracing::{info, warn};

const SCALE: u64 = 1_000_000;

#[derive(Debug, Default, Clone)]
pub struct MonitorMemory {
    pub consecutive_emergency_cycles: u64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
// On-chain layout — all fields must be present for correct deserialization
pub struct MonitorOutcome {
    pub collateral_ratio_warnings: Vec<String>,
    pub circuit_breaker_triggered: bool,
    pub emergency_shutdown_signature: Option<Signature>,
    pub global_collateral_ratio_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorCrossRpcView {
    pub global_cr_bps: u64,
    pub protocol_total_supply: u64,
    pub protocol_emergency_shutdown: bool,
    pub protocol_keeper_set: [Pubkey; 3],
    pub circuit_status: [u8; 4],
    pub vault_total_deposits: [u64; 4],
    pub vault_prices: [u64; 4],
}

impl MonitorCrossRpcView {
    pub fn from_state(
        protocol: &wire::ProtocolState,
        circuit: &wire::CircuitBreakerState,
        vaults: &[wire::CollateralVault; 4],
        global_cr_bps: u64,
    ) -> Self {
        Self {
            global_cr_bps,
            protocol_total_supply: protocol.total_supply,
            protocol_emergency_shutdown: protocol.emergency_shutdown,
            protocol_keeper_set: protocol.keeper_set,
            circuit_status: circuit.status,
            vault_total_deposits: std::array::from_fn(|i| vaults[i].total_deposits),
            vault_prices: std::array::from_fn(|i| vaults[i].price),
        }
    }
}

pub fn validate_monitor_cross_rpc(
    primary: &MonitorCrossRpcView,
    secondary: &MonitorCrossRpcView,
) -> Result<()> {
    if !utils::within_u64_tolerance(
        primary.global_cr_bps,
        secondary.global_cr_bps,
        utils::CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "global_cr_bps mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.global_cr_bps,
            secondary.global_cr_bps,
            utils::CROSS_RPC_NUMERIC_TOLERANCE,
        ));
    }

    if !utils::within_u64_tolerance(
        primary.protocol_total_supply,
        secondary.protocol_total_supply,
        utils::CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.total_supply mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.protocol_total_supply,
            secondary.protocol_total_supply,
            utils::CROSS_RPC_NUMERIC_TOLERANCE,
        ));
    }

    if primary.protocol_emergency_shutdown != secondary.protocol_emergency_shutdown {
        return Err(anyhow!(
            "protocol.emergency_shutdown mismatch (primary={}, secondary={})",
            primary.protocol_emergency_shutdown,
            secondary.protocol_emergency_shutdown
        ));
    }

    if primary.protocol_keeper_set != secondary.protocol_keeper_set {
        return Err(anyhow!(
            "protocol.keeper_set mismatch (primary={:?}, secondary={:?})",
            primary.protocol_keeper_set,
            secondary.protocol_keeper_set
        ));
    }

    if primary.circuit_status != secondary.circuit_status {
        return Err(anyhow!(
            "circuit.status mismatch (primary={:?}, secondary={:?})",
            primary.circuit_status,
            secondary.circuit_status
        ));
    }

    for i in 0..4 {
        if !utils::within_u64_tolerance(
            primary.vault_total_deposits[i],
            secondary.vault_total_deposits[i],
            utils::CROSS_RPC_NUMERIC_TOLERANCE,
        ) {
            return Err(anyhow!(
                "vault.total_deposits[{i}] mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
                primary.vault_total_deposits[i],
                secondary.vault_total_deposits[i],
                utils::CROSS_RPC_NUMERIC_TOLERANCE,
            ));
        }

        if !utils::within_u64_tolerance(
            primary.vault_prices[i],
            secondary.vault_prices[i],
            utils::CROSS_RPC_NUMERIC_TOLERANCE,
        ) {
            return Err(anyhow!(
                "vault.price[{i}] mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
                primary.vault_prices[i],
                secondary.vault_prices[i],
                utils::CROSS_RPC_NUMERIC_TOLERANCE,
            ));
        }
    }

    Ok(())
}

pub fn run_monitor_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    memory: &mut MonitorMemory,
) -> Result<MonitorOutcome> {
    let secondary_for_reads = if secondary_mode.uses_secondary_reads() {
        secondary_rpc
    } else {
        None
    };

    let snapshot = if let Some(secondary) = secondary_for_reads {
        match utils::retry_with_backoff(
            utils::CROSS_RPC_MAX_ATTEMPTS,
            utils::CROSS_RPC_BACKOFF_BASE_MS,
            |attempt| {
                let primary_snapshot = fetch_monitor_snapshot(rpc, derived)?;
                let secondary_snapshot =
                    fetch_monitor_snapshot(secondary, derived).map_err(|err| {
                        let entered_degraded = utils::register_secondary_rpc_failure();
                        anyhow!(
                            "secondary monitor snapshot read failed (attempt {attempt}/{}): {err}; entered_degraded={entered_degraded}",
                            utils::CROSS_RPC_MAX_ATTEMPTS
                        )
                    })?;

                let primary_view = MonitorCrossRpcView::from_state(
                    &primary_snapshot.0,
                    &primary_snapshot.1,
                    &primary_snapshot.2,
                    primary_snapshot.3,
                );
                let secondary_view = MonitorCrossRpcView::from_state(
                    &secondary_snapshot.0,
                    &secondary_snapshot.1,
                    &secondary_snapshot.2,
                    secondary_snapshot.3,
                );

                if let Err(err) = validate_monitor_cross_rpc(&primary_view, &secondary_view) {
                    return Err(anyhow!(
                        "cross-RPC mismatch (attempt {attempt}/{}): {err}",
                        utils::CROSS_RPC_MAX_ATTEMPTS
                    ));
                }

                Ok(primary_snapshot)
            },
        ) {
            Ok(snapshot) => {
                let _ = utils::register_secondary_rpc_success();
                snapshot
            }
            Err(err) => {
                if memory.consecutive_emergency_cycles > 0 {
                    info!(
                        previous_consecutive_observations = memory.consecutive_emergency_cycles,
                        "debounce counter reset due to skipped cycle"
                    );
                } else {
                    info!("debounce counter reset due to skipped cycle");
                }
                memory.consecutive_emergency_cycles = 0;

                if utils::secondary_rpc_is_degraded() {
                    warn!(
                        error = %err,
                        "secondary RPC degraded during monitor read-path checks; falling back to primary-only mode"
                    );
                    fetch_monitor_snapshot(rpc, derived)?
                } else {
                    return Err(anyhow!(
                        "monitor cycle failed after cross-RPC retries: {err}"
                    ));
                }
            }
        }
    } else {
        fetch_monitor_snapshot(rpc, derived)?
    };

    let (protocol, circuit, vaults, global_cr_bps) = snapshot;

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

            let sig = utils::send_instructions(
                rpc,
                secondary_rpc,
                secondary_mode,
                k1,
                &[k1, k2],
                vec![ix],
            )?;
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

fn fetch_monitor_snapshot(
    rpc: &RpcClient,
    derived: &DerivedAccounts,
) -> Result<(
    wire::ProtocolState,
    wire::CircuitBreakerState,
    [wire::CollateralVault; 4],
    u64,
)> {
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
    Ok((protocol, circuit, vaults, global_cr_bps))
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
