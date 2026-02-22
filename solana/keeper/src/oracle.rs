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
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

const PYTH_RECEIVER_PROGRAM: &str = "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ";

#[derive(Debug, Clone)]
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
struct OracleObservation {
    price: u64,
    confidence: u64,
    publish_time: i64,
    observed_slot: u64,
}

#[derive(Debug, Clone, BorshDeserialize)]
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
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
) -> Result<Vec<OracleUpdateResult>> {
    let protocol: wire::ProtocolState =
        utils::fetch_account(rpc, &derived.protocol_state, "ProtocolState")?;

    if protocol.emergency_shutdown {
        warn!("oracle cycle skipped: protocol is in emergency shutdown");
        return Ok(Vec::new());
    }

    let vaults = [
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[0], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[1], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[2], "CollateralVault")?,
        utils::fetch_account::<wire::CollateralVault>(rpc, &derived.vaults[3], "CollateralVault")?,
    ];

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before unix epoch")?
        .as_secs() as i64;

    let mut successful_updates = Vec::with_capacity(cfg.pyth_feeds.len());
    let (k1, k2) = utils::keeper_quorum(keepers)?;

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

        let observation = match fetch_pyth_observation(rpc, feed, pyth_account) {
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

        if is_stale(
            now,
            observation.publish_time,
            cfg.oracle_publish_max_age_secs,
        ) {
            warn!(
                symbol = %feed.symbol,
                collateral_index = feed.collateral_index,
                publish_time = observation.publish_time,
                max_age_secs = cfg.oracle_publish_max_age_secs,
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

        let ix = wire::ix_update_oracle_pyth(
            cfg.program_id,
            derived.protocol_state,
            derived.circuit_breaker,
            derived.vaults,
            k1.pubkey(),
            k2.pubkey(),
            pyth_account,
            feed.collateral_index,
        );

        match utils::send_instructions(rpc, k1, &[k1, k2], vec![ix]) {
            Ok(sig) => {
                info!(
                    symbol = %feed.symbol,
                    collateral_index = feed.collateral_index,
                    signature = %sig,
                    price = observation.price,
                    confidence = observation.confidence,
                    confidence_bps,
                    publish_time = observation.publish_time,
                    observed_slot = observation.observed_slot,
                    "oracle update sent"
                );

                successful_updates.push(OracleUpdateResult {
                    symbol: feed.symbol.clone(),
                    collateral_index: feed.collateral_index,
                    price: observation.price,
                    confidence: observation.confidence,
                    confidence_bps,
                    publish_time: observation.publish_time,
                    observed_slot: observation.observed_slot,
                    signature: sig,
                });
            }
            Err(err) => {
                warn!(
                    symbol = %feed.symbol,
                    collateral_index = feed.collateral_index,
                    error = %err,
                    "oracle update tx failed"
                );
            }
        }
    }

    Ok(successful_updates)
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

    let _ = feed;
    let _ = update.write_authority;
    let _ = update.price_message.feed_id;
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
