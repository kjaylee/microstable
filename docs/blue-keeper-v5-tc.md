# Blue-Keeper v5 Test Cases (TDD)

## Scope
PKV4 findings design-level remediation verification:
- PKV4-001: self-hash bootstrapping failure (startup fail-stop)
- PKV4-002: dual-RPC confirmation liveness DoS

---

## PKV4-001 — Self-hash 제거 + Cargo.lock attestation 전환

### TC-PKV4-001-01 (Cargo.lock attestation hash match)
- Given compile-time embedded `Cargo.lock` SHA-256 and runtime `Cargo.lock` bytes are identical
- When attestation verification runs
- Then verification succeeds.

### TC-PKV4-001-02 (Cargo.lock attestation hash mismatch)
- Given runtime `Cargo.lock` bytes differ from expected SHA-256
- When attestation verification runs
- Then verification fails with explicit mismatch error.

### TC-PKV4-001-03 (invalid Cargo.lock hash format rejection)
- Given malformed expected hash (not 64-hex)
- When attestation verification runs
- Then verification fails due to invalid hash format.

### TC-PKV4-001-04 (`enforce_supply_chain_controls` no self-binary dependency)
- Given normal build artifact
- When supply-chain controls run
- Then validation path depends on Cargo.lock attestation + dependency source policy only (no self-hash resolution), and returns success.

---

## PKV4-002 — Adaptive confirm window + warning-only secondary failure + degraded mode

### TC-PKV4-002-01 (adaptive confirm window expansion)
- Given primary confirmed and secondary not confirmed in base window
- When adaptive window is evaluated
- Then window expands from 30s to 60s.

### TC-PKV4-002-02 (primary-only confirmation accepted)
- Given primary confirmed and secondary unconfirmed
- When transaction outcome is assessed
- Then transaction is accepted (secondary failure is warning-only).

### TC-PKV4-002-03 (both unconfirmed rejected)
- Given primary and secondary both unconfirmed
- When transaction outcome is assessed
- Then transaction is rejected.

### TC-PKV4-002-04 (secondary failure threshold enters degraded mode)
- Given consecutive secondary failures reach threshold N
- When failure state is updated
- Then keeper enters degraded mode and disables secondary usage.

### TC-PKV4-002-05 (secondary success recovers from degraded mode)
- Given keeper is in degraded mode
- When secondary success is observed
- Then degraded mode clears and dual-RPC path is re-enabled.
