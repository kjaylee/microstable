use anyhow::{anyhow, Context, Result};
use borsh::BorshDeserialize;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{read_keypair, Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::{
    collections::HashSet,
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn, Level};
use tracing_subscriber::{fmt, EnvFilter};

pub const CROSS_RPC_NUMERIC_TOLERANCE: u64 = 1;
pub const CROSS_RPC_TIME_TOLERANCE_SECS: i64 = 1;
pub const CROSS_RPC_MAX_ATTEMPTS: usize = 3;
pub const CROSS_RPC_BACKOFF_BASE_MS: u64 = 40;

pub const TX_CONFIRM_WINDOW_BASE_SECS: u64 = 30;
pub const TX_CONFIRM_WINDOW_MAX_SECS: u64 = 60;
pub const TX_CONFIRM_POLL_INTERVAL_MS: u64 = 500;
pub const SECONDARY_RPC_DEGRADE_THRESHOLD: u64 = 3;
pub const SECONDARY_RPC_RECOVERY_PROBE_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryRpcMode {
    NoSecondaryConfigured,
    Normal,
    Degraded,
}

impl SecondaryRpcMode {
    pub fn uses_secondary_reads(self) -> bool {
        matches!(self, Self::Normal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxConfirmationDisposition {
    Confirmed,
    RetrySecondaryOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecondaryRpcHealthSnapshot {
    pub degraded: bool,
    pub consecutive_failures: u64,
}

#[derive(Debug)]
struct SecondaryRpcHealthState {
    degraded: bool,
    consecutive_failures: u64,
    last_recovery_probe_at: Option<Instant>,
}

impl Default for SecondaryRpcHealthState {
    fn default() -> Self {
        Self {
            degraded: false,
            consecutive_failures: 0,
            last_recovery_probe_at: None,
        }
    }
}

fn secondary_rpc_health_state() -> &'static Mutex<SecondaryRpcHealthState> {
    static STATE: OnceLock<Mutex<SecondaryRpcHealthState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(SecondaryRpcHealthState::default()))
}

pub fn secondary_rpc_health_snapshot() -> SecondaryRpcHealthSnapshot {
    let state = secondary_rpc_health_state()
        .lock()
        .expect("secondary rpc health mutex poisoned");

    SecondaryRpcHealthSnapshot {
        degraded: state.degraded,
        consecutive_failures: state.consecutive_failures,
    }
}

pub fn secondary_rpc_is_degraded() -> bool {
    secondary_rpc_health_snapshot().degraded
}

pub fn secondary_rpc_mode(has_secondary_rpc_configured: bool) -> SecondaryRpcMode {
    if !has_secondary_rpc_configured {
        SecondaryRpcMode::NoSecondaryConfigured
    } else if secondary_rpc_is_degraded() {
        SecondaryRpcMode::Degraded
    } else {
        SecondaryRpcMode::Normal
    }
}

pub fn reset_secondary_rpc_health_for_tests() {
    let mut state = secondary_rpc_health_state()
        .lock()
        .expect("secondary rpc health mutex poisoned");
    *state = SecondaryRpcHealthState::default();
}

pub fn register_secondary_rpc_failure() -> bool {
    let mut state = secondary_rpc_health_state()
        .lock()
        .expect("secondary rpc health mutex poisoned");

    state.consecutive_failures = state.consecutive_failures.saturating_add(1);

    if !state.degraded && state.consecutive_failures >= SECONDARY_RPC_DEGRADE_THRESHOLD {
        state.degraded = true;
        return true;
    }

    false
}

pub fn register_secondary_rpc_success() -> bool {
    let mut state = secondary_rpc_health_state()
        .lock()
        .expect("secondary rpc health mutex poisoned");

    let recovered = state.degraded;
    state.degraded = false;
    state.consecutive_failures = 0;
    state.last_recovery_probe_at = None;
    recovered
}

pub fn adaptive_secondary_confirm_window_secs(
    primary_confirmed: bool,
    secondary_confirmed_with_base_window: bool,
) -> u64 {
    if primary_confirmed && !secondary_confirmed_with_base_window {
        TX_CONFIRM_WINDOW_MAX_SECS
    } else {
        TX_CONFIRM_WINDOW_BASE_SECS
    }
}

pub fn assess_tx_confirmation_outcome(
    primary_confirmed: bool,
    secondary_confirmed: bool,
    mode: SecondaryRpcMode,
    retry_exhausted: bool,
) -> Result<TxConfirmationDisposition> {
    match mode {
        SecondaryRpcMode::NoSecondaryConfigured => {
            if primary_confirmed {
                Ok(TxConfirmationDisposition::Confirmed)
            } else {
                Err(anyhow!("transaction failed primary confirmation"))
            }
        }
        SecondaryRpcMode::Degraded => {
            if primary_confirmed || secondary_confirmed {
                Ok(TxConfirmationDisposition::Confirmed)
            } else {
                Err(anyhow!(
                    "transaction was not confirmed while running in degraded mode (primary_confirmed={}, secondary_confirmed={})",
                    primary_confirmed,
                    secondary_confirmed
                ))
            }
        }
        SecondaryRpcMode::Normal => {
            if primary_confirmed && secondary_confirmed {
                return Ok(TxConfirmationDisposition::Confirmed);
            }

            if primary_confirmed && !secondary_confirmed && !retry_exhausted {
                return Ok(TxConfirmationDisposition::RetrySecondaryOnce);
            }

            Err(anyhow!(
                "transaction did not reach dual-RPC confirmation in normal mode (primary_confirmed={}, secondary_confirmed={}, retry_exhausted={})",
                primary_confirmed,
                secondary_confirmed,
                retry_exhausted
            ))
        }
    }
}

pub fn maybe_probe_secondary_rpc_recovery(secondary: &RpcClient) {
    if !secondary_rpc_is_degraded() {
        return;
    }

    let should_probe = {
        let mut state = secondary_rpc_health_state()
            .lock()
            .expect("secondary rpc health mutex poisoned");

        if !state.degraded {
            false
        } else {
            let now = Instant::now();
            let allow_probe = state
                .last_recovery_probe_at
                .map(|at| {
                    now.duration_since(at)
                        >= Duration::from_secs(SECONDARY_RPC_RECOVERY_PROBE_INTERVAL_SECS)
                })
                .unwrap_or(true);

            if allow_probe {
                state.last_recovery_probe_at = Some(now);
            }

            allow_probe
        }
    };

    if !should_probe {
        return;
    }

    match secondary.get_latest_blockhash() {
        Ok(_) => {
            if register_secondary_rpc_success() {
                warn!("secondary RPC recovered from degraded mode; dual-RPC operations re-enabled");
            }
        }
        Err(err) => {
            warn!(
                error = %err,
                "secondary RPC recovery probe failed; staying in degraded mode"
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct DerivedAccounts {
    pub protocol_state: Pubkey,
    pub circuit_breaker: Pubkey,
    pub vaults: [Pubkey; 4],
}

impl DerivedAccounts {
    pub fn derive(program_id: &Pubkey) -> Self {
        let (protocol_state, _) = Pubkey::find_program_address(&[b"protocol_state"], program_id);
        let (circuit_breaker, _) = Pubkey::find_program_address(&[b"circuit_breaker"], program_id);
        let vaults = [0u8, 1, 2, 3]
            .map(|i| Pubkey::find_program_address(&[b"collateral_vault", &[i]], program_id).0);

        Self {
            protocol_state,
            circuit_breaker,
            vaults,
        }
    }
}

pub fn init_tracing(json_logs: bool) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::builder()
            .with_default_directive(Level::INFO.into())
            .from_env_lossy()
    });

    let formatter = fmt().with_env_filter(env_filter).with_target(false);
    if json_logs {
        formatter.json().flatten_event(true).init();
    } else {
        formatter.compact().init();
    }
}

pub fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if !raw.starts_with("~/") {
        return path.to_path_buf();
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
    PathBuf::from(home).join(raw.trim_start_matches("~/"))
}

pub fn load_keypairs(paths: &[PathBuf]) -> Result<Vec<Keypair>> {
    let mut out = Vec::with_capacity(paths.len());
    let mut seen_pubkeys = HashSet::new();

    for path in paths {
        let expanded = expand_tilde(path);
        let kp = load_keypair_secure(&expanded)?;

        if !seen_pubkeys.insert(kp.pubkey()) {
            return Err(anyhow!(
                "duplicate keeper keypair detected in config: {}",
                kp.pubkey()
            ));
        }

        out.push(kp);
    }

    Ok(out)
}

fn load_keypair_secure(path: &Path) -> Result<Keypair> {
    let file = open_keypair_file(path)?;
    validate_keypair_file_security(&file, path)?;
    read_keypair_from_fd(file, path)
}

#[cfg(unix)]
fn open_keypair_file(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .with_context(|| format!("failed to open keypair file securely: {}", path.display()))
}

#[cfg(not(unix))]
fn open_keypair_file(path: &Path) -> Result<fs::File> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat keypair path: {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "refusing symlinked keypair file: {}",
            path.display()
        ));
    }

    fs::File::open(path).with_context(|| format!("failed to open keypair file: {}", path.display()))
}

fn validate_keypair_file_security(file: &fs::File, path: &Path) -> Result<()> {
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to stat opened keypair fd: {}", path.display()))?;

    if !metadata.is_file() {
        return Err(anyhow!("keypair path is not a file: {}", path.display()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(anyhow!(
                "insecure keypair file mode {:o} for {} (must not be group/world accessible)",
                mode,
                path.display()
            ));
        }

        let owner_uid = metadata.uid();
        let effective_uid = effective_uid()?;
        if owner_uid != effective_uid {
            return Err(anyhow!(
                "keypair file {} owned by uid {}, expected uid {}",
                path.display(),
                owner_uid,
                effective_uid
            ));
        }
    }

    Ok(())
}

fn read_keypair_from_fd(file: fs::File, path: &Path) -> Result<Keypair> {
    let mut reader = BufReader::new(file);
    read_keypair(&mut reader).map_err(|e| {
        anyhow!(
            "failed to read keypair {} from opened fd: {e}",
            path.display()
        )
    })
}

#[cfg(unix)]
fn effective_uid() -> Result<u32> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to execute `id -u` for keypair owner validation")?;

    if !output.status.success() {
        return Err(anyhow!("`id -u` failed while validating keypair ownership"));
    }

    let raw = String::from_utf8(output.stdout).context("`id -u` returned non-utf8 output")?;
    let uid = raw
        .trim()
        .parse::<u32>()
        .context("failed to parse uid from `id -u` output")?;
    Ok(uid)
}

pub fn keeper_quorum_for_protocol<'a>(
    keepers: &'a [Keypair],
    protocol_keeper_set: &[Pubkey; 3],
) -> Result<(&'a Keypair, &'a Keypair)> {
    let mut members: Vec<&Keypair> = Vec::new();

    for kp in keepers {
        let pubkey = kp.pubkey();
        if protocol_keeper_set.iter().any(|k| *k == pubkey)
            && !members.iter().any(|existing| existing.pubkey() == pubkey)
        {
            members.push(kp);
        }
    }

    if members.len() < 2 {
        return Err(anyhow!(
            "configured keypairs do not satisfy protocol keeper quorum; protocol set={:?}",
            protocol_keeper_set
        ));
    }

    Ok((members[0], members[1]))
}

pub fn verify_program_deployed(rpc: &RpcClient, program_id: &Pubkey) -> Result<()> {
    let account = rpc
        .get_account(program_id)
        .with_context(|| format!("program account not found: {program_id}"))?;
    if !account.executable {
        return Err(anyhow!(
            "program account exists but is not executable: {program_id}"
        ));
    }
    Ok(())
}

pub fn fetch_account<T: BorshDeserialize>(
    rpc: &RpcClient,
    address: &Pubkey,
    account_name: &str,
) -> Result<T> {
    let account = rpc
        .get_account(address)
        .with_context(|| format!("failed to fetch account: {address}"))?;
    crate::wire::decode_account::<T>(&account.data, account_name)
        .with_context(|| format!("failed to decode {account_name} at {address}"))
}

pub fn within_u64_tolerance(primary: u64, secondary: u64, tolerance: u64) -> bool {
    primary.abs_diff(secondary) <= tolerance
}

pub fn within_i64_tolerance(primary: i64, secondary: i64, tolerance: i64) -> bool {
    primary.abs_diff(secondary) <= tolerance as u64
}

pub fn validate_protocol_state_with_tolerance(
    primary: &crate::wire::ProtocolState,
    secondary: &crate::wire::ProtocolState,
) -> Result<()> {
    for i in 0..4 {
        if !within_u64_tolerance(
            primary.weights[i],
            secondary.weights[i],
            CROSS_RPC_NUMERIC_TOLERANCE,
        ) {
            return Err(anyhow!(
                "protocol.weights[{i}] mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
                primary.weights[i],
                secondary.weights[i],
                CROSS_RPC_NUMERIC_TOLERANCE
            ));
        }
    }

    if !within_u64_tolerance(
        primary.fee_rate,
        secondary.fee_rate,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.fee_rate mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.fee_rate,
            secondary.fee_rate,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.cr_target,
        secondary.cr_target,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.cr_target mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.cr_target,
            secondary.cr_target,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.total_supply,
        secondary.total_supply,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.total_supply mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.total_supply,
            secondary.total_supply,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.last_update_slot,
        secondary.last_update_slot,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.last_update_slot mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.last_update_slot,
            secondary.last_update_slot,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if primary.keeper_set != secondary.keeper_set {
        return Err(anyhow!(
            "protocol.keeper_set mismatch (primary={:?}, secondary={:?})",
            primary.keeper_set,
            secondary.keeper_set
        ));
    }

    if primary.emergency_shutdown != secondary.emergency_shutdown {
        return Err(anyhow!(
            "protocol.emergency_shutdown mismatch (primary={}, secondary={})",
            primary.emergency_shutdown,
            secondary.emergency_shutdown
        ));
    }

    if primary.pending_rebalance_commit != secondary.pending_rebalance_commit {
        return Err(anyhow!(
            "protocol.pending_rebalance_commit mismatch (primary={:?}, secondary={:?})",
            primary.pending_rebalance_commit,
            secondary.pending_rebalance_commit
        ));
    }

    if !within_u64_tolerance(
        primary.pending_rebalance_slot,
        secondary.pending_rebalance_slot,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.pending_rebalance_slot mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.pending_rebalance_slot,
            secondary.pending_rebalance_slot,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.pending_rebalance_expiry,
        secondary.pending_rebalance_expiry,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "protocol.pending_rebalance_expiry mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.pending_rebalance_expiry,
            secondary.pending_rebalance_expiry,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if primary.bump != secondary.bump {
        return Err(anyhow!(
            "protocol.bump mismatch (primary={}, secondary={})",
            primary.bump,
            secondary.bump
        ));
    }

    Ok(())
}

pub fn validate_vault_with_tolerance(
    primary: &crate::wire::CollateralVault,
    secondary: &crate::wire::CollateralVault,
    index: usize,
) -> Result<()> {
    if primary.index != secondary.index {
        return Err(anyhow!(
            "vault[{index}].index mismatch (primary={}, secondary={})",
            primary.index,
            secondary.index
        ));
    }

    if primary.mint != secondary.mint {
        return Err(anyhow!(
            "vault[{index}].mint mismatch (primary={}, secondary={})",
            primary.mint,
            secondary.mint
        ));
    }

    if primary.vault != secondary.vault {
        return Err(anyhow!(
            "vault[{index}].vault mismatch (primary={}, secondary={})",
            primary.vault,
            secondary.vault
        ));
    }

    if primary.oracle != secondary.oracle {
        return Err(anyhow!(
            "vault[{index}].oracle mismatch (primary={}, secondary={})",
            primary.oracle,
            secondary.oracle
        ));
    }

    if !within_u64_tolerance(
        primary.risk_score,
        secondary.risk_score,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "vault[{index}].risk_score mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.risk_score,
            secondary.risk_score,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.weight_cap,
        secondary.weight_cap,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "vault[{index}].weight_cap mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.weight_cap,
            secondary.weight_cap,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.base_weight_cap,
        secondary.base_weight_cap,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "vault[{index}].base_weight_cap mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.base_weight_cap,
            secondary.base_weight_cap,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(primary.price, secondary.price, CROSS_RPC_NUMERIC_TOLERANCE) {
        return Err(anyhow!(
            "vault[{index}].price mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.price,
            secondary.price,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.confidence,
        secondary.confidence,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "vault[{index}].confidence mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.confidence,
            secondary.confidence,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.last_oracle_slot,
        secondary.last_oracle_slot,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "vault[{index}].last_oracle_slot mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.last_oracle_slot,
            secondary.last_oracle_slot,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !within_u64_tolerance(
        primary.total_deposits,
        secondary.total_deposits,
        CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "vault[{index}].total_deposits mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.total_deposits,
            secondary.total_deposits,
            CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if primary.bump != secondary.bump {
        return Err(anyhow!(
            "vault[{index}].bump mismatch (primary={}, secondary={})",
            primary.bump,
            secondary.bump
        ));
    }

    if primary.pyth_price_feed != secondary.pyth_price_feed {
        return Err(anyhow!(
            "vault[{index}].pyth_price_feed mismatch (primary={}, secondary={})",
            primary.pyth_price_feed,
            secondary.pyth_price_feed
        ));
    }

    Ok(())
}

pub fn validate_vaults_with_tolerance(
    primary: &[crate::wire::CollateralVault; 4],
    secondary: &[crate::wire::CollateralVault; 4],
) -> Result<()> {
    for i in 0..4 {
        validate_vault_with_tolerance(&primary[i], &secondary[i], i)?;
    }
    Ok(())
}

pub fn backoff_millis_for_attempt(attempt: usize, base_backoff_ms: u64) -> u64 {
    if attempt <= 1 {
        return 0;
    }

    let shift = (attempt - 2).min(62) as u32;
    base_backoff_ms.saturating_mul(1u64 << shift)
}

pub fn retry_with_backoff<T, F>(
    max_attempts: usize,
    base_backoff_ms: u64,
    mut operation: F,
) -> Result<T>
where
    F: FnMut(usize) -> Result<T>,
{
    if max_attempts == 0 {
        return Err(anyhow!("retry_with_backoff requires max_attempts > 0"));
    }

    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 1..=max_attempts {
        match operation(attempt) {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(err);

                if attempt < max_attempts {
                    let backoff_ms = backoff_millis_for_attempt(attempt + 1, base_backoff_ms);
                    if backoff_ms > 0 {
                        thread::sleep(Duration::from_millis(backoff_ms));
                    }
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("retry operation failed without explicit error")))
}

pub fn assess_dual_rpc_confirmation(
    primary_confirmed: bool,
    secondary_confirmed: bool,
    has_secondary_rpc: bool,
) -> Result<()> {
    if has_secondary_rpc {
        if !primary_confirmed && secondary_confirmed {
            return Ok(());
        }

        if primary_confirmed && secondary_confirmed {
            return Ok(());
        }

        return Err(anyhow!(
            "transaction did not reach dual-RPC confirmation (primary_confirmed={}, secondary_confirmed={})",
            primary_confirmed,
            secondary_confirmed
        ));
    }

    if primary_confirmed {
        Ok(())
    } else {
        Err(anyhow!("transaction failed primary confirmation"))
    }
}

pub fn send_instructions(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: SecondaryRpcMode,
    payer: &Keypair,
    signers: &[&Keypair],
    instructions: Vec<Instruction>,
) -> Result<Signature> {
    if instructions.is_empty() {
        return Err(anyhow!("no instructions to send"));
    }

    if let Some(secondary) = secondary_rpc {
        maybe_probe_secondary_rpc_recovery(secondary);
    }

    let secondary_configured = !matches!(secondary_mode, SecondaryRpcMode::NoSecondaryConfigured);
    let mut confirmation_mode = secondary_rpc_mode(secondary_configured);
    let active_secondary = if confirmation_mode.uses_secondary_reads() {
        secondary_rpc
    } else {
        None
    };

    let blockhash = rpc
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        signers,
        blockhash,
    );

    let send_cfg = RpcSendTransactionConfig {
        skip_preflight: false,
        preflight_commitment: Some(CommitmentConfig::processed().commitment),
        ..RpcSendTransactionConfig::default()
    };

    let primary_send = rpc.send_transaction_with_config(&tx, send_cfg.clone());
    let mut primary_sig = None;
    let mut primary_send_err = None;

    match primary_send {
        Ok(sig) => {
            primary_sig = Some(sig);
        }
        Err(err) => {
            primary_send_err = Some(err);
        }
    }

    let mut secondary_sig = None;
    let mut secondary_send_err = None;
    if let Some(secondary) = active_secondary {
        match secondary.send_transaction_with_config(&tx, send_cfg) {
            Ok(sig) => {
                secondary_sig = Some(sig);
            }
            Err(err) => {
                secondary_send_err = Some(err);
            }
        }
    }

    let sig = primary_sig.or(secondary_sig).ok_or_else(|| {
        let primary_err = primary_send_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "none".to_string());
        let secondary_err = secondary_send_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "none".to_string());

        anyhow!(
            "failed to submit transaction on both RPCs: primary={primary_err}, secondary={secondary_err}"
        )
    })?;

    let primary_confirmed = match confirm_signature_with_window(
        rpc,
        &sig,
        Duration::from_secs(TX_CONFIRM_WINDOW_BASE_SECS),
    ) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                signature = %sig,
                error = %err,
                "primary RPC confirmation errored; treating as unconfirmed"
            );
            false
        }
    };

    let mut secondary_confirmed = false;
    let mut secondary_failure_detail: Option<String> = None;

    if let Some(secondary) = active_secondary {
        let base_secondary_confirmation = confirm_signature_with_window(
            secondary,
            &sig,
            Duration::from_secs(TX_CONFIRM_WINDOW_BASE_SECS),
        );

        let mut base_secondary_confirmed = false;
        match base_secondary_confirmation {
            Ok(true) => {
                base_secondary_confirmed = true;
                secondary_confirmed = true;
            }
            Ok(false) => {
                secondary_failure_detail =
                    Some("secondary RPC timeout within base confirmation window".to_string());
            }
            Err(err) => {
                secondary_failure_detail = Some(format!("secondary RPC confirmation error: {err}"));
            }
        }

        if !secondary_confirmed {
            let extended_window_secs =
                adaptive_secondary_confirm_window_secs(primary_confirmed, base_secondary_confirmed);

            if extended_window_secs > TX_CONFIRM_WINDOW_BASE_SECS {
                info!(
                    signature = %sig,
                    base_window_secs = TX_CONFIRM_WINDOW_BASE_SECS,
                    extended_window_secs,
                    "secondary RPC confirmation is lagging; extending confirm window"
                );

                let extra_window_secs =
                    extended_window_secs.saturating_sub(TX_CONFIRM_WINDOW_BASE_SECS);
                match confirm_signature_with_window(
                    secondary,
                    &sig,
                    Duration::from_secs(extra_window_secs),
                ) {
                    Ok(true) => {
                        secondary_confirmed = true;
                        secondary_failure_detail = None;
                    }
                    Ok(false) => {
                        secondary_failure_detail = Some(
                            "secondary RPC timeout after adaptive confirmation window".to_string(),
                        );
                    }
                    Err(err) => {
                        secondary_failure_detail = Some(format!(
                            "secondary RPC error during adaptive extension: {err}"
                        ));
                    }
                }
            }
        }

        if matches!(
            assess_tx_confirmation_outcome(
                primary_confirmed,
                secondary_confirmed,
                confirmation_mode,
                false
            ),
            Ok(TxConfirmationDisposition::RetrySecondaryOnce)
        ) {
            warn!(
                signature = %sig,
                "secondary confirmation missing in normal mode; soft-failing and retrying once"
            );

            match confirm_signature_with_window(
                secondary,
                &sig,
                Duration::from_secs(TX_CONFIRM_WINDOW_BASE_SECS),
            ) {
                Ok(true) => {
                    secondary_confirmed = true;
                    secondary_failure_detail = None;
                }
                Ok(false) => {
                    secondary_failure_detail =
                        Some("secondary RPC timeout on soft-fail retry".to_string());
                }
                Err(err) => {
                    secondary_failure_detail =
                        Some(format!("secondary RPC confirmation retry error: {err}"));
                }
            }
        }

        if secondary_confirmed {
            if register_secondary_rpc_success() {
                info!("secondary RPC recovered from degraded mode after successful confirmation");
            }
        } else {
            let entered_degraded = register_secondary_rpc_failure();
            warn!(
                signature = %sig,
                failures = secondary_rpc_health_snapshot().consecutive_failures,
                threshold = SECONDARY_RPC_DEGRADE_THRESHOLD,
                detail = %secondary_failure_detail.as_deref().unwrap_or("unknown"),
                "secondary RPC confirmation failed"
            );

            if entered_degraded {
                warn!(
                    threshold = SECONDARY_RPC_DEGRADE_THRESHOLD,
                    "secondary RPC entered degraded mode after consecutive failures"
                );
            }
        }
    }

    confirmation_mode = secondary_rpc_mode(secondary_configured);
    assess_tx_confirmation_outcome(
        primary_confirmed,
        secondary_confirmed,
        confirmation_mode,
        true,
    )
    .map_err(|err| anyhow!("transaction {}: {err}", sig))?;

    if primary_confirmed && !secondary_confirmed {
        match confirmation_mode {
            SecondaryRpcMode::Degraded => {
                warn!(
                    signature = %sig,
                    "transaction confirmed on primary RPC while secondary is degraded"
                );
            }
            SecondaryRpcMode::Normal => {}
            SecondaryRpcMode::NoSecondaryConfigured => {}
        }
    }

    if !primary_confirmed && secondary_confirmed {
        info!(
            signature = %sig,
            "primary confirmation failed, recovered via secondary RPC confirmation"
        );
    }

    Ok(sig)
}

fn confirm_signature_with_window(
    rpc: &RpcClient,
    sig: &Signature,
    window: Duration,
) -> Result<bool> {
    let deadline = Instant::now()
        .checked_add(window)
        .unwrap_or_else(|| Instant::now() + Duration::from_secs(TX_CONFIRM_WINDOW_MAX_SECS));
    let poll_interval = Duration::from_millis(TX_CONFIRM_POLL_INTERVAL_MS);

    let mut last_error: Option<anyhow::Error> = None;

    loop {
        match rpc.confirm_transaction_with_commitment(sig, CommitmentConfig::confirmed()) {
            Ok(response) => {
                if response.value {
                    return Ok(true);
                }
            }
            Err(err) => {
                last_error = Some(anyhow!(err));
            }
        }

        let now = Instant::now();
        if now >= deadline {
            break;
        }

        let sleep_for = std::cmp::min(deadline.saturating_duration_since(now), poll_interval);
        if sleep_for.is_zero() {
            break;
        }
        thread::sleep(sleep_for);
    }

    if let Some(err) = last_error {
        return Err(anyhow!(
            "RPC confirmation polling failed within {:?}: {}",
            window,
            err
        ));
    }

    Ok(false)
}

const TRUSTED_CARGO_REGISTRY_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";
const EMBEDDED_CARGO_LOCK_SHA256: &str = env!("KEEPER_CARGO_LOCK_HASH");

pub fn enforce_supply_chain_controls() -> Result<()> {
    let lock_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let lockfile = fs::read_to_string(&lock_path)
        .with_context(|| format!("failed to read Cargo.lock: {}", lock_path.display()))?;

    let expected_lock_hash =
        normalize_sha256_hex(EMBEDDED_CARGO_LOCK_SHA256, "compile-time Cargo.lock hash")?;
    verify_cargo_lock_attestation_for_bytes(lockfile.as_bytes(), &expected_lock_hash)
        .with_context(|| format!("Cargo.lock attestation failed for {}", lock_path.display()))?;

    validate_lockfile_dependency_sources(&lockfile)?;

    info!(
        cargo_lock = %lock_path.display(),
        cargo_lock_hash = %expected_lock_hash,
        "supply-chain controls verified (Cargo.lock attestation)"
    );

    Ok(())
}

pub fn verify_cargo_lock_attestation_for_bytes(
    lockfile_bytes: &[u8],
    expected_sha256_hex: &str,
) -> Result<()> {
    let expected = normalize_sha256_hex(expected_sha256_hex, "Cargo.lock attestation hash")?;
    let actual = sha256_hex(lockfile_bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(anyhow!(
            "Cargo.lock sha256 mismatch: expected {}, got {}",
            expected,
            actual
        ))
    }
}

// Legacy helper retained for previous-version test coverage.
pub fn resolve_expected_binary_sha256(
    embedded_expected: Option<&str>,
    env_expected: Option<&str>,
    file_expected: Option<&str>,
) -> Result<String> {
    let embedded = embedded_expected
        .map(|value| normalize_sha256_hex(value, "compile-time embedded hash"))
        .transpose()?;
    let env = env_expected
        .map(|value| normalize_sha256_hex(value, "KEEPER_BINARY_SHA256"))
        .transpose()?;
    let file = file_expected
        .map(|value| normalize_sha256_hex(value, "KEEPER_BINARY_SHA256_FILE"))
        .transpose()?;

    if let Some(embedded_hash) = embedded {
        if let Some(env_hash) = &env {
            if env_hash != &embedded_hash {
                return Err(anyhow!(
                    "binary hash mismatch: env KEEPER_BINARY_SHA256 does not match embedded trusted hash"
                ));
            }
        }
        if let Some(file_hash) = &file {
            if file_hash != &embedded_hash {
                return Err(anyhow!(
                    "binary hash mismatch: hash file does not match embedded trusted hash"
                ));
            }
        }

        return Ok(embedded_hash);
    }

    let env_hash = env.ok_or_else(|| {
        anyhow!(
            "missing KEEPER_BINARY_SHA256; binary attestation requires env+file dual verification"
        )
    })?;
    let file_hash = file
        .ok_or_else(|| anyhow!("missing trusted hash file content; binary attestation requires env+file dual verification"))?;

    if env_hash != file_hash {
        return Err(anyhow!(
            "binary hash mismatch between env KEEPER_BINARY_SHA256 and hash file"
        ));
    }

    Ok(env_hash)
}

pub fn validate_lockfile_dependency_sources(lockfile: &str) -> Result<()> {
    for (idx, line) in lockfile.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("source = ") {
            continue;
        }

        let source = parse_lockfile_source(trimmed).ok_or_else(|| {
            anyhow!(
                "invalid source entry format in Cargo.lock at line {}: {}",
                idx + 1,
                trimmed
            )
        })?;

        if source.starts_with("registry+") {
            if source != TRUSTED_CARGO_REGISTRY_SOURCE {
                return Err(anyhow!(
                    "unsupported registry source in Cargo.lock at line {}: {}",
                    idx + 1,
                    source
                ));
            }
            continue;
        }

        if source.starts_with("git+") || source.starts_with("path+") {
            return Err(anyhow!(
                "unsupported dependency source in Cargo.lock at line {}: {}",
                idx + 1,
                source
            ));
        }

        return Err(anyhow!(
            "unsupported dependency source scheme in Cargo.lock at line {}: {}",
            idx + 1,
            source
        ));
    }

    Ok(())
}

fn parse_lockfile_source(trimmed_source_line: &str) -> Option<&str> {
    let value = trimmed_source_line.strip_prefix("source = ")?.trim();
    value.strip_prefix('"')?.strip_suffix('"')
}

fn normalize_sha256_hex(raw: &str, source: &str) -> Result<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!("invalid sha256 hex in {source}"));
    }

    Ok(normalized)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = hash(bytes).to_bytes();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn verify_binary_attestation_for_bytes(binary: &[u8], expected_sha256_hex: &str) -> Result<()> {
    let actual = sha256_hex(binary);
    if actual == expected_sha256_hex.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(anyhow!(
            "binary sha256 mismatch: expected {}, got {}",
            expected_sha256_hex,
            actual
        ))
    }
}

pub fn parse_pubkey(raw: &str) -> Result<Pubkey> {
    Pubkey::from_str(raw).with_context(|| format!("invalid pubkey: {raw}"))
}
