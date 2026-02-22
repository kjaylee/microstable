# Whitepaper → Implementation Gap: Execution Plan

## Status: COMPLETED
## Priority: CRITICAL — whitepaper claims must match code

---

## Gap Matrix

### G1. Loss Function ℒ_t (§4.3) — **COMPLETED** (`c358a73`)
**Whitepaper claims:**
```
ℒ_t = λ_p(p_t - 1)² + λ_cr max(0, CR_min - CR_t)² + λ_vol Var(ΔNAV)
     + λ_turn ‖w_t - w_{t-1}‖₁ + λ_conc Σ w²_{i,t} + λ_orc(1 - q_t)²
```
6-term composite loss: peg deviation, CR shortfall, NAV volatility, turnover penalty, concentration (HHI), oracle quality.

**Implementation:**
- `solana/keeper/src/optimizer.rs` (`LossFunction::compute`, `LossTerms`, `LossGradients`)

**Tests:**
- `solana/keeper/src/optimizer_tests.rs` (12 tests)
- `solana/keeper/src/optimizer.rs` (2 tests)

### G2. Gradient/Adam Optimizer (§4.4) — **COMPLETED** (`c358a73`)
**Whitepaper claims:**
```
θ_{t+1} = Π_Ω(θ_t - α_t ∇_θ ℒ_t)
```
Gradient/Adam-like updates with gradient clipping, bounded delta checks, simplex + cap projection, safety-gate acceptance.

**Implementation:**
- `solana/keeper/src/optimizer.rs` (`AdamOptimizer`, `project_to_safety_set`, `validate_safety_set`, `optimize_step`)
- Integrated via `solana/keeper/src/rebalance.rs::compute_target_weights`

**Tests:**
- `solana/keeper/src/optimizer_tests.rs` (12 tests)
- `solana/keeper/src/rebalance.rs` (6 tests)

### G3. Parameter Vector θ_t (§4.2) — **COMPLETED** (`c358a73`, `b68448c`)
**Whitepaper claims:**
```
θ_t = [targetCR_t, mintFee_t, redeemFee_t, w_t, ...]
```
Optimizable parameter vector including CR target, fees, weights, etc.

**Implementation:**
- `solana/keeper/src/optimizer.rs` (`ParamVector` includes CR + fees + weights)
- `solana/keeper/src/rebalance.rs` (propagates CR + fees via `ProtocolParamUpdate`)
- `solana/programs/microstable/src/lib.rs` (`update_protocol_params`)
- `solana/keeper/src/wire.rs` (`ix_update_protocol_params`)

**Tests:**
- `solana/keeper/src/rebalance.rs` (6 tests)
- `solana/tests/microstable.ts` (7 Anchor tests)

### G4. CB-4 Numerical Rollback (§4.5) — **COMPLETED** (`c358a73`)
**Whitepaper claims:** Checkpoint rollback on non-finite/unsafe optimizer state.

**Implementation:**
- `solana/keeper/src/optimizer.rs` (`OptimizerCheckpoint`, rollback-on-error in `optimize_step`)
- Checkpoint persisted to `.state/microstable/optimizer_checkpoint.json`

**Tests:**
- `solana/keeper/src/optimizer_tests.rs` (12 tests)

### G5. Open Agent Economy (§7) — **COMPLETED** (`fc9db6d`)
**Whitepaper claims:** Permissionless agent registration, Agent Registry PDA, role specialization, ACP protocol, optimization tournaments.

**Implementation:**
- On-chain Agent Registry + lifecycle ops: `solana/programs/microstable/src/lib.rs`
  (`register_agent`, `deregister_agent`, `update_agent_score`, `promote_agent`, `demote_agent`, `slash_agent`, `claim_stake`)
- Tournament scoring + proposals: `solana/keeper/src/tournament.rs`
- Keeper wiring: `solana/keeper/src/agent_loop.rs`, `solana/keeper/src/main.rs`

**Tests:**
- `solana/keeper/src/tournament_tests.rs` (12 tests)
- `solana/keeper/src/agent_loop_tests.rs` (6 tests)

### G6. Agent Intelligence Gate (§8) — **COMPLETED** (`15177b7`, `a74d55c`)
**Whitepaper claims:** Tier 0→3 progression, AgentScore model, runtime demotion.

**Implementation:**
- Off-chain AIG challenge runner: `solana/keeper/src/aig.rs`
- Keeper scheduling: `solana/keeper/src/agent_loop.rs`, `solana/keeper/src/main.rs`
- Tier data model: `solana/programs/microstable/src/lib.rs` (`AgentRecord.tier`)

**Tests:**
- `solana/keeper/src/aig_tests.rs` (10 tests)
- `solana/keeper/src/agent_loop_tests.rs` (6 tests)

---

## Integration Gaps (Keeper ↔ On-chain Wiring)

### I1. Optimizer wired into rebalance loop — **COMPLETED** (`c358a73`)
- `solana/keeper/src/rebalance.rs` uses `optimizer::optimize_step` and `f64_weights_to_ppm`.

### I2. Protocol parameter updates on-chain — **COMPLETED** (`b68448c`)
- `update_protocol_params` instruction + keeper integration in `rebalance.rs`.

### I3. AIG cycle scheduled in keeper main loop — **COMPLETED** (`a74d55c`)
- `agent_loop::maybe_run_aig_cycle` invoked from `main.rs`.

### I4. Tournament cycle scheduled in keeper main loop — **COMPLETED** (`a74d55c`)
- `agent_loop::maybe_run_tournament_cycle` invoked from `main.rs`.

---

## Summary

All whitepaper claims now have corresponding implementations in the on-chain program and keeper runtime.

### Minor gaps / future improvement opportunities
- Wire tournament outcomes to on-chain score updates (`ix_update_agent_score`) and tier promotions/demotions.
- Persist AIG/tournament outcomes for auditability (structured storage + dashboards).
- Expand ACP/MCP tooling to cover the full on-chain instruction surface with rate limits + replay protection.
- Feed risk-manager outputs into parameter update cadence (automated mitigation loops).
