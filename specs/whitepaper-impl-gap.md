# Whitepaper → Implementation Gap: Execution Plan

## Status: ACTIVE
## Priority: CRITICAL — whitepaper claims must match code

---

## Gap Matrix

### G1. Loss Function ℒ_t (§4.3) — **NOT IMPLEMENTED**
**Whitepaper claims:**
```
ℒ_t = λ_p(p_t - 1)² + λ_cr max(0, CR_min - CR_t)² + λ_vol Var(ΔNAV)
     + λ_turn ‖w_t - w_{t-1}‖₁ + λ_conc Σ w²_{i,t} + λ_orc(1 - q_t)²
```
6-term composite loss: peg deviation, CR shortfall, NAV volatility, turnover penalty, concentration (HHI), oracle quality.

**Current code:** No loss function exists anywhere.

**Implementation target:** `solana/keeper/src/optimizer.rs` (new module)
- Compute all 6 terms from on-chain state
- Lambda weights configurable via `config.toml`
- Returns scalar loss + per-term gradient vector

### G2. Gradient/Adam Optimizer (§4.4) — **NOT IMPLEMENTED**
**Whitepaper claims:**
```
θ_{t+1} = Π_Ω(θ_t - α_t ∇_θ ℒ_t)
```
Gradient/Adam-like updates with gradient clipping, bounded delta checks, simplex + cap projection, safety-gate acceptance.

**Current code:** `compute_target_weights()` is a static closed-form formula. `learning_rate_scale` is a 100%/50% toggle. No gradient computation exists.

**Implementation target:** `solana/keeper/src/optimizer.rs`
- Adam optimizer state (m, v, t) persisted across keeper cycles
- Gradient clipping (configurable max norm)
- Per-epoch bounded delta enforcement
- Safety set projection Π_Ω: simplex constraint, per-asset caps/floors, fee bounds, CR bounds
- Learning rate scheduler (warm-up + decay, not a binary toggle)

### G3. Parameter Vector θ_t (§4.2) — **PARTIALLY IMPLEMENTED**
**Whitepaper claims:**
```
θ_t = [targetCR_t, mintFee_t, redeemFee_t, w_t, ...]
```
Optimizable parameter vector including CR target, fees, weights, etc.

**Current code:** Only weights are computed (static formula). `targetCR`, `mintFee`, `redeemFee` exist on-chain but are never optimized by the keeper.

**Implementation target:**
- Keeper optimizer outputs full θ vector (not just weights)
- On-chain `commit_rebalance`/`rebalance` extended to accept θ updates (or new instruction)
- Bounded acceptance: on-chain validates each parameter delta against max-per-epoch bounds

### G4. CB-4 Numerical Rollback (§4.5) — **NOT IMPLEMENTED**
**Whitepaper claims:** Checkpoint rollback on non-finite/unsafe optimizer state.

**Current code:** CB has 4 status flags but no numerical rollback logic. No optimizer state to roll back.

**Implementation target:** 
- Keeper persists optimizer checkpoints (pre-update θ + Adam state)
- On NaN/Inf/safety violation → rollback to last good checkpoint
- CB-4 activation logged + reported to monitor

### G5. Open Agent Economy — **DOCS ONLY**
**Whitepaper claims (§7):** Permissionless agent registration, Agent Registry PDA, role specialization, ACP protocol, optimization tournaments.

**Current code:** Zero on-chain or off-chain implementation. Only specs/docs exist.

**Implementation target (phased):**
- Phase 1: On-chain Agent Registry (PDA per agent: stake, reputation, status)
- Phase 2: Registration/deregistration instructions + stake escrow
- Phase 3: Proposal submission flow (commit/reveal with agent attribution)
- Phase 4: Tournament scoring + reward distribution

### G6. Agent Intelligence Gate — **DOCS ONLY**
**Whitepaper claims (§8):** Tier 0→3 progression, AgentScore model, runtime demotion.

**Current code:** Zero implementation. Only specs/docs exist.

**Implementation target (phased):**
- Phase 1: Off-chain AIG challenge runner (Tier 0 exam on historical data)
- Phase 2: Sandbox trial infrastructure (100-epoch simulated run)
- Phase 3: On-chain tier tracking in Agent Registry
- Phase 4: Runtime demotion hooks in keeper/monitor

---

## Execution Priority

1. **G1 + G2 (Loss + Optimizer)** — Core claim of the whitepaper. MUST implement first.
2. **G3 (Full θ vector)** — Extends G2 to all optimizable parameters.
3. **G4 (CB-4 Rollback)** — Safety net for optimizer. Required before any optimizer goes live.
4. **G5 (OAE)** — Major feature, phased.
5. **G6 (AIG)** — Depends on G5 Agent Registry.

## Implementation Approach

- All optimizer logic lives in the **keeper** (off-chain). On-chain only validates bounds.
- New keeper module: `optimizer.rs` containing:
  - `LossFunction` struct with configurable lambdas
  - `AdamOptimizer` struct with state persistence
  - `SafetyProjection` (Π_Ω implementation)
  - `OptimizerCheckpoint` for CB-4 rollback
- On-chain changes minimal: extend `commit_rebalance` to include fee/CR parameter proposals (or add `commit_param_update` instruction).
- OAE/AIG require new on-chain programs or instructions — Phase 2+.

## Test Requirements (TDD)

Each gap requires test cases BEFORE implementation:
- G1: Loss computation correctness for all 6 terms + edge cases (zero division, NaN inputs)
- G2: Adam convergence on known toy problem + gradient clipping + projection correctness
- G3: Full θ round-trip (keeper propose → on-chain validate → apply)
- G4: Rollback triggers on NaN/Inf/out-of-bounds + checkpoint restore correctness
- G5: Agent registration lifecycle + stake accounting
- G6: Tier progression + demotion + score computation
