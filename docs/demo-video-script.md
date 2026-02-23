# Microstable Demo Video Script (2m 40s)

## Goal
Deliver a concise judge-friendly demo that proves:
1) Microstable is live on Solana devnet, 2) core stablecoin state is observable in real time, 3) AI-governed operations are visible, and 4) users can interact through Mission Control.

## Pre-record checklist (before pressing record)
- Open Mission Control: https://kjaylee.github.io/microstable/
- Prepare one browser tab with Solana Explorer (devnet) for transaction proof
- Have Phantom wallet ready on devnet (funded)
- Keep terminal ready (optional) with keeper command for credibility shot:
  - `cargo run -p microstable-keeper -- --config keeper/config.devnet.json`

---

## Timestamped script

| Time | Screen / action | Talking points (voice-over) |
|---|---|---|
| 0:00–0:10 | **Title slide** (repo + tagline) | "Microstable is a self-evolving multi-collateral stablecoin on Solana, governed by AI agents." |
| 0:10–0:25 | Open **Mission Control header**. Show `DEVNET`, `RPC` badge, and Program ID line. | "This is our live Mission Control dashboard. It reads Solana RPC directly with no backend relay, so what you see is on-chain state in real time." |
| 0:25–0:45 | Focus **Protocol Health** panel. Point to CR, MSTB Supply, Circuit Breaker, collateral weights. | "Here we track collateral ratio, total supply, and safety controls like emergency shutdown and circuit breaker status." |
| 0:45–1:05 | Move to **AI Optimizer** panel. Highlight CR Target, Mint Fee, Redeem Fee, and history chart. | "The optimizer updates risk parameters under bounded rules. Governance is adaptive, but enforcement remains deterministic on-chain." |
| 1:05–1:40 | Go to **Mint Console**. Show wallet connect, pick collateral (USDC), enter amount, click **MINT**. Wait for tx status. | "Now I’ll mint MSTB using collateral from this wallet. The UI shows estimated output, fee context, and transaction status." |
| 1:40–1:55 | Immediately show updated wallet balances in Mint Console. | "After confirmation, balances and protocol state update live without a custom backend." |
| 1:55–2:15 | Scroll to **Agent Arena**. Highlight rank, role, tier, score, status. | "Microstable supports an Open Agent Economy. Agents are ranked and scored, and only qualified agents influence safety-critical decisions." |
| 2:15–2:30 | Show **Live Transaction Feed** table. Click latest signature to open Solana Explorer. | "Every operation is traceable. Judges can verify each transaction directly on Solana devnet Explorer." |
| 2:30–2:40 | Final screen: repo + dashboard URL + program ID. | "Microstable combines on-chain guarantees with AI-native risk adaptation. Code is open-source, live on devnet, and ready for ecosystem integrations." |

---

## What to explicitly show on Mission Control
- Header badges: `DEVNET`, RPC health, program short ID
- **Protocol Health**: CR value, MSTB supply, circuit breaker flag
- **AI Optimizer**: CR target, mint/redeem fees, parameter chart
- **Mint Console**: wallet connection + mint transaction status
- **Agent Arena**: agent tiers/scores/roles
- **Live Transaction Feed**: clickable signatures to Explorer

## Recording notes
- Keep pace brisk (about 140–160 words/minute)
- Avoid deep theory; prioritize visible proof and shipped functionality
- If wallet transaction is slow, keep the take and narrate "pending on devnet confirmation" rather than cutting
