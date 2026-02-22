# Blue-Keeper v4 Test Cases (TDD)

## Scope
Remediation verification for Purple-Keeper v3 findings:
- PKV3-001: cross-RPC strict equality + immediate fail-stop DoS
- PKV3-002: supply-chain trust anchor runtime override
- PKV3-003: privileged tx submit trusted single RPC

---

## PKV3-001 — Cross-RPC DoS hardening (tolerance + retry/backoff)

### TC-PKV3-001-01 (small drift within tolerance must pass)
- Given primary/secondary snapshots differ only by small drift (`±1` bps / `±1` lamport scale)
- When cross-RPC validation runs
- Then validation succeeds (no false mismatch rejection).

### TC-PKV3-001-02 (large drift beyond tolerance must fail)
- Given primary/secondary snapshots differ beyond tolerance (e.g., `+5` lamports)
- When cross-RPC validation runs
- Then validation fails with explicit mismatch error.

### TC-PKV3-001-03 (mismatch retries up to 3 attempts)
- Given first two attempts return mismatch and third attempt succeeds
- When retry-with-backoff wrapper runs
- Then operation succeeds on attempt 3 (no immediate fail-stop).

### TC-PKV3-001-04 (persistent mismatch fails after max retries)
- Given all 3 attempts return mismatch
- When retry-with-backoff wrapper runs
- Then operation fails only after exhausting retry budget.

---

## PKV3-002 — Immutable supply-chain trust anchor

### TC-PKV3-002-01 (embedded hash cannot be overridden by runtime env)
- Given compile-time embedded hash is present
- And runtime `KEEPER_BINARY_SHA256` is different
- When expected hash resolution runs
- Then resolution fails due to mismatch against embedded trusted hash.

### TC-PKV3-002-02 (embedded hash accepted without runtime env)
- Given compile-time embedded hash is present and valid
- And runtime env override is absent
- When expected hash resolution runs
- Then embedded hash is selected as the trust anchor.

---

## PKV3-003 — Dual-RPC submission trust model

### TC-PKV3-003-01 (primary fail + secondary confirm fallback)
- Given primary confirmation path fails
- And secondary confirmation path succeeds
- When tx outcome policy is evaluated
- Then tx is accepted via secondary fallback.

### TC-PKV3-003-02 (single-side confirmation is rejected when dual required)
- Given tx is only primary-confirmed and secondary confirmation is missing
- When tx outcome policy is evaluated
- Then tx is rejected as non-dual-confirmed.
