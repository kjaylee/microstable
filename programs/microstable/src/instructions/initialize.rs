use anchor_lang::prelude::*;

use crate::{
    errors::MicrostableError,
    instructions::validate_common_invariants,
    state::{
        AssetConfigInput, BasketConfig, CircuitState, GlobalState, ProtocolMode,
        BASKET_CONFIG_SEED, BPS_DENOMINATOR, CIRCUIT_STATE_SEED, GLOBAL_STATE_SEED, MAX_ASSETS,
    },
};

#[derive(Accounts)]
#[instruction(_assets: Vec<AssetConfigInput>)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + GlobalState::LEN,
        seeds = [GLOBAL_STATE_SEED],
        bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        init,
        payer = payer,
        space = 8 + BasketConfig::LEN,
        seeds = [BASKET_CONFIG_SEED],
        bump
    )]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(
        init,
        payer = payer,
        space = 8 + CircuitState::LEN,
        seeds = [CIRCUIT_STATE_SEED],
        bump
    )]
    pub circuit_state: Account<'info, CircuitState>,
    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<Initialize>,
    target_cr: u64,
    mint_fee_bps: u16,
    redeem_fee_bps: u16,
    assets: Vec<AssetConfigInput>,
) -> Result<()> {
    require!(!assets.is_empty(), MicrostableError::InvalidAssetList);
    require!(
        assets.len() <= MAX_ASSETS,
        MicrostableError::InvalidAssetList
    );
    require!(
        mint_fee_bps <= BPS_DENOMINATOR as u16,
        MicrostableError::InvalidFeeBps
    );
    require!(
        redeem_fee_bps <= BPS_DENOMINATOR as u16,
        MicrostableError::InvalidFeeBps
    );

    let basket = &mut ctx.accounts.basket_config;
    basket.assets = assets.into_iter().map(Into::into).collect();
    basket.bump = ctx.bumps.basket_config;

    let global = &mut ctx.accounts.global_state;
    global.authority = ctx.accounts.authority.key();
    global.mode = ProtocolMode::Normal;
    global.target_cr = target_cr;
    global.mint_fee_bps = mint_fee_bps;
    global.redeem_fee_bps = redeem_fee_bps;
    global.total_supply = 0;
    global.last_update_epoch = Clock::get()?.unix_timestamp;
    global.version = 1;
    global.bump = ctx.bumps.global_state;

    let circuit = &mut ctx.accounts.circuit_state;
    circuit.cb_states = [Default::default(); 4];
    circuit.bump = ctx.bumps.circuit_state;

    validate_common_invariants(global, basket, BPS_DENOMINATOR)?;

    Ok(())
}
