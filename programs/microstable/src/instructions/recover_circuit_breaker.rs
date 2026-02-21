use anchor_lang::prelude::*;

use crate::{
    errors::MicrostableError,
    instructions::validate_common_invariants,
    state::{
        BasketConfig, CBStatus, CircuitState, GlobalState, ProtocolMode, BASKET_CONFIG_SEED,
        CIRCUIT_BREAKER_COUNT, CIRCUIT_STATE_SEED, GLOBAL_STATE_SEED,
    },
};

fn min_hold_seconds(cb_index: u8, status: CBStatus) -> i64 {
    let base = match cb_index {
        0 => 5,
        1 => 10,
        2 => 3,
        3 => 3,
        _ => 5,
    };

    if matches!(status, CBStatus::ExtendedActive) {
        base * 3
    } else {
        base
    }
}

#[derive(Accounts)]
pub struct RecoverCircuitBreaker<'info> {
    pub watchdog: Signer<'info>,
    #[account(mut, seeds = [GLOBAL_STATE_SEED], bump = global_state.bump)]
    pub global_state: Account<'info, GlobalState>,
    #[account(seeds = [BASKET_CONFIG_SEED], bump = basket_config.bump)]
    pub basket_config: Account<'info, BasketConfig>,
    #[account(mut, seeds = [CIRCUIT_STATE_SEED], bump = circuit_state.bump)]
    pub circuit_state: Account<'info, CircuitState>,
}

pub fn handler(
    ctx: Context<RecoverCircuitBreaker>,
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

    let now = Clock::get()?.unix_timestamp;
    let circuit = &mut ctx.accounts.circuit_state;
    let entry = &mut circuit.cb_states[cb_index as usize];

    require!(entry.is_active(), MicrostableError::CircuitBreakerNotActive);

    let hold = min_hold_seconds(cb_index, entry.status);
    require!(
        now >= entry.activated_at.saturating_add(hold),
        MicrostableError::CircuitBreakerRecoveryNotReady
    );

    entry.status = CBStatus::Cooldown;
    entry.last_recovery_at = now;

    let global = &mut ctx.accounts.global_state;
    global.mode = if circuit.any_active() {
        ProtocolMode::SafeMode
    } else {
        ProtocolMode::Normal
    };

    validate_common_invariants(global, &ctx.accounts.basket_config, effective_cr_bps)?;

    Ok(())
}
