# Microstable Purple-Keeper — Rust Keeper Daemon Vulnerability Audit

## Scope
Audited files:
- `solana/keeper/src/main.rs`
- `solana/keeper/src/oracle.rs`
- `solana/keeper/src/rebalance.rs`
- `solana/keeper/src/monitor.rs`
- `solana/keeper/src/watchdog.rs`
- `solana/keeper/src/wire.rs`
- `solana/keeper/src/config.rs`
- `solana/keeper/src/utils.rs`
- `solana/keeper/config.devnet.json`

Reference checked for alignment:
- `solana/programs/microstable/src/lib.rs`

## Findings Summary
- **CRITICAL:** 2
- **HIGH:** 2
- **MEDIUM:** 5
- **LOW:** 1
- **Total:** 10

## Top 3 Critical/High Issues
1. **PK-001 (CRITICAL):** Keeper fully trusts a single RPC endpoint for security-critical reads and transaction submission, enabling spoofed state to drive privileged keeper signatures.
2. **PK-002 (CRITICAL):** Auto-emergency shutdown is triggerable from a single sampled condition without debounce/hysteresis; combined with spoofed/unstable data it can halt protocol operation.
3. **PK-003 (HIGH):** Rebalance commit/reveal salt is deterministic and slot-derived, undermining secrecy and enabling pre-reveal strategy inference/front-running.

---

### PK-001 — Single-RPC trust enables privileged action forgery
- **Severity:** CRITICAL
- **Category:** E
- **File+Lines:** `solana/keeper/src/main.rs:47`, `solana/keeper/src/oracle.rs:214-216`, `solana/keeper/src/rebalance.rs:33-41,57`, `solana/keeper/src/monitor.rs:27-37,82`, `solana/keeper/src/watchdog.rs:47-56`
- **Attack scenario:**
  1. Attacker controls or MITMs the configured `rpc_url`.
  2. Keeper reads forged protocol/vault/oracle state from that endpoint.
  3. Keeper computes oracle/rebalance/shutdown decisions from forged data.
  4. Keeper signs and submits privileged transactions (2-of-3 keeper keys) based on attacker-controlled view.
- **Impact:** Forced emergency shutdowns, repeated failed ops (DoS), or coerced valid rebalances/oracle writes based on manipulated off-chain decisioning.
- **Evidence:**
  ```rust
  // main.rs
  let rpc = RpcClient::new_with_commitment(cfg.rpc_url.clone(), CommitmentConfig::confirmed());
  ```
  All decision modules (`oracle`, `rebalance`, `monitor`, `watchdog`) consume this same client for authoritative state.

### PK-002 — One-shot auto emergency shutdown trigger (no debounce/hysteresis)
- **Severity:** CRITICAL
- **Category:** D
- **File+Lines:** `solana/keeper/src/monitor.rs:68-73,82-89,116-146`
- **Attack scenario:**
  1. Attacker causes one bad read cycle (RPC spoof, transient oracle distortion, stale vault state).
  2. `global_cr_bps` drops below `emergency_collateral_ratio_bps` for that single cycle.
  3. If `auto_emergency_shutdown=true`, keeper immediately submits `emergency_shutdown`.
  4. On-chain `emergency_shutdown` instruction is quorum-gated but not CR-gated, so the shutdown executes if signatures are valid.
- **Impact:** Protocol halt from a transient/manipulated observation; high blast radius due keeper authority.
- **Evidence:**
  ```rust
  let emergency_condition = protocol.total_supply > 0
      && global_cr_bps < cfg.emergency_collateral_ratio_bps
      && !protocol.emergency_shutdown;

  if emergency_condition && cfg.auto_emergency_shutdown {
      let sig = utils::send_instructions(rpc, k1, &[k1, k2], vec![ix])?;
  }
  ```

### PK-003 — Deterministic reveal salt breaks commit/reveal secrecy
- **Severity:** HIGH
- **Category:** C
- **File+Lines:** `solana/keeper/src/rebalance.rs:153-160,401-407,410-430,370-398`
- **Attack scenario:**
  1. Keeper computes `batch_slot = current_slot + delay`.
  2. `reveal_salt` is deterministically derived from `batch_slot` only.
  3. Observer can enumerate candidate slots and reconstruct commit preimages from public state/algorithm.
  4. Rebalance intent becomes inferable before reveal window, enabling strategic positioning/front-running.
- **Impact:** Commit/reveal privacy objective is weakened; adversaries can anticipate rebalances and extract value.
- **Evidence:**
  ```rust
  fn build_reveal_salt(batch_slot: u64) -> [u8; 32] {
      reveal_salt[..8].copy_from_slice(&batch_slot.to_le_bytes());
      reveal_salt[8..16].copy_from_slice(&batch_slot.rotate_left(17).to_le_bytes());
      ...
  }
  ```

### PK-004 — Keypair loading lacks filesystem hardening checks
- **Severity:** HIGH
- **Category:** A
- **File+Lines:** `solana/keeper/src/utils.rs:65-71`, `solana/keeper/src/config.rs:182-186`
- **Attack scenario:**
  1. Local attacker targets keeper host (shared user/group, writable home mount, symlink tricks).
  2. Key files are read with no owner/mode/symlink validation.
  3. Attacker can replace/read keys under weak host permissions.
  4. Keeper signs privileged transactions with compromised keys.
- **Impact:** Keeper key compromise risk is materially increased; protocol control can be lost if quorum keys are exposed.
- **Evidence:**
  ```rust
  let kp = read_keypair_file(&expanded)
      .map_err(|e| anyhow!("failed to read keypair {}: {e}", expanded.display()))?;
  ```
  No `metadata` checks for `0600`, UID ownership, or symlink rejection before load.

### PK-005 — Quorum selection is static first-two keys (no membership/rotation validation)
- **Severity:** MEDIUM
- **Category:** A
- **File+Lines:** `solana/keeper/src/utils.rs:76-81`, `solana/keeper/src/main.rs:64-66`
- **Attack scenario:**
  1. Keeper config contains reordered, duplicated, stale, or rotated-out keys.
  2. Daemon blindly uses `[0]` and `[1]` for all privileged actions.
  3. Transactions fail repeatedly or use unintended signer subset.
  4. During keeper-set rotation, daemon can silently become nonfunctional until manual intervention.
- **Impact:** Quorum reliability degradation and prolonged liveness failure for critical keeper operations.
- **Evidence:**
  ```rust
  pub fn keeper_quorum(keepers: &[Keypair]) -> Result<(&Keypair, &Keypair)> {
      if keepers.len() < 2 { ... }
      Ok((&keepers[0], &keepers[1]))
  }
  ```

### PK-006 — Per-feed freshness controls are present in config but not enforced
- **Severity:** MEDIUM
- **Category:** B
- **File+Lines:** `solana/keeper/src/config.rs:48-53,62,86,175`, `solana/keeper/src/oracle.rs:130-134`
- **Attack scenario:**
  1. Operator configures strict `max_age_secs` per feed.
  2. Keeper ignores per-feed value and uses global `oracle_publish_max_age_secs`.
  3. Feed-specific stale updates pass local gate if within global threshold.
  4. Stale prices can still drive keeper decisions/tx attempts.
- **Impact:** Security controls appear configured but are ineffective; stale oracle acceptance window may be wider than intended.
- **Evidence:**
  ```rust
  if is_stale(now, observation.publish_time, cfg.oracle_publish_max_age_secs) { ... }
  ```
  `feed.max_age_secs` is parsed but never used in oracle cycle checks.

### PK-007 — Off-chain oracle prevalidation discards feed identity and write authority fields
- **Severity:** MEDIUM
- **Category:** B
- **File+Lines:** `solana/keeper/src/oracle.rs:251-256`, `solana/keeper/src/oracle.rs:230-236,258-263`
- **Attack scenario:**
  1. Attacker-controlled RPC serves crafted decode-compatible payloads from receiver-owned accounts.
  2. Keeper validates only owner + verification level + positivity + freshness/confidence.
  3. `feed_id` and `write_authority` are ignored in keeper-side decisioning.
  4. Keeper can be induced into bad decision paths (spam failed tx, suppress/skip updates, or mis-prioritize feeds).
- **Impact:** Reduced oracle authenticity guarantees at daemon layer; easier decision manipulation/DoS despite on-chain checks.
- **Evidence:**
  ```rust
  let _ = update.write_authority;
  let _ = update.price_message.feed_id;
  ```

### PK-008 — Config has no sanity bounds; malformed values can crash keeper
- **Severity:** MEDIUM
- **Category:** F
- **File+Lines:** `solana/keeper/src/config.rs:163-221`, `solana/keeper/src/main.rs:77`
- **Attack scenario:**
  1. Attacker or misconfiguration sets `tick_interval_secs = 0` (or other pathological values).
  2. Keeper accepts config without validation.
  3. Runtime creates zero-period interval.
  4. Daemon panics/halts (or enters repeated failure states for invalid parameter combos).
- **Impact:** Keeper liveness loss via config tampering/error.
- **Evidence:**
  ```rust
  let mut interval = tokio::time::interval(Duration::from_secs(cfg.tick_interval_secs));
  ```
  No min/max validation is performed in `KeeperConfig::from_file`.

### PK-009 — Watchdog history limit is unbounded from config (memory DoS surface)
- **Severity:** MEDIUM
- **Category:** G
- **File+Lines:** `solana/keeper/src/config.rs:217-219`, `solana/keeper/src/watchdog.rs:135-149`
- **Attack scenario:**
  1. Config sets extremely large `watchdog_history_limit`.
  2. Attacker induces recurring anomalies (e.g., via RPC manipulation).
  3. Watchdog continuously appends to in-memory `history` up to huge limit.
  4. Process memory grows until degradation/OOM.
- **Impact:** Resource exhaustion and keeper instability.
- **Evidence:**
  ```rust
  memory.history.push(...);
  if memory.history.len() > cfg.watchdog_history_limit {
      memory.history.drain(0..overflow);
  }
  ```
  Limit is user-controlled and not capped.

### PK-010 — Critical module failures are only logged and suppressed (silent failure mode)
- **Severity:** LOW
- **Category:** D
- **File+Lines:** `solana/keeper/src/main.rs:134-179`
- **Attack scenario:**
  1. Attacker causes persistent errors in one or more modules (RPC partial failures, decode failures, tx rejections).
  2. Keeper loop continues indefinitely while emitting warnings only.
  3. No fail-closed mode, no escalation, no backoff shutdown.
  4. Operators may interpret process as healthy while protections are effectively offline.
- **Impact:** Alert suppression / degraded security posture with extended detection latency.
- **Evidence:**
  ```rust
  match oracle::run_oracle_cycle(...) {
      Ok(..) => ...,
      Err(err) => warn!(error = %err, "oracle step failed"),
  }
  ```
