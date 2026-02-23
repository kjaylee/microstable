# Microstable — Hackathon Submission Package

## 1) Project Description (≈500 words)

Microstable is a **self‑evolving, multi‑collateral stablecoin protocol on Solana** designed to stay safe under market stress while remaining open to experimentation. Unlike static stablecoins that rely on fixed parameters or centralized governance, Microstable is governed by **AI agents** that continuously tune risk parameters using **gradient‑based optimization**. The result is a stablecoin that can *learn* and adapt as liquidity, volatility, and collateral quality change — without sacrificing transparency or on‑chain guarantees.

At the core is a fully on‑chain program (Anchor/Rust) that enforces deterministic state, minting and redemption rules, collateral vault accounting, and circuit breakers. This on‑chain layer is paired with a **Rust keeper daemon (14K+ LOC)** that acts as the protocol’s real‑time safety layer: it updates Pyth oracle data, scores participating agents, runs multi‑agent consensus on risk proposals, and executes rebalances through commit‑reveal to prevent manipulation. This “two‑layer” design allows Microstable to remain **trust‑minimized** while still benefiting from off‑chain computation.

Microstable introduces an **Open Agent Economy (OAE)**: any AI agent can participate in governance by proposing parameter updates, risk assessments, or rebalancing strategies. To protect the system from low‑quality or adversarial agents, it includes an **Agent Intelligence Gate (AIG)** that verifies capabilities before agents can influence protocol decisions. This is crucial for a stablecoin that must be resilient to both economic attacks and algorithmic errors.

The project is already **live on Solana devnet**, with a Mission Control dashboard that streams on‑chain state in real time. The dashboard is intentionally **zero‑backend**: the browser polls Solana RPC directly, eliminating a centralized middleware layer. This architecture makes it easy for judges (and future users) to verify the protocol’s behavior end‑to‑end.

Microstable is also engineered for security. The protocol completed a **17‑round adversarial audit cycle**, including two “ZERO findings” convergence milestones. The audits covered both the on‑chain program and keeper logic, with red/purple team iterations and regression verification. A formal audit report and a bug bounty policy are publicly available, reflecting a security‑first philosophy typically reserved for mainnet‑ready systems.

Beyond code, Microstable is designed for ecosystem integration. It ships an **npm‑published MCP server**, making it easy for external AI agents and tooling to connect to the protocol. This enables composability across the wider agent economy, and turns Microstable into a foundational primitive for AI‑native DeFi applications.

In short, Microstable is not just another stablecoin. It is a **living system**: multi‑collateral by design, governed by AI, enforced on‑chain, and continuously tested under adversarial conditions. For hackathon judges, it offers a rare combination of deep technical rigor (Solana program + keeper + audits) and forward‑looking innovation (AI agent governance + open agent economy). It is a stablecoin built for the next generation of autonomous, on‑chain finance.

---

## 2) Technical Architecture Diagram (Mermaid)

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

## 3) Demo Script

### 30‑second Elevator Pitch
“Stablecoins fail when markets move faster than governance. **Microstable** is a self‑evolving multi‑collateral stablecoin on Solana, governed by AI agents that optimize risk parameters in real time using gradient‑based techniques. Anyone can contribute an agent through our **Open Agent Economy**, but only verified agents can influence the system via the **Agent Intelligence Gate**. The protocol is fully on‑chain, paired with a Rust keeper for oracle updates and rebalancing, and is already live on devnet with a zero‑backend Mission Control dashboard. We’ve completed a 17‑round security audit cycle and published an MCP server so external agents can integrate immediately.”

### 2‑minute Demo Flow
1. **Dashboard overview**: Open the Mission Control dashboard and show live on‑chain state (vault balances, MSTB supply, health factors).
2. **Connect wallet**: Connect Phantom and demonstrate devnet wallet readiness.
3. **Mint flow**: Deposit collateral (USDC/USDT/DAI) and mint MSTB; show the resulting transaction on Solana explorer.
4. **Agent Arena**: Navigate to agent governance view and show active agents, scores, and proposed parameter updates.
5. **Live transactions**: Highlight keeper‑driven updates (oracle refreshes, scoring rounds, rebalances).
6. **Optimizer view**: Demonstrate the gradient‑based risk optimizer output and explain how it affects collateral ratios.
7. **Security proof**: Briefly show the audit report and bug bounty policy links.

---

## 4) Submission Checklist

- [x] **GitHub repo (public)**
- [ ] **README with setup instructions**
- [x] **Live demo URL** — https://kjaylee.github.io/microstable/
- [ ] **Demo video** (record screen + voice; 2–3 minutes recommended)
- [ ] **Team info** (names, roles, contact)
- [x] **License** — MIT
