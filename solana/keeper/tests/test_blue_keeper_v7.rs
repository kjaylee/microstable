#[path = "../src/config.rs"]
mod config;
#[path = "../src/oracle.rs"]
mod oracle;
#[path = "../src/utils.rs"]
mod utils;
#[path = "../src/wire.rs"]
mod wire;

use solana_sdk::pubkey::Pubkey;

fn test_pubkey(tag: u8) -> Pubkey {
    Pubkey::new_from_array([tag; 32])
}

#[test]
fn tc_pkv7_001_default_devnet_secondary_rpc_must_not_be_placeholder() {
    let cfg = config::KeeperConfig::default_devnet();
    let secondary = cfg
        .secondary_rpc_url
        .expect("default config must include secondary RPC");

    assert!(
        !secondary.contains("example.invalid"),
        "secondary RPC must not be placeholder: {secondary}"
    );
    assert!(
        secondary.starts_with("https://"),
        "secondary RPC must be https URL: {secondary}"
    );
}

#[test]
fn tc_pkv7_002_accepts_pyth_account_self_write_authority() {
    let pyth_account = test_pubkey(42);
    let trusted = test_pubkey(7);

    assert!(oracle::is_allowed_pyth_write_authority(
        pyth_account,
        pyth_account,
        trusted
    ));
}

#[test]
fn tc_pkv7_002_rejects_unknown_pyth_write_authority() {
    let pyth_account = test_pubkey(42);
    let trusted = test_pubkey(7);
    let unknown = test_pubkey(99);

    assert!(!oracle::is_allowed_pyth_write_authority(
        unknown,
        pyth_account,
        trusted
    ));
}
