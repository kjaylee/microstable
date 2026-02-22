mod config;
mod monitor;
mod oracle;
mod rebalance;
mod utils;
mod watchdog;
mod wire;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use config::KeeperConfig;
use monitor::MonitorMemory;
use rebalance::RebalanceMemory;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, signature::Signer};
use std::{path::PathBuf, time::Duration};
use tokio::time::MissedTickBehavior;
use tracing::{error, info, warn};
use watchdog::WatchdogMemory;

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

    let cfg = KeeperConfig::load(cli.config.as_deref())?;
    info!(
        primary_rpc = %cfg.rpc_url,
        secondary_rpc = ?cfg.secondary_rpc_url,
        program_id = %cfg.program_id,
        "config loaded"
    );

    let rpc = RpcClient::new_with_commitment(cfg.rpc_url.clone(), CommitmentConfig::confirmed());
    let secondary_rpc = cfg.secondary_rpc_url.as_ref().map(|url| {
        RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed())
    });

    let epoch_info = rpc.get_epoch_info().context("primary RPC health check failed")?;
    info!(
        epoch = epoch_info.epoch,
        absolute_slot = epoch_info.absolute_slot,
        "primary rpc connected"
    );

    if let Some(secondary) = &secondary_rpc {
        let epoch_info = secondary
            .get_epoch_info()
            .context("secondary RPC health check failed")?;
        info!(
            epoch = epoch_info.epoch,
            absolute_slot = epoch_info.absolute_slot,
            "secondary rpc connected"
        );
    }

    utils::verify_program_deployed(&rpc, &cfg.program_id)?;
    if let Some(secondary) = &secondary_rpc {
        utils::verify_program_deployed(secondary, &cfg.program_id)?;
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

    let mut monitor_memory = MonitorMemory::default();
    let mut rebalance_memory = RebalanceMemory::default();
    let mut watchdog_memory = WatchdogMemory::default();

    if cli.once {
        run_cycle(
            &rpc,
            secondary_rpc.as_ref(),
            &cfg,
            &keypairs,
            &derived,
            &mut monitor_memory,
            &mut rebalance_memory,
            &mut watchdog_memory,
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
                    match run_cycle(
                        &rpc,
                        secondary_rpc.as_ref(),
                        &cfg,
                        &keypairs,
                        &derived,
                        &mut monitor_memory,
                        &mut rebalance_memory,
                        &mut watchdog_memory,
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
                    match run_cycle(
                        &rpc,
                        secondary_rpc.as_ref(),
                        &cfg,
                        &keypairs,
                        &derived,
                        &mut monitor_memory,
                        &mut rebalance_memory,
                        &mut watchdog_memory,
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

#[allow(clippy::too_many_arguments)]
fn run_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    cfg: &KeeperConfig,
    keepers: &[solana_sdk::signature::Keypair],
    derived: &utils::DerivedAccounts,
    monitor_memory: &mut MonitorMemory,
    rebalance_memory: &mut RebalanceMemory,
    watchdog_memory: &mut WatchdogMemory,
) -> Result<()> {
    info!("cycle start");

    let mut failed_steps = Vec::new();

    match oracle::run_oracle_cycle(rpc, secondary_rpc, cfg, keepers, derived) {
        Ok(updates) => info!(count = updates.len(), "oracle step complete"),
        Err(err) => {
            failed_steps.push("oracle");
            warn!(error = %err, "oracle step failed");
        }
    }

    match rebalance::run_rebalance_cycle(rpc, secondary_rpc, cfg, keepers, derived, rebalance_memory) {
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

    match monitor::run_monitor_cycle(
        rpc,
        secondary_rpc,
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
