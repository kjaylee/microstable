# Week2 On-chain Alignment Plan

1. **Environment/Repo confirmation**
   - Confirm candidate repos and choose actual source.
2. **Implement diagnostic E2E client patch**
   - Add mint-authority alignment check (safe, signer-bound).
   - Add register-agent seed probing across candidate seed schemas.
   - Add faucet 429 handling: one retry max.
3. **Execute MiniPC validation**
   - Run script on devnet.
   - Persist JSON run result + human-readable log.
4. **Promote artifacts to Mac Studio repo**
   - `scp` patched files and evidence.
5. **Git commit/push on Mac Studio**
   - Stage only week2 alignment files.

## Risk handling
- If mint authority cannot be changed by current signer: report explicit blocker.
- If faucet repeatedly 429 after one retry: stop retry loop and report blocker.
