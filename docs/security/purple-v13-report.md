# Microstable Security Audit — Purple Team v13 (FINAL Zero-Finding Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v13
- Scope (full verification round):
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper: all `solana/keeper/src/*.rs`
  - Ops/runtime hardening artifacts: `solana/keeper/scripts/verify-isolation.sh`, `docs/security/ops-hardening.md`
  - Historical closure targets: v9~v12 finding set
  - Patch under final verification: `b4e2e09` (MSV12-001)

---

## Executive Result

## **ZERO NEW FINDINGS**

All previously open findings from v9~v12 are verified closed or maintained closed with evidence below. No new exploitable security issue was identified in this final round.

---

## Verification Method

1. Full source re-read for on-chain + keeper runtime paths.
2. Per-finding closure revalidation against current HEAD (`b4e2e09`).
3. Regression sweep for prior risk classes (rebalance liveness, sybil resistance, PM2 isolation, ABI drift, scan DoS).
4. Fresh issue hunt across keeper/program critical paths.
5. Build/test execution:
   - `cd solana && cargo test -p microstable --lib --quiet`
   - `cd solana/keeper && cargo test --quiet`
   - `bash -n solana/keeper/scripts/verify-isolation.sh`
   - Functional script simulation with JSON containing boolean literals (`false`) to validate MSV12-001 fix path.

---

## Historical Finding Closure Matrix (v9~v12)

| ID | Severity | v13 Result |
|---|---:|---|
| MSV8-001 | CRITICAL | **CLOSED (maintained)** |
| MSV8-002 | HIGH | **CLOSED (maintained)** |
| MSV8-003→MSV9-002 | HIGH | **CLOSED (maintained)** |
| MSV8-004→MSV9-003 | MEDIUM | **CLOSED (maintained)** |
| MSV8-005 | MEDIUM | **CLOSED (maintained)** |
| MSV8-006→MSV9-004→MSV11-002→MSV12-001 | HIGH→MEDIUM chain | **CLOSED (final script fix verified)** |
| MSV9-001→MSV10-001→MSV11-001 | HIGH | **CLOSED (effective fail-fast path maintained)** |
| MSV10-002 | MEDIUM | **CLOSED (maintained)** |
| MSV9-005 | LOW | **CLOSED (maintained)** |

---

## Per-Finding Evidence

### MSV8-001 (devnet_force_reinit feature gate)
- `devnet_force_reinit` remains feature-gated: `solana/programs/microstable/src/lib.rs:1651`.
- `DevnetForceReinit` account context also gated: `solana/programs/microstable/src/lib.rs:2235`.
- `devnet-admin` is not in default feature set: `solana/programs/microstable/Cargo.toml:12-21`.

### MSV8-002 (commit_rebalance ABI sync)
- On-chain `CommitRebalance` context requires both:
  - `agent_record`: `solana/programs/microstable/src/lib.rs:2165-2170`
  - `submitting_agent`: `solana/programs/microstable/src/lib.rs:2172`
- On-chain eligibility assertion consumes these accounts: `solana/programs/microstable/src/lib.rs:1224-1227`.
- Keeper wire instruction includes both accounts: `solana/keeper/src/wire.rs:216-234`.

### MSV8-003→MSV9-002 (sybil resistance + governance hardening)
- Minimum registration stake remains 0.1 SOL:
  - constant: `solana/programs/microstable/src/lib.rs:72`
  - enforced: `solana/programs/microstable/src/lib.rs:3316-3321`
- Score/promote/demote remain keeper-quorum-gated:
  - logic: `solana/programs/microstable/src/lib.rs:405-409, 419-423, 434-438`
  - account-level dual signers: `solana/programs/microstable/src/lib.rs:2078-2113`
- Candidate selection remains stake-weighted randomized by slot seed:
  - `solana/keeper/src/agent_loop.rs:396-429`

### MSV8-004→MSV9-003 (key leakage history)
- Key ignore rules remain present: `.gitignore:10-14`.
- History re-check:
  - Command: `git log --all -- solana/keeper/keeper2.json solana/keeper/keeper3.json`
  - Result: `lines=0`, `CLEAN` (no historical blobs for those key files).

### MSV8-005 (effective UID check)
- `effective_uid()` still uses `libc::geteuid()` directly: `solana/keeper/src/utils.rs:393-394`.

### MSV9-001→MSV10-001→MSV11-001 (rebalance liveness controls)
- Keeper startup preflight enforces eligibility semantics and emits ERROR guidance:
  - preflight + require path: `solana/keeper/src/main.rs:299-337`
  - eligibility check includes `status == Active && tier >= 2`: `solana/keeper/src/main.rs:404-433`
- `--require-rebalance` fail-fast path remains active: `solana/keeper/src/main.rs:322-326, 334-337`.
- Runtime commit still intentionally requires local signer overlap with eligible agents (documented tradeoff):
  - skip when no local eligible key: `solana/keeper/src/rebalance.rs:288-300`
  - local-key resolver: `solana/keeper/src/rebalance.rs:934-953`
- Operational guidance documents this explicitly: `docs/security/ops-hardening.md:60-80`.

### MSV10-002 (RPC filtered scans)
- Rebalance agent discovery uses filtered scans:
  - scan call: `solana/keeper/src/rebalance.rs:899-905`
  - memcmp+datasize filters: `solana/keeper/src/rebalance.rs:924-931`
- Agent loop discovery also uses filtered scans:
  - scan call: `solana/keeper/src/agent_loop.rs:444-450`
  - filter builder: `solana/keeper/src/agent_loop.rs:474-481`

### MSV9-005 (wire ABI drift)
- Keeper `wire::ProtocolState` includes pending keeper fields:
  - `pending_keeper_set`: `solana/keeper/src/wire.rs:24`
  - `pending_keeper_activation_slot`: `solana/keeper/src/wire.rs:25`
- Decoder behavior unchanged (warns on trailing bytes): `solana/keeper/src/wire.rs:156-161`.
- No new ABI mismatch observed in current on-chain vs keeper layout for the previously affected fields.

### MSV8-006→MSV9-004→MSV11-002→MSV12-001 (PM2 isolation verification chain)
- Runtime PM2 shared-domain check + canonicalization remains:
  - `check_pm2_isolation()`: `solana/keeper/src/main.rs:447-460`
  - canonicalized default-path detection (`~`, `$HOME`, trailing slash, symlink path forms): `solana/keeper/src/main.rs:463-508`
- Test coverage for canonicalization edge cases remains:
  - trailing slash: `solana/keeper/src/main_preflight_tests.rs:39-44`
  - symlink handling: `solana/keeper/src/main_preflight_tests.rs:85-111`
- **MSV12-001 regression fix verified in script**:
  - Python now consumes JSON via env var (`JLIST_JSON=... python3 <<'PY'`): `solana/keeper/scripts/verify-isolation.sh:19-25`
  - no conflicting Python+JSON stdin redirection at invocation site.
- Functional simulation confirmed script parses PM2 JSON containing `false` without Python `NameError` and completes isolation checks.

---

## New Vulnerability Hunt (v13)

Reviewed for fresh issues in:
- privileged path authorization,
- rebalance commit/reveal state transitions,
- agent selection/manipulation paths,
- key handling/file-permission checks,
- cross-RPC consistency fallback logic,
- PM2 isolation verification pipeline.

Result: **No new security findings** (no new exploitable auth bypass, integrity break, or liveness deadlock beyond documented/accepted operational tradeoffs).

---

## Test/Execution Evidence

- `cd solana && cargo test -p microstable --lib --quiet` → passed (27 tests).
- `cd solana/keeper && cargo test --quiet` → passed (keeper unit+integration suites).
- `bash -n solana/keeper/scripts/verify-isolation.sh` → `OK`.
- Script simulation (stubbed `pm2 jlist` returning JSON with boolean literals) completed successfully and produced isolation output (no Python parsing crash).

---

## Final Assessment

- **ZERO NEW FINDINGS: YES**
- Previously tracked v9~v12 findings: verified closed/maintained closed.
- Purple v13 final verification concludes with no additional actionable findings.