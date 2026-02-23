#[path = "../src/config.rs"]
mod config;
#[path = "../src/monitor.rs"]
mod monitor;
#[path = "../src/optimizer.rs"]
mod optimizer;
#[path = "../src/oracle.rs"]
mod oracle;
#[path = "../src/rebalance.rs"]
mod rebalance;
#[path = "../src/utils.rs"]
mod utils;
#[path = "../src/watchdog.rs"]
mod watchdog;
#[path = "../src/wire.rs"]
mod wire;

use anyhow::anyhow;
use solana_sdk::pubkey::Pubkey;

fn test_pubkey(tag: u8) -> Pubkey {
    Pubkey::new_from_array([tag; 32])
}

fn base_protocol() -> wire::ProtocolState {
    wire::ProtocolState {
        weights: [250_000, 250_000, 250_000, 250_000],
        fee_rate: 30,
        mint_fee_rate: 30,
        redeem_fee_rate: 30,
        cr_target: 15_000,
        total_supply: 1_000_000,
        last_update_slot: 123,
        keeper_set: [test_pubkey(1), test_pubkey(2), test_pubkey(3)],
        emergency_shutdown: false,
        pending_rebalance_commit: [0u8; 32],
        pending_rebalance_slot: 0,
        pending_rebalance_expiry: 0,
        bump: 1,
    }
}

fn base_circuit() -> wire::CircuitBreakerState {
    wire::CircuitBreakerState {
        status: [0, 0, 0, 0],
        activation_tick: [0; 4],
        trigger_count: [0; 4],
        cooldown_until: [0; 4],
        last_trigger_tick: [0; 4],
        recent_trigger_count: [0; 4],
        recovery_tick: [0; 4],
        cb1_collateral_index: 0,
        mint_rate_limit: 0,
        optimizer_enabled: true,
        learning_rate_scale: 100,
        max_activation_duration: 100,
        bump: 1,
    }
}

fn vault(index: u8, total_deposits: u64, price: u64) -> wire::CollateralVault {
    wire::CollateralVault {
        index,
        mint: test_pubkey(10 + index),
        vault: test_pubkey(20 + index),
        oracle: test_pubkey(30 + index),
        risk_score: 100,
        weight_cap: 1_000_000,
        base_weight_cap: 1_000_000,
        price,
        confidence: 100,
        last_oracle_slot: 120,
        total_deposits,
        bump: 1,
        pyth_price_feed: test_pubkey(40 + index),
    }
}

fn monitor_view(
    protocol: &wire::ProtocolState,
    circuit: &wire::CircuitBreakerState,
    vaults: &[wire::CollateralVault; 4],
    global_cr_bps: u64,
) -> monitor::MonitorCrossRpcView {
    monitor::MonitorCrossRpcView::from_state(protocol, circuit, vaults, global_cr_bps)
}

fn observation(price: u64, confidence: u64, publish_time: i64, observed_slot: u64) -> oracle::OracleObservation {
    oracle::OracleObservation {
        price,
        confidence,
        publish_time,
        observed_slot,
    }
}

#[test]
fn tc_pkv3_001_monitor_tolerance_allows_small_drift() {
    let protocol = base_protocol();
    let circuit = base_circuit();
    let primary_vaults = [
        vault(0, 100, 1_000_000),
        vault(1, 100, 1_000_000),
        vault(2, 100, 1_000_000),
        vault(3, 100, 1_000_000),
    ];

    let mut secondary_protocol = protocol.clone();
    secondary_protocol.total_supply = secondary_protocol.total_supply.saturating_add(1);
    let mut secondary_vaults = primary_vaults.clone();
    secondary_vaults[0].total_deposits = secondary_vaults[0].total_deposits.saturating_add(1);
    secondary_vaults[1].price = secondary_vaults[1].price.saturating_add(1);

    let primary = monitor_view(&protocol, &circuit, &primary_vaults, 12_000);
    let secondary = monitor_view(&secondary_protocol, &circuit, &secondary_vaults, 12_001);

    monitor::validate_monitor_cross_rpc(&primary, &secondary)
        .expect("small cross-RPC drift should pass with tolerance");
}

#[test]
fn tc_pkv3_001_monitor_tolerance_rejects_large_drift() {
    let protocol = base_protocol();
    let circuit = base_circuit();
    let primary_vaults = [
        vault(0, 100, 1_000_000),
        vault(1, 100, 1_000_000),
        vault(2, 100, 1_000_000),
        vault(3, 100, 1_000_000),
    ];
    let mut secondary_vaults = primary_vaults.clone();
    secondary_vaults[0].total_deposits = secondary_vaults[0].total_deposits.saturating_add(5);

    let primary = monitor_view(&protocol, &circuit, &primary_vaults, 12_000);
    let secondary = monitor_view(&protocol, &circuit, &secondary_vaults, 12_000);

    let err = monitor::validate_monitor_cross_rpc(&primary, &secondary)
        .expect_err("large cross-RPC drift must fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("mismatch beyond tolerance"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv3_001_retry_with_backoff_succeeds_on_third_attempt() {
    let mut attempts = 0usize;
    let result = utils::retry_with_backoff(3, 0, |_| {
        attempts = attempts.saturating_add(1);
        if attempts < 3 {
            Err(anyhow!("transient mismatch"))
        } else {
            Ok("ok")
        }
    })
    .expect("third attempt should recover");

    assert_eq!(result, "ok");
    assert_eq!(attempts, 3);
}

#[test]
fn tc_pkv3_001_retry_with_backoff_fails_after_max_attempts() {
    let mut attempts = 0usize;
    let err = utils::retry_with_backoff::<(), _>(3, 0, |_| {
        attempts = attempts.saturating_add(1);
        Err(anyhow!("persistent mismatch"))
    })
    .expect_err("persistent mismatch must fail after retries");

    assert_eq!(attempts, 3);
    assert!(format!("{err:#}").contains("persistent mismatch"));
}

#[test]
fn tc_pkv3_002_embedded_hash_rejects_runtime_override() {
    let embedded = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let runtime_env = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let err = utils::resolve_expected_binary_sha256(Some(embedded), Some(runtime_env), None)
        .expect_err("runtime env hash must not override embedded hash");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("embedded trusted hash"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv3_002_embedded_hash_is_primary_trust_anchor() {
    let embedded = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let resolved = utils::resolve_expected_binary_sha256(Some(embedded), None, None)
        .expect("embedded hash should be accepted as trust anchor");

    assert_eq!(resolved, embedded);
}

#[test]
fn tc_pkv3_003_accepts_secondary_fallback_when_primary_fails() {
    utils::assess_dual_rpc_confirmation(false, true, true)
        .expect("secondary confirmation should be accepted as fallback");
}

#[test]
fn tc_pkv3_003_rejects_primary_only_confirmation_when_dual_required() {
    let err = utils::assess_dual_rpc_confirmation(true, false, true)
        .expect_err("primary-only confirmation should fail under dual-RPC policy");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("dual-RPC confirmation"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv3_004_oracle_observation_consistency_accepts_small_drift() {
    let primary = observation(1_000_000, 500, 1_700_000_000, 123_456);
    let secondary = observation(1_000_001, 501, 1_700_000_001, 123_457);

    oracle::validate_oracle_observation_consistency(&primary, &secondary)
        .expect("observation drift within tolerance should pass");
}

#[test]
fn tc_pkv3_004_oracle_observation_consistency_rejects_large_price_gap() {
    let primary = observation(1_000_000, 500, 1_700_000_000, 123_456);
    let secondary = observation(1_000_010, 500, 1_700_000_000, 123_456);

    let err = oracle::validate_oracle_observation_consistency(&primary, &secondary)
        .expect_err("large price gap must fail consistency check");

    assert!(
        format!("{err:#}").contains("price mismatch beyond tolerance"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn tc_pkv3_005_watchdog_cross_rpc_rejects_cr_spike_mismatch() {
    let protocol = base_protocol();
    let primary_vaults = [
        vault(0, 100, 1_000_000),
        vault(1, 100, 1_000_000),
        vault(2, 100, 1_000_000),
        vault(3, 100, 1_000_000),
    ];

    let err = watchdog::validate_watchdog_cross_rpc(
        &protocol,
        &protocol,
        &primary_vaults,
        &primary_vaults,
        12_000,
        12_100,
    )
    .expect_err("large global CR mismatch must fail watchdog cross-RPC check");

    assert!(
        format!("{err:#}").contains("global collateral ratio mismatch"),
        "unexpected error: {err:#}"
    );
}
