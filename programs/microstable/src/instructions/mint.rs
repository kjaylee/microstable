use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};

use crate::{
    errors::MicrostableError,
    instructions::{apply_fee_bps, validate_common_invariants},
    state::{
        BasketConfig, CircuitState, GlobalState, ProtocolMode, BASKET_CONFIG_SEED,
        CIRCUIT_STATE_SEED, GLOBAL_STATE_SEED,
    },
};

#[derive(Accounts)]
pub struct MintStable<'info> {
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
    ctx: Context<MintStable>,
    collateral_amount: u64,
    effective_cr_bps: u64,
) -> Result<()> {
    let global = &mut ctx.accounts.global_state;

    require!(
        !matches!(global.mode, ProtocolMode::Frozen | ProtocolMode::RedeemOnly),
        MicrostableError::MintDisabled
    );

    // CB-2/3 style throttle check.
    require!(
        !ctx.accounts.circuit_state.is_active(1),
        MicrostableError::MintDisabled
    );

    validate_common_invariants(global, &ctx.accounts.basket_config, effective_cr_bps)?;

    require!(collateral_amount > 0, MicrostableError::MintAmountTooSmall);
    let mint_amount = apply_fee_bps(collateral_amount, global.mint_fee_bps)?;
    require!(mint_amount > 0, MicrostableError::MintAmountTooSmall);

    let transfer_accounts = Transfer {
        from: ctx.accounts.user_collateral.to_account_info(),
        to: ctx.accounts.vault_collateral.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_accounts,
        ),
        collateral_amount,
    )?;

    let signer_seeds: &[&[u8]] = &[GLOBAL_STATE_SEED, &[global.bump]];
    let mint_accounts = MintTo {
        mint: ctx.accounts.stable_mint.to_account_info(),
        to: ctx.accounts.user_stable.to_account_info(),
        authority: global.to_account_info(),
    };
    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            mint_accounts,
            &[signer_seeds],
        ),
        mint_amount,
    )?;

    global.total_supply = global
        .total_supply
        .checked_add(mint_amount)
        .ok_or(MicrostableError::MathOverflow)?;

    validate_common_invariants(global, &ctx.accounts.basket_config, effective_cr_bps)?;

    Ok(())
}
