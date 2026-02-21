use anchor_lang::prelude::*;

use crate::{
    errors::MicrostableError,
    instructions::validate_common_invariants,
    state::{
        BasketConfig, GlobalState, UpdateProposal, BASKET_CONFIG_SEED, BPS_DENOMINATOR,
        GLOBAL_STATE_SEED, MAX_ASSETS, UPDATE_PROPOSAL_SEED,
    },
};

#[derive(Accounts)]
pub struct SubmitUpdate<'info> {
    #[account(mut)]
    pub proposer: Signer<'info>,
    #[account(seeds = [GLOBAL_STATE_SEED], bump = global_state.bump)]
    pub global_state: Account<'info, GlobalState>,
    #[account(seeds = [BASKET_CONFIG_SEED], bump = basket_config.bump)]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(
        init_if_needed,
        payer = proposer,
        space = 8 + UpdateProposal::LEN,
        seeds = [UPDATE_PROPOSAL_SEED, proposer.key().as_ref()],
        bump
    )]
    pub update_proposal: Account<'info, UpdateProposal>,
    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<SubmitUpdate>,
    new_weights_bps: Vec<u16>,
    new_target_cr: u64,
    new_mint_fee_bps: u16,
    new_redeem_fee_bps: u16,
    effective_cr_bps: u64,
) -> Result<()> {
    require!(
        new_weights_bps.len() == ctx.accounts.basket_config.assets.len(),
        MicrostableError::ProposalLengthMismatch
    );
    require!(
        new_weights_bps.len() <= MAX_ASSETS,
        MicrostableError::InvalidAssetList
    );
    require!(
        new_weights_bps.iter().map(|v| *v as u64).sum::<u64>() == BPS_DENOMINATOR,
        MicrostableError::InvalidWeightSum
    );

    validate_common_invariants(
        &ctx.accounts.global_state,
        &ctx.accounts.basket_config,
        effective_cr_bps,
    )?;

    let proposal = &mut ctx.accounts.update_proposal;
    proposal.proposer = ctx.accounts.proposer.key();
    proposal.new_weights_bps = new_weights_bps;
    proposal.new_target_cr = new_target_cr;
    proposal.new_mint_fee_bps = new_mint_fee_bps;
    proposal.new_redeem_fee_bps = new_redeem_fee_bps;
    proposal.proposed_at = Clock::get()?.unix_timestamp;
    proposal.bump = ctx.bumps.update_proposal;

    Ok(())
}
