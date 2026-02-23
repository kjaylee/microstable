# Microstable Security Audit — Purple Team v10 (Post-Blue-v6 Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v10
- Scope (full read):
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper: all `solana/keeper/src/*.rs`
  - Prior reports: `docs/security/purple-v9-report.md`, `docs/security/purple-red-v8-max.md`
  - Target patch commit: `b4ab9f6`

---

## Blue v6 Patch Verification Matrix (MSV9-001~005)

| ID | Blue v6 Claim | v10 Result |
|---|---|---|
| MSV9-001 | Rebalance submitter resolved from on-chain eligible tier-2 agents + preflight | **PARTIAL** (hardcoded `k1` removed, but submitter resolution is still restricted to locally loaded keeper keys; rebalance can remain non-functional when eligible tier-2 agent is not a local keeper key) |
| MSV9-002 | Min stake = 100M lamports, stake-weighted slot-seeded selection, score/promote/demote = keeper quorum | **VERIFIED** |
| MSV9-003 | Git history cleaned + key rotation documented | **VERIFIED** |
| MSV9-004 | MiniPC PM2 isolation operationalized (dedicated PM2_HOME, `.env`, 600 perms) | **REOPENED / NOT EVIDENCED IN REPO** |
| MSV9-005 | `wire::ProtocolState` includes pending keeper fields; trailing-byte decode warning added | **VERIFIED (warning-only behavior)** |

---

## Findings

### MSV10-001 — Rebalance liveness still depends on keeper-held agent key material
- Severity: **HIGH**
- Category: Access control / Availability
- Component:
  - `solana/keeper/src/rebalance.rs:283-291`
  - `solana/keeper/src/rebalance.rs:913-931`
  - `solana/keeper/src/utils.rs:293-311`
- Details:
  - Blue v6 removed the hardcoded `k1` submitter and now queries eligible on-chain tier-2 active agents.
  - However, actual submitter selection is still constrained to `keepers: &[Keypair]` only (`select_commit_submitting_signer`), i.e., only local keeper keypairs can become `submitting_agent`.
  - If no locally loaded keeper key is also an eligible tier-2 active agent, commit path is skipped every cycle (`"no locally-available tier-2 active registered agent"`), leaving rebalance unavailable.
- Impact:
  - Persistent rebalance non-execution under valid on-chain agent population (when eligibility and key custody are separated).
  - Core protocol balancing can stall without explicit hard failure.
- Evidence:
  - Skip path: `rebalance.rs:286-290`
  - Keeper-only signer resolution: `rebalance.rs:913-931`
  - Keeper key loading surface: `utils.rs:293-311`

### MSV10-002 — Unbounded full-program account scans enable account-bloat DoS pressure
- Severity: **MEDIUM**
- Category: DoS / Resource exhaustion
- Component:
  - `solana/keeper/src/rebalance.rs:886-911` (eligible commit-agent discovery)
  - `solana/keeper/src/agent_loop.rs:432-472` (registered-agent discovery)
  - Call sites: `rebalance.rs:272-281`, `agent_loop.rs:117-125`, `agent_loop.rs:236-247`
- Details:
  - Agent discovery uses unfiltered `rpc.get_program_accounts(&program_id)` and decodes candidates client-side.
  - Runtime cost scales with *all* program-owned accounts, not only agent records.
- Impact:
  - High account cardinality can inflate keeper RPC latency and memory pressure.
  - In sustained conditions this can degrade cycle reliability and amplify failure rates.

### MSV10-003 — MSV9-004 operational hardening remains non-verifiable from tracked artifacts
- Severity: **HIGH**
- Category: Operational security / Isolation
- Component:
  - `.gitignore:20-23`
  - `docs/security/ops-hardening.md:13-63`
- Details:
  - Blue v6 claim states PM2 runtime isolation is implemented operationally.
  - In tracked repository artifacts, deployment/runtime files are excluded (`deploy.sh`, `ecosystem.config.js`, `infrastructure/`), and the hardening doc remains procedural guidance rather than enforceable/runtime-verifiable state.
- Impact:
  - Security posture for PM2 domain isolation cannot be independently verified from source-controlled evidence.
  - Prior MSV9-004 risk class remains open from an auditability/assurance perspective.

---

## Verified Items (No finding raised)

### MSV9-002 verification evidence
- Min stake raised to 100M:
  - `solana/programs/microstable/src/lib.rs:72`
  - Enforcement: `solana/programs/microstable/src/lib.rs:3316-3321`
- Score/promote/demote now quorum-gated:
  - Logic: `solana/programs/microstable/src/lib.rs:405-409`, `419-423`, `434-438`, `2902-2911`
  - Account structs require two signers: `solana/programs/microstable/src/lib.rs:2078-2113`
  - Keeper wire ABI updated for two signers: `solana/keeper/src/wire.rs:300-360`
- Stake-weighted slot-seeded selection:
  - `solana/keeper/src/agent_loop.rs:362-427`

### MSV9-003 verification evidence
- History check for leaked key files returned empty:
  - `git log --all -- solana/keeper/keeper2.json solana/keeper/keeper3.json` => no commits
- Rotation log present:
  - `docs/security/key-rotation-log.md:1-5`

### MSV9-005 verification evidence
- Keeper wire struct synchronized with pending keeper fields:
  - `solana/keeper/src/wire.rs:10-27`
- Decoder trailing-byte warning present:
  - `solana/keeper/src/wire.rs:151-164`
- Cross-RPC state tolerance checks include new fields:
  - `solana/keeper/src/utils.rs:575-594`

---

## Test/Build Evidence

- Keeper crate tests:
  - `cd solana/keeper && cargo test --quiet` → passed
- On-chain library tests:
  - `cargo test -p microstable --lib --quiet` → passed

---

## Final Assessment

- **ZERO NEW FINDINGS**: **NO**
- New/Reopened findings: **3**
  - HIGH ×2
  - MEDIUM ×1
- Most urgent:
  1. **MSV10-001** (rebalance liveness still tied to keeper-held agent keys)
  2. **MSV10-003** (MSV9-004 runtime isolation still not source-verifiable)
