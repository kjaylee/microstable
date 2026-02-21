use anchor_lang::prelude::*;

use crate::{
    errors::MicrostableError,
    instructions::validate_common_invariants,
    state::{
        BasketConfig, GlobalState, UpdateProposal, BASKET_CONFIG_SEED, BPS_DENOMINATOR,
        DELTA_MAX_BPS, FEE_DELTA_MAX_BPS, GLOBAL_STATE_SEED, TARGET_CR_DELTA_MAX_BPS,
        UPDATE_PROPOSAL_SEED,
    },
};

#[derive(Accounts)]
pub struct ApplyUpdate<'info> {
    #[account(mut)]
    pub applier: Signer<'info>,
    #[account(mut, seeds = [GLOBAL_STATE_SEED], bump = global_state.bump)]
    pub global_state: Account<'info, GlobalState>,
    #[account(mut, seeds = [BASKET_CONFIG_SEED], bump = basket_config.bump)]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(
        mut,
        seeds = [UPDATE_PROPOSAL_SEED, update_proposal.proposer.as_ref()],
        bump = update_proposal.bump,
        close = applier
    )]
    pub update_proposal: Account<'info, UpdateProposal>,
}

pub fn handler(ctx: Context<ApplyUpdate>, effective_cr_bps: u64) -> Result<()> {
    let global = &mut ctx.accounts.global_state;
    require_keys_eq!(
        ctx.accounts.applier.key(),
        global.authority,
        MicrostableError::Unauthorized
    );

    let proposal = &ctx.accounts.update_proposal;
    let basket = &mut ctx.accounts.basket_config;

    require!(
        proposal.new_weights_bps.len() == basket.assets.len(),
        MicrostableError::ProposalLengthMismatch
    );
    require!(
        proposal
            .new_weights_bps
            .iter()
            .map(|w| *w as u64)
            .sum::<u64>()
            == BPS_DENOMINATOR,
        MicrostableError::InvalidWeightSum
    );

    for (asset, proposed_weight) in basket
        .assets
        .iter_mut()
        .zip(proposal.new_weights_bps.iter())
    {
        let delta = asset.weight_bps.abs_diff(*proposed_weight);
        require!(delta <= DELTA_MAX_BPS, MicrostableError::DeltaCapExceeded);
        asset.weight_bps = *proposed_weight;
    }

    let target_delta = global.target_cr.abs_diff(proposal.new_target_cr);
    require!(
        target_delta <= TARGET_CR_DELTA_MAX_BPS,
        MicrostableError::TargetCrDeltaExceeded
    );

    let mint_fee_delta = global.mint_fee_bps.abs_diff(proposal.new_mint_fee_bps);
    let redeem_fee_delta = global.redeem_fee_bps.abs_diff(proposal.new_redeem_fee_bps);
    require!(
        mint_fee_delta <= FEE_DELTA_MAX_BPS,
        MicrostableError::FeeDeltaExceeded
    );
    require!(
        redeem_fee_delta <= FEE_DELTA_MAX_BPS,
        MicrostableError::FeeDeltaExceeded
    );

    global.target_cr = proposal.new_target_cr;
    global.mint_fee_bps = proposal.new_mint_fee_bps;
    global.redeem_fee_bps = proposal.new_redeem_fee_bps;
    global.last_update_epoch = Clock::get()?.unix_timestamp;

    validate_common_invariants(global, basket, effective_cr_bps)?;

    Ok(())
}
