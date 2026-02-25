# Microstable Week 2 — Test Cases (Mint/Redeem/Agent Registration)

## Scope
- Dashboard file: `docs/index.html`
- Client logic: `docs/app.js`
- Network: Solana Devnet RPC only
- Wallet: Phantom (browser extension)

## 1) UI Rendering

### TC-W2-UI-001: Redeem Console panel exists
**Steps**
1. Open `docs/index.html` in browser.
2. Scroll below Mint Console.

**Expected**
- `Redeem Console` panel is visible.
- Contains collateral selector (USDC/USDT/DAI), MSTB amount input, MAX button.
- Shows estimate block (estimated output, fee, oracle price, basket payout).
- `REDEEM` button and transaction status line are visible.

### TC-W2-UI-002: Agent Registration panel exists
**Steps**
1. Open dashboard.
2. Locate `Agent Registration` panel.

**Expected**
- Agent type selector includes Optimizer/Monitor/Auditor/Liquidator.
- Stake amount input and `REGISTER AGENT` button are visible.
- Status text exists.
- On-chain registration preview table renders.

### TC-W2-UI-003: Agent Arena link is functional
**Steps**
1. In Agent Arena, click `Register Your Agent →`.

**Expected**
- Browser navigates to `#agentRegistrationPanel` section.

## 2) Wallet Flow

### TC-W2-WALLET-001: Connect/Disconnect updates action states
**Steps**
1. Click `Connect Wallet` and approve in Phantom.
2. Click `Disconnect`.

**Expected**
- On connect: wallet address appears, Mint/Redeem/Agent status shows connected, buttons can enable on valid input.
- On disconnect: statuses change to disconnected and action buttons disable.

### TC-W2-WALLET-002: Balance refresh updates Mint/Redeem views
**Steps**
1. Connect wallet.
2. Wait for polling cycle or trigger actions causing refresh.

**Expected**
- Wallet balances update for USDC/USDT/DAI/MSTB.
- Mint MAX and Redeem MAX use refreshed balances.

## 3) Transaction Building (Client-side)

### TC-W2-TX-001: Mint transaction
**Steps**
1. Connect wallet.
2. Enter mint amount and select collateral.
3. Click `MINT`.

**Expected**
- Client constructs Anchor-style `mint` instruction data:
  - discriminator `global:mint`
  - args: `collateral_index (u8)`, `collateral_amount (u64)`, `max_price (u64)`
- Includes required accounts (protocol, circuit breaker, vault PDAs, user position, ATAs, mint/program accounts).
- Shows pending/confirmed/error status.
- On success: clears input and refreshes balances.

### TC-W2-TX-002: Redeem transaction
**Steps**
1. Connect wallet with MSTB balance.
2. Enter redeem amount and click `REDEEM`.

**Expected**
- Client constructs `redeem` instruction data:
  - discriminator `global:redeem`
  - args: `musd_amount (u64)`, `min_out_amount (u64)`
- Includes required redeem accounts (all vault/user collateral ATAs, mints, user position, token/ATA programs).
- Handles missing user position / insufficient MSTB with clear status.
- On success: clears input and refreshes balances.

### TC-W2-TX-003: Register agent transaction
**Steps**
1. Connect wallet.
2. Select role and stake amount (>= 1 SOL).
3. Click `REGISTER AGENT`.

**Expected**
- Client derives PDAs:
  - `agent_record`: seeds `["agent", wallet]`
  - `agent_escrow` primary: seeds `["v2:agent_escrow", wallet]`
  - compatibility fallback: seeds `["agent_escrow"]` when devnet program returns `ConstraintSeeds` for `agent_escrow`
- Builds `register_agent` instruction data:
  - discriminator `global:register_agent`
  - args: `role (u8)`, `stake_amount (u64 lamports)`
- Handles already-registered and insufficient-SOL states.
- On success: poll refresh shows new/updated on-chain registry data.

## 4) Faucet behavior

### TC-W2-FAUCET-001: Devnet fallback airdrop
**Steps**
1. Connect wallet.
2. Click any faucet button (USDC/USDT/DAI).

**Expected**
- If token faucet instruction is unavailable, UI explains fallback.
- Button triggers 1 SOL devnet airdrop request and confirmation status.
- Buttons disable while airdrop is in progress.

## 5) Static Verification run for this update
- `node --check docs/app.js` passes (no syntax errors).
- Manual structure check confirms new IDs used by JS exist in `docs/index.html`:
  - Redeem: `redeemCollateral`, `redeemAmount`, `redeemMaxBtn`, `redeemSubmitBtn`, `redeemTxStatus`
  - Agent registration: `agentRole`, `agentStake`, `agentRegisterBtn`, `agentRegisterStatus`, `agentRegistryRows`

## 6) Executable runbook (used for 2026-02-25 verification)
1. Static syntax / DOM ID check
   - `node --check docs/app.js`
   - `grep` ID presence check against `docs/index.html`
   - Evidence: `docs/evidence/week2-e2e-20260225/verification.log`
2. Devnet state check
   - `curl` JSON-RPC `getAccountInfo` for Program ID
   - `curl` JSON-RPC `getHealth`
   - `curl -I https://kjaylee.github.io/microstable/`
   - Evidence: `program-account.json`, `rpc-health.json`, `dashboard-head.txt`
3. Browser live dashboard check
   - Open dashboard and capture full-page screenshots
   - Evidence: `docs/evidence/week2-e2e-20260225/screenshot-*.jpg`
4. Tx builder / simulation check (CLI reproducible)
   - `node scripts/week2-e2e-devnet-check.js docs/evidence/week2-e2e-20260225/e2e-result-v2.json`
   - Evidence: `e2e-run-v2.log`, `e2e-result-v2.json`
5. Blocker-focused probes
   - Mint authority mismatch proof (`MSTB mint authority` vs `protocol_state PDA`)
   - Register-agent seed mismatch probe (`register-seed-probe.log`)
   - Devnet faucet 429 proof (`verification.log`)
