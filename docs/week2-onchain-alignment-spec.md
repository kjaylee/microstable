# Week2 On-chain Alignment Spec (Mint Authority + register_agent seed)

## Context
Devnet E2E intermittently fails in two places:
1) `mint` pre-account validation when `mstb_mint` authority is not `protocol_state` PDA.
2) `register_agent` when client-side `agent_escrow` PDA seed does not match deployed on-chain seed convention.

## Scope
- Diagnose current devnet authority/seed expectations.
- Patch client-side devnet E2E script to auto-detect and align with on-chain reality.
- Keep on-chain program constraints explicit (no blind bypass).

## In-Scope Outcomes
- Mint path returns one of:
  - success with minted delta > 0, or
  - explicit blocker reason (`mint authority signer mismatch`, oracle degraded, etc.).
- Agent registration path returns one of:
  - success with selected seed label + tx signature, or
  - explicit seed mismatch blocker after probing candidates.

## Non-Goals
- Forced program redeploy on devnet.
- Changing tokenomics or keeper governance logic.

## Root-cause hypotheses to verify
- `mstb_mint` authority currently != `protocol_state` (or requires non-local signer).
- Deployed program seed is one of legacy/global variants; client used a different variant.

## Acceptance
- `docs/week2-onchain-alignment-spec.md` and `docs/week2-onchain-alignment-tc.md` exist.
- E2E re-check run on MiniPC with JSON evidence.
- Result clearly states “Mint succeeded” or exact blocking reason.
