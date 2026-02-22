#[path = "../src/config.rs"]
mod config;
#[path = "../src/monitor.rs"]
mod monitor;
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

fn reset_secondary_state() {
    utils::reset_secondary_rpc_health_for_tests();
}

#[test]
fn tc_pkv5_001_read_failures_enter_degraded_mode_at_threshold() {
    reset_secondary_state();

    for i in 0..utils::SECONDARY_RPC_DEGRADE_THRESHOLD {
        let entered = utils::register_secondary_rpc_failure();
        if i + 1 < utils::SECONDARY_RPC_DEGRADE_THRESHOLD {
            assert!(!entered, "degraded mode entered before threshold");
        } else {
            assert!(entered, "degraded mode must activate at threshold");
        }
    }

    let snapshot = utils::secondary_rpc_health_snapshot();
    assert!(
        snapshot.degraded,
        "secondary must be degraded after threshold"
    );
    assert_eq!(
        utils::secondary_rpc_mode(true),
        utils::SecondaryRpcMode::Degraded
    );
}

#[test]
fn tc_pkv5_001_degraded_mode_skips_secondary_reads() {
    reset_secondary_state();

    assert!(
        utils::secondary_rpc_mode(true).uses_secondary_reads(),
        "normal mode should use secondary reads"
    );

    for _ in 0..utils::SECONDARY_RPC_DEGRADE_THRESHOLD {
        let _ = utils::register_secondary_rpc_failure();
    }

    assert!(
        !utils::secondary_rpc_mode(true).uses_secondary_reads(),
        "degraded mode must disable secondary reads"
    );
}

#[test]
fn tc_pkv5_001_recovery_restores_normal_dual_rpc_mode() {
    reset_secondary_state();

    for _ in 0..utils::SECONDARY_RPC_DEGRADE_THRESHOLD {
        let _ = utils::register_secondary_rpc_failure();
    }
    assert_eq!(
        utils::secondary_rpc_mode(true),
        utils::SecondaryRpcMode::Degraded
    );

    let recovered = utils::register_secondary_rpc_success();
    assert!(recovered, "secondary success should clear degraded mode");

    let snapshot = utils::secondary_rpc_health_snapshot();
    assert!(!snapshot.degraded);
    assert_eq!(snapshot.consecutive_failures, 0);
    assert_eq!(
        utils::secondary_rpc_mode(true),
        utils::SecondaryRpcMode::Normal
    );
}

#[test]
fn tc_pkv5_002_normal_mode_primary_only_is_soft_fail_retry_once() {
    reset_secondary_state();

    let disposition =
        utils::assess_tx_confirmation_outcome(true, false, utils::SecondaryRpcMode::Normal, false)
            .expect("normal mode primary-only should produce soft-fail retry signal");

    assert_eq!(
        disposition,
        utils::TxConfirmationDisposition::RetrySecondaryOnce
    );
}

#[test]
fn tc_pkv5_002_normal_mode_primary_only_after_retry_is_rejected() {
    reset_secondary_state();

    let err =
        utils::assess_tx_confirmation_outcome(true, false, utils::SecondaryRpcMode::Normal, true)
            .expect_err("normal mode must reject primary-only confirmation after retry");

    assert!(
        format!("{err:#}").contains("dual-RPC confirmation"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn tc_pkv5_002_degraded_mode_allows_primary_only_confirmation() {
    reset_secondary_state();

    let disposition =
        utils::assess_tx_confirmation_outcome(true, false, utils::SecondaryRpcMode::Degraded, true)
            .expect("degraded mode should allow primary-only confirmation");

    assert_eq!(disposition, utils::TxConfirmationDisposition::Confirmed);
}

#[test]
fn tc_pkv5_002_normal_mode_requires_both_rpc_even_if_secondary_only() {
    reset_secondary_state();

    let err =
        utils::assess_tx_confirmation_outcome(false, true, utils::SecondaryRpcMode::Normal, true)
            .expect_err("normal mode must not trust secondary-only confirmation");

    assert!(
        format!("{err:#}").contains("dual-RPC confirmation"),
        "unexpected error: {err:#}"
    );
}
