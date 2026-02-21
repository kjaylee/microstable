use anchor_lang::prelude::*;

use crate::{
    errors::MicrostableError,
    instructions::validate_common_invariants,
    state::{
        BasketConfig, CBStatus, CircuitState, GlobalState, ProtocolMode, BASKET_CONFIG_SEED,
        CIRCUIT_BREAKER_COUNT, CIRCUIT_STATE_SEED, GLOBAL_STATE_SEED,
    },
};

#[derive(Accounts)]
pub struct TriggerCircuitBreaker<'info> {
    pub watchdog: Signer<'info>,
    #[account(mut, seeds = [GLOBAL_STATE_SEED], bump = global_state.bump)]
    pub global_state: Account<'info, GlobalState>,
    #[account(seeds = [BASKET_CONFIG_SEED], bump = basket_config.bump)]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(mut, seeds = [CIRCUIT_STATE_SEED], bump = circuit_state.bump)]
    pub circuit_state: Account<'info, CircuitState>,
}

pub fn handler(
    ctx: Context<TriggerCircuitBreaker>,
    cb_index: u8,
    effective_cr_bps: u64,
) -> Result<()> {
    validate_common_invariants(
        &ctx.accounts.global_state,
        &ctx.accounts.basket_config,
        effective_cr_bps,
    )?;

    require!(
        (cb_index as usize) < CIRCUIT_BREAKER_COUNT,
        MicrostableError::InvalidCircuitBreaker
    );

    let circuit = &mut ctx.accounts.circuit_state;
    let entry = &mut circuit.cb_states[cb_index as usize];

    require!(
        !entry.is_active(),
        MicrostableError::CircuitBreakerAlreadyActive
    );

    entry.status = CBStatus::Active;
    entry.activated_at = Clock::get()?.unix_timestamp;
    entry.activation_count_30 = entry.activation_count_30.saturating_add(1);

    if entry.activation_count_30 >= 3 {
        entry.status = CBStatus::ExtendedActive;
    }

    let global = &mut ctx.accounts.global_state;
    global.mode = match cb_index {
        0 => ProtocolMode::SafeMode,
        1 => ProtocolMode::Frozen,
        2 => ProtocolMode::SafeMode,
        3 => ProtocolMode::Frozen,
        _ => ProtocolMode::SafeMode,
    };

    validate_common_invariants(global, &ctx.accounts.basket_config, effective_cr_bps)?;

    Ok(())
}
