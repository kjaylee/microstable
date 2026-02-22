# Blue-Keeper v6 Test Cases (TDD)

## Scope
Purple-Keeper v5 residual findings final remediation:
- PKV5-001: cross-RPC **read-path** secondary failure was not connected to degraded mode.
- PKV5-002: tx confirmation accepted primary-only in normal mode (single-RPC trust regression).

---

## PKV5-001 — Cross-RPC read failure must drive degraded mode and primary-only fallback

### TC-PKV5-001-01 (read failure counter reaches threshold)
- Given secondary RPC read failures occur consecutively
- When `consecutive_secondary_failures` reaches threshold(3)
- Then keeper enters degraded mode.

### TC-PKV5-001-02 (degraded mode disables cross-RPC read comparison)
- Given secondary RPC is in degraded mode
- When module read cycle runs
- Then module skips secondary cross-check and continues with primary-only snapshot.

### TC-PKV5-001-03 (degraded mode auto-recovery)
- Given degraded mode is active
- When secondary read/health probe succeeds
- Then degraded mode is cleared and dual-RPC read path is re-enabled automatically.

---

## PKV5-002 — Primary-only confirmation allowed only in degraded mode

### TC-PKV5-002-01 (normal mode primary-only => soft fail + retry signal)
- Given normal mode and primary confirmed but secondary unconfirmed
- When confirmation outcome is assessed (first pass)
- Then decision is soft-fail and requires one retry.

### TC-PKV5-002-02 (normal mode primary-only after retry => reject)
- Given normal mode and primary-only confirmation persists after retry
- When confirmation outcome is assessed with retry exhausted
- Then transaction is rejected (dual-RPC confirmation required).

### TC-PKV5-002-03 (degraded mode primary-only => conditional success)
- Given degraded mode and primary confirmed while secondary unavailable
- When confirmation outcome is assessed
- Then transaction is accepted to preserve liveness.

### TC-PKV5-002-04 (normal mode secondary-only also rejected)
- Given normal mode and only secondary confirmed
- When confirmation outcome is assessed
- Then transaction is rejected (both primary+secondary required in normal mode).
