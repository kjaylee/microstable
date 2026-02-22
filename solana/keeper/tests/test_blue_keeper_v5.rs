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

#[test]
fn tc_pkv4_001_cargo_lock_attestation_match_passes() {
    let lock_bytes = b"[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    let expected = utils::sha256_hex(lock_bytes);

    utils::verify_cargo_lock_attestation_for_bytes(lock_bytes, &expected)
        .expect("matching Cargo.lock hash must pass");
}

#[test]
fn tc_pkv4_001_cargo_lock_attestation_mismatch_fails() {
    let lock_bytes = b"[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    let expected = utils::sha256_hex(b"tampered-lock");

    let err = utils::verify_cargo_lock_attestation_for_bytes(lock_bytes, &expected)
        .expect_err("mismatched Cargo.lock hash must fail");
    assert!(
        format!("{err:#}").contains("Cargo.lock sha256 mismatch"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn tc_pkv4_001_invalid_hash_format_is_rejected() {
    let lock_bytes = b"dummy-lock";
    let err = utils::verify_cargo_lock_attestation_for_bytes(lock_bytes, "not-a-valid-hash")
        .expect_err("invalid hash format must fail");

    assert!(
        format!("{err:#}").contains("invalid sha256 hex"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn tc_pkv4_001_supply_chain_control_executes_without_self_hash() {
    utils::enforce_supply_chain_controls()
        .expect("supply-chain controls should pass with Cargo.lock attestation only");
}

#[test]
fn tc_pkv4_002_adaptive_confirm_window_extends_to_sixty_seconds() {
    let base = utils::adaptive_secondary_confirm_window_secs(true, true);
    let extended = utils::adaptive_secondary_confirm_window_secs(true, false);

    assert_eq!(base, utils::TX_CONFIRM_WINDOW_BASE_SECS);
    assert_eq!(extended, utils::TX_CONFIRM_WINDOW_MAX_SECS);
}

#[test]
fn tc_pkv4_002_primary_only_confirmation_is_accepted_in_degraded_mode() {
    let decision =
        utils::assess_tx_confirmation_outcome(true, false, utils::SecondaryRpcMode::Degraded, true)
            .expect("primary confirmation must be accepted in degraded mode");

    assert_eq!(decision, utils::TxConfirmationDisposition::Confirmed);
}

#[test]
fn tc_pkv4_002_reject_when_both_confirmations_missing() {
    let err =
        utils::assess_tx_confirmation_outcome(false, false, utils::SecondaryRpcMode::Normal, true)
            .expect_err("both-side unconfirmed transaction must be rejected");
    assert!(
        format!("{err:#}").contains("dual-RPC confirmation"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn tc_pkv4_002_enter_and_recover_degraded_mode() {
    utils::reset_secondary_rpc_health_for_tests();

    for i in 0..utils::SECONDARY_RPC_DEGRADE_THRESHOLD {
        let entered = utils::register_secondary_rpc_failure();
        if i + 1 < utils::SECONDARY_RPC_DEGRADE_THRESHOLD {
            assert!(!entered, "degraded mode entered too early");
        } else {
            assert!(entered, "degraded mode must activate at threshold");
        }
    }

    let snapshot = utils::secondary_rpc_health_snapshot();
    assert!(snapshot.degraded, "secondary must be degraded at threshold");

    let recovered = utils::register_secondary_rpc_success();
    assert!(
        recovered,
        "successful secondary event should clear degraded mode"
    );

    let snapshot_after = utils::secondary_rpc_health_snapshot();
    assert!(
        !snapshot_after.degraded,
        "secondary should recover from degraded mode"
    );
    assert_eq!(snapshot_after.consecutive_failures, 0);
}
