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
| Phase 1 | ✅ Complete | Pure Python simulation (`microstable.py`) and scenario checks |
| Phase 2 (M6) | 🟡 Local-validated | Anchor program deployed and exercised on local validator (devnet faucet blocked) |
| Phase 3 | ⬜ Planned | Agent interfaces + autonomous operation |

## M6 Deployment Result

- **Deployment target:** `localnet` (fallback from devnet due repeated faucet 429 rate limits)
- **Program ID:** `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- **Upgrade authority:** `3fimeXDHiEK9oeJX6XM1rXNoavTCWhzbxNXVmwFzh6Kk`
- **Program deploy tx signatures:**
  - `3DLpWHrMihE1crsWM5PHmFDSM5MfEkpX5wDqQpgoVV92zsmgeAXZLhLhC8Qngaq6ovLjFQNuFBzsVKkEs78bvGtU`
  - `3Bgg62meM3otQnVFe6xiwi6A9FFASz9LRuTnfDymzBkuqGc2Mjb3hHur2k9Ys7t2bnGKjQ4GnZ9omWWCrnE7ktkb`
- **Initialize tx signature:**
  - `2LGAitxLffMVbLP2cSHNaEusaS6MQmCnKbVkT9ot9kkYpetTtry6men9mRwCeHSYhbudmzqxwMCw9bdmbBqqrAMM`

### Test Status (localnet)

`anchor test --skip-local-validator --skip-deploy --provider.cluster localnet`

- Passing: 5
- Failing: 2
  - `mint flow`: `InsufficientCollateralRatio (6012)`
  - `redeem flow`: `AccountNotInitialized (3012)` (downstream of mint failure)

### Verified On-Chain State

- Program account exists and is executable under `BPFLoaderUpgradeab1e11111111111111111111111`
- `protocol_state` PDA exists: `9NbeDUSPdhC4ZgpefoqT3p48eLEyXknQJEm6v5pLGFQP`
- `circuit_breaker` PDA exists: `7xy7xc4nqhywYa72Bb5A2u7g3t6kz96HN2e2z4Yn9WXe`
- Decoded protocol state confirms keeper, fee params, and weight vector persisted on-chain

## How to Interact (Anchor)

```bash
cd solana
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.cargo/bin:$PATH"

# Local validator flow
solana-test-validator --reset
solana config set --url localhost
anchor deploy --provider.cluster localnet
anchor test --skip-local-validator --provider.cluster localnet

# Inspect deployed program/account state
solana program show BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3 --url localhost
anchor account microstable.ProtocolState 9NbeDUSPdhC4ZgpefoqT3p48eLEyXknQJEm6v5pLGFQP --provider.cluster localnet
```

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
