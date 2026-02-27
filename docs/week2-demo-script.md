# Microstable Week 2 — 1-Minute Demo Script

## Goal
Show Week 2 deliverables: Mint/Redeem UI, Agent Registration, Faucet behavior, and devnet validation.

---

## 0:00–0:08 — Intro + Live URL
- Open: `https://kjaylee.github.io/microstable/`
- Narration: “This is Microstable Mission Control running live on Solana devnet.”

## 0:08–0:18 — Program + Health Context
- Point to Program ID in docs/repo context:
  - `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- Briefly mention: keeper is running continuously on MiniPC.

## 0:18–0:32 — Mint / Redeem UX
- Show **Mint Console**: collateral, amount, estimate, fee/oracle.
- Show **Redeem Console**: MSTB amount, expected collateral out, basket payout.
- Narration: “Both actions are wired to devnet transaction builders with wallet-aware validation.”

## 0:32–0:45 — Agent Registration
- Scroll to **Agent Registration** panel.
- Show role selection + stake input + register action button.
- Narration: “Agents can register on-chain with role and stake; registry updates are reflected in the dashboard.”

## 0:45–0:54 — Faucet behavior
- Show faucet area and fallback explanation.
- Narration: “When token faucet instruction is unavailable, the UI falls back to devnet SOL airdrop with safe button lock + status messages.”

## 0:54–1:00 — Close
- Recap: “Week 2 deliverables complete: Mint, Redeem, Agent Registration, and faucet UX—all validated against devnet.”
- End on dashboard and repo link.
