#[path = "../src/config.rs"]
mod config;
#[path = "../src/utils.rs"]
mod utils;
#[path = "../src/wire.rs"]
mod wire;

use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_temp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    p.push(format!("microstable_blue_keeper_v2_{name}_{nanos}.json"));
    p
}

fn base_config_json() -> Value {
    json!({
        "rpc_url": "https://api.devnet.solana.com",
        "secondary_rpc_url": "https://rpc.ankr.com/solana_devnet",
        "program_id": "BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3",
        "keeper_keypairs": [
            "~/.config/solana/devnet-keypair.json",
            "~/.config/solana/devnet-deploy.json"
        ],
        "pyth_feeds": [
            {
                "symbol": "USDC/USD",
                "collateral_index": 0,
                "price_account": "Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX",
                "max_age_secs": 120
            }
        ],
        "tick_interval_secs": 30,
        "oracle_max_age_secs": 120,
        "oracle_publish_max_age_secs": 60,
        "oracle_confidence_max_bps": 500,
        "min_collateral_ratio_bps": 15000,
        "emergency_collateral_ratio_bps": 10000,
        "emergency_debounce_cycles": 3,
        "rebalance_deviation_bps": 300,
        "max_rebalance_slippage_bps": 200,
        "commit_valid_for_slots": 200,
        "commit_reveal_delay_slots": 5,
        "auto_emergency_shutdown": false,
        "send_watchdog_alert_tx": false,
        "execute_rebalance_immediately": false,
        "watchdog_supply_spike_bps": 2500,
        "watchdog_cr_drop_bps": 1500,
        "watchdog_oracle_stale_slots": 120,
        "watchdog_weight_shift_bps": 600,
        "watchdog_history_limit": 64,
        "max_consecutive_failed_cycles": 5
    })
}

fn write_and_load(value: Value) -> anyhow::Result<config::KeeperConfig> {
    let path = unique_temp_path("config");
    fs::write(
        &path,
        serde_json::to_string_pretty(&value).expect("serialize config"),
    )?;
    let out = config::KeeperConfig::load(Some(path.as_path()));
    let _ = fs::remove_file(path);
    out
}

#[test]
fn tc_rk001_rejects_null_secondary_rpc() {
    let mut cfg = base_config_json();
    cfg["secondary_rpc_url"] = Value::Null;

    let err = write_and_load(cfg).expect_err("secondary_rpc_url: null must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("secondary_rpc_url") && (msg.contains("required") || msg.contains("empty")),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_rk005_tick_interval_bounds() {
    let mut too_low = base_config_json();
    too_low["tick_interval_secs"] = json!(4);
    assert!(write_and_load(too_low).is_err());

    let mut too_high = base_config_json();
    too_high["tick_interval_secs"] = json!(301);
    assert!(write_and_load(too_high).is_err());

    let mut ok = base_config_json();
    ok["tick_interval_secs"] = json!(5);
    assert!(write_and_load(ok).is_ok());
}

#[test]
fn tc_rk005_emergency_cr_bounds() {
    let mut too_low = base_config_json();
    too_low["emergency_collateral_ratio_bps"] = json!(9_999u64);
    assert!(write_and_load(too_low).is_err());

    let mut too_high = base_config_json();
    too_high["emergency_collateral_ratio_bps"] = json!(20_001u64);
    too_high["min_collateral_ratio_bps"] = json!(30_000u64);
    assert!(write_and_load(too_high).is_err());

    let mut ok = base_config_json();
    ok["emergency_collateral_ratio_bps"] = json!(15_000u64);
    ok["min_collateral_ratio_bps"] = json!(16_000u64);
    assert!(write_and_load(ok).is_ok());
}

#[test]
fn tc_rk005_staleness_and_confidence_bounds() {
    let mut stale_low = base_config_json();
    stale_low["oracle_publish_max_age_secs"] = json!(9u64);
    assert!(write_and_load(stale_low).is_err());

    let mut stale_high = base_config_json();
    stale_high["oracle_publish_max_age_secs"] = json!(301u64);
    assert!(write_and_load(stale_high).is_err());

    let mut feed_stale_high = base_config_json();
    feed_stale_high["pyth_feeds"][0]["max_age_secs"] = json!(301u64);
    assert!(write_and_load(feed_stale_high).is_err());

    let mut conf_low = base_config_json();
    conf_low["oracle_confidence_max_bps"] = json!(0u64);
    assert!(write_and_load(conf_low).is_err());

    let mut conf_high = base_config_json();
    conf_high["oracle_confidence_max_bps"] = json!(1_001u64);
    assert!(write_and_load(conf_high).is_err());
}

#[test]
fn tc_rk005_commit_and_failure_bounds() {
    let mut commit_too_high = base_config_json();
    commit_too_high["commit_valid_for_slots"] = json!(1_001u64);
    assert!(write_and_load(commit_too_high).is_err());

    let mut commit_too_low = base_config_json();
    commit_too_low["commit_reveal_delay_slots"] = json!(10u64);
    commit_too_low["commit_valid_for_slots"] = json!(5u64);
    assert!(write_and_load(commit_too_low).is_err());

    let mut fail_cycles_too_high = base_config_json();
    fail_cycles_too_high["max_consecutive_failed_cycles"] = json!(101u64);
    assert!(write_and_load(fail_cycles_too_high).is_err());
}

#[test]
fn tc_rk006_rejects_unvetted_lockfile_sources() {
    let trusted = "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    assert!(utils::validate_lockfile_dependency_sources(trusted).is_ok());

    let git_source = "[[package]]\nname = \"evil\"\nversion = \"0.1.0\"\nsource = \"git+https://evil.example/repo\"\n";
    assert!(utils::validate_lockfile_dependency_sources(git_source).is_err());

    let path_source =
        "[[package]]\nname = \"evil\"\nversion = \"0.1.0\"\nsource = \"path+file:///tmp/evil\"\n";
    assert!(utils::validate_lockfile_dependency_sources(path_source).is_err());
}

#[test]
fn tc_rk006_binary_attestation_detects_mismatch() {
    let expected = utils::sha256_hex(b"keeper-v2");
    assert!(utils::verify_binary_attestation_for_bytes(b"keeper-v2", &expected).is_ok());
    assert!(utils::verify_binary_attestation_for_bytes(b"tampered", &expected).is_err());
}
