# Microstable — Colosseum Week 2 Submission

## Project
- **Live Dashboard:** https://kjaylee.github.io/microstable/
- **Devnet Program ID:** `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- **Repository:** https://github.com/kjaylee/microstable

---

## 1) Week 2 Deliverables Checklist

- [x] Mint UI flow implemented and wired to devnet tx builder
- [x] Redeem UI flow implemented and wired to devnet tx builder
- [x] Agent Registration UI + tx builder implemented
- [x] Devnet faucet UX fallback implemented (SOL airdrop fallback)
- [x] Live dashboard deployment (GitHub Pages)
- [x] Program ID integrated in dashboard config
- [x] Keeper daemon running against devnet

Validation references:
- `docs/test-cases-week2.md`
- `docs/app.js`
- `docs/index.html`

---

## 2) Mint / Redeem UI Screenshot Notes

Primary screenshot artifacts (existing):
- `docs/video-frames/02-dashboard-full.jpg`
- `docs/video-frames/02-dashboard.png`

What is shown / should be highlighted in submission:
1. **Mint Console**
   - collateral selector (USDC/USDT/DAI)
   - input amount + estimate area
   - fee/oracle readouts
2. **Redeem Console**
   - MSTB amount input + MAX
   - estimated collateral out + fee + basket payout
3. **Wallet-aware states**
   - connected/disconnected guidance
   - tx status line updates

---

## 3) Agent Registration (Week 2)

Implemented UI and transaction flow includes:
- role selection: Optimizer / Monitor / Auditor / Liquidator
- stake amount input in SOL
- register action with status feedback
- on-chain preview table updates after successful registration

Client-side transaction build details:
- derives `agent_record` PDA (`["agent", wallet]`)
- derives `agent_escrow` PDA (`["v2:agent_escrow", wallet]`)
- builds `register_agent` instruction payload with role + stake

Reference test case: `TC-W2-TX-003` in `docs/test-cases-week2.md`.

---

## 4) Faucet Feature (Week 2)

Faucet UX is implemented with explicit fallback behavior:
- if token faucet instruction is unavailable, UI explains fallback path
- fallback requests devnet SOL airdrop and updates status
- buttons lock during in-flight request to avoid duplicate tx submissions

Current dashboard config note (`docs/app.js`):
- `FAUCET_CONFIG.instructionAvailable = false`
- hint: `On-chain faucet instruction needed`

Reference test case: `TC-W2-FAUCET-001`.

---

## 5) Devnet Verification Results

### Dashboard reachability
- `https://kjaylee.github.io/microstable/` responds successfully (HTTP 200).

### Program verification
- Program account check on devnet confirms executable:
  - Public key: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
  - Owner: `BPFLoaderUpgradeab1e11111111111111111111111`
  - Executable: `true`

### Keeper status (MiniPC)
- Running process observed:
  - `/home/spritz/microstable-keeper/solana/target/release/microstable-keeper --config /home/spritz/microstable-keeper/config.devnet.json`

---

## 6) Tech Stack Summary

- **On-chain:** Solana + Anchor (Rust)
- **Keeper:** Rust daemon (off-chain orchestration)
- **Oracle/Data:** Pyth + Solana RPC
- **Dashboard:** Static HTML/CSS/JavaScript (zero-backend)
- **Wallet:** Phantom
- **Infra/Hosting:** GitHub Pages

---

## 7) Links

- GitHub: https://github.com/kjaylee/microstable
- Dashboard: https://kjaylee.github.io/microstable/
- Week 2 Test Cases: `docs/test-cases-week2.md`
