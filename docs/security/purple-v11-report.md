# Microstable Security Audit — Purple Team v11 (Post-Blue-v7 Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v11
- Scope (full read):
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper: all `solana/keeper/src/*.rs`
  - Prior reports: `docs/security/purple-v10-report.md`, `docs/security/purple-v9-report.md`
  - Target patch commit: `4a21f9c`

---

## Blue v7 Patch Verification Matrix (MSV10-001~003)

| ID | Blue v7 Claim | v11 Result |
|---|---|---|
| MSV10-001 | Startup preflight checks keeper key registration + tier warning | **PARTIAL / INEFFECTIVE FOR LIVENESS** (checks exist, but rebalance still hard-depends on locally held tier-2 active keeper-agent key; preflight is warn-only and misses `status != Active`) |
| MSV10-002 | `get_program_accounts` switched to `Memcmp`+`DataSize` filters for AgentRecord in `rebalance.rs` and `agent_loop.rs` | **VERIFIED** |
| MSV10-003 | Runtime PM2 isolation/.env preflight checks + `scripts/verify-isolation.sh` | **PARTIAL** (checks added, but new false-negative paths exist in both runtime detection and script verification) |

---

## Findings

### MSV11-001 — MSV10-001 remains open: rebalance availability still depends on local keeper-agent key custody
- Severity: **HIGH**
- Category: Availability / Agent governance
- Component:
  - `solana/keeper/src/rebalance.rs:288-301`
  - `solana/keeper/src/rebalance.rs:934-953`
  - `solana/keeper/src/main.rs:295-305`, `307-355`
  - `solana/programs/microstable/src/lib.rs:3398-3411`
- Details:
  - Blue v7 added startup warnings for missing registration / low tier (`main.rs:307-355`), but commit signer resolution is still strictly local-key based (`rebalance.rs:934-953`).
  - Runtime still skips commit if no configured keeper key is currently an eligible tier-2 active agent (`rebalance.rs:288-301`).
  - On-chain eligibility still requires `status == Active` and `tier >= 2` (`lib.rs:3404-3411`), while preflight only checks owner/decode/tier and does **not** verify active status (`main.rs:347-355`).
- Impact:
  - Keeper can run indefinitely with rebalance path unavailable (warning-only behavior, no fail-fast), preserving the same structural liveness risk class identified in v10.
- Verification note:
  - Patch implementation is present, but effectiveness is insufficient for closing the prior HIGH finding.

### MSV11-002 — PM2 isolation verification has false-negative bypasses (new in Blue v7 hardening path)
- Severity: **MEDIUM**
- Category: Operational security / Startup preflight correctness
- Component:
  - `solana/keeper/scripts/verify-isolation.sh:5-6`
  - `solana/keeper/src/main.rs:369-390`
- Details:
  - `verify-isolation.sh` prints that unset `PM2_HOME` means default `~/.pm2` (`line 5`), but executes `pm2 jlist` with fallback `PM2_HOME=/home/spritz/.pm2-keeper` (`line 6`), i.e., checks a different domain than implied runtime default.
  - Runtime shared-domain detection uses exact path equality only (`is_default_pm2_home`), with no canonicalization (`main.rs:378-389`), allowing syntactic variants of default path (e.g., trailing slash/symlinked path forms) to evade shared-domain warning.
- Impact:
  - Operators can receive a false sense of PM2 isolation while still operating in a shared process domain, preserving cross-tenant env/metadata exposure risk.

---

## Re-Verification of Prior Findings (v8/v9/v10)

### v10 findings
- **MSV10-001**: **REOPENED** as `MSV11-001` (HIGH), evidence above.
- **MSV10-002**: **VERIFIED FIXED**
  - `rebalance.rs` uses filtered scan: `899-905`, filter builder `924-931`
  - `agent_loop.rs` uses filtered scan: `444-450`, filter builder `479-486`
  - Account size aligns with on-chain `AgentRecord::SPACE = 8 + 160` (`lib.rs:2320`; keeper constants `rebalance.rs:31`, `agent_loop.rs:32`)
- **MSV10-003**: **PARTIAL**
  - Startup preflight hooks present (`main.rs:295-305`, `365-435`) and script added, but false-negative issues are present (`MSV11-002`).

### v9 findings
- **MSV9-002 (sybil resistance / governance hardening)**: **VERIFIED MAINTAINED**
  - Min stake: `lib.rs:72`, enforced at `3316-3319`
  - Keeper quorum for score/promote/demote: `lib.rs:405-409`, `419-423`, `434-438`, quorum core `2902-2911`
  - Stake-weighted random selection: `agent_loop.rs:396-433`
- **MSV9-003 (key history exposure)**: **VERIFIED MAINTAINED**
  - `git log --all -- solana/keeper/keeper2.json solana/keeper/keeper3.json` returned no commits.
- **MSV9-004 (PM2 isolation operationalization)**: **STILL OPEN IN PART** via `MSV11-002` verification false negatives.
- **MSV9-005 (wire drift pending keeper fields)**: **VERIFIED MAINTAINED**
  - Keeper wire struct includes pending keeper fields: `solana/keeper/src/wire.rs:24-26`

---

## Test/Build Evidence

- `cargo test -p microstable --lib --quiet` → passed
- `cd solana/keeper && cargo test --quiet` → passed

---

## Final Assessment

- **ZERO NEW FINDINGS**: **NO**
- Findings in v11: **2**
  - HIGH ×1
  - MEDIUM ×1
- Most urgent:
  1. **MSV11-001** (rebalance liveness remains structurally dependent on local keeper-agent key custody; preflight is warning-only)
  2. **MSV11-002** (PM2 isolation verification false negatives can mask shared-domain operation)
