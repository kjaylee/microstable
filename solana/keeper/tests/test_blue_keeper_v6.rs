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

use solana_sdk::{pubkey::Pubkey, signature::{Keypair, Signer}};

fn reset_secondary_state() {
    utils::reset_secondary_rpc_health_for_tests();
}

fn keeper_pubkeys(k1: &Keypair, k2: &Keypair, k3: &Keypair) -> [Pubkey; 3] {
    [k1.pubkey(), k2.pubkey(), k3.pubkey()]
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

#[test]
fn tc_pkv5_003_keeper_quorum_selects_two_members_from_protocol_set() {
    let k1 = Keypair::new();
    let k2 = Keypair::new();
    let k3 = Keypair::new();
    let outsider = Keypair::new();

    let keepers = vec![outsider, k1, k2, k3];
    let protocol_keeper_set = keeper_pubkeys(&keepers[1], &keepers[2], &keepers[3]);

    let (q1, q2) = utils::keeper_quorum_for_protocol(&keepers, &protocol_keeper_set)
        .expect("should find 2-of-3 keeper quorum");

    assert!(
        protocol_keeper_set.contains(&q1.pubkey()) && protocol_keeper_set.contains(&q2.pubkey()),
        "selected quorum signers must belong to protocol keeper set"
    );
    assert_ne!(q1.pubkey(), q2.pubkey(), "quorum signers must be distinct");
}

#[test]
fn tc_pkv5_004_keeper_quorum_rejects_1_of_3_configuration() {
    let k1 = Keypair::new();
    let k2 = Keypair::new();
    let k3 = Keypair::new();

    let keepers = vec![k1];
    let protocol_keeper_set = [keepers[0].pubkey(), k2.pubkey(), k3.pubkey()];

    let err = utils::keeper_quorum_for_protocol(&keepers, &protocol_keeper_set)
        .expect_err("1-of-3 keeper material must be rejected");

    assert!(
        format!("{err:#}").contains("do not satisfy protocol keeper quorum"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn tc_pkv5_005_keeper_rotation_retargets_quorum_selection() {
    let old1 = Keypair::new();
    let old2 = Keypair::new();
    let old3 = Keypair::new();
    let new1 = Keypair::new();
    let new2 = Keypair::new();
    let new3 = Keypair::new();

    let keepers = vec![old1, old2, old3, new1, new2, new3];

    let old_set = keeper_pubkeys(&keepers[0], &keepers[1], &keepers[2]);
    let new_set = keeper_pubkeys(&keepers[3], &keepers[4], &keepers[5]);

    let (old_q1, old_q2) =
        utils::keeper_quorum_for_protocol(&keepers, &old_set).expect("old quorum should resolve");
    assert!(old_set.contains(&old_q1.pubkey()) && old_set.contains(&old_q2.pubkey()));

    let (new_q1, new_q2) =
        utils::keeper_quorum_for_protocol(&keepers, &new_set).expect("new quorum should resolve");
    assert!(new_set.contains(&new_q1.pubkey()) && new_set.contains(&new_q2.pubkey()));

    assert!(
        !new_set.contains(&old_q1.pubkey()) || !new_set.contains(&old_q2.pubkey()),
        "post-rotation quorum selection should not stay pinned to old keeper set"
    );
}
