# Week2 On-chain Alignment Test Cases

## TC-01 Mint authority precheck
- **Given** devnet MSTB mint + protocol PDA
- **When** script reads `getMint(mstb)`
- **Then** output includes `beforeAuthority`, `expectedAuthority`, and whether signer can align.

## TC-02 Mint authority alignment
- **Given** `beforeAuthority == signer` and `beforeAuthority != protocol_pda`
- **When** script sends `setAuthority(MintTokens -> protocol_pda)`
- **Then** mint authority becomes protocol PDA and tx signature is logged.

## TC-03 Mint execution
- **Given** required ATAs and oracles refreshed
- **When** script sends `mint` ix
- **Then** either `mintedDeltaRaw > 0` or explicit blocker reason is logged.

## TC-04 register_agent seed probe
- **Given** candidate seeds (`v2_wallet`, `legacy_wallet`, `legacy_global`)
- **When** simulation is run per candidate
- **Then** script selects first non-`ConstraintSeeds(agent_escrow)` candidate and logs decision.

## TC-05 register_agent execution
- **Given** selected seed + funded ephemeral signer
- **When** script sends `register_agent`
- **Then** `agent_record` exists and escrow lamports reflect stake transfer, or blocker reason is explicit.

## TC-06 Faucet 429 policy
- **Given** airdrop request returns 429
- **When** script retries
- **Then** retry only once and then reports cause without infinite retry.
