use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::{self, Mint as TokenMint, Token, TokenAccount, TransferChecked};

declare_id!("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3");

const SCALE: u64 = 1_000_000;
const WEIGHT_STEP_LIMIT: u64 = 20_000; // 2%
const TURNOVER_LIMIT: u64 = 150_000; // 15%
const ORACLE_STALENESS_MAX: u64 = 120;
const ORACLE_CONFIDENCE_MAX: u64 = 50_000; // 5%
const DEPEG_ON_THRESHOLD: u64 = 20_000; // 2%
const DEPEG_OFF_THRESHOLD: u64 = 5_000; // 0.5%
const COOLDOWN_TICKS: u64 = 5;
// FIX HI-02: cap breaker activation duration to mitigate griefing DoS.
const MAX_ACTIVATION_DURATION: u64 = 120;
// FIX HI-02: widen recovery hysteresis window gradually after prolonged activation.
const ADAPTIVE_RECOVERY_BPS_MAX: u64 = 15_000; // +1.5%

// // BLUE-TEAM: F12 - restrict initializer to trusted deploy authority.
const TRUSTED_INITIALIZER: Pubkey = pubkey!("3fimeXDHiEK9oeJX6XM1rXNoavTCWhzbxNXVmwFzh6Kk");

// // BLUE-TEAM: INPUT-HARDEN - strict bounds for external numeric inputs.
const PRICE_MIN: u64 = 500_000; // $0.50
const PRICE_MAX: u64 = 1_500_000; // $1.50
const MAX_COLLATERAL_AMOUNT: u64 = 1_000_000_000_000;
const MAX_REBALANCE_SLIPPAGE_BPS: u64 = 1_500; // 15%
const BATCH_WINDOW_SLOTS: u64 = 32;

// // BLUE-TEAM: I25 - commit/reveal for large rebalances.
const LARGE_REBALANCE_THRESHOLD: u64 = 50_000; // 5%
const COMMIT_REVEAL_DELAY_SLOTS: u64 = 5;
const COMMIT_REVEAL_MAX_VALIDITY: u64 = 1_000;

#[program]
pub mod microstable {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, keeper_set: [Pubkey; 3]) -> Result<()> {
        let slot = Clock::get()?.slot;

        // FIX HI-04: enforce a 2-of-3 keeper set instead of single-key authority.
        validate_keeper_set(&keeper_set)?;
        require_keys_eq!(
            ctx.accounts.authority.key(),
            TRUSTED_INITIALIZER,
            ErrorCode::UnauthorizedInitializer
        );
        require!(
            keeper_set.iter().any(|k| *k == ctx.accounts.authority.key()),
            ErrorCode::InvalidKeeperSet
        );

        let protocol = &mut ctx.accounts.protocol_state;
        protocol.weights = [400_000, 300_000, 200_000, 100_000];
        protocol.fee_rate = 2_000; // 0.2%
        protocol.cr_target = 1_200_000; // 120%
        protocol.total_supply = 0;
        protocol.last_update_slot = slot;
        protocol.keeper_set = keeper_set;
        protocol.emergency_shutdown = false;
        // // BLUE-TEAM: I25 - initialize commit/reveal state.
        protocol.pending_rebalance_commit = [0u8; 32];
        protocol.pending_rebalance_slot = 0;
        protocol.pending_rebalance_expiry = 0;
        protocol.bump = ctx.bumps.protocol_state;

        init_vault(
            &mut ctx.accounts.vault_usdc,
            0,
            ctx.accounts.usdc_mint.key(),
            protocol.key(),
            550_000,
            50_000,
            slot,
            ctx.bumps.vault_usdc,
        );
        init_vault(
            &mut ctx.accounts.vault_usdt,
            1,
            ctx.accounts.usdt_mint.key(),
            protocol.key(),
            450_000,
            70_000,
            slot,
            ctx.bumps.vault_usdt,
        );
        init_vault(
            &mut ctx.accounts.vault_dai,
            2,
            ctx.accounts.dai_mint.key(),
            protocol.key(),
            450_000,
            80_000,
            slot,
            ctx.bumps.vault_dai,
        );
        init_vault(
            &mut ctx.accounts.vault_usds,
            3,
            ctx.accounts.usds_mint.key(),
            protocol.key(),
            350_000,
            100_000,
            slot,
            ctx.bumps.vault_usds,
        );

        let circuit = &mut ctx.accounts.circuit_breaker;
        circuit.status = [BreakerStatus::Inactive as u8; 4];
        circuit.activation_tick = [0; 4];
        circuit.trigger_count = [0; 4];
        circuit.cooldown_until = [0; 4];
        circuit.last_trigger_tick = [0; 4];
        circuit.recent_trigger_count = [0; 4];
        circuit.recovery_tick = [0; 4];
        circuit.cb1_collateral_index = 0;
        circuit.mint_rate_limit = SCALE;
        circuit.optimizer_enabled = true;
        circuit.learning_rate_scale = SCALE;
        // FIX HI-02: enforce maximum activation window for all breakers.
        circuit.max_activation_duration = MAX_ACTIVATION_DURATION;
        circuit.bump = ctx.bumps.circuit_breaker;

        let vaults = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(protocol, vaults)?;
        Ok(())
    }

    pub fn update_oracle(
        ctx: Context<UpdateOracle>,
        collateral_index: u8,
        price: u64,
        confidence: u64,
        observed_slot: u64,
    ) -> Result<()> {
        // FIX HI-04: require 2-of-3 keeper quorum for privileged oracle writes.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        require!(price >= PRICE_MIN && price <= PRICE_MAX, ErrorCode::InvalidPrice);
        require!(confidence <= ORACLE_CONFIDENCE_MAX, ErrorCode::ConfidenceTooHigh);
        require!(observed_slot <= slot, ErrorCode::InvalidObservedSlot);
        require!(
            slot.saturating_sub(observed_slot) <= ORACLE_STALENESS_MAX,
            ErrorCode::OracleStale
        );

        match collateral_index {
            0 => update_vault_oracle(&mut ctx.accounts.vault_usdc, price, confidence, observed_slot)?,
            1 => update_vault_oracle(&mut ctx.accounts.vault_usdt, price, confidence, observed_slot)?,
            2 => update_vault_oracle(&mut ctx.accounts.vault_dai, price, confidence, observed_slot)?,
            3 => update_vault_oracle(&mut ctx.accounts.vault_usds, price, confidence, observed_slot)?,
            _ => return err!(ErrorCode::InvalidCollateralIndex),
        }

        ctx.accounts.protocol_state.last_update_slot = slot;
        let vaults = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults)?;
        Ok(())
    }

    pub fn mint(ctx: Context<Mint>, collateral_index: u8, collateral_amount: u64) -> Result<()> {
        require!(collateral_index < 4, ErrorCode::InvalidCollateralIndex);
        require!(collateral_amount > 0, ErrorCode::InvalidAmount);
        require!(collateral_amount <= MAX_COLLATERAL_AMOUNT, ErrorCode::AmountTooLarge);
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        let vaults_before = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults_before)?;

        require!(
            !is_active_like(ctx.accounts.circuit_breaker.status[1]),
            ErrorCode::MintPausedByCircuitBreaker
        );
        require!(
            ctx.accounts.circuit_breaker.mint_rate_limit > 0,
            ErrorCode::MintPausedByCircuitBreaker
        );
        require!(
            !oracle_degraded(vaults_before, slot),
            ErrorCode::OracleDegraded
        );

        let price = match collateral_index {
            0 => {
                require!(
                    slot.saturating_sub(ctx.accounts.vault_usdc.last_oracle_slot) <= ORACLE_STALENESS_MAX,
                    ErrorCode::OracleStale
                );
                ctx.accounts.vault_usdc.price
            }
            1 => {
                require!(
                    slot.saturating_sub(ctx.accounts.vault_usdt.last_oracle_slot) <= ORACLE_STALENESS_MAX,
                    ErrorCode::OracleStale
                );
                ctx.accounts.vault_usdt.price
            }
            2 => {
                require!(
                    slot.saturating_sub(ctx.accounts.vault_dai.last_oracle_slot) <= ORACLE_STALENESS_MAX,
                    ErrorCode::OracleStale
                );
                ctx.accounts.vault_dai.price
            }
            3 => {
                require!(
                    slot.saturating_sub(ctx.accounts.vault_usds.last_oracle_slot) <= ORACLE_STALENESS_MAX,
                    ErrorCode::OracleStale
                );
                ctx.accounts.vault_usds.price
            }
            _ => return err!(ErrorCode::InvalidCollateralIndex),
        };

        let gross_musd = mul_div_floor(collateral_amount, price, SCALE)?;
        let max_mintable_by_cr = mul_div_floor(gross_musd, SCALE, ctx.accounts.protocol_state.cr_target)?;
        let fee = mul_div_ceil(max_mintable_by_cr, ctx.accounts.protocol_state.fee_rate, SCALE)?;
        let minted_musd = max_mintable_by_cr
            .checked_sub(fee)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        require!(minted_musd > 0, ErrorCode::InvalidAmount);

        let max_mint = mul_div_floor(gross_musd, ctx.accounts.circuit_breaker.mint_rate_limit, SCALE)?;
        require!(minted_musd <= max_mint, ErrorCode::MintRateLimited);

        // FIX CR-01: validate selected collateral mint/vault bindings and canonical ATA addresses.
        let (expected_mint, expected_vault_ata) = match collateral_index {
            0 => (ctx.accounts.vault_usdc.mint, ctx.accounts.vault_usdc.vault),
            1 => (ctx.accounts.vault_usdt.mint, ctx.accounts.vault_usdt.vault),
            2 => (ctx.accounts.vault_dai.mint, ctx.accounts.vault_dai.vault),
            3 => (ctx.accounts.vault_usds.mint, ctx.accounts.vault_usds.vault),
            _ => return err!(ErrorCode::InvalidCollateralIndex),
        };
        require_keys_eq!(ctx.accounts.collateral_mint.key(), expected_mint, ErrorCode::InvalidCollateralMint);
        require_keys_eq!(
            ctx.accounts.vault_collateral_ata.key(),
            expected_vault_ata,
            ErrorCode::InvalidTokenAccount
        );

        let expected_user_ata = get_associated_token_address(
            &ctx.accounts.user.key(),
            &ctx.accounts.collateral_mint.key(),
        );
        let expected_protocol_ata = get_associated_token_address(
            &ctx.accounts.protocol_state.key(),
            &ctx.accounts.collateral_mint.key(),
        );
        require_keys_eq!(ctx.accounts.user_collateral_ata.key(), expected_user_ata, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.vault_collateral_ata.key(), expected_protocol_ata, ErrorCode::InvalidTokenAccount);

        // FIX CR-01: execute real SPL transfer before mutating accounting state.
        token::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.user_collateral_ata.to_account_info(),
                    mint: ctx.accounts.collateral_mint.to_account_info(),
                    to: ctx.accounts.vault_collateral_ata.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            collateral_amount,
            ctx.accounts.collateral_mint.decimals,
        )?;

        match collateral_index {
            0 => {
                ctx.accounts.vault_usdc.total_deposits = ctx
                    .accounts
                    .vault_usdc
                    .total_deposits
                    .checked_add(collateral_amount)
                    .ok_or_else(|| error!(ErrorCode::MathOverflow))?
            }
            1 => {
                ctx.accounts.vault_usdt.total_deposits = ctx
                    .accounts
                    .vault_usdt
                    .total_deposits
                    .checked_add(collateral_amount)
                    .ok_or_else(|| error!(ErrorCode::MathOverflow))?
            }
            2 => {
                ctx.accounts.vault_dai.total_deposits = ctx
                    .accounts
                    .vault_dai
                    .total_deposits
                    .checked_add(collateral_amount)
                    .ok_or_else(|| error!(ErrorCode::MathOverflow))?
            }
            3 => {
                ctx.accounts.vault_usds.total_deposits = ctx
                    .accounts
                    .vault_usds
                    .total_deposits
                    .checked_add(collateral_amount)
                    .ok_or_else(|| error!(ErrorCode::MathOverflow))?
            }
            _ => return err!(ErrorCode::InvalidCollateralIndex),
        }

        let vaults_after = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        let total_value = total_collateral_value(vaults_after)?;
        let new_supply = ctx
            .accounts
            .protocol_state
            .total_supply
            .checked_add(minted_musd)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        let required_value = mul_div_ceil(new_supply, ctx.accounts.protocol_state.cr_target, SCALE)?;
        require!(total_value >= required_value, ErrorCode::InsufficientCollateralRatio);

        let user_position = &mut ctx.accounts.user_position;
        if user_position.owner == Pubkey::default() {
            user_position.owner = ctx.accounts.user.key();
            user_position.bump = ctx.bumps.user_position;
        }
        require_keys_eq!(user_position.owner, ctx.accounts.user.key(), ErrorCode::Unauthorized);

        user_position.usd_balance = user_position
            .usd_balance
            .checked_add(minted_musd)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        let idx = collateral_index as usize;
        user_position.collateral_deposits[idx] = user_position.collateral_deposits[idx]
            .checked_add(collateral_amount)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;

        ctx.accounts.protocol_state.total_supply = new_supply;
        ctx.accounts.protocol_state.last_update_slot = slot;

        assert_invariants(&ctx.accounts.protocol_state, vaults_after)?;
        Ok(())
    }

    pub fn redeem(ctx: Context<Redeem>, musd_amount: u64) -> Result<()> {
        require!(musd_amount > 0, ErrorCode::InvalidAmount);
        require!(musd_amount <= MAX_COLLATERAL_AMOUNT, ErrorCode::AmountTooLarge);
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        let vaults_before = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults_before)?;

        let user_position = &mut ctx.accounts.user_position;
        require_keys_eq!(user_position.owner, ctx.accounts.user.key(), ErrorCode::Unauthorized);
        require!(user_position.usd_balance >= musd_amount, ErrorCode::InsufficientBalance);
        require!(
            ctx.accounts.protocol_state.total_supply >= musd_amount,
            ErrorCode::InsufficientBalance
        );

        let supply_before = ctx.accounts.protocol_state.total_supply;
        require!(supply_before > 0, ErrorCode::InsufficientBalance);

        let payout_discount = if oracle_degraded(vaults_before, slot) {
            950_000
        } else {
            SCALE
        };

        // FIX CR-01: validate canonical mint/vault token-account bindings for all collateral legs.
        require_keys_eq!(ctx.accounts.usdc_mint.key(), ctx.accounts.vault_usdc.mint, ErrorCode::InvalidCollateralMint);
        require_keys_eq!(ctx.accounts.usdt_mint.key(), ctx.accounts.vault_usdt.mint, ErrorCode::InvalidCollateralMint);
        require_keys_eq!(ctx.accounts.dai_mint.key(), ctx.accounts.vault_dai.mint, ErrorCode::InvalidCollateralMint);
        require_keys_eq!(ctx.accounts.usds_mint.key(), ctx.accounts.vault_usds.mint, ErrorCode::InvalidCollateralMint);

        require_keys_eq!(ctx.accounts.vault_usdc_ata.key(), ctx.accounts.vault_usdc.vault, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.vault_usdt_ata.key(), ctx.accounts.vault_usdt.vault, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.vault_dai_ata.key(), ctx.accounts.vault_dai.vault, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.vault_usds_ata.key(), ctx.accounts.vault_usds.vault, ErrorCode::InvalidTokenAccount);

        let expected_user_usdc = get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.usdc_mint.key());
        let expected_user_usdt = get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.usdt_mint.key());
        let expected_user_dai = get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.dai_mint.key());
        let expected_user_usds = get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.usds_mint.key());
        require_keys_eq!(ctx.accounts.user_usdc_ata.key(), expected_user_usdc, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.user_usdt_ata.key(), expected_user_usdt, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.user_dai_ata.key(), expected_user_dai, ErrorCode::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.user_usds_ata.key(), expected_user_usds, ErrorCode::InvalidTokenAccount);

        let payout_usdc = preview_redeem_from_vault(
            ctx.accounts.vault_usdc.total_deposits,
            musd_amount,
            supply_before,
            payout_discount,
        )?;
        let payout_usdt = preview_redeem_from_vault(
            ctx.accounts.vault_usdt.total_deposits,
            musd_amount,
            supply_before,
            payout_discount,
        )?;
        let payout_dai = preview_redeem_from_vault(
            ctx.accounts.vault_dai.total_deposits,
            musd_amount,
            supply_before,
            payout_discount,
        )?;
        let payout_usds = preview_redeem_from_vault(
            ctx.accounts.vault_usds.total_deposits,
            musd_amount,
            supply_before,
            payout_discount,
        )?;

        // FIX CR-01: transfer out before decrementing vault accounting.
        transfer_vault_to_user(
            &ctx.accounts.token_program,
            &ctx.accounts.vault_usdc_ata,
            &ctx.accounts.user_usdc_ata,
            &ctx.accounts.usdc_mint,
            &ctx.accounts.protocol_state,
            payout_usdc,
        )?;
        transfer_vault_to_user(
            &ctx.accounts.token_program,
            &ctx.accounts.vault_usdt_ata,
            &ctx.accounts.user_usdt_ata,
            &ctx.accounts.usdt_mint,
            &ctx.accounts.protocol_state,
            payout_usdt,
        )?;
        transfer_vault_to_user(
            &ctx.accounts.token_program,
            &ctx.accounts.vault_dai_ata,
            &ctx.accounts.user_dai_ata,
            &ctx.accounts.dai_mint,
            &ctx.accounts.protocol_state,
            payout_dai,
        )?;
        transfer_vault_to_user(
            &ctx.accounts.token_program,
            &ctx.accounts.vault_usds_ata,
            &ctx.accounts.user_usds_ata,
            &ctx.accounts.usds_mint,
            &ctx.accounts.protocol_state,
            payout_usds,
        )?;

        apply_redeem_from_vault(&mut ctx.accounts.vault_usdc, payout_usdc)?;
        apply_redeem_from_vault(&mut ctx.accounts.vault_usdt, payout_usdt)?;
        apply_redeem_from_vault(&mut ctx.accounts.vault_dai, payout_dai)?;
        apply_redeem_from_vault(&mut ctx.accounts.vault_usds, payout_usds)?;

        user_position.collateral_redeemed[0] = user_position.collateral_redeemed[0]
            .checked_add(payout_usdc)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        user_position.collateral_redeemed[1] = user_position.collateral_redeemed[1]
            .checked_add(payout_usdt)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        user_position.collateral_redeemed[2] = user_position.collateral_redeemed[2]
            .checked_add(payout_dai)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        user_position.collateral_redeemed[3] = user_position.collateral_redeemed[3]
            .checked_add(payout_usds)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;

        user_position.usd_balance = user_position
            .usd_balance
            .checked_sub(musd_amount)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;

        ctx.accounts.protocol_state.total_supply = ctx
            .accounts
            .protocol_state
            .total_supply
            .checked_sub(musd_amount)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        ctx.accounts.protocol_state.last_update_slot = slot;

        let vaults_after = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults_after)?;
        Ok(())
    }

    pub fn commit_rebalance(
        ctx: Context<CommitRebalance>,
        commit_hash: [u8; 32],
        valid_for_slots: u64,
    ) -> Result<()> {
        // // BLUE-TEAM: I25 - keeper quorum-gated commit step for large rebalances.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );
        require!(valid_for_slots >= COMMIT_REVEAL_DELAY_SLOTS, ErrorCode::InvalidCommitWindow);
        require!(valid_for_slots <= COMMIT_REVEAL_MAX_VALIDITY, ErrorCode::InvalidCommitWindow);

        let slot = Clock::get()?.slot;
        let protocol = &mut ctx.accounts.protocol_state;
        protocol.pending_rebalance_commit = commit_hash;
        protocol.pending_rebalance_slot = slot;
        protocol.pending_rebalance_expiry = slot.saturating_add(valid_for_slots);
        protocol.last_update_slot = slot;
        Ok(())
    }

    pub fn rebalance(
        ctx: Context<Rebalance>,
        new_weights: [u64; 4],
        max_slippage_bps: u64,
        batch_slot: u64,
        reveal_salt: [u8; 32],
    ) -> Result<()> {
        // FIX HI-04: enforce keeper 2-of-3 quorum for privileged rebalances.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        require!(max_slippage_bps <= MAX_REBALANCE_SLIPPAGE_BPS, ErrorCode::InvalidSlippageBound);
        validate_batch_window(slot, batch_slot)?;
        validate_weight_sum(new_weights)?;

        let old_weights = ctx.accounts.protocol_state.weights;
        let mut l1: u64 = 0;
        for i in 0..4 {
            let d = abs_diff(old_weights[i], new_weights[i]);
            require!(d <= WEIGHT_STEP_LIMIT, ErrorCode::WeightStepTooLarge);
            l1 = l1.checked_add(d).ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        }
        let turnover = l1 / 2;
        require!(turnover <= TURNOVER_LIMIT, ErrorCode::TurnoverTooHigh);

        // // BLUE-TEAM: I25 - caller-defined slippage bound.
        let slippage_limit_ppm = max_slippage_bps.saturating_mul(100);
        require!(turnover <= slippage_limit_ppm, ErrorCode::SlippageExceeded);

        // // BLUE-TEAM: I25 - commit/reveal required for large turnover operations.
        if turnover >= LARGE_REBALANCE_THRESHOLD {
            let protocol = &mut ctx.accounts.protocol_state;
            require!(protocol.pending_rebalance_commit != [0u8; 32], ErrorCode::MissingCommitReveal);
            require!(slot >= protocol.pending_rebalance_slot.saturating_add(COMMIT_REVEAL_DELAY_SLOTS), ErrorCode::CommitRevealTooEarly);
            require!(slot <= protocol.pending_rebalance_expiry, ErrorCode::CommitRevealExpired);

            let expected = compute_rebalance_commit(protocol.key(), new_weights, batch_slot, reveal_salt);
            require!(expected == protocol.pending_rebalance_commit, ErrorCode::CommitRevealMismatch);

            protocol.pending_rebalance_commit = [0u8; 32];
            protocol.pending_rebalance_slot = 0;
            protocol.pending_rebalance_expiry = 0;
        }

        ctx.accounts.protocol_state.weights = new_weights;
        ctx.accounts.protocol_state.last_update_slot = slot;

        let vaults = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults)?;
        Ok(())
    }

    pub fn activate_circuit_breaker(
        ctx: Context<ManageCircuitBreaker>,
        cb_index: u8,
        collateral_index: u8,
    ) -> Result<()> {
        // FIX HI-04: enforce keeper 2-of-3 quorum for breaker management.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        let idx = cb_to_index(cb_index)?;

        {
            let vaults = [
                &ctx.accounts.vault_usdc,
                &ctx.accounts.vault_usdt,
                &ctx.accounts.vault_dai,
                &ctx.accounts.vault_usds,
            ];
            assert_invariants(&ctx.accounts.protocol_state, vaults)?;
            require!(
                can_activate(
                    cb_index,
                    collateral_index,
                    &ctx.accounts.protocol_state,
                    vaults,
                    slot
                )?,
                ErrorCode::CircuitConditionNotMet
            );
        }

        let circuit = &mut ctx.accounts.circuit_breaker;
        require!(
            !is_active_like(circuit.status[idx]),
            ErrorCode::CircuitBreakerAlreadyActive
        );
        require!(slot >= circuit.cooldown_until[idx], ErrorCode::CircuitBreakerCoolingDown);

        if slot.saturating_sub(circuit.last_trigger_tick[idx]) <= 30 {
            circuit.recent_trigger_count[idx] = circuit.recent_trigger_count[idx].saturating_add(1);
        } else {
            circuit.recent_trigger_count[idx] = 1;
        }

        circuit.last_trigger_tick[idx] = slot;
        circuit.trigger_count[idx] = circuit.trigger_count[idx]
            .checked_add(1)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        circuit.activation_tick[idx] = slot;
        circuit.status[idx] = if circuit.recent_trigger_count[idx] >= 3 {
            BreakerStatus::ExtendedActive as u8
        } else {
            BreakerStatus::Holding as u8
        };

        match cb_index {
            1 => {
                require!(collateral_index < 4, ErrorCode::InvalidCollateralIndex);
                circuit.cb1_collateral_index = collateral_index;
                let target_weight = ctx.accounts.protocol_state.weights[collateral_index as usize];
                match collateral_index {
                    0 => {
                        let v = &mut ctx.accounts.vault_usdc;
                        v.weight_cap = (v.base_weight_cap / 2).max(target_weight);
                    }
                    1 => {
                        let v = &mut ctx.accounts.vault_usdt;
                        v.weight_cap = (v.base_weight_cap / 2).max(target_weight);
                    }
                    2 => {
                        let v = &mut ctx.accounts.vault_dai;
                        v.weight_cap = (v.base_weight_cap / 2).max(target_weight);
                    }
                    3 => {
                        let v = &mut ctx.accounts.vault_usds;
                        v.weight_cap = (v.base_weight_cap / 2).max(target_weight);
                    }
                    _ => return err!(ErrorCode::InvalidCollateralIndex),
                }
                ctx.accounts.protocol_state.cr_target = ctx
                    .accounts
                    .protocol_state
                    .cr_target
                    .checked_add(50_000)
                    .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
            }
            2 => {
                circuit.mint_rate_limit = 0;
            }
            3 => {
                circuit.optimizer_enabled = false;
            }
            4 => {
                circuit.learning_rate_scale = 500_000;
            }
            _ => return err!(ErrorCode::InvalidCircuitBreaker),
        }

        ctx.accounts.protocol_state.last_update_slot = slot;
        let vaults = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults)?;
        Ok(())
    }

    pub fn recover_circuit_breaker(ctx: Context<ManageCircuitBreaker>, cb_index: u8) -> Result<()> {
        // FIX HI-04: enforce keeper 2-of-3 quorum for breaker recovery.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        let idx = cb_to_index(cb_index)?;
        let (status, activation_tick, cb1_index) = {
            let view = &ctx.accounts.circuit_breaker;
            (view.status[idx], view.activation_tick[idx], view.cb1_collateral_index)
        };

        require!(is_active_like(status), ErrorCode::CircuitBreakerNotActive);

        let min_hold = effective_min_hold(cb_index, status);
        require!(slot.saturating_sub(activation_tick) >= min_hold, ErrorCode::MinHoldNotMet);

        {
            let vaults = [
                &ctx.accounts.vault_usdc,
                &ctx.accounts.vault_usdt,
                &ctx.accounts.vault_dai,
                &ctx.accounts.vault_usds,
            ];
            assert_invariants(&ctx.accounts.protocol_state, vaults)?;
            require!(
                hysteresis_ok(
                    cb_index,
                    &ctx.accounts.protocol_state,
                    vaults,
                    &ctx.accounts.circuit_breaker,
                    slot
                )?,
                ErrorCode::HysteresisNotMet
            );
        }

        let circuit = &mut ctx.accounts.circuit_breaker;
        circuit.status[idx] = BreakerStatus::Recovery as u8;
        circuit.cooldown_until[idx] = slot.saturating_add(COOLDOWN_TICKS);
        circuit.recovery_tick[idx] = slot;

        match cb_index {
            1 => match cb1_index {
                0 => progressive_restore_cap(&mut ctx.accounts.vault_usdc)?,
                1 => progressive_restore_cap(&mut ctx.accounts.vault_usdt)?,
                2 => progressive_restore_cap(&mut ctx.accounts.vault_dai)?,
                3 => progressive_restore_cap(&mut ctx.accounts.vault_usds)?,
                _ => return err!(ErrorCode::InvalidCollateralIndex),
            },
            2 => {
                circuit.mint_rate_limit = 500_000;
            }
            3 => {
                circuit.optimizer_enabled = true;
            }
            4 => {
                circuit.learning_rate_scale = 500_000;
            }
            _ => return err!(ErrorCode::InvalidCircuitBreaker),
        }

        ctx.accounts.protocol_state.last_update_slot = slot;
        let vaults = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults)?;
        Ok(())
    }

    pub fn emergency_shutdown(ctx: Context<EmergencyShutdown>) -> Result<()> {
        // FIX HI-04: any single authorized keeper can trigger emergency shutdown.
        require_keeper_member(&ctx.accounts.protocol_state, ctx.accounts.keeper.key())?;

        let protocol = &mut ctx.accounts.protocol_state;
        protocol.emergency_shutdown = true;
        protocol.last_update_slot = Clock::get()?.slot;

        let circuit = &mut ctx.accounts.circuit_breaker;
        circuit.mint_rate_limit = 0;
        circuit.optimizer_enabled = false;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = ProtocolState::SPACE,
        seeds = [b"protocol_state"],
        bump
    )]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(
        init,
        payer = authority,
        space = CircuitBreakerState::SPACE,
        seeds = [b"circuit_breaker"],
        bump
    )]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,

    #[account(
        init,
        payer = authority,
        space = CollateralVault::SPACE,
        seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()],
        bump
    )]
    pub vault_usdc: Account<'info, CollateralVault>,

    #[account(
        init,
        payer = authority,
        space = CollateralVault::SPACE,
        seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()],
        bump
    )]
    pub vault_usdt: Account<'info, CollateralVault>,

    #[account(
        init,
        payer = authority,
        space = CollateralVault::SPACE,
        seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()],
        bump
    )]
    pub vault_dai: Account<'info, CollateralVault>,

    #[account(
        init,
        payer = authority,
        space = CollateralVault::SPACE,
        seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()],
        bump
    )]
    pub vault_usds: Account<'info, CollateralVault>,

    /// FIX CR-01: canonical collateral mint bindings are configured at init.
    pub usdc_mint: Account<'info, TokenMint>,
    pub usdt_mint: Account<'info, TokenMint>,
    pub dai_mint: Account<'info, TokenMint>,
    pub usds_mint: Account<'info, TokenMint>,

    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateOracle<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Account<'info, CollateralVault>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,
}

#[derive(Accounts)]
pub struct Mint<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Account<'info, CollateralVault>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init_if_needed,
        payer = user,
        space = UserPosition::SPACE,
        seeds = [b"user_position", user.key().as_ref()],
        bump
    )]
    pub user_position: Account<'info, UserPosition>,

    /// FIX CR-01: enforce canonical user ATA for selected collateral.
    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = user,
    )]
    pub user_collateral_ata: Account<'info, TokenAccount>,

    /// FIX CR-01: enforce canonical protocol vault ATA for selected collateral.
    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = protocol_state,
    )]
    pub vault_collateral_ata: Account<'info, TokenAccount>,

    pub collateral_mint: Account<'info, TokenMint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Redeem<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Account<'info, CollateralVault>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user_position", user.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_position: Account<'info, UserPosition>,

    /// FIX CR-01: canonical user ATAs for all collateral payouts.
    #[account(mut, associated_token::mint = usdc_mint, associated_token::authority = user)]
    pub user_usdc_ata: Account<'info, TokenAccount>,
    #[account(mut, associated_token::mint = usdt_mint, associated_token::authority = user)]
    pub user_usdt_ata: Account<'info, TokenAccount>,
    #[account(mut, associated_token::mint = dai_mint, associated_token::authority = user)]
    pub user_dai_ata: Account<'info, TokenAccount>,
    #[account(mut, associated_token::mint = usds_mint, associated_token::authority = user)]
    pub user_usds_ata: Account<'info, TokenAccount>,

    /// FIX CR-01: canonical protocol vault ATAs for all collateral payouts.
    #[account(mut, associated_token::mint = usdc_mint, associated_token::authority = protocol_state)]
    pub vault_usdc_ata: Account<'info, TokenAccount>,
    #[account(mut, associated_token::mint = usdt_mint, associated_token::authority = protocol_state)]
    pub vault_usdt_ata: Account<'info, TokenAccount>,
    #[account(mut, associated_token::mint = dai_mint, associated_token::authority = protocol_state)]
    pub vault_dai_ata: Account<'info, TokenAccount>,
    #[account(mut, associated_token::mint = usds_mint, associated_token::authority = protocol_state)]
    pub vault_usds_ata: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, TokenMint>,
    pub usdt_mint: Account<'info, TokenMint>,
    pub dai_mint: Account<'info, TokenMint>,
    pub usds_mint: Account<'info, TokenMint>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}

#[derive(Accounts)]
pub struct CommitRebalance<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,
    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,
}

#[derive(Accounts)]
pub struct Rebalance<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,

    #[account(seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Account<'info, CollateralVault>,

    #[account(seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Account<'info, CollateralVault>,

    #[account(seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Account<'info, CollateralVault>,

    #[account(seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Account<'info, CollateralVault>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,
}

#[derive(Accounts)]
pub struct ManageCircuitBreaker<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Account<'info, CollateralVault>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Account<'info, CollateralVault>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,
}

#[derive(Accounts)]
pub struct EmergencyShutdown<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,
    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Account<'info, CircuitBreakerState>,
    pub keeper: Signer<'info>,
}

#[account]
pub struct ProtocolState {
    pub weights: [u64; 4],
    pub fee_rate: u64,
    pub cr_target: u64,
    pub total_supply: u64,
    pub last_update_slot: u64,
    /// FIX HI-04: 2-of-3 keeper multisig set.
    pub keeper_set: [Pubkey; 3],
    pub emergency_shutdown: bool,
    // // BLUE-TEAM: I25 - commit/reveal storage for large rebalances.
    pub pending_rebalance_commit: [u8; 32],
    pub pending_rebalance_slot: u64,
    pub pending_rebalance_expiry: u64,
    pub bump: u8,
}

impl ProtocolState {
    pub const SPACE: usize = 8 + 256;
}

#[account]
pub struct CollateralVault {
    pub index: u8,
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub oracle: Pubkey,
    pub risk_score: u64,
    pub weight_cap: u64,
    pub base_weight_cap: u64,
    pub price: u64,
    pub confidence: u64,
    pub last_oracle_slot: u64,
    pub total_deposits: u64,
    pub bump: u8,
}

impl CollateralVault {
    pub const SPACE: usize = 8 + 256;
}

#[account]
pub struct CircuitBreakerState {
    pub status: [u8; 4],
    pub activation_tick: [u64; 4],
    pub trigger_count: [u64; 4],
    pub cooldown_until: [u64; 4],
    pub last_trigger_tick: [u64; 4],
    pub recent_trigger_count: [u8; 4],
    pub recovery_tick: [u64; 4],
    pub cb1_collateral_index: u8,
    pub mint_rate_limit: u64,
    pub optimizer_enabled: bool,
    pub learning_rate_scale: u64,
    /// FIX HI-02: bounded breaker active window.
    pub max_activation_duration: u64,
    pub bump: u8,
}

impl CircuitBreakerState {
    pub const SPACE: usize = 8 + 640;
}

#[account]
pub struct UserPosition {
    pub owner: Pubkey,
    pub usd_balance: u64,
    pub collateral_deposits: [u64; 4],
    pub collateral_redeemed: [u64; 4],
    pub bump: u8,
}

impl UserPosition {
    pub const SPACE: usize = 8 + 160;
}

#[repr(u8)]
pub enum BreakerStatus {
    Inactive = 0,
    Active = 1,
    Holding = 2,
    Recovery = 3,
    ExtendedActive = 4,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized keeper")]
    Unauthorized,
    #[msg("Invalid collateral index")]
    InvalidCollateralIndex,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Weight sum invariant violated")]
    WeightSumInvariant,
    #[msg("Weight exceeds collateral cap")]
    WeightCapExceeded,
    #[msg("CR target must be > 0")]
    InvalidCrTarget,
    #[msg("Oracle input is stale")]
    OracleStale,
    #[msg("Oracle confidence too high")]
    ConfidenceTooHigh,
    #[msg("Observed slot is invalid")]
    InvalidObservedSlot,
    #[msg("Oracle update must be monotonic")]
    OracleSlotRegression,
    #[msg("Invalid oracle price")]
    InvalidPrice,
    #[msg("Insufficient collateral ratio")]
    InsufficientCollateralRatio,
    #[msg("Mint is paused by circuit breaker")]
    MintPausedByCircuitBreaker,
    #[msg("Mint amount exceeds active rate limit")]
    MintRateLimited,
    #[msg("Insufficient balance")]
    InsufficientBalance,
    #[msg("Weight change exceeds per-step limit")]
    WeightStepTooLarge,
    #[msg("Turnover exceeds 15%")]
    TurnoverTooHigh,
    #[msg("Circuit breaker id must be 1..4")]
    InvalidCircuitBreaker,
    #[msg("Circuit breaker condition not met")]
    CircuitConditionNotMet,
    #[msg("Circuit breaker already active")]
    CircuitBreakerAlreadyActive,
    #[msg("Circuit breaker is cooling down")]
    CircuitBreakerCoolingDown,
    #[msg("Circuit breaker is not active")]
    CircuitBreakerNotActive,
    #[msg("Circuit breaker minimum hold time not met")]
    MinHoldNotMet,
    #[msg("Hysteresis condition not met")]
    HysteresisNotMet,
    #[msg("Oracle is degraded")]
    OracleDegraded,
    #[msg("Keeper quorum not met")]
    KeeperQuorumNotMet,
    #[msg("Keeper signer duplicated")]
    DuplicateKeeperSigner,
    #[msg("Invalid keeper set")]
    InvalidKeeperSet,
    #[msg("Invalid collateral mint binding")]
    InvalidCollateralMint,
    #[msg("Invalid token account")]
    InvalidTokenAccount,
    #[msg("Emergency shutdown is active")]
    EmergencyShutdownActive,
    #[msg("Unauthorized initializer")]
    UnauthorizedInitializer,
    #[msg("Amount exceeds hard maximum")]
    AmountTooLarge,
    #[msg("Invalid commit/reveal validity window")]
    InvalidCommitWindow,
    #[msg("Missing rebalance commit")]
    MissingCommitReveal,
    #[msg("Commit/reveal too early")]
    CommitRevealTooEarly,
    #[msg("Commit/reveal expired")]
    CommitRevealExpired,
    #[msg("Commit/reveal hash mismatch")]
    CommitRevealMismatch,
    #[msg("Rebalance outside allowed batch window")]
    OutsideBatchWindow,
    #[msg("Invalid slippage bound")]
    InvalidSlippageBound,
    #[msg("Rebalance slippage exceeds caller bound")]
    SlippageExceeded,
}

fn init_vault(
    vault: &mut Account<CollateralVault>,
    index: u8,
    mint: Pubkey,
    protocol_authority: Pubkey,
    cap: u64,
    risk_score: u64,
    slot: u64,
    bump: u8,
) {
    vault.index = index;
    vault.mint = mint;
    // FIX CR-01: bind each vault to canonical protocol ATA for its mint.
    vault.vault = get_associated_token_address(&protocol_authority, &mint);
    vault.oracle = Pubkey::default();
    vault.risk_score = risk_score;
    vault.weight_cap = cap;
    vault.base_weight_cap = cap;
    vault.price = SCALE;
    vault.confidence = 1_000;
    vault.last_oracle_slot = slot;
    vault.total_deposits = 0;
    vault.bump = bump;
}

fn update_vault_oracle(
    vault: &mut Account<CollateralVault>,
    price: u64,
    confidence: u64,
    observed_slot: u64,
) -> Result<()> {
    require!(
        observed_slot >= vault.last_oracle_slot,
        ErrorCode::OracleSlotRegression
    );
    vault.price = price;
    vault.confidence = confidence;
    vault.last_oracle_slot = observed_slot;
    Ok(())
}

fn validate_keeper_set(keeper_set: &[Pubkey; 3]) -> Result<()> {
    // FIX HI-04: reject duplicate/default keeper keys at initialization.
    require!(
        keeper_set.iter().all(|k| *k != Pubkey::default()),
        ErrorCode::InvalidKeeperSet
    );
    require!(
        keeper_set[0] != keeper_set[1]
            && keeper_set[0] != keeper_set[2]
            && keeper_set[1] != keeper_set[2],
        ErrorCode::InvalidKeeperSet
    );
    Ok(())
}

fn keeper_member(protocol: &ProtocolState, signer: Pubkey) -> bool {
    protocol.keeper_set.iter().any(|k| *k == signer)
}

fn require_keeper_member(protocol: &ProtocolState, signer: Pubkey) -> Result<()> {
    require!(keeper_member(protocol, signer), ErrorCode::Unauthorized);
    Ok(())
}

fn require_keeper_quorum(protocol: &ProtocolState, signer_a: Pubkey, signer_b: Pubkey) -> Result<()> {
    require!(signer_a != signer_b, ErrorCode::DuplicateKeeperSigner);
    let ok_a = keeper_member(protocol, signer_a);
    let ok_b = keeper_member(protocol, signer_b);
    require!(ok_a && ok_b, ErrorCode::KeeperQuorumNotMet);
    Ok(())
}

fn mul_div_floor(a: u64, b: u64, denominator: u64) -> Result<u64> {
    require!(denominator > 0, ErrorCode::MathOverflow);
    let numerator = (a as u128)
        .checked_mul(b as u128)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    let value = numerator
        .checked_div(denominator as u128)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    u64::try_from(value).map_err(|_| error!(ErrorCode::MathOverflow))
}

fn mul_div_ceil(a: u64, b: u64, denominator: u64) -> Result<u64> {
    require!(denominator > 0, ErrorCode::MathOverflow);
    let numerator = (a as u128)
        .checked_mul(b as u128)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    let adjusted = numerator
        .checked_add((denominator as u128).saturating_sub(1))
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    let value = adjusted
        .checked_div(denominator as u128)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    u64::try_from(value).map_err(|_| error!(ErrorCode::MathOverflow))
}

fn abs_diff(a: u64, b: u64) -> u64 {
    if a >= b {
        a - b
    } else {
        b - a
    }
}

fn compute_rebalance_commit(
    protocol_key: Pubkey,
    new_weights: [u64; 4],
    batch_slot: u64,
    reveal_salt: [u8; 32],
) -> [u8; 32] {
    let mut weights_bytes = [0u8; 32];
    for (i, w) in new_weights.iter().enumerate() {
        let start = i * 8;
        weights_bytes[start..start + 8].copy_from_slice(&w.to_le_bytes());
    }

    let digest = hashv(&[
        b"rebalance_commit_v1",
        protocol_key.as_ref(),
        &weights_bytes,
        &batch_slot.to_le_bytes(),
        &reveal_salt,
    ]);
    digest.to_bytes()
}

fn validate_batch_window(slot: u64, batch_slot: u64) -> Result<()> {
    // // BLUE-TEAM: I25 - constrain execution to predefined batch windows.
    require!(
        slot / BATCH_WINDOW_SLOTS == batch_slot / BATCH_WINDOW_SLOTS,
        ErrorCode::OutsideBatchWindow
    );
    Ok(())
}

fn validate_weight_sum(weights: [u64; 4]) -> Result<()> {
    let mut sum = 0u64;
    for w in weights {
        require!(w <= SCALE, ErrorCode::WeightCapExceeded);
        sum = sum.checked_add(w).ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    }
    require!(abs_diff(sum, SCALE) <= 1, ErrorCode::WeightSumInvariant);
    Ok(())
}

fn assert_invariants<'info>(
    protocol: &ProtocolState,
    vaults: [&Account<'info, CollateralVault>; 4],
) -> Result<()> {
    validate_weight_sum(protocol.weights)?;
    require!(protocol.cr_target > 0, ErrorCode::InvalidCrTarget);
    for (i, v) in vaults.iter().enumerate() {
        require!(protocol.weights[i] <= v.weight_cap, ErrorCode::WeightCapExceeded);
    }
    Ok(())
}

fn total_collateral_value<'info>(vaults: [&Account<'info, CollateralVault>; 4]) -> Result<u64> {
    let mut total: u128 = 0;
    for v in vaults {
        let piece = (v.total_deposits as u128)
            .checked_mul(v.price as u128)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?
            .checked_div(SCALE as u128)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        total = total
            .checked_add(piece)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    }
    u64::try_from(total).map_err(|_| error!(ErrorCode::MathOverflow))
}

fn oracle_degraded<'info>(vaults: [&Account<'info, CollateralVault>; 4], slot: u64) -> bool {
    vaults.iter().any(|v| {
        v.price == 0
            || v.confidence > ORACLE_CONFIDENCE_MAX
            || slot.saturating_sub(v.last_oracle_slot) > ORACLE_STALENESS_MAX
    })
}

fn preview_redeem_from_vault(
    total_deposits: u64,
    musd_amount: u64,
    supply_before: u64,
    discount: u64,
) -> Result<u64> {
    let gross = mul_div_floor(total_deposits, musd_amount, supply_before)?;
    mul_div_floor(gross, discount, SCALE)
}

fn apply_redeem_from_vault(vault: &mut Account<CollateralVault>, payout: u64) -> Result<()> {
    vault.total_deposits = vault
        .total_deposits
        .checked_sub(payout)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
    Ok(())
}

fn transfer_vault_to_user<'info>(
    token_program: &Program<'info, Token>,
    from: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    mint: &Account<'info, TokenMint>,
    authority: &Account<'info, ProtocolState>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }

    let bump_seed = [authority.bump];
    let signer_seeds: &[&[u8]] = &[b"protocol_state", &bump_seed];

    token::transfer_checked(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
        mint.decimals,
    )?;

    Ok(())
}

fn cb_to_index(cb_index: u8) -> Result<usize> {
    if !(1..=4).contains(&cb_index) {
        return err!(ErrorCode::InvalidCircuitBreaker);
    }
    Ok((cb_index - 1) as usize)
}

fn min_hold_ticks(cb_index: u8) -> u64 {
    match cb_index {
        1 => 5,
        2 => 10,
        3 => 3,
        4 => 3,
        _ => 0,
    }
}

fn effective_min_hold(cb_index: u8, status: u8) -> u64 {
    let base = min_hold_ticks(cb_index);
    if status == BreakerStatus::ExtendedActive as u8 {
        base.saturating_mul(3)
    } else {
        base
    }
}

fn refresh_circuit_breakers(circuit: &mut CircuitBreakerState, slot: u64) {
    for i in 0..4 {
        if circuit.status[i] == BreakerStatus::Holding as u8 {
            let cb = (i + 1) as u8;
            if slot.saturating_sub(circuit.activation_tick[i]) >= min_hold_ticks(cb) {
                circuit.status[i] = BreakerStatus::Active as u8;
            }
        }

        // FIX HI-02: force breaker into recovery when max activation duration is exceeded.
        if is_active_like(circuit.status[i])
            && slot.saturating_sub(circuit.activation_tick[i]) >= circuit.max_activation_duration
        {
            circuit.status[i] = BreakerStatus::Recovery as u8;
            circuit.recovery_tick[i] = slot;
            circuit.cooldown_until[i] = slot.saturating_add(COOLDOWN_TICKS);
        }

        if i == 3
            && circuit.status[i] == BreakerStatus::Recovery as u8
            && slot.saturating_sub(circuit.recovery_tick[i]) >= 10
        {
            // // BLUE-TEAM: F8 - opportunistic LR restore while still in recovery.
            circuit.learning_rate_scale = SCALE;
        }

        if circuit.status[i] == BreakerStatus::Recovery as u8 && slot >= circuit.cooldown_until[i] {
            if i == 3 {
                // // BLUE-TEAM: F8 - hard restore before Recovery->Inactive transition.
                circuit.learning_rate_scale = SCALE;
            }
            circuit.status[i] = BreakerStatus::Inactive as u8;
        }
    }

    if circuit.status[1] == BreakerStatus::Recovery as u8 {
        let elapsed = slot.saturating_sub(circuit.recovery_tick[1]);
        let steps = elapsed / 10;
        let increment = steps.saturating_mul(100_000);

        // FIX HI-02: adaptive recovery widens mint ramp when CB-2 was active for too long.
        let overtime = slot
            .saturating_sub(circuit.activation_tick[1])
            .saturating_sub(circuit.max_activation_duration);
        let adaptive_boost = ((overtime / 10).saturating_mul(50_000)).min(ADAPTIVE_RECOVERY_BPS_MAX);
        let start = 500_000u64.saturating_add(adaptive_boost).min(SCALE);

        circuit.mint_rate_limit = start.saturating_add(increment).min(SCALE);
    } else if circuit.status[1] == BreakerStatus::Inactive as u8 {
        circuit.mint_rate_limit = SCALE;
    }

}

fn is_active_like(status: u8) -> bool {
    status == BreakerStatus::Holding as u8
        || status == BreakerStatus::Active as u8
        || status == BreakerStatus::ExtendedActive as u8
}

fn can_activate<'info>(
    cb_index: u8,
    collateral_index: u8,
    protocol: &ProtocolState,
    vaults: [&Account<'info, CollateralVault>; 4],
    slot: u64,
) -> Result<bool> {
    let depeg_count = vaults
        .iter()
        .filter(|v| abs_diff(v.price, SCALE) > DEPEG_ON_THRESHOLD)
        .count();

    let ok = match cb_index {
        1 => {
            if collateral_index > 3 {
                false
            } else {
                abs_diff(vaults[collateral_index as usize].price, SCALE) > DEPEG_ON_THRESHOLD
            }
        }
        2 => depeg_count >= 2,
        3 => oracle_degraded(vaults, slot),
        4 => {
            if protocol.total_supply == 0 {
                false
            } else {
                let total = total_collateral_value(vaults)?;
                let cr = mul_div_floor(total, SCALE, protocol.total_supply)?;
                cr + 100_000 < protocol.cr_target
            }
        }
        _ => false,
    };

    Ok(ok)
}

fn hysteresis_ok<'info>(
    cb_index: u8,
    protocol: &ProtocolState,
    vaults: [&Account<'info, CollateralVault>; 4],
    circuit: &CircuitBreakerState,
    slot: u64,
) -> Result<bool> {
    let idx = cb_to_index(cb_index)?;
    let active_for = slot.saturating_sub(circuit.activation_tick[idx]);
    // FIX HI-02: adaptive hysteresis widens recovery windows under prolonged stress.
    let overtime = active_for.saturating_sub(circuit.max_activation_duration);
    let adaptive_bonus = ((overtime / 10).saturating_mul(1_000)).min(ADAPTIVE_RECOVERY_BPS_MAX);
    let adaptive_off_threshold = DEPEG_OFF_THRESHOLD.saturating_add(adaptive_bonus);

    let ok = match cb_index {
        1 => {
            let cb1_idx = circuit.cb1_collateral_index as usize;
            abs_diff(vaults[cb1_idx].price, SCALE) < adaptive_off_threshold
        }
        2 => {
            let healthy = vaults
                .iter()
                .all(|v| abs_diff(v.price, SCALE) < adaptive_off_threshold);
            // FIX HI-03: CB-3 (oracle degradation) blocks CB-2 recovery.
            healthy && !oracle_degraded(vaults, slot)
        }
        3 => !oracle_degraded(vaults, slot),
        4 => {
            if protocol.total_supply == 0 {
                true
            } else {
                let total = total_collateral_value(vaults)?;
                let cr = mul_div_floor(total, SCALE, protocol.total_supply)?;
                cr >= protocol.cr_target && !oracle_degraded(vaults, slot)
            }
        }
        _ => false,
    };

    Ok(ok)
}

fn progressive_restore_cap(vault: &mut Account<CollateralVault>) -> Result<()> {
    let gap = vault.base_weight_cap.saturating_sub(vault.weight_cap);
    if gap == 0 {
        return Ok(());
    }
    let step = (gap / 20).max(1);
    vault.weight_cap = vault
        .weight_cap
        .checked_add(step)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?
        .min(vault.base_weight_cap);
    Ok(())
}
