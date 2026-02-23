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
    signature::{Keypair, Signer},
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
    let keeper_pubkeys: Vec<_> = keypairs.iter().map(|kp| kp.pubkey().to_string()).collect();
    info!(keepers = ?keeper_pubkeys, "keeper keypairs loaded");

    run_startup_preflight(&rpc, &cfg, &keypairs);

    let mut monitor_memory = MonitorMemory::default();
    let mut rebalance_memory = RebalanceMemory::default();
    let mut risk_manager_memory = risk_manager::RiskManagerMemory::default();
    let mut watchdog_memory = WatchdogMemory::default();
    let mut agent_loop_state = AgentLoopState::default();

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
                                return Err(anyhow!(
                                    "too many consecutive failed cycles ({}), exiting for operator intervention",
                                    consecutive_failed_cycles
                                ));
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
                                return Err(anyhow!(
                                    "too many consecutive failed cycles ({}), exiting for operator intervention",
                                    consecutive_failed_cycles
                                ));
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

fn run_startup_preflight(rpc: &RpcClient, cfg: &KeeperConfig, keepers: &[Keypair]) {
    if let Err(err) = preflight_keeper_agent_registration(rpc, cfg, keepers) {
        warn!(
            error = %err,
            "startup preflight could not fully verify keeper agent registration state"
        );
    }

    check_pm2_isolation();
    check_dotenv_permissions();
}

fn preflight_keeper_agent_registration(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
) -> Result<()> {
    for keeper in keepers {
        let keeper_key = keeper.pubkey();
        let agent_record = derive_agent_record_pda(cfg.program_id, keeper_key);

        let response =
            rpc.get_account_with_commitment(&agent_record, CommitmentConfig::processed())?;
        let Some(account) = response.value else {
            warn!(
                keeper_key = %keeper_key,
                "keeper key {} not registered as agent — rebalance commit will be unavailable",
                keeper_key
            );
            continue;
        };

        if account.owner != cfg.program_id {
            warn!(
                keeper_key = %keeper_key,
                owner = %account.owner,
                "keeper key {} not registered as agent — rebalance commit will be unavailable",
                keeper_key
            );
            continue;
        }

        let Ok(record) = wire::decode_account::<wire::AgentRecord>(&account.data, "AgentRecord")
        else {
            warn!(
                keeper_key = %keeper_key,
                "keeper key {} not registered as agent — rebalance commit will be unavailable",
                keeper_key
            );
            continue;
        };

        if record.tier < 2 {
            warn!(
                keeper_key = %keeper_key,
                tier = record.tier,
                "keeper key {} is tier {}, needs tier 2+ for rebalance commit",
                keeper_key,
                record.tier
            );
        }
    }

    Ok(())
}

fn derive_agent_record_pda(program_id: Pubkey, agent: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"agent", agent.as_ref()], &program_id).0
}

fn check_pm2_isolation() {
    let home = env::var("HOME").ok().map(PathBuf::from);
    let pm2_home = env::var("PM2_HOME").ok();

    let pm2_is_shared = pm2_home.as_deref().map(Path::new).map_or(true, |pm2_path| {
        is_default_pm2_home(pm2_path, home.as_deref())
    });

    if pm2_is_shared {
        warn!("keeper running in shared PM2 domain — recommend dedicated PM2_HOME for isolation");
    }
}

fn is_default_pm2_home(pm2_home: &Path, home: Option<&Path>) -> bool {
    if pm2_home == Path::new("~/.pm2") {
        return true;
    }

    if let Some(home) = home {
        if pm2_home == home.join(".pm2") {
            return true;
        }
    }

    false
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
        }
        Err(err) => {
            failed_steps.push("risk_manager");
            warn!(error = %err, "risk manager step failed");
        }
    }

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

    match monitor::run_monitor_cycle(
        rpc,
        secondary_rpc,
        secondary_mode,
        cfg,
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
        cfg,
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
