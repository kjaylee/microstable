use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

pub const DEFAULT_CONFIG_PATH: &str = "keeper/config.devnet.json";
const DEFAULT_PROGRAM_ID: &str = "BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3";

#[derive(Debug, Clone)]
pub struct KeeperConfig {
    pub rpc_url: String,
    pub program_id: Pubkey,
    pub keeper_keypairs: Vec<PathBuf>,
    pub pyth_feeds: Vec<PythFeedConfig>,
    pub tick_interval_secs: u64,
    pub oracle_max_age_secs: u64,
    pub min_collateral_ratio_bps: u64,
    pub rebalance_deviation_bps: u64,
    pub max_rebalance_slippage_bps: u64,
    pub commit_valid_for_slots: u64,
    pub auto_emergency_shutdown: bool,
    pub send_watchdog_alert_tx: bool,
    pub execute_rebalance_immediately: bool,
    pub watchdog_supply_spike_bps: u64,
    pub watchdog_cr_drop_bps: u64,
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
    program_id: String,
    keeper_keypairs: Vec<String>,
    pyth_feeds: Vec<PythFeedFile>,
    tick_interval_secs: u64,
    oracle_max_age_secs: u64,
    min_collateral_ratio_bps: u64,
    rebalance_deviation_bps: u64,
    max_rebalance_slippage_bps: u64,
    commit_valid_for_slots: u64,
    auto_emergency_shutdown: bool,
    send_watchdog_alert_tx: bool,
    execute_rebalance_immediately: bool,
    watchdog_supply_spike_bps: u64,
    watchdog_cr_drop_bps: u64,
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
        if config_path.exists() {
            let content = fs::read_to_string(config_path)
                .with_context(|| format!("failed to read config: {}", config_path.display()))?;
            let file: KeeperConfigFile = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
            return Self::from_file(file);
        }

        let default = Self::default_devnet();
        let serialized = serde_json::to_string_pretty(&default.to_file())?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(config_path, serialized).with_context(|| {
            format!("failed to write default config: {}", config_path.display())
        })?;
        Ok(default)
    }

    pub fn default_devnet() -> Self {
        Self {
            rpc_url: "https://api.devnet.solana.com".to_string(),
            program_id: Pubkey::from_str(DEFAULT_PROGRAM_ID).expect("valid program id"),
            keeper_keypairs: vec![
                PathBuf::from("~/.config/solana/id.json"),
                PathBuf::from("~/.config/solana/keeper2.json"),
            ],
            pyth_feeds: vec![
                PythFeedConfig {
                    symbol: "USDC/USD".to_string(),
                    collateral_index: 0,
                    price_account: Pubkey::from_str("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX")
                        .expect("valid pubkey"),
                    max_age_secs: 120,
                },
                PythFeedConfig {
                    symbol: "USDT/USD".to_string(),
                    collateral_index: 1,
                    price_account: Pubkey::from_str("HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM")
                        .expect("valid pubkey"),
                    max_age_secs: 120,
                },
                PythFeedConfig {
                    symbol: "DAI/USD".to_string(),
                    collateral_index: 2,
                    price_account: Pubkey::from_str("FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN")
                        .expect("valid pubkey"),
                    max_age_secs: 120,
                },
            ],
            tick_interval_secs: 30,
            oracle_max_age_secs: 120,
            min_collateral_ratio_bps: 10_500,
            rebalance_deviation_bps: 300,
            max_rebalance_slippage_bps: 200,
            commit_valid_for_slots: 200,
            auto_emergency_shutdown: false,
            send_watchdog_alert_tx: false,
            execute_rebalance_immediately: false,
            watchdog_supply_spike_bps: 2_500,
            watchdog_cr_drop_bps: 1_500,
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

        Ok(Self {
            rpc_url: file.rpc_url,
            program_id,
            keeper_keypairs: file
                .keeper_keypairs
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            pyth_feeds,
            tick_interval_secs: file.tick_interval_secs,
            oracle_max_age_secs: file.oracle_max_age_secs,
            min_collateral_ratio_bps: file.min_collateral_ratio_bps,
            rebalance_deviation_bps: file.rebalance_deviation_bps,
            max_rebalance_slippage_bps: file.max_rebalance_slippage_bps,
            commit_valid_for_slots: file.commit_valid_for_slots,
            auto_emergency_shutdown: file.auto_emergency_shutdown,
            send_watchdog_alert_tx: file.send_watchdog_alert_tx,
            execute_rebalance_immediately: file.execute_rebalance_immediately,
            watchdog_supply_spike_bps: file.watchdog_supply_spike_bps,
            watchdog_cr_drop_bps: file.watchdog_cr_drop_bps,
        })
    }

    fn to_file(&self) -> KeeperConfigFile {
        KeeperConfigFile {
            rpc_url: self.rpc_url.clone(),
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
            min_collateral_ratio_bps: self.min_collateral_ratio_bps,
            rebalance_deviation_bps: self.rebalance_deviation_bps,
            max_rebalance_slippage_bps: self.max_rebalance_slippage_bps,
            commit_valid_for_slots: self.commit_valid_for_slots,
            auto_emergency_shutdown: self.auto_emergency_shutdown,
            send_watchdog_alert_tx: self.send_watchdog_alert_tx,
            execute_rebalance_immediately: self.execute_rebalance_immediately,
            watchdog_supply_spike_bps: self.watchdog_supply_spike_bps,
            watchdog_cr_drop_bps: self.watchdog_cr_drop_bps,
        }
    }
}
