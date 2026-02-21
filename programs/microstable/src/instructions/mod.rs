pub mod apply_update;
pub mod distribute_fees;
pub mod initialize;
pub mod mint;
pub mod recover_circuit_breaker;
pub mod redeem;
pub mod submit_update;
pub mod trigger_circuit_breaker;

use anchor_lang::prelude::*;

use crate::{
    errors::MicrostableError,
    state::{BasketConfig, GlobalState, BPS_DENOMINATOR, CR_HARD_MIN_BPS},
};

pub use apply_update::*;
pub use distribute_fees::*;
pub use initialize::*;
pub use mint::*;
pub use recover_circuit_breaker::*;
pub use redeem::*;
pub use submit_update::*;
pub use trigger_circuit_breaker::*;

pub fn validate_common_invariants(
    global_state: &GlobalState,
    basket_config: &BasketConfig,
    effective_cr_bps: u64,
) -> Result<()> {
    require!(
        basket_config.total_weight_bps() == BPS_DENOMINATOR,
        MicrostableError::InvalidWeightSum
    );

    if global_state.total_supply > 0 {
        require!(
            effective_cr_bps >= CR_HARD_MIN_BPS,
            MicrostableError::CrHardMinViolation
        );
    }

    Ok(())
}

pub fn apply_fee_bps(amount: u64, fee_bps: u16) -> Result<u64> {
    let numerator = amount
        .checked_mul((BPS_DENOMINATOR as u16).saturating_sub(fee_bps) as u64)
        .ok_or(MicrostableError::MathOverflow)?;

    numerator
        .checked_div(BPS_DENOMINATOR)
        .ok_or(MicrostableError::MathOverflow.into())
}
