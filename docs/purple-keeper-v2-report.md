# Microstable Purple-Keeper v2 — Zero Verification Audit

Date: 2026-02-22 (KST)
Target: Blue-Keeper v2 patched Keeper daemon

## Verdict
**ZERO FINDINGS 목표 미달성**

- Total findings: **3**
- Severity: **HIGH 2 / MEDIUM 1**

## Audit Scope
Reviewed files:
- `solana/keeper/src/main.rs`
- `solana/keeper/src/oracle.rs`
- `solana/keeper/src/rebalance.rs`
- `solana/keeper/src/monitor.rs`
- `solana/keeper/src/watchdog.rs`
- `solana/keeper/src/config.rs`
- `solana/keeper/src/utils.rs`
- `solana/keeper/src/wire.rs`
- `solana/keeper/Cargo.toml`
- `solana/Cargo.lock`

References:
- `docs/purple-keeper-report.md` (PK-001~010)
- `docs/red-keeper-report.md` (Red exploit campaign)
- `docs/blue-keeper-v2-tc.md`

Validation run:
- `cd solana/keeper && cargo test` → **pass** (7 tests passed)

---

## Patch Verification Summary (PK + Red Exploits)

### Purple findings (PK-001~010)
- **PK-001 (single-RPC trust):** **Partially fixed** (oracle/rebalance/watchdog are strict cross-RPC; monitor path has residual gap → Finding PKV2-001)
- **PK-002 (one-shot emergency trigger):** Fixed (debounce + reset logic)
- **PK-003 (deterministic reveal salt):** Fixed (random entropy salt)
- **PK-004 (keypair file hardening):** Fixed (secure open + fd-based checks)
- **PK-005 (static first-two quorum):** Fixed (protocol keeper-set matched quorum)
- **PK-006 (per-feed freshness ignored):** Fixed (per-feed max age applied)
- **PK-007 (feed identity/write authority ignored):** Fixed (explicit validation)
- **PK-008 (config sanity bounds missing):** Mostly fixed (critical bounds added)
- **PK-009 (watchdog history unbounded):** Fixed (hard cap in config + trim)
- **PK-010 (silent module failure suppression):** Fixed for error paths (cycle returns `Err` on module failure)

### Red exploited set (6)
(From prior Red campaign exploited items: RK-003, RK-005, RK-009, RK-013, RK-015, RK-020)
- Secondary RPC optional bypass: Fixed (`secondary_rpc_url` mandatory)
- Debounce skip accumulation: Fixed (skip path resets counter)
- Keypair TOCTOU: Fixed (single opened fd validate+read)
- Extreme config abuse: Mostly fixed (critical upper/lower bounds added)
- Skip-but-Ok mismatch masking: Fixed for cross-RPC mismatch paths (now `Err`)
- Supply-chain guard missing: **Partially fixed** (controls added, but bypassable trust model remains → Findings PKV2-002/003)

---

## Findings

## PKV2-001 — Monitor cross-RPC verification is value-equivalence only (state-integrity gap)
- **Severity:** HIGH
- **File / Line:** `solana/keeper/src/monitor.rs:48-76, 121-128`
- **Attack scenario:**
  1. Attacker controls/poisons primary RPC responses for monitor reads.
  2. Attacker forges protocol/vault/circuit fields while preserving `global_cr_bps` to match secondary.
  3. Monitor only compares `global_cr_bps` across RPC endpoints, so mismatch check passes.
  4. Emergency/circuit decisions are made from forged primary state.
- **Impact:** Auto-emergency action can be suppressed or mis-triggered under adaptive RPC manipulation; monitor safety signal integrity is not equivalent to full-state integrity.
- **Evidence:**
  - Secondary data fetched, but check is only:
    - `if global_cr_bps != secondary_global_cr_bps { ... Err(...) }`
  - No `protocol != secondary_protocol` / `vaults != secondary_vaults` / circuit-state comparison in monitor path.

## PKV2-002 — Binary attestation trust anchor is runtime environment (self-attestable)
- **Severity:** HIGH
- **File / Line:** `solana/keeper/src/utils.rs:272-284, 322-325`
- **Attack scenario:**
  1. Adversary compromises deploy/startup script (or service environment injection).
  2. Trojanned keeper binary is deployed.
  3. Attacker sets `KEEPER_BINARY_SHA256` to trojan hash (and optional lockfile path override).
  4. Startup attestation passes because expected hash source is attacker-controlled runtime env.
- **Impact:** Supply-chain guard can be bypassed without defeating hash comparison; control does not provide independent provenance assurance.
- **Evidence:**
  - Expected hash is read from `std::env::var("KEEPER_BINARY_SHA256")`.
  - Attestation only checks runtime-provided expected value against runtime binary hash.

## PKV2-003 — Lockfile source policy blocks only git/path, allows untrusted registry sources
- **Severity:** MEDIUM
- **File / Line:** `solana/keeper/src/utils.rs:298-306`
- **Attack scenario:**
  1. Dependency source changed to `registry+https://<attacker-registry>` in lockfile/supply chain.
  2. Validation rejects only `git+` and `path+`.
  3. Non-crates.io registry source passes validation.
- **Impact:** Dependency-source verification can be bypassed while still satisfying current guard; malicious registry-based dependency injection remains possible.
- **Evidence:**
  - Current logic: reject if `trimmed.contains("\"git+") || trimmed.contains("\"path+")`.
  - No allowlist enforcement for crates.io registry URL, no checksum/signature policy checks.

---

## Cargo / Dependency Review Notes
- Current `solana/Cargo.lock` checked: no `git+` / `path+` source entries found.
- `solana/keeper/Cargo.toml` has direct versioned dependencies only (no direct `[patch]`/`[replace]` in reviewed manifests).
- Residual risk is in **verification logic robustness**, not just current lockfile snapshot.

---

## Conclusion
Keeper v2 significantly improved and closes most Purple/Red findings, but **residual high-risk gaps remain** in:
1. monitor cross-RPC integrity semantics, and
2. supply-chain trust model/lockfile source policy.

Therefore, **Keeper daemon is not at zero-vulnerability state** under this audit scope.
