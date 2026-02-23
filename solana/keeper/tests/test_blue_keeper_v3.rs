#[path = "../src/config.rs"]
mod config;
#[path = "../src/monitor.rs"]
mod monitor;
#[path = "../src/utils.rs"]
mod utils;
#[path = "../src/wire.rs"]
mod wire;

use solana_sdk::pubkey::Pubkey;

fn test_pubkey(tag: u8) -> Pubkey {
    Pubkey::new_from_array([tag; 32])
}

fn base_protocol() -> wire::ProtocolState {
    wire::ProtocolState {
        weights: [25_000, 25_000, 25_000, 25_000],
        fee_rate: 30,
        mint_fee_rate: 30,
        redeem_fee_rate: 30,
        cr_target: 15_000,
        total_supply: 100,
        last_update_slot: 123,
        keeper_set: [test_pubkey(1), test_pubkey(2), test_pubkey(3)],
        emergency_shutdown: false,
        pending_rebalance_commit: [0u8; 32],
        pending_rebalance_slot: 0,
        pending_rebalance_expiry: 0,
        pending_keeper_set: [[0u8; 32]; 3],
        pending_keeper_activation_slot: 0,
        bump: 255,
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
        bump: 7,
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
        bump: 42,
        pyth_price_feed: test_pubkey(40 + index),
    }
}

fn view_from(
    protocol: &wire::ProtocolState,
    circuit: &wire::CircuitBreakerState,
    vaults: &[wire::CollateralVault; 4],
    global_cr_bps: u64,
) -> monitor::MonitorCrossRpcView {
    monitor::MonitorCrossRpcView::from_state(protocol, circuit, vaults, global_cr_bps)
}

#[test]
fn tc_pkv2_001_rejects_vault_collateral_mismatch_even_when_global_cr_equal() {
    let protocol = base_protocol();
    let circuit = base_circuit();

    let primary_vaults = [
        vault(0, 100, 1_000_000),
        vault(1, 100, 1_000_000),
        vault(2, 0, 1_000_000),
        vault(3, 0, 1_000_000),
    ];
    let secondary_vaults = [
        vault(0, 150, 1_000_000),
        vault(1, 50, 1_000_000),
        vault(2, 0, 1_000_000),
        vault(3, 0, 1_000_000),
    ];

    let primary = view_from(&protocol, &circuit, &primary_vaults, 20_000);
    let secondary = view_from(&protocol, &circuit, &secondary_vaults, 20_000);

    let err = monitor::validate_monitor_cross_rpc(&primary, &secondary)
        .expect_err("vault collateral mismatch must fail even when global CR matches");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("vault.total_deposits"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv2_001_rejects_protocol_state_mismatch() {
    let protocol = base_protocol();
    let circuit = base_circuit();
    let vaults = [
        vault(0, 50, 1_000_000),
        vault(1, 50, 1_000_000),
        vault(2, 50, 1_000_000),
        vault(3, 50, 1_000_000),
    ];

    let primary = view_from(&protocol, &circuit, &vaults, 20_000);
    let mut secondary = primary.clone();
    secondary.protocol_total_supply = 103;

    let err = monitor::validate_monitor_cross_rpc(&primary, &secondary)
        .expect_err("protocol total_supply mismatch must fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("protocol.total_supply mismatch"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv2_002_requires_env_and_file_dual_verification_without_embedded_hash() {
    let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let err = utils::resolve_expected_binary_sha256(None, Some(hash), None)
        .expect_err("env-only verification must fail without embedded hash");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("env+file dual verification"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv2_002_rejects_env_file_hash_mismatch() {
    let env_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let file_hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let err = utils::resolve_expected_binary_sha256(None, Some(env_hash), Some(file_hash))
        .expect_err("mismatched env/file hashes must fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("mismatch between env KEEPER_BINARY_SHA256 and hash file"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv2_002_accepts_matching_env_file_hash() {
    let upper = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let lower = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let resolved = utils::resolve_expected_binary_sha256(None, Some(upper), Some(lower))
        .expect("matching env/file hash must pass");

    assert_eq!(resolved, lower);
}

#[test]
fn tc_pkv2_003_allows_only_crates_io_registry_source() {
    let trusted = "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    utils::validate_lockfile_dependency_sources(trusted)
        .expect("crates.io index must remain allowed");
}

#[test]
fn tc_pkv2_003_rejects_non_crates_registry_source() {
    let untrusted = "[[package]]\nname = \"evil\"\nversion = \"0.1.0\"\nsource = \"registry+https://evil.example/index\"\n";
    let err = utils::validate_lockfile_dependency_sources(untrusted)
        .expect_err("alternate registry must be rejected");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("unsupported registry source"),
        "unexpected error: {msg}"
    );
}

#[test]
fn tc_pkv2_003_rejects_git_and_path_sources() {
    let git_source =
        "[[package]]\nname = \"evil\"\nversion = \"0.1.0\"\nsource = \"git+https://evil.example/repo\"\n";
    let path_source =
        "[[package]]\nname = \"evil\"\nversion = \"0.1.0\"\nsource = \"path+file:///tmp/evil\"\n";

    assert!(utils::validate_lockfile_dependency_sources(git_source).is_err());
    assert!(utils::validate_lockfile_dependency_sources(path_source).is_err());
}
