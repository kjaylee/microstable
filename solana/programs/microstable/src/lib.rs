#![allow(deprecated)]
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    hash::{hash, hashv},
    program::invoke,
    system_instruction,
};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::{
    self, Burn, Mint as TokenMint, MintTo, Token, TokenAccount, TransferChecked,
};

declare_id!("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3");

const SCALE: u64 = 1_000_000;
const CR_TARGET_MIN: u64 = 1_000_000; // 100%
const CR_TARGET_MAX: u64 = 2_000_000; // 200%
const FEE_RATE_MAX: u64 = 10_000; // 1%
const WEIGHT_STEP_LIMIT: u64 = 20_000; // 2%
const TURNOVER_LIMIT: u64 = 150_000; // 15%
const ORACLE_STALENESS_MAX: u64 = 120;
// FIX HIGH-02/21/22: apply stricter freshness bounds to user mint/redeem paths.
const MINT_ORACLE_STALENESS_MAX: u64 = 20;
const REDEEM_ORACLE_STALENESS_MAX: u64 = 45;
const HIGH_VOL_MINT_ORACLE_STALENESS_MAX: u64 = 8;
const HIGH_VOL_REDEEM_ORACLE_STALENESS_MAX: u64 = 16;
const HIGH_VOL_ORACLE_STALENESS_MAX: u64 = 30;
const MINT_ORACLE_CONFIDENCE_MAX: u64 = 20_000; // 2%
                                                // FIX CRITICAL-22: stale oracle data receives progressive valuation haircut.
const STALE_ORACLE_PENALTY_PER_SLOT: u64 = 1_500; // 0.15%/slot
                                                  // FIX CRITICAL-22: confidence spread receives progressive valuation haircut.
const CONFIDENCE_PENALTY_MULTIPLIER: u64 = 4; // 4x confidence ratio penalty
                                              // FIX HIGH-22: hard stop for depeg mint attempts.
const MINT_DEPEG_PAUSE_THRESHOLD: u64 = 30_000; // 3%
                                                // FIX CRITICAL-21/HI-02: on-chain per-slot flow controls.
const SLOT_FLOW_LIMIT_MIN_UNITS: u64 = 50_000_000; // 50 MSTB @ 6 decimals
const DEFAULT_MAX_MINT_PER_SLOT_PPM: u64 = 60_000; // 6%
const DEFAULT_MAX_REDEEM_PER_SLOT_PPM: u64 = 30_000; // 3%
const MAX_MINT_PER_TX_PPM: u64 = 20_000; // 2%
const MAX_REDEEM_PER_TX_PPM: u64 = 15_000; // 1.5%
const MAX_ABSOLUTE_REDEEM_PER_SLOT: u64 = 2_500_000_000; // 2,500 MSTB @ 6 decimals
const REDEEM_VELOCITY_FEE_START_PPM: u64 = 400_000; // start surcharge above 40% slot utilization
const MAX_PROGRESSIVE_REDEEM_FEE_RATE: u64 = 10_000; // capped to global protocol fee max (1%)
                                                     // FIX MEDIUM-23/10: governance update pacing limits.
const AGENT_GOVERNANCE_COOLDOWN_SECS: i64 = 60;
const AGENT_SCORE_DELTA_LIMIT: u64 = 100_000; // 10%
                                              // FIX HIGH-03: manual oracle write path is disabled unless explicitly time-boxed.
const MANUAL_ORACLE_MODE_MAX_SLOTS: u64 = 120;
const MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS: u64 = 600;
const MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_MAX_SLOTS: u64 = 38_400;
const MANUAL_ORACLE_BACKOFF_WINDOW_SLOTS: u64 = 216_000;
const MANUAL_ORACLE_MAX_ACTIVATIONS_PER_EPOCH: u64 = 3;
const TWAP_ALPHA_PPM: u64 = 250_000; // 25% fresh observation, 75% history.
const TWAP_MAX_DEVIATION_PPM: u64 = 25_000; // 2.5%
const HIGH_VOLATILITY_DEVIATION_PPM: u64 = 12_500; // 1.25%
                                                   // FIX PTV2-002: enforce publish_time freshness for Pyth price updates.
const PYTH_PUBLISH_TIME_MAX_AGE: i64 = 60;
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
const PYTH_RECEIVER_PROGRAM: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
// FIX PTV2-011: enforce canonical 6-decimal collateral mints.
const COLLATERAL_DECIMALS_EXPECTED: u8 = 6;
// FIX PTV2-005: collateral-index feed allowlist.
const PYTH_USDC_USD: Pubkey = pubkey!("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX");
const PYTH_USDT_USD: Pubkey = pubkey!("HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM");
const PYTH_DAI_USD: Pubkey = pubkey!("FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN");
const PYTH_USDS_USD: Pubkey = pubkey!("9h4r3d4s8Jc8k5YfVY6Bnd3ETf6gVfGvSzj8Pzpo7aQw");
// FIX PTV2-003: expected feed-id binding per collateral.
const PYTH_FEED_ID_USDC: [u8; 32] = [
    0xea, 0xa0, 0x20, 0xc6, 0x1c, 0xc4, 0x79, 0x71, 0x28, 0x13, 0x46, 0x1c, 0xe1, 0x53, 0x89, 0x4a,
    0x96, 0xa6, 0xc0, 0x0b, 0x21, 0xed, 0x0c, 0xfc, 0x27, 0x98, 0xd1, 0xf9, 0xa9, 0xe9, 0xc9, 0x4a,
];
const PYTH_FEED_ID_USDT: [u8; 32] = [
    0x2b, 0x89, 0xb9, 0xdc, 0x8f, 0xdf, 0x9f, 0x34, 0x70, 0x9a, 0x5b, 0x10, 0x6b, 0x47, 0x2f, 0x0f,
    0x39, 0xbb, 0x6c, 0xa9, 0xce, 0x04, 0xb0, 0xfd, 0x7f, 0x2e, 0x97, 0x16, 0x88, 0xe2, 0xe5, 0x3b,
];
const PYTH_FEED_ID_DAI: [u8; 32] = [
    0xb0, 0x94, 0x8a, 0x5e, 0x53, 0x13, 0x20, 0x0c, 0x63, 0x2b, 0x51, 0xbb, 0x5c, 0xa3, 0x2f, 0x6d,
    0xe0, 0xd3, 0x6e, 0x99, 0x50, 0xa9, 0x42, 0xd1, 0x97, 0x51, 0xe8, 0x33, 0xf7, 0x0d, 0xab, 0xfd,
];
const PYTH_FEED_ID_USDS: [u8; 32] = [
    0xc2, 0xf5, 0xc9, 0xb4, 0xd9, 0xe7, 0xa1, 0xfc, 0xb5, 0xa8, 0x0c, 0x7a, 0x2c, 0x3e, 0xc0, 0xf8,
    0x4a, 0xb1, 0xde, 0x9f, 0x77, 0x8c, 0x0d, 0xf1, 0xb6, 0xe9, 0xc7, 0xab, 0x4f, 0x1e, 0x0d, 0x9a,
];
// FIX PTV2-004: bind accepted updates to trusted write authority.
const PYTH_TRUSTED_WRITE_AUTHORITY: Pubkey = TRUSTED_INITIALIZER;
const PYTH_PRICE_UPDATE_ACCOUNT_NAME: &str = "PriceUpdateV2";

// // BLUE-TEAM: INPUT-HARDEN - strict bounds for external numeric inputs.
const PRICE_MIN: u64 = 500_000; // $0.50
const PRICE_MAX: u64 = 1_500_000; // $1.50
const MAX_COLLATERAL_AMOUNT: u64 = 1_000_000_000_000;
const MAX_REBALANCE_SLIPPAGE_BPS: u64 = 1_500; // 15%
const BATCH_WINDOW_SLOTS: u64 = 32;

// // BLUE-TEAM: I25 - commit/reveal for large rebalances.
const LARGE_REBALANCE_THRESHOLD: u64 = 40_000; // 4% (<= 2 * WEIGHT_STEP_LIMIT)
const COMMIT_REVEAL_DELAY_SLOTS: u64 = 5;
const COMMIT_REVEAL_MAX_VALIDITY: u64 = 1_000;
const KEEPER_ROTATION_DELAY_SLOTS: u64 = 100;

// OAE / AIG constants.
const AGENT_MIN_STAKE_LAMPORTS: u64 = 1_000_000_000;
const AGENT_STAKE_COOLDOWN_SECONDS: i64 = 86_400;
const AGENT_SLASH_COOLDOWN_SLOTS: u64 = 100;
const AIG_MIN_COMMIT_TIER: u8 = 2;
const AIG_TIER1_THRESHOLD: u64 = 600_000;
const AIG_TIER2_THRESHOLD: u64 = 750_000;
const AIG_TIER3_THRESHOLD: u64 = 850_000;
const PROTOCOL_TREASURY: Pubkey = TRUSTED_INITIALIZER;

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
            keeper_set
                .iter()
                .any(|k| *k == ctx.accounts.authority.key()),
            ErrorCode::InvalidKeeperSet
        );

        // FIX PTV2-011: collateral mint decimals must remain invariant.
        require_collateral_decimals(&ctx.accounts.usdc_mint, COLLATERAL_DECIMALS_EXPECTED)?;
        require_collateral_decimals(&ctx.accounts.usdt_mint, COLLATERAL_DECIMALS_EXPECTED)?;
        require_collateral_decimals(&ctx.accounts.dai_mint, COLLATERAL_DECIMALS_EXPECTED)?;
        require_collateral_decimals(&ctx.accounts.usds_mint, COLLATERAL_DECIMALS_EXPECTED)?;

        let protocol = &mut ctx.accounts.protocol_state;
        protocol.weights = [400_000, 300_000, 200_000, 100_000];
        protocol.fee_rate = 2_000; // 0.2%
        protocol.mint_fee_rate = 2_000; // 0.2%
        protocol.redeem_fee_rate = 2_000; // 0.2%
        protocol.cr_target = 1_200_000; // 120%
        protocol.total_supply = 0;
        protocol.last_update_slot = slot;
        protocol.keeper_set = keeper_set;
        protocol.emergency_shutdown = false;
        // // BLUE-TEAM: I25 - initialize commit/reveal state.
        protocol.pending_rebalance_commit = [0u8; 32];
        protocol.pending_rebalance_slot = 0;
        protocol.pending_rebalance_expiry = 0;
        protocol.pending_keeper_set = [Pubkey::default(); 3];
        protocol.pending_keeper_activation_slot = 0;
        protocol.flow_control_slot = slot;
        protocol.minted_in_flow_slot = 0;
        protocol.redeemed_in_flow_slot = 0;
        protocol.last_twap_update_slots = [slot; 4];
        protocol.max_mint_per_slot_ppm = DEFAULT_MAX_MINT_PER_SLOT_PPM;
        protocol.max_redeem_per_slot_ppm = DEFAULT_MAX_REDEEM_PER_SLOT_PPM;
        protocol.manual_oracle_mode_expiry_slot = 0;
        protocol.bump = ctx.bumps.protocol_state;
        protocol.manual_oracle_reenable_delay_slots = MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS;
        protocol.manual_oracle_last_activation_slot = 0;
        protocol.manual_oracle_activation_epoch = 0;
        protocol.manual_oracle_activation_count_epoch = 0;

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

    pub fn migrate_legacy_state(
        ctx: Context<MigrateLegacyState>,
        keeper_set: [Pubkey; 3],
    ) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.authority.key(),
            TRUSTED_INITIALIZER,
            ErrorCode::UnauthorizedInitializer
        );
        validate_keeper_set(&keeper_set)?;
        require!(
            keeper_set
                .iter()
                .any(|k| *k == ctx.accounts.authority.key()),
            ErrorCode::InvalidKeeperSet
        );

        // FIX PTV2-011: collateral mint decimals must remain invariant.
        require_collateral_decimals(&ctx.accounts.usdc_mint, COLLATERAL_DECIMALS_EXPECTED)?;
        require_collateral_decimals(&ctx.accounts.usdt_mint, COLLATERAL_DECIMALS_EXPECTED)?;
        require_collateral_decimals(&ctx.accounts.dai_mint, COLLATERAL_DECIMALS_EXPECTED)?;
        require_collateral_decimals(&ctx.accounts.usds_mint, COLLATERAL_DECIMALS_EXPECTED)?;

        let program_id = ctx.program_id;
        let slot = Clock::get()?.slot;

        let (protocol_pda, protocol_bump) =
            Pubkey::find_program_address(&[b"protocol_state"], program_id);
        require_keys_eq!(
            ctx.accounts.protocol_state.key(),
            protocol_pda,
            ErrorCode::InvalidLegacyAccount
        );
        require_keys_eq!(
            *ctx.accounts.protocol_state.owner,
            *program_id,
            ErrorCode::InvalidLegacyAccount
        );
        {
            let info = ctx.accounts.protocol_state.to_account_info();
            let data = info
                .try_borrow_data()
                .map_err(|_| error!(ErrorCode::InvalidLegacyAccount))?;
            // FIX PTV2-007: migration is one-shot; reject reruns on initialized state.
            if data.len() >= 8 && data[..8] == *ProtocolState::DISCRIMINATOR {
                return err!(ErrorCode::MigrationAlreadyCompleted);
            }
        }
        ensure_account_space(
            &ctx.accounts.protocol_state.to_account_info(),
            &ctx.accounts.authority,
            &ctx.accounts.system_program,
            ProtocolState::SPACE,
        )?;

        let protocol = ProtocolState {
            weights: [400_000, 300_000, 200_000, 100_000],
            fee_rate: 2_000,
            mint_fee_rate: 2_000,
            redeem_fee_rate: 2_000,
            cr_target: 1_200_000,
            // FIX PTV2-008: guarded one-shot migration is restricted to pre-launch state.
            total_supply: 0,
            last_update_slot: slot,
            keeper_set,
            emergency_shutdown: false,
            pending_rebalance_commit: [0u8; 32],
            pending_rebalance_slot: 0,
            pending_rebalance_expiry: 0,
            pending_keeper_set: [Pubkey::default(); 3],
            pending_keeper_activation_slot: 0,
            flow_control_slot: slot,
            minted_in_flow_slot: 0,
            redeemed_in_flow_slot: 0,
            last_twap_update_slots: [slot; 4],
            max_mint_per_slot_ppm: DEFAULT_MAX_MINT_PER_SLOT_PPM,
            max_redeem_per_slot_ppm: DEFAULT_MAX_REDEEM_PER_SLOT_PPM,
            manual_oracle_mode_expiry_slot: 0,
            bump: protocol_bump,
            manual_oracle_reenable_delay_slots: MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS,
            manual_oracle_last_activation_slot: 0,
            manual_oracle_activation_epoch: 0,
            manual_oracle_activation_count_epoch: 0,
        };
        write_anchor_account(&ctx.accounts.protocol_state.to_account_info(), &protocol)?;

        let (circuit_pda, circuit_bump) =
            Pubkey::find_program_address(&[b"circuit_breaker"], program_id);
        require_keys_eq!(
            ctx.accounts.circuit_breaker.key(),
            circuit_pda,
            ErrorCode::InvalidLegacyAccount
        );
        require_keys_eq!(
            *ctx.accounts.circuit_breaker.owner,
            *program_id,
            ErrorCode::InvalidLegacyAccount
        );
        ensure_account_space(
            &ctx.accounts.circuit_breaker.to_account_info(),
            &ctx.accounts.authority,
            &ctx.accounts.system_program,
            CircuitBreakerState::SPACE,
        )?;

        let circuit = CircuitBreakerState {
            status: [BreakerStatus::Inactive as u8; 4],
            activation_tick: [0; 4],
            trigger_count: [0; 4],
            cooldown_until: [0; 4],
            last_trigger_tick: [0; 4],
            recent_trigger_count: [0; 4],
            recovery_tick: [0; 4],
            cb1_collateral_index: 0,
            mint_rate_limit: SCALE,
            optimizer_enabled: true,
            learning_rate_scale: SCALE,
            max_activation_duration: MAX_ACTIVATION_DURATION,
            bump: circuit_bump,
        };
        write_anchor_account(&ctx.accounts.circuit_breaker.to_account_info(), &circuit)?;

        migrate_vault_account(
            &ctx.accounts.vault_usdc.to_account_info(),
            0,
            ctx.accounts.usdc_mint.key(),
            protocol_pda,
            550_000,
            50_000,
            slot,
            program_id,
        )?;
        migrate_vault_account(
            &ctx.accounts.vault_usdt.to_account_info(),
            1,
            ctx.accounts.usdt_mint.key(),
            protocol_pda,
            450_000,
            70_000,
            slot,
            program_id,
        )?;
        migrate_vault_account(
            &ctx.accounts.vault_dai.to_account_info(),
            2,
            ctx.accounts.dai_mint.key(),
            protocol_pda,
            450_000,
            80_000,
            slot,
            program_id,
        )?;
        migrate_vault_account(
            &ctx.accounts.vault_usds.to_account_info(),
            3,
            ctx.accounts.usds_mint.key(),
            protocol_pda,
            350_000,
            100_000,
            slot,
            program_id,
        )?;

        Ok(())
    }

    pub fn register_agent(
        ctx: Context<RegisterAgent>,
        role: AgentRole,
        stake_amount: u64,
    ) -> Result<()> {
        validate_registration_stake(stake_amount)?;

        let clock = Clock::get()?;
        let now = clock.unix_timestamp;
        let slot = clock.slot;
        let agent_key = ctx.accounts.agent.key();

        invoke(
            &system_instruction::transfer(
                &agent_key,
                &ctx.accounts.agent_escrow.key(),
                stake_amount,
            ),
            &[
                ctx.accounts.agent.to_account_info(),
                ctx.accounts.agent_escrow.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        ctx.accounts.agent_escrow.agent = agent_key;
        ctx.accounts.agent_escrow.bump = ctx.bumps.agent_escrow;

        let record = &mut ctx.accounts.agent_record;
        record.set_inner(build_agent_record(
            agent_key,
            role,
            stake_amount,
            now,
            slot,
            ctx.bumps.agent_record,
        ));
        Ok(())
    }

    pub fn deregister_agent(ctx: Context<DeregisterAgent>) -> Result<()> {
        let record = &mut ctx.accounts.agent_record;
        require_keys_eq!(
            record.agent,
            ctx.accounts.agent.key(),
            ErrorCode::Unauthorized
        );
        require!(
            record.status != AgentStatus::Deregistered,
            ErrorCode::AgentAlreadyDeregistered
        );

        record.status = AgentStatus::Deregistered;
        record.last_active_at = Clock::get()?.unix_timestamp;
        Ok(())
    }

    pub fn update_agent_score(
        ctx: Context<UpdateAgentScore>,
        _agent: Pubkey,
        new_score: u64,
    ) -> Result<()> {
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        validate_agent_score(new_score)?;

        let now = Clock::get()?.unix_timestamp;
        let record = &mut ctx.accounts.agent_record;
        require!(
            record.status == AgentStatus::Active,
            ErrorCode::AgentNotActive
        );
        require!(
            now >= record
                .last_active_at
                .saturating_add(AGENT_GOVERNANCE_COOLDOWN_SECS),
            ErrorCode::AgentGovernanceCooldownActive
        );
        require!(
            abs_diff(record.agent_score, new_score) <= AGENT_SCORE_DELTA_LIMIT,
            ErrorCode::AgentScoreDeltaTooLarge
        );
        record.agent_score = new_score;
        record.last_active_at = now;
        Ok(())
    }

    pub fn promote_agent(ctx: Context<PromoteAgent>, _agent: Pubkey, new_tier: u8) -> Result<()> {
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;

        let now = Clock::get()?.unix_timestamp;
        let record = &mut ctx.accounts.agent_record;
        require!(
            record.status == AgentStatus::Active,
            ErrorCode::AgentNotActive
        );
        require!(
            now >= record
                .last_active_at
                .saturating_add(AGENT_GOVERNANCE_COOLDOWN_SECS),
            ErrorCode::AgentGovernanceCooldownActive
        );
        validate_tier_promotion(record.tier, new_tier, record.agent_score)?;
        record.tier = new_tier;
        record.last_active_at = now;
        Ok(())
    }

    pub fn demote_agent(ctx: Context<DemoteAgent>, _agent: Pubkey, new_tier: u8) -> Result<()> {
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;

        let now = Clock::get()?.unix_timestamp;
        let record = &mut ctx.accounts.agent_record;
        require!(
            record.status == AgentStatus::Active,
            ErrorCode::AgentNotActive
        );
        require!(
            now >= record
                .last_active_at
                .saturating_add(AGENT_GOVERNANCE_COOLDOWN_SECS),
            ErrorCode::AgentGovernanceCooldownActive
        );
        validate_tier_demotion(record.tier, new_tier)?;
        record.tier = new_tier;
        record.last_active_at = now;
        Ok(())
    }

    pub fn slash_agent(
        ctx: Context<SlashAgent>,
        _agent: Pubkey,
        slash_amount: u64,
        reason: [u8; 32],
    ) -> Result<()> {
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require_keys_eq!(
            ctx.accounts.protocol_treasury.key(),
            PROTOCOL_TREASURY,
            ErrorCode::InvalidTreasuryAccount
        );
        require!(slash_amount > 0, ErrorCode::InvalidAmount);

        let clock = Clock::get()?;
        let now = clock.unix_timestamp;
        let slot = clock.slot;
        let record = &mut ctx.accounts.agent_record;
        require!(
            slash_cooldown_elapsed(record.last_slashed_slot, slot),
            ErrorCode::SlashCooldownActive
        );

        require_keys_eq!(
            ctx.accounts.agent_escrow.agent,
            record.agent,
            ErrorCode::InvalidEscrowOwner
        );

        let slash_value = capped_slash_amount(record.stake, slash_amount);
        require!(slash_value > 0, ErrorCode::InvalidAmount);

        let escrow_info = ctx.accounts.agent_escrow.to_account_info();
        let treasury_info = ctx.accounts.protocol_treasury.to_account_info();

        let escrow_remaining = escrow_info
            .lamports()
            .checked_sub(slash_value)
            .ok_or_else(|| error!(ErrorCode::EscrowInsufficientBalance))?;
        let treasury_next = treasury_info
            .lamports()
            .checked_add(slash_value)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;

        **escrow_info.try_borrow_mut_lamports()? = escrow_remaining;
        **treasury_info.try_borrow_mut_lamports()? = treasury_next;

        record.stake = record
            .stake
            .checked_sub(slash_value)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        record.last_slashed_slot = slot;
        record.status = AgentStatus::Slashed;
        record.last_active_at = now;
        let _ = reason;
        Ok(())
    }

    pub fn claim_stake(ctx: Context<ClaimStake>, agent: Pubkey) -> Result<()> {
        require_keys_eq!(ctx.accounts.claimant.key(), agent, ErrorCode::Unauthorized);

        let now = Clock::get()?.unix_timestamp;
        let record = &mut ctx.accounts.agent_record;
        require_keys_eq!(record.agent, agent, ErrorCode::Unauthorized);
        require_keys_eq!(
            ctx.accounts.agent_escrow.agent,
            agent,
            ErrorCode::InvalidEscrowOwner
        );
        can_claim_stake(record.status, record.last_active_at, now)?;

        let claim_amount = record.stake;
        if claim_amount > 0 {
            let escrow_info = ctx.accounts.agent_escrow.to_account_info();
            let claimant_info = ctx.accounts.claimant.to_account_info();

            let escrow_remaining = escrow_info
                .lamports()
                .checked_sub(claim_amount)
                .ok_or_else(|| error!(ErrorCode::EscrowInsufficientBalance))?;
            let claimant_next = claimant_info
                .lamports()
                .checked_add(claim_amount)
                .ok_or_else(|| error!(ErrorCode::MathOverflow))?;

            **escrow_info.try_borrow_mut_lamports()? = escrow_remaining;
            **claimant_info.try_borrow_mut_lamports()? = claimant_next;
            record.stake = 0;
        }

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

        let slot = Clock::get()?.slot;
        require!(
            slot <= ctx.accounts.protocol_state.manual_oracle_mode_expiry_slot,
            ErrorCode::ManualOracleModeInactive
        );
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        require!(
            price >= PRICE_MIN && price <= PRICE_MAX,
            ErrorCode::InvalidPrice
        );
        require!(
            confidence <= ORACLE_CONFIDENCE_MAX,
            ErrorCode::ConfidenceTooHigh
        );
        require!(observed_slot <= slot, ErrorCode::InvalidObservedSlot);
        require!(
            slot.saturating_sub(observed_slot) <= ORACLE_STALENESS_MAX,
            ErrorCode::OracleStale
        );

        match collateral_index {
            0 => {
                let next_slot = update_vault_oracle(
                    &mut ctx.accounts.vault_usdc,
                    price,
                    confidence,
                    observed_slot,
                    ctx.accounts.protocol_state.last_twap_update_slots[0],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[0] = next_slot;
            }
            1 => {
                let next_slot = update_vault_oracle(
                    &mut ctx.accounts.vault_usdt,
                    price,
                    confidence,
                    observed_slot,
                    ctx.accounts.protocol_state.last_twap_update_slots[1],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[1] = next_slot;
            }
            2 => {
                let next_slot = update_vault_oracle(
                    &mut ctx.accounts.vault_dai,
                    price,
                    confidence,
                    observed_slot,
                    ctx.accounts.protocol_state.last_twap_update_slots[2],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[2] = next_slot;
            }
            3 => {
                let next_slot = update_vault_oracle(
                    &mut ctx.accounts.vault_usds,
                    price,
                    confidence,
                    observed_slot,
                    ctx.accounts.protocol_state.last_twap_update_slots[3],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[3] = next_slot;
            }
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

    pub fn set_pyth_feed(
        ctx: Context<SetPythFeed>,
        collateral_index: u8,
        pyth_price_feed: Pubkey,
    ) -> Result<()> {
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            pyth_price_feed != Pubkey::default(),
            ErrorCode::PythFeedNotConfigured
        );

        // FIX PTV2-005: enforce per-collateral feed allowlist.
        let expected_feed = expected_pyth_feed_account(collateral_index)?;
        require_keys_eq!(
            pyth_price_feed,
            expected_feed,
            ErrorCode::InvalidPythFeedAccount
        );

        // FIX PTV2-006: prevent assigning one feed account to multiple vaults.
        let existing = [
            ctx.accounts.vault_usdc.pyth_price_feed,
            ctx.accounts.vault_usdt.pyth_price_feed,
            ctx.accounts.vault_dai.pyth_price_feed,
            ctx.accounts.vault_usds.pyth_price_feed,
        ];
        for (i, feed) in existing.iter().enumerate() {
            if i != collateral_index as usize
                && *feed == pyth_price_feed
                && *feed != Pubkey::default()
            {
                return err!(ErrorCode::DuplicatePythFeed);
            }
        }

        match collateral_index {
            0 => ctx.accounts.vault_usdc.pyth_price_feed = pyth_price_feed,
            1 => ctx.accounts.vault_usdt.pyth_price_feed = pyth_price_feed,
            2 => ctx.accounts.vault_dai.pyth_price_feed = pyth_price_feed,
            3 => ctx.accounts.vault_usds.pyth_price_feed = pyth_price_feed,
            _ => return err!(ErrorCode::InvalidCollateralIndex),
        }

        ctx.accounts.protocol_state.last_update_slot = Clock::get()?.slot;
        let vaults = [
            &ctx.accounts.vault_usdc,
            &ctx.accounts.vault_usdt,
            &ctx.accounts.vault_dai,
            &ctx.accounts.vault_usds,
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults)?;
        Ok(())
    }

    pub fn update_oracle_pyth(ctx: Context<UpdateOraclePyth>, collateral_index: u8) -> Result<()> {
        // FIX PTV2-001 / RTV3-A34: require keeper quorum for Pyth oracle updates.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let clock = Clock::get()?;
        let slot = clock.slot;
        let unix_timestamp = clock.unix_timestamp;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        // FIX PTV2-003: bind updates to expected feed-id per collateral.
        let expected_feed_id = expected_pyth_feed_id(collateral_index)?;
        let mut allowed_authorities = Vec::with_capacity(4);
        allowed_authorities.push(PYTH_TRUSTED_WRITE_AUTHORITY);
        allowed_authorities.extend_from_slice(&ctx.accounts.protocol_state.keeper_set);
        match collateral_index {
            0 => {
                let next_slot = update_vault_oracle_from_pyth(
                    &mut ctx.accounts.vault_usdc,
                    &ctx.accounts.pyth_price_account,
                    slot,
                    unix_timestamp,
                    expected_feed_id,
                    &allowed_authorities,
                    ctx.accounts.protocol_state.last_twap_update_slots[0],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[0] = next_slot;
            }
            1 => {
                let next_slot = update_vault_oracle_from_pyth(
                    &mut ctx.accounts.vault_usdt,
                    &ctx.accounts.pyth_price_account,
                    slot,
                    unix_timestamp,
                    expected_feed_id,
                    &allowed_authorities,
                    ctx.accounts.protocol_state.last_twap_update_slots[1],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[1] = next_slot;
            }
            2 => {
                let next_slot = update_vault_oracle_from_pyth(
                    &mut ctx.accounts.vault_dai,
                    &ctx.accounts.pyth_price_account,
                    slot,
                    unix_timestamp,
                    expected_feed_id,
                    &allowed_authorities,
                    ctx.accounts.protocol_state.last_twap_update_slots[2],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[2] = next_slot;
            }
            3 => {
                let next_slot = update_vault_oracle_from_pyth(
                    &mut ctx.accounts.vault_usds,
                    &ctx.accounts.pyth_price_account,
                    slot,
                    unix_timestamp,
                    expected_feed_id,
                    &allowed_authorities,
                    ctx.accounts.protocol_state.last_twap_update_slots[3],
                )?;
                ctx.accounts.protocol_state.last_twap_update_slots[3] = next_slot;
            }
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

    pub fn mint(
        ctx: Context<Mint>,
        collateral_index: u8,
        collateral_amount: u64,
        max_price: u64,
    ) -> Result<()> {
        // FIX PTV2-012: execute real SPL collateral transfer + MSTB mint CPI path.
        require!(collateral_index < 4, ErrorCode::InvalidCollateralIndex);
        require!(collateral_amount > 0, ErrorCode::InvalidAmount);
        require!(
            collateral_amount <= MAX_COLLATERAL_AMOUNT,
            ErrorCode::AmountTooLarge
        );
        require!(max_price > 0, ErrorCode::InvalidSlippageBound);
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        let vaults_before = [
            ctx.accounts.vault_usdc.as_ref(),
            ctx.accounts.vault_usdt.as_ref(),
            ctx.accounts.vault_dai.as_ref(),
            ctx.accounts.vault_usds.as_ref(),
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
        let (price, confidence, oracle_slot, raw_twap_price, selected_oracle_degraded) =
            match collateral_index {
                0 => (
                    ctx.accounts.vault_usdc.price,
                    ctx.accounts.vault_usdc.confidence,
                    ctx.accounts.vault_usdc.last_oracle_slot,
                    ctx.accounts.vault_usdc.twap_price,
                    vault_oracle_degraded(
                        ctx.accounts.vault_usdc.as_ref(),
                        slot,
                        ORACLE_STALENESS_MAX,
                        HIGH_VOL_ORACLE_STALENESS_MAX,
                    ),
                ),
                1 => (
                    ctx.accounts.vault_usdt.price,
                    ctx.accounts.vault_usdt.confidence,
                    ctx.accounts.vault_usdt.last_oracle_slot,
                    ctx.accounts.vault_usdt.twap_price,
                    vault_oracle_degraded(
                        ctx.accounts.vault_usdt.as_ref(),
                        slot,
                        ORACLE_STALENESS_MAX,
                        HIGH_VOL_ORACLE_STALENESS_MAX,
                    ),
                ),
                2 => (
                    ctx.accounts.vault_dai.price,
                    ctx.accounts.vault_dai.confidence,
                    ctx.accounts.vault_dai.last_oracle_slot,
                    ctx.accounts.vault_dai.twap_price,
                    vault_oracle_degraded(
                        ctx.accounts.vault_dai.as_ref(),
                        slot,
                        ORACLE_STALENESS_MAX,
                        HIGH_VOL_ORACLE_STALENESS_MAX,
                    ),
                ),
                3 => (
                    ctx.accounts.vault_usds.price,
                    ctx.accounts.vault_usds.confidence,
                    ctx.accounts.vault_usds.last_oracle_slot,
                    ctx.accounts.vault_usds.twap_price,
                    vault_oracle_degraded(
                        ctx.accounts.vault_usds.as_ref(),
                        slot,
                        ORACLE_STALENESS_MAX,
                        HIGH_VOL_ORACLE_STALENESS_MAX,
                    ),
                ),
                _ => return err!(ErrorCode::InvalidCollateralIndex),
            };
        require!(!selected_oracle_degraded, ErrorCode::OracleDegraded);

        let twap_price = canonical_twap_price(price, raw_twap_price);
        validate_spot_vs_twap(price, twap_price)?;
        let mint_staleness_limit = dynamic_oracle_staleness_limit(
            price,
            twap_price,
            MINT_ORACLE_STALENESS_MAX,
            HIGH_VOL_MINT_ORACLE_STALENESS_MAX,
        );
        require!(
            slot.saturating_sub(oracle_slot) <= mint_staleness_limit,
            ErrorCode::OracleStale
        );
        require!(
            confidence <= MINT_ORACLE_CONFIDENCE_MAX,
            ErrorCode::ConfidenceTooHigh
        );
        require!(
            basket_max_depeg(vaults_before) < MINT_DEPEG_PAUSE_THRESHOLD,
            ErrorCode::DepegMintPaused
        );

        let mint_haircut_ppm = mint_haircut_ppm(price, confidence, oracle_slot, slot)?;
        let effective_price = mul_div_floor(price, mint_haircut_ppm, SCALE)?;
        require!(effective_price > 0, ErrorCode::InvalidPrice);
        require!(
            effective_price <= max_price,
            ErrorCode::MintPriceAboveUserLimit
        );

        let gross_musd = mul_div_floor(collateral_amount, effective_price, SCALE)?;
        let max_mintable_by_cr =
            mul_div_floor(gross_musd, SCALE, ctx.accounts.protocol_state.cr_target)?;
        let fee = protocol_fee_amount(
            max_mintable_by_cr,
            ctx.accounts.protocol_state.mint_fee_rate,
        )?;
        let minted_musd = max_mintable_by_cr
            .checked_sub(fee)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        require!(minted_musd > 0, ErrorCode::InvalidAmount);

        enforce_single_tx_flow_limit(
            ctx.accounts.protocol_state.total_supply,
            minted_musd,
            MAX_MINT_PER_TX_PPM,
            SLOT_FLOW_LIMIT_MIN_UNITS,
            SlotFlowKind::Mint,
        )?;

        let max_mint = mul_div_floor(
            gross_musd,
            ctx.accounts.circuit_breaker.mint_rate_limit,
            SCALE,
        )?;
        require!(minted_musd <= max_mint, ErrorCode::MintRateLimited);
        enforce_slot_flow_limit(
            &mut ctx.accounts.protocol_state,
            slot,
            minted_musd,
            SlotFlowKind::Mint,
        )?;

        // FIX CR-01: validate selected collateral mint/vault bindings and canonical ATA addresses.
        let (expected_mint, expected_vault_ata) = match collateral_index {
            0 => (ctx.accounts.vault_usdc.mint, ctx.accounts.vault_usdc.vault),
            1 => (ctx.accounts.vault_usdt.mint, ctx.accounts.vault_usdt.vault),
            2 => (ctx.accounts.vault_dai.mint, ctx.accounts.vault_dai.vault),
            3 => (ctx.accounts.vault_usds.mint, ctx.accounts.vault_usds.vault),
            _ => return err!(ErrorCode::InvalidCollateralIndex),
        };
        require_keys_eq!(
            ctx.accounts.collateral_mint.key(),
            expected_mint,
            ErrorCode::InvalidCollateralMint
        );
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
        let expected_user_mstb_ata =
            get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.mstb_mint.key());
        require_keys_eq!(
            ctx.accounts.user_collateral_ata.key(),
            expected_user_ata,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.vault_collateral_ata.key(),
            expected_protocol_ata,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.user_mstb_ata.key(),
            expected_user_mstb_ata,
            ErrorCode::InvalidTokenAccount
        );

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

        mint_mstb_to_user(
            &ctx.accounts.token_program,
            &ctx.accounts.mstb_mint,
            &ctx.accounts.user_mstb_ata,
            &ctx.accounts.protocol_state,
            minted_musd,
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
            ctx.accounts.vault_usdc.as_ref(),
            ctx.accounts.vault_usdt.as_ref(),
            ctx.accounts.vault_dai.as_ref(),
            ctx.accounts.vault_usds.as_ref(),
        ];
        let total_value = total_collateral_value(vaults_after)?;
        let new_supply = ctx
            .accounts
            .protocol_state
            .total_supply
            .checked_add(minted_musd)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        let required_value =
            mul_div_ceil(new_supply, ctx.accounts.protocol_state.cr_target, SCALE)?;
        require!(
            total_value >= required_value,
            ErrorCode::InsufficientCollateralRatio
        );

        let user_position = &mut ctx.accounts.user_position;
        if user_position.owner == Pubkey::default() {
            user_position.owner = ctx.accounts.user.key();
            user_position.bump = ctx.bumps.user_position;
        }
        require_keys_eq!(
            user_position.owner,
            ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

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

    pub fn redeem(ctx: Context<Redeem>, musd_amount: u64, min_out_amount: u64) -> Result<()> {
        // FIX PTV2-012: execute real MSTB burn + collateral transfer CPI path.
        require!(musd_amount > 0, ErrorCode::InvalidAmount);
        require!(
            musd_amount <= MAX_COLLATERAL_AMOUNT,
            ErrorCode::AmountTooLarge
        );
        require!(
            !ctx.accounts.protocol_state.emergency_shutdown,
            ErrorCode::EmergencyShutdownActive
        );

        let slot = Clock::get()?.slot;
        refresh_circuit_breakers(&mut ctx.accounts.circuit_breaker, slot);

        let vaults_before = [
            ctx.accounts.vault_usdc.as_ref(),
            ctx.accounts.vault_usdt.as_ref(),
            ctx.accounts.vault_dai.as_ref(),
            ctx.accounts.vault_usds.as_ref(),
        ];
        assert_invariants(&ctx.accounts.protocol_state, vaults_before)?;

        let user_position = &mut ctx.accounts.user_position;
        require_keys_eq!(
            user_position.owner,
            ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );
        require!(
            user_position.usd_balance >= musd_amount,
            ErrorCode::InsufficientBalance
        );
        require!(
            ctx.accounts.protocol_state.total_supply >= musd_amount,
            ErrorCode::InsufficientBalance
        );

        let supply_before = ctx.accounts.protocol_state.total_supply;
        require!(supply_before > 0, ErrorCode::InsufficientBalance);

        let degraded_redeem_vaults = count_degraded_vaults(
            vaults_before,
            slot,
            REDEEM_ORACLE_STALENESS_MAX,
            HIGH_VOL_REDEEM_ORACLE_STALENESS_MAX,
        );
        require!(degraded_redeem_vaults < 4, ErrorCode::OracleDegraded);

        refresh_slot_flow_window(&mut ctx.accounts.protocol_state, slot);
        let redeemed_in_flow_slot_before_tx = ctx.accounts.protocol_state.redeemed_in_flow_slot;
        let effective_redeem_fee_rate = progressive_redeem_fee_rate(
            &ctx.accounts.protocol_state,
            redeemed_in_flow_slot_before_tx,
        )?;

        enforce_single_tx_flow_limit(
            ctx.accounts.protocol_state.total_supply,
            musd_amount,
            MAX_REDEEM_PER_TX_PPM,
            SLOT_FLOW_LIMIT_MIN_UNITS,
            SlotFlowKind::Redeem,
        )?;

        enforce_slot_flow_limit(
            &mut ctx.accounts.protocol_state,
            slot,
            musd_amount,
            SlotFlowKind::Redeem,
        )?;

        let payout_discount = redeem_discount_ppm(vaults_before, slot)?;

        // FIX CR-01: validate canonical mint/vault token-account bindings for all collateral legs.
        require_keys_eq!(
            ctx.accounts.usdc_mint.key(),
            ctx.accounts.vault_usdc.mint,
            ErrorCode::InvalidCollateralMint
        );
        require_keys_eq!(
            ctx.accounts.usdt_mint.key(),
            ctx.accounts.vault_usdt.mint,
            ErrorCode::InvalidCollateralMint
        );
        require_keys_eq!(
            ctx.accounts.dai_mint.key(),
            ctx.accounts.vault_dai.mint,
            ErrorCode::InvalidCollateralMint
        );
        require_keys_eq!(
            ctx.accounts.usds_mint.key(),
            ctx.accounts.vault_usds.mint,
            ErrorCode::InvalidCollateralMint
        );

        require_keys_eq!(
            ctx.accounts.vault_usdc_ata.key(),
            ctx.accounts.vault_usdc.vault,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.vault_usdt_ata.key(),
            ctx.accounts.vault_usdt.vault,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.vault_dai_ata.key(),
            ctx.accounts.vault_dai.vault,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.vault_usds_ata.key(),
            ctx.accounts.vault_usds.vault,
            ErrorCode::InvalidTokenAccount
        );

        let expected_user_usdc =
            get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.usdc_mint.key());
        let expected_user_usdt =
            get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.usdt_mint.key());
        let expected_user_dai =
            get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.dai_mint.key());
        let expected_user_usds =
            get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.usds_mint.key());
        require_keys_eq!(
            ctx.accounts.user_usdc_ata.key(),
            expected_user_usdc,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.user_usdt_ata.key(),
            expected_user_usdt,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.user_dai_ata.key(),
            expected_user_dai,
            ErrorCode::InvalidTokenAccount
        );
        require_keys_eq!(
            ctx.accounts.user_usds_ata.key(),
            expected_user_usds,
            ErrorCode::InvalidTokenAccount
        );
        let expected_user_mstb =
            get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.mstb_mint.key());
        require_keys_eq!(
            ctx.accounts.user_mstb_ata.key(),
            expected_user_mstb,
            ErrorCode::InvalidTokenAccount
        );

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mstb_mint.to_account_info(),
                    from: ctx.accounts.user_mstb_ata.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            musd_amount,
        )?;

        let redeem_fee = protocol_fee_amount(musd_amount, effective_redeem_fee_rate)?;
        let net_redeem_musd = musd_amount
            .checked_sub(redeem_fee)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        require!(net_redeem_musd > 0, ErrorCode::InvalidAmount);

        let payout_usdc = preview_redeem_from_vault(
            ctx.accounts.vault_usdc.total_deposits,
            net_redeem_musd,
            supply_before,
            payout_discount,
        )?;
        let payout_usdt = preview_redeem_from_vault(
            ctx.accounts.vault_usdt.total_deposits,
            net_redeem_musd,
            supply_before,
            payout_discount,
        )?;
        let payout_dai = preview_redeem_from_vault(
            ctx.accounts.vault_dai.total_deposits,
            net_redeem_musd,
            supply_before,
            payout_discount,
        )?;
        let payout_usds = preview_redeem_from_vault(
            ctx.accounts.vault_usds.total_deposits,
            net_redeem_musd,
            supply_before,
            payout_discount,
        )?;

        let total_payout = payout_usdc
            .checked_add(payout_usdt)
            .and_then(|v| v.checked_add(payout_dai))
            .and_then(|v| v.checked_add(payout_usds))
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        require!(
            total_payout >= min_out_amount,
            ErrorCode::RedeemOutputBelowUserLimit
        );

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
            ctx.accounts.vault_usdc.as_ref(),
            ctx.accounts.vault_usdt.as_ref(),
            ctx.accounts.vault_dai.as_ref(),
            ctx.accounts.vault_usds.as_ref(),
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
        assert_agent_commit_eligibility(
            &ctx.accounts.agent_record,
            ctx.accounts.submitting_agent.key(),
        )?;
        require!(
            valid_for_slots >= COMMIT_REVEAL_DELAY_SLOTS,
            ErrorCode::InvalidCommitWindow
        );
        require!(
            valid_for_slots <= COMMIT_REVEAL_MAX_VALIDITY,
            ErrorCode::InvalidCommitWindow
        );

        let clock = Clock::get()?;
        let slot = clock.slot;

        let agent_record = &mut ctx.accounts.agent_record;
        agent_record.proposals_submitted = agent_record
            .proposals_submitted
            .checked_add(1)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        agent_record.last_active_at = clock.unix_timestamp;

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

        require!(
            max_slippage_bps <= MAX_REBALANCE_SLIPPAGE_BPS,
            ErrorCode::InvalidSlippageBound
        );
        validate_batch_window(slot, batch_slot)?;
        validate_weight_sum(new_weights)?;

        let old_weights = ctx.accounts.protocol_state.weights;
        let mut l1: u64 = 0;
        for i in 0..4 {
            let d = abs_diff(old_weights[i], new_weights[i]);
            require!(d <= WEIGHT_STEP_LIMIT, ErrorCode::WeightStepTooLarge);
            l1 = l1
                .checked_add(d)
                .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
        }
        let turnover = l1 / 2;
        require!(turnover <= TURNOVER_LIMIT, ErrorCode::TurnoverTooHigh);

        // // BLUE-TEAM: I25 - caller-defined slippage bound.
        let slippage_limit_ppm = max_slippage_bps.saturating_mul(100);
        require!(turnover <= slippage_limit_ppm, ErrorCode::SlippageExceeded);

        // // BLUE-TEAM: I25 - commit/reveal required for large turnover operations.
        if turnover >= LARGE_REBALANCE_THRESHOLD {
            let protocol = &mut ctx.accounts.protocol_state;
            require!(
                protocol.pending_rebalance_commit != [0u8; 32],
                ErrorCode::MissingCommitReveal
            );
            require!(
                slot >= protocol
                    .pending_rebalance_slot
                    .saturating_add(COMMIT_REVEAL_DELAY_SLOTS),
                ErrorCode::CommitRevealTooEarly
            );
            require!(
                slot <= protocol.pending_rebalance_expiry,
                ErrorCode::CommitRevealExpired
            );

            let expected =
                compute_rebalance_commit(protocol.key(), new_weights, batch_slot, reveal_salt);
            require!(
                expected == protocol.pending_rebalance_commit,
                ErrorCode::CommitRevealMismatch
            );

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

    pub fn update_protocol_params(
        ctx: Context<UpdateProtocolParams>,
        new_cr_target: u64,
        new_mint_fee: u64,
        new_redeem_fee: u64,
    ) -> Result<()> {
        let slot = Clock::get()?.slot;
        apply_protocol_param_update(
            &mut ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
            new_cr_target,
            new_mint_fee,
            new_redeem_fee,
            slot,
        )
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
        require!(
            slot >= circuit.cooldown_until[idx],
            ErrorCode::CircuitBreakerCoolingDown
        );

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
            (
                view.status[idx],
                view.activation_tick[idx],
                view.cb1_collateral_index,
            )
        };

        require!(is_active_like(status), ErrorCode::CircuitBreakerNotActive);

        let min_hold = effective_min_hold(cb_index, status);
        require!(
            slot.saturating_sub(activation_tick) >= min_hold,
            ErrorCode::MinHoldNotMet
        );

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
        // FIX PTV2-013: require 2-of-3 keeper quorum for global shutdown.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;

        let protocol = &mut ctx.accounts.protocol_state;
        protocol.emergency_shutdown = true;
        protocol.last_update_slot = Clock::get()?.slot;

        let circuit = &mut ctx.accounts.circuit_breaker;
        circuit.mint_rate_limit = 0;
        circuit.optimizer_enabled = false;

        Ok(())
    }

    pub fn resume_from_shutdown(ctx: Context<EmergencyShutdown>) -> Result<()> {
        // FIX PTV2-014: explicit keeper-quorum recovery path from shutdown.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        let circuit = &ctx.accounts.circuit_breaker;
        require!(
            circuit
                .status
                .iter()
                .all(|s| *s == BreakerStatus::Inactive as u8),
            ErrorCode::UnsafeToResume
        );

        let protocol = &mut ctx.accounts.protocol_state;
        protocol.emergency_shutdown = false;
        protocol.last_update_slot = Clock::get()?.slot;

        let circuit = &mut ctx.accounts.circuit_breaker;
        circuit.mint_rate_limit = SCALE;
        circuit.optimizer_enabled = true;
        Ok(())
    }

    pub fn rotate_keeper_set(
        ctx: Context<EmergencyShutdown>,
        new_keeper_set: [Pubkey; 3],
    ) -> Result<()> {
        // FIX PTV2-015 / PTV3-022: keeper rotation guarded by quorum + timelock.
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        validate_keeper_set(&new_keeper_set)?;

        let slot = Clock::get()?.slot;
        let protocol = &mut ctx.accounts.protocol_state;

        if protocol.pending_keeper_set != new_keeper_set {
            protocol.pending_keeper_set = new_keeper_set;
            protocol.pending_keeper_activation_slot =
                slot.saturating_add(KEEPER_ROTATION_DELAY_SLOTS);
            protocol.last_update_slot = slot;
            return Ok(());
        }

        require!(
            slot >= protocol.pending_keeper_activation_slot,
            ErrorCode::KeeperRotationTimelockActive
        );

        protocol.keeper_set = protocol.pending_keeper_set;
        protocol.pending_keeper_set = [Pubkey::default(); 3];
        protocol.pending_keeper_activation_slot = 0;
        protocol.last_update_slot = slot;
        Ok(())
    }

    pub fn enable_manual_oracle_mode(
        ctx: Context<EmergencyShutdown>,
        valid_for_slots: u64,
    ) -> Result<()> {
        require_keeper_quorum(
            &ctx.accounts.protocol_state,
            ctx.accounts.keeper_one.key(),
            ctx.accounts.keeper_two.key(),
        )?;
        require!(
            (1..=MANUAL_ORACLE_MODE_MAX_SLOTS).contains(&valid_for_slots),
            ErrorCode::InvalidManualOracleWindow
        );

        let clock = Clock::get()?;
        let slot = clock.slot;
        let epoch = clock.epoch;
        let protocol = &mut ctx.accounts.protocol_state;
        require!(
            slot > protocol.manual_oracle_mode_expiry_slot,
            ErrorCode::ManualOracleModeAlreadyActive
        );

        if protocol.manual_oracle_activation_epoch != epoch {
            protocol.manual_oracle_activation_epoch = epoch;
            protocol.manual_oracle_activation_count_epoch = 0;
        }
        require!(
            protocol.manual_oracle_activation_count_epoch < MANUAL_ORACLE_MAX_ACTIVATIONS_PER_EPOCH,
            ErrorCode::ManualOracleActivationLimitExceeded
        );

        let mut cooldown = protocol.manual_oracle_reenable_delay_slots;
        if cooldown == 0 {
            cooldown = MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS;
        }
        if protocol.manual_oracle_mode_expiry_slot > 0 {
            let reenable_slot = protocol
                .manual_oracle_mode_expiry_slot
                .saturating_add(cooldown);
            require!(
                slot >= reenable_slot,
                ErrorCode::ManualOracleModeCooldownActive
            );
        }

        require!(
            ctx.accounts
                .circuit_breaker
                .status
                .iter()
                .any(|status| *status != BreakerStatus::Inactive as u8),
            ErrorCode::ManualOracleModeRequiresCircuitBreaker
        );

        if protocol.manual_oracle_last_activation_slot > 0
            && slot.saturating_sub(protocol.manual_oracle_last_activation_slot)
                <= MANUAL_ORACLE_BACKOFF_WINDOW_SLOTS
        {
            cooldown = cooldown
                .saturating_mul(2)
                .min(MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_MAX_SLOTS);
        } else {
            cooldown = MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS;
        }

        protocol.manual_oracle_mode_expiry_slot = slot.saturating_add(valid_for_slots);
        protocol.manual_oracle_last_activation_slot = slot;
        protocol.manual_oracle_reenable_delay_slots = cooldown;
        protocol.manual_oracle_activation_count_epoch = protocol
            .manual_oracle_activation_count_epoch
            .saturating_add(1);
        protocol.last_update_slot = slot;
        Ok(())
    }

    #[cfg(feature = "devnet-admin")]
    /// DEVNET ONLY: upgrade authority can force-reinitialize protocol state.
    /// Handles struct size migration + keeper_set reset in one shot.
    /// Bypasses quorum/timelock for devnet recovery.
    pub fn devnet_force_reinit(
        ctx: Context<DevnetForceReinit>,
        new_keeper_set: [Pubkey; 3],
    ) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.authority.key(),
            TRUSTED_INITIALIZER,
            ErrorCode::UnauthorizedInitializer
        );
        validate_keeper_set(&new_keeper_set)?;

        let program_id = ctx.program_id;
        let slot = Clock::get()?.slot;

        // Resize protocol_state if needed
        ensure_account_space(
            &ctx.accounts.protocol_state.to_account_info(),
            &ctx.accounts.authority,
            &ctx.accounts.system_program,
            ProtocolState::SPACE,
        )?;

        let (_, protocol_bump) = Pubkey::find_program_address(&[b"protocol_state"], program_id);

        let protocol = ProtocolState {
            weights: [400_000, 300_000, 200_000, 100_000],
            fee_rate: 2_000,
            mint_fee_rate: 2_000,
            redeem_fee_rate: 2_000,
            cr_target: 1_200_000,
            total_supply: 0,
            last_update_slot: slot,
            keeper_set: new_keeper_set,
            emergency_shutdown: false,
            pending_rebalance_commit: [0u8; 32],
            pending_rebalance_slot: 0,
            pending_rebalance_expiry: 0,
            pending_keeper_set: [Pubkey::default(); 3],
            pending_keeper_activation_slot: 0,
            flow_control_slot: slot,
            minted_in_flow_slot: 0,
            redeemed_in_flow_slot: 0,
            last_twap_update_slots: [slot; 4],
            max_mint_per_slot_ppm: DEFAULT_MAX_MINT_PER_SLOT_PPM,
            max_redeem_per_slot_ppm: DEFAULT_MAX_REDEEM_PER_SLOT_PPM,
            manual_oracle_mode_expiry_slot: 0,
            bump: protocol_bump,
            manual_oracle_reenable_delay_slots: MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS,
            manual_oracle_last_activation_slot: 0,
            manual_oracle_activation_epoch: 0,
            manual_oracle_activation_count_epoch: 0,
        };
        write_anchor_account(&ctx.accounts.protocol_state.to_account_info(), &protocol)?;

        // Resize circuit_breaker if needed
        ensure_account_space(
            &ctx.accounts.circuit_breaker.to_account_info(),
            &ctx.accounts.authority,
            &ctx.accounts.system_program,
            CircuitBreakerState::SPACE,
        )?;

        let (_, circuit_bump) = Pubkey::find_program_address(&[b"circuit_breaker"], program_id);

        let circuit = CircuitBreakerState {
            status: [0u8; 4], // Inactive
            activation_tick: [0; 4],
            trigger_count: [0; 4],
            cooldown_until: [0; 4],
            last_trigger_tick: [0; 4],
            recent_trigger_count: [0; 4],
            recovery_tick: [0; 4],
            cb1_collateral_index: 0,
            mint_rate_limit: SCALE,
            optimizer_enabled: true,
            learning_rate_scale: SCALE,
            max_activation_duration: MAX_ACTIVATION_DURATION,
            bump: circuit_bump,
        };
        write_anchor_account(&ctx.accounts.circuit_breaker.to_account_info(), &circuit)?;

        msg!("devnet_force_reinit: protocol + circuit_breaker reset OK");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct MigrateLegacyState<'info> {
    /// CHECK: legacy account migration path; PDA and owner checked in instruction.
    #[account(mut)]
    pub protocol_state: UncheckedAccount<'info>,

    /// CHECK: legacy account migration path; PDA and owner checked in instruction.
    #[account(mut)]
    pub circuit_breaker: UncheckedAccount<'info>,

    /// CHECK: legacy account migration path; PDA and owner checked in instruction.
    #[account(mut)]
    pub vault_usdc: UncheckedAccount<'info>,

    /// CHECK: legacy account migration path; PDA and owner checked in instruction.
    #[account(mut)]
    pub vault_usdt: UncheckedAccount<'info>,

    /// CHECK: legacy account migration path; PDA and owner checked in instruction.
    #[account(mut)]
    pub vault_dai: UncheckedAccount<'info>,

    /// CHECK: legacy account migration path; PDA and owner checked in instruction.
    #[account(mut)]
    pub vault_usds: UncheckedAccount<'info>,

    pub usdc_mint: Account<'info, TokenMint>,
    pub usdt_mint: Account<'info, TokenMint>,
    pub dai_mint: Account<'info, TokenMint>,
    pub usds_mint: Account<'info, TokenMint>,

    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
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
pub struct SetPythFeed<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

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
pub struct UpdateOraclePyth<'info> {
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

    // FIX PTV2-001 / RTV3-A34: keeper quorum signer set.
    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,

    /// CHECK: ownership and feed binding are validated in instruction logic.
    pub pyth_price_account: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct Mint<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Box<Account<'info, ProtocolState>>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Box<Account<'info, CircuitBreakerState>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Box<Account<'info, CollateralVault>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Box<Account<'info, CollateralVault>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Box<Account<'info, CollateralVault>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Box<Account<'info, CollateralVault>>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init_if_needed,
        payer = user,
        space = UserPosition::SPACE,
        seeds = [b"user_position", user.key().as_ref()],
        bump
    )]
    pub user_position: Box<Account<'info, UserPosition>>,

    /// FIX CR-01: enforce canonical user ATA for selected collateral.
    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = user,
    )]
    pub user_collateral_ata: Box<Account<'info, TokenAccount>>,

    /// FIX CR-01: enforce canonical protocol vault ATA for selected collateral.
    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = protocol_state,
    )]
    pub vault_collateral_ata: Box<Account<'info, TokenAccount>>,

    #[account(mut, mint::authority = protocol_state)]
    pub mstb_mint: Box<Account<'info, TokenMint>>,

    #[account(
        mut,
        associated_token::mint = mstb_mint,
        associated_token::authority = user,
    )]
    pub user_mstb_ata: Box<Account<'info, TokenAccount>>,

    pub collateral_mint: Box<Account<'info, TokenMint>>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Redeem<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Box<Account<'info, ProtocolState>>,

    #[account(mut, seeds = [b"circuit_breaker"], bump = circuit_breaker.bump)]
    pub circuit_breaker: Box<Account<'info, CircuitBreakerState>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [0u8].as_ref()], bump = vault_usdc.bump)]
    pub vault_usdc: Box<Account<'info, CollateralVault>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [1u8].as_ref()], bump = vault_usdt.bump)]
    pub vault_usdt: Box<Account<'info, CollateralVault>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [2u8].as_ref()], bump = vault_dai.bump)]
    pub vault_dai: Box<Account<'info, CollateralVault>>,

    #[account(mut, seeds = [b"collateral_vault".as_ref(), [3u8].as_ref()], bump = vault_usds.bump)]
    pub vault_usds: Box<Account<'info, CollateralVault>>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user_position", user.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_position: Box<Account<'info, UserPosition>>,

    /// FIX CR-01: canonical user ATAs for all collateral payouts.
    #[account(mut, associated_token::mint = usdc_mint, associated_token::authority = user)]
    pub user_usdc_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = usdt_mint, associated_token::authority = user)]
    pub user_usdt_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = dai_mint, associated_token::authority = user)]
    pub user_dai_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = usds_mint, associated_token::authority = user)]
    pub user_usds_ata: Box<Account<'info, TokenAccount>>,

    /// FIX CR-01: canonical protocol vault ATAs for all collateral payouts.
    #[account(mut, associated_token::mint = usdc_mint, associated_token::authority = protocol_state)]
    pub vault_usdc_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = usdt_mint, associated_token::authority = protocol_state)]
    pub vault_usdt_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = dai_mint, associated_token::authority = protocol_state)]
    pub vault_dai_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = usds_mint, associated_token::authority = protocol_state)]
    pub vault_usds_ata: Box<Account<'info, TokenAccount>>,

    pub usdc_mint: Box<Account<'info, TokenMint>>,
    pub usdt_mint: Box<Account<'info, TokenMint>>,
    pub dai_mint: Box<Account<'info, TokenMint>>,
    pub usds_mint: Box<Account<'info, TokenMint>>,

    #[account(mut, mint::authority = protocol_state)]
    pub mstb_mint: Box<Account<'info, TokenMint>>,

    #[account(mut, associated_token::mint = mstb_mint, associated_token::authority = user)]
    pub user_mstb_ata: Box<Account<'info, TokenAccount>>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}

#[derive(Accounts)]
pub struct RegisterAgent<'info> {
    #[account(mut)]
    pub agent: Signer<'info>,

    #[account(
        init,
        payer = agent,
        space = AgentRecord::SPACE,
        seeds = [b"agent", agent.key().as_ref()],
        bump
    )]
    pub agent_record: Account<'info, AgentRecord>,

    #[account(
        init_if_needed,
        payer = agent,
        space = AgentEscrow::SPACE,
        seeds = [b"v2:agent_escrow", agent.key().as_ref()],
        bump
    )]
    pub agent_escrow: Account<'info, AgentEscrow>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DeregisterAgent<'info> {
    #[account(mut)]
    pub agent: Signer<'info>,

    #[account(
        mut,
        seeds = [b"agent", agent.key().as_ref()],
        bump = agent_record.bump
    )]
    pub agent_record: Account<'info, AgentRecord>,
}

#[derive(Accounts)]
#[instruction(agent: Pubkey, _new_score: u64)]
pub struct UpdateAgentScore<'info> {
    #[account(seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,

    #[account(mut, seeds = [b"agent", agent.as_ref()], bump = agent_record.bump)]
    pub agent_record: Account<'info, AgentRecord>,
}

#[derive(Accounts)]
#[instruction(agent: Pubkey, _new_tier: u8)]
pub struct PromoteAgent<'info> {
    #[account(seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,

    #[account(mut, seeds = [b"agent", agent.as_ref()], bump = agent_record.bump)]
    pub agent_record: Account<'info, AgentRecord>,
}

#[derive(Accounts)]
#[instruction(agent: Pubkey, _new_tier: u8)]
pub struct DemoteAgent<'info> {
    #[account(seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,

    #[account(mut, seeds = [b"agent", agent.as_ref()], bump = agent_record.bump)]
    pub agent_record: Account<'info, AgentRecord>,
}

#[derive(Accounts)]
#[instruction(agent: Pubkey, _slash_amount: u64, _reason: [u8; 32])]
pub struct SlashAgent<'info> {
    #[account(seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,

    #[account(mut, seeds = [b"agent", agent.as_ref()], bump = agent_record.bump)]
    pub agent_record: Account<'info, AgentRecord>,

    #[account(
        mut,
        seeds = [b"v2:agent_escrow", agent.as_ref()],
        bump = agent_escrow.bump
    )]
    pub agent_escrow: Account<'info, AgentEscrow>,

    #[account(mut)]
    pub protocol_treasury: SystemAccount<'info>,
}

#[derive(Accounts)]
#[instruction(agent: Pubkey)]
pub struct ClaimStake<'info> {
    #[account(mut)]
    pub claimant: Signer<'info>,

    #[account(
        mut,
        close = claimant,
        seeds = [b"agent", agent.as_ref()],
        bump = agent_record.bump
    )]
    pub agent_record: Account<'info, AgentRecord>,

    #[account(
        mut,
        seeds = [b"v2:agent_escrow", agent.as_ref()],
        bump = agent_escrow.bump
    )]
    pub agent_escrow: Account<'info, AgentEscrow>,
}

#[derive(Accounts)]
pub struct UpdateProtocolParams<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,
    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,
}

#[derive(Accounts)]
pub struct CommitRebalance<'info> {
    #[account(mut, seeds = [b"protocol_state"], bump = protocol_state.bump)]
    pub protocol_state: Account<'info, ProtocolState>,

    #[account(
        mut,
        seeds = [b"agent", submitting_agent.key().as_ref()],
        bump = agent_record.bump
    )]
    pub agent_record: Account<'info, AgentRecord>,

    pub submitting_agent: Signer<'info>,
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
    pub keeper_one: Signer<'info>,
    pub keeper_two: Signer<'info>,
}

#[cfg(feature = "devnet-admin")]
/// DEVNET ONLY: force-reinitialize protocol state (handles struct resize).
#[derive(Accounts)]
pub struct DevnetForceReinit<'info> {
    /// CHECK: raw account — will be resized and rewritten in-instruction.
    #[account(mut)]
    pub protocol_state: UncheckedAccount<'info>,
    /// CHECK: raw account — will be resized and rewritten in-instruction.
    #[account(mut)]
    pub circuit_breaker: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct ProtocolState {
    pub weights: [u64; 4],
    pub fee_rate: u64,
    pub mint_fee_rate: u64,
    pub redeem_fee_rate: u64,
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
    /// Pending keeper rotation (timelocked).
    pub pending_keeper_set: [Pubkey; 3],
    pub pending_keeper_activation_slot: u64,
    /// FIX CRITICAL-21: per-slot mint/redeem flow controls.
    pub flow_control_slot: u64,
    pub minted_in_flow_slot: u64,
    pub redeemed_in_flow_slot: u64,
    /// Per-vault TWAP update slots to enforce single update per slot.
    pub last_twap_update_slots: [u64; 4],
    pub max_mint_per_slot_ppm: u64,
    pub max_redeem_per_slot_ppm: u64,
    /// FIX HIGH-03: manual keeper oracle writes are time-boxed.
    pub manual_oracle_mode_expiry_slot: u64,
    pub bump: u8,
    /// Exponential cooldown state for manual oracle mode reactivation.
    pub manual_oracle_reenable_delay_slots: u64,
    pub manual_oracle_last_activation_slot: u64,
    pub manual_oracle_activation_epoch: u64,
    pub manual_oracle_activation_count_epoch: u64,
}

impl ProtocolState {
    pub const SPACE: usize = 8 + 512;
}

#[account]
pub struct AgentEscrow {
    pub agent: Pubkey,
    pub bump: u8,
}

impl AgentEscrow {
    pub const SPACE: usize = 8 + 64;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AgentRole {
    Optimizer = 0,
    Monitor = 1,
    Auditor = 2,
    Liquidator = 3,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AgentStatus {
    Active = 0,
    Cooldown = 1,
    Slashed = 2,
    Deregistered = 3,
}

#[account]
pub struct AgentRecord {
    pub agent: Pubkey,
    pub stake: u64,
    pub reputation: u64,
    pub role: AgentRole,
    pub tier: u8,
    pub status: AgentStatus,
    pub proposals_submitted: u64,
    pub proposals_accepted: u64,
    pub registered_at: i64,
    pub registered_slot: u64,
    pub last_active_at: i64,
    pub agent_score: u64,
    pub last_slashed_slot: u64,
    pub bump: u8,
}

impl AgentRecord {
    pub const SPACE: usize = 8 + 160;
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
    /// Optional Pyth feed account (receiver-owned price feed account).
    pub pyth_price_feed: Pubkey,
    /// EWMA TWAP price used for spot-vs-twap validation under volatility.
    pub twap_price: u64,
}

impl CollateralVault {
    pub const SPACE: usize = 8 + 208;
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
    #[msg("Invalid agent stake amount")]
    InvalidAgentStake,
    #[msg("Invalid agent score")]
    InvalidAgentScore,
    #[msg("Invalid agent tier transition")]
    InvalidAgentTier,
    #[msg("Agent is not active")]
    AgentNotActive,
    #[msg("Agent governance update cooldown is still active")]
    AgentGovernanceCooldownActive,
    #[msg("Agent score delta exceeds per-update limit")]
    AgentScoreDeltaTooLarge,
    #[msg("Agent is already deregistered")]
    AgentAlreadyDeregistered,
    #[msg("Agent is not deregistered")]
    AgentNotDeregistered,
    #[msg("Stake cooldown is still active")]
    StakeCooldownActive,
    #[msg("Agent slash cooldown is still active")]
    SlashCooldownActive,
    #[msg("Agent tier is below minimum rebalance submit tier")]
    AgentTierTooLow,
    #[msg("Agent signer does not match agent record")]
    AgentSignerMismatch,
    #[msg("Escrow has insufficient lamports")]
    EscrowInsufficientBalance,
    #[msg("Agent escrow owner does not match target agent")]
    InvalidEscrowOwner,
    #[msg("Invalid protocol treasury account")]
    InvalidTreasuryAccount,
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
    #[msg("CR target is out of allowed bounds")]
    InvalidCrTarget,
    #[msg("Fee rate is out of allowed bounds")]
    InvalidFeeRate,
    #[msg("Oracle input is stale")]
    OracleStale,
    #[msg("Oracle confidence too high")]
    ConfidenceTooHigh,
    #[msg("Observed slot is invalid")]
    InvalidObservedSlot,
    #[msg("Manual oracle mode is not active")]
    ManualOracleModeInactive,
    #[msg("Invalid manual oracle validity window")]
    InvalidManualOracleWindow,
    #[msg("Manual oracle mode is already active")]
    ManualOracleModeAlreadyActive,
    #[msg("Manual oracle mode re-enable cooldown is still active")]
    ManualOracleModeCooldownActive,
    #[msg("Manual oracle mode requires at least one active circuit breaker")]
    ManualOracleModeRequiresCircuitBreaker,
    #[msg("Manual oracle mode activations exceeded per-epoch cap")]
    ManualOracleActivationLimitExceeded,
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
    #[msg("Mint execution price exceeds user max bound")]
    MintPriceAboveUserLimit,
    #[msg("Mint volume exceeds per-transaction flow control limit")]
    MintTxFlowLimitExceeded,
    #[msg("Mint volume exceeds per-slot flow control limit")]
    MintSlotFlowLimitExceeded,
    #[msg("Redeem volume exceeds per-transaction flow control limit")]
    RedeemTxFlowLimitExceeded,
    #[msg("Redeem volume exceeds per-slot flow control limit")]
    RedeemSlotFlowLimitExceeded,
    #[msg("Redeem output is below user minimum bound")]
    RedeemOutputBelowUserLimit,
    #[msg("Mint paused for collateral under deep depeg stress")]
    DepegMintPaused,
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
    #[msg("Spot oracle price deviates too far from TWAP")]
    OracleTwapDeviationTooHigh,
    #[msg("Keeper quorum not met")]
    KeeperQuorumNotMet,
    #[msg("Keeper signer duplicated")]
    DuplicateKeeperSigner,
    #[msg("Invalid keeper set")]
    InvalidKeeperSet,
    #[msg("Keeper rotation timelock active")]
    KeeperRotationTimelockActive,
    #[msg("Invalid collateral mint binding")]
    InvalidCollateralMint,
    #[msg("Invalid collateral mint decimals")]
    InvalidCollateralDecimals,
    #[msg("Duplicate Pyth feed assignment")]
    DuplicatePythFeed,
    #[msg("Invalid token account")]
    InvalidTokenAccount,
    #[msg("Emergency shutdown is active")]
    EmergencyShutdownActive,
    #[msg("Unsafe to resume from shutdown")]
    UnsafeToResume,
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
    #[msg("Pyth feed is not configured for collateral")]
    PythFeedNotConfigured,
    #[msg("Pyth feed account does not match configured vault feed")]
    InvalidPythFeedAccount,
    #[msg("Pyth feed id does not match expected collateral feed")]
    InvalidPythFeedId,
    #[msg("Pyth update write authority is invalid")]
    InvalidPythWriteAuthority,
    #[msg("Pyth feed account owner is invalid")]
    InvalidPythFeedOwner,
    #[msg("Pyth account discriminator is invalid")]
    InvalidPythAccountDiscriminator,
    #[msg("Pyth price account data is invalid")]
    InvalidPythAccountData,
    #[msg("Pyth price must be positive")]
    PythPriceNonPositive,
    #[msg("Pyth verification level is insufficient")]
    PythVerificationLevelTooLow,
    #[msg("Pyth scaling overflow")]
    PythScaleOverflow,
    #[msg("Legacy state account is invalid")]
    InvalidLegacyAccount,
    #[msg("Legacy migration already completed")]
    MigrationAlreadyCompleted,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
enum RawPythVerificationLevel {
    Partial { num_signatures: u8 },
    Full,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
struct RawPythPriceFeedMessage {
    feed_id: [u8; 32],
    price: i64,
    conf: u64,
    exponent: i32,
    publish_time: i64,
    prev_publish_time: i64,
    ema_price: i64,
    ema_conf: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
struct RawPythPriceUpdateV2 {
    write_authority: Pubkey,
    verification_level: RawPythVerificationLevel,
    price_message: RawPythPriceFeedMessage,
    posted_slot: u64,
}

fn ensure_account_space<'info>(
    account: &AccountInfo<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    target_space: usize,
) -> Result<()> {
    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(target_space);
    if account.lamports() < required_lamports {
        let diff = required_lamports.saturating_sub(account.lamports());
        invoke(
            &system_instruction::transfer(&payer.key(), account.key, diff),
            &[
                payer.to_account_info(),
                account.clone(),
                system_program.to_account_info(),
            ],
        )?;
    }

    if account.data_len() < target_space {
        account.resize(target_space)?;
    }

    Ok(())
}

fn write_anchor_account<T: AnchorSerialize + Discriminator>(
    account: &AccountInfo,
    value: &T,
) -> Result<()> {
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(ErrorCode::InvalidLegacyAccount))?;
    require!(data.len() >= 8, ErrorCode::InvalidLegacyAccount);

    data[..8].copy_from_slice(&T::DISCRIMINATOR);
    let mut payload = &mut data[8..];
    value
        .serialize(&mut payload)
        .map_err(|_| error!(ErrorCode::InvalidLegacyAccount))?;
    Ok(())
}

fn build_vault_struct(
    index: u8,
    mint: Pubkey,
    protocol_authority: Pubkey,
    cap: u64,
    risk_score: u64,
    slot: u64,
    bump: u8,
) -> CollateralVault {
    CollateralVault {
        index,
        mint,
        vault: get_associated_token_address(&protocol_authority, &mint),
        oracle: Pubkey::default(),
        risk_score,
        weight_cap: cap,
        base_weight_cap: cap,
        price: SCALE,
        confidence: 1_000,
        last_oracle_slot: slot,
        total_deposits: 0,
        bump,
        pyth_price_feed: Pubkey::default(),
        twap_price: SCALE,
    }
}

fn migrate_vault_account(
    account: &AccountInfo,
    index: u8,
    mint: Pubkey,
    protocol_authority: Pubkey,
    cap: u64,
    risk_score: u64,
    slot: u64,
    program_id: &Pubkey,
) -> Result<()> {
    let seed_idx = [index];
    let (expected, bump) = Pubkey::find_program_address(
        &[b"collateral_vault".as_ref(), seed_idx.as_ref()],
        program_id,
    );
    require_keys_eq!(*account.key, expected, ErrorCode::InvalidLegacyAccount);
    require_keys_eq!(*account.owner, *program_id, ErrorCode::InvalidLegacyAccount);

    let vault = build_vault_struct(index, mint, protocol_authority, cap, risk_score, slot, bump);
    write_anchor_account(account, &vault)
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
    vault.pyth_price_feed = Pubkey::default();
    vault.twap_price = SCALE;
}

fn update_vault_oracle(
    vault: &mut Account<CollateralVault>,
    price: u64,
    confidence: u64,
    observed_slot: u64,
    last_twap_update_slot: u64,
) -> Result<u64> {
    require!(
        observed_slot > vault.last_oracle_slot,
        ErrorCode::OracleSlotRegression
    );

    let twap_reference_slot = last_twap_update_slot.max(vault.last_oracle_slot);
    require!(
        observed_slot > twap_reference_slot,
        ErrorCode::OracleSlotRegression
    );

    let slot_delta = observed_slot
        .checked_sub(twap_reference_slot)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?
        .max(1);
    let previous_twap = canonical_twap_price(vault.price, vault.twap_price);
    let alpha = twap_alpha_for_slot_delta(slot_delta)?;
    let next_twap = ewma_next_price(previous_twap, price, alpha)?;

    vault.price = price;
    vault.confidence = confidence;
    vault.last_oracle_slot = observed_slot;
    vault.twap_price = next_twap.max(1);
    Ok(observed_slot)
}

fn update_vault_oracle_from_pyth(
    vault: &mut Account<CollateralVault>,
    pyth_price_account: &UncheckedAccount,
    current_slot: u64,
    current_timestamp: i64,
    expected_feed_id: [u8; 32],
    allowed_authorities: &[Pubkey],
    last_twap_update_slot: u64,
) -> Result<u64> {
    require!(
        vault.pyth_price_feed != Pubkey::default(),
        ErrorCode::PythFeedNotConfigured
    );
    require_keys_eq!(
        pyth_price_account.key(),
        vault.pyth_price_feed,
        ErrorCode::InvalidPythFeedAccount
    );

    let (price, confidence, observed_slot, publish_time, feed_id) =
        read_pyth_price_update(pyth_price_account, allowed_authorities)?;

    // FIX PTV2-003: ensure feed-id matches configured collateral.
    require!(feed_id == expected_feed_id, ErrorCode::InvalidPythFeedId);

    // FIX PTV2-002: enforce publish_time freshness (<= 60s).
    require!(publish_time <= current_timestamp, ErrorCode::OracleStale);
    let age = current_timestamp - publish_time;
    require!(age <= PYTH_PUBLISH_TIME_MAX_AGE, ErrorCode::OracleStale);

    require!(
        price >= PRICE_MIN && price <= PRICE_MAX,
        ErrorCode::InvalidPrice
    );
    require!(
        confidence <= ORACLE_CONFIDENCE_MAX,
        ErrorCode::ConfidenceTooHigh
    );
    require!(
        observed_slot <= current_slot,
        ErrorCode::InvalidObservedSlot
    );

    let twap_price = canonical_twap_price(price, vault.twap_price);
    let staleness_limit = dynamic_oracle_staleness_limit(
        price,
        twap_price,
        ORACLE_STALENESS_MAX,
        HIGH_VOL_ORACLE_STALENESS_MAX,
    );
    require!(
        current_slot.saturating_sub(observed_slot) <= staleness_limit,
        ErrorCode::OracleStale
    );

    update_vault_oracle(
        vault,
        price,
        confidence,
        observed_slot,
        last_twap_update_slot,
    )
}

fn is_allowed_pyth_write_authority(
    write_authority: Pubkey,
    pyth_price_account: Pubkey,
    allowed_authorities: &[Pubkey],
) -> bool {
    write_authority == pyth_price_account
        || allowed_authorities.iter().any(|k| *k == write_authority)
}

fn read_pyth_price_update(
    pyth_price_account: &UncheckedAccount,
    allowed_authorities: &[Pubkey],
) -> Result<(u64, u64, u64, i64, [u8; 32])> {
    let info = pyth_price_account.to_account_info();
    require_keys_eq!(
        *info.owner,
        PYTH_RECEIVER_PROGRAM,
        ErrorCode::InvalidPythFeedOwner
    );

    let data = info
        .try_borrow_data()
        .map_err(|_| error!(ErrorCode::InvalidPythAccountData))?;
    require!(data.len() >= 8, ErrorCode::InvalidPythAccountData);

    let expected_discriminator = anchor_discriminator("account", PYTH_PRICE_UPDATE_ACCOUNT_NAME);
    require!(
        data[..8] == expected_discriminator,
        ErrorCode::InvalidPythAccountDiscriminator
    );

    let mut payload: &[u8] = &data[8..];
    let price_update = RawPythPriceUpdateV2::deserialize(&mut payload)
        .map_err(|_| error!(ErrorCode::InvalidPythAccountData))?;

    match price_update.verification_level {
        RawPythVerificationLevel::Full => {}
        _ => return err!(ErrorCode::PythVerificationLevelTooLow),
    }

    // FIX PTV2-004 / PTV3-023: enforce trusted write authority on Pyth updates.
    // Some devnet PriceUpdateV2 feeds set write_authority == price account itself,
    // so we accept either an allowlisted authority or self-authority.
    require!(
        is_allowed_pyth_write_authority(
            price_update.write_authority,
            pyth_price_account.key(),
            allowed_authorities,
        ),
        ErrorCode::InvalidPythWriteAuthority
    );

    require!(
        price_update.price_message.price > 0,
        ErrorCode::PythPriceNonPositive
    );

    let price = scale_signed_to_six_decimals(
        i128::from(price_update.price_message.price),
        price_update.price_message.exponent,
    )?;
    let confidence = scale_unsigned_to_six_decimals(
        u128::from(price_update.price_message.conf),
        price_update.price_message.exponent,
    )?;
    let feed_id = price_update.price_message.feed_id;
    let publish_time = price_update.price_message.publish_time;

    Ok((
        price,
        confidence,
        price_update.posted_slot,
        publish_time,
        feed_id,
    ))
}

fn scale_signed_to_six_decimals(value: i128, exponent: i32) -> Result<u64> {
    let unsigned = u128::try_from(value).map_err(|_| error!(ErrorCode::PythPriceNonPositive))?;
    scale_unsigned_to_six_decimals(unsigned, exponent)
}

fn scale_unsigned_to_six_decimals(value: u128, exponent: i32) -> Result<u64> {
    let shift = exponent
        .checked_add(6)
        .ok_or_else(|| error!(ErrorCode::PythScaleOverflow))?;

    let scaled = if shift >= 0 {
        let factor = pow10_u128(shift as u32)?;
        value
            .checked_mul(factor)
            .ok_or_else(|| error!(ErrorCode::PythScaleOverflow))?
    } else {
        let factor = pow10_u128((-shift) as u32)?;
        value
            .checked_add(factor.saturating_sub(1))
            .ok_or_else(|| error!(ErrorCode::PythScaleOverflow))?
            .checked_div(factor)
            .ok_or_else(|| error!(ErrorCode::PythScaleOverflow))?
    };

    u64::try_from(scaled).map_err(|_| error!(ErrorCode::PythScaleOverflow))
}

fn pow10_u128(exp: u32) -> Result<u128> {
    let mut acc = 1u128;
    for _ in 0..exp {
        acc = acc
            .checked_mul(10)
            .ok_or_else(|| error!(ErrorCode::PythScaleOverflow))?;
    }
    Ok(acc)
}

fn require_collateral_decimals(mint: &Account<TokenMint>, expected: u8) -> Result<()> {
    if mint.decimals != expected {
        return err!(ErrorCode::InvalidCollateralDecimals);
    }
    Ok(())
}

fn expected_pyth_feed_account(collateral_index: u8) -> Result<Pubkey> {
    let feed = match collateral_index {
        0 => PYTH_USDC_USD,
        1 => PYTH_USDT_USD,
        2 => PYTH_DAI_USD,
        3 => PYTH_USDS_USD,
        _ => return err!(ErrorCode::InvalidCollateralIndex),
    };
    Ok(feed)
}

fn expected_pyth_feed_id(collateral_index: u8) -> Result<[u8; 32]> {
    // FIX PTV2-003: expected feed-id binding per collateral.
    let feed_id = match collateral_index {
        0 => PYTH_FEED_ID_USDC,
        1 => PYTH_FEED_ID_USDT,
        2 => PYTH_FEED_ID_DAI,
        3 => PYTH_FEED_ID_USDS,
        _ => return err!(ErrorCode::InvalidCollateralIndex),
    };
    Ok(feed_id)
}

fn anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let preimage = format!("{namespace}:{name}");
    let digest = hash(preimage.as_bytes()).to_bytes();
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
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

fn require_keeper_quorum(
    protocol: &ProtocolState,
    signer_a: Pubkey,
    signer_b: Pubkey,
) -> Result<()> {
    require!(signer_a != signer_b, ErrorCode::DuplicateKeeperSigner);
    let ok_a = keeper_member(protocol, signer_a);
    let ok_b = keeper_member(protocol, signer_b);
    require!(ok_a && ok_b, ErrorCode::KeeperQuorumNotMet);
    Ok(())
}

fn apply_protocol_param_update(
    protocol: &mut ProtocolState,
    keeper_one: Pubkey,
    keeper_two: Pubkey,
    new_cr_target: u64,
    new_mint_fee: u64,
    new_redeem_fee: u64,
    slot: u64,
) -> Result<()> {
    require_keeper_quorum(protocol, keeper_one, keeper_two)?;
    require!(
        !protocol.emergency_shutdown,
        ErrorCode::EmergencyShutdownActive
    );
    require!(
        (CR_TARGET_MIN..=CR_TARGET_MAX).contains(&new_cr_target),
        ErrorCode::InvalidCrTarget
    );
    require!(new_mint_fee <= FEE_RATE_MAX, ErrorCode::InvalidFeeRate);
    require!(new_redeem_fee <= FEE_RATE_MAX, ErrorCode::InvalidFeeRate);

    protocol.cr_target = new_cr_target;
    protocol.mint_fee_rate = new_mint_fee;
    protocol.redeem_fee_rate = new_redeem_fee;
    // Legacy alias: keep fee_rate synchronized with mint_fee_rate for wire compatibility.
    protocol.fee_rate = new_mint_fee;
    protocol.last_update_slot = slot;
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

fn protocol_fee_amount(amount: u64, fee_rate: u64) -> Result<u64> {
    if fee_rate == 0 || amount == 0 {
        return Ok(0);
    }
    mul_div_ceil(amount, fee_rate, SCALE)
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
        sum = sum
            .checked_add(w)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))?;
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
    require!(
        protocol.max_mint_per_slot_ppm <= SCALE,
        ErrorCode::MintRateLimited
    );
    require!(
        protocol.max_redeem_per_slot_ppm <= SCALE,
        ErrorCode::MintRateLimited
    );
    for (i, v) in vaults.iter().enumerate() {
        require!(
            protocol.weights[i] <= v.weight_cap,
            ErrorCode::WeightCapExceeded
        );
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

fn canonical_twap_price(spot_price: u64, twap_price: u64) -> u64 {
    if twap_price == 0 {
        spot_price.max(1)
    } else {
        twap_price.max(1)
    }
}

fn fixed_point_pow_ppm(mut base_ppm: u64, mut exponent: u64) -> Result<u64> {
    let mut result = SCALE;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = mul_div_floor(result, base_ppm, SCALE)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base_ppm = mul_div_floor(base_ppm, base_ppm, SCALE)?;
        }
    }

    Ok(result)
}

fn twap_alpha_for_slot_delta(slot_delta: u64) -> Result<u64> {
    let per_slot_decay = SCALE.saturating_sub(TWAP_ALPHA_PPM);
    let decay = fixed_point_pow_ppm(per_slot_decay, slot_delta)?;
    Ok(SCALE.saturating_sub(decay).min(SCALE))
}

fn ewma_next_price(previous: u64, observed: u64, alpha_ppm: u64) -> Result<u64> {
    let history_weight = SCALE.saturating_sub(alpha_ppm.min(SCALE));
    mul_div_floor(previous, history_weight, SCALE)?
        .checked_add(mul_div_floor(observed, alpha_ppm.min(SCALE), SCALE)?)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))
}

fn price_deviation_ppm(spot_price: u64, twap_price: u64) -> u64 {
    let twap = canonical_twap_price(spot_price, twap_price);
    let diff = abs_diff(spot_price, twap) as u128;
    diff.saturating_mul(SCALE as u128)
        .checked_div(twap as u128)
        .unwrap_or(u128::MAX)
        .min(SCALE as u128) as u64
}

fn validate_spot_vs_twap(spot_price: u64, twap_price: u64) -> Result<()> {
    let deviation = price_deviation_ppm(spot_price, twap_price);
    require!(
        deviation <= TWAP_MAX_DEVIATION_PPM,
        ErrorCode::OracleTwapDeviationTooHigh
    );
    Ok(())
}

fn dynamic_oracle_staleness_limit(
    spot_price: u64,
    twap_price: u64,
    normal_limit: u64,
    high_vol_limit: u64,
) -> u64 {
    if price_deviation_ppm(spot_price, twap_price) >= HIGH_VOLATILITY_DEVIATION_PPM {
        high_vol_limit
    } else {
        normal_limit
    }
}

fn vault_oracle_degraded(
    vault: &CollateralVault,
    slot: u64,
    normal_staleness_limit: u64,
    high_vol_staleness_limit: u64,
) -> bool {
    let twap = canonical_twap_price(vault.price, vault.twap_price);
    let staleness_limit = dynamic_oracle_staleness_limit(
        vault.price,
        twap,
        normal_staleness_limit,
        high_vol_staleness_limit,
    );
    let deviation = price_deviation_ppm(vault.price, twap);

    vault.price == 0
        || vault.confidence > ORACLE_CONFIDENCE_MAX
        || deviation > TWAP_MAX_DEVIATION_PPM
        || slot.saturating_sub(vault.last_oracle_slot) > staleness_limit
}

fn count_degraded_vaults<'info>(
    vaults: [&Account<'info, CollateralVault>; 4],
    slot: u64,
    normal_staleness_limit: u64,
    high_vol_staleness_limit: u64,
) -> usize {
    vaults
        .iter()
        .filter(|v| {
            vault_oracle_degraded(
                v.as_ref(),
                slot,
                normal_staleness_limit,
                high_vol_staleness_limit,
            )
        })
        .count()
}

fn oracle_degraded<'info>(vaults: [&Account<'info, CollateralVault>; 4], slot: u64) -> bool {
    count_degraded_vaults(
        vaults,
        slot,
        ORACLE_STALENESS_MAX,
        HIGH_VOL_ORACLE_STALENESS_MAX,
    ) > 0
}

fn basket_max_depeg<'info>(vaults: [&Account<'info, CollateralVault>; 4]) -> u64 {
    vaults
        .iter()
        .fold(0u64, |acc, vault| acc.max(abs_diff(vault.price, SCALE)))
}

#[derive(Clone, Copy)]
enum SlotFlowKind {
    Mint,
    Redeem,
}

fn refresh_slot_flow_window(protocol: &mut ProtocolState, slot: u64) {
    if protocol.flow_control_slot != slot {
        protocol.flow_control_slot = slot;
        protocol.minted_in_flow_slot = 0;
        protocol.redeemed_in_flow_slot = 0;
    }
}

fn slot_flow_limit(total_supply: u64, limit_ppm: u64, flow_kind: SlotFlowKind) -> Result<u64> {
    let base = total_supply.max(SLOT_FLOW_LIMIT_MIN_UNITS);
    let by_ppm = mul_div_floor(base, limit_ppm, SCALE)?.max(SLOT_FLOW_LIMIT_MIN_UNITS);

    let capped = match flow_kind {
        SlotFlowKind::Mint => by_ppm,
        SlotFlowKind::Redeem => by_ppm
            .min(MAX_ABSOLUTE_REDEEM_PER_SLOT)
            .max(SLOT_FLOW_LIMIT_MIN_UNITS),
    };

    Ok(capped)
}

fn enforce_single_tx_flow_limit(
    total_supply: u64,
    amount: u64,
    limit_ppm: u64,
    minimum_units: u64,
    flow_kind: SlotFlowKind,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }

    let base = total_supply.max(minimum_units);
    let cap = mul_div_floor(base, limit_ppm, SCALE)?.max(minimum_units);
    match flow_kind {
        SlotFlowKind::Mint => require!(amount <= cap, ErrorCode::MintTxFlowLimitExceeded),
        SlotFlowKind::Redeem => require!(amount <= cap, ErrorCode::RedeemTxFlowLimitExceeded),
    }
    Ok(())
}

fn enforce_slot_flow_limit(
    protocol: &mut ProtocolState,
    slot: u64,
    amount: u64,
    flow_kind: SlotFlowKind,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }

    refresh_slot_flow_window(protocol, slot);

    let (counter, limit_ppm) = match flow_kind {
        SlotFlowKind::Mint => (
            &mut protocol.minted_in_flow_slot,
            protocol.max_mint_per_slot_ppm,
        ),
        SlotFlowKind::Redeem => (
            &mut protocol.redeemed_in_flow_slot,
            protocol.max_redeem_per_slot_ppm,
        ),
    };

    let cap = slot_flow_limit(protocol.total_supply, limit_ppm, flow_kind)?;
    let next = counter
        .checked_add(amount)
        .ok_or_else(|| error!(ErrorCode::MathOverflow))?;

    match flow_kind {
        SlotFlowKind::Mint => {
            require!(next <= cap, ErrorCode::MintSlotFlowLimitExceeded);
        }
        SlotFlowKind::Redeem => {
            require!(next <= cap, ErrorCode::RedeemSlotFlowLimitExceeded);
        }
    }

    *counter = next;
    Ok(())
}

fn mint_haircut_ppm(
    price: u64,
    confidence: u64,
    oracle_slot: u64,
    current_slot: u64,
) -> Result<u64> {
    let depeg = abs_diff(price, SCALE);
    require!(
        depeg < MINT_DEPEG_PAUSE_THRESHOLD,
        ErrorCode::DepegMintPaused
    );

    let staleness = current_slot.saturating_sub(oracle_slot);
    let stale_penalty = staleness.saturating_mul(STALE_ORACLE_PENALTY_PER_SLOT);
    let depeg_penalty = depeg.saturating_mul(2);

    let confidence_penalty = ((confidence as u128)
        .saturating_mul(CONFIDENCE_PENALTY_MULTIPLIER as u128)
        .saturating_mul(SCALE as u128)
        .checked_div(price.max(1) as u128)
        .unwrap_or(u128::MAX))
    .min(SCALE as u128) as u64;

    let total_penalty = stale_penalty
        .saturating_add(depeg_penalty)
        .saturating_add(confidence_penalty)
        .min(450_000);
    Ok(SCALE.saturating_sub(total_penalty))
}

fn redeem_discount_ppm<'info>(
    vaults: [&Account<'info, CollateralVault>; 4],
    slot: u64,
) -> Result<u64> {
    let mut worst_depeg = 0u64;
    let mut worst_staleness = 0u64;
    let mut confidence_penalty = 0u64;

    for v in vaults {
        worst_depeg = worst_depeg.max(abs_diff(v.price, SCALE));
        worst_staleness = worst_staleness.max(slot.saturating_sub(v.last_oracle_slot));

        let penalty = ((v.confidence as u128)
            .saturating_mul(CONFIDENCE_PENALTY_MULTIPLIER as u128)
            .saturating_mul(SCALE as u128)
            .checked_div(v.price.max(1) as u128)
            .unwrap_or(u128::MAX))
        .min(SCALE as u128) as u64;
        confidence_penalty = confidence_penalty.max(penalty);
    }

    let mut total_penalty = worst_depeg
        .saturating_mul(2)
        .saturating_add(worst_staleness.saturating_mul(STALE_ORACLE_PENALTY_PER_SLOT))
        .saturating_add(confidence_penalty)
        .min(250_000);

    if oracle_degraded(vaults, slot) {
        total_penalty = total_penalty.max(50_000);
    }

    Ok(SCALE.saturating_sub(total_penalty))
}

fn progressive_redeem_fee_rate(
    protocol: &ProtocolState,
    redeemed_in_flow_slot_before_tx: u64,
) -> Result<u64> {
    let redeem_cap = slot_flow_limit(
        protocol.total_supply,
        protocol.max_redeem_per_slot_ppm,
        SlotFlowKind::Redeem,
    )?;
    if redeem_cap == 0 {
        return Ok(protocol
            .redeem_fee_rate
            .min(MAX_PROGRESSIVE_REDEEM_FEE_RATE));
    }

    let velocity_ppm = mul_div_floor(
        redeemed_in_flow_slot_before_tx.min(redeem_cap),
        SCALE,
        redeem_cap,
    )?;

    let surcharge = if velocity_ppm <= REDEEM_VELOCITY_FEE_START_PPM {
        0
    } else {
        let numerator = velocity_ppm.saturating_sub(REDEEM_VELOCITY_FEE_START_PPM);
        let denominator = SCALE.saturating_sub(REDEEM_VELOCITY_FEE_START_PPM).max(1);
        mul_div_floor(numerator, MAX_PROGRESSIVE_REDEEM_FEE_RATE, denominator)?
    };

    Ok(protocol
        .redeem_fee_rate
        .saturating_add(surcharge)
        .min(MAX_PROGRESSIVE_REDEEM_FEE_RATE)
        .min(FEE_RATE_MAX))
}

fn mint_mstb_to_user<'info>(
    token_program: &Program<'info, Token>,
    mstb_mint: &Account<'info, TokenMint>,
    user_mstb_ata: &Account<'info, TokenAccount>,
    authority: &Account<'info, ProtocolState>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }

    let bump_seed = [authority.bump];
    let signer_seeds: &[&[u8]] = &[b"protocol_state", &bump_seed];

    token::mint_to(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            MintTo {
                mint: mstb_mint.to_account_info(),
                to: user_mstb_ata.to_account_info(),
                authority: authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

    Ok(())
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
        let adaptive_boost =
            ((overtime / 10).saturating_mul(50_000)).min(ADAPTIVE_RECOVERY_BPS_MAX);
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

fn validate_registration_stake(stake_amount: u64) -> Result<()> {
    require!(
        stake_amount >= AGENT_MIN_STAKE_LAMPORTS,
        ErrorCode::InvalidAgentStake
    );
    Ok(())
}

fn validate_agent_score(score: u64) -> Result<()> {
    require!(score <= SCALE, ErrorCode::InvalidAgentScore);
    Ok(())
}

fn build_agent_record(
    agent: Pubkey,
    role: AgentRole,
    stake: u64,
    now: i64,
    slot: u64,
    bump: u8,
) -> AgentRecord {
    AgentRecord {
        agent,
        stake,
        reputation: 0,
        role,
        tier: 0,
        status: AgentStatus::Active,
        proposals_submitted: 0,
        proposals_accepted: 0,
        registered_at: now,
        registered_slot: slot,
        last_active_at: now,
        agent_score: 0,
        last_slashed_slot: 0,
        bump,
    }
}

fn capped_slash_amount(current_stake: u64, requested_slash: u64) -> u64 {
    let max_slash = current_stake / 2;
    requested_slash.min(max_slash)
}

fn slash_cooldown_elapsed(last_slashed_slot: u64, current_slot: u64) -> bool {
    last_slashed_slot == 0
        || current_slot.saturating_sub(last_slashed_slot) >= AGENT_SLASH_COOLDOWN_SLOTS
}

fn can_claim_stake(status: AgentStatus, cooldown_started_at: i64, now: i64) -> Result<()> {
    require!(
        status == AgentStatus::Deregistered,
        ErrorCode::AgentNotDeregistered
    );
    require!(
        now >= cooldown_started_at.saturating_add(AGENT_STAKE_COOLDOWN_SECONDS),
        ErrorCode::StakeCooldownActive
    );
    Ok(())
}

fn min_score_for_tier(tier: u8) -> Result<u64> {
    let score = match tier {
        1 => AIG_TIER1_THRESHOLD,
        2 => AIG_TIER2_THRESHOLD,
        3 => AIG_TIER3_THRESHOLD,
        _ => return err!(ErrorCode::InvalidAgentTier),
    };
    Ok(score)
}

fn validate_tier_promotion(current_tier: u8, new_tier: u8, agent_score: u64) -> Result<()> {
    require!(new_tier <= 3, ErrorCode::InvalidAgentTier);
    require!(
        new_tier == current_tier.saturating_add(1),
        ErrorCode::InvalidAgentTier
    );
    let required_score = min_score_for_tier(new_tier)?;
    require!(agent_score >= required_score, ErrorCode::InvalidAgentScore);
    Ok(())
}

fn validate_tier_demotion(current_tier: u8, new_tier: u8) -> Result<()> {
    require!(new_tier <= 3, ErrorCode::InvalidAgentTier);
    require!(
        current_tier == new_tier.saturating_add(1),
        ErrorCode::InvalidAgentTier
    );
    Ok(())
}

fn assert_agent_commit_eligibility(record: &AgentRecord, submitting_agent: Pubkey) -> Result<()> {
    require_keys_eq!(
        record.agent,
        submitting_agent,
        ErrorCode::AgentSignerMismatch
    );
    require!(
        record.status == AgentStatus::Active,
        ErrorCode::AgentNotActive
    );
    require!(
        record.tier >= AIG_MIN_COMMIT_TIER,
        ErrorCode::AgentTierTooLow
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_err_contains<T: core::fmt::Debug>(res: Result<T>, needle: &str) {
        let err = res.expect_err("expected failure");
        assert!(
            format!("{err:?}").contains(needle),
            "expected error containing `{needle}`, got `{err:?}`"
        );
    }

    fn sample_protocol(keepers: [Pubkey; 3]) -> ProtocolState {
        ProtocolState {
            weights: [400_000, 300_000, 200_000, 100_000],
            fee_rate: 2_000,
            mint_fee_rate: 2_000,
            redeem_fee_rate: 2_000,
            cr_target: 1_200_000,
            total_supply: 0,
            last_update_slot: 0,
            keeper_set: keepers,
            emergency_shutdown: false,
            pending_rebalance_commit: [0u8; 32],
            pending_rebalance_slot: 0,
            pending_rebalance_expiry: 0,
            pending_keeper_set: [Pubkey::default(); 3],
            pending_keeper_activation_slot: 0,
            flow_control_slot: 0,
            minted_in_flow_slot: 0,
            redeemed_in_flow_slot: 0,
            last_twap_update_slots: [0; 4],
            max_mint_per_slot_ppm: DEFAULT_MAX_MINT_PER_SLOT_PPM,
            max_redeem_per_slot_ppm: DEFAULT_MAX_REDEEM_PER_SLOT_PPM,
            manual_oracle_mode_expiry_slot: 0,
            bump: 0,
            manual_oracle_reenable_delay_slots: MANUAL_ORACLE_MODE_REENABLE_COOLDOWN_SLOTS,
            manual_oracle_last_activation_slot: 0,
            manual_oracle_activation_epoch: 0,
            manual_oracle_activation_count_epoch: 0,
        }
    }

    fn sample_circuit() -> CircuitBreakerState {
        CircuitBreakerState {
            status: [BreakerStatus::Inactive as u8; 4],
            activation_tick: [0; 4],
            trigger_count: [0; 4],
            cooldown_until: [0; 4],
            last_trigger_tick: [0; 4],
            recent_trigger_count: [0; 4],
            recovery_tick: [0; 4],
            cb1_collateral_index: 0,
            mint_rate_limit: SCALE,
            optimizer_enabled: true,
            learning_rate_scale: SCALE,
            max_activation_duration: MAX_ACTIVATION_DURATION,
            bump: 0,
        }
    }

    #[test]
    fn registration_valid_stake_and_all_roles() {
        let agent = Pubkey::new_unique();
        for role in [
            AgentRole::Optimizer,
            AgentRole::Monitor,
            AgentRole::Auditor,
            AgentRole::Liquidator,
        ] {
            validate_registration_stake(AGENT_MIN_STAKE_LAMPORTS).unwrap();
            let record = build_agent_record(agent, role, 10, 123, 456, 1);
            assert_eq!(record.agent, agent);
            assert_eq!(record.role, role);
            assert_eq!(record.stake, 10);
            assert_eq!(record.tier, 0);
            assert_eq!(record.status, AgentStatus::Active);
            assert_eq!(record.registered_slot, 456);
            assert_eq!(record.last_slashed_slot, 0);
        }
    }

    #[test]
    fn registration_zero_stake_rejected() {
        assert_err_contains(validate_registration_stake(0), "InvalidAgentStake");
    }

    #[test]
    fn registration_below_minimum_stake_rejected() {
        assert_err_contains(
            validate_registration_stake(AGENT_MIN_STAKE_LAMPORTS - 1),
            "InvalidAgentStake",
        );
        validate_registration_stake(AGENT_MIN_STAKE_LAMPORTS).unwrap();
    }

    #[test]
    fn registration_pda_is_deterministic_preventing_double_register() {
        let agent = Pubkey::new_unique();
        let (pda_a, _) = Pubkey::find_program_address(&[b"agent", agent.as_ref()], &crate::id());
        let (pda_b, _) = Pubkey::find_program_address(&[b"agent", agent.as_ref()], &crate::id());
        assert_eq!(pda_a, pda_b);
    }

    #[test]
    fn score_and_tier_progression_checks() {
        validate_agent_score(750_000).unwrap();
        assert_err_contains(validate_agent_score(SCALE + 1), "InvalidAgentScore");

        validate_tier_promotion(0, 1, 600_000).unwrap();
        validate_tier_promotion(1, 2, 750_000).unwrap();
        validate_tier_promotion(2, 3, 850_000).unwrap();
        assert_err_contains(validate_tier_promotion(3, 4, 900_000), "InvalidAgentTier");

        validate_tier_demotion(3, 2).unwrap();
        validate_tier_demotion(2, 1).unwrap();
        validate_tier_demotion(1, 0).unwrap();
        assert_err_contains(validate_tier_demotion(3, 1), "InvalidAgentTier");
    }

    #[test]
    fn keeper_authorization_for_agent_updates() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let protocol = sample_protocol([k1, k2, k3]);

        require_keeper_quorum(&protocol, k1, k2).unwrap();
        assert_err_contains(
            require_keeper_quorum(&protocol, k1, k1),
            "DuplicateKeeperSigner",
        );
        assert_err_contains(
            require_keeper_quorum(&protocol, k1, Pubkey::new_unique()),
            "KeeperQuorumNotMet",
        );
    }

    #[test]
    fn slash_capped_at_half_stake() {
        assert_eq!(capped_slash_amount(1_000, 300), 300);
        assert_eq!(capped_slash_amount(1_000, 9_999), 500);
    }

    #[test]
    fn slash_cooldown_enforced_by_slot_gap() {
        assert!(slash_cooldown_elapsed(0, 1));
        assert!(!slash_cooldown_elapsed(1_000, 1_050));
        assert!(slash_cooldown_elapsed(1_000, 1_100));
    }

    #[test]
    fn protocol_fee_amount_zero_rate_preserves_backward_compat_no_fee() {
        assert_eq!(protocol_fee_amount(1_000_000, 0).unwrap(), 0);
        assert_eq!(protocol_fee_amount(1_000_000, 2_000).unwrap(), 2_000);
    }

    #[test]
    fn deregister_and_claim_cooldown() {
        let now = 1_000_i64;
        assert_err_contains(
            can_claim_stake(AgentStatus::Active, now - AGENT_STAKE_COOLDOWN_SECONDS, now),
            "AgentNotDeregistered",
        );
        assert_err_contains(
            can_claim_stake(
                AgentStatus::Deregistered,
                now - AGENT_STAKE_COOLDOWN_SECONDS + 1,
                now,
            ),
            "StakeCooldownActive",
        );
        can_claim_stake(
            AgentStatus::Deregistered,
            now - AGENT_STAKE_COOLDOWN_SECONDS,
            now,
        )
        .unwrap();
    }

    #[test]
    fn commit_rebalance_requires_registered_tier2_agent() {
        let agent = Pubkey::new_unique();
        let wrong_signer = Pubkey::new_unique();

        let mut record = build_agent_record(agent, AgentRole::Optimizer, 10, 0, 0, 1);
        record.tier = 2;
        assert_err_contains(
            assert_agent_commit_eligibility(&record, wrong_signer),
            "AgentSignerMismatch",
        );

        record.status = AgentStatus::Deregistered;
        assert_err_contains(
            assert_agent_commit_eligibility(&record, agent),
            "AgentNotActive",
        );

        record.status = AgentStatus::Active;
        record.tier = 1;
        assert_err_contains(
            assert_agent_commit_eligibility(&record, agent),
            "AgentTierTooLow",
        );

        record.tier = 2;
        assert_agent_commit_eligibility(&record, agent).unwrap();
    }

    #[test]
    fn tc_prog_001_accepts_pyth_account_self_write_authority() {
        let pyth_account = Pubkey::new_from_array([42u8; 32]);
        let trusted = Pubkey::new_from_array([7u8; 32]);

        assert!(is_allowed_pyth_write_authority(
            pyth_account,
            pyth_account,
            &[trusted]
        ));
    }

    #[test]
    fn tc_prog_001_rejects_unknown_write_authority() {
        let pyth_account = Pubkey::new_from_array([42u8; 32]);
        let trusted = Pubkey::new_from_array([7u8; 32]);
        let unknown = Pubkey::new_from_array([99u8; 32]);

        assert!(!is_allowed_pyth_write_authority(
            unknown,
            pyth_account,
            &[trusted]
        ));
    }

    #[test]
    fn tc_update_protocol_params_1_valid_values_succeeds() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        apply_protocol_param_update(&mut protocol, k1, k2, 1_200_000, 1_000, 1_000, 777).unwrap();
    }

    #[test]
    fn tc_update_protocol_params_2_cr_below_min_fails() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, k2, 999_999, 1_000, 1_000, 777),
            "InvalidCrTarget",
        );
    }

    #[test]
    fn tc_update_protocol_params_3_cr_above_max_fails() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, k2, 2_000_001, 1_000, 1_000, 777),
            "InvalidCrTarget",
        );
    }

    #[test]
    fn tc_update_protocol_params_4_mint_fee_above_max_fails() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, k2, 1_200_000, 10_001, 1_000, 777),
            "InvalidFeeRate",
        );
    }

    #[test]
    fn tc_update_protocol_params_5_redeem_fee_above_max_fails() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, k2, 1_200_000, 1_000, 10_001, 777),
            "InvalidFeeRate",
        );
    }

    #[test]
    fn tc_update_protocol_params_6_emergency_shutdown_fails() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);
        protocol.emergency_shutdown = true;

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, k2, 1_200_000, 1_000, 1_000, 777),
            "EmergencyShutdownActive",
        );
    }

    #[test]
    fn tc_update_protocol_params_7_keeper_quorum_required() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let outsider = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, outsider, 1_200_000, 1_000, 1_000, 777),
            "KeeperQuorumNotMet",
        );
    }

    #[test]
    fn tc_update_protocol_params_8_updates_all_fields() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        apply_protocol_param_update(&mut protocol, k1, k2, 1_500_000, 900, 700, 999).unwrap();

        assert_eq!(protocol.cr_target, 1_500_000);
        assert_eq!(protocol.mint_fee_rate, 900);
        assert_eq!(protocol.redeem_fee_rate, 700);
        assert_eq!(protocol.fee_rate, 900);
        assert_eq!(protocol.last_update_slot, 999);
    }

    #[test]
    fn tc_update_protocol_params_9_duplicate_signers_rejected() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        assert_err_contains(
            apply_protocol_param_update(&mut protocol, k1, k1, 1_200_000, 1_000, 1_000, 777),
            "DuplicateKeeperSigner",
        );
    }

    #[test]
    fn protocol_state_pda_is_deterministic() {
        let (a, bump_a) = Pubkey::find_program_address(&[b"protocol_state"], &crate::id());
        let (b, bump_b) = Pubkey::find_program_address(&[b"protocol_state"], &crate::id());
        assert_eq!(a, b);
        assert_eq!(bump_a, bump_b);
        assert_ne!(a, Pubkey::default());
    }

    #[test]
    fn keeper_set_validation_rejects_duplicates_and_default() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();

        validate_keeper_set(&[a, b, Pubkey::default()]).expect_err("default key must fail");
        validate_keeper_set(&[a, b, a]).expect_err("duplicate key must fail");
        validate_keeper_set(&[a, b, Pubkey::new_unique()]).expect("valid unique keeper set");
    }

    #[test]
    fn keeper_quorum_requires_two_distinct_members_and_supports_rotation() {
        let k1 = Pubkey::new_unique();
        let k2 = Pubkey::new_unique();
        let k3 = Pubkey::new_unique();
        let mut protocol = sample_protocol([k1, k2, k3]);

        require_keeper_quorum(&protocol, k1, k2).expect("2-of-3 quorum should pass");
        assert_err_contains(
            require_keeper_quorum(&protocol, k1, k1),
            "DuplicateKeeperSigner",
        );
        assert_err_contains(
            require_keeper_quorum(&protocol, k1, Pubkey::new_unique()),
            "KeeperQuorumNotMet",
        );

        let n1 = Pubkey::new_unique();
        let n2 = Pubkey::new_unique();
        let n3 = Pubkey::new_unique();
        protocol.keeper_set = [n1, n2, n3];

        assert_err_contains(
            require_keeper_quorum(&protocol, k1, k2),
            "KeeperQuorumNotMet",
        );
        require_keeper_quorum(&protocol, n1, n2).expect("rotated quorum should pass");
    }

    #[test]
    fn expected_pyth_feed_mappings_cover_all_four_vaults() {
        assert_eq!(expected_pyth_feed_account(0).unwrap(), PYTH_USDC_USD);
        assert_eq!(expected_pyth_feed_account(1).unwrap(), PYTH_USDT_USD);
        assert_eq!(expected_pyth_feed_account(2).unwrap(), PYTH_DAI_USD);
        assert_eq!(expected_pyth_feed_account(3).unwrap(), PYTH_USDS_USD);
        assert_err_contains(expected_pyth_feed_account(4), "InvalidCollateralIndex");

        assert_eq!(expected_pyth_feed_id(0).unwrap(), PYTH_FEED_ID_USDC);
        assert_eq!(expected_pyth_feed_id(1).unwrap(), PYTH_FEED_ID_USDT);
        assert_eq!(expected_pyth_feed_id(2).unwrap(), PYTH_FEED_ID_DAI);
        assert_eq!(expected_pyth_feed_id(3).unwrap(), PYTH_FEED_ID_USDS);
        assert_err_contains(expected_pyth_feed_id(4), "InvalidCollateralIndex");
    }

    #[test]
    fn circuit_breaker_recovery_and_resume_path_restores_inactive_state() {
        let mut circuit = sample_circuit();
        circuit.status[0] = BreakerStatus::Holding as u8;
        circuit.activation_tick[0] = 10;

        refresh_circuit_breakers(&mut circuit, 15);
        assert_eq!(circuit.status[0], BreakerStatus::Active as u8);

        circuit.max_activation_duration = 4;
        refresh_circuit_breakers(&mut circuit, 20);
        assert_eq!(circuit.status[0], BreakerStatus::Recovery as u8);

        let cooldown_end = circuit.cooldown_until[0];
        refresh_circuit_breakers(&mut circuit, cooldown_end);
        assert_eq!(circuit.status[0], BreakerStatus::Inactive as u8);
    }

    #[test]
    fn cb4_recovery_restores_learning_rate_before_inactive_transition() {
        let mut circuit = sample_circuit();
        circuit.status[3] = BreakerStatus::Recovery as u8;
        circuit.recovery_tick[3] = 0;
        circuit.cooldown_until[3] = 8;
        circuit.learning_rate_scale = 500_000;

        refresh_circuit_breakers(&mut circuit, 10);

        assert_eq!(circuit.learning_rate_scale, SCALE);
        assert_eq!(circuit.status[3], BreakerStatus::Inactive as u8);
    }
}
