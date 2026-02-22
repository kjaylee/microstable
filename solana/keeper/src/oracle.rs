use crate::{
    config::{KeeperConfig, PythFeedConfig},
    utils::{self, DerivedAccounts},
    wire,
};
use anyhow::{Context, Result};
use pyth_sdk_solana::state::SolanaPriceAccount;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct OracleUpdateResult {
    pub symbol: String,
    pub collateral_index: u8,
    pub price: i64,
    pub confidence: u64,
    pub publish_time: i64,
    pub skipped_reason: Option<String>,
    pub signature: Option<Signature>,
}

pub fn run_oracle_cycle(
    rpc: &RpcClient,
    cfg: &KeeperConfig,
    keepers: &[Keypair],
    derived: &DerivedAccounts,
) -> Result<Vec<OracleUpdateResult>> {
    let mut updates = Vec::with_capacity(cfg.pyth_feeds.len());

    for feed in &cfg.pyth_feeds {
        let fetched = fetch_price(rpc, feed, cfg.oracle_max_age_secs)?;
        if let Some(reason) = &fetched.skipped_reason {
            warn!(
                symbol = %fetched.symbol,
                collateral_index = fetched.collateral_index,
                reason = %reason,
                "oracle update skipped"
            );
            updates.push(fetched);
            continue;
        }

        let ix = build_update_oracle_instruction(cfg, derived, keepers, feed);
        let (k1, k2) = utils::keeper_quorum(keepers)?;
        let sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![ix])
            .with_context(|| format!("failed to send update_oracle_pyth for {}", feed.symbol))?;

        info!(
            symbol = %feed.symbol,
            collateral_index = feed.collateral_index,
            signature = %sig,
            "oracle update sent"
        );

        updates.push(OracleUpdateResult {
            signature: Some(sig),
            ..fetched
        });
    }

    Ok(updates)
}

fn fetch_price(
    rpc: &RpcClient,
    feed: &PythFeedConfig,
    default_age_secs: u64,
) -> Result<OracleUpdateResult> {
    let mut account = rpc
        .get_account(&feed.price_account)
        .with_context(|| format!("failed to fetch Pyth account {}", feed.price_account))?;

    let price_feed = SolanaPriceAccount::account_to_feed(&feed.price_account, &mut account)
        .with_context(|| format!("failed to decode Pyth account {}", feed.price_account))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before unix epoch")?
        .as_secs() as i64;

    let max_age = if feed.max_age_secs == 0 {
        default_age_secs
    } else {
        feed.max_age_secs
    };

    let Some(price) = price_feed.get_price_no_older_than(now, max_age) else {
        return Ok(OracleUpdateResult {
            symbol: feed.symbol.clone(),
            collateral_index: feed.collateral_index,
            price: 0,
            confidence: 0,
            publish_time: 0,
            skipped_reason: Some(format!("stale price: age > {}s", max_age)),
            signature: None,
        });
    };

    Ok(OracleUpdateResult {
        symbol: feed.symbol.clone(),
        collateral_index: feed.collateral_index,
        price: price.price,
        confidence: price.conf,
        publish_time: price.publish_time,
        skipped_reason: None,
        signature: None,
    })
}

fn build_update_oracle_instruction(
    cfg: &KeeperConfig,
    derived: &DerivedAccounts,
    keepers: &[Keypair],
    feed: &PythFeedConfig,
) -> Instruction {
    wire::ix_update_oracle_pyth(
        cfg.program_id,
        derived.protocol_state,
        derived.circuit_breaker,
        derived.vaults,
        keepers[0].pubkey(),
        keepers[1].pubkey(),
        feed.price_account,
        feed.collateral_index,
    )
}
