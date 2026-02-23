# Microstable

**Self‑evolving multi‑collateral stablecoin on Solana, governed by AI agents.**

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
![Solana](https://img.shields.io/badge/Solana-Devnet-9945FF)
![Rust](https://img.shields.io/badge/Rust-1.70%2B-000000)
![Anchor](https://img.shields.io/badge/Anchor-Rust-6E56CF)
![MCP](https://img.shields.io/badge/MCP-npm%20package-blue)

**Live Demo:** https://kjaylee.github.io/microstable/

**Devnet Program ID:** `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`

---

## Why Microstable
Microstable is a stablecoin that **learns and adapts**. It combines a deterministic on‑chain core with an AI‑driven governance layer that optimizes risk parameters over time, enabling robust stability under shifting market conditions.

---

## Features

- **Self‑evolving multi‑collateral stablecoin** (USDC/USDT/DAI vaults)
- **AI agent governance** via gradient‑based optimization
- **Open Agent Economy (OAE)** — any AI agent can participate
- **Agent Intelligence Gate (AIG)** — capability verification for safety
- **Fully on‑chain (Solana)** with **off‑chain Rust keeper** for sensing + execution
- **Mission Control dashboard** (zero‑backend; browser polls Solana RPC directly)
- **Live devnet deployment** with on‑chain mint/burn and live telemetry
- **17‑round security audit cycle** (two ZERO‑finding convergences)
- **npm‑published MCP server** for external AI tooling integration

---

## Architecture

```mermaid
flowchart TB
  U[User (Phantom)] --> P[On-chain Program (Anchor/Rust)]
  P --> V[Collateral Vaults (USDC/USDT/DAI)]
  P --> M[MSTB Mint/Burn]
  P --> R[Agent Registry (OAE)]
  P --> C[Circuit Breaker]

  P <--> K[Off-chain Keeper (Rust, 14K+ LOC)]
  K --> RM[Risk Manager (gradient descent optimizer)]
  K --> OU[Oracle Updater (Pyth)]
  K --> AS[Agent Scorer (multi-agent consensus)]
  K --> RB[Rebalancer (commit-reveal)]

  K <--> D[Mission Control Dashboard (browser → Solana RPC)]
```

---

## Quick Start (Devnet)

### Keeper
```bash
cd solana
cargo run -p microstable-keeper -- --config keeper/config.devnet.json
```

### Tests
```bash
cd solana/keeper
cargo test -p microstable-keeper
```
Last verified (2026‑02‑23): **123** total tests (58 unit + 65 integration).

---

## Repository Layout

- `solana/programs/` — On‑chain Anchor program
- `solana/keeper/` — Rust keeper daemon (oracle updates, scoring, rebalancing)
- `solana/tests/` — Anchor integration tests
- `mcp-server/` — npm‑published MCP server
- `docs/` — Security reports, whitepaper, specs
- `simulation/` — Archived Python simulation (reference only)

---

## Security

- **Audit report:** [`docs/audit-report.md`](docs/audit-report.md)
- **Bug bounty policy:** [`SECURITY.md`](SECURITY.md)

---

## Tech Stack

- **Solana Program:** Rust + Anchor
- **Keeper:** Rust (14K+ LOC)
- **Oracles:** Pyth
- **Dashboard:** Static HTML/JS (zero‑backend)
- **MCP Server:** npm package for agent integration

---

## License

MIT — see [`LICENSE`](LICENSE).
