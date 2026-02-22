# microstable: A Deterministic, Agent-Native Multi-Collateral Stablecoin Protocol

**Version**: Draft v0.3  
**Status**: Research Whitepaper (Educational / Hobby Project)  
**Runtime Architecture**: Solana on-chain program (Anchor/Rust) + off-chain Rust keeper daemon  
**Program ID (Devnet)**: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`

> "Keep the protocol small and inspectable. Make adaptation bounded and auditable."

---

## 1. Abstract

Stablecoins remain essential settlement primitives, but typical failure modes are persistent: collateral concentration, delayed policy updates, oracle degradation, and weak emergency controls. **microstable** proposes a bounded and inspectable design: deterministic settlement on-chain, rule-based policy computation off-chain, and strict safety guards at every transition.

Draft **v0.3** updates the architecture and operations model to reflect production direction:

- Solana on-chain kernel in **Anchor/Rust**,
- Rust keeper daemon for oracle/rebalance/monitoring operations,
- archived Python simulation retained only for educational and verification reference,
- full security-cycle status with zero-finding rounds,
- explicit instruction surface and devnet identifiers,
- formal-verification hints for core invariants.

## 2. Scope and Disclaimer

This document is a research whitepaper for an educational/hobby project. It is **not** investment advice, legal advice, or a promise of future performance. The objective is protocol design clarity, safety reasoning, and reproducible engineering.

## 3. Architecture (v0.3)

### 3.1 On-chain kernel: Solana + Anchor/Rust

The on-chain program is the source of truth for:

- custody/accounting state,
- mint/redeem transitions,
- circuit-breaker state machine,
- bounded acceptance of keeper proposals,
- role- and signature-scoped authority checks.

### 3.2 Off-chain runtime: Rust keeper daemon

The keeper performs deterministic, policy-bounded off-chain computation and submits auditable intents to the chain.

### 3.3 Python simulation status

The Python simulator under `simulation/` is now treated as **archived educational material and verification harness**, not a production runtime component.

## 4. Protocol Model (Summary)

### 4.1 Current Implementation: Deterministic Rule-Based Rebalancing

microstable v0.3 uses a **static, deterministic rule-based** rebalancing model — not gradient-based optimization. The design deliberately prioritizes auditability and predictability over autonomy.

**Weight computation** (`compute_target_weights`):

For each collateral vault *i* with deposits *dᵢ*, oracle price *pᵢ*, risk score *rᵢ*, and weight cap *cᵢ*:

1. Compute collateral value: *vᵢ = dᵢ × pᵢ*
2. Compute value ratio: *ratioᵢ = vᵢ / Σvⱼ*
3. Apply risk discount: *scoreᵢ = ratioᵢ × (1 − rᵢ)*
4. Normalize: *wᵢ = scoreᵢ / Σscoreⱼ*
5. Enforce weight caps: clamp each *wᵢ ≤ cᵢ*, redistribute excess proportionally

This is a **closed-form, stateless formula** — no loss function, no gradient, no learning rate, no history dependence. The same inputs always produce the same outputs.

**Circuit breaker state fields**: The on-chain `CircuitBreakerState` contains `optimizer_enabled` and `learning_rate_scale` fields. In the current implementation, these function as **simple toggle flags** (`optimizer_enabled` gates whether the keeper can submit rebalance proposals; `learning_rate_scale` switches between 100% and 50% weight-change dampening). They do not implement actual optimization or learning.

**"Adaptive" behaviors**: The keeper's `adaptive_secondary_confirm_window_secs` adjusts RPC confirmation timeouts based on observed network latency — a standard operational heuristic, not a learning algorithm.

### 4.2 Future Research Direction: Bounded Gradient Optimization

A potential evolution would introduce bounded gradient-based parameter adaptation:

$$
\theta_{t+1}=\Pi_{\Omega}\left(\theta_t-\alpha_t\nabla_\theta\mathcal{L}_t\right)
$$

where \(\Pi_{\Omega}\) projects onto the feasible safety set (caps/floors/simplex/fee limits/CB states), and \(\mathcal{L}_t\) is a composite loss over peg deviation, collateral concentration, and liquidity utilization.

**This formula describes a research target, not the current implementation.** Any future adoption would require:

- formal specification of \(\mathcal{L}_t\) and proof of convergence within \(\Omega\),
- on-chain bounds enforcement independent of keeper correctness,
- adversarial robustness analysis (gradient manipulation, oracle poisoning through loss surface),
- governance approval for the transition from rule-based to adaptive mode.

## 5. On-Chain Instruction Surface (13)

v0.3 instruction surface (13 entrypoints):

1. `initialize`
2. `migrate_legacy_state`
3. `update_oracle`
4. `update_oracle_pyth`
5. `set_pyth_feed`
6. `mint`
7. `redeem`
8. `commit_rebalance`
9. `rebalance`
10. `activate_circuit_breaker`
11. `recover_circuit_breaker`
12. `emergency_shutdown` / `resume`
13. `rotate_keeper_set`

## 6. Keeper Daemon Design (Rust)

The keeper daemon is organized into four core modules:

1. **oracle module**: fetches and normalizes multi-source data, freshness checks, confidence-aware scoring.
2. **rebalance module**: computes bounded proposals under policy constraints and prepares commit/reveal-compatible transactions.
3. **monitor module**: evaluates protocol health (peg, CR, liquidity, oracle quality, breaker triggers) and emits deterministic alerts.
4. **watchdog module**: supervises keeper process health, task deadlines, and safety fallback transitions.

### 6.1 Dual-RPC validation and tolerance checks

Keeper reads are cross-checked between primary and secondary RPC endpoints. State responses are compared with tolerance-aware rules to detect divergence, lag, and transient fork ambiguity.

### 6.2 `SecondaryRpcMode`

Keeper state transitions include three explicit secondary-RPC modes:

- **normal**: primary + secondary both healthy and convergent,
- **degraded**: secondary present but unstable/divergent (tightened policy, conservative cadence),
- **no-secondary**: only primary available (most conservative behavior and escalation logging).

### 6.3 Adaptive confirmation windows

Submission confirmation windows adapt to observed network conditions (slot delay, commit latency, recent finalization behavior), reducing both premature retries and unsafe assumptions.

### 6.4 Build attestation and supply-chain controls

- `Cargo.lock` anchored compile-time attestation,
- reproducible build checks,
- dependency pinning and review gates,
- controlled artifact provenance for keeper release flow.

## 7. Security Lifecycle Results (v0.3)

microstable completed a full iterative security cycle with Purple/Red/Blue/Crimson style operations. Final cycle report: **6 rounds, ZERO FINDINGS**.

Additional verification status:

- **38 integration tests: all passing**,
- no open critical findings in the current tracked cycle,
- continuous adversarial testing retained as a permanent requirement.

## 8. Devnet Deployment

- **Program ID**: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- **MSTB mint**: `EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R`

These identifiers are the current reference endpoints for devnet experimentation and integration tests.

## 9. Formal Verification Hints

Formal verification scope candidates for v0.3:

1. **Solvency invariant**: protocol liabilities never exceed risk-adjusted collateral under accepted transitions.
2. **Weight simplex invariant**: collateral weights remain non-negative and sum to one under all update paths.
3. **Bounded-parameter invariant**: every accepted parameter update respects per-epoch and global bounds.
4. **Circuit-breaker liveness**: once recovery conditions are met, the protocol can progress from breaker states without deadlock.

These properties can be specified as executable invariants and model-checking targets for critical state-machine paths.

## 10. Open Agent Economy (OAE) + AIG

microstable keeps permissionless participation as a core direction while coupling it with accountability controls:

- permissionless registration and role specialization,
- stake/reputation-aware participation,
- tournament-style proposal competition,
- **Agent Intelligence Gate (AIG)** for phased admission and operational demotion when quality/safety degrades.

The design goal is open participation without surrendering protocol safety.

## 11. Agent Integration and MCP

The protocol integration layer now includes published MCP tooling:

- npm package: **`microstable-mcp-server@0.1.0`**,
- purpose: expose machine-friendly protocol operations for external agent systems,
- policy goal: automation-friendly interfaces with auditable control boundaries.

## 12. Conclusion

microstable v0.3 keeps the same thesis with clearer implementation posture:

- on-chain settlement and invariants in Solana Anchor/Rust,
- off-chain deterministic rule-based policy in a hardened Rust keeper,
- gradient-based optimization reserved as a future research direction (§4.2),
- archived Python simulation for education and verification,
- explicit safety-cycle evidence and reproducible integration status.

The current design deliberately chooses auditability and predictability over autonomous adaptation. Every weight computation is a stateless, closed-form function — the same inputs always produce the same outputs. The project remains intentionally incremental, inspectable, and safety-first.

## 13. References (Selected)

1. S. Nakamoto, *Bitcoin: A Peer-to-Peer Electronic Cash System*, 2008.  
2. Solana and Anchor documentation.  
3. Pyth network documentation.  
4. Public literature on stablecoin failures and risk controls.  
5. microstable internal specs (OAE, AIG, keeper, security cycle reports).
