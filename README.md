# microstable

> *Self-evolving multi-collateral stablecoin protocol*

A stablecoin protocol that replaces human governance with **gradient-based parameter optimization** and **autonomous AI agent governance**.

## Core Idea

Stablecoin rules should be explicit, but not static. A protocol can remain deterministic at settlement while adapting bounded risk parameters through transparent gradient-based updates.

```
Oracle Feed → Loss Function → ∇θ → Adam Update → Bounded Projection → On-Chain
```

**No DAO votes. No multisig delays. No human governors.**

Three AI agents (Optimizer-Keeper, Watchdog, Auditor) manage the protocol autonomously, earning protocol fees as incentive — creating a self-sustaining loop.

## What Makes This Different

| | Traditional DeFi | microstable |
|---|---|---|
| Parameter updates | Governance vote (days~weeks) | Gradient descent (per tick) |
| Crisis response | Emergency multisig | Circuit breakers (automatic) |
| Governance | Human DAO | Multi-Agent consensus |
| Incentive | Token voting power | Protocol fee distribution |
| Transparency | Varies | Full algorithm in one file |

## Philosophy

Inspired by two minimalist traditions:
- **Bitcoin whitepaper**: monetary rules as transparent protocol logic
- **Karpathy's micrograd**: complex behavior from small, explicit code

> *"This file is the complete algorithm. Everything else is just efficiency."*

## Project Status

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 | 🔵 In Progress | Pure Python simulation (`microstable.py`, ≤500 lines, zero dependencies) |
| Phase 2 | ⬜ Planned | Rust/Anchor → Solana devnet |
| Phase 3 | ⬜ Planned | Agent interfaces + autonomous operation |

## Architecture

```
┌─────────────── On-Chain (Solana) ───────────────┐
│  Vault │ Basket Config │ Circuit Breaker SM      │
│              ▲ Update Gate (bounded)             │
└──────────────│───────────────────────────────────┘
               │ signed proposal
┌──────────────│─────── Off-Chain (AI Agents) ─────┐
│  Agent #1: Optimizer-Keeper (30% fee)            │
│  Agent #2: Watchdog (10% fee)                    │
│  Agent #3: Auditor (5% fee)                      │
│  Treasury: 55%                                   │
└──────────────────────────────────────────────────┘
```

## Documentation

- [Whitepaper (English)](docs/whitepaper.md)
- [Whitepaper (한국어)](docs/whitepaper-ko.md)
- [Technical Spec (Lv5)](docs/spec.md)
- [Algorithm Verification](docs/algorithm-verification.md)
- [Test Cases](docs/test-cases.md)
- [Execution Plan](docs/plan.md)

## Quick Start (Phase 1 Simulation)

```bash
python3 microstable.py
```

No dependencies. No virtual environment. Just Python 3.10+.

## Security

- All protocol parameters are **bounded** — no unbounded reflexivity
- Circuit breakers enforce hard safety rails independent of optimization
- Multi-agent consensus prevents single-point-of-failure governance
- Oracle confidence scoring degrades risk appetite on feed degradation

For security concerns, please open an issue or contact the maintainers.

## License

MIT — because transparency is the product.
