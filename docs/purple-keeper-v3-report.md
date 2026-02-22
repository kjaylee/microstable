# Microstable Purple-Keeper v3 — Zero Verification Audit

Date: 2026-02-22 (KST)  
Target: `microstable` @ `07d725b`

## Verdict
**ZERO FINDINGS 목표 미달성**

- Total findings: **3**
- Severity: **HIGH 2 / MEDIUM 1**

---

## Scope
Audited files:
- `solana/keeper/src/main.rs`
- `solana/keeper/src/oracle.rs`
- `solana/keeper/src/rebalance.rs`
- `solana/keeper/src/monitor.rs`
- `solana/keeper/src/watchdog.rs`
- `solana/keeper/src/config.rs`
- `solana/keeper/src/utils.rs`
- `solana/keeper/src/wire.rs`

Audit references:
1. `docs/purple-keeper-report.md` (PK-001~010)
2. `docs/red-keeper-report.md` (RK-001~020)
3. `docs/blue-keeper-v2-tc.md`
4. `docs/purple-keeper-v2-report.md` (PKV2-001~003)
5. `docs/blue-keeper-v3-tc.md`

Verification run:
- `cd solana/keeper && cargo test` → **pass** (v2: 7 tests, v3: 8 tests, failures: 0)

---

## Patch/Regression Recheck Summary

### PKV2 findings
- **PKV2-001 (monitor cross-RPC depth gap):** **Fixed** (deep view comparison includes protocol/circuit/vault fields).
- **PKV2-002 (supply-chain trust anchor weakness):** **Partially fixed** (env+file dual check added, but trust anchor remains runtime-overridable) → **PKV3-002**.
- **PKV2-003 (registry source bypass):** **Fixed** (strict crates.io allowlist enforced).

### Legacy set (PK-001~010, RK-001~006)
- Most previously reported controls remain in place (mandatory secondary RPC, debounce reset, TOCTOU hardening, mismatch as hard failure, tighter config bounds).
- Residual/new high-impact gaps remain in:
  - cross-RPC mismatch handling + fail-stop behavior,
  - supply-chain attestation trust anchoring,
  - write-path single-RPC trust.

---

## Findings

## PKV3-001 — Strict cross-RPC equality + fail-stop enables remote keeper DoS
- **Severity:** HIGH
- **File / Line:**
  - `solana/keeper/src/oracle.rs:133-137`
  - `solana/keeper/src/rebalance.rs:88-92`
  - `solana/keeper/src/monitor.rs:183-203`
  - `solana/keeper/src/watchdog.rs:88-99`
  - `solana/keeper/src/main.rs:145-156,320-327`
- **Attack scenario:**
  1. Attacker (or high normal activity) causes frequent on-chain state changes (mint/redeem/rebalance window churn).
  2. Keeper fetches primary and secondary snapshots sequentially (not same-slot pinned) and requires exact equality.
  3. Benign timing divergence is treated as module `Err` in multiple modules.
  4. `run_cycle()` returns failure; consecutive failures trigger daemon exit at threshold.
- **Impact:** Remote/market-driven liveness kill of keeper process (operator intervention required), with potential loss of timely oracle, monitor, and rebalance operations.
- **Evidence:**
  ```rust
  // oracle/rebalance/watchdog: strict equality -> Err
  if protocol != secondary_protocol || vaults != secondary_vaults { return Err(...); }

  // main: fail-stop exit
  if consecutive_failed_cycles >= cfg.max_consecutive_failed_cycles {
      return Err(anyhow!("too many consecutive failed cycles ..."));
  }
  ```

## PKV3-002 — Binary attestation remains bypassable via runtime-controlled trust inputs
- **Severity:** HIGH
- **File / Line:** `solana/keeper/src/utils.rs:277-283,301-305,364-378`
- **Attack scenario:**
  1. Adversary compromises startup environment/CI runtime variables.
  2. Sets `KEEPER_BINARY_SHA256` to trojan hash.
  3. Sets `KEEPER_BINARY_SHA256_FILE` to attacker-controlled file containing the same hash.
  4. `resolve_expected_binary_sha256()` accepts env+file match and attestation passes.
- **Impact:** Supply-chain attestation can be satisfied by attacker-provided runtime inputs; trojan keeper binary can pass startup checks without independent immutable trust anchor.
- **Evidence:**
  ```rust
  let env_expected = std::env::var("KEEPER_BINARY_SHA256").ok();
  let hash_file_path = std::env::var("KEEPER_BINARY_SHA256_FILE")...;
  ...
  if env_hash != file_hash { return Err(...); }
  Ok(env_hash)
  ```

## PKV3-003 — Privileged tx submission/confirmation still trusts single RPC endpoint
- **Severity:** MEDIUM
- **File / Line:**
  - `solana/keeper/src/utils.rs:237-269`
  - `solana/keeper/src/oracle.rs:276`
  - `solana/keeper/src/rebalance.rs:163,229,276`
  - `solana/keeper/src/monitor.rs:260`
  - `solana/keeper/src/watchdog.rs:223`
- **Attack scenario:**
  1. Primary RPC is malicious/degraded but read paths are kept consistent enough to pass dual-RPC checks.
  2. Keeper signs and submits all privileged tx via primary only.
  3. Primary can censor or falsely acknowledge sends/confirmations.
  4. Keeper logs success while on-chain state may remain unchanged.
- **Impact:** Silent protection failure/liveness degradation in emergency shutdown, oracle updates, rebalance commit/reveal, and watchdog alerts.
- **Evidence:**
  ```rust
  pub fn send_instructions(rpc: &RpcClient, ...) -> Result<Signature> {
      let blockhash = rpc.get_latest_blockhash()?;
      let sig = rpc.send_and_confirm_transaction_with_spinner_and_config(...)?;
      Ok(sig)
  }
  ```

---

## Conclusion
Blue-Keeper v3 closes most prior gaps and passes v2/v3 test suites, but **remaining vulnerabilities are not zero** under this audit scope.

Priority fixes:
1. Cross-RPC snapshot consistency strategy (slot pinning/tolerance) to prevent fail-stop DoS.
2. Immutable trust anchor for binary attestation (embedded hash/signed manifest not runtime-overridable).
3. Dual-endpoint write-path verification or independent confirmation path for privileged tx outcomes.
