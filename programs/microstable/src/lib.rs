use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
use state::AssetConfigInput;

declare_id!("C4eAvHBdnjub8A2eCsbbVsFk2rmsVzJdeqHPAoDfazyK");

#[program]
pub mod microstable {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        target_cr: u64,
        mint_fee_bps: u16,
        redeem_fee_bps: u16,
        assets: Vec<AssetConfigInput>,
    ) -> Result<()> {
        initialize::handler(ctx, target_cr, mint_fee_bps, redeem_fee_bps, assets)
    }

    pub fn mint(
        ctx: Context<MintStable>,
        collateral_amount: u64,
        effective_cr_bps: u64,
    ) -> Result<()> {
        mint::handler(ctx, collateral_amount, effective_cr_bps)
    }

    pub fn redeem(
        ctx: Context<RedeemStable>,
        stable_amount: u64,
        effective_cr_bps: u64,
    ) -> Result<()> {
        redeem::handler(ctx, stable_amount, effective_cr_bps)
    }

    pub fn submit_update(
        ctx: Context<SubmitUpdate>,
        new_weights_bps: Vec<u16>,
        new_target_cr: u64,
        new_mint_fee_bps: u16,
        new_redeem_fee_bps: u16,
        effective_cr_bps: u64,
    ) -> Result<()> {
        submit_update::handler(
            ctx,
            new_weights_bps,
            new_target_cr,
            new_mint_fee_bps,
            new_redeem_fee_bps,
            effective_cr_bps,
        )
    }

    pub fn apply_update(ctx: Context<ApplyUpdate>, effective_cr_bps: u64) -> Result<()> {
        apply_update::handler(ctx, effective_cr_bps)
    }

    pub fn trigger_circuit_breaker(
        ctx: Context<TriggerCircuitBreaker>,
        cb_index: u8,
        effective_cr_bps: u64,
    ) -> Result<()> {
        trigger_circuit_breaker::handler(ctx, cb_index, effective_cr_bps)
    }

    pub fn recover_circuit_breaker(
        ctx: Context<RecoverCircuitBreaker>,
        cb_index: u8,
        effective_cr_bps: u64,
    ) -> Result<()> {
        recover_circuit_breaker::handler(ctx, cb_index, effective_cr_bps)
    }

    pub fn distribute_fees(
        ctx: Context<DistributeFees>,
        total_fee_amount: u64,
        effective_cr_bps: u64,
    ) -> Result<()> {
        distribute_fees::handler(ctx, total_fee_amount, effective_cr_bps)
    }
}
