use anchor_lang::prelude::*;

#[error_code]
pub enum MicrostableError {
    #[msg("Invalid asset list")]
    InvalidAssetList,
    #[msg("Weight sum must be exactly 10000 bps")]
    InvalidWeightSum,
    #[msg("Effective collateral ratio below hard minimum")]
    CrHardMinViolation,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Invalid fee bps")]
    InvalidFeeBps,
    #[msg("Only authority can perform this action")]
    Unauthorized,
    #[msg("Minting is disabled in current mode")]
    MintDisabled,
    #[msg("Redeem is disabled in current mode")]
    RedeemDisabled,
    #[msg("Circuit breaker index out of range")]
    InvalidCircuitBreaker,
    #[msg("Circuit breaker already active")]
    CircuitBreakerAlreadyActive,
    #[msg("Circuit breaker is not active")]
    CircuitBreakerNotActive,
    #[msg("Circuit breaker recovery condition not met")]
    CircuitBreakerRecoveryNotReady,
    #[msg("Bounded delta violated")]
    DeltaCapExceeded,
    #[msg("Target CR delta cap exceeded")]
    TargetCrDeltaExceeded,
    #[msg("Fee delta cap exceeded")]
    FeeDeltaExceeded,
    #[msg("Proposal weights length mismatch")]
    ProposalLengthMismatch,
    #[msg("Mint amount too small after fees")]
    MintAmountTooSmall,
    #[msg("Redeem amount too small after fees")]
    RedeemAmountTooSmall,
    #[msg("Queue throttle exceeded")]
    QueueThrottleExceeded,
}
