use crate::{
    config::KeeperConfig,
    optimizer::{self, AdamOptimizer, OptimizerCheckpoint, ParamVector, SafetyBounds},
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcProgramAccountsConfig,
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::{hash, hashv},
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};

const WEIGHT_SCALE: u64 = 1_000_000;
const BATCH_WINDOW_SLOTS: u64 = 32;
const MIN_WEIGHT_CAP_PPM: u64 = 10_000;
const MAX_WEIGHT_CAP_PPM: u64 = WEIGHT_SCALE;
const CR_TARGET_MIN_PPM: u64 = 1_000_000;
const CR_TARGET_MAX_PPM: u64 = 2_000_000;
const FEE_MAX_PPM: u64 = 10_000;
const AGENT_RECORD_ACCOUNT_DATA_SIZE: u64 = 8 + 160;
const PENDING_REVEAL_STATE_PATH: &str = ".state/microstable/pending_reveal.json";
const PENDING_REVEAL_STATE_VERSION: u8 = 2;
const STATE_HMAC_ENV_KEY: &str = "MICROSTABLE_STATE_HMAC_KEY";
const PENDING_REVEAL_LOCK_STALE_SECS: u64 = 120;

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

impl RebalanceMemory {
    pub fn restore_optimizer_checkpoint(&mut self, checkpoint: OptimizerCheckpoint) {
        let mut optimizer = self.adam_optimizer.take().unwrap_or_default();
        optimizer.state = checkpoint.adam_state.clone();
        self.adam_optimizer = Some(optimizer);
        self.optimizer_checkpoint = Some(checkpoint);
    }

    pub fn load_pending_reveal_from_disk(&mut self) {
        if self.pending_reveal.is_some() {
            return;
        }

        match load_pending_reveal_checkpoint() {
            Ok(Some(pending)) => {
                self.pending_reveal = Some(pending);
            }
            Ok(None) => {}
            Err(err) => {
                warn!(error = %err, "failed to load pending reveal checkpoint");
            }
        }
    }

    fn persist_pending_reveal(&self) {
        if let Some(pending) = &self.pending_reveal {
            if let Err(err) = save_pending_reveal_checkpoint(pending) {
                warn!(error = %err, "failed to persist pending reveal checkpoint");
            }
        }
    }

    fn clear_pending_reveal(&mut self) {
        self.pending_reveal = None;
        if let Err(err) = clear_pending_reveal_checkpoint() {
            warn!(error = %err, "failed to clear pending reveal checkpoint");
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingReveal {
    commit_hash: [u8; 32],
    target_weights: [u64; 4],
    batch_slot: u64,
    reveal_salt: [u8; 32],
    pending_params: Option<ProtocolParamUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    memory.load_pending_reveal_from_disk();

    let (target_weights, pending_params) = compute_target_weights(
        &vaults,
        &protocol,
        &circuit_breaker,
        memory,
        cfg.optimizer_enabled,
    );
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
                memory.clear_pending_reveal();
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
                && deferred_reveal_ready(
                    current_slot,
                    pending.batch_slot,
                    protocol.pending_rebalance_slot,
                    cfg.commit_reveal_delay_slots,
                )
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
                    memory.clear_pending_reveal();
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

    let eligible_agents = match fetch_eligible_commit_agents(rpc, cfg.program_id) {
        Ok(agents) => agents,
        Err(err) => {
            warn!(
                error = %err,
                "rebalance commit skipped: failed to query eligible registered agents"
            );
            return Ok(outcome);
        }
    };

    let Some(submitting_agent_signer) =
        select_commit_submitting_signer(keepers, &eligible_agents, k1.pubkey())
    else {
        let local_keeper_pubkeys: Vec<String> = keepers
            .iter()
            .map(|keeper| keeper.pubkey().to_string())
            .collect();
        warn!(
            eligible_agents = eligible_agents.len(),
            local_keepers = ?local_keeper_pubkeys,
            "rebalance commit skipped: no local keeper key is an active tier-2 registered agent; register/promote at least one configured keeper key to tier 2+"
        );
        return Ok(outcome);
    };

    let submitting_agent = submitting_agent_signer.pubkey();
    let agent_record = derive_agent_record_pda(cfg.program_id, submitting_agent);
    match preflight_agent_record_exists(rpc, agent_record, cfg.program_id) {
        Ok(true) => {}
        Ok(false) => {
            warn!(
                agent = %submitting_agent,
                agent_record = %agent_record,
                "rebalance commit skipped: submitting agent record PDA missing or invalid"
            );
            return Ok(outcome);
        }
        Err(err) => {
            warn!(
                error = %err,
                agent = %submitting_agent,
                agent_record = %agent_record,
                "rebalance commit skipped: failed agent_record preflight check"
            );
            return Ok(outcome);
        }
    }

    // Check batch window has room for reveal delay before committing
    if !cfg.execute_rebalance_immediately
        && !batch_window_has_room(current_slot, cfg.commit_reveal_delay_slots)
    {
        info!(
            current_slot,
            position_in_window = current_slot % BATCH_WINDOW_SLOTS,
            delay = cfg.commit_reveal_delay_slots,
            "skipping commit: not enough slots remaining in batch window for deferred reveal"
        );
        return Ok(outcome);
    }

    let batch_slot = select_batch_slot(current_slot);
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
        agent_record,
        submitting_agent,
        k1.pubkey(),
        k2.pubkey(),
        commit_hash,
        cfg.commit_valid_for_slots,
    )?;

    let mut commit_signers = vec![k1, k2];
    if submitting_agent != k1.pubkey() && submitting_agent != k2.pubkey() {
        commit_signers.push(submitting_agent_signer);
    }

    let commit_sig = utils::send_instructions(
        rpc,
        secondary_rpc,
        secondary_mode,
        k1,
        &commit_signers,
        vec![commit_ix],
    )?;
    info!(
        deviation_bps,
        signature = %commit_sig,
        batch_slot,
        target_weights = ?target_weights,
        "rebalance commit sent"
    );

    // Freeze the commit-time batch_slot in keeper memory.
    // Reveal hash verification must use the same batch_slot preimage
    // that was used when commit_hash was constructed.
    memory.pending_reveal = Some(PendingReveal {
        commit_hash,
        target_weights,
        batch_slot,
        reveal_salt,
        pending_params,
    });
    memory.persist_pending_reveal();

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
            memory.clear_pending_reveal();
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
        current_weights[i] = (*w as f64 / WEIGHT_SCALE as f64).max(0.0);
    }

    let weight_sum: f64 = current_weights.iter().sum();
    if weight_sum > 0.0 {
        for w in &mut current_weights {
            *w /= weight_sum;
        }
    } else {
        current_weights = [0.25; 4];
    }

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
        nav_history: vec![collateral_ratio, collateral_ratio],
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
fn f64_weights_to_ppm(weights: [f64; 4]) -> [u32; 4] {
    let mut sanitized = [0.0f64; 4];
    for i in 0..4 {
        sanitized[i] = if weights[i].is_finite() {
            weights[i].max(0.0)
        } else {
            0.0
        };
    }

    let sum: f64 = sanitized.iter().sum();
    if sum <= 0.0 {
        return [250_000, 250_000, 250_000, 250_000];
    }

    let mut base = [0u32; 4];
    let mut remainders = [(0usize, 0.0f64); 4];
    let mut assigned = 0u64;

    for i in 0..4 {
        let scaled = sanitized[i] / sum * WEIGHT_SCALE as f64;
        let floored = scaled.floor();
        base[i] = floored as u32;
        remainders[i] = (i, scaled - floored);
        assigned = assigned.saturating_add(base[i] as u64);
    }

    let mut remaining = WEIGHT_SCALE.saturating_sub(assigned);
    remainders.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut cursor = 0usize;
    while remaining > 0 {
        let idx = remainders[cursor % 4].0;
        base[idx] = base[idx].saturating_add(1);
        remaining -= 1;
        cursor += 1;
    }

    base
}

/// Decide target weights (and optional CR/fee update) — optimizer path or static fallback.
fn compute_target_weights(
    vaults: &[wire::CollateralVault; 4],
    protocol: &wire::ProtocolState,
    circuit_breaker: &wire::CircuitBreakerState,
    memory: &mut RebalanceMemory,
    optimizer_enabled: bool,
) -> ([u64; 4], Option<ProtocolParamUpdate>) {
    if !(optimizer_enabled && circuit_breaker.optimizer_enabled) {
        return (compute_static_target_weights(vaults), None);
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
            let target_weights_ppm = f64_weights_to_ppm(optimized.weights);
            let target_weights = target_weights_ppm.map(|value| value as u64);

            let new_cr = (optimized.target_cr * WEIGHT_SCALE as f64).round() as u64;
            let new_mint_fee = (optimized.mint_fee * WEIGHT_SCALE as f64).round() as u64;
            let new_redeem_fee = (optimized.redeem_fee * WEIGHT_SCALE as f64).round() as u64;

            let cr_changed = new_cr != protocol.cr_target;
            let fee_changed = new_mint_fee != protocol.mint_fee_rate
                || new_redeem_fee != protocol.redeem_fee_rate;

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
            (compute_static_target_weights(vaults), None)
        }
    }
}

/// Submit CR / fee parameter update on-chain after a successful rebalance.
fn maybe_submit_protocol_params_update(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    derived: &DerivedAccounts,
    k1: &Keypair,
    k2: &Keypair,
    protocol: &wire::ProtocolState,
    params: ProtocolParamUpdate,
) {
    if params.target_cr == protocol.cr_target
        && params.mint_fee == protocol.mint_fee_rate
        && params.redeem_fee == protocol.redeem_fee_rate
    {
        return;
    }

    let ix = match wire::ix_update_protocol_params(
        cfg.program_id,
        derived.protocol_state,
        k1.pubkey(),
        k2.pubkey(),
        wire::UpdateProtocolParamsArgs {
            new_cr_target: params.target_cr,
            new_mint_fee: params.mint_fee,
            new_redeem_fee: params.redeem_fee,
        },
    ) {
        Ok(ix) => ix,
        Err(err) => {
            warn!(
                error = %err,
                target_cr = params.target_cr,
                mint_fee = params.mint_fee,
                redeem_fee = params.redeem_fee,
                "failed to build update_protocol_params instruction"
            );
            return;
        }
    };

    match utils::send_instructions(rpc, secondary_rpc, secondary_mode, k1, &[k1, k2], vec![ix]) {
        Ok(sig) => {
            info!(
                signature = %sig,
                target_cr = params.target_cr,
                mint_fee = params.mint_fee,
                redeem_fee = params.redeem_fee,
                "update_protocol_params sent"
            );
        }
        Err(err) => {
            warn!(
                error = %err,
                target_cr = params.target_cr,
                mint_fee = params.mint_fee,
                redeem_fee = params.redeem_fee,
                "update_protocol_params submission failed"
            );
        }
    }
}

fn compute_static_target_weights(vaults: &[wire::CollateralVault; 4]) -> [u64; 4] {
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

fn select_batch_slot(current_slot: u64) -> u64 {
    current_slot
}

/// Check if we have enough slots left in the current batch window for the
/// reveal delay. Prevents commits that would be unreachable due to window
/// boundary crossing.
fn batch_window_has_room(current_slot: u64, reveal_delay_slots: u64) -> bool {
    let position_in_window = current_slot % BATCH_WINDOW_SLOTS;
    let remaining = BATCH_WINDOW_SLOTS.saturating_sub(position_in_window);
    // Need at least delay + 2 slot margin for tolerated ±2 TX landing drift
    remaining > reveal_delay_slots.saturating_add(2)
}

fn deferred_reveal_ready(
    current_slot: u64,
    batch_slot: u64,
    protocol_pending_slot: u64,
    reveal_delay_slots: u64,
) -> bool {
    // Tolerate ±2 slot drift between keeper-observed slot and on-chain commit slot
    let slot_drift = batch_slot.abs_diff(protocol_pending_slot);
    slot_drift <= 2
        && current_slot >= protocol_pending_slot.saturating_add(reveal_delay_slots)
        && current_slot / BATCH_WINDOW_SLOTS == protocol_pending_slot / BATCH_WINDOW_SLOTS
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

fn derive_agent_record_pda(program_id: Pubkey, agent: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"agent", agent.as_ref()], &program_id).0
}

fn fetch_eligible_commit_agents(rpc: &RpcClient, program_id: Pubkey) -> Result<Vec<Pubkey>> {
    let mut agents = Vec::new();

    for (_, account) in rpc.get_program_accounts_with_config(
        &program_id,
        RpcProgramAccountsConfig {
            filters: Some(agent_record_scan_filters()),
            ..RpcProgramAccountsConfig::default()
        },
    )? {
        let data = account.data;
        let record = match wire::decode_account::<wire::AgentRecord>(&data, "AgentRecord") {
            Ok(record) => record,
            Err(_) => continue,
        };

        if record.status != wire::AgentStatus::Active || record.tier < 2 {
            continue;
        }

        agents.push(record.agent);
    }

    agents.sort();
    agents.dedup();
    Ok(agents)
}

fn agent_record_scan_filters() -> Vec<RpcFilterType> {
    vec![
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            0,
            agent_record_discriminator().to_vec(),
        )),
        RpcFilterType::DataSize(AGENT_RECORD_ACCOUNT_DATA_SIZE),
    ]
}

fn select_commit_submitting_signer<'a>(
    keepers: &'a [Keypair],
    eligible_agents: &[Pubkey],
    preferred_signer: Pubkey,
) -> Option<&'a Keypair> {
    let eligible: HashSet<Pubkey> = eligible_agents.iter().copied().collect();

    if eligible.contains(&preferred_signer) {
        if let Some(preferred) = keepers
            .iter()
            .find(|signer| signer.pubkey() == preferred_signer)
        {
            return Some(preferred);
        }
    }

    keepers
        .iter()
        .find(|signer| eligible.contains(&signer.pubkey()))
}

fn preflight_agent_record_exists(
    rpc: &RpcClient,
    agent_record: Pubkey,
    program_id: Pubkey,
) -> Result<bool> {
    let response = rpc.get_account_with_commitment(&agent_record, CommitmentConfig::processed())?;
    let Some(account) = response.value else {
        return Ok(false);
    };

    Ok(account.owner == program_id)
}

fn agent_record_discriminator() -> [u8; 8] {
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash(b"account:AgentRecord").to_bytes()[..8]);
    discriminator
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRevealCheckpoint {
    version: u8,
    integrity_tag: String,
    saved_at_unix_secs: u64,
    pending: PendingReveal,
}

pub fn pending_reveal_checkpoint_path() -> PathBuf {
    PathBuf::from(PENDING_REVEAL_STATE_PATH)
}

fn pending_reveal_lock_path() -> PathBuf {
    pending_reveal_checkpoint_path().with_extension("json.lock")
}

struct PendingRevealLockGuard {
    path: PathBuf,
}

impl Drop for PendingRevealLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_pending_reveal_lock() -> Result<PendingRevealLockGuard> {
    let lock_path = pending_reveal_lock_path();
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Ok(metadata) = fs::metadata(&lock_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(age) = SystemTime::now().duration_since(modified) {
                if age.as_secs() > PENDING_REVEAL_LOCK_STALE_SECS {
                    let _ = fs::remove_file(&lock_path);
                }
            }
        }
    }

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut lock_file) => {
            use std::io::Write;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = writeln!(lock_file, "pid={} ts={}", std::process::id(), now);
            Ok(PendingRevealLockGuard { path: lock_path })
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Err(anyhow!(
            "pending reveal checkpoint lock is held: {}",
            lock_path.display()
        )),
        Err(err) => Err(anyhow!(
            "failed to acquire pending reveal checkpoint lock {}: {}",
            lock_path.display(),
            err
        )),
    }
}

fn save_pending_reveal_checkpoint(pending: &PendingReveal) -> Result<()> {
    let _lock = acquire_pending_reveal_lock()?;
    let path = pending_reveal_checkpoint_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let pending_json = serde_json::to_vec_pretty(pending)?;
    let integrity_tag = keyed_hash_hex(state_hmac_key()?.as_bytes(), &pending_json);
    let envelope = PendingRevealCheckpoint {
        version: PENDING_REVEAL_STATE_VERSION,
        integrity_tag,
        saved_at_unix_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        pending: pending.clone(),
    };

    let payload = serde_json::to_vec_pretty(&envelope)?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, payload)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(&tmp_path, &path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

fn load_pending_reveal_checkpoint() -> Result<Option<PendingReveal>> {
    let _lock = acquire_pending_reveal_lock()?;
    let path = pending_reveal_checkpoint_path();
    if !path.exists() {
        return Ok(None);
    }

    verify_secure_state_file(&path)?;
    let payload = fs::read(&path)?;
    let envelope: PendingRevealCheckpoint = serde_json::from_slice(&payload)?;
    if envelope.version != PENDING_REVEAL_STATE_VERSION {
        return Err(anyhow!(
            "unsupported pending reveal checkpoint version: {}",
            envelope.version
        ));
    }

    let pending_json = serde_json::to_vec_pretty(&envelope.pending)?;
    let expected_tag = keyed_hash_hex(state_hmac_key()?.as_bytes(), &pending_json);
    if envelope.integrity_tag.trim().to_ascii_lowercase() != expected_tag {
        return Err(anyhow!("pending reveal checkpoint integrity verification failed"));
    }

    Ok(Some(envelope.pending))
}

fn clear_pending_reveal_checkpoint() -> Result<()> {
    let _lock = acquire_pending_reveal_lock()?;
    let path = pending_reveal_checkpoint_path();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn state_hmac_key() -> Result<String> {
    env::var(STATE_HMAC_ENV_KEY)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("{} must be set to persist pending reveal state", STATE_HMAC_ENV_KEY))
}

fn keyed_hash_hex(key: &[u8], payload: &[u8]) -> String {
    let digest = hashv(&[b"microstable:pending-reveal:v2", key, payload, key]).to_bytes();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

fn verify_secure_state_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::metadata(path)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(anyhow!(
                "insecure pending reveal checkpoint permissions {:o}: {}",
                mode,
                path.display()
            ));
        }

        let owner_uid = metadata.uid();
        let effective_uid = unsafe { libc::geteuid() as u32 };
        if owner_uid != effective_uid {
            return Err(anyhow!(
                "pending reveal checkpoint owner mismatch for {} (owner_uid={}, effective_uid={})",
                path.display(),
                owner_uid,
                effective_uid
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    fn mock_protocol(weights: [u64; 4]) -> wire::ProtocolState {
        wire::ProtocolState {
            weights,
            fee_rate: 2_000,
            mint_fee_rate: 2_000,
            redeem_fee_rate: 2_000,
            cr_target: 1_200_000,
            total_supply: 1_000_000,
            last_update_slot: 0,
            keeper_set: [
                Pubkey::new_unique(),
                Pubkey::new_unique(),
                Pubkey::new_unique(),
            ],
            emergency_shutdown: false,
            pending_rebalance_commit: [0u8; 32],
            pending_rebalance_slot: 0,
            pending_rebalance_expiry: 0,
            pending_keeper_set: [[0u8; 32]; 3],
            pending_keeper_activation_slot: 0,
            flow_control_slot: 0,
            minted_in_flow_slot: 0,
            redeemed_in_flow_slot: 0,
            max_mint_per_slot_ppm: 120_000,
            max_redeem_per_slot_ppm: 80_000,
            manual_oracle_mode_expiry_slot: 0,
            bump: 255,
        }
    }

    fn mock_circuit(optimizer_enabled: bool) -> wire::CircuitBreakerState {
        wire::CircuitBreakerState {
            status: [0; 4],
            activation_tick: [0; 4],
            trigger_count: [0; 4],
            cooldown_until: [0; 4],
            last_trigger_tick: [0; 4],
            recent_trigger_count: [0; 4],
            recovery_tick: [0; 4],
            cb1_collateral_index: 0,
            mint_rate_limit: 0,
            optimizer_enabled,
            learning_rate_scale: 1_000_000,
            max_activation_duration: 0,
            bump: 255,
        }
    }

    fn mock_vault(index: u8, deposits: u64, price: u64, confidence: u64) -> wire::CollateralVault {
        wire::CollateralVault {
            index,
            mint: Pubkey::new_unique(),
            vault: Pubkey::new_unique(),
            oracle: Pubkey::new_unique(),
            risk_score: 100_000,
            weight_cap: 900_000,
            base_weight_cap: 900_000,
            price,
            confidence,
            last_oracle_slot: 0,
            total_deposits: deposits,
            bump: 255,
            pyth_price_feed: Pubkey::new_unique(),
        }
    }

    fn mock_vaults() -> [wire::CollateralVault; 4] {
        [
            mock_vault(0, 1_000_000, 1_000_000, 1_000),
            mock_vault(1, 2_000_000, 1_000_000, 2_000),
            mock_vault(2, 1_000_000, 2_000_000, 500),
            mock_vault(3, 500_000, 1_000_000, 1_500),
        ]
    }

    #[test]
    fn tc_ow_01_build_protocol_snapshot_from_mock_data() {
        let protocol = mock_protocol([400_000, 300_000, 200_000, 100_000]);
        let vaults = mock_vaults();

        let snapshot = build_protocol_snapshot(&vaults, &protocol);

        snapshot.validate().expect("snapshot should be valid");
        let expected = [0.4, 0.3, 0.2, 0.1];
        for (idx, expected_w) in expected.iter().enumerate() {
            assert!((snapshot.current_weights[idx] - expected_w).abs() < 1e-9);
            assert!((snapshot.previous_weights[idx] - expected_w).abs() < 1e-9);
        }
        assert!((snapshot.collateral_ratio - 5.5).abs() < 1e-9);
        assert!((snapshot.target_cr - 1.2).abs() < 1e-9);
        assert!((snapshot.mint_fee - 0.002).abs() < 1e-9);
        assert!((snapshot.redeem_fee - 0.002).abs() < 1e-9);
    }

    #[test]
    fn tc_ow_02_f64_weights_to_ppm_sum_is_one_million() {
        let out = f64_weights_to_ppm([0.1, 0.2, 0.3, 0.4]);
        let sum: u64 = out.iter().map(|x| *x as u64).sum();
        assert_eq!(sum, WEIGHT_SCALE);
        assert_eq!(out, [100_000, 200_000, 300_000, 400_000]);
    }

    #[test]
    fn tc_ow_03_f64_weights_to_ppm_edge_cases() {
        assert_eq!(
            f64_weights_to_ppm([1.0, 0.0, 0.0, 0.0]),
            [1_000_000, 0, 0, 0]
        );

        let out = f64_weights_to_ppm([0.0, 0.0, 0.5, 0.5]);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 0);
        let sum: u64 = out.iter().map(|x| *x as u64).sum();
        assert_eq!(sum, WEIGHT_SCALE);
    }

    #[test]
    fn tc_ow_04_compute_target_weights_optimizer_disabled_uses_static_formula() {
        let protocol = mock_protocol([700_000, 100_000, 100_000, 100_000]);
        let vaults = mock_vaults();
        let circuit = mock_circuit(true);
        let mut memory = RebalanceMemory::default();

        let expected = compute_static_target_weights(&vaults);
        let (target, pending_params) =
            compute_target_weights(&vaults, &protocol, &circuit, &mut memory, false);

        assert_eq!(target, expected);
        assert!(pending_params.is_none());
        assert!(memory.optimizer_checkpoint.is_none());
    }

    #[test]
    fn tc_ow_05_compute_target_weights_optimizer_enabled_calls_optimizer() {
        let protocol = mock_protocol([900_000, 100_000, 0, 0]);
        let vaults = mock_vaults();
        let circuit = mock_circuit(true);
        let mut memory = RebalanceMemory {
            adam_optimizer: Some(AdamOptimizer {
                learning_rate: 0.5,
                warmup_steps: 0,
                decay_steps: 0,
                min_learning_rate: 0.5,
                ..AdamOptimizer::default()
            }),
            ..RebalanceMemory::default()
        };

        let expected_static = compute_static_target_weights(&vaults);
        let (target, _pending_params) =
            compute_target_weights(&vaults, &protocol, &circuit, &mut memory, true);

        assert!(memory.optimizer_checkpoint.is_some());
        assert_ne!(
            target, expected_static,
            "optimizer path should differ from static"
        );
    }

    #[test]
    fn tc_ow_06_optimizer_failure_falls_back_to_static_formula() {
        let protocol = mock_protocol([900_000, 100_000, 0, 0]);
        let vaults = mock_vaults();
        let circuit = mock_circuit(true);
        let mut memory = RebalanceMemory {
            safety_bounds: Some(SafetyBounds {
                weight_caps: [0.2, 0.2, 0.2, 0.2],
                ..SafetyBounds::default()
            }),
            ..RebalanceMemory::default()
        };

        let expected_static = compute_static_target_weights(&vaults);
        let (target, pending_params) =
            compute_target_weights(&vaults, &protocol, &circuit, &mut memory, true);

        assert_eq!(target, expected_static);
        assert!(pending_params.is_none());
    }

    #[test]
    fn tc_ow_07_weight_deviation_bps_reflects_large_rebalance_need() {
        let current = [400_000, 300_000, 200_000, 100_000];
        let target = [100_000, 400_000, 300_000, 200_000];

        let deviation_bps = weight_deviation_bps(current, target);

        assert!(
            deviation_bps > 300,
            "deviation should exceed default 300bps threshold"
        );
        assert_eq!(deviation_bps, 6_000);
    }

    #[test]
    fn tc_ow_08_commit_hash_changes_with_salt_or_batch_slot() {
        let protocol = Pubkey::new_unique();
        let weights = [250_000, 250_000, 250_000, 250_000];

        let h1 = compute_rebalance_commit(protocol, weights, 100, [1u8; 32]);
        let h2 = compute_rebalance_commit(protocol, weights, 100, [2u8; 32]);
        let h3 = compute_rebalance_commit(protocol, weights, 101, [1u8; 32]);

        assert_ne!(h1, h2, "salt must change commit hash");
        assert_ne!(h1, h3, "batch slot must change commit hash");
    }

    #[test]
    fn tc_ow_09_commit_submitter_prefers_primary_keeper_when_eligible() {
        let k1 = Keypair::new();
        let k2 = Keypair::new();
        let keepers = vec![k1, k2];

        let selected = select_commit_submitting_signer(
            &keepers,
            &[keepers[0].pubkey(), keepers[1].pubkey()],
            keepers[0].pubkey(),
        )
        .expect("eligible signer should resolve");

        assert_eq!(selected.pubkey(), keepers[0].pubkey());
    }

    #[test]
    fn tc_ow_10_commit_submitter_falls_back_to_other_eligible_keeper() {
        let k1 = Keypair::new();
        let k2 = Keypair::new();
        let keepers = vec![k1, k2];

        let selected =
            select_commit_submitting_signer(&keepers, &[keepers[1].pubkey()], keepers[0].pubkey())
                .expect("fallback signer should resolve");

        assert_eq!(selected.pubkey(), keepers[1].pubkey());
    }

    #[test]
    fn tc_ow_11_agent_record_scan_filters_include_discriminator_and_size() {
        let filters = agent_record_scan_filters();
        assert_eq!(filters.len(), 2);

        match &filters[0] {
            RpcFilterType::Memcmp(memcmp) => {
                assert_eq!(memcmp.offset(), 0);
                let bytes = memcmp.bytes().expect("memcmp bytes should decode");
                assert_eq!(bytes.as_ref(), &agent_record_discriminator());
            }
            _ => panic!("first filter should be memcmp discriminator"),
        }

        match filters[1] {
            RpcFilterType::DataSize(size) => assert_eq!(size, AGENT_RECORD_ACCOUNT_DATA_SIZE),
            _ => panic!("second filter should be dataSize"),
        }
    }

    #[test]
    fn tc_ow_12_deferred_reveal_becomes_reachable_after_delay() {
        let commit_slot = 1_000;
        let delay = 5;

        assert!(!deferred_reveal_ready(
            commit_slot + delay - 1,
            commit_slot,
            commit_slot,
            delay,
        ));
        assert!(deferred_reveal_ready(
            commit_slot + delay,
            commit_slot,
            commit_slot,
            delay,
        ));
    }

    #[test]
    fn tc_ow_13_deferred_reveal_tolerates_slot_drift() {
        let keeper_slot = 1_000;
        let onchain_slot = 1_001; // TX landed 1 slot later
        let delay = 5;

        // With drift tolerance of ±2, this should still work
        assert!(deferred_reveal_ready(
            onchain_slot + delay,
            keeper_slot,
            onchain_slot,
            delay,
        ));

        // Drift of 3 should fail
        assert!(!deferred_reveal_ready(1_003 + delay, 1_000, 1_003, delay,));
    }

    #[test]
    fn tc_ow_14_deferred_reveal_respects_batch_window() {
        let commit_slot = 64; // window 2 (64/32 = 2)
        let delay = 5;

        // Reveal in same window: OK
        assert!(deferred_reveal_ready(
            commit_slot + delay, // slot 69, window 2
            commit_slot,
            commit_slot,
            delay,
        ));

        // Reveal in next window: FAIL
        assert!(!deferred_reveal_ready(
            96, // window 3
            commit_slot,
            commit_slot,
            delay,
        ));
    }

    #[test]
    fn tc_ow_15_batch_window_has_room_prevents_dead_zone_commits() {
        // Position 24 in window, delay 5 → remaining 8 > 7 → OK
        assert!(batch_window_has_room(24, 5));

        // Position 25 in window, delay 5 → remaining 7 = 7 → NOT OK
        assert!(!batch_window_has_room(25, 5));

        // Position 27 in window, delay 5 → remaining 5 ≤ 7 → NOT OK
        assert!(!batch_window_has_room(27, 5));

        // Position 31 in window, delay 1 → remaining 1 ≤ 3 → NOT OK
        assert!(!batch_window_has_room(31, 1));

        // Position 0, delay 29 → remaining 32 > 31 → OK
        assert!(batch_window_has_room(0, 29));

        // Position 0, delay 30 → remaining 32 = 32 → NOT OK
        assert!(!batch_window_has_room(0, 30));
    }
}
