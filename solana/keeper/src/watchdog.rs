use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use tracing::warn;

const WEIGHT_SCALE: u64 = 1_000_000;
const MEMO_PROGRAM_ID: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";

#[derive(Debug, Default, Clone)]
pub struct WatchdogMemory {
    pub last_total_supply: Option<u64>,
    pub last_global_cr_bps: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WatchdogOutcome {
    pub anomalies: Vec<String>,
    pub alert_signature: Option<Signature>,
}

pub fn run_watchdog_cycle(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    memory: &mut WatchdogMemory,
) -> Result<WatchdogOutcome> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];

    let mut anomalies = Vec::new();

    let weight_sum: u64 = protocol.weights.iter().sum();
    if weight_sum.abs_diff(WEIGHT_SCALE) > 1 {
        anomalies.push(format!(
            "weight sum invariant violated: sum={}, expected≈{}",
            weight_sum, WEIGHT_SCALE
        ));
    }

    let global_cr_bps = total_collateral_ratio_bps(&protocol, &vaults);
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

    memory.last_total_supply = Some(protocol.total_supply);
    memory.last_global_cr_bps = Some(global_cr_bps);

    let alert_signature = if !anomalies.is_empty() && cfg.send_watchdog_alert_tx {
        let payload = serde_json::json!({
            "kind": "watchdog_alert",
            "program_id": cfg.program_id.to_string(),
            "anomalies": anomalies,
            "slot": rpc.get_slot()?,
        })
        .to_string();

        let (k1, _) = utils::keeper_quorum(keepers)?;
        let memo_ix = build_memo_instruction(k1.pubkey(), payload.into_bytes())?;
        Some(utils::send_instructions(rpc, k1, &[k1], vec![memo_ix])?)
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

fn total_collateral_ratio_bps(
    protocol: &wire::ProtocolState,
    vaults: &[wire::CollateralVault; 4],
) -> u64 {
    if protocol.total_supply == 0 {
        return u64::MAX;
    }

    let total_value: u128 = vaults
        .iter()
        .map(|v| (v.total_deposits as u128).saturating_mul(v.price as u128))
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

fn build_memo_instruction(signer: Pubkey, memo: Vec<u8>) -> Result<Instruction> {
    let program_id = utils::parse_pubkey(MEMO_PROGRAM_ID)?;
    Ok(Instruction {
        program_id,
        accounts: vec![AccountMeta::new_readonly(signer, true)],
        data: memo,
    })
}
