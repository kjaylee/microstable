use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

use crate::{
    errors::MicrostableError,
    instructions::{apply_fee_bps, validate_common_invariants},
    state::{
        BasketConfig, CircuitState, GlobalState, ProtocolMode, BASKET_CONFIG_SEED,
        CIRCUIT_STATE_SEED, GLOBAL_STATE_SEED,
    },
};

#[derive(Accounts)]
pub struct RedeemStable<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [GLOBAL_STATE_SEED], bump = global_state.bump)]
    pub global_state: Account<'info, GlobalState>,
    #[account(seeds = [BASKET_CONFIG_SEED], bump = basket_config.bump)]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(seeds = [CIRCUIT_STATE_SEED], bump = circuit_state.bump)]
    pub circuit_state: Account<'info, CircuitState>,
    #[account(mut)]
    pub user_collateral: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_collateral: Account<'info, TokenAccount>,
    #[account(mut)]
    pub stable_mint: Account<'info, Mint>,
    #[account(mut)]
    pub user_stable: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(
    ctx: Context<RedeemStable>,
    stable_amount: u64,
    effective_cr_bps: u64,
) -> Result<()> {
    let global = &mut ctx.accounts.global_state;

    require!(
        !matches!(global.mode, ProtocolMode::Frozen),
        MicrostableError::RedeemDisabled
    );

    validate_common_invariants(global, &ctx.accounts.basket_config, effective_cr_bps)?;

    require!(stable_amount > 0, MicrostableError::RedeemAmountTooSmall);

    // Queue throttle under CB-2 stress mode.
    if ctx.accounts.circuit_state.is_active(1) {
        let max_redeem = global.total_supply / 10;
        require!(
            stable_amount <= max_redeem.max(1),
            MicrostableError::QueueThrottleExceeded
        );
    }

    let burn_accounts = Burn {
        mint: ctx.accounts.stable_mint.to_account_info(),
        from: ctx.accounts.user_stable.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    token::burn(
        CpiContext::new(ctx.accounts.token_program.to_account_info(), burn_accounts),
        stable_amount,
    )?;

    let collateral_out = apply_fee_bps(stable_amount, global.redeem_fee_bps)?;
    require!(collateral_out > 0, MicrostableError::RedeemAmountTooSmall);

    let signer_seeds: &[&[u8]] = &[GLOBAL_STATE_SEED, &[global.bump]];
    let transfer_accounts = Transfer {
        from: ctx.accounts.vault_collateral.to_account_info(),
        to: ctx.accounts.user_collateral.to_account_info(),
        authority: global.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_accounts,
            &[signer_seeds],
        ),
        collateral_out,
    )?;

    global.total_supply = global
        .total_supply
        .checked_sub(stable_amount)
        .ok_or(MicrostableError::MathOverflow)?;

    validate_common_invariants(global, &ctx.accounts.basket_config, effective_cr_bps)?;

    Ok(())
}
