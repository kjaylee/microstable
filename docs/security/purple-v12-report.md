# Microstable Security Audit — Purple Team v12 (Post-Blue-v8 Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v12
- Scope (full read):
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper: all `solana/keeper/src/*.rs`
  - Ops/runtime artifacts: `solana/keeper/scripts/verify-isolation.sh`, `docs/security/ops-hardening.md`
  - Prior reports: `docs/security/purple-v9-report.md`, `docs/security/purple-v10-report.md`, `docs/security/purple-v11-report.md`
  - Target patch commit: `49dd8d8`

---

## Blue v8 Patch Verification Matrix (MSV11-001~002)

| ID | Blue v8 Claim | v12 Result |
|---|---|---|
| MSV11-001 | Preflight checks `Active && tier>=2`; ERROR guidance; `--require-rebalance` exits non-zero; helper added | **VERIFIED (effective with documented operational mode)** |
| MSV11-002 | PM2 path canonicalization (`~`, `$HOME`, trailing slash, symlink), script hardened and `/proc/<pid>/environ` check | **PARTIAL / REGRESSED IN SCRIPT** (runtime canonicalization fixed, but script execution path is broken by redirection bug) |
| Docs | `ops-hardening.md` includes rebalance requirements | **VERIFIED** |

---

## Findings

### MSV12-001 — `verify-isolation.sh` regression: Python invocation is malformed and aborts on real `pm2 jlist` JSON
- Severity: **MEDIUM**
- Category: Operational security verification / false-assurance control failure
- Component:
  - `solana/keeper/scripts/verify-isolation.sh:3`
  - `solana/keeper/scripts/verify-isolation.sh:19`
  - `solana/keeper/scripts/verify-isolation.sh:25`
- Details:
  - Script uses both heredoc and here-string on the same `python3 -` invocation:
    - `python3 - <<'PY' <<<"${JLIST_JSON}"`
  - In this form, JSON is fed as Python source (not stdin payload for `json.load`), while embedded Python block is not executed as intended.
  - Real `pm2 jlist` payload includes JSON literals (`false`/`true`/`null`), causing Python runtime errors (e.g., `NameError: name 'false' is not defined`).
  - Because `set -euo pipefail` is enabled (`line 3`), the script exits early and does not complete isolation verification.
- Impact:
  - Blue v8 script-side assurance for PM2 isolation is unreliable in real environments.
  - Operator verification flow can terminate before process-domain/PM2_HOME consistency checks, reducing confidence in isolation posture.
- Reproduction evidence:
  - Equivalent invocation from script with representative `pm2 jlist` JSON fails:
    - `NameError: name 'false' is not defined`

---

## Re-Verification of Prior Findings (v9/v10/v11)

### v11 items
- **MSV11-001 (rebalance preflight correctness/liveness guardrail): CLOSED / VERIFIED**
  - `--require-rebalance` flag added and wired: `solana/keeper/src/main.rs:73-75`, `155`, `299-345`
  - Eligibility helper enforces `status == Active && tier >= 2`: `solana/keeper/src/main.rs:404-433`
  - ERROR-level guidance when no eligible submitter: `solana/keeper/src/main.rs:315-320`, `435-440`
  - Runtime rebalance path still correctly requires local signer overlap with eligible agents: `solana/keeper/src/rebalance.rs:288-301`, `934-953`
  - On-chain commit eligibility unchanged and consistent: `solana/programs/microstable/src/lib.rs:3398-3411`
  - Ops policy explicitly documents this design constraint and `--require-rebalance` usage: `docs/security/ops-hardening.md:60-80`

- **MSV11-002 (PM2 path false negatives): PARTIALLY CLOSED**
  - Runtime canonicalization hardening verified in keeper:
    - `solana/keeper/src/main.rs:463-508`
  - Added tests for `$HOME`, trailing slash, symlink handling:
    - `solana/keeper/src/main_preflight_tests.rs:8-23`, `39-44`, `85-111`
  - **But** script-side verification regressed (MSV12-001), so full closure not achieved.

### v10 items
- **MSV10-001**: **CLOSED by Blue v8 operational fail-fast path** (with explicit operator-controlled mode)
  - Verified evidence listed under MSV11-001 closure.
- **MSV10-002**: **VERIFIED MAINTAINED FIXED**
  - Filtered `AgentRecord` scans in rebalance: `solana/keeper/src/rebalance.rs:899-905`, `924-931`
  - Filtered scans in agent loop: `solana/keeper/src/agent_loop.rs:444-450`, `479-486`
  - Account size alignment maintained: `solana/programs/microstable/src/lib.rs:2320`
- **MSV10-003**: **NOT FULLY CLOSED** due MSV12-001 script regression

### v9 items
- **MSV9-002 (sybil/governance hardening)**: **VERIFIED MAINTAINED**
  - Min stake: `solana/programs/microstable/src/lib.rs:72`, `3316-3321`
  - Keeper quorum for score/promote/demote: `solana/programs/microstable/src/lib.rs:405-409`, `419-423`, `434-438`, `2902-2911`
  - Stake-weighted selection retained: `solana/keeper/src/agent_loop.rs:396-433`
- **MSV9-003 (key history exposure)**: **VERIFIED MAINTAINED**
  - `git log --all -- solana/keeper/keeper2.json solana/keeper/keeper3.json` returned no commits
- **MSV9-004 (PM2 isolation operationalization)**: **PARTIAL**
  - Runtime checks improved, but script verification regression keeps assurance gap (MSV12-001)
- **MSV9-005 (wire drift)**: **VERIFIED MAINTAINED**
  - Pending keeper fields present in keeper wire struct: `solana/keeper/src/wire.rs:24-26`
  - Cross-RPC tolerance check includes pending keeper fields: `solana/keeper/src/utils.rs:575-594`

---

## Test/Build Evidence

- `cargo test -p microstable --lib --quiet` → passed
- `cd solana/keeper && cargo test --quiet` → passed

---

## Final Assessment

- **ZERO NEW FINDINGS**: **NO**
- Findings in v12: **1**
  - MEDIUM ×1
- Most urgent:
  1. **MSV12-001** — fix `verify-isolation.sh` Python invocation regression to restore reliable PM2 isolation verification path.
