mod agent_loop;
mod aig;
mod config;
mod monitor;
mod optimizer;
mod oracle;
mod rebalance;
mod risk_manager;
mod tournament;
mod utils;
mod watchdog;
mod wire;

#[cfg(test)]
mod agent_loop_tests;
#[cfg(test)]
mod aig_tests;
#[cfg(test)]
mod main_preflight_tests;
#[cfg(test)]
mod main_wiring_tests;
#[cfg(test)]
mod optimizer_tests;
#[cfg(test)]
mod risk_manager_tests;
#[cfg(test)]
mod stress_tests;
#[cfg(test)]
mod tournament_tests;

use agent_loop::AgentLoopState;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use config::KeeperConfig;
use monitor::MonitorMemory;
use rebalance::RebalanceMemory;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::MissedTickBehavior;
use tracing::{error, info, warn};
use watchdog::WatchdogMemory;

const DEFAULT_KEEPER_ENV_PATH: &str = "/home/spritz/microstable-keeper/.env";
const FAILURE_BACKOFF_SECS: u64 = 30;

#[derive(Debug, Parser)]
#[command(
    name = "microstable-keeper",
    version,
    about = "Microstable keeper daemon"
)]
struct Cli {
    /// Path to keeper JSON config
    #[arg(long)]
    config: Option<PathBuf>,

    /// Run one full cycle and exit
    #[arg(long)]
    once: bool,

    /// Emit structured JSON logs
    #[arg(long)]
    json_logs: bool,

    /// Exit with status 1 when no locally loaded keeper key can submit rebalance commits.
    #[arg(long)]
    require_rebalance: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    utils::init_tracing(cli.json_logs);
    utils::enforce_supply_chain_controls()?;

    let cfg = KeeperConfig::load(cli.config.as_deref())?;
    info!(
        primary_rpc = %cfg.rpc_url,
        secondary_rpc = ?cfg.secondary_rpc_url,
        program_id = %cfg.program_id,
        "config loaded"
    );

    let rpc = RpcClient::new_with_commitment(cfg.rpc_url.clone(), CommitmentConfig::confirmed());
    let secondary_rpc = cfg
        .secondary_rpc_url
        .as_ref()
        .map(|url| RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed()));

    let epoch_info = rpc
        .get_epoch_info()
        .context("primary RPC health check failed")?;
    info!(
        epoch = epoch_info.epoch,
        absolute_slot = epoch_info.absolute_slot,
        "primary rpc connected"
    );

    if let Some(secondary) = secondary_rpc.as_ref() {
        match secondary.get_epoch_info() {
            Ok(secondary_epoch_info) => {
                info!(
                    epoch = secondary_epoch_info.epoch,
                    absolute_slot = secondary_epoch_info.absolute_slot,
                    "secondary rpc connected"
                );
                let _ = utils::register_secondary_rpc_success();
            }
            Err(err) => {
                let entered_degraded = utils::register_secondary_rpc_failure();
                warn!(
                    error = %err,
                    degraded = utils::secondary_rpc_is_degraded(),
                    entered_degraded,
                    "secondary RPC health check failed; continuing in degraded mode"
                );
            }
        }
    }

    utils::verify_program_deployed(&rpc, &cfg.program_id)?;

    let secondary_runtime_for_boot = resolve_secondary_rpc_runtime(secondary_rpc.as_ref());
    if let Some(secondary) = secondary_runtime_for_boot.active_secondary_rpc {
        if let Err(err) = utils::verify_program_deployed(secondary, &cfg.program_id) {
            let entered_degraded = utils::register_secondary_rpc_failure();
            warn!(
                error = %err,
                degraded = utils::secondary_rpc_is_degraded(),
                entered_degraded,
                "secondary program deployment check failed; disabling secondary RPC"
            );
        }
    }

    let derived = utils::DerivedAccounts::derive(&cfg.program_id);
    info!(
        protocol_state = %derived.protocol_state,
        circuit_breaker = %derived.circuit_breaker,
        "derived program addresses"
    );

    let keypairs = utils::load_keypairs(&cfg.keeper_keypairs)?;
    info!(keeper_count = keypairs.len(), "keeper keypairs loaded");

    run_startup_preflight(&rpc, &cfg, &keypairs, cli.require_rebalance)?;

    let mut monitor_memory = MonitorMemory::default();
    let mut rebalance_memory = RebalanceMemory::default();
    let mut risk_manager_memory = risk_manager::RiskManagerMemory::default();
    let mut watchdog_memory = WatchdogMemory::default();
    let mut agent_loop_state = AgentLoopState::default();

    load_optimizer_checkpoint_into_memory(&mut rebalance_memory);
    rebalance_memory.load_pending_reveal_from_disk();

    if cli.once {
        let secondary_runtime = resolve_secondary_rpc_runtime(secondary_rpc.as_ref());
        run_cycle(
            &rpc,
            secondary_runtime.active_secondary_rpc,
            secondary_runtime.mode,
            &cfg,
            &keypairs,
            &derived,
            &mut monitor_memory,
            &mut rebalance_memory,
            &mut risk_manager_memory,
            &mut watchdog_memory,
            &mut agent_loop_state,
        )?;
        return Ok(());
    }

    info!(tick_secs = cfg.tick_interval_secs, "keeper daemon started");

    let mut interval = tokio::time::interval(Duration::from_secs(cfg.tick_interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut consecutive_failed_cycles = 0u64;

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("failed to bind SIGTERM handler")?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let secondary_runtime = resolve_secondary_rpc_runtime(secondary_rpc.as_ref());
                    match run_cycle(
                        &rpc,
                        secondary_runtime.active_secondary_rpc,
                        secondary_runtime.mode,
                        &cfg,
                        &keypairs,
                        &derived,
                        &mut monitor_memory,
                        &mut rebalance_memory,
                        &mut risk_manager_memory,
                        &mut watchdog_memory,
                        &mut agent_loop_state,
                    ) {
                        Ok(()) => {
                            if consecutive_failed_cycles > 0 {
                                info!(previous_failed_cycles = consecutive_failed_cycles, "cycle recovered");
                            }
                            consecutive_failed_cycles = 0;
                        }
                        Err(err) => {
                            consecutive_failed_cycles = consecutive_failed_cycles.saturating_add(1);
                            error!(
                                error = %err,
                                consecutive_failed_cycles,
                                max_consecutive_failed_cycles = cfg.max_consecutive_failed_cycles,
                                "cycle failed"
                            );
                            if consecutive_failed_cycles >= cfg.max_consecutive_failed_cycles {
                                error!(
                                    consecutive_failed_cycles,
                                    backoff_secs = FAILURE_BACKOFF_SECS,
                                    "failure threshold reached; entering self-heal backoff instead of exiting"
                                );
                                std::thread::sleep(Duration::from_secs(FAILURE_BACKOFF_SECS));
                                consecutive_failed_cycles = 0;
                            }
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT, shutting down");
                    break;
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM, shutting down");
                    break;
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let secondary_runtime = resolve_secondary_rpc_runtime(secondary_rpc.as_ref());
                    match run_cycle(
                        &rpc,
                        secondary_runtime.active_secondary_rpc,
                        secondary_runtime.mode,
                        &cfg,
                        &keypairs,
                        &derived,
                        &mut monitor_memory,
                        &mut rebalance_memory,
                        &mut risk_manager_memory,
                        &mut watchdog_memory,
                        &mut agent_loop_state,
                    ) {
                        Ok(()) => {
                            if consecutive_failed_cycles > 0 {
                                info!(previous_failed_cycles = consecutive_failed_cycles, "cycle recovered");
                            }
                            consecutive_failed_cycles = 0;
                        }
                        Err(err) => {
                            consecutive_failed_cycles = consecutive_failed_cycles.saturating_add(1);
                            error!(
                                error = %err,
                                consecutive_failed_cycles,
                                max_consecutive_failed_cycles = cfg.max_consecutive_failed_cycles,
                                "cycle failed"
                            );
                            if consecutive_failed_cycles >= cfg.max_consecutive_failed_cycles {
                                error!(
                                    consecutive_failed_cycles,
                                    backoff_secs = FAILURE_BACKOFF_SECS,
                                    "failure threshold reached; entering self-heal backoff instead of exiting"
                                );
                                std::thread::sleep(Duration::from_secs(FAILURE_BACKOFF_SECS));
                                consecutive_failed_cycles = 0;
                            }
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT, shutting down");
                    break;
                }
            }
        }
    }

    info!("keeper stopped gracefully");
    Ok(())
}

fn load_optimizer_checkpoint_into_memory(rebalance_memory: &mut RebalanceMemory) {
    let checkpoint_path = optimizer::checkpoint_path();

    if !checkpoint_path.exists() {
        info!(
            path = %checkpoint_path.display(),
            "optimizer checkpoint not found, starting fresh"
        );
        return;
    }

    match optimizer::OptimizerCheckpoint::load_from_path(&checkpoint_path) {
        Ok(checkpoint) => {
            let tick = checkpoint.tick;
            let loss = checkpoint.loss;
            rebalance_memory.restore_optimizer_checkpoint(checkpoint);
            info!(
                path = %checkpoint_path.display(),
                tick,
                loss,
                "restored optimizer checkpoint"
            );
        }
        Err(err) => {
            warn!(
                path = %checkpoint_path.display(),
                error = %err,
                "failed to load optimizer checkpoint, starting fresh"
            );
        }
    }
}

fn run_startup_preflight(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    require_rebalance: bool,
) -> Result<()> {
    match preflight_keeper_agent_registration(rpc, cfg, keepers) {
        Ok(readiness) => {
            if readiness.has_eligible_submitter() {
                info!(
                    eligible_keepers = ?readiness.eligible_keepers,
                    checked_keepers = readiness.checked_keepers,
                    "startup preflight verified rebalance submitter readiness"
                );
            } else {
                let guidance = rebalance_preflight_instructions();
                error!(
                    checked_keepers = readiness.checked_keepers,
                    "startup preflight found no locally loaded keeper key eligible for rebalance commit (requires registered AgentRecord + status=Active + tier>=2)"
                );
                error!(instructions = guidance, "rebalance setup required");
                eprintln!("{guidance}");

                if require_rebalance {
                    return Err(anyhow!(
                        "--require-rebalance set and no eligible local rebalance submitter is available"
                    ));
                }
            }
        }
        Err(err) => {
            warn!(
                error = %err,
                "startup preflight could not fully verify keeper agent registration state"
            );
            if require_rebalance {
                return Err(anyhow!(
                    "--require-rebalance set and keeper agent registration preflight could not be completed: {err}"
                ));
            }
        }
    }

    check_pm2_isolation();
    check_dotenv_permissions();
    Ok(())
}

#[derive(Debug, Default)]
struct RebalanceSubmitterReadiness {
    checked_keepers: usize,
    eligible_keepers: Vec<Pubkey>,
}

impl RebalanceSubmitterReadiness {
    fn has_eligible_submitter(&self) -> bool {
        !self.eligible_keepers.is_empty()
    }
}

fn preflight_keeper_agent_registration(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
) -> Result<RebalanceSubmitterReadiness> {
    let mut readiness = RebalanceSubmitterReadiness {
        checked_keepers: keepers.len(),
        ..RebalanceSubmitterReadiness::default()
    };

    for keeper in keepers {
        let keeper_key = keeper.pubkey();
        let agent_record = derive_agent_record_pda(cfg.program_id, keeper_key);

        let response =
            rpc.get_account_with_commitment(&agent_record, CommitmentConfig::processed())?;
        let Some(account) = response.value else {
            warn!(
                keeper_key = %redact_pubkey(keeper_key),
                "keeper signer is not registered as agent — rebalance commit will be unavailable"
            );
            continue;
        };

        if account.owner != cfg.program_id {
            warn!(
                keeper_key = %redact_pubkey(keeper_key),
                owner = %account.owner,
                "keeper signer has invalid agent record owner — rebalance commit will be unavailable"
            );
            continue;
        }

        let Ok(record) = wire::decode_account::<wire::AgentRecord>(&account.data, "AgentRecord")
        else {
            warn!(
                keeper_key = %redact_pubkey(keeper_key),
                "keeper signer agent record decode failed — rebalance commit will be unavailable"
            );
            continue;
        };

        if !agent_record_is_rebalance_eligible(&record) {
            if record.status != wire::AgentStatus::Active {
                warn!(
                    keeper_key = %redact_pubkey(keeper_key),
                    status = ?record.status,
                    "keeper signer is not Active — rebalance commit requires active status"
                );
            }
            if record.tier < 2 {
                warn!(
                    keeper_key = %redact_pubkey(keeper_key),
                    tier = record.tier,
                    "keeper signer tier is below 2 (current={})",
                    record.tier
                );
            }
            continue;
        }

        readiness.eligible_keepers.push(keeper_key);
    }

    Ok(readiness)
}

fn agent_record_is_rebalance_eligible(record: &wire::AgentRecord) -> bool {
    record.status == wire::AgentStatus::Active && record.tier >= 2
}

fn rebalance_preflight_instructions() -> &'static str {
    "Rebalance submitter setup required:\n\
- Register at least one configured keeper key as an agent:\n\
  ts-node solana/scripts/register-agents.ts\n\
- Promote that keeper agent to tier 2+ (keeper quorum required):\n\
  use update_agent_score + promote_agent instructions"
}

fn derive_agent_record_pda(program_id: Pubkey, agent: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"agent", agent.as_ref()], &program_id).0
}

fn redact_pubkey(pubkey: Pubkey) -> String {
    let raw = pubkey.to_string();
    if raw.len() <= 10 {
        return raw;
    }
    format!("{}…{}", &raw[..4], &raw[raw.len() - 4..])
}

fn check_pm2_isolation() {
    let home = env::var("HOME").ok().map(PathBuf::from);
    let pm2_home = env::var("PM2_HOME").ok();

    let pm2_is_shared = pm2_home.as_deref().map(Path::new).map_or(true, |pm2_path| {
        is_default_pm2_home(pm2_path, home.as_deref())
    });

    if pm2_is_shared {
        warn!(
            pm2_home = pm2_home.as_deref().unwrap_or("<unset>"),
            "keeper running in shared PM2 domain — recommend dedicated PM2_HOME for isolation"
        );
    }
}

fn is_default_pm2_home(pm2_home: &Path, home: Option<&Path>) -> bool {
    if pm2_home == Path::new("~/.pm2") || pm2_home == Path::new("$HOME/.pm2") {
        return true;
    }

    let Some(home_dir) = home else {
        return false;
    };

    let observed = canonicalize_for_compare(&expand_home_alias(pm2_home, home_dir));
    [
        PathBuf::from("~/.pm2"),
        PathBuf::from("$HOME/.pm2"),
        home_dir.join(".pm2"),
    ]
    .into_iter()
    .map(|candidate| expand_home_alias(&candidate, home_dir))
    .map(|candidate| canonicalize_for_compare(&candidate))
    .any(|candidate| candidate == observed)
}

fn expand_home_alias(path: &Path, home: &Path) -> PathBuf {
    let raw = path.to_string_lossy();

    if raw == "~" || raw == "$HOME" {
        return home.to_path_buf();
    }

    if let Some(suffix) = raw.strip_prefix("~/") {
        return home.join(suffix);
    }

    if let Some(suffix) = raw.strip_prefix("$HOME/") {
        return home.join(suffix);
    }

    path.to_path_buf()
}

fn canonicalize_for_compare(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| normalize_path(path))
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

#[cfg(unix)]
fn has_restrictive_env_permissions(mode: u32) -> bool {
    mode == 0o600
}

fn check_dotenv_permissions() {
    let env_path = env::var("KEEPER_ENV_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_KEEPER_ENV_PATH));

    let metadata = match fs::metadata(&env_path) {
        Ok(metadata) => metadata,
        Err(err) => {
            warn!(
                path = %env_path.display(),
                error = %err,
                "keeper .env file not found at expected path"
            );
            return;
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = metadata.permissions().mode() & 0o777;
        if !has_restrictive_env_permissions(mode) {
            warn!(
                path = %env_path.display(),
                mode = format_args!("{:o}", mode),
                "keeper .env permissions are not restrictive; expected mode 600"
            );
        }
    }

    #[cfg(not(unix))]
    {
        let _ = metadata;
        warn!(
            path = %env_path.display(),
            "keeper .env permission mode check is not supported on this platform"
        );
    }
}

struct SecondaryRpcRuntime<'a> {
    active_secondary_rpc: Option<&'a RpcClient>,
    mode: utils::SecondaryRpcMode,
}

fn resolve_secondary_rpc_runtime<'a>(
    secondary_rpc: Option<&'a RpcClient>,
) -> SecondaryRpcRuntime<'a> {
    if let Some(secondary) = secondary_rpc {
        utils::maybe_probe_secondary_rpc_recovery(secondary);
    }

    let mode = utils::secondary_rpc_mode(secondary_rpc.is_some());
    let active_secondary_rpc = if mode.uses_secondary_reads() {
        secondary_rpc
    } else {
        None
    };

    SecondaryRpcRuntime {
        active_secondary_rpc,
        mode,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DynamicFeeUpdate {
    base_mint_fee: u64,
    base_redeem_fee: u64,
    next_mint_fee: u64,
    next_redeem_fee: u64,
}

fn dynamic_fee_update_for_risk_level(
    risk_level: risk_manager::RiskLevel,
    base_mint_fee: u64,
    base_redeem_fee: u64,
    current_mint_fee: u64,
    current_redeem_fee: u64,
) -> Option<DynamicFeeUpdate> {
    let base_mint_u32 = u32::try_from(base_mint_fee).unwrap_or(u32::MAX);
    let base_redeem_u32 = u32::try_from(base_redeem_fee).unwrap_or(u32::MAX);
    let (next_mint_fee, next_redeem_fee) =
        risk_manager::compute_dynamic_fees(risk_level, base_mint_u32, base_redeem_u32);

    if u64::from(next_mint_fee) == current_mint_fee
        && u64::from(next_redeem_fee) == current_redeem_fee
    {
        return None;
    }

    Some(DynamicFeeUpdate {
        base_mint_fee,
        base_redeem_fee,
        next_mint_fee: u64::from(next_mint_fee),
        next_redeem_fee: u64::from(next_redeem_fee),
    })
}

fn maybe_apply_dynamic_fees(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &utils::DerivedAccounts,
    risk_level: risk_manager::RiskLevel,
    risk_manager_memory: &mut risk_manager::RiskManagerMemory,
) -> Result<Option<Signature>> {
    if keepers.len() < 2 {
        return Err(anyhow!(
            "dynamic fee update requires at least 2 keeper signers, got {}",
            keepers.len()
        ));
    }

    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    let (base_mint_fee, base_redeem_fee) = risk_manager_memory.dynamic_fee_bases(&protocol);

    let Some(update) = dynamic_fee_update_for_risk_level(
        risk_level,
        base_mint_fee,
        base_redeem_fee,
        protocol.mint_fee_rate,
        protocol.redeem_fee_rate,
    ) else {
        return Ok(None);
    };

    let keeper_one = &keepers[0];
    let keeper_two = &keepers[1];

    let ix = wire::ix_update_protocol_params(
        cfg.program_id,
        derived.protocol_state,
        keeper_one.pubkey(),
        keeper_two.pubkey(),
        wire::UpdateProtocolParamsArgs {
            new_cr_target: protocol.cr_target,
            new_mint_fee: update.next_mint_fee,
            new_redeem_fee: update.next_redeem_fee,
        },
    )
    .context("failed to build dynamic fee update_protocol_params instruction")?;

    let signature = utils::send_instructions(
        rpc,
        secondary_rpc,
        secondary_mode,
        keeper_one,
        &[keeper_one, keeper_two],
        vec![ix],
    )
    .context("failed to submit dynamic fee update_protocol_params transaction")?;

    info!(
        risk_level = ?risk_level,
        base_mint_fee = update.base_mint_fee,
        base_redeem_fee = update.base_redeem_fee,
        current_mint_fee = protocol.mint_fee_rate,
        current_redeem_fee = protocol.redeem_fee_rate,
        next_mint_fee = update.next_mint_fee,
        next_redeem_fee = update.next_redeem_fee,
        signature = %signature,
        "applied dynamic fee adjustment from risk manager"
    );

    Ok(Some(signature))
}

#[allow(clippy::too_many_arguments)]
fn run_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[solana_sdk::signature::Keypair],
    derived: &utils::DerivedAccounts,
    monitor_memory: &mut MonitorMemory,
    rebalance_memory: &mut RebalanceMemory,
    risk_manager_memory: &mut risk_manager::RiskManagerMemory,
    watchdog_memory: &mut WatchdogMemory,
    agent_loop_state: &mut AgentLoopState,
) -> Result<()> {
    info!(secondary_mode = ?secondary_mode, "cycle start");

    let mut failed_steps = Vec::new();
    let allow_mutation_txs = !matches!(secondary_mode, utils::SecondaryRpcMode::Degraded);
    if !allow_mutation_txs {
        warn!("secondary RPC is degraded; running keeper in read-only safe mode");
    }

    let mut safe_cfg = cfg.clone();
    if !allow_mutation_txs {
        safe_cfg.auto_emergency_shutdown = false;
        safe_cfg.send_watchdog_alert_tx = false;
        safe_cfg.execute_rebalance_immediately = false;
    }

    if allow_mutation_txs {
        match oracle::run_oracle_cycle(rpc, secondary_rpc, secondary_mode, cfg, keepers, derived) {
            Ok(updates) => info!(count = updates.len(), "oracle step complete"),
            Err(err) => {
                failed_steps.push("oracle");
                warn!(error = %err, "oracle step failed");
            }
        }

        match rebalance::run_rebalance_cycle(
            rpc,
            secondary_rpc,
            secondary_mode,
            cfg,
            keepers,
            derived,
            rebalance_memory,
        ) {
            Ok(outcome) => {
                if outcome.proposed {
                    info!(
                        deviation_bps = outcome.deviation_bps,
                        target_weights = ?outcome.target_weights,
                        commit_signature = ?outcome.commit_signature,
                        rebalance_signature = ?outcome.rebalance_signature,
                        "rebalance proposal generated"
                    );
                }
            }
            Err(err) => {
                failed_steps.push("rebalance");
                warn!(error = %err, "rebalance step failed");
            }
        }
    } else {
        info!("oracle/rebalance mutations skipped in degraded read-only mode");
    }

    match risk_manager::run_risk_manager_cycle(
        rpc,
        secondary_rpc,
        secondary_mode,
        cfg,
        keepers,
        derived,
        risk_manager_memory,
    ) {
        Ok(outcome) => {
            info!(
                risk_level = ?outcome.risk_level,
                global_cr_bps = outcome.global_cr_bps,
                throttle = outcome.throttle_redemptions,
                "risk manager step complete"
            );

            if allow_mutation_txs {
                if let Err(err) = maybe_apply_dynamic_fees(
                    rpc,
                    secondary_rpc,
                    secondary_mode,
                    cfg,
                    keepers,
                    derived,
                    outcome.risk_level,
                    risk_manager_memory,
                ) {
                    failed_steps.push("risk_manager");
                    warn!(error = %err, "risk manager dynamic fee update failed");
                }
            } else {
                info!("risk manager dynamic fee writes skipped in degraded read-only mode");
            }
        }
        Err(err) => {
            failed_steps.push("risk_manager");
            warn!(error = %err, "risk manager step failed");
        }
    }

    if allow_mutation_txs {
        match agent_loop::maybe_run_aig_cycle_with_tx(
            rpc,
            secondary_rpc,
            secondary_mode,
            cfg,
            keepers,
            derived,
            agent_loop_state,
        ) {
            Ok(()) => {}
            Err(err) => {
                failed_steps.push("aig");
                warn!(error = %err, "aig step failed");
            }
        }

        match agent_loop::maybe_run_tournament_cycle_with_tx(
            rpc,
            secondary_rpc,
            secondary_mode,
            cfg,
            keepers,
            derived,
            agent_loop_state,
        ) {
            Ok(()) => {}
            Err(err) => {
                failed_steps.push("tournament");
                warn!(error = %err, "tournament step failed");
            }
        }
    } else {
        info!("aig/tournament mutations skipped in degraded read-only mode");
    }

    match monitor::run_monitor_cycle(
        rpc,
        secondary_rpc,
        secondary_mode,
        &safe_cfg,
        keepers,
        derived,
        monitor_memory,
    ) {
        Ok(outcome) => {
            if outcome.circuit_breaker_triggered {
                warn!("circuit breaker active");
            }
            if !outcome.collateral_ratio_warnings.is_empty() {
                warn!(warnings = ?outcome.collateral_ratio_warnings, "collateral ratio warnings");
            }
            if let Some(sig) = outcome.emergency_shutdown_signature {
                warn!(signature = %sig, "emergency shutdown executed");
            }
        }
        Err(err) => {
            failed_steps.push("monitor");
            warn!(error = %err, "monitor step failed");
        }
    }

    match watchdog::run_watchdog_cycle(
        rpc,
        secondary_rpc,
        secondary_mode,
        &safe_cfg,
        keepers,
        derived,
        watchdog_memory,
    ) {
        Ok(outcome) => {
            if !outcome.anomalies.is_empty() {
                warn!(anomalies = ?outcome.anomalies, "watchdog anomalies");
            }
            if let Some(sig) = outcome.alert_signature {
                warn!(signature = %sig, "watchdog alert tx sent");
            }
        }
        Err(err) => {
            failed_steps.push("watchdog");
            warn!(error = %err, "watchdog step failed");
        }
    }

    info!("cycle end");

    if failed_steps.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "one or more module steps failed in cycle: {}",
            failed_steps.join(",")
        ))
    }
}
