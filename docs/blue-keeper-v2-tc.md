# Blue-Keeper v2 Test Cases (TDD)

## Scope
Covers the 6 exploited findings from `docs/red-keeper-report.md`:
- RK-001: Secondary RPC optional bypass
- RK-002: Debounce counter not reset on skipped monitor cycle
- RK-003: Keypair TOCTOU during load
- RK-004: Skip-but-Ok silent degradation
- RK-005: Extreme config values accepted
- RK-006: Supply-chain guardrails missing at startup

## Test Cases

### RK-001 — Secondary RPC is mandatory and fail-fast
1. **TC-RK001-01 (config reject null secondary)**
   - Given config has `secondary_rpc_url: null`
   - When keeper loads config
   - Then load fails with validation error indicating secondary RPC is required.

2. **TC-RK001-02 (startup fail-fast on secondary outage)**
   - Given config has non-null secondary RPC URL
   - And primary RPC is reachable but secondary is unreachable
   - When keeper starts
   - Then startup exits with error before entering cycle loop.

### RK-002 — Debounce counter reset on skipped monitor cycle
3. **TC-RK002-01 (skip resets debounce)**
   - Given `consecutive_emergency_cycles > 0`
   - And monitor detects cross-RPC mismatch in the same cycle
   - When monitor returns early
   - Then `consecutive_emergency_cycles` becomes `0`.

4. **TC-RK002-02 (explicit reset log)**
   - Given mismatch-triggered skip and non-zero debounce memory
   - When monitor skips the cycle
   - Then log contains exact text: `debounce counter reset due to skipped cycle`.

### RK-003 — Keypair load TOCTOU hardening
5. **TC-RK003-01 (validate on opened fd)**
   - Given keeper loads keypair from filesystem
   - When security checks run
   - Then owner/mode checks are evaluated from metadata of the opened file descriptor (`fstat` semantics), not path-based `stat` before read.

6. **TC-RK003-02 (single open/read path)**
   - Given keypair loading path
   - When keypair is parsed
   - Then the same opened descriptor is used for both validation and bytes read.

### RK-004 — Skip-paths become hard failures
7. **TC-RK004-01 (oracle mismatch -> Err)**
   - Given primary/secondary mismatch on protocol/vault or oracle observation
   - When oracle cycle runs
   - Then cycle returns `Err` (not `Ok(Vec::new())`).

8. **TC-RK004-02 (rebalance/monitor/watchdog mismatch -> Err)**
   - Given cross-RPC mismatch in each module
   - When cycle runs
   - Then each module returns `Err` and main cycle counts module as failed.

### RK-005 — Tight config bounds enforced
9. **TC-RK005-01 (bounds reject unsafe values)**
   - `tick_interval_secs` outside `5..=300` rejected.
   - `emergency_collateral_ratio_bps` outside `10000..=20000` rejected.
   - staleness (`oracle_max_age_secs`, `oracle_publish_max_age_secs`, per-feed `max_age_secs`) outside `10..=300` rejected.
   - `oracle_confidence_max_bps` outside `1..=1000` rejected.
   - on-chain vault `weight_cap` outside `10000..=1000000` ppm (`0.01..1.0`) causes rebalance cycle failure.

10. **TC-RK005-02 (commit/liveness sanity bounds)**
    - `commit_valid_for_slots` must be `>= commit_reveal_delay_slots` and `<= 1000`.
    - `max_consecutive_failed_cycles` has a strict upper bound and rejects extreme values.

### RK-006 — Supply-chain startup controls
11. **TC-RK006-01 (binary attestation required)**
    - Given startup without binary attestation hash
    - When supply-chain guard runs
    - Then startup fails before keeper cycles.

12. **TC-RK006-02 (lockfile vetting gate)**
    - Given dependency lock content includes `git+` or `path+` source
    - When supply-chain guard runs
    - Then validation fails with explicit dependency source policy error.
