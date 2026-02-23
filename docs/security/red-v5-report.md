## Attack RV5-001: Unauthorized Oracle Write Without Keeper Quorum
- Category: Access Control Bypass
- Target: `solana/programs/microstable/src/lib.rs:527-539, 1836-1857, 2902-2911`
- Vector: Submit `update_oracle` using non-keeper signer(s), or one keeper + outsider, to force arbitrary prices.
- PoC: 1) Build `update_oracle(collateral_index=0, price=1_500_000, confidence=1, observed_slot=current_slot)` tx. 2) Provide `keeper_one=<attacker>`, `keeper_two=<outsider>` signatures. 3) Send tx.
- Result: FAILED
- Defense: `require_keeper_quorum` enforces both signers are distinct members of `protocol.keeper_set`; invalid combos revert with `KeeperQuorumNotMet`/`DuplicateKeeperSigner`.

## Attack RV5-002: Duplicate-Signer Quorum Forgery
- Category: Access Control Bypass
- Target: `solana/programs/microstable/src/lib.rs:2902-2911`
- Vector: Reuse the same keeper key for both quorum slots to fake 2-of-3.
- PoC: 1) Build any privileged ix (e.g., `rebalance`). 2) Pass same pubkey into `keeper_one` and `keeper_two`. 3) Sign once and submit.
- Result: FAILED
- Defense: Explicit `require!(signer_a != signer_b, ErrorCode::DuplicateKeeperSigner)` blocks duplicate-quorum abuse.

## Attack RV5-003: Instant Keeper-Set Takeover (No Timelock)
- Category: Access Control Bypass
- Target: `solana/programs/microstable/src/lib.rs:1616-1642, 2878-2889`
- Vector: Attempt to rotate to attacker-controlled keeper set in one step.
- PoC: 1) Submit `rotate_keeper_set(new_set=attacker_triplet)` once with current quorum. 2) Immediately submit same instruction again in same/next slot to force activation.
- Result: FAILED
- Defense: First call only stages `pending_keeper_set`; second call requires slot >= `pending_keeper_activation_slot` (`KEEPER_ROTATION_DELAY_SLOTS` timelock).

## Attack RV5-004: Collateral Mint/ATA Spoof for Free MSTB
- Category: Economic Exploits
- Target: `solana/programs/microstable/src/lib.rs:821-878, 1909-1954`
- Vector: Provide attacker-controlled mint/token accounts as collateral path while minting real MSTB.
- PoC: 1) Call `mint` with `collateral_index=0` but pass fake `collateral_mint` and fake vault ATA. 2) Try bypassing with non-canonical user ATA.
- Result: FAILED
- Defense: Instruction enforces expected vault mint binding and canonical ATAs (`require_keys_eq!` + Anchor associated token constraints), then executes SPL `transfer_checked` under user authority.

## Attack RV5-005: Dynamic Fee Bypass (Protocol Params Not Enforced in Mint/Redeem)
- Category: Economic Exploits
- Target: `solana/programs/microstable/src/lib.rs:107-111, 801-808, 1097-1132, 2914-2938`
- Vector: Governance updates `mint_fee_rate`/`redeem_fee_rate`, but execution path still uses legacy `fee_rate` for mint and no explicit redeem fee deduction.
- PoC: 1) Keeper quorum calls `update_protocol_params(new_mint_fee=10_000, new_redeem_fee=10_000)`. 2) User calls `mint`; fee is computed from `protocol_state.fee_rate` (unchanged legacy field). 3) User calls `redeem`; payout path has no `redeem_fee_rate` deduction branch.
- Result: SUCCESS
- Severity: MEDIUM
- Impact: Fee-governance controls are partially ineffective; protocol can under-collect fees versus configured risk posture.

## Attack RV5-006: Sybil Tournament Capture via Stake Splitting + Predictable Sampling
- Category: Sybil Attacks
- Target: `solana/programs/microstable/src/lib.rs:347-353, 3316-3320`; `solana/keeper/src/agent_loop.rs:285-302, 377-433, 436-471`
- Vector: Register many minimum-stake agent identities; tournament picks two participants without identity-level anti-sybil controls, weighted only by per-record stake.
- PoC: 1) Register N agent accounts at min stake (0.1 SOL each). 2) Keep all active. 3) Observe deterministic slot-seeded sampling (`hashv(slot_seed, nonce)`) and enter rounds where attacker identities dominate both participant slots. 4) Keeper applies score updates to sampled agents.
- Result: SUCCESS
- Severity: HIGH
- Impact: Cheap identity fan-out can disproportionately control tournament/AIG score progression and downstream agent governance influence.

## Attack RV5-007: 3-Feed Runtime Config Induces 4-Vault Oracle Degradation Mint Halt
- Category: DoS/Liveness
- Target: `solana/keeper/config.devnet.json:12-31`; `solana/keeper/src/config.rs:306-335`; `solana/keeper/src/oracle.rs:166-169`; `solana/programs/microstable/src/lib.rs:760-763, 3048-3053`
- Vector: Keeper runtime updates only configured feeds; config includes 3 feeds while protocol evaluates oracle health across 4 vaults.
- PoC: 1) Run keeper with shipped `config.devnet.json` (USDC/USDT/DAI only). 2) Wait > `ORACLE_STALENESS_MAX` slots for untouched vault (USDS). 3) Attempt user `mint` on any collateral.
- Result: SUCCESS
- Severity: HIGH
- Impact: Global mint path can halt with `OracleDegraded` due single never-refreshed vault, creating sustained liveness failure.

## Attack RV5-008: PM2 Isolation Verification Fail-Open
- Category: DoS/Liveness
- Target: `solana/keeper/scripts/verify-isolation.sh:53-57, 79-85`
- Vector: Operational isolation check reports warnings but does not fail CI/runtime gate.
- PoC: 1) Run keeper in shared PM2 domain with extra processes. 2) Execute `verify-isolation.sh`. 3) Observe `NOT ISOLATED` warning and shell exit code remains 0.
- Result: SUCCESS
- Severity: LOW
- Impact: Isolation regression can pass verification workflow, increasing blast radius for process-level compromise.

## Attack RV5-009: PDA Forgery for Protocol/Vault State Corruption
- Category: State Manipulation
- Target: `solana/programs/microstable/src/lib.rs:1836-1853, 1909-1926, 1973-1990`
- Vector: Supply attacker-owned accounts in place of protocol/vault PDAs to mutate arbitrary state.
- PoC: 1) Build privileged ix with forged accounts not matching expected PDA seeds. 2) Submit tx with valid signers.
- Result: FAILED
- Defense: Anchor account constraints pin protocol/circuit/vault accounts to canonical PDA seeds + bumps; mismatched accounts fail before instruction logic.

## Attack RV5-010: Pyth Feed Substitution / Counterfeit Account Injection
- Category: Oracle Manipulation
- Target: `solana/programs/microstable/src/lib.rs:601-622, 2699-2714, 2754-2784`
- Vector: Swap configured feed to attacker account or pass forged Pyth account payload.
- PoC: 1) Attempt `set_pyth_feed` with non-allowlisted feed pubkey. 2) Attempt `update_oracle_pyth` using account with wrong owner/feed-id/write_authority.
- Result: FAILED
- Defense: Per-collateral feed allowlist, receiver owner check, feed-id binding, and write-authority validation reject spoofed feeds.

## Attack RV5-011: Stale or Replayed Oracle Update Acceptance
- Category: Oracle Manipulation
- Target: `solana/programs/microstable/src/lib.rs:16-18, 2715-2734`
- Vector: Replay old-but-signed Pyth updates to bias pricing.
- PoC: 1) Submit `update_oracle_pyth` using update with `publish_time` older than 60s or stale posted slot. 2) Try future timestamp skew.
- Result: FAILED
- Defense: On-chain freshness checks enforce `publish_time <= now`, age <= 60s, and slot staleness bound; stale/future updates revert.

## Attack RV5-012: Single Keeper Key Compromise (Non-Initializer) for Privileged Actions
- Category: Keeper Compromise
- Target: `solana/programs/microstable/src/lib.rs:1262-1267, 1571-1577, 2902-2911`
- Vector: Use one stolen keeper key to execute `rebalance` / `emergency_shutdown` by pairing with attacker key.
- PoC: 1) Sign privileged ix with compromised keeper as `keeper_one`, attacker as `keeper_two`. 2) Submit tx.
- Result: FAILED
- Defense: Privileged flows require two distinct keeper-set members; outsider second signer cannot satisfy quorum.

## Attack RV5-013: Trusted-Initializer Keeper Key Compromise Enables Unilateral Slashing
- Category: Keeper Compromise
- Target: `solana/programs/microstable/src/lib.rs:29, 78, 95-99, 448-463, 2117-2131`
- Vector: `slash_agent` is controlled by single `TRUSTED_INITIALIZER` signer and treasury is fixed to same key.
- PoC: 1) Compromise initializer key (which must be included in keeper set at init). 2) Enumerate agent PDAs. 3) Call `slash_agent` repeatedly with max slash amounts. 4) Redirect seized lamports to protocol treasury (initializer key).
- Result: SUCCESS
- Severity: HIGH
- Impact: Compromise of one special keeper key allows unilateral confiscation of all agent stake and governance suppression.

## Attack RV5-014: Commit/Reveal Front-Run Without Preimage
- Category: Race Conditions
- Target: `solana/programs/microstable/src/lib.rs:1299-1322, 2975-2995`
- Vector: Observe pending commit hash and attempt reveal with guessed salt/weights before legitimate revealer.
- PoC: 1) Monitor pending commit on-chain. 2) Submit `rebalance` with alternative `reveal_salt` and guessed weights in same window.
- Result: FAILED
- Defense: Hash commits include protocol key + full weight vector + batch_slot + salt; mismatch triggers `CommitRevealMismatch`.

## Attack RV5-015: Replay/Timing Abuse of Commit Window
- Category: Race Conditions
- Target: `solana/programs/microstable/src/lib.rs:1306-1315, 1324-1326, 2997-3003`
- Vector: Replay stale reveal after expiry or from prior batch window.
- PoC: 1) Wait past `pending_rebalance_expiry` then send reveal. 2) Reuse old reveal in different batch window.
- Result: FAILED
- Defense: Expiry enforcement (`CommitRevealExpired`), commit clearing after successful reveal, and batch-window validation prevent replay/timing abuse.

## Attack RV5-016: CPI/Reentrancy via Malicious Program Substitution
- Category: CPI/Reentrancy
- Target: `solana/programs/microstable/src/lib.rs:866-878, 1097-1107, 1967-1969`
- Vector: Attempt to swap SPL Token program for attacker-controlled program to trigger callback/reentrant side-effects during mint/redeem CPIs.
- PoC: 1) Build mint/redeem tx with attacker program passed as `token_program`. 2) Attempt CPI-driven reentry/state corruption.
- Result: FAILED
- Defense: Account type `Program<Token>` constrains `token_program` to canonical SPL Token program; no attacker-controlled CPI target is accepted.

## Attack RV5-017: Overflow/Precision Abuse in Mint, Redeem, and Oracle Scaling
- Category: Arithmetic
- Target: `solana/programs/microstable/src/lib.rs:732-735, 2816-2836, 2942-2964`
- Vector: Push extreme amount/exponent combinations to trigger silent wrap or precision underflow.
- PoC: 1) Attempt mint/redeem with near-u64-max logical values. 2) Craft Pyth payload with extreme exponent for scale conversion. 3) Force mul/div operations near boundaries.
- Result: FAILED
- Defense: Hard input caps (`MAX_COLLATERAL_AMOUNT`), checked u128 arithmetic in `mul_div_floor`/`mul_div_ceil`, checked exponent scaling and u64 conversion with explicit overflow errors.
