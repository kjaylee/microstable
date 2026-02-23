# Superteam Brazil — Build the Solana Stablecoin Standard

## Submission focus

Microstable is a live devnet stablecoin system (program + keeper + dashboard) and is well positioned to become a **reference implementation for a Solana stablecoin standard**.

For this bounty, we focus on turning our production-like codebase into reusable public goods:

- standard interfaces for stablecoin lifecycle actions (initialize, oracle update, mint, redeem, rebalance)
- a developer SDK for fast integration
- implementation templates for teams launching new collateralized stablecoins on Solana

## What we will ship for the bounty

1. **Stablecoin Standard Spec (v1)**
   - account model conventions (protocol state, collateral vault, circuit breaker, agent registry)
   - required instruction set and expected invariants
   - event/log recommendations for indexers and analytics tools

2. **TypeScript SDK (reference client)**
   - typed instruction builders and PDA derivation helpers
   - transaction composition utilities for mint/redeem paths
   - devnet-ready examples with clear failure-mode handling

3. **Reference Templates**
   - "minimal stablecoin" starter template
   - "multi-collateral stablecoin" template
   - CI and local validation checklist for safe launches

4. **Compatibility Matrix + Docs**
   - SPL Token vs Token-2022 capability matrix
   - migration notes and extension compatibility guidance
   - integration cookbook for wallets, bots, and dashboards

## Token-2022 compatibility notes

Current Microstable devnet deployment uses SPL Token (classic token program) for operational simplicity.  
For this bounty scope, we provide a **Token-2022 compatibility layer design + implementation path**:

- abstract token program selection at SDK and instruction-builder level
- define extension-safe checks (decimals, mint authority behavior, metadata expectations)
- document compatibility with common Token-2022 extension patterns and integration caveats

This keeps the reference implementation practical today while giving teams a clear path to Token-2022 adoption.

## Why Microstable is a strong reference implementation

- Real running system (not just pseudocode): live on Solana devnet
- Full stack available publicly: on-chain program, off-chain keeper, dashboard, scripts
- Security process already exercised through 17 iterative audit rounds
- Strong testing surface in Rust keeper and integration flows
- MIT-licensed public repository suitable for direct reuse and extension

## Open-source contribution to the ecosystem

This bounty work will contribute reusable infrastructure for the broader Solana ecosystem:

- a common baseline for stablecoin protocol interfaces
- faster onboarding for new teams building stable-value assets
- lower integration costs for wallets, protocols, and agent systems
- transparent, auditable code that can be forked and improved by the community

Microstable’s goal is not only to launch one stablecoin, but to help establish a practical **Solana stablecoin standard** others can build on immediately.
