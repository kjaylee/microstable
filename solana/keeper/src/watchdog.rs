use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use tracing::warn;

const WEIGHT_SCALE: u64 = 1_000_000;
const MEMO_PROGRAM_ID: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";

#[derive(Debug, Clone)]
pub struct AnomalyEvent {
    pub slot: u64,
    pub message: String,
}

#[derive(Debug, Default, Clone)]
pub struct WatchdogMemory {
    pub last_total_supply: Option<u64>,
    pub last_global_cr_bps: Option<u64>,
    pub last_protocol_update_slot: Option<u64>,
    pub last_weights: Option<[u64; 4]>,
    pub last_emergency_shutdown: Option<bool>,
    pub history: Vec<AnomalyEvent>,
}

#[derive(Debug, Clone)]
pub struct WatchdogOutcome {
    pub anomalies: Vec<String>,
    pub alert_signature: Option<Signature>,
}

pub fn run_watchdog_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    memory: &mut WatchdogMemory,
) -> Result<WatchdogOutcome> {
    let (protocol, _vaults, current_slot, global_cr_bps) = if let Some(secondary) = secondary_rpc {
        utils::retry_with_backoff(
            utils::CROSS_RPC_MAX_ATTEMPTS,
            utils::CROSS_RPC_BACKOFF_BASE_MS,
            |attempt| {
                let primary_snapshot = fetch_watchdog_snapshot(rpc, derived)?;
                let secondary_snapshot = fetch_watchdog_snapshot(secondary, derived)?;

                validate_watchdog_cross_rpc(
                    &primary_snapshot.0,
                    &secondary_snapshot.0,
                    &primary_snapshot.1,
                    &secondary_snapshot.1,
                    primary_snapshot.3,
                    secondary_snapshot.3,
                )
                .map_err(|err| {
                    anyhow!(
                        "watchdog cross-RPC mismatch (attempt {attempt}/{}): {err}",
                        utils::CROSS_RPC_MAX_ATTEMPTS
                    )
                })?;

                Ok(primary_snapshot)
            },
        )
        .map_err(|err| anyhow!("watchdog cycle failed after cross-RPC retries: {err}"))?
    } else {
        fetch_watchdog_snapshot(rpc, derived)?
    };

    let mut anomalies = Vec::new();

    let weight_sum: u64 = protocol.weights.iter().sum();
    if weight_sum.abs_diff(WEIGHT_SCALE) > 1 {
        anomalies.push(format!(
            "weight sum invariant violated: sum={}, expected≈{}",
            weight_sum, WEIGHT_SCALE
        ));
    }

    let oracle_stale_slots = current_slot.saturating_sub(protocol.last_update_slot);
    if oracle_stale_slots > cfg.watchdog_oracle_stale_slots {
        anomalies.push(format!(
            "oracle updates are stale: last_update_slot={}, current_slot={}, lag={} slots",
            protocol.last_update_slot, current_slot, oracle_stale_slots
        ));
    }

    if protocol.total_supply > 0 && global_cr_bps < cfg.min_collateral_ratio_bps {
        anomalies.push(format!(
            "global collateral ratio low: {} bps (< {} bps)",
            global_cr_bps, cfg.min_collateral_ratio_bps
        ));
    }

    if let Some(prev_supply) = memory.last_total_supply {
        let spike_bps = relative_change_bps(prev_supply, protocol.total_supply);
        if spike_bps >= cfg.watchdog_supply_spike_bps {
            anomalies.push(format!(
                "sudden supply change detected: {} -> {} ({} bps)",
                prev_supply, protocol.total_supply, spike_bps
            ));
        }
    }

    if let Some(prev_cr) = memory.last_global_cr_bps {
        if prev_cr > global_cr_bps {
            let drop_bps = relative_change_bps(prev_cr, global_cr_bps);
            if drop_bps >= cfg.watchdog_cr_drop_bps {
                anomalies.push(format!(
                    "sudden CR drop detected: {} -> {} ({} bps)",
                    prev_cr, global_cr_bps, drop_bps
                ));
            }
        }
    }

    if let Some(prev_weights) = memory.last_weights {
        let shift_bps = weight_shift_bps(prev_weights, protocol.weights);
        if shift_bps >= cfg.watchdog_weight_shift_bps {
            anomalies.push(format!(
                "large weight shift detected: {:?} -> {:?} ({} bps)",
                prev_weights, protocol.weights, shift_bps
            ));
        }
    }

    if let Some(prev_shutdown) = memory.last_emergency_shutdown {
        if prev_shutdown != protocol.emergency_shutdown {
            anomalies.push(format!(
                "emergency shutdown state changed: {} -> {}",
                prev_shutdown, protocol.emergency_shutdown
            ));
        }
    }

    if let Some(prev_update_slot) = memory.last_protocol_update_slot {
        if prev_update_slot == protocol.last_update_slot
            && oracle_stale_slots > cfg.watchdog_oracle_stale_slots
        {
            anomalies.push(format!(
                "protocol update slot unchanged across cycles while stale: {}",
                protocol.last_update_slot
            ));
        }
    }

    if !anomalies.is_empty() {
        for message in &anomalies {
            memory.history.push(AnomalyEvent {
                slot: current_slot,
                message: message.clone(),
            });
        }

        if memory.history.len() > cfg.watchdog_history_limit {
            let overflow = memory
                .history
                .len()
                .saturating_sub(cfg.watchdog_history_limit);
            memory.history.drain(0..overflow);
        }
    }

    memory.last_total_supply = Some(protocol.total_supply);
    memory.last_global_cr_bps = Some(global_cr_bps);
    memory.last_protocol_update_slot = Some(protocol.last_update_slot);
    memory.last_weights = Some(protocol.weights);
    memory.last_emergency_shutdown = Some(protocol.emergency_shutdown);

    let alert_signature = if !anomalies.is_empty() && cfg.send_watchdog_alert_tx {
        let recent_history: Vec<_> = memory
            .history
            .iter()
            .rev()
            .take(8)
            .map(|e| serde_json::json!({ "slot": e.slot, "message": e.message }))
            .collect();

        let payload = serde_json::json!({
            "kind": "watchdog_alert",
            "program_id": cfg.program_id.to_string(),
            "slot": current_slot,
            "anomalies": anomalies.clone(),
            "recent_history": recent_history,
        })
        .to_string();

        let (k1, _) = utils::keeper_quorum_for_protocol(keepers, &protocol.keeper_set)?;
        let memo_ix = build_memo_instruction(k1.pubkey(), payload.into_bytes())?;
        Some(utils::send_instructions(
            rpc,
            secondary_rpc,
            k1,
            &[k1],
            vec![memo_ix],
        )?)
    } else {
        None
    };

    if !anomalies.is_empty() {
        warn!(anomalies = ?anomalies, "watchdog detected anomalies");
    }

    Ok(WatchdogOutcome {
        anomalies,
        alert_signature,
    })
}

pub fn validate_watchdog_cross_rpc(
    primary_protocol: &wire::ProtocolState,
    secondary_protocol: &wire::ProtocolState,
    primary_vaults: &[wire::CollateralVault; 4],
    secondary_vaults: &[wire::CollateralVault; 4],
    primary_global_cr_bps: u64,
    secondary_global_cr_bps: u64,
) -> Result<()> {
    if !utils::within_u64_tolerance(
        primary_global_cr_bps,
        secondary_global_cr_bps,
        utils::CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "global collateral ratio mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary_global_cr_bps,
            secondary_global_cr_bps,
            utils::CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    utils::validate_protocol_state_with_tolerance(primary_protocol, secondary_protocol)?;
    utils::validate_vaults_with_tolerance(primary_vaults, secondary_vaults)?;
    Ok(())
}

fn fetch_watchdog_snapshot(
    rpc: &RpcClient,
    derived: &DerivedAccounts,
) -> Result<(wire::ProtocolState, [wire::CollateralVault; 4], u64, u64)> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];

    let current_slot = rpc.get_slot()?;
    let global_cr_bps = total_collateral_ratio_bps(&protocol, &vaults);
    Ok((protocol, vaults, current_slot, global_cr_bps))
}

fn total_collateral_ratio_bps(
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
                .saturating_div(WEIGHT_SCALE as u128)
        })
        .sum();

    ((total_value.saturating_mul(10_000)) / protocol.total_supply as u128) as u64
}

fn relative_change_bps(prev: u64, current: u64) -> u64 {
    if prev == 0 {
        return 0;
    }
    let delta = prev.abs_diff(current) as u128;
    ((delta.saturating_mul(10_000)) / prev as u128) as u64
}

fn weight_shift_bps(prev: [u64; 4], current: [u64; 4]) -> u64 {
    let l1 = (0..4)
        .map(|i| prev[i].abs_diff(current[i]) as u128)
        .sum::<u128>();
    ((l1.saturating_mul(10_000)) / WEIGHT_SCALE as u128) as u64
}

fn build_memo_instruction(signer: Pubkey, memo: Vec<u8>) -> Result<Instruction> {
    let program_id = utils::parse_pubkey(MEMO_PROGRAM_ID)?;
    Ok(Instruction {
        program_id,
        accounts: vec![AccountMeta::new_readonly(signer, true)],
        data: memo,
    })
}
