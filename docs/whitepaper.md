# microstable: A Self-Evolving Multi-Collateral Stablecoin Protocol

**Version**: Draft v0.1  
**Status**: Research Whitepaper (Educational / Hobby Project)  
**Target Chain (Phase 2+)**: Solana

> "This file is the complete algorithm. Everything else is just efficiency."

---

## 1. Abstract

Stablecoins have become critical infrastructure for decentralized finance, yet existing designs face persistent fragility: collateral concentration risk, governance latency, oracle dependency, and static risk parameters that fail under regime shifts. We propose **microstable**, a self-evolving multi-collateral stablecoin protocol inspired by two minimalist traditions: (i) the explicit rule-based spirit of the Bitcoin whitepaper and (ii) the compact differentiable programming approach exemplified by Karpathy’s microgpt/micrograd style.

microstable maintains a basket of stablecoin collateral (e.g., USDC, USDT, DAI) and continuously optimizes basket weights and selected protocol parameters using gradient descent on a differentiable risk-and-peg loss function. The protocol combines automatic rebalancing with deterministic safety layers (circuit breakers, bounded parameter projections, mint/redeem throttles) to preserve solvency under stress. We describe a two-layer architecture: a deterministic on-chain execution kernel and an off-chain optimization engine that computes candidate updates and submits verifiable, bounded actions. 

This paper presents the design goals, mathematical formulation, security model, and implementation pathway from a pure Python simulation (`microstable.py`) to a Rust/Anchor deployment on Solana devnet.

## 2. Introduction

Bitcoin demonstrated that monetary rules can be encoded as transparent protocol logic rather than institutional discretion. In a different domain, compact educational implementations such as microgpt showed that complex behavior can emerge from surprisingly small, explicit code.

microstable combines these two impulses: keep the mechanism small and inspectable, but let the protocol adapt its internal parameters from market feedback in a constrained and auditable way.

### 2.1 Motivation

Current stablecoin designs broadly fall into three categories:

1. **Fiat-backed centralized issuers** (high short-term stability, but custody/freeze/regulatory concentration).
2. **Over-collateralized decentralized systems** (robust but capital inefficient, often with slow governance adjustment).
3. **Algorithmic or partially algorithmic designs** (capital-efficient ambitions, but historically vulnerable to reflexive collapse).

A recurring weakness is parameter rigidity. Risk thresholds, fee curves, and collateral weights are often updated manually or episodically, while market states shift continuously.

### 2.2 Thesis

microstable’s thesis is that a stablecoin protocol can remain deterministic and transparent while making **small, bounded, gradient-based parameter updates** at high frequency. Instead of hand-tuned periodic governance reactions, the protocol uses an explicit objective: peg quality, solvency margin, collateral diversification, and market impact cost.

## 3. Background

### 3.1 Lessons from Algorithmic Stablecoin Failures (UST/Luna)

The UST/Luna collapse highlighted three systemic failure modes:

- Reflexive mint/burn feedback loops under confidence shock.
- Liquidity evaporation and slippage amplification.
- Inadequate hard safety constraints during rapid depeg.

The key lesson is not merely “algorithmic is bad,” but that **unbounded reflexivity without robust circuit breakers is catastrophic**.

### 3.2 Existing Approaches

- **DAI (MakerDAO)**: over-collateralized debt positions and conservative risk management; stronger resilience but parameter updates are governance-heavy and slower than market microstructure.
- **FRAX (historical hybrid model)**: partial collateralization plus algorithmic components; adaptive but dependent on market confidence and external liquidity conditions.
- **mStable**: basket-based stablecoin aggregation emphasizing diversified stable exposure and swap efficiency.

microstable borrows from basket diversification and over-collateralized discipline, while introducing differentiable parameter adaptation as a first-class primitive.

## 4. System Design

### 4.1 Multi-Collateral Basket

Let the collateral set be:

$$
\mathcal{C} = \{c_1, c_2, \dots, c_n\}
$$

with basket weights:

$$
\mathbf{w}_t = (w_{1,t}, \dots, w_{n,t}), \quad w_{i,t} \ge 0, \quad \sum_i w_{i,t} = 1.
$$

Collateral value is marked from oracle prices and haircut-adjusted by asset-specific risk coefficients.

### 4.2 Differentiable Protocol Parameters

Define parameter vector:

$$
\boldsymbol{\theta}_t = [\text{targetCR}_t, \text{mintFee}_t, \text{redeemFee}_t, \mathbf{w}_t, \ldots]
$$

where each component is represented through a lightweight autograd primitive (`Value` class) in the simulator. The implementation objective is inspectability over framework complexity.

### 4.3 Self-Evolving Rebalancing via Gradient Descent

At each rebalance epoch, the optimizer computes:

$$
\mathbf{g}_t = \nabla_{\boldsymbol{\theta}} \mathcal{L}_t
$$

and applies an update (e.g., Adam) with projection to feasible bounds:

$$
\boldsymbol{\theta}_{t+1} = \Pi_{\Omega}\left(\boldsymbol{\theta}_t - \alpha_t \cdot \text{AdamStep}(\mathbf{g}_t)\right)
$$

where $\Pi_{\Omega}$ enforces constraints (e.g., fee ranges, collateral limits, simplex weights, minimum collateral ratio).

### 4.4 Loss Function Design

A representative objective:

$$
\mathcal{L}_t =
\lambda_p (p_t - 1)^2
+ \lambda_{cr} \max(0, CR_{\min} - CR_t)^2
+ \lambda_{vol}\, \mathrm{Var}(\Delta NAV_{t:t+H})
+ \lambda_{turn}\, \|\mathbf{w}_t - \mathbf{w}_{t-1}\|_1
+ \lambda_{conc}\, \sum_i w_{i,t}^2
+ \lambda_{orc}(1-q_t)^2
$$

where:

- $p_t$: protocol token market price.
- $CR_t$: effective collateral ratio.
- $NAV$: basket net asset value.
- $q_t$: oracle confidence score.
- $\lambda_*$: tunable risk preference coefficients.

Interpretation:
- Keep peg close to 1.
- Penalize under-collateralization heavily.
- Reduce path volatility and turnover.
- Avoid concentration in a single issuer/asset.
- Degrade risk appetite when oracle confidence deteriorates.

### 4.5 Circuit Breakers

microstable is not “purely continuous optimization.” It is optimization **inside hard guardrails**.

Circuit breaker classes:

1. **Depeg breaker (CB-1)**: if an asset depegs beyond threshold $\delta$ for duration $\tau$, reduce its max weight and pause mint expansions.
2. **Collateral stress breaker (CB-2)**: if projected $CR$ breaches safety floor under stress simulation, increase targetCR and tighten mint path.
3. **Oracle breaker (CB-3)**: if feed divergence/latency exceeds bounds, freeze optimization updates and switch to conservative static profile.
4. **Numerical/rollback breaker (CB-4)**: if non-finite loss/gradient or unsafe optimizer state is detected, rollback to last checkpoint and enter conservative mode.

### 4.6 Liquidity / Slippage Constraints (separate from CB-4)

Liquidity/slippage controls are an **execution constraint layer**, not the CB-4 numerical safety breaker.

- Slippage caps and turnover bounds limit how much reallocation can execute per epoch.
- If projected impact is high, rebalance is sliced over multiple epochs.
- This mechanism controls market impact risk even when optimization math remains valid.

In short: **CB-4 protects numerical integrity and rollback safety**, while **liquidity/slippage controls protect market execution quality**.

## 5. Architecture

### 5.1 On-Chain vs Off-Chain Responsibilities

**On-chain (deterministic kernel)**
- Custody/accounting of collateral balances.
- Mint/redeem settlement rules.
- Enforcement of bounds and invariants.
- Circuit breaker state machine.
- Acceptance/rejection of proposed parameter updates.

**Off-chain (optimization layer)**
- Ingest oracle and market telemetry.
- Compute gradients and candidate updates.
- Produce signed update proposals with bounded deltas.
- Submit updates through keeper network.

This split preserves deterministic settlement while enabling richer computation without excessive on-chain cost.

### 5.2 Why Solana

Solana is selected for:

- High throughput for frequent rebalance checkpoints.
- Low transaction costs for iterative small updates.
- Fast finality supporting near-real-time safety actions.
- Mature oracle ecosystem (e.g., Pyth / Switchboard patterns).

The protocol can map naturally to Solana programs + PDAs + crank/keeper execution.

## 6. Security Analysis

### 6.1 Gradient Manipulation Attacks

Adversaries may try to shape input data so optimization drifts toward exploitable allocations.

Mitigations:
- Robust loss terms using clipped errors and multi-window statistics.
- Maximum per-epoch parameter delta constraints.
- Ensemble oracle inputs with divergence checks.
- Delayed activation / two-step commit for large updates.

### 6.2 Collateral Risk (Centralized Stablecoin Freeze)

Basket components may carry issuer freeze or sanction risk.

Mitigations:
- Asset-level freeze-risk score in loss/constraints.
- Hard issuer concentration caps.
- Emergency migration profile that reduces frozen or suspect collateral weights.
- Redemption policy prioritizing unaffected reserves.

### 6.3 Oracle Risk

Failure modes include stale prices, manipulated feeds, and liveness loss.

Mitigations:
- Median-of-sources with confidence thresholding.
- Staleness and heartbeat checks.
- Automatic fallback mode with conservative static parameters.
- Explicit “oracle degraded” state visible on-chain.

### 6.4 Death Spiral Prevention

The protocol avoids reflexive expansion under stress by design:

- Mint throttling when peg < threshold.
- Dynamic collateral ratio increases during volatility spikes.
- Redemption queue controls to reduce run dynamics.
- No unbounded endogenous governance token reflexivity in Phase 1/2 design.

## 7. Simulation Results

This section is updated with **real M4 verification data** from the current implementation (`microstable.py`, `test_microstable.py`).

### 7.1 Verification Campaign Setup

- Command: `cd /Users/kjaylee/.openclaw/workspace/microstable && python3 test_microstable.py`
- Monte Carlo scope: **100 seeds × 6 scenarios × 80 ticks** (600 runs)
- Scenarios: `normal`, `single_depeg`, `multi_depeg`, `volatile`, `gradient_attack`, `oracle_failure`
- Full raw log: `outputs/m4-test_microstable-full-output.txt`
- Aggregate stats artifact: `outputs/m4-montecarlo-analysis.json`

### 7.2 Gate A Outcomes (Per Scenario, 100 runs each)

Gate A criteria:
- peg MAE < 0.0015
- CR violation rate < 1%
- breaker false positive rate < 5%

| Scenario | pass_count | fail_count | Gate A status | peg MAE worst | CR_min worst (lowest) | CR violation worst | FP worst |
|---|---:|---:|---:|---:|---:|---:|---:|
| normal | 100 | 0 | PASS | 0.000366 | 1.201000 | 0.000000% | 0.000000% |
| single_depeg | 100 | 0 | PASS | 0.000460 | 1.201000 | 0.000000% | 0.000000% |
| multi_depeg | 100 | 0 | PASS | 0.001171 | 1.202422 | 0.000000% | 0.000000% |
| volatile | 100 | 0 | PASS | 0.000504 | 1.201000 | 0.000000% | 0.000000% |
| gradient_attack | 100 | 0 | PASS | 0.000389 | 1.201000 | 0.000000% | 0.000000% |
| oracle_failure | 100 | 0 | PASS | 0.000639 | 1.202554 | 0.000000% | 0.000000% |

**Result**: all scenarios pass Gate A; stop condition (≥3 failures on any scenario) was not triggered.

### 7.3 Monte Carlo KPI Distributions (mean / median / p5 / p95 / worst)

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

#### (C) turnover and breaker activations

| Scenario | turnover mean | turnover p95 | turnover worst | breaker activations mean | breaker activations worst |
|---|---:|---:|---:|---:|---:|
| normal | 0.000003 | 0.000003 | 0.000003 | 0.0 | 0 |
| single_depeg | 0.001369 | 0.001370 | 0.001370 | 1.0 | 1 |
| multi_depeg | 0.001658 | 0.001929 | 0.001938 | 2.0 | 2 |
| volatile | 0.000010 | 0.000023 | 0.000039 | 0.0 | 0 |
| gradient_attack | 0.000003 | 0.000003 | 0.000004 | 0.0 | 0 |
| oracle_failure | 0.000003 | 0.000004 | 0.000004 | 1.0 | 1 |

### 7.4 Interpretation

1. **Peg robustness**: all scenarios remain below the MAE threshold (0.0015), with `multi_depeg` as the closest stress case (worst 0.001171).
2. **Solvency safety**: no CR hard-floor violations observed in any of 600 runs.
3. **CR_min direction fix confirmed**: for CR_min (lower is worse), the reported `worst` value is now the minimum observed value (`min(vals)`), and Gate A tables use this directional definition.
4. **Breaker quality**: zero false positives under this scenario set; breaker activations align with intended stress classes (depeg/oracle-linked routing).

## 8. Comparison with Existing Approaches

| Dimension | DAI-like | FRAX-like (historical hybrid) | mStable-style basket | microstable |
|---|---|---|---|---|
| Collateralization | Over-collateralized | Fractional/hybrid | Basket aggregation | Basket + adaptive CR |
| Parameter updates | Governance epochs | Policy/controller dependent | Mostly rule/static | Gradient-based, bounded, frequent |
| Diversification | Medium | Medium | High on stable basket | High + risk-aware concentration penalty |
| Reflexivity risk | Lower | Medium/High (model dependent) | Lower | Controlled via breakers + bounds |
| Oracle dependency | High | High | Medium/High | High, mitigated by oracle confidence penalties |
| Primary novelty | Conservative CDP model | Capital-efficiency experiments | Basket UX/capital routing | Differentiable self-evolving policy |

## 9. Limitations & Future Work

1. **Model risk**: Loss function misspecification may optimize the wrong objective.
2. **Data risk**: Optimization quality is bounded by oracle quality and latency.
3. **Interpretability**: Frequent parameter updates can reduce human readability unless excellent observability is provided.
4. **Governance boundary**: Some policy choices (allowed assets, hard caps) should remain explicit governance decisions.
5. **Adversarial adaptation**: Attackers can co-adapt to deterministic update logic.

Future work:
- Formal verification of invariant enforcement.
- Robust optimization (CVaR, adversarial training, distribution shift tests).
- Cryptographic attestations for off-chain optimizer runs.
- Multi-chain collateral abstraction with canonical risk normalization.

## 10. Conclusion

microstable proposes a simple idea: stablecoin rules should be explicit, but not static. A protocol can remain deterministic at settlement while adapting bounded risk parameters through transparent gradient-based updates.

The project starts intentionally small: a dependency-free Python simulation where the full mechanism is inspectable in one file. If the simulator demonstrates robust behavior under stress, the design graduates to Solana with strict invariant checks and conservative rollout controls.

In that sense, microstable follows the same engineering ethos that inspired it: keep the algorithm understandable, keep the safety rails hard, and treat optimization as a servant of solvency—not a replacement for it.

## 11. Reproducibility

- All results generated at commit: `2bff8d6` (and regenerated from this working tree update)
- Command: `python3 test_microstable.py`
- Environment: Python 3.x, no external dependencies

## 12. References

1. S. Nakamoto, *Bitcoin: A Peer-to-Peer Electronic Cash System*, 2008.  
2. A. Karpathy, *micrograd / microgpt educational implementations* (public repositories and lectures).  
3. MakerDAO Documentation, *DAI and Collateral Risk Framework*.  
4. FRAX Documentation and historical design notes (fractional-algorithmic model evolution).  
5. mStable Documentation, *Basket-based stable asset design*.  
6. Post-mortem analyses of UST/Luna collapse (2022) from industry research reports.
