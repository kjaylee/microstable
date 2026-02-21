use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::{
    errors::MicrostableError,
    instructions::validate_common_invariants,
    state::{BasketConfig, GlobalState, BASKET_CONFIG_SEED, GLOBAL_STATE_SEED},
};

#[derive(Accounts)]
pub struct DistributeFees<'info> {
    pub authority: Signer<'info>,
    #[account(seeds = [GLOBAL_STATE_SEED], bump = global_state.bump)]
    pub global_state: Account<'info, GlobalState>,
    #[account(seeds = [BASKET_CONFIG_SEED], bump = basket_config.bump)]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(mut)]
    pub fee_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub keeper_fee_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub watchdog_fee_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub auditor_fee_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub treasury_fee_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(
    ctx: Context<DistributeFees>,
    total_fee_amount: u64,
    effective_cr_bps: u64,
) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.authority.key(),
        ctx.accounts.global_state.authority,
        MicrostableError::Unauthorized
    );

    validate_common_invariants(
        &ctx.accounts.global_state,
        &ctx.accounts.basket_config,
        effective_cr_bps,
    )?;

    let keeper = total_fee_amount.saturating_mul(30) / 100;
    let watchdog = total_fee_amount.saturating_mul(10) / 100;
    let auditor = total_fee_amount.saturating_mul(5) / 100;
    let treasury = total_fee_amount
        .checked_sub(keeper)
        .and_then(|v| v.checked_sub(watchdog))
        .and_then(|v| v.checked_sub(auditor))
        .ok_or(MicrostableError::MathOverflow)?;

    let signer_seeds: &[&[u8]] = &[GLOBAL_STATE_SEED, &[ctx.accounts.global_state.bump]];

    let transfers = [
        (keeper, &ctx.accounts.keeper_fee_account),
        (watchdog, &ctx.accounts.watchdog_fee_account),
        (auditor, &ctx.accounts.auditor_fee_account),
        (treasury, &ctx.accounts.treasury_fee_account),
    ];

    for (amount, to) in transfers {
        if amount == 0 {
            continue;
        }

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.fee_vault.to_account_info(),
                    to: to.to_account_info(),
                    authority: ctx.accounts.global_state.to_account_info(),
                },
                &[signer_seeds],
            ),
            amount,
        )?;
    }

    Ok(())
}
