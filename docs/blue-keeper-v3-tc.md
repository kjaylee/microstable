# Blue-Keeper v3 Test Cases (TDD)

## Scope
Remediation verification for Purple-Keeper v2 findings:
- PKV2-001: monitor cross-RPC depth/integrity gap
- PKV2-002: supply-chain binary attestation trust anchor weakness
- PKV2-003: lockfile registry source policy bypass

---

## PKV2-001 — Monitor cross-RPC integrity (deep state)

### TC-PKV2-001-01 (global CR equal but vault collateral mismatch must fail)
- Given primary/secondary snapshots have identical `global_cr_bps`
- And per-vault `total_deposits` differ
- When monitor cross-RPC validation runs
- Then monitor returns `Err` (no silent pass)
- And the error indicates vault collateral mismatch.

### TC-PKV2-001-02 (protocol supply/state mismatch must fail)
- Given primary/secondary snapshots differ on protocol state (e.g., `total_supply`, emergency flag, keeper set)
- When monitor cross-RPC validation runs
- Then monitor returns `Err`
- And the error indicates protocol-state mismatch.

### TC-PKV2-001-03 (circuit status mismatch must fail)
- Given primary/secondary snapshots differ on circuit breaker status
- When monitor cross-RPC validation runs
- Then monitor returns `Err`.

---

## PKV2-002 — Supply-chain self-attestation hardening

### TC-PKV2-002-01 (env+file dual verification required without embedded hash)
- Given no compile-time embedded trusted hash
- And only env hash is provided (no file hash)
- When expected hash resolution runs
- Then resolution fails with dual-verification requirement.

### TC-PKV2-002-02 (env/file mismatch must fail)
- Given env hash and file hash are both provided
- And values are different
- When expected hash resolution runs
- Then resolution fails due to mismatch.

### TC-PKV2-002-03 (matching env+file hash must pass)
- Given env hash and file hash are both provided
- And values are equal valid SHA-256 hex
- When expected hash resolution runs
- Then resolution succeeds and returns normalized trusted hash.

---

## PKV2-003 — Lockfile registry policy strict allowlist

### TC-PKV2-003-01 (only crates.io index registry allowed)
- Given lockfile source is `registry+https://github.com/rust-lang/crates.io-index`
- When dependency source validation runs
- Then validation passes.

### TC-PKV2-003-02 (alternate registry must fail)
- Given lockfile source is `registry+https://evil.example/index`
- When dependency source validation runs
- Then validation fails with unsupported registry source.

### TC-PKV2-003-03 (git/path source must fail)
- Given lockfile contains `git+...` or `path+...` source
- When dependency source validation runs
- Then validation fails.
