# Purple Team v2 Report — Post-Integration Vulnerability Hunt

## Scope & Method
- Target repo: `/Users/kjaylee/.openclaw/workspace/microstable/`
- Priority surfaces reviewed:
  - `solana/programs/microstable/src/lib.rs`
  - Pyth integration (`set_pyth_feed`, `update_oracle_pyth`, parser path)
  - SPL token mint/redeem flow
  - `migrate_legacy_state`
  - Keeper quorum / shutdown controls
  - `solana/tests/devnet-e2e.ts` coverage
  - Python simulation layer (`open_agent_economy.py`, `microstable.py`, `protocol_resilience.py`)
  - `devnet-tokens.json`
- Baseline de-dup source: `docs/purple-team-report.md` (v1, PT-001..PT-027)

## Summary
- **New findings (non-duplicated vs v1): 28**
- Severity split:
  - **CRITICAL: 2**
  - **HIGH: 13**
  - **MEDIUM: 9**
  - **LOW: 4**

---

## Findings

### PTV2-001 — `update_oracle_pyth` missing keeper quorum authorization
- **Severity:** CRITICAL
- **Affected:** `solana/programs/microstable/src/lib.rs:381-423`, `1367-1388`
- **Attack scenario:**
  1. Any external signer calls `update_oracle_pyth`.
  2. Program updates vault oracle state without keeper signatures.
  3. Unauthorized caller can force oracle/circuit-breaker state transitions.
- **PoC evidence:**
```rust
pub fn update_oracle_pyth(ctx: Context<UpdateOraclePyth>, collateral_index: u8) -> Result<()> {
    require!(!ctx.accounts.protocol_state.emergency_shutdown, ...);
    // no require_keeper_quorum(...)
}

pub struct UpdateOraclePyth<'info> {
    ...
    pub pyth_price_account: UncheckedAccount<'info>,
    // no keeper signer fields
}
```
- **Estimated impact:** Unauthorized oracle writes; potential mint/redeem liveness and risk-control manipulation.

### PTV2-002 — Stale price replay via `posted_slot` freshness check (ignores `publish_time`)
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:1767-1775`, `1957-1953`, `1993`
- **Attack scenario:**
  1. Adversary republishes an old but valid Pyth update.
  2. Receiver account gets fresh `posted_slot` while `publish_time` is old.
  3. Program accepts update as fresh because staleness uses `posted_slot` only.
- **PoC evidence:**
```rust
struct RawPythPriceFeedMessage { ... publish_time: i64, ... }
...
Ok((price, confidence, price_update.posted_slot))
...
current_slot.saturating_sub(observed_slot) <= ORACLE_STALENESS_MAX
```
- **Estimated impact:** Oracle freshness guarantees can be bypassed; stale prices can influence mint/redeem and breaker logic.

### PTV2-003 — Pyth `feed_id` in message is never validated
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:1767-1773`, `1957-1993`
- **Attack scenario:**
  1. Vault is pointed to a receiver-owned account.
  2. Account message `feed_id` differs from intended collateral feed.
  3. Program still accepts the price because `feed_id` is parsed but unused.
- **PoC evidence:**
```rust
struct RawPythPriceFeedMessage { feed_id: [u8; 32], ... }
// read_pyth_price_update never checks feed_id against expected value
```
- **Estimated impact:** Feed spoof/mis-binding risk at parser layer; wrong market data can be consumed.

### PTV2-004 — Pyth `write_authority` parsed but never authenticated
- **Severity:** MEDIUM
- **Affected:** `solana/programs/microstable/src/lib.rs:1779-1783`, `1957-1993`
- **Attack scenario:**
  1. Price update payload includes arbitrary `write_authority` metadata.
  2. Program does not bind it to an expected authority.
  3. Trust model relies only on owner+deserialize path.
- **PoC evidence:**
```rust
struct RawPythPriceUpdateV2 {
    write_authority: Pubkey,
    ...
}
// no validation of write_authority
```
- **Estimated impact:** Reduced provenance guarantees for accepted oracle updates.

### PTV2-005 — `set_pyth_feed` accepts arbitrary feed pubkeys (no asset-level whitelist)
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:347-378`
- **Attack scenario:**
  1. Keeper quorum sets collateral feed to any non-default pubkey.
  2. Feed may correspond to wrong instrument/market.
  3. Subsequent `update_oracle_pyth` consumes mis-bound data.
- **PoC evidence:**
```rust
require!(pyth_price_feed != Pubkey::default(), ErrorCode::PythFeedNotConfigured);
// no check against approved feed IDs per collateral_index
```
- **Estimated impact:** Mispricing risk from configuration abuse or operator error.

### PTV2-006 — Same Pyth feed can be assigned to multiple collateral vaults
- **Severity:** MEDIUM
- **Affected:** `solana/programs/microstable/src/lib.rs:362-367`
- **Attack scenario:**
  1. Keeper sets identical feed account for multiple vaults.
  2. Distinct collateral assets now share one oracle source.
  3. Single-feed failure/manipulation cascades across basket.
- **PoC evidence:**
```rust
match collateral_index {
  0 => vault_usdc.pyth_price_feed = pyth_price_feed,
  1 => vault_usdt.pyth_price_feed = pyth_price_feed,
  ...
}
// no uniqueness constraint
```
- **Estimated impact:** Correlated oracle failure domain expansion.

### PTV2-007 — `migrate_legacy_state` is re-runnable (no one-time migration lock)
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:139-270`
- **Attack scenario:**
  1. Trusted initializer invokes migration after protocol is live.
  2. State structs are overwritten again.
  3. Critical protocol fields are reset regardless of current state.
- **PoC evidence:**
```rust
pub fn migrate_legacy_state(...) -> Result<()> {
  ...
  write_anchor_account(&ctx.accounts.protocol_state.to_account_info(), &protocol)?;
  ...
  write_anchor_account(&ctx.accounts.circuit_breaker.to_account_info(), &circuit)?;
}
```
- **Estimated impact:** Re-initialization vector; governance and accounting instability.

### PTV2-008 — Migration resets liabilities/assets accounting to zero
- **Severity:** CRITICAL
- **Affected:** `solana/programs/microstable/src/lib.rs:178-183`, `1849-1851`, `229-268`
- **Attack scenario:**
  1. Migration is called on active protocol.
  2. `total_supply` and vault `total_deposits` are rewritten to `0`.
  3. SPL token balances in ATAs remain, creating ledger divergence.
- **PoC evidence:**
```rust
let protocol = ProtocolState { ... total_supply: 0, ... };
...
CollateralVault { ... total_deposits: 0, ... }
```
- **Estimated impact:** State corruption and severe accounting desync across collateral/supply.

### PTV2-009 — Migration can remap vault collateral mints arbitrarily
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:229-268`, `1830-1843`
- **Attack scenario:**
  1. Migration caller supplies replacement mint accounts.
  2. Vault structs are rebuilt with attacker/operator-provided mints.
  3. Collateral identity basis changes post-deployment.
- **PoC evidence:**
```rust
migrate_vault_account(... ctx.accounts.usdc_mint.key(), ...)
...
CollateralVault { mint, vault: get_associated_token_address(&protocol_authority, &mint), ... }
```
- **Estimated impact:** Collateral substitution risk and downstream valuation breakage.

### PTV2-010 — Migration can replace keeper set unilaterally
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:141-154`, `184`
- **Attack scenario:**
  1. Trusted initializer calls migration with a new keeper set.
  2. Protocol keeper quorum changes immediately.
  3. Control plane can be transferred without on-chain delay.
- **PoC evidence:**
```rust
pub fn migrate_legacy_state(..., keeper_set: [Pubkey; 3])
...
keeper_set,
```
- **Estimated impact:** Sudden governance/control takeover if initializer key is compromised.

### PTV2-011 — No decimal invariants for collateral mints
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:72-111`, `229-268`, `497-515`, `2069-2092`
- **Attack scenario:**
  1. Vault mint has unexpected decimals (e.g., 9 instead of 6).
  2. Mint/redeem math uses raw token units with price ppm.
  3. Resulting µSD amounts are mis-scaled.
- **PoC evidence:**
```rust
let gross_musd = mul_div_floor(collateral_amount, price, SCALE)?;
// no check that collateral_mint.decimals == expected_decimals
```
- **Estimated impact:** Over-/under-mint risk from decimal mismatch.

### PTV2-012 — MSTB SPL mint is not enforced in mint/redeem accounting
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:425-651`, `653-869`, `1390-1502`
- **Attack scenario:**
  1. Protocol mints internal `usd_balance` only.
  2. No SPL mint-to or burn operation for MSTB token occurs.
  3. External MSTB token supply can diverge from protocol liabilities.
- **PoC evidence:**
```rust
user_position.usd_balance = user_position.usd_balance.checked_add(minted_musd)?;
ctx.accounts.protocol_state.total_supply = new_supply;
// no MSTB mint account, no token::mint_to, no token::burn
```
- **Estimated impact:** Supply accounting split between internal ledger and token layer.

### PTV2-013 — Single-keeper emergency shutdown (1-of-3) can halt protocol
- **Severity:** HIGH
- **Affected:** `solana/programs/microstable/src/lib.rs:1202-1214`
- **Attack scenario:**
  1. One compromised keeper key calls `emergency_shutdown`.
  2. `emergency_shutdown=true`, `mint_rate_limit=0` applied.
  3. Critical operations become unavailable.
- **PoC evidence:**
```rust
require_keeper_member(&ctx.accounts.protocol_state, ctx.accounts.keeper.key())?;
protocol.emergency_shutdown = true;
```
- **Estimated impact:** Single-key protocol-wide DoS.

### PTV2-014 — No recovery path from emergency shutdown
- **Severity:** MEDIUM
- **Affected:** `solana/programs/microstable/src/lib.rs:286-289`, `382-385`, `433-435`, `660-662`, `883-885`, `918-920`, `1002-1005`, `1120-1123`
- **Attack scenario:**
  1. Shutdown is triggered.
  2. Most state-changing instructions reject while `emergency_shutdown` is true.
  3. No opposite instruction exists to clear shutdown.
- **PoC evidence:**
```rust
require!(!ctx.accounts.protocol_state.emergency_shutdown, ErrorCode::EmergencyShutdownActive);
// repeated across major instructions; no "resume" instruction present
```
- **Estimated impact:** Potential permanent liveness loss.

### PTV2-015 — Keeper set cannot be rotated post-deploy
- **Severity:** MEDIUM
- **Affected:** `solana/programs/microstable/src/lib.rs:41-70`, `139-190`, `1570-1584`
- **Attack scenario:**
  1. Keeper key is leaked/lost.
  2. No normal instruction exists to rotate keeper_set.
  3. System remains exposed to compromised key material.
- **PoC evidence:**
```rust
keeper_set is only written in initialize/migrate_legacy_state; no dedicated set_keeper_set instruction.
```
- **Estimated impact:** Long-lived governance key compromise risk.

### PTV2-016 — CB1 activation ratchets `cr_target` upward without symmetric rollback
- **Severity:** MEDIUM
- **Affected:** `solana/programs/microstable/src/lib.rs:1083-1088`, `1171-1178`, `2149-2157`
- **Attack scenario:**
  1. CB1 is repeatedly activated under noisy depeg conditions.
  2. Each activation increases `cr_target` by 50,000.
  3. Recovery path does not reduce `cr_target`.
- **PoC evidence:**
```rust
ctx.accounts.protocol_state.cr_target = ctx.accounts.protocol_state.cr_target.checked_add(50_000)?;
// no corresponding decrement in recover_circuit_breaker for cb_index 1
```
- **Estimated impact:** Progressive hardening spiral; minting can become structurally suppressed.

### PTV2-017 — Forced max-duration recovery may relax CB protections under ongoing stress
- **Severity:** MEDIUM
- **Affected:** `solana/programs/microstable/src/lib.rs:2269-2276`, `2295-2310`
- **Attack scenario:**
  1. Breaker remains active until `max_activation_duration` is hit.
  2. `refresh_circuit_breakers` force-transitions to Recovery automatically.
  3. Mint-rate ramp logic may resume despite unresolved adverse conditions.
- **PoC evidence:**
```rust
if is_active_like(...) && slot - activation_tick >= max_activation_duration {
  status = Recovery;
}
...
if circuit.status[1] == Recovery { circuit.mint_rate_limit = ... }
```
- **Estimated impact:** Premature risk-control relaxation, potential under-collateralized mint windows.

### PTV2-018 — `set_public_key` allows unauthenticated identity/key takeover in ACP layer
- **Severity:** HIGH
- **Affected:** `open_agent_economy.py:289-293`
- **Attack scenario:**
  1. Attacker calls `set_public_key(victim, attacker_key)`.
  2. Registry now verifies victim ACP messages with attacker-chosen key.
  3. Attacker signs forged victim messages that pass verify.
- **PoC evidence:**
```python
reg.set_public_key('victim','attacker_known_key')
msg = ACPMessage.create('vote', {'agent_id':'victim'}, 'm1', 'attacker_known_key', epoch=1, expiry_epoch=2, nonce='n1')
ACPMessage.verify(msg, registry=reg, now_epoch=1, expected_epoch=1)  # True
```
- **Estimated impact:** Agent identity hijack in control-plane messaging.

### PTV2-019 — Legacy ACP replay path still enabled by default
- **Severity:** HIGH
- **Affected:** `open_agent_economy.py:585-607`
- **Attack scenario:**
  1. Adversary captures a legacy ACP message without nonce/expiry fields.
  2. Verifier called with default args (`allow_legacy=True`).
  3. Same signed message replays indefinitely.
- **PoC evidence:**
```python
legacy_verify1 = ACPMessage.verify(legacy_msg, secret='s')  # True
legacy_verify2 = ACPMessage.verify(legacy_msg, secret='s')  # True (replay)
```
- **Estimated impact:** Replay of privileged ACP operations in legacy compatibility mode.

### PTV2-020 — `claim_reward` accepts NaN and poisons staking/accounting state
- **Severity:** HIGH
- **Affected:** `open_agent_economy.py:485-503`
- **Attack scenario:**
  1. Caller submits `amount=float('nan')` with unique claim_id.
  2. Comparisons/caps are bypassed due NaN semantics.
  3. Balance and epoch accounting become NaN.
- **PoC evidence:**
```python
ok = staking.claim_reward('a', float('nan'), 'cid-nan', 1, proof=None)
# ok == True, balances['a'] == nan, claimed_by_epoch[1] == nan
```
- **Estimated impact:** Economic engine state corruption and downstream logic failure.

### PTV2-021 — Insurance fund supports implicit infinite refill loop + Sybil cooldown bypass
- **Severity:** HIGH
- **Affected:** `microstable.py:1744-1747`, `1756-1758`, `1768`
- **Attack scenario:**
  1. Attacker rotates claimant IDs each tick (`user0`,`user1`,...).
  2. Per-claimant cooldown is bypassed.
  3. Auto-refill triggers repeatedly when treasury dips below threshold.
- **PoC evidence:**
```python
claim 0 -> treasury 190000.0
claim 1 -> treasury 320000.0  # auto-refill happened
claim 2 -> treasury 250000.0
```
- **Estimated impact:** Treasury drain dynamics and unbounded liability growth.

### PTV2-022 — `commit_proof` in Keeper flow is fully predictable (no secrecy)
- **Severity:** MEDIUM
- **Affected:** `microstable.py:1228-1230`, `1265-1267`
- **Attack scenario:**
  1. Attacker observes `market_epoch` and `market_state_hash`.
  2. Constructs expected `commit_proof` deterministically.
  3. Cumulative-turnover gate accepts proof without cryptographic secrecy.
- **PoC evidence:**
```python
proposal['commit_proof'] == f"{state.market_epoch}:{state.market_state_hash}"  # True
```
- **Estimated impact:** Commit-reveal style gate becomes forgeable at application layer.

### PTV2-023 — E2E omits `update_oracle_pyth` authorization test
- **Severity:** MEDIUM
- **Affected:** `solana/tests/devnet-e2e.ts:186-201`
- **Attack scenario:**
  1. Access-control regression lands in `update_oracle_pyth`.
  2. E2E still passes because only `updateOracle` path is tested.
  3. Unauthorized oracle write bug reaches deployment undetected.
- **PoC evidence:**
```ts
for (let i = 0; i < 4; i += 1) {
  await program.methods.updateOracle(...)
}
// no program.methods.updateOraclePyth(...) coverage
```
- **Estimated impact:** High-risk auth regressions evade CI.

### PTV2-024 — E2E configures Pyth feeds for only 3/4 collateral assets
- **Severity:** LOW
- **Affected:** `solana/tests/devnet-e2e.ts:169-171`, `68-70`
- **Attack scenario:**
  1. USDS feed path remains unconfigured/unvalidated by integration test.
  2. Production assumptions diverge from test assumptions.
  3. Uncovered collateral path fails or degrades at runtime.
- **PoC evidence:**
```ts
const pythFeeds = [PYTH_USDC_USD, PYTH_USDT_USD, PYTH_DAI_USD]; // only 3
```
- **Estimated impact:** Oracle integration blind spot for one collateral leg.

### PTV2-025 — Devnet E2E key material loading is path-substitution prone
- **Severity:** LOW
- **Affected:** `solana/tests/devnet-e2e.ts:16-18`, `30-31`
- **Attack scenario:**
  1. Attacker with host access writes malicious keypair file at expected path (`/tmp/keeper2.json`).
  2. Test run loads attacker-controlled signer material.
  3. CI/devnet assertions no longer represent intended keeper trust set.
- **PoC evidence:**
```ts
const KEEPER2_KEYPAIR = "/tmp/keeper2.json";
Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))))
```
- **Estimated impact:** Test integrity compromise; false confidence in signer/quorum behavior.

### PTV2-026 — Devnet E2E mints collateral using test wallet as mint authority
- **Severity:** LOW
- **Affected:** `solana/tests/devnet-e2e.ts:140-147`
- **Attack scenario:**
  1. Test wallet is also collateral mint authority.
  2. Unlimited collateral can be minted in test flow.
  3. Authority-hardening defects remain hidden.
- **PoC evidence:**
```ts
await mintTo(provider.connection, main, USDC_MINT, userUsdcAta.address, main, ...)
```
- **Estimated impact:** Coverage distortion for SPL authority risks.

### PTV2-027 — `devnet-tokens.json` exposes full keeper identity set
- **Severity:** LOW
- **Affected:** `devnet-tokens.json:10-14`
- **Attack scenario:**
  1. Attacker scrapes keeper public keys from repo.
  2. Targets operators with tailored phishing/social-engineering payloads.
  3. Increases key-compromise probability for quorum-controlled actions.
- **PoC evidence:**
```json
"keeper_set": ["3fime...", "2CuN...", "2gAL..."]
```
- **Estimated impact:** Elevated operational attack surface against keeper operators.

### PTV2-028 — `devnet-tokens.json` lacks USDS Pyth feed mapping while protocol has 4 vaults
- **Severity:** MEDIUM
- **Affected:** `devnet-tokens.json:7`, `21-34`
- **Attack scenario:**
  1. Deployment automation relies on this mapping file.
  2. USDS feed is omitted; only 3 feed IDs/accounts are provided.
  3. One collateral oracle remains unbound/misconfigured.
- **PoC evidence:**
```json
"mints": { "usdc":..., "usdt":..., "dai":..., "usds":... }
"feeds": { "usdc_usd":..., "usdt_usd":..., "dai_usd":... } // no usds_usd
```
- **Estimated impact:** Collateral oracle coverage gap and runtime operational risk.

---

## Notes on non-duplication
This report intentionally excludes v1 items PT-001..PT-027. In particular, it does **not** re-report:
- commit overwrite issue,
- split-rebalance threshold bypass (Rust),
- `risk_score` unused in Rust valuation,
- global mint halt on any degraded oracle.

All findings above are additional surfaces or distinct variants introduced/exposed in post-v1 integration paths.
