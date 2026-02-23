use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::{hash::hashv, pubkey, pubkey::Pubkey};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};
use tracing::error;

pub const DEFAULT_CONFIG_PATH: &str = "keeper/config.devnet.json";

const DEFAULT_ORACLE_PUBLISH_MAX_AGE_SECS: u64 = 60;
const DEFAULT_ORACLE_CONFIDENCE_MAX_BPS: u64 = 500;
const DEFAULT_EMERGENCY_COLLATERAL_RATIO_BPS: u64 = 10_000;
const DEFAULT_EMERGENCY_DEBOUNCE_CYCLES: u64 = 3;
const DEFAULT_COMMIT_REVEAL_DELAY_SLOTS: u64 = 5;
const DEFAULT_WATCHDOG_ORACLE_STALE_SLOTS: u64 = 120;
const DEFAULT_WATCHDOG_WEIGHT_SHIFT_BPS: u64 = 600;
const DEFAULT_WATCHDOG_HISTORY_LIMIT: usize = 64;
const DEFAULT_MAX_CONSECUTIVE_FAILED_CYCLES: u64 = 5;
const DEFAULT_OPTIMIZER_ENABLED: bool = true;
const DEFAULT_AIG_ENABLED: bool = false;
const DEFAULT_TOURNAMENT_ENABLED: bool = false;
const DEFAULT_AIG_INTERVAL_SECS: u64 = 3_600;
const DEFAULT_TOURNAMENT_INTERVAL_SECS: u64 = 86_400;
const DEFAULT_KEY_SIGNATURES_MAX_PER_EPOCH: u64 = 50_000;
const DEFAULT_KEY_ROTATION_GRACE_EPOCHS: u64 = 2;
const DEFAULT_KEY_ANOMALY_WINDOW_SECS: u64 = 60;
const DEFAULT_KEY_ANOMALY_BURST_THRESHOLD: u64 = 200;

const MAX_WATCHDOG_HISTORY_LIMIT: usize = 4_096;
const MIN_TICK_INTERVAL_SECS: u64 = 5;
const MAX_TICK_INTERVAL_SECS: u64 = 300;
const MIN_STALENESS_SECS: u64 = 10;
const MAX_STALENESS_SECS: u64 = 300;
const MIN_ORACLE_CONFIDENCE_BPS: u64 = 1;
const MAX_ORACLE_CONFIDENCE_BPS: u64 = 1_000;
const MIN_EMERGENCY_CR_BPS: u64 = 10_000;
const MAX_EMERGENCY_CR_BPS: u64 = 20_000;
const MAX_COMMIT_VALID_FOR_SLOTS: u64 = 1_000;
const MAX_CONSECUTIVE_FAILED_CYCLES: u64 = 100;
const MAX_KEY_SIGNATURES_PER_EPOCH: u64 = 5_000_000;
const MAX_KEY_ROTATION_GRACE_EPOCHS: u64 = 64;
const MAX_KEY_ANOMALY_WINDOW_SECS: u64 = 3_600;
const MAX_KEY_ANOMALY_BURST_THRESHOLD: u64 = 10_000;
const EXPECTED_VAULT_COUNT: usize = 4;
const CONFIG_HMAC_ENV_KEY: &str = "MICROSTABLE_CONFIG_HMAC_KEY";
const CONFIG_HMAC_ALLOW_UNSIGNED_ENV: &str = "MICROSTABLE_ALLOW_UNSIGNED_CONFIG";
const CONFIG_SIGNATURE_SUFFIX: &str = ".sig";
const RPC_ALLOWLIST: [&str; 3] = [
    "api.devnet.solana.com",
    "devnet.rpcpool.com",
    "rpc.ankr.com",
];
const FORBIDDEN_KEYPAIR_PREFIXES: [&str; 3] = ["/tmp/", "/var/tmp/", "/dev/shm/"];

#[derive(Debug, Clone)]
pub struct KeeperConfig {
    pub rpc_url: String,
    pub secondary_rpc_url: Option<String>,
    pub program_id: Pubkey,
    pub keeper_keypairs: Vec<PathBuf>,
    pub pyth_feeds: Vec<PythFeedConfig>,
    pub tick_interval_secs: u64,
    pub oracle_max_age_secs: u64,
    pub oracle_publish_max_age_secs: u64,
    pub oracle_confidence_max_bps: u64,
    pub min_collateral_ratio_bps: u64,
    pub emergency_collateral_ratio_bps: u64,
    pub emergency_debounce_cycles: u64,
    pub rebalance_deviation_bps: u64,
    pub max_rebalance_slippage_bps: u64,
    pub commit_valid_for_slots: u64,
    pub commit_reveal_delay_slots: u64,
    pub auto_emergency_shutdown: bool,
    pub send_watchdog_alert_tx: bool,
    pub execute_rebalance_immediately: bool,
    pub optimizer_enabled: bool,
    pub aig_enabled: bool,
    pub tournament_enabled: bool,
    pub aig_interval_secs: u64,
    pub tournament_interval_secs: u64,
    pub watchdog_supply_spike_bps: u64,
    pub watchdog_cr_drop_bps: u64,
    pub watchdog_oracle_stale_slots: u64,
    pub watchdog_weight_shift_bps: u64,
    pub watchdog_history_limit: usize,
    pub max_consecutive_failed_cycles: u64,
    pub allowed_upgrade_authority_multisigs: Vec<Pubkey>,
    pub key_signatures_max_per_epoch: u64,
    pub key_rotation_cutover_epoch: Option<u64>,
    pub key_rotation_grace_epochs: u64,
    pub key_rotation_next_pubkeys: Vec<Pubkey>,
    pub key_anomaly_window_secs: u64,
    pub key_anomaly_burst_threshold: u64,
}

#[derive(Debug, Clone)]
pub struct PythFeedConfig {
    pub symbol: String,
    pub collateral_index: u8,
    pub price_account: Pubkey,
    pub max_age_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeeperConfigFile {
    rpc_url: String,
    secondary_rpc_url: Option<String>,
    program_id: String,
    keeper_keypairs: Vec<String>,
    pyth_feeds: Vec<PythFeedFile>,
    tick_interval_secs: u64,
    oracle_max_age_secs: u64,
    oracle_publish_max_age_secs: Option<u64>,
    oracle_confidence_max_bps: Option<u64>,
    min_collateral_ratio_bps: u64,
    emergency_collateral_ratio_bps: Option<u64>,
    emergency_debounce_cycles: Option<u64>,
    rebalance_deviation_bps: u64,
    max_rebalance_slippage_bps: u64,
    commit_valid_for_slots: u64,
    commit_reveal_delay_slots: Option<u64>,
    auto_emergency_shutdown: bool,
    send_watchdog_alert_tx: bool,
    execute_rebalance_immediately: bool,
    optimizer_enabled: Option<bool>,
    aig_enabled: Option<bool>,
    tournament_enabled: Option<bool>,
    aig_interval_secs: Option<u64>,
    tournament_interval_secs: Option<u64>,
    watchdog_supply_spike_bps: u64,
    watchdog_cr_drop_bps: u64,
    watchdog_oracle_stale_slots: Option<u64>,
    watchdog_weight_shift_bps: Option<u64>,
    watchdog_history_limit: Option<usize>,
    max_consecutive_failed_cycles: Option<u64>,
    allowed_upgrade_authority_multisigs: Option<Vec<String>>,
    key_signatures_max_per_epoch: Option<u64>,
    key_rotation_cutover_epoch: Option<u64>,
    key_rotation_grace_epochs: Option<u64>,
    key_rotation_next_pubkeys: Option<Vec<String>>,
    key_anomaly_window_secs: Option<u64>,
    key_anomaly_burst_threshold: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PythFeedFile {
    symbol: String,
    collateral_index: u8,
    price_account: String,
    max_age_secs: Option<u64>,
}

impl KeeperConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = path.unwrap_or_else(|| Path::new(DEFAULT_CONFIG_PATH));
        if !config_path.exists() {
            return Err(anyhow!(
                "keeper config missing: {} (fail-closed). Run with --init-config to bootstrap an explicit default file.",
                config_path.display()
            ));
        }

        let content = fs::read_to_string(config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;
        verify_config_integrity(config_path, &content)?;
        let file: KeeperConfigFile = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
        Self::from_file(file)
    }

    pub fn init_default(path: Option<&Path>) -> Result<PathBuf> {
        let config_path = path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

        if config_path.exists() {
            return Err(anyhow!(
                "refusing to overwrite existing config in --init-config mode: {}",
                config_path.display()
            ));
        }

        let default = Self::default_devnet();
        let serialized = serde_json::to_string_pretty(&default.to_file())?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, serialized).with_context(|| {
            format!("failed to write default config: {}", config_path.display())
        })?;

        Ok(config_path)
    }

    pub fn default_devnet() -> Self {
        Self {
            rpc_url: "https://api.devnet.solana.com".to_string(),
            secondary_rpc_url: Some("https://devnet.rpcpool.com".to_string()),
            program_id: pubkey!("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3"),
            keeper_keypairs: vec![
                PathBuf::from("~/.config/solana/devnet-keypair.json"),
                PathBuf::from("keeper/keys-2/keeper2.json"),
                PathBuf::from("keeper/keys-3/keeper3.json"),
            ],
            pyth_feeds: vec![
                PythFeedConfig {
                    symbol: "USDC/USD".to_string(),
                    collateral_index: 0,
                    price_account: pubkey!("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX"),
                    max_age_secs: 120,
                },
                PythFeedConfig {
                    symbol: "USDT/USD".to_string(),
                    collateral_index: 1,
                    price_account: pubkey!("HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM"),
                    max_age_secs: 120,
                },
                PythFeedConfig {
                    symbol: "DAI/USD".to_string(),
                    collateral_index: 2,
                    price_account: pubkey!("FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN"),
                    max_age_secs: 120,
                },
                PythFeedConfig {
                    symbol: "USDS/USD".to_string(),
                    collateral_index: 3,
                    price_account: pubkey!("9h4r3d4s8Jc8k5YfVY6Bnd3ETf6gVfGvSzj8Pzpo7aQw"),
                    max_age_secs: 120,
                },
            ],
            tick_interval_secs: 30,
            oracle_max_age_secs: 120,
            oracle_publish_max_age_secs: DEFAULT_ORACLE_PUBLISH_MAX_AGE_SECS,
            oracle_confidence_max_bps: DEFAULT_ORACLE_CONFIDENCE_MAX_BPS,
            min_collateral_ratio_bps: 10_500,
            emergency_collateral_ratio_bps: DEFAULT_EMERGENCY_COLLATERAL_RATIO_BPS,
            emergency_debounce_cycles: DEFAULT_EMERGENCY_DEBOUNCE_CYCLES,
            rebalance_deviation_bps: 300,
            max_rebalance_slippage_bps: 200,
            commit_valid_for_slots: 200,
            commit_reveal_delay_slots: DEFAULT_COMMIT_REVEAL_DELAY_SLOTS,
            auto_emergency_shutdown: false,
            send_watchdog_alert_tx: false,
            execute_rebalance_immediately: false,
            optimizer_enabled: DEFAULT_OPTIMIZER_ENABLED,
            aig_enabled: DEFAULT_AIG_ENABLED,
            tournament_enabled: DEFAULT_TOURNAMENT_ENABLED,
            aig_interval_secs: DEFAULT_AIG_INTERVAL_SECS,
            tournament_interval_secs: DEFAULT_TOURNAMENT_INTERVAL_SECS,
            watchdog_supply_spike_bps: 2_500,
            watchdog_cr_drop_bps: 1_500,
            watchdog_oracle_stale_slots: DEFAULT_WATCHDOG_ORACLE_STALE_SLOTS,
            watchdog_weight_shift_bps: DEFAULT_WATCHDOG_WEIGHT_SHIFT_BPS,
            watchdog_history_limit: DEFAULT_WATCHDOG_HISTORY_LIMIT,
            max_consecutive_failed_cycles: DEFAULT_MAX_CONSECUTIVE_FAILED_CYCLES,
            allowed_upgrade_authority_multisigs: Vec::new(),
            key_signatures_max_per_epoch: DEFAULT_KEY_SIGNATURES_MAX_PER_EPOCH,
            key_rotation_cutover_epoch: None,
            key_rotation_grace_epochs: DEFAULT_KEY_ROTATION_GRACE_EPOCHS,
            key_rotation_next_pubkeys: Vec::new(),
            key_anomaly_window_secs: DEFAULT_KEY_ANOMALY_WINDOW_SECS,
            key_anomaly_burst_threshold: DEFAULT_KEY_ANOMALY_BURST_THRESHOLD,
        }
    }

    fn from_file(file: KeeperConfigFile) -> Result<Self> {
        let program_id = Pubkey::from_str(&file.program_id)
            .with_context(|| format!("invalid program id: {}", file.program_id))?;

        let mut pyth_feeds = Vec::with_capacity(file.pyth_feeds.len());
        for feed in file.pyth_feeds {
            let price_account = Pubkey::from_str(&feed.price_account)
                .with_context(|| format!("invalid pyth price account: {}", feed.price_account))?;
            pyth_feeds.push(PythFeedConfig {
                symbol: feed.symbol,
                collateral_index: feed.collateral_index,
                price_account,
                max_age_secs: feed.max_age_secs.unwrap_or(file.oracle_max_age_secs),
            });
        }

        let allowed_upgrade_authority_multisigs = parse_pubkey_list(
            file.allowed_upgrade_authority_multisigs.as_deref(),
            "allowed_upgrade_authority_multisigs",
        )?;
        let key_rotation_next_pubkeys = parse_pubkey_list(
            file.key_rotation_next_pubkeys.as_deref(),
            "key_rotation_next_pubkeys",
        )?;

        let cfg = Self {
            rpc_url: file.rpc_url,
            secondary_rpc_url: file.secondary_rpc_url,
            program_id,
            keeper_keypairs: file
                .keeper_keypairs
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            pyth_feeds,
            tick_interval_secs: file.tick_interval_secs,
            oracle_max_age_secs: file.oracle_max_age_secs,
            oracle_publish_max_age_secs: file
                .oracle_publish_max_age_secs
                .unwrap_or(DEFAULT_ORACLE_PUBLISH_MAX_AGE_SECS),
            oracle_confidence_max_bps: file
                .oracle_confidence_max_bps
                .unwrap_or(DEFAULT_ORACLE_CONFIDENCE_MAX_BPS),
            min_collateral_ratio_bps: file.min_collateral_ratio_bps,
            emergency_collateral_ratio_bps: file
                .emergency_collateral_ratio_bps
                .unwrap_or(DEFAULT_EMERGENCY_COLLATERAL_RATIO_BPS),
            emergency_debounce_cycles: file
                .emergency_debounce_cycles
                .unwrap_or(DEFAULT_EMERGENCY_DEBOUNCE_CYCLES),
            rebalance_deviation_bps: file.rebalance_deviation_bps,
            max_rebalance_slippage_bps: file.max_rebalance_slippage_bps,
            commit_valid_for_slots: file.commit_valid_for_slots,
            commit_reveal_delay_slots: file
                .commit_reveal_delay_slots
                .unwrap_or(DEFAULT_COMMIT_REVEAL_DELAY_SLOTS),
            auto_emergency_shutdown: file.auto_emergency_shutdown,
            send_watchdog_alert_tx: file.send_watchdog_alert_tx,
            execute_rebalance_immediately: file.execute_rebalance_immediately,
            optimizer_enabled: file.optimizer_enabled.unwrap_or(DEFAULT_OPTIMIZER_ENABLED),
            aig_enabled: file.aig_enabled.unwrap_or(DEFAULT_AIG_ENABLED),
            tournament_enabled: file
                .tournament_enabled
                .unwrap_or(DEFAULT_TOURNAMENT_ENABLED),
            aig_interval_secs: file.aig_interval_secs.unwrap_or(DEFAULT_AIG_INTERVAL_SECS),
            tournament_interval_secs: file
                .tournament_interval_secs
                .unwrap_or(DEFAULT_TOURNAMENT_INTERVAL_SECS),
            watchdog_supply_spike_bps: file.watchdog_supply_spike_bps,
            watchdog_cr_drop_bps: file.watchdog_cr_drop_bps,
            watchdog_oracle_stale_slots: file
                .watchdog_oracle_stale_slots
                .unwrap_or(DEFAULT_WATCHDOG_ORACLE_STALE_SLOTS),
            watchdog_weight_shift_bps: file
                .watchdog_weight_shift_bps
                .unwrap_or(DEFAULT_WATCHDOG_WEIGHT_SHIFT_BPS),
            watchdog_history_limit: file
                .watchdog_history_limit
                .unwrap_or(DEFAULT_WATCHDOG_HISTORY_LIMIT),
            max_consecutive_failed_cycles: file
                .max_consecutive_failed_cycles
                .unwrap_or(DEFAULT_MAX_CONSECUTIVE_FAILED_CYCLES),
            allowed_upgrade_authority_multisigs,
            key_signatures_max_per_epoch: file
                .key_signatures_max_per_epoch
                .unwrap_or(DEFAULT_KEY_SIGNATURES_MAX_PER_EPOCH),
            key_rotation_cutover_epoch: file.key_rotation_cutover_epoch,
            key_rotation_grace_epochs: file
                .key_rotation_grace_epochs
                .unwrap_or(DEFAULT_KEY_ROTATION_GRACE_EPOCHS),
            key_rotation_next_pubkeys,
            key_anomaly_window_secs: file
                .key_anomaly_window_secs
                .unwrap_or(DEFAULT_KEY_ANOMALY_WINDOW_SECS),
            key_anomaly_burst_threshold: file
                .key_anomaly_burst_threshold
                .unwrap_or(DEFAULT_KEY_ANOMALY_BURST_THRESHOLD),
        };

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if self.rpc_url.trim().is_empty() {
            return Err(anyhow!("rpc_url cannot be empty"));
        }

        let secondary = self
            .secondary_rpc_url
            .as_ref()
            .ok_or_else(|| anyhow!("secondary_rpc_url is required and cannot be null"))?;

        if secondary.trim().is_empty() {
            return Err(anyhow!("secondary_rpc_url cannot be empty"));
        }
        if *secondary == self.rpc_url {
            return Err(anyhow!(
                "secondary_rpc_url must differ from rpc_url for cross-validation"
            ));
        }

        let primary_host = extract_https_host(&self.rpc_url)?;
        let secondary_host = extract_https_host(secondary)?;
        if primary_host.eq_ignore_ascii_case(secondary_host) {
            return Err(anyhow!(
                "rpc_url and secondary_rpc_url must use distinct hosts (got {})",
                primary_host
            ));
        }

        validate_rpc_allowlist(&self.rpc_url)?;
        validate_rpc_allowlist(secondary)?;

        if self.keeper_keypairs.len() != 3 {
            return Err(anyhow!(
                "keeper_keypairs must contain exactly 3 entries for 2-of-3 quorum hardening"
            ));
        }
        validate_keypair_path_policy(&self.keeper_keypairs)?;

        if self.pyth_feeds.len() < EXPECTED_VAULT_COUNT {
            let configured: HashSet<u8> = self
                .pyth_feeds
                .iter()
                .map(|feed| feed.collateral_index)
                .collect();
            let missing: Vec<u8> = (0..EXPECTED_VAULT_COUNT as u8)
                .filter(|idx| !configured.contains(idx))
                .collect();
            error!(
                expected_vault_count = EXPECTED_VAULT_COUNT,
                configured_feed_count = self.pyth_feeds.len(),
                missing_vault_indices = ?missing,
                "keeper config invalid: missing oracle feed mapping for one or more vaults"
            );
            return Err(anyhow!(
                "pyth_feeds must include all {} vaults; missing collateral indexes {:?}",
                EXPECTED_VAULT_COUNT,
                missing
            ));
        }

        let mut seen_collateral_indexes = HashSet::new();
        for feed in &self.pyth_feeds {
            if feed.symbol.trim().is_empty() {
                return Err(anyhow!("pyth feed symbol cannot be empty"));
            }
            if feed.collateral_index > 3 {
                return Err(anyhow!(
                    "pyth feed collateral_index out of range [0,3]: {}",
                    feed.collateral_index
                ));
            }
            if !seen_collateral_indexes.insert(feed.collateral_index) {
                return Err(anyhow!(
                    "duplicate pyth feed collateral_index in config: {}",
                    feed.collateral_index
                ));
            }
            if !(MIN_STALENESS_SECS..=MAX_STALENESS_SECS).contains(&feed.max_age_secs) {
                return Err(anyhow!(
                    "pyth feed max_age_secs must be within {}..={} for {}",
                    MIN_STALENESS_SECS,
                    MAX_STALENESS_SECS,
                    feed.symbol
                ));
            }
        }

        if self.pyth_feeds.len() != EXPECTED_VAULT_COUNT {
            return Err(anyhow!(
                "pyth_feeds must contain exactly {} entries (got {})",
                EXPECTED_VAULT_COUNT,
                self.pyth_feeds.len()
            ));
        }

        let missing_after_validation: Vec<u8> = (0..EXPECTED_VAULT_COUNT as u8)
            .filter(|idx| !seen_collateral_indexes.contains(idx))
            .collect();
        if !missing_after_validation.is_empty() {
            error!(
                expected_vault_count = EXPECTED_VAULT_COUNT,
                missing_vault_indices = ?missing_after_validation,
                "keeper config invalid: unresolved collateral feed gaps"
            );
            return Err(anyhow!(
                "pyth_feeds missing collateral indexes {:?}",
                missing_after_validation
            ));
        }

        if !(MIN_TICK_INTERVAL_SECS..=MAX_TICK_INTERVAL_SECS).contains(&self.tick_interval_secs) {
            return Err(anyhow!(
                "tick_interval_secs must be within {}..={}",
                MIN_TICK_INTERVAL_SECS,
                MAX_TICK_INTERVAL_SECS
            ));
        }
        if !(MIN_STALENESS_SECS..=MAX_STALENESS_SECS).contains(&self.oracle_max_age_secs) {
            return Err(anyhow!(
                "oracle_max_age_secs must be within {}..={}",
                MIN_STALENESS_SECS,
                MAX_STALENESS_SECS
            ));
        }
        if !(MIN_STALENESS_SECS..=MAX_STALENESS_SECS).contains(&self.oracle_publish_max_age_secs) {
            return Err(anyhow!(
                "oracle_publish_max_age_secs must be within {}..={}",
                MIN_STALENESS_SECS,
                MAX_STALENESS_SECS
            ));
        }
        if !(MIN_ORACLE_CONFIDENCE_BPS..=MAX_ORACLE_CONFIDENCE_BPS)
            .contains(&self.oracle_confidence_max_bps)
        {
            return Err(anyhow!(
                "oracle_confidence_max_bps must be within {}..={} (bps)",
                MIN_ORACLE_CONFIDENCE_BPS,
                MAX_ORACLE_CONFIDENCE_BPS
            ));
        }
        if self.min_collateral_ratio_bps == 0 {
            return Err(anyhow!("min_collateral_ratio_bps must be > 0"));
        }
        if !(MIN_EMERGENCY_CR_BPS..=MAX_EMERGENCY_CR_BPS)
            .contains(&self.emergency_collateral_ratio_bps)
        {
            return Err(anyhow!(
                "emergency_collateral_ratio_bps must be within {}..={} (1.0x-2.0x)",
                MIN_EMERGENCY_CR_BPS,
                MAX_EMERGENCY_CR_BPS
            ));
        }
        if self.emergency_collateral_ratio_bps > self.min_collateral_ratio_bps {
            return Err(anyhow!(
                "emergency_collateral_ratio_bps must be <= min_collateral_ratio_bps"
            ));
        }
        if self.emergency_debounce_cycles == 0 {
            return Err(anyhow!("emergency_debounce_cycles must be > 0"));
        }
        if self.max_rebalance_slippage_bps > 10_000 {
            return Err(anyhow!("max_rebalance_slippage_bps must be <= 10000"));
        }
        if self.commit_valid_for_slots == 0 {
            return Err(anyhow!("commit_valid_for_slots must be > 0"));
        }
        if self.commit_valid_for_slots < self.commit_reveal_delay_slots {
            return Err(anyhow!(
                "commit_valid_for_slots must be >= commit_reveal_delay_slots"
            ));
        }
        if self.commit_valid_for_slots > MAX_COMMIT_VALID_FOR_SLOTS {
            return Err(anyhow!(
                "commit_valid_for_slots too large: {} (max {})",
                self.commit_valid_for_slots,
                MAX_COMMIT_VALID_FOR_SLOTS
            ));
        }
        if self.commit_reveal_delay_slots == 0 {
            return Err(anyhow!("commit_reveal_delay_slots must be > 0"));
        }
        if !(MIN_STALENESS_SECS..=MAX_STALENESS_SECS).contains(&self.watchdog_oracle_stale_slots) {
            return Err(anyhow!(
                "watchdog_oracle_stale_slots must be within {}..={}",
                MIN_STALENESS_SECS,
                MAX_STALENESS_SECS
            ));
        }
        if self.watchdog_history_limit == 0 {
            return Err(anyhow!("watchdog_history_limit must be > 0"));
        }
        if self.watchdog_history_limit > MAX_WATCHDOG_HISTORY_LIMIT {
            return Err(anyhow!(
                "watchdog_history_limit too large: {} (max {})",
                self.watchdog_history_limit,
                MAX_WATCHDOG_HISTORY_LIMIT
            ));
        }
        if self.max_consecutive_failed_cycles == 0 {
            return Err(anyhow!("max_consecutive_failed_cycles must be > 0"));
        }
        if self.max_consecutive_failed_cycles > MAX_CONSECUTIVE_FAILED_CYCLES {
            return Err(anyhow!(
                "max_consecutive_failed_cycles too large: {} (max {})",
                self.max_consecutive_failed_cycles,
                MAX_CONSECUTIVE_FAILED_CYCLES
            ));
        }

        if self.key_signatures_max_per_epoch == 0 {
            return Err(anyhow!("key_signatures_max_per_epoch must be > 0"));
        }
        if self.key_signatures_max_per_epoch > MAX_KEY_SIGNATURES_PER_EPOCH {
            return Err(anyhow!(
                "key_signatures_max_per_epoch too large: {} (max {})",
                self.key_signatures_max_per_epoch,
                MAX_KEY_SIGNATURES_PER_EPOCH
            ));
        }

        if self.key_rotation_grace_epochs > MAX_KEY_ROTATION_GRACE_EPOCHS {
            return Err(anyhow!(
                "key_rotation_grace_epochs too large: {} (max {})",
                self.key_rotation_grace_epochs,
                MAX_KEY_ROTATION_GRACE_EPOCHS
            ));
        }

        if !(1..=MAX_KEY_ANOMALY_WINDOW_SECS).contains(&self.key_anomaly_window_secs) {
            return Err(anyhow!(
                "key_anomaly_window_secs must be within 1..={} (got {})",
                MAX_KEY_ANOMALY_WINDOW_SECS,
                self.key_anomaly_window_secs
            ));
        }

        if self.key_anomaly_burst_threshold == 0 {
            return Err(anyhow!("key_anomaly_burst_threshold must be > 0"));
        }
        if self.key_anomaly_burst_threshold > MAX_KEY_ANOMALY_BURST_THRESHOLD {
            return Err(anyhow!(
                "key_anomaly_burst_threshold too large: {} (max {})",
                self.key_anomaly_burst_threshold,
                MAX_KEY_ANOMALY_BURST_THRESHOLD
            ));
        }

        if self.key_rotation_cutover_epoch.is_some() && self.key_rotation_next_pubkeys.len() != 3 {
            return Err(anyhow!(
                "key_rotation_next_pubkeys must contain exactly 3 entries when key_rotation_cutover_epoch is set"
            ));
        }

        if !self.key_rotation_next_pubkeys.is_empty() {
            let unique: HashSet<Pubkey> = self.key_rotation_next_pubkeys.iter().copied().collect();
            if unique.len() != self.key_rotation_next_pubkeys.len() {
                return Err(anyhow!("key_rotation_next_pubkeys contains duplicates"));
            }
        }

        if !self.allowed_upgrade_authority_multisigs.is_empty() {
            let unique: HashSet<Pubkey> = self
                .allowed_upgrade_authority_multisigs
                .iter()
                .copied()
                .collect();
            if unique.len() != self.allowed_upgrade_authority_multisigs.len() {
                return Err(anyhow!(
                    "allowed_upgrade_authority_multisigs contains duplicates"
                ));
            }
        }

        Ok(())
    }

    fn to_file(&self) -> KeeperConfigFile {
        KeeperConfigFile {
            rpc_url: self.rpc_url.clone(),
            secondary_rpc_url: self.secondary_rpc_url.clone(),
            program_id: self.program_id.to_string(),
            keeper_keypairs: self
                .keeper_keypairs
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            pyth_feeds: self
                .pyth_feeds
                .iter()
                .map(|f| PythFeedFile {
                    symbol: f.symbol.clone(),
                    collateral_index: f.collateral_index,
                    price_account: f.price_account.to_string(),
                    max_age_secs: Some(f.max_age_secs),
                })
                .collect(),
            tick_interval_secs: self.tick_interval_secs,
            oracle_max_age_secs: self.oracle_max_age_secs,
            oracle_publish_max_age_secs: Some(self.oracle_publish_max_age_secs),
            oracle_confidence_max_bps: Some(self.oracle_confidence_max_bps),
            min_collateral_ratio_bps: self.min_collateral_ratio_bps,
            emergency_collateral_ratio_bps: Some(self.emergency_collateral_ratio_bps),
            emergency_debounce_cycles: Some(self.emergency_debounce_cycles),
            rebalance_deviation_bps: self.rebalance_deviation_bps,
            max_rebalance_slippage_bps: self.max_rebalance_slippage_bps,
            commit_valid_for_slots: self.commit_valid_for_slots,
            commit_reveal_delay_slots: Some(self.commit_reveal_delay_slots),
            auto_emergency_shutdown: self.auto_emergency_shutdown,
            send_watchdog_alert_tx: self.send_watchdog_alert_tx,
            execute_rebalance_immediately: self.execute_rebalance_immediately,
            optimizer_enabled: Some(self.optimizer_enabled),
            aig_enabled: Some(self.aig_enabled),
            tournament_enabled: Some(self.tournament_enabled),
            aig_interval_secs: Some(self.aig_interval_secs),
            tournament_interval_secs: Some(self.tournament_interval_secs),
            watchdog_supply_spike_bps: self.watchdog_supply_spike_bps,
            watchdog_cr_drop_bps: self.watchdog_cr_drop_bps,
            watchdog_oracle_stale_slots: Some(self.watchdog_oracle_stale_slots),
            watchdog_weight_shift_bps: Some(self.watchdog_weight_shift_bps),
            watchdog_history_limit: Some(self.watchdog_history_limit),
            max_consecutive_failed_cycles: Some(self.max_consecutive_failed_cycles),
            allowed_upgrade_authority_multisigs: Some(
                self.allowed_upgrade_authority_multisigs
                    .iter()
                    .map(|pk| pk.to_string())
                    .collect(),
            ),
            key_signatures_max_per_epoch: Some(self.key_signatures_max_per_epoch),
            key_rotation_cutover_epoch: self.key_rotation_cutover_epoch,
            key_rotation_grace_epochs: Some(self.key_rotation_grace_epochs),
            key_rotation_next_pubkeys: Some(
                self.key_rotation_next_pubkeys
                    .iter()
                    .map(|pk| pk.to_string())
                    .collect(),
            ),
            key_anomaly_window_secs: Some(self.key_anomaly_window_secs),
            key_anomaly_burst_threshold: Some(self.key_anomaly_burst_threshold),
        }
    }
}

fn validate_rpc_allowlist(raw_url: &str) -> Result<()> {
    let host = extract_https_host(raw_url)?;
    if RPC_ALLOWLIST
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
    {
        return Ok(());
    }

    Err(anyhow!(
        "rpc endpoint host is not allowlisted: {} (allowed: {:?})",
        host,
        RPC_ALLOWLIST
    ))
}

fn extract_https_host(raw_url: &str) -> Result<&str> {
    let url = raw_url.trim();
    let without_scheme = url
        .strip_prefix("https://")
        .ok_or_else(|| anyhow!("rpc_url must use https scheme: {url}"))?;

    let host_port = without_scheme.split('/').next().unwrap_or_default();
    if host_port.is_empty() {
        return Err(anyhow!("rpc_url missing host: {url}"));
    }

    let host = host_port.split('@').next_back().unwrap_or_default();
    if host.is_empty() {
        return Err(anyhow!("rpc_url missing host component: {url}"));
    }

    Ok(host.split(':').next().unwrap_or(host))
}

fn parse_pubkey_list(values: Option<&[String]>, field: &str) -> Result<Vec<Pubkey>> {
    let Some(values) = values else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity(values.len());
    for value in values {
        let parsed = Pubkey::from_str(value)
            .with_context(|| format!("invalid pubkey in {}: {}", field, value))?;
        out.push(parsed);
    }

    Ok(out)
}

fn validate_keypair_path_policy(paths: &[PathBuf]) -> Result<()> {
    let mut unique_parent_dirs = HashSet::new();

    for path in paths {
        let normalized = path.to_string_lossy();
        let normalized_lower = normalized.to_ascii_lowercase();

        if FORBIDDEN_KEYPAIR_PREFIXES
            .iter()
            .any(|prefix| normalized_lower.starts_with(prefix))
        {
            return Err(anyhow!(
                "keeper_keypairs cannot use ephemeral directory paths ({})",
                normalized
            ));
        }

        if normalized.contains("..") {
            return Err(anyhow!(
                "keeper_keypairs cannot contain parent traversal segments: {}",
                normalized
            ));
        }

        if let Some(parent) = path.parent() {
            unique_parent_dirs.insert(parent.to_string_lossy().to_string());
        }
    }

    if unique_parent_dirs.len() < 3 {
        return Err(anyhow!(
            "keeper_keypairs must span three distinct parent directories for blast-radius reduction"
        ));
    }

    Ok(())
}

fn verify_config_integrity(config_path: &Path, content: &str) -> Result<()> {
    let maybe_key = env::var(CONFIG_HMAC_ENV_KEY)
        .ok()
        .filter(|v| !v.trim().is_empty());
    if maybe_key.is_none() {
        let allow_unsigned = env::var(CONFIG_HMAC_ALLOW_UNSIGNED_ENV)
            .map(|v| v == "1")
            .unwrap_or(false);

        if allow_unsigned && cfg!(debug_assertions) {
            return Ok(());
        }

        return Err(anyhow!(
            "{} is required (set {}=1 only in debug/test builds for explicit insecure override)",
            CONFIG_HMAC_ENV_KEY,
            CONFIG_HMAC_ALLOW_UNSIGNED_ENV
        ));
    }

    let key = maybe_key.expect("checked is_some");
    let signature_path = PathBuf::from(format!(
        "{}{}",
        config_path.display(),
        CONFIG_SIGNATURE_SUFFIX
    ));
    verify_secure_signature_file(&signature_path)?;

    let signature = fs::read_to_string(&signature_path).with_context(|| {
        format!(
            "failed to read config signature sidecar: {}",
            signature_path.display()
        )
    })?;

    let expected = keyed_hash_hex(key.as_bytes(), content.as_bytes());
    let observed = signature.trim().to_ascii_lowercase();
    if observed != expected {
        return Err(anyhow!(
            "config signature mismatch for {}",
            config_path.display()
        ));
    }

    Ok(())
}

fn verify_secure_signature_file(signature_path: &Path) -> Result<()> {
    let metadata = fs::metadata(signature_path).with_context(|| {
        format!(
            "failed to stat config signature sidecar: {}",
            signature_path.display()
        )
    })?;

    if !metadata.is_file() {
        return Err(anyhow!(
            "config signature sidecar is not a regular file: {}",
            signature_path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(anyhow!(
                "config signature sidecar has insecure permissions {:o}: {}",
                mode,
                signature_path.display()
            ));
        }

        let owner_uid = metadata.uid();
        let effective_uid = unsafe { libc::geteuid() as u32 };
        if owner_uid != effective_uid {
            return Err(anyhow!(
                "config signature sidecar owner mismatch for {} (owner_uid={}, effective_uid={})",
                signature_path.display(),
                owner_uid,
                effective_uid
            ));
        }
    }

    Ok(())
}

fn keyed_hash_hex(key: &[u8], payload: &[u8]) -> String {
    let digest = hashv(&[b"microstable:keeper-config:v1", key, payload, key]).to_bytes();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}
