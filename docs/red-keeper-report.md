# Microstable Red-Keeper Report — Exploit Campaign Against Patched Keeper (`2b2e2b4`)

## Scope
- Target commit: `2b2e2b4` (Blue-Keeper patch set)
- Primary artifacts reviewed:
  - `docs/purple-keeper-report.md`
  - `solana/keeper/src/*.rs`
  - `solana/programs/microstable/src/lib.rs` (on-chain guard validation)
- Validation mode: static exploit analysis + build validation (`cargo check` for keeper)

## Executive Summary
- **Total attempts:** 20
- **Blocked:** 14 ✅
- **Exploited:** 6 ❌

Most Purple findings were materially hardened (dual-RPC cross-checking, feed-id/write-authority checks, random salt, quorum binding, watchdog cap, fail-stop counter).  
However, this campaign found **six bypasses/gaps**, mainly in **optional secondary-RPC operation**, **debounce state handling across skipped cycles**, **keypair TOCTOU**, **extreme-but-valid config abuse**, **silent skip-path behavior**, and **supply-chain controls**.

---

### RK-001 — Dual-RPC mismatch injection
- **Target:** PK-001 bypass
- **Result:** BLOCKED ✅
- **Method:** Tried to force primary/secondary divergence for protocol/vault state and observe whether keeper still signs privileged tx.
- **Evidence:** `oracle.rs:137-140`, `rebalance.rs:84-93`, `monitor.rs:75-87`, `watchdog.rs:87-97` all short-circuit on mismatch and skip action.
- **Impact:** N/A (no privileged tx on divergent state).

### RK-002 — Secondary RPC outage handling
- **Target:** PK-001 bypass
- **Result:** BLOCKED ✅
- **Method:** Simulated secondary unavailability during runtime.
- **Evidence:** Secondary fetch paths use `?` in monitor/rebalance/watchdog (`monitor.rs:49-72`, `rebalance.rs:59-82`, `watchdog.rs:61-84`), causing step failure; `main.rs:142-155` increments failure counter and exits at threshold.
- **Impact:** N/A (fail-closed with operator intervention).

### RK-003 — Optional secondary disabled (single-RPC trust reintroduced)
- **Target:** PK-001 bypass
- **Result:** EXPLOITED ❌
- **Method:** Set `secondary_rpc_url` to `None` (default-like deployment), then attacker controls only primary RPC.
- **Evidence:** `config.rs:124` default is `secondary_rpc_url: None`; validation does not require secondary (`config.rs:248-257`); runtime only cross-validates inside `if let Some(secondary)` blocks (`main.rs:55-57`).
- **Impact:** Single-endpoint spoofing threat returns; forged state can drive keeper decisions/signatures.

### RK-004 — Debounce flash-and-recover (strict consecutive check)
- **Target:** PK-002 bypass
- **Result:** BLOCKED ✅
- **Method:** Tried below-threshold CR flash then healthy cycle to accumulate toward emergency action.
- **Evidence:** Counter reset on non-emergency cycle at `monitor.rs:163-171`.
- **Impact:** N/A (simple flash/recovery cannot accumulate).

### RK-005 — Debounce accumulation via skipped monitor cycles
- **Target:** PK-002 bypass
- **Result:** EXPLOITED ❌
- **Method:** Low-CR observation increments debounce counter, then force monitor early-return skip via cross-RPC mismatch before recovery cycle, then low-CR again.
- **Evidence:** Early return on mismatch (`monitor.rs:75-87`) bypasses both increment and reset paths; reset only happens in non-emergency branch (`monitor.rs:163-171`).
- **Impact:** Non-consecutive real-world breaches can still accumulate toward auto `emergency_shutdown`.

### RK-006 — Random salt predictability attempt
- **Target:** PK-003 bypass
- **Result:** BLOCKED ✅
- **Method:** Tested whether reveal salt is slot/time deterministic or derivable from public chain fields.
- **Evidence:** `rebalance.rs:437-441` uses `Keypair::new().to_bytes()` entropy, not slot-derived value.
- **Impact:** N/A (commit preimage not predictably reconstructable from chain metadata).

### RK-007 — Salt leakage/persistence extraction path
- **Target:** PK-003 bypass
- **Result:** BLOCKED ✅
- **Method:** Looked for logging/disk persistence of `reveal_salt`.
- **Evidence:** Salt stored only in in-memory `RebalanceMemory.pending_reveal` (`rebalance.rs:27-38`, `238-243`); logs include commit/rebalance signatures and weights, not salt (`rebalance.rs:230-236`, `279-284`).
- **Impact:** N/A (no obvious passive leakage path in daemon code).

### RK-008 — Symlink/hardening bypass with symlinked key files
- **Target:** PK-004 bypass
- **Result:** BLOCKED ✅
- **Method:** Attempted key-file indirection via symlink path.
- **Evidence:** Explicit symlink rejection in `utils.rs:96-101`, plus owner/mode checks (`utils.rs:111-129`).
- **Impact:** N/A (direct symlink trick blocked).

### RK-009 — Keypair TOCTOU race after metadata validation
- **Target:** PK-004 bypass
- **Result:** EXPLOITED ❌
- **Method:** Race file replacement between `validate_keypair_file_security()` and `read_keypair_file()`.
- **Evidence:** Validation and read are separate path-based operations (`utils.rs:74-77`); no fd pinning, inode re-check, or atomic open-with-flags at read time.
- **Impact:** Local attacker with directory-write timing can load attacker-chosen keeper key and co-sign privileged ops.

### RK-010 — Keeper-set rotation mid-cycle (stale quorum attempt)
- **Target:** PK-005 bypass
- **Result:** BLOCKED ✅
- **Method:** Tried to use quorum selected from stale read while keeper set rotates before transaction execution.
- **Evidence:** On-chain quorum rechecked at execution (`lib.rs:2300-2309`); rotation timelocked and validated (`lib.rs:1343-1373`).
- **Impact:** N/A (stale-signer tx fails rather than executing).

### RK-011 — Per-feed freshness exact-boundary test
- **Target:** PK-006 bypass
- **Result:** BLOCKED ✅
- **Method:** Tested `publish_time` around `max_age_secs` boundary and feed/global override behavior.
- **Evidence:** Per-feed age enforced using `min(feed.max_age_secs, cfg.oracle_publish_max_age_secs)` (`oracle.rs:222`); stale check at `oracle.rs:401-406`.
- **Impact:** N/A (feed-specific freshness control now active).

### RK-012 — Feed identity bypass with crafted account payload
- **Target:** PK-007 bypass
- **Result:** BLOCKED ✅
- **Method:** Attempted decode-compatible fake payload with mismatched feed id/write authority.
- **Evidence:** Keeper enforces owner + `Full` verification + trusted write authority + expected feed id (`oracle.rs:321-355`); on-chain repeats checks (`lib.rs:2102-2148`, `2151-2182`).
- **Impact:** N/A (payload fails authenticity gates).

### RK-013 — Extreme-but-valid config abuse
- **Target:** PK-008 bypass
- **Result:** EXPLOITED ❌
- **Method:** Use technically valid but unsafe values (e.g., huge `oracle_publish_max_age_secs`, huge `max_consecutive_failed_cycles`, oversized `commit_valid_for_slots` that conflicts with on-chain max).
- **Evidence:** Validation has no upper bounds for several risk-critical values (`config.rs:292-339`), while on-chain commit window max is strict (`lib.rs:983-989`).
- **Impact:** Can degrade oracle freshness guarantees and/or create persistent liveness failures without immediate config rejection.

### RK-014 — Watchdog history flood / memory pressure
- **Target:** PK-009 bypass
- **Result:** BLOCKED ✅
- **Method:** Tried anomaly spam before cap enforcement.
- **Evidence:** Hard cap validation (`config.rs:327-335`) + runtime trim on overflow (`watchdog.rs:185-191`), with max cap `4096`.
- **Impact:** N/A (unbounded growth from config no longer possible).

### RK-015 — Silent partial-success bypass via skip paths
- **Target:** PK-010 bypass
- **Result:** EXPLOITED ❌
- **Method:** Keep modules in repeated “skip but Ok” states (cross-RPC mismatch), avoiding explicit errors.
- **Evidence:** Multiple modules return `Ok(...)` on critical skip (`oracle.rs:137-140`, `rebalance.rs:84-93`, `monitor.rs:81-87`, `watchdog.rs:93-97`); `run_cycle()` treats all-Ok as success (`main.rs:311-313`).
- **Impact:** Keeper can appear healthy while key protections/actions are effectively suppressed.

### RK-016 — Transaction replay between cycles
- **Target:** Novel attack
- **Result:** BLOCKED ✅
- **Method:** Attempted replay of previously signed tx / commit-reveal material across later cycles.
- **Evidence:** New blockhash per send (`utils.rs:215-223`) and commit consumed on successful reveal (`lib.rs:1069-1071`).
- **Impact:** N/A (replay window constrained; stale tx invalid or commit already cleared).

### RK-017 — Concurrent keeper instances racing state
- **Target:** Novel attack
- **Result:** BLOCKED ✅
- **Method:** Modeled two keeper daemons operating same state simultaneously.
- **Evidence:** Active pending commit check in keeper loop (`rebalance.rs:199-209`) and strict on-chain quorum/commit checks (`lib.rs:973-989`, `1007-1067`) prevent unauthorized transitions.
- **Impact:** Integrity preserved; potential operational noise/liveness contention only.

### RK-018 — Crafted Pyth data that “looks valid” but wrong price
- **Target:** Novel attack
- **Result:** BLOCKED ✅
- **Method:** Tried to pass structurally valid account data with adversarial economics.
- **Evidence:** Price/account constraints in keeper (`oracle.rs:321-359`) and on-chain (`lib.rs:2123-2145`, `2156-2187`) enforce owner/feed-id/authority/verification/price sanity.
- **Impact:** N/A unless upstream trusted authority itself is compromised.

### RK-019 — TOCTOU between RPC read and tx submit
- **Target:** Novel attack
- **Result:** BLOCKED ✅
- **Method:** State flips between off-chain read and tx execution to trigger unsafe state transition.
- **Evidence:** On-chain instruction handlers revalidate quorum and critical invariants at execution (`lib.rs:1007-1039`, `2300-2309`, `2367-2385`).
- **Impact:** Privileged safety checks still enforced; primary effect is tx failure/DoS, not invariant break.

### RK-020 — Keeper binary/dependency supply-chain attack surface
- **Target:** Novel attack
- **Result:** EXPLOITED ❌
- **Method:** Audited for built-in binary attestation / dependency-vetting guardrails.
- **Evidence:** No runtime binary provenance check, no in-repo dependency-vetting gate, and no enforced signed-release verification in keeper startup path.
- **Impact:** If CI/build or dependency source is compromised, attacker can ship a trojan keeper binary with direct key access.

---

## Top Findings (Red-Keeper)
1. **RK-003 (PK-001 bypass):** secondary RPC remains optional; single-RPC trust can be reintroduced by config posture.
2. **RK-005 (PK-002 bypass):** debounce counter can be accumulated across skipped monitor cycles.
3. **RK-009 (PK-004 bypass):** keypair validation/read TOCTOU race remains.
4. **RK-015 (PK-010 bypass):** repeated skip-path `Ok` responses can mask degraded protections.
5. **RK-013 (PK-008 bypass):** extreme-but-valid config values can still undermine safety/liveness.

## Build Check
```bash
cd /Users/kjaylee/.openclaw/workspace/microstable/solana/keeper
cargo check
```
Result: build passes (warnings only).
