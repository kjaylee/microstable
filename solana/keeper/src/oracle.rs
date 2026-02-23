use crate::{
    config::{KeeperConfig, PythFeedConfig},
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::{anyhow, Context, Result};
use borsh::BorshDeserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use std::{collections::HashSet, time::{SystemTime, UNIX_EPOCH}};
use tracing::{info, warn};

const PYTH_RECEIVER_PROGRAM: &str = "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ";
const PYTH_TRUSTED_WRITE_AUTHORITY: &str = "3fimeXDHiEK9oeJX6XM1rXNoavTCWhzbxNXVmwFzh6Kk";
const PYTH_FEED_ID_USDC: [u8; 32] = [
    0xea, 0xa0, 0x20, 0xc6, 0x1c, 0xc4, 0x79, 0x71, 0x28, 0x13, 0x46, 0x1c, 0xe1, 0x53, 0x89, 0x4a,
    0x96, 0xa6, 0xc0, 0x0b, 0x21, 0xed, 0x0c, 0xfc, 0x27, 0x98, 0xd1, 0xf9, 0xa9, 0xe9, 0xc9, 0x4a,
];
const PYTH_FEED_ID_USDT: [u8; 32] = [
    0x2b, 0x89, 0xb9, 0xdc, 0x8f, 0xdf, 0x9f, 0x34, 0x70, 0x9a, 0x5b, 0x10, 0x6b, 0x47, 0x2f, 0x0f,
    0x39, 0xbb, 0x6c, 0xa9, 0xce, 0x04, 0xb0, 0xfd, 0x7f, 0x2e, 0x97, 0x16, 0x88, 0xe2, 0xe5, 0x3b,
];
const PYTH_FEED_ID_DAI: [u8; 32] = [
    0xb0, 0x94, 0x8a, 0x5e, 0x53, 0x13, 0x20, 0x0c, 0x63, 0x2b, 0x51, 0xbb, 0x5c, 0xa3, 0x2f, 0x6d,
    0xe0, 0xd3, 0x6e, 0x99, 0x50, 0xa9, 0x42, 0xd1, 0x97, 0x51, 0xe8, 0x33, 0xf7, 0x0d, 0xab, 0xfd,
];
const PYTH_FEED_ID_USDS: [u8; 32] = [
    0xc2, 0xf5, 0xc9, 0xb4, 0xd9, 0xe7, 0xa1, 0xfc, 0xb5, 0xa8, 0x0c, 0x7a, 0x2c, 0x3e, 0xc0, 0xf8,
    0x4a, 0xb1, 0xde, 0x9f, 0x77, 0x8c, 0x0d, 0xf1, 0xb6, 0xe9, 0xc7, 0xab, 0x4f, 0x1e, 0x0d, 0x9a,
];

#[derive(Debug, Clone)]
#[allow(dead_code)]
// On-chain layout — all fields must be present for correct deserialization
pub struct OracleUpdateResult {
    pub symbol: String,
    pub collateral_index: u8,
    pub price: u64,
    pub confidence: u64,
    pub confidence_bps: u64,
    pub publish_time: i64,
    pub observed_slot: u64,
    pub signature: Signature,
}

#[derive(Debug, Clone)]
pub struct OracleObservation {
    pub price: u64,
    pub confidence: u64,
    pub publish_time: i64,
    pub observed_slot: u64,
}

#[derive(Debug, Clone)]
struct PreparedOracleUpdate {
    symbol: String,
    collateral_index: u8,
    pyth_account: Pubkey,
    observation: OracleObservation,
    confidence_bps: u64,
}

#[derive(Debug, Clone, BorshDeserialize)]
#[allow(dead_code)]
// On-chain layout — all fields must be present for correct deserialization
enum RawPythVerificationLevel {
    Partial { num_signatures: u8 },
    Full,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct RawPythPriceFeedMessage {
    feed_id: [u8; 32],
    price: i64,
    conf: u64,
    exponent: i32,
    publish_time: i64,
    prev_publish_time: i64,
    ema_price: i64,
    ema_conf: u64,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct RawPythPriceUpdateV2 {
    write_authority: Pubkey,
    verification_level: RawPythVerificationLevel,
    price_message: RawPythPriceFeedMessage,
    posted_slot: u64,
}

pub fn run_oracle_cycle(
    rpc: &RpcClient,
    secondary_rpc: Option<&RpcClient>,
    secondary_mode: utils::SecondaryRpcMode,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
) -> Result<Vec<OracleUpdateResult>> {
    let secondary_for_reads = if secondary_mode.uses_secondary_reads() {
        secondary_rpc
    } else {
        None
    };

    let (protocol, vaults) = if let Some(secondary) = secondary_for_reads {
        match utils::retry_with_backoff(
            utils::CROSS_RPC_MAX_ATTEMPTS,
            utils::CROSS_RPC_BACKOFF_BASE_MS,
            |attempt| {
                let primary_snapshot = fetch_oracle_snapshot(rpc, derived)?;
                let secondary_snapshot = fetch_oracle_snapshot(secondary, derived).map_err(|err| {
                    let entered_degraded = utils::register_secondary_rpc_failure();
                    anyhow!(
                        "secondary oracle snapshot read failed (attempt {attempt}/{}): {err}; entered_degraded={entered_degraded}",
                        utils::CROSS_RPC_MAX_ATTEMPTS
                    )
                })?;

                validate_oracle_cross_rpc(
                    &primary_snapshot.0,
                    &secondary_snapshot.0,
                    &primary_snapshot.1,
                    &secondary_snapshot.1,
                )
                .map_err(|err| {
                    anyhow!(
                        "oracle cross-RPC mismatch (attempt {attempt}/{}): {err}",
                        utils::CROSS_RPC_MAX_ATTEMPTS
                    )
                })?;

                Ok(primary_snapshot)
            },
        ) {
            Ok(snapshot) => {
                let _ = utils::register_secondary_rpc_success();
                snapshot
            }
            Err(err) => {
                if utils::secondary_rpc_is_degraded() {
                    warn!(
                        error = %err,
                        "secondary RPC degraded during oracle read-path checks; falling back to primary-only mode"
                    );
                    fetch_oracle_snapshot(rpc, derived)?
                } else {
                    return Err(anyhow!(
                        "oracle cycle failed after cross-RPC retries: {err}"
                    ));
                }
            }
        }
    } else {
        fetch_oracle_snapshot(rpc, derived)?
    };

    if protocol.emergency_shutdown {
        warn!("oracle cycle skipped: protocol is in emergency shutdown");
        return Ok(Vec::new());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before unix epoch")?
        .as_secs() as i64;

    let mut prepared_updates = Vec::with_capacity(cfg.pyth_feeds.len());

    let configured_indices: HashSet<u8> = cfg
        .pyth_feeds
        .iter()
        .map(|feed| feed.collateral_index)
        .collect();
    for (vault_index, vault) in vaults.iter().enumerate() {
        let collateral_index = vault_index as u8;
        if !configured_indices.contains(&collateral_index) {
            warn!(
                collateral_index,
                vault = %vault.vault,
                status = "unconfigured",
                "oracle update skipped: vault has no configured feed"
            );
        }
    }

    for feed in &cfg.pyth_feeds {
        let Some(vault) = vaults.get(feed.collateral_index as usize) else {
            warn!(
                symbol = %feed.symbol,
                collateral_index = feed.collateral_index,
                "oracle update skipped: invalid collateral index"
            );
            continue;
        };

        let pyth_account = if vault.pyth_price_feed == Pubkey::default() {
            feed.price_account
        } else {
            if vault.pyth_price_feed != feed.price_account {
                warn!(
                    symbol = %feed.symbol,
                    configured = %feed.price_account,
                    vault_feed = %vault.pyth_price_feed,
                    "vault feed differs from config; using vault feed"
                );
            }
            vault.pyth_price_feed
        };

        let initial_observation = match fetch_pyth_observation(rpc, feed, pyth_account) {
            Ok(observation) => observation,
            Err(err) => {
                warn!(
                    symbol = %feed.symbol,
                    collateral_index = feed.collateral_index,
                    error = %err,
                    "oracle update skipped: failed to fetch/decode pyth account"
                );
                continue;
            }
        };

        let observation = if let Some(secondary) = secondary_for_reads {
            if utils::secondary_rpc_is_degraded() {
                initial_observation
            } else {
                match utils::retry_with_backoff(
                    utils::CROSS_RPC_MAX_ATTEMPTS,
                    utils::CROSS_RPC_BACKOFF_BASE_MS,
                    |attempt| {
                        let primary_observation = if attempt == 1 {
                            initial_observation.clone()
                        } else {
                            fetch_pyth_observation(rpc, feed, pyth_account)?
                        };

                        let secondary_observation =
                            fetch_pyth_observation(secondary, feed, pyth_account).map_err(|err| {
                                let entered_degraded = utils::register_secondary_rpc_failure();
                                anyhow!(
                                    "secondary observation fetch failed (attempt {attempt}/{}): {err}; entered_degraded={entered_degraded}",
                                    utils::CROSS_RPC_MAX_ATTEMPTS
                                )
                            })?;

                        validate_oracle_observation_consistency(
                            &primary_observation,
                            &secondary_observation,
                        )
                        .map_err(|err| {
                            anyhow!(
                                "oracle observation mismatch for {} (attempt {attempt}/{}): {err}",
                                feed.symbol,
                                utils::CROSS_RPC_MAX_ATTEMPTS
                            )
                        })?;

                        Ok(primary_observation)
                    },
                ) {
                    Ok(observation) => {
                        let _ = utils::register_secondary_rpc_success();
                        observation
                    }
                    Err(err) => {
                        if utils::secondary_rpc_is_degraded() {
                            warn!(
                                symbol = %feed.symbol,
                                collateral_index = feed.collateral_index,
                                error = %err,
                                "secondary RPC degraded during oracle observation checks; using primary-only observation"
                            );
                            initial_observation
                        } else {
                            return Err(anyhow!(
                                "oracle cycle failed after observation cross-RPC retries for {}: {err}",
                                feed.symbol
                            ));
                        }
                    }
                }
            }
        } else {
            initial_observation
        };

        let max_publish_age_secs = feed.max_age_secs.min(cfg.oracle_publish_max_age_secs);
        if is_stale(now, observation.publish_time, max_publish_age_secs) {
            warn!(
                symbol = %feed.symbol,
                collateral_index = feed.collateral_index,
                publish_time = observation.publish_time,
                max_age_secs = max_publish_age_secs,
                configured_feed_max_age_secs = feed.max_age_secs,
                "oracle update skipped: stale publish time"
            );
            continue;
        }

        let confidence_bps = confidence_bps(observation.price, observation.confidence);
        if confidence_bps > cfg.oracle_confidence_max_bps {
            warn!(
                symbol = %feed.symbol,
                collateral_index = feed.collateral_index,
                confidence = observation.confidence,
                price = observation.price,
                confidence_bps,
                max_confidence_bps = cfg.oracle_confidence_max_bps,
                "oracle update skipped: confidence interval too wide"
            );
            continue;
        }

        prepared_updates.push(PreparedOracleUpdate {
            symbol: feed.symbol.clone(),
            collateral_index: feed.collateral_index,
            pyth_account,
            observation,
            confidence_bps,
        });
    }

    let (k1, k2) = utils::keeper_quorum_for_protocol(keepers, &protocol.keeper_set)?;
    let mut successful_updates = Vec::with_capacity(prepared_updates.len());

    for prepared in prepared_updates {
        let ix = wire::ix_update_oracle_pyth(
            cfg.program_id,
            derived.protocol_state,
            derived.circuit_breaker,
            derived.vaults,
            k1.pubkey(),
            k2.pubkey(),
            prepared.pyth_account,
            prepared.collateral_index,
        )?;

        match utils::send_instructions(rpc, secondary_rpc, secondary_mode, k1, &[k1, k2], vec![ix])
        {
            Ok(sig) => {
                info!(
                    symbol = %prepared.symbol,
                    collateral_index = prepared.collateral_index,
                    signature = %sig,
                    price = prepared.observation.price,
                    confidence = prepared.observation.confidence,
                    confidence_bps = prepared.confidence_bps,
                    publish_time = prepared.observation.publish_time,
                    observed_slot = prepared.observation.observed_slot,
                    "oracle update sent"
                );

                successful_updates.push(OracleUpdateResult {
                    symbol: prepared.symbol,
                    collateral_index: prepared.collateral_index,
                    price: prepared.observation.price,
                    confidence: prepared.observation.confidence,
                    confidence_bps: prepared.confidence_bps,
                    publish_time: prepared.observation.publish_time,
                    observed_slot: prepared.observation.observed_slot,
                    signature: sig,
                });
            }
            Err(err) => {
                warn!(
                    symbol = %prepared.symbol,
                    collateral_index = prepared.collateral_index,
                    error = %err,
                    "oracle update tx failed"
                );
            }
        }
    }

    Ok(successful_updates)
}

pub fn validate_oracle_cross_rpc(
    primary_protocol: &wire::ProtocolState,
    secondary_protocol: &wire::ProtocolState,
    primary_vaults: &[wire::CollateralVault; 4],
    secondary_vaults: &[wire::CollateralVault; 4],
) -> Result<()> {
    utils::validate_protocol_state_with_tolerance(primary_protocol, secondary_protocol)?;
    utils::validate_vaults_with_tolerance(primary_vaults, secondary_vaults)?;
    Ok(())
}

pub fn validate_oracle_observation_consistency(
    primary: &OracleObservation,
    secondary: &OracleObservation,
) -> Result<()> {
    if !utils::within_u64_tolerance(
        primary.price,
        secondary.price,
        utils::CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "price mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.price,
            secondary.price,
            utils::CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !utils::within_u64_tolerance(
        primary.confidence,
        secondary.confidence,
        utils::CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "confidence mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.confidence,
            secondary.confidence,
            utils::CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    if !utils::within_i64_tolerance(
        primary.publish_time,
        secondary.publish_time,
        utils::CROSS_RPC_TIME_TOLERANCE_SECS,
    ) {
        return Err(anyhow!(
            "publish_time mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.publish_time,
            secondary.publish_time,
            utils::CROSS_RPC_TIME_TOLERANCE_SECS
        ));
    }

    if !utils::within_u64_tolerance(
        primary.observed_slot,
        secondary.observed_slot,
        utils::CROSS_RPC_NUMERIC_TOLERANCE,
    ) {
        return Err(anyhow!(
            "observed_slot mismatch beyond tolerance (primary={}, secondary={}, tolerance={})",
            primary.observed_slot,
            secondary.observed_slot,
            utils::CROSS_RPC_NUMERIC_TOLERANCE
        ));
    }

    Ok(())
}

fn fetch_oracle_snapshot(
    rpc: &RpcClient,
    derived: &DerivedAccounts,
) -> Result<(wire::ProtocolState, [wire::CollateralVault; 4])> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;
    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];
    Ok((protocol, vaults))
}

fn fetch_pyth_observation(
    rpc: &RpcClient,
    feed: &PythFeedConfig,
    account: Pubkey,
) -> Result<OracleObservation> {
    let account_data = rpc
        .get_account(&account)
        .with_context(|| format!("failed to fetch Pyth account {account}"))?;

    let receiver_program = utils::parse_pubkey(PYTH_RECEIVER_PROGRAM)?;
    if account_data.owner != receiver_program {
        return Err(anyhow!(
            "invalid Pyth owner for {account}: expected {receiver_program}, got {}",
            account_data.owner
        ));
    }

    if account_data.data.len() < 8 {
        return Err(anyhow!("Pyth account {account} too short"));
    }

    let mut payload = &account_data.data[8..];
    let update = RawPythPriceUpdateV2::deserialize(&mut payload)
        .with_context(|| format!("failed to decode Pyth payload for {account}"))?;

    if !matches!(update.verification_level, RawPythVerificationLevel::Full) {
        return Err(anyhow!("Pyth verification level too low for {account}"));
    }

    let trusted_write_authority = utils::parse_pubkey(PYTH_TRUSTED_WRITE_AUTHORITY)?;
    if !is_allowed_pyth_write_authority(
        update.write_authority,
        account,
        trusted_write_authority,
    ) {
        return Err(anyhow!(
            "unexpected Pyth write_authority for {account}: expected either {trusted_write_authority} or account self, got {}",
            update.write_authority
        ));
    }

    let expected_feed_id = expected_feed_id(feed.collateral_index)?;
    if update.price_message.feed_id != expected_feed_id {
        return Err(anyhow!(
            "unexpected Pyth feed_id for {account}: collateral_index={} does not match expected feed",
            feed.collateral_index
        ));
    }

    if update.price_message.price <= 0 {
        return Err(anyhow!("Pyth price not positive for {account}"));
    }

    let price = scale_signed_to_six_decimals(
        i128::from(update.price_message.price),
        update.price_message.exponent,
    )?;
    let confidence = scale_unsigned_to_six_decimals(
        u128::from(update.price_message.conf),
        update.price_message.exponent,
    )?;

    let _ = update.price_message.prev_publish_time;
    let _ = update.price_message.ema_price;
    let _ = update.price_message.ema_conf;

    Ok(OracleObservation {
        price,
        confidence,
        publish_time: update.price_message.publish_time,
        observed_slot: update.posted_slot,
    })
}

pub fn is_allowed_pyth_write_authority(
    write_authority: Pubkey,
    pyth_account: Pubkey,
    trusted_write_authority: Pubkey,
) -> bool {
    write_authority == pyth_account || write_authority == trusted_write_authority
}

fn expected_feed_id(collateral_index: u8) -> Result<[u8; 32]> {
    match collateral_index {
        0 => Ok(PYTH_FEED_ID_USDC),
        1 => Ok(PYTH_FEED_ID_USDT),
        2 => Ok(PYTH_FEED_ID_DAI),
        3 => Ok(PYTH_FEED_ID_USDS),
        _ => Err(anyhow!(
            "invalid collateral index for feed-id check: {}",
            collateral_index
        )),
    }
}

fn is_stale(now_unix_ts: i64, publish_time: i64, max_age_secs: u64) -> bool {
    if publish_time > now_unix_ts {
        return true;
    }
    now_unix_ts.saturating_sub(publish_time) > max_age_secs as i64
}

fn confidence_bps(price: u64, confidence: u64) -> u64 {
    if price == 0 {
        return u64::MAX;
    }

    ((confidence as u128).saturating_mul(10_000) / price as u128) as u64
}

fn scale_signed_to_six_decimals(value: i128, exponent: i32) -> Result<u64> {
    let unsigned = u128::try_from(value).map_err(|_| anyhow!("pyth price cannot be negative"))?;
    scale_unsigned_to_six_decimals(unsigned, exponent)
}

fn scale_unsigned_to_six_decimals(value: u128, exponent: i32) -> Result<u64> {
    let shift = exponent
        .checked_add(6)
        .ok_or_else(|| anyhow!("pyth exponent overflow"))?;

    let scaled = if shift >= 0 {
        let factor = pow10_u128(shift as u32)?;
        value
            .checked_mul(factor)
            .ok_or_else(|| anyhow!("pyth scale overflow"))?
    } else {
        let factor = pow10_u128((-shift) as u32)?;
        value
            .checked_add(factor.saturating_sub(1))
            .ok_or_else(|| anyhow!("pyth scale overflow"))?
            .checked_div(factor)
            .ok_or_else(|| anyhow!("pyth scale overflow"))?
    };

    u64::try_from(scaled).map_err(|_| anyhow!("pyth scaled value does not fit u64"))
}

fn pow10_u128(exp: u32) -> Result<u128> {
    let mut acc = 1u128;
    for _ in 0..exp {
        acc = acc
            .checked_mul(10)
            .ok_or_else(|| anyhow!("pow10 overflow"))?;
    }
    Ok(acc)
}
