use crate::{
    config::KeeperConfig,
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    hash::hashv,
    signature::{Keypair, Signature, Signer},
};
use std::{thread, time::Duration};
use tracing::{info, warn};

const WEIGHT_SCALE: u64 = 1_000_000;
const BATCH_WINDOW_SLOTS: u64 = 32;

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

    let mut outcome = RebalanceOutcome {
        proposed: false,
        deviation_bps: 0,
        target_weights: protocol.weights,
        commit_signature: None,
        rebalance_signature: None,
    };

    if protocol.emergency_shutdown {
        warn!("rebalance skipped: protocol is in emergency shutdown");
        return Ok(outcome);
    }

    let (k1, k2) = utils::keeper_quorum(keepers)?;
    let mut current_slot = rpc.get_slot()?;

    let target_weights = compute_target_weights(&vaults);
    let deviation_bps = weight_deviation_bps(protocol.weights, target_weights);

    outcome.deviation_bps = deviation_bps;
    outcome.target_weights = target_weights;

    if protocol.pending_rebalance_commit != [0u8; 32] {
        if current_slot > protocol.pending_rebalance_expiry {
            warn!(
                pending_slot = protocol.pending_rebalance_slot,
                pending_expiry = protocol.pending_rebalance_expiry,
                current_slot,
                "existing rebalance commit is expired"
            );
        } else if current_slot
            < protocol
                .pending_rebalance_slot
                .saturating_add(cfg.commit_reveal_delay_slots)
        {
            info!(
                pending_slot = protocol.pending_rebalance_slot,
                current_slot,
                required_slot = protocol
                    .pending_rebalance_slot
                    .saturating_add(cfg.commit_reveal_delay_slots),
                "pending rebalance commit not yet revealable"
            );
        } else if let Some((batch_slot, reveal_salt)) = recover_reveal_preimage(
            derived.protocol_state,
            protocol.pending_rebalance_commit,
            protocol.pending_rebalance_slot,
            protocol.pending_rebalance_expiry,
            current_slot,
            target_weights,
        ) {
            let rebalance_ix = wire::ix_rebalance(
                cfg.program_id,
                derived.protocol_state,
                derived.circuit_breaker,
                derived.vaults,
                k1.pubkey(),
                k2.pubkey(),
                target_weights,
                cfg.max_rebalance_slippage_bps,
                batch_slot,
                reveal_salt,
            );

            match utils::send_instructions(rpc, k1, &[k1, k2], vec![rebalance_ix]) {
                Ok(sig) => {
                    info!(
                        signature = %sig,
                        batch_slot,
                        deviation_bps,
                        target_weights = ?target_weights,
                        "rebalance reveal sent for pending commit"
                    );
                    outcome.proposed = true;
                    outcome.rebalance_signature = Some(sig);
                    return Ok(outcome);
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        batch_slot,
                        "rebalance reveal failed for pending commit"
                    );
                    return Ok(outcome);
                }
            }
        } else {
            warn!(
                pending_slot = protocol.pending_rebalance_slot,
                current_slot, "cannot reconstruct reveal preimage for pending commit"
            );
        }
    }

    if deviation_bps <= cfg.rebalance_deviation_bps {
        return Ok(outcome);
    }

    if protocol.pending_rebalance_commit != [0u8; 32]
        && current_slot <= protocol.pending_rebalance_expiry
    {
        warn!(
            pending_slot = protocol.pending_rebalance_slot,
            pending_expiry = protocol.pending_rebalance_expiry,
            current_slot,
            "skipping new commit because an active pending commit already exists"
        );
        return Ok(outcome);
    }

    let batch_slot = select_batch_slot(current_slot, cfg.commit_reveal_delay_slots);
    let reveal_salt = build_reveal_salt(batch_slot);
    let commit_hash = compute_rebalance_commit(
        derived.protocol_state,
        target_weights,
        batch_slot,
        reveal_salt,
    );

    let commit_ix = wire::ix_commit_rebalance(
        cfg.program_id,
        derived.protocol_state,
        k1.pubkey(),
        k2.pubkey(),
        commit_hash,
        cfg.commit_valid_for_slots,
    );

    let commit_sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![commit_ix])?;
    info!(
        deviation_bps,
        signature = %commit_sig,
        batch_slot,
        target_weights = ?target_weights,
        "rebalance commit sent"
    );

    outcome.proposed = true;
    outcome.commit_signature = Some(commit_sig);

    if !cfg.execute_rebalance_immediately {
        return Ok(outcome);
    }

    let ready_slot = current_slot.saturating_add(cfg.commit_reveal_delay_slots);
    current_slot = wait_until_slot(rpc, ready_slot, 30)?;

    if current_slot / BATCH_WINDOW_SLOTS != batch_slot / BATCH_WINDOW_SLOTS {
        warn!(
            current_slot,
            batch_slot, "skipping immediate reveal: moved outside batch window"
        );
        return Ok(outcome);
    }

    let rebalance_ix = wire::ix_rebalance(
        cfg.program_id,
        derived.protocol_state,
        derived.circuit_breaker,
        derived.vaults,
        k1.pubkey(),
        k2.pubkey(),
        target_weights,
        cfg.max_rebalance_slippage_bps,
        batch_slot,
        reveal_salt,
    );

    match utils::send_instructions(rpc, k1, &[k1, k2], vec![rebalance_ix]) {
        Ok(sig) => {
            info!(
                signature = %sig,
                batch_slot,
                target_weights = ?target_weights,
                "rebalance reveal sent"
            );
            outcome.rebalance_signature = Some(sig);
        }
        Err(err) => {
            warn!(
                error = %err,
                batch_slot,
                "rebalance reveal failed; commit remains active"
            );
        }
    }

    Ok(outcome)
}

fn compute_target_weights(vaults: &[wire::CollateralVault; 4]) -> [u64; 4] {
    let mut collateral_values = [0u128; 4];
    for (i, vault) in vaults.iter().enumerate() {
        let value = (vault.total_deposits as u128)
            .saturating_mul(vault.price as u128)
            .saturating_div(WEIGHT_SCALE as u128);
        collateral_values[i] = value;
    }

    let total_value = collateral_values
        .iter()
        .fold(0u128, |acc, v| acc.saturating_add(*v));
    if total_value == 0 {
        return [250_000, 250_000, 250_000, 250_000];
    }

    let mut risk_adjusted_scores = [0u128; 4];
    for i in 0..4 {
        let ratio_ppm = collateral_values[i]
            .saturating_mul(WEIGHT_SCALE as u128)
            .saturating_div(total_value);
        let risk_discount_ppm =
            WEIGHT_SCALE.saturating_sub(vaults[i].risk_score.min(WEIGHT_SCALE.saturating_sub(1)));
        risk_adjusted_scores[i] = ratio_ppm
            .saturating_mul(risk_discount_ppm as u128)
            .saturating_div(WEIGHT_SCALE as u128);
    }

    let mut weights = normalize_scores(risk_adjusted_scores);
    apply_weight_caps(&mut weights, vaults);
    weights
}

fn normalize_scores(scores: [u128; 4]) -> [u64; 4] {
    let total = scores.iter().fold(0u128, |acc, v| acc.saturating_add(*v));
    if total == 0 {
        return [250_000, 250_000, 250_000, 250_000];
    }

    let mut weights = [0u64; 4];
    let mut assigned = 0u64;
    for i in 0..3 {
        let w = scores[i]
            .saturating_mul(WEIGHT_SCALE as u128)
            .saturating_div(total) as u64;
        weights[i] = w;
        assigned = assigned.saturating_add(w);
    }
    weights[3] = WEIGHT_SCALE.saturating_sub(assigned);
    weights
}

fn apply_weight_caps(weights: &mut [u64; 4], vaults: &[wire::CollateralVault; 4]) {
    let mut capped = [false; 4];
    let mut remaining = WEIGHT_SCALE;

    for i in 0..4 {
        let cap = vaults[i].weight_cap.min(WEIGHT_SCALE);
        if weights[i] > cap {
            weights[i] = cap;
            capped[i] = true;
        }
        remaining = remaining.saturating_sub(weights[i]);
    }

    if remaining == 0 {
        rebalance_tail(weights);
        return;
    }

    let uncapped_total: u64 = (0..4).filter(|i| !capped[*i]).map(|i| weights[i]).sum();

    if uncapped_total == 0 {
        weights[3] = weights[3].saturating_add(remaining);
        rebalance_tail(weights);
        return;
    }

    let mut redistributed = 0u64;
    for i in 0..3 {
        if capped[i] {
            continue;
        }
        let add = (remaining as u128)
            .saturating_mul(weights[i] as u128)
            .saturating_div(uncapped_total as u128) as u64;
        weights[i] = weights[i].saturating_add(add);
        redistributed = redistributed.saturating_add(add);
    }
    weights[3] = weights[3].saturating_add(remaining.saturating_sub(redistributed));

    for i in 0..4 {
        let cap = vaults[i].weight_cap.min(WEIGHT_SCALE);
        if weights[i] > cap {
            weights[i] = cap;
        }
    }
    rebalance_tail(weights);
}

fn rebalance_tail(weights: &mut [u64; 4]) {
    let assigned = weights[0]
        .saturating_add(weights[1])
        .saturating_add(weights[2]);
    weights[3] = WEIGHT_SCALE.saturating_sub(assigned);
}

fn weight_deviation_bps(current: [u64; 4], target: [u64; 4]) -> u64 {
    let l1 = (0..4)
        .map(|i| current[i].abs_diff(target[i]) as u128)
        .fold(0u128, |acc, x| acc.saturating_add(x));
    ((l1.saturating_mul(10_000)) / WEIGHT_SCALE as u128) as u64
}

fn select_batch_slot(current_slot: u64, reveal_delay_slots: u64) -> u64 {
    current_slot.saturating_add(reveal_delay_slots)
}

fn wait_until_slot(rpc: &RpcClient, target_slot: u64, max_wait_secs: u64) -> Result<u64> {
    let mut waited_ms = 0u64;
    let sleep_ms = 400u64;

    loop {
        let slot = rpc.get_slot()?;
        if slot >= target_slot {
            return Ok(slot);
        }

        if waited_ms >= max_wait_secs.saturating_mul(1_000) {
            return Ok(slot);
        }

        thread::sleep(Duration::from_millis(sleep_ms));
        waited_ms = waited_ms.saturating_add(sleep_ms);
    }
}

fn recover_reveal_preimage(
    protocol_key: solana_sdk::pubkey::Pubkey,
    pending_commit: [u8; 32],
    pending_slot: u64,
    pending_expiry: u64,
    current_slot: u64,
    target_weights: [u64; 4],
) -> Option<(u64, [u8; 32])> {
    if pending_commit == [0u8; 32] {
        return None;
    }

    let search_start = pending_slot.saturating_sub(BATCH_WINDOW_SLOTS);
    let search_end = pending_expiry.min(current_slot.saturating_add(BATCH_WINDOW_SLOTS));

    for batch_slot in search_start..=search_end {
        if current_slot / BATCH_WINDOW_SLOTS != batch_slot / BATCH_WINDOW_SLOTS {
            continue;
        }

        let reveal_salt = build_reveal_salt(batch_slot);
        let candidate =
            compute_rebalance_commit(protocol_key, target_weights, batch_slot, reveal_salt);
        if candidate == pending_commit {
            return Some((batch_slot, reveal_salt));
        }
    }

    None
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
