use crate::{
    config::KeeperConfig,
    optimizer::{self, AdamOptimizer, OptimizerCheckpoint, ParamVector, SafetyBounds},
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    hash::hashv,
    signature::{Keypair, Signature, Signer},
};
use std::{thread, time::Duration};
use tracing::{info, warn};

const WEIGHT_SCALE: u64 = 1_000_000;
const BATCH_WINDOW_SLOTS: u64 = 32;
const MIN_WEIGHT_CAP_PPM: u64 = 10_000;
const MAX_WEIGHT_CAP_PPM: u64 = WEIGHT_SCALE;
const CR_TARGET_MIN_PPM: u64 = 1_000_000;
const CR_TARGET_MAX_PPM: u64 = 2_000_000;
const FEE_MAX_PPM: u64 = 10_000;
const DEFAULT_CR_TARGET_PPM: u64 = 1_200_000;
const DEFAULT_FEE_PPM: u64 = 2_000;

#[derive(Debug, Clone)]
pub struct RebalanceOutcome {
    pub proposed: bool,
    pub deviation_bps: u64,
    pub target_weights: [u64; 4],
    pub commit_signature: Option<Signature>,
    pub rebalance_signature: Option<Signature>,
}

#[derive(Debug, Default, Clone)]
pub struct RebalanceMemory {
    pending_reveal: Option<PendingReveal>,
    adam_optimizer: Option<AdamOptimizer>,
    optimizer_checkpoint: Option<OptimizerCheckpoint>,
    safety_bounds: Option<SafetyBounds>,
}

#[derive(Debug, Clone)]
struct PendingReveal {
    commit_hash: [u8; 32],
    target_weights: [u64; 4],
    batch_slot: u64,
    reveal_salt: [u8; 32],
    pending_params: Option<ProtocolParamUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProtocolParamUpdate {
    target_cr: u64,
    mint_fee: u64,
    redeem_fee: u64,
}

pub fn run_rebalance_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
    memory: &mut RebalanceMemory,
) -> Result<RebalanceOutcome> {
    let secondary_for_reads = if secondary_mode.uses_secondary_reads() {
        secondary_rpc
    } else {
        None
    };

    let (protocol, circuit_breaker, vaults) = if let Some(secondary) = secondary_for_reads {
        match utils::retry_with_backoff(
            utils::CROSS_RPC_MAX_ATTEMPTS,
            utils::CROSS_RPC_BACKOFF_BASE_MS,
            |attempt| {
                let primary_snapshot = fetch_rebalance_snapshot(rpc, derived)?;
                let secondary_snapshot =
                    fetch_rebalance_snapshot(secondary, derived).map_err(|err| {
                        let entered_degraded = utils::register_secondary_rpc_failure();
                        anyhow!(
                            "secondary rebalance snapshot read failed (attempt {attempt}/{}): {err}; entered_degraded={entered_degraded}",
                            utils::CROSS_RPC_MAX_ATTEMPTS
                        )
                    })?;

                validate_rebalance_cross_rpc(
                    &primary_snapshot.0,
                    &secondary_snapshot.0,
                    &primary_snapshot.1,
                    &secondary_snapshot.1,
                    &primary_snapshot.2,
                    &secondary_snapshot.2,
                )
                .map_err(|err| {
                    anyhow!(
                        "rebalance cross-RPC mismatch (attempt {attempt}/{}): {err}",
                        utils::CROSS_RPC_MAX_ATTEMPTS
                    )
                })?;

                Ok(primary_snapshot)
            },
        ) {
            Ok(snapshot) => {
                let _ = utils::register_secondary_rpc_success();
                snapshot
            }
            Err(err) => {
                if utils::secondary_rpc_is_degraded() {
                    warn!(
                        error = %err,
                        "secondary RPC degraded during rebalance read-path checks; falling back to primary-only mode"
                    );
                    fetch_rebalance_snapshot(rpc, derived)?
                } else {
                    return Err(anyhow!(
                        "rebalance cycle failed after cross-RPC retries: {err}"
                    ));
                }
            }
        }
    } else {
        fetch_rebalance_snapshot(rpc, derived)?
    };

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

    let (k1, k2) = utils::keeper_quorum_for_protocol(keepers, &protocol.keeper_set)?;
    let mut current_slot = rpc.get_slot()?;

    let (target_weights, pending_params) =
        compute_rebalance_targets(&vaults, &protocol, &circuit_breaker, memory);
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
            if memory
                .pending_reveal
                .as_ref()
                .is_some_and(|p| p.commit_hash == protocol.pending_rebalance_commit)
            {
                memory.pending_reveal = None;
            }
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
        } else if let Some(local_pending) = memory.pending_reveal.clone().filter(|pending| {
            pending.commit_hash == protocol.pending_rebalance_commit
                && pending.batch_slot == protocol.pending_rebalance_slot
        }) {
            let rebalance_ix = wire::ix_rebalance(
                cfg.program_id,
                derived.protocol_state,
                derived.circuit_breaker,
                derived.vaults,
                k1.pubkey(),
                k2.pubkey(),
                local_pending.target_weights,
                cfg.max_rebalance_slippage_bps,
                local_pending.batch_slot,
                local_pending.reveal_salt,
            )?;

            match utils::send_instructions(
                rpc,
                secondary_rpc,
                secondary_mode,
                k1,
                &[k1, k2],
                vec![rebalance_ix],
            ) {
                Ok(sig) => {
                    info!(
                        signature = %sig,
                        batch_slot = local_pending.batch_slot,
                        deviation_bps,
                        target_weights = ?local_pending.target_weights,
                        "rebalance reveal sent for pending commit"
                    );
                    outcome.proposed = true;
                    outcome.rebalance_signature = Some(sig);
                    if let Some(params) = local_pending.pending_params {
                        maybe_submit_protocol_params_update(
                            rpc,
                            secondary_rpc,
                            secondary_mode,
                            cfg,
                            derived,
                            k1,
                            k2,
                            &protocol,
                            params,
                        );
                    }
                    memory.pending_reveal = None;
                    return Ok(outcome);
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        batch_slot = local_pending.batch_slot,
                        "rebalance reveal failed for pending commit"
                    );
                    return Ok(outcome);
                }
            }
        } else {
            warn!(
                pending_slot = protocol.pending_rebalance_slot,
                current_slot,
                "cannot reveal pending commit: missing ephemeral preimage in keeper memory"
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
    let reveal_salt = build_reveal_salt();
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
    )?;

    let commit_sig = utils::send_instructions(
        rpc,
        secondary_rpc,
        secondary_mode,
        k1,
        &[k1, k2],
        vec![commit_ix],
    )?;
    info!(
        deviation_bps,
        signature = %commit_sig,
        batch_slot,
        target_weights = ?target_weights,
        "rebalance commit sent"
    );

    memory.pending_reveal = Some(PendingReveal {
        commit_hash,
        target_weights,
        batch_slot,
        reveal_salt,
        pending_params,
    });

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
    )?;

    match utils::send_instructions(
        rpc,
        secondary_rpc,
        secondary_mode,
        k1,
        &[k1, k2],
        vec![rebalance_ix],
    ) {
        Ok(sig) => {
            info!(
                signature = %sig,
                batch_slot,
                target_weights = ?target_weights,
                "rebalance reveal sent"
            );
            outcome.rebalance_signature = Some(sig);
            if let Some(params) = pending_params {
                maybe_submit_protocol_params_update(
                    rpc,
                    secondary_rpc,
                    secondary_mode,
                    cfg,
                    derived,
                    k1,
                    k2,
                    &protocol,
                    params,
                );
            }
            memory.pending_reveal = None;
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

pub fn validate_rebalance_cross_rpc(
    primary_protocol: &wire::ProtocolState,
    secondary_protocol: &wire::ProtocolState,
    primary_circuit: &wire::CircuitBreakerState,
    secondary_circuit: &wire::CircuitBreakerState,
    primary_vaults: &[wire::CollateralVault; 4],
    secondary_vaults: &[wire::CollateralVault; 4],
) -> Result<()> {
    validate_vault_weight_caps(primary_vaults)?;
    validate_vault_weight_caps(secondary_vaults)?;
    utils::validate_protocol_state_with_tolerance(primary_protocol, secondary_protocol)?;
    utils::validate_vaults_with_tolerance(primary_vaults, secondary_vaults)?;

    if primary_circuit.optimizer_enabled != secondary_circuit.optimizer_enabled {
        return Err(anyhow!(
            "circuit.optimizer_enabled mismatch (primary={}, secondary={})",
            primary_circuit.optimizer_enabled,
            secondary_circuit.optimizer_enabled
        ));
    }

    Ok(())
}

fn fetch_rebalance_snapshot(
    rpc: &RpcClient,
    derived: &DerivedAccounts,
) -> Result<(
    wire::ProtocolState,
    wire::CircuitBreakerState,
    [wire::CollateralVault; 4],
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

    validate_vault_weight_caps(&vaults)?;
    Ok((protocol, circuit, vaults))
}

fn validate_vault_weight_caps(vaults: &[wire::CollateralVault; 4]) -> Result<()> {
    for (idx, vault) in vaults.iter().enumerate() {
        if !(MIN_WEIGHT_CAP_PPM..=MAX_WEIGHT_CAP_PPM).contains(&vault.weight_cap) {
            return Err(anyhow!(
                "vault[{idx}] weight_cap out of range: {} (expected {}..={} => 0.01..1.0)",
                vault.weight_cap,
                MIN_WEIGHT_CAP_PPM,
                MAX_WEIGHT_CAP_PPM
            ));
        }
    }

    Ok(())
}

/// Bridge: convert on-chain wire types → optimizer ProtocolSnapshot.
fn build_protocol_snapshot(
    vaults: &[wire::CollateralVault; 4],
    protocol: &wire::ProtocolState,
) -> optimizer::ProtocolSnapshot {
    let mut current_weights = [0.0f64; 4];
    for (i, w) in protocol.weights.iter().enumerate() {
        current_weights[i] = *w as f64 / WEIGHT_SCALE as f64;
    }

    let total_deposits: u128 = vaults.iter().map(|v| v.total_deposits as u128).sum();
    let total_value: u128 = vaults
        .iter()
        .map(|v| {
            (v.total_deposits as u128)
                .saturating_mul(v.price as u128)
                .saturating_div(WEIGHT_SCALE as u128)
        })
        .sum();
    let supply = protocol.total_supply.max(1) as f64;
    let collateral_ratio = total_value as f64 / supply;

    let mut oracle_quality = [1.0f64; 4];
    for (i, vault) in vaults.iter().enumerate() {
        if vault.price == 0 {
            oracle_quality[i] = 0.0;
        } else {
            let conf_ratio = vault.confidence as f64 / vault.price.max(1) as f64;
            oracle_quality[i] = (1.0 - conf_ratio * 10.0).clamp(0.0, 1.0);
        }
    }

    optimizer::ProtocolSnapshot {
        peg_price: 1.0,
        collateral_ratio,
        nav_history: vec![],
        current_weights,
        previous_weights: current_weights,
        oracle_quality_scores: oracle_quality,
        target_cr: protocol.cr_target as f64 / WEIGHT_SCALE as f64,
        mint_fee: protocol.mint_fee_rate as f64 / WEIGHT_SCALE as f64,
        redeem_fee: protocol.redeem_fee_rate as f64 / WEIGHT_SCALE as f64,
        loss_function: None,
    }
}

/// Convert optimizer f64 weights [0..1] → on-chain PPM [0..1_000_000].
fn f64_weights_to_ppm(weights: [f64; 4]) -> [u64; 4] {
    let mut ppm = [0u64; 4];
    let mut assigned = 0u64;
    for i in 0..3 {
        let w = (weights[i] * WEIGHT_SCALE as f64).round() as u64;
        ppm[i] = w.min(WEIGHT_SCALE);
        assigned = assigned.saturating_add(ppm[i]);
    }
    ppm[3] = WEIGHT_SCALE.saturating_sub(assigned);
    ppm
}

/// Decide target weights (and optional CR/fee update) — optimizer path or static fallback.
fn compute_rebalance_targets(
    vaults: &[wire::CollateralVault; 4],
    protocol: &wire::ProtocolState,
    circuit_breaker: &wire::CircuitBreakerState,
    memory: &mut RebalanceMemory,
) -> ([u64; 4], Option<ProtocolParamUpdate>) {
    if !circuit_breaker.optimizer_enabled {
        return (compute_target_weights(vaults), None);
    }

    let snapshot = build_protocol_snapshot(vaults, protocol);

    let current_params = ParamVector {
        weights: snapshot.current_weights,
        target_cr: snapshot.target_cr,
        mint_fee: snapshot.mint_fee,
        redeem_fee: snapshot.redeem_fee,
    };

    let adam = memory
        .adam_optimizer
        .get_or_insert_with(AdamOptimizer::default);
    let bounds = memory
        .safety_bounds
        .get_or_insert_with(SafetyBounds::default);
    let checkpoint = &mut memory.optimizer_checkpoint;

    match optimizer::optimize_step(&snapshot, &current_params, adam, bounds, checkpoint) {
        Ok(optimized) => {
            let target_weights = f64_weights_to_ppm(optimized.weights);

            let new_cr = (optimized.target_cr * WEIGHT_SCALE as f64).round() as u64;
            let new_mint_fee = (optimized.mint_fee * WEIGHT_SCALE as f64).round() as u64;
            let new_redeem_fee = (optimized.redeem_fee * WEIGHT_SCALE as f64).round() as u64;

            let cr_changed = new_cr != protocol.cr_target;
            let fee_changed =
                new_mint_fee != protocol.mint_fee_rate || new_redeem_fee != protocol.redeem_fee_rate;

            let pending_params = if cr_changed || fee_changed {
                Some(ProtocolParamUpdate {
                    target_cr: new_cr.clamp(CR_TARGET_MIN_PPM, CR_TARGET_MAX_PPM),
                    mint_fee: new_mint_fee.min(FEE_MAX_PPM),
                    redeem_fee: new_redeem_fee.min(FEE_MAX_PPM),
                })
            } else {
                None
            };

            info!(
                optimizer = true,
                loss = ?checkpoint.as_ref().map(|c| c.loss),
                target_weights = ?target_weights,
                cr = new_cr,
                mint_fee = new_mint_fee,
                redeem_fee = new_redeem_fee,
                "optimizer produced rebalance targets"
            );

            (target_weights, pending_params)
        }
        Err(err) => {
            warn!(
                error = %err,
                "optimizer failed, falling back to static weights"
            );
            (compute_target_weights(vaults), None)
        }
    }
}

/// Submit CR / fee parameter update on-chain after a successful rebalance.
fn maybe_submit_protocol_params_update(
    _rpc: &RpcClient,
    _secondary_rpc: Option<&RpcClient>,
    _secondary_mode: utils::SecondaryRpcMode,
    _cfg: &KeeperConfig,
    _derived: &DerivedAccounts,
    _k1: &Keypair,
    _k2: &Keypair,
    _protocol: &wire::ProtocolState,
    params: ProtocolParamUpdate,
) {
    // NOTE: on-chain `update_protocol_params` instruction is not yet deployed.
    // When available, build + submit the ix here.  For now, log the intended update.
    info!(
        target_cr = params.target_cr,
        mint_fee = params.mint_fee,
        redeem_fee = params.redeem_fee,
        "protocol param update deferred (on-chain instruction pending)"
    );
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

fn build_reveal_salt() -> [u8; 32] {
    let entropy = Keypair::new().to_bytes();
    let mut reveal_salt = [0u8; 32];
    reveal_salt.copy_from_slice(&entropy[..32]);
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
