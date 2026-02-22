# microstable: A Self-Evolving, Agent-Native Multi-Collateral Stablecoin Protocol

**Version**: v0.3 (Implementation Sync)  
**Status**: Implementation-aligned whitepaper (Educational / Hobby Project)  
**Runtime Architecture**: Solana on-chain program + off-chain Rust keeper daemon  
**Program ID (Devnet)**: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`

> "Keep the protocol small and inspectable. Make adaptation bounded and auditable."

---

## 1. Abstract

Stablecoins remain essential digital settlement primitives, but they still fail in predictable ways: concentrated collateral exposure, delayed governance reactions, oracle fragility, and static policy parameters that do not adapt to regime shifts. **microstable** proposes a bounded, inspectable alternative: a multi-collateral stablecoin protocol that continuously re-optimizes selected parameters while preserving hard safety invariants.

Version v0.3 syncs the whitepaper with the implemented codebase. It includes an **Open Agent Economy (OAE)** with on-chain Agent Registry and lifecycle controls (`solana/programs/microstable/src/lib.rs`), competitive tournament evaluation (`solana/keeper/src/tournament.rs`), and keeper wiring (`solana/keeper/src/agent_loop.rs`). It also includes an **Agent Intelligence Gate (AIG)** with an off-chain challenge runner (`solana/keeper/src/aig.rs`) wired into the keeper loop. The release reflects Solana devnet deployment, Pyth integration, SPL-token E2E flow validation, and continuous red/purple/crimson adversarial campaigns.

The central claim remains unchanged: optimization should be a servant of solvency. microstable keeps settlement deterministic on-chain, pushes heavy optimization off-chain, and enforces strict circuit breakers, bounded deltas, and rollback-safe state transitions.

## 2. Introduction

Bitcoin demonstrated that monetary policy can be protocolized. Modern AI systems demonstrated that compact, explicit code can still produce adaptive behavior. microstable combines these two ideas for stablecoin design.

Instead of discretionary, episodic parameter tuning, microstable uses explicit objectives and bounded updates. Unlike fully reflexive algorithmic systems, it does not allow unconstrained expansion loops.

### 2.1 Motivation: Why now

Stablecoins are no longer only DeFi infrastructure; they are becoming settlement rails for **agent-driven transactions** (autonomous market-making, machine-to-machine payments, automated treasury operations). Agent economies require:

- deterministic and machine-verifiable rules,
- low-latency parameter adaptation,
- explicit safety constraints under stress,
- permissionless but accountable participation.

### 2.2 Thesis

Let $\theta_t$ denote protocol parameters and $\mathcal{L}_t$ a risk-aware objective. microstable updates only within strict bounds:

$$
\theta_{t+1} = \Pi_{\Omega}\left(\theta_t - \alpha_t \nabla_{\theta}\mathcal{L}_t\right),
$$

where $\Pi_{\Omega}$ projects to a feasible safety set $\Omega$ (caps, floors, simplex constraints, fee limits, circuit state constraints).

The objective is not unconstrained growth; it is robust peg quality and solvency under adversarial conditions.

## 3. Background

### 3.1 Lessons from prior failures

The UST/Luna collapse highlighted three recurring failure channels:

1. reflexive mint/burn feedback under confidence shock,  
2. liquidity evaporation and slippage amplification,  
3. absent or weak hard stops during fast depeg dynamics.

microstable treats these as control-system failures, not marketing failures.

### 3.2 Existing approaches and gap

- **DAI-like systems**: strong collateral discipline, slower governance adaptation.
- **FRAX-like historical hybrids**: flexible but confidence-sensitive.
- **mStable-style baskets**: diversification and routing efficiency.

microstable keeps basket diversification but adds agent-native optimization and formalized safety rails.

### 3.3 Agent-native protocol context

Agent-native protocols need deterministic interfaces (for automation) plus anti-sybil economics (for adversarial environments). v0.3 therefore expands from “adaptive stablecoin” to “adaptive stablecoin with accountable machine participants.”

## 4. System Design

### 4.1 Collateral basket and constraints

For collateral set $\mathcal{C}=\{c_1,\dots,c_n\}$ with weights $w_{i,t}$:

$$
\sum_i w_{i,t}=1,\quad w_{i,t}\ge 0,
$$

with per-asset upper bounds $w_{i,t}\le w_i^{\max}$ and risk haircuts in valuation.

### 4.2 Parameter vector

$$
\theta_t=[\text{targetCR}_t,\text{mintFee}_t,\text{redeemFee}_t,\mathbf{w}_t,\dots]
$$

Updates are clipped by per-epoch movement caps (e.g., bounded weight and fee deltas).

**Implementation reference:** `solana/keeper/src/optimizer.rs` (`ParamVector`), `solana/keeper/src/rebalance.rs` (parameter propagation), `solana/programs/microstable/src/lib.rs` (`update_protocol_params`).

### 4.3 Loss function

A representative objective:

$$
\mathcal{L}_t =
\lambda_p(p_t-1)^2
+\lambda_{cr}\max(0,CR_{\min}-CR_t)^2
+\lambda_{vol}\,\mathrm{Var}(\Delta NAV)
+\lambda_{turn}\|\mathbf{w}_t-\mathbf{w}_{t-1}\|_1
+\lambda_{conc}\sum_i w_{i,t}^2
+\lambda_{orc}(1-q_t)^2.
$$

Interpretation: preserve peg, avoid under-collateralization, reduce concentration and turnover, and penalize low-confidence oracle regimes.

**Implementation reference:** `solana/keeper/src/optimizer.rs` (`LossFunction::compute`, `LossTerms`, `LossGradients`).

### 4.4 Optimization and projection

microstable applies gradient/Adam-like updates only after:

- gradient clipping,
- bounded delta checks,
- simplex + cap projection,
- safety-gate acceptance.

**Implementation reference:** `solana/keeper/src/optimizer.rs` (`AdamOptimizer`, `project_to_safety_set`, `optimize_step`) and `solana/keeper/src/rebalance.rs` (optimizer wiring).

### 4.5 Circuit breakers

- **CB-1 (depeg)**: asset-level depeg response and weight-cap reduction.
- **CB-2 (collateral stress)**: mint tightening / halt under systemic stress.
- **CB-3 (oracle degraded)**: conservative mode and optimization freeze.
- **CB-4 (numerical rollback)**: checkpoint rollback on non-finite/unsafe optimizer state.

**Implementation reference:** `solana/keeper/src/optimizer.rs` (`OptimizerCheckpoint`, rollback logic in `optimize_step`).

### 4.6 Liquidity execution guardrails

Execution constraints (slippage and turnover slicing) are distinct from numerical safety breakers, preventing optimization-valid but market-toxic moves.

## 5. Architecture

v0.3 formalizes a **two-layer production architecture**:

### 5.1 On-chain (Solana program)

- custody/accounting,
- mint/redeem state transitions,
- invariant enforcement,
- circuit-breaker state machine,
- bounded update acceptance.

### 5.2 Off-chain (Rust keeper daemon)

- oracle ingestion (`oracle.rs`),
- rebalance proposals + optimizer (`rebalance.rs`, `optimizer.rs`),
- monitor/watchdog + risk manager (`monitor.rs`, `watchdog.rs`, `risk_manager.rs`),
- AIG/tournament scheduling (`aig.rs`, `tournament.rs`, `agent_loop.rs`),
- keeper quorum coordination and submission (`utils.rs`, `wire.rs`).

Reference implementation: `solana/keeper/` (`microstable-keeper`).

### 5.3 Python simulation status

The Python simulator remains preserved under `simulation/` as an **archived reference and verification harness**, not the production runtime component.

## 6. Security Analysis

Security analysis in v0.3 is informed by multi-round adversarial campaigns rather than purely hypothetical threat narratives.

### 6.1 Attack classes observed in practice

- reward/accounting manipulation,
- identity/authorization bypass attempts,
- oracle freshness and binding abuse,
- tournament gaming and sybil reward capture,
- watchdog consensus abuse,
- numeric poisoning (NaN/Inf/edge semantics),
- lifecycle and governance race conditions.

### 6.2 Defensive design principles

1. **Hard state invariants first** (caps/floors/finite checks).
2. **Layered authorization** (keeper quorum, signer checks, scoped keys).
3. **One-shot and replay-safe flows** (commit/reveal consume semantics, nonce discipline).
4. **Economic penalties with semantic consistency** (slash model coherence).
5. **Conservative fallback paths** for oracle degradation and abnormal states.

### 6.3 Residual risk

Remaining risk clusters are economic griefing and edge-case semantics under adversarial composition. Continuous red/blue cycling is treated as a permanent operational requirement, not a one-time audit phase.

## 7. Open Agent Economy

(Implemented in on-chain + keeper modules)

### 7.1 Participation model

microstable moves from fixed 3-agent operation toward permissionless participation:

- open registration via stake,
- role specialization (Optimizer, Monitor, Auditor, Liquidator),
- on-chain Agent Registry records status, stake, and reputation.

**Implementation reference:** `solana/programs/microstable/src/lib.rs` (agent registry + lifecycle instructions).

### 7.2 Agent Registry

Each agent is tracked through a registry account (PDA), including:

- stake,
- reputation,
- accepted/proposed counts,
- lifecycle status (Active/Cooldown/Slashed/Deregistered).

**Implementation reference:** `solana/programs/microstable/src/lib.rs` (`AgentRecord`, `register_agent`, `deregister_agent`, `update_agent_score`, `promote_agent`, `demote_agent`, `slash_agent`, `claim_stake`).

### 7.3 ACP (Agent Communication Protocol)

ACP v1 is exposed through the MCP server (`mcp-server/`) as a JSON-RPC style interface for proposal submission, anomaly reporting, and state queries. On-chain enforcement is handled by the Agent Registry instructions.

### 7.4 Optimization tournaments

OAE introduces competitive proposal selection (commit/reveal compatible) with score adjustments and anti-gaming controls (copycat penalties, minimum stake, stake-weighted reputation).

**Implementation reference:** `solana/keeper/src/tournament.rs` (proposal scoring) + `solana/programs/microstable/src/lib.rs` (`commit_rebalance`, `rebalance`).

## 8. Agent Intelligence Gate

AIG adds progressive trust before full protocol influence.

### 8.1 Tiered progression

- **Tier 0**: challenge exam on historical stress scenarios.
- **Tier 1**: sandbox trial (100 epochs).
- **Tier 2**: probation with restricted authority (minimum 30 epochs).
- **Tier 3**: full participation with ongoing demotion checks.

**Implementation reference:** `solana/keeper/src/aig.rs` (challenge runner), `solana/keeper/src/agent_loop.rs` (scheduler), `solana/programs/microstable/src/lib.rs` (`AgentRecord.tier`).

### 8.2 AgentScore model

AgentScore combines quality, latency, safety, adversarial resilience, and consistency:

$$
\mathrm{Score}=0.35Q_{opt}+0.20Q_{lat}+0.20Q_{safe}+0.15Q_{adv}+0.10Q_{cons},
$$

with score-to-tier mapping and downgrade triggers for deterioration.

### 8.3 Integration policy

AIG gating is enforced at admission and during operation, reducing low-quality or malicious agent influence in OAE.

## 9. Protocol Resilience

(From priority matrix in protocol gap analysis)

### 9.1 Structural gap matrix

| Gap | Risk | Priority | Mitigation Direction |
|---|---|---|---|
| Correlated collateral risk | CRITICAL | P1 | Correlation-aware rebalancing + preemptive caps |
| Collateral freeze risk | CRITICAL | P1 | Freeze-aware reweighting + redemption rerouting |
| Bank run / redemption spiral | CRITICAL | P1 | Dynamic redemption fees + queued fair settlement |
| Off-chain agent collusion | HIGH | P2 | Behavioral cluster detection + cluster penalties |
| Governance plutocracy | HIGH | P2 | Entity caps + dampened governance weighting |
| MEV / front-running | HIGH | P2 | Extended commit-reveal + batch style settlement |
| CB cascading deadlock | CRITICAL | P1 | Interaction graph + forced recovery order |
| Program upgrade single-key risk | CRITICAL | P1 | Multisig + timelock + guardian separation |
| Economic death spiral | CRITICAL | P1 | Economic floor + treasury draw caps |
| Information asymmetry | HIGH | P2 | Real-time disclosure + audit logs + policy penalties |

### 9.2 Interpretation

The top resilience priorities are not cosmetic optimizations; they are existential liveness and solvency controls.

## 10. Adversarial Infrastructure

(From sections 1–2 of adversarial infrastructure spec)

### 10.1 Threat model assumptions

Adversaries are modeled as high-speed, persistent, massively parallel, adaptive entities (up to swarm-level coordination and substantial attack budget).

### 10.2 Embedded Red/Blue architecture

- **Red side**: mutation-based attack generation, compositional attack chains, swarm execution, evolutionary search.
- **Blue side**: anomaly detection (statistical/graph/behavioral), automatic response, forensics signatureing, adaptive hardening.

### 10.3 Antifragile loop

microstable uses a continuous adversarial loop where successful attacks are converted into signatures/policies to improve future immunity.

Immunity metric:

$$
\mathrm{Immunity}=1-\frac{\text{successful attacks}}{\text{total attacks}}.
$$

Reported campaign immunity score: **1.0** (adversarial infrastructure test report).

## 11. Security Audit Results

### 11.1 Continuous cycling methodology

microstable used alternating discovery/patch campaigns (Purple/Red + Blue patching) rather than one static audit.

Campaign chain (as reported):

- Purple v1: **27 findings**
- Blue v2: **27 patched**
- Purple v2: **28 findings**
- Red v3: **16 successful / 36 attempts**
- Blue v3: **full patch cycle**
- Purple v3: **23 findings**
- Red v4: **13 successful / 24 attempts**
- Crimson: **20 successful / 27 attempts**

### 11.2 Test totals across modules

- **Core**: 71/71 PASS
- **Mega Stress**: 8000/8000 PASS
- **Open Agent Economy**: 115/115 PASS
- **Adversarial Infrastructure**: 100/100 PASS
- **Agent Intelligence Gate**: 54/54 PASS
- **Protocol Resilience**: 98/98 PASS

Additional operational hardening evidence:

- Chaos engineering: **8/8 PASS**
- Degradation tests: **5/5 PASS**

## 12. Simulation Results

This section preserves the Gate A + Monte Carlo structure from v0.1 and adds mega-stress context.

### 12.1 Gate A setup

- Monte Carlo scope: **100 seeds × 6 scenarios × 80 ticks**
- Gate A criteria:
  - peg MAE < 0.0015
  - CR violation rate < 1%
  - breaker false positive rate < 5%

### 12.2 Gate A outcomes (100 runs per scenario)

| Scenario | pass_count | fail_count | Gate A | peg MAE worst | CR_min worst (lowest) | CR violation worst | FP worst |
|---|---:|---:|---:|---:|---:|---:|---:|
| normal | 100 | 0 | PASS | 0.000366 | 1.201000 | 0.000000% | 0.000000% |
| single_depeg | 100 | 0 | PASS | 0.000460 | 1.201000 | 0.000000% | 0.000000% |
| multi_depeg | 100 | 0 | PASS | 0.001171 | 1.202422 | 0.000000% | 0.000000% |
| volatile | 100 | 0 | PASS | 0.000504 | 1.201000 | 0.000000% | 0.000000% |
| gradient_attack | 100 | 0 | PASS | 0.000389 | 1.201000 | 0.000000% | 0.000000% |
| oracle_failure | 100 | 0 | PASS | 0.000639 | 1.202554 | 0.000000% | 0.000000% |

Worst peg MAE under Gate A: **0.001171** (`multi_depeg`).

### 12.3 Monte Carlo KPI distributions (mean / median / p5 / p95 / worst)

#### (A) peg MAE

| Scenario | mean | median | p5 | p95 | worst |
|---|---:|---:|---:|---:|---:|
| normal | 0.000336 | 0.000336 | 0.000321 | 0.000353 | 0.000366 |
| single_depeg | 0.000405 | 0.000406 | 0.000378 | 0.000439 | 0.000460 |
| multi_depeg | 0.001113 | 0.001113 | 0.001083 | 0.001143 | 0.001171 |
| volatile | 0.000405 | 0.000406 | 0.000354 | 0.000457 | 0.000504 |
| gradient_attack | 0.000334 | 0.000334 | 0.000307 | 0.000366 | 0.000389 |
| oracle_failure | 0.000585 | 0.000586 | 0.000558 | 0.000614 | 0.000639 |

#### (B) CR_min

| Scenario | mean | median | p5 | p95 | worst (lowest) |
|---|---:|---:|---:|---:|---:|
| normal | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| single_depeg | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| multi_depeg | 1.202941 | 1.202961 | 1.202601 | 1.203325 | 1.202422 |
| volatile | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| gradient_attack | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| oracle_failure | 1.202944 | 1.202959 | 1.202689 | 1.203232 | 1.202554 |

### 12.4 Breaker/turnover behavior

Breaker activations were scenario-aligned (e.g., depeg/oracle scenarios), with zero false positives in this campaign.

### 12.5 Mega stress campaign

- Scope: **80 scenarios × 100 Monte Carlo = 8,000 runs**
- Result: **ALL PASS (8000/8000)**
- Max MAE: **0.02684**
- Crash/NaN/Inf: **0**

## 13. Devnet Deployment

### 13.1 On-chain identifiers

- Program ID: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- MSTB mint: `EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R`

### 13.2 Oracle integration

Pyth feeds integrated for:

- USDC/USD
- USDT/USD
- DAI/USD

(Devnet feed mapping and accounts tracked in deployment artifacts.)

### 13.3 SPL token flow

Devnet E2E test (`solana/tests/devnet-e2e.ts`) validates:

1. state migration/initialization,  
2. oracle updates,  
3. collateral deposit mint path,  
4. redeem path,  
5. post-redeem accounting consistency.

## 14. Agent Integration

### 14.1 MCP server

- Package: `microstable-mcp-server@0.1.0` (npm)
- Purpose: expose protocol operations via MCP for external agents.

### 14.2 ClawHub skill integration

- Agent skill path: `microstable-agent` integration profile
- OAE-compatible operations: proposal submission, anomaly reporting, state query, reward workflows.

### 14.3 Design objective

Agent integrations are intended to be machine-friendly while keeping protocol internals inspectable and auditable.

## 15. Comparison

| Dimension | DAI-like | FRAX-like (historical hybrid) | mStable-style basket | microstable v0.3 |
|---|---|---|---|---|
| Collateral model | Over-collateralized | Fractional/hybrid | Basket aggregation | Basket + adaptive CR |
| Parameter updates | Governance epochs | Controller/policy dependent | Mostly static/rule-based | Bounded gradient updates |
| Circuit-breaker formalism | Moderate | Model-dependent | Limited | Explicit multi-CB state machine |
| Agent-native participation | Low | Low/Medium | Low | **High (OAE + AIG)** |
| Adversarial feedback loop | Limited | Limited | Limited | **Embedded Red/Blue cycling** |
| Runtime architecture | On-chain heavy governance | Mixed | App-layer routing | Solana kernel + Rust keeper daemon |

## 16. Limitations & Future Work

1. **Objective misspecification risk**: better loss does not guarantee better real-world outcomes.
2. **Data and oracle dependence**: adaptation quality is bounded by data integrity.
3. **Economic gameability**: adversaries co-adapt to deterministic defenses.
4. **Cross-layer divergence risk**: simulation and on-chain behavior must remain tightly aligned.
5. **Governance capture pressure**: stake-based systems require persistent anti-plutocracy controls.

Priority future work:

- formal verification for critical state transitions,
- stronger oracle provenance and freshness hardening,
- tighter auth semantics in all control paths,
- broader stress matrices for correlated failures,
- policy-level transparency tooling for agent actions.

## 17. Conclusion

microstable v0.3 is not a claim of finality; it is a claim of method.

- keep mechanism compact,
- keep adaptation bounded,
- keep safety non-negotiable,
- keep adversarial testing continuous.

The protocol’s path is intentionally incremental: archived simulation rigor, production-oriented Solana + Rust runtime, and agent-native controls that are permissionless but not trustless-by-assumption.

## 18. Reproducibility & References

### 18.1 Reproducibility pointers

- Reference commit for this whitepaper snapshot: `main` (post-doc sync)
- Key artifacts:
  - `simulation/outputs/open-agent-economy-test-report.md`
  - `simulation/outputs/adversarial-agent-report.md`
  - `simulation/outputs/chaos/chaos-summary.md`
  - `simulation/outputs/chaos/degradation-test-results.json`
  - `simulation/outputs/mega-stress-report.md`

### 18.2 References

1. S. Nakamoto, *Bitcoin: A Peer-to-Peer Electronic Cash System*, 2008.  
2. A. Karpathy, *micrograd/microgpt educational implementations* (public lectures/repos).  
3. MakerDAO documentation and risk framework materials.  
4. FRAX historical design documentation.  
5. mStable documentation on basket-based stable assets.  
6. Post-mortem analyses on algorithmic stablecoin failures (including UST/Luna).  
7. microstable internal docs: OAE, AIG, protocol gap analysis, adversarial infrastructure, and security campaign reports.
