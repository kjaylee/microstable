use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    hash::hashv,
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};
use tracing::{info, warn};

const WEIGHT_SCALE: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct RebalanceOutcome {
    pub proposed: bool,
    pub deviation_bps: u64,
    pub target_weights: [u64; 4],
    pub commit_signature: Option<Signature>,
    pub rebalance_signature: Option<Signature>,
}

pub fn run_rebalance_cycle(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
) -> Result<RebalanceOutcome> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;

    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];

    let target_weights = compute_target_weights(&vaults);
    let deviation_bps = weight_deviation_bps(protocol.weights, target_weights);

    if deviation_bps <= cfg.rebalance_deviation_bps {
        return Ok(RebalanceOutcome {
            proposed: false,
            deviation_bps,
            target_weights,
            commit_signature: None,
            rebalance_signature: None,
        });
    }

    let batch_slot = rpc.get_slot()?;
    let reveal_salt = build_reveal_salt(batch_slot);
    let commit_hash = compute_rebalance_commit(
        derived.protocol_state,
        target_weights,
        batch_slot,
        reveal_salt,
    );

    let (k1, k2) = utils::keeper_quorum(keepers)?;
    let commit_ix = build_commit_ix(cfg, derived, keepers, commit_hash);
    let commit_sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![commit_ix])?;

    info!(
        deviation_bps,
        signature = %commit_sig,
        weights = ?target_weights,
        "rebalance commit sent"
    );

    if !cfg.execute_rebalance_immediately {
        return Ok(RebalanceOutcome {
            proposed: true,
            deviation_bps,
            target_weights,
            commit_signature: Some(commit_sig),
            rebalance_signature: None,
        });
    }

    let rebalance_ix = build_rebalance_ix(
        cfg,
        derived,
        keepers,
        target_weights,
        batch_slot,
        reveal_salt,
    );

    let rebalance_sig = match utils::send_instructions(rpc, k1, &[k1, k2], vec![rebalance_ix]) {
        Ok(sig) => Some(sig),
        Err(err) => {
            warn!(error = %err, "rebalance send failed (commit preserved for later reveal window)");
            None
        }
    };

    Ok(RebalanceOutcome {
        proposed: true,
        deviation_bps,
        target_weights,
        commit_signature: Some(commit_sig),
        rebalance_signature: rebalance_sig,
    })
}

fn compute_target_weights(vaults: &[wire::CollateralVault; 4]) -> [u64; 4] {
    let mut values = [0u128; 4];
    for (i, vault) in vaults.iter().enumerate() {
        values[i] = (vault.total_deposits as u128).saturating_mul(vault.price as u128);
    }
    let total = values.iter().fold(0u128, |acc, v| acc.saturating_add(*v));
    if total == 0 {
        return [250_000, 250_000, 250_000, 250_000];
    }

    let mut weights = [0u64; 4];
    let mut assigned = 0u64;

    for i in 0..3 {
        let w = ((values[i].saturating_mul(WEIGHT_SCALE as u128)) / total) as u64;
        weights[i] = w;
        assigned = assigned.saturating_add(w);
    }
    weights[3] = WEIGHT_SCALE.saturating_sub(assigned);
    weights
}

fn weight_deviation_bps(current: [u64; 4], target: [u64; 4]) -> u64 {
    let l1 = (0..4)
        .map(|i| current[i].abs_diff(target[i]) as u128)
        .fold(0u128, |acc, x| acc.saturating_add(x));
    ((l1.saturating_mul(10_000)) / WEIGHT_SCALE as u128) as u64
}

fn build_commit_ix(
    cfg: &KeeperConfig,
    derived: &DerivedAccounts,
    keepers: &[Keypair],
    commit_hash: [u8; 32],
) -> Instruction {
    wire::ix_commit_rebalance(
        cfg.program_id,
        derived.protocol_state,
        keepers[0].pubkey(),
        keepers[1].pubkey(),
        commit_hash,
        cfg.commit_valid_for_slots,
    )
}

fn build_rebalance_ix(
    cfg: &KeeperConfig,
    derived: &DerivedAccounts,
    keepers: &[Keypair],
    new_weights: [u64; 4],
    batch_slot: u64,
    reveal_salt: [u8; 32],
) -> Instruction {
    wire::ix_rebalance(
        cfg.program_id,
        derived.protocol_state,
        derived.circuit_breaker,
        derived.vaults,
        keepers[0].pubkey(),
        keepers[1].pubkey(),
        new_weights,
        cfg.max_rebalance_slippage_bps,
        batch_slot,
        reveal_salt,
    )
}

fn build_reveal_salt(batch_slot: u64) -> [u8; 32] {
    let mut reveal_salt = [0u8; 32];
    reveal_salt[..8].copy_from_slice(&batch_slot.to_le_bytes());
    reveal_salt[8..16].copy_from_slice(&batch_slot.rotate_left(17).to_le_bytes());
    reveal_salt[16..24].copy_from_slice(&batch_slot.rotate_left(33).to_le_bytes());
    reveal_salt[24..32].copy_from_slice(&batch_slot.rotate_left(49).to_le_bytes());
    reveal_salt
}

fn compute_rebalance_commit(
    protocol_key: solana_sdk::pubkey::Pubkey,
    new_weights: [u64; 4],
    batch_slot: u64,
    reveal_salt: [u8; 32],
) -> [u8; 32] {
    let mut weights_bytes = [0u8; 32];
    for (i, weight) in new_weights.iter().enumerate() {
        let start = i * 8;
        weights_bytes[start..start + 8].copy_from_slice(&weight.to_le_bytes());
    }

    hashv(&[
        b"rebalance_commit_v1",
        protocol_key.as_ref(),
        &weights_bytes,
        &batch_slot.to_le_bytes(),
        &reveal_salt,
    ])
    .to_bytes()
}
