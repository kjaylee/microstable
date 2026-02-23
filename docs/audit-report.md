# Microstable Formal Security Audit Report

**Protocol:** Microstable  
**Program ID (devnet):** `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`  
**Repository:** `https://github.com/kjaylee/microstable`  
**Audit date range:** 2026-02-22 to 2026-02-23 (KST)  
**Final report date:** 2026-02-23  
**Audited revision (main HEAD):** `4603f9bb608bda7831963444de9ac73c5ecc8f8b`

---

## 1) Executive Summary

This report consolidates the full security workstream executed for Microstable across:

- **Main security cycle:** v8 → v13 (convergence: **6 → 5 → 3 → 2 → 1 → 0**)  
- **Deferred-reveal cycle:** v14 → v17 (convergence: **1 → 1 → 2 → 0**)  
- **Keeper security cycle:** v1 → v6 (convergence: **10 → 3 → 3 → 2 → 2 → 0**)  
- **Additional adversarial campaigns:** Red v1–v5, Purple v1–v3, Blue v1–v13, Crimson hybrid, Yellow remediation, Green operational readiness.

The review identified and tracked vulnerabilities spanning:

- access control and signer/quorum policy,
- oracle trust/freshness/feed binding,
- mint/redeem accounting and fee correctness,
- commit-reveal integrity and liveness,
- agent governance and Sybil resistance,
- keeper daemon reliability under cross-RPC faults,
- operational isolation and supply-chain hardening.

### Final consolidated verdict

At report finalization, **all tracked findings in the audited cycles are resolved** at code and verification level, with closure evidence captured through Blue remediation commits and subsequent Purple/Red re-verification rounds.

### Severity distribution (consolidated finding families in this report)

- **Critical:** 7  
- **High:** 12  
- **Medium:** 5  
- **Low:** 1  
- **Total consolidated findings:** 25 (mapped to all cycle IDs in Appendix)

**Overall risk posture:** **Low-to-Moderate residual operational risk**, with no unresolved Critical/High technical finding in the audited on-chain and keeper code paths as of `4603f9b`.

---

## 2) Scope

### Repository and code scope

- **Commit:** `4603f9bb608bda7831963444de9ac73c5ecc8f8b`
- **Primary in-scope files (requested):**
  - `solana/programs/microstable/src/lib.rs`
  - `solana/keeper/src/*.rs`

### Measured code size at audited revision

- `solana/programs/microstable/src/lib.rs`: **3,887 LOC**
- `solana/keeper/src` production modules (13 files): **8,885 LOC**
- `solana/keeper/src` all Rust files including tests: **11,045 LOC**

### Runtime/toolchain context

- **Anchor version:** `0.31.1` (from `Anchor.toml` / crate dependencies)
- **Solana client/SDK version:** `2.3.0` (keeper dependencies)
- **Pyth SDK:** `0.10.6`

---

## 3) Methodology

The audit process combined independent static review, exploit-oriented adversarial campaigns, iterative remediation, and regression verification.

1. **Manual code review**  
   Line-level review of on-chain state transitions, PDA/account constraints, signer checks, fee and collateral accounting, and keeper decision/transaction paths.

2. **Automated and compiler-level checks**  
   Rust test runs, targeted build checks, and warning-reduction hardening (including security-focused maintenance leading to zero-warning posture in recent revisions).

3. **Adversarial simulation campaigns**  
   Purple, Red, Crimson, and keeper-focused campaigns with explicit exploit hypotheses, PoC evidence, and replayable artifacts.

4. **Economic attack modeling**  
   Stress of mint/redeem asymmetry, queue fairness, collateral quality substitution, fee bypass, and governance/tournament incentive manipulation.

5. **Iterative Blue remediation + Purple re-verification**  
   Each wave tracked closure with explicit commit references and follow-up validation rounds until zero-finding convergence.

---

## 4) Findings Summary Table

| ID | Severity | Title | Status |
|---|---|---|---|
| MS-LEG-001 | Critical | Unbacked mint/redeem and accounting divergence | RESOLVED |
| MS-LEG-002 | High | Oracle authorization/freshness/feed-binding weaknesses (legacy waves) | RESOLVED |
| MS-LEG-003 | Critical | Reinitialization/migration takeover vectors | RESOLVED |
| MS-LEG-004 | High | Commit-reveal bypass/overwrite/predictability (legacy app layer) | RESOLVED |
| MS-LEG-005 | Critical | ACP identity takeover and key-binding bypass | RESOLVED |
| MS-LEG-006 | Critical | Reward claim forgery, unsigned drain, and claim-id griefing | RESOLVED |
| MS-LEG-007 | High | Watchdog resolve abuse and consensus-bypass slashing | RESOLVED |
| MS-LEG-008 | Critical | NaN/Inf numeric poisoning in staking, queue, insurance, governance | RESOLVED |
| MS-KP-001 | Critical | Single-RPC trust model and secondary-RPC optionality | RESOLVED |
| MS-KP-002 | Critical | Auto-emergency debounce/skip handling leading to shutdown abuse | RESOLVED |
| MS-KP-003 | High | Commit/reveal salt and keeper rebalance secrecy hardening | RESOLVED |
| MS-KP-004 | High | Keypair filesystem/TOCTOU/path-hardening gaps | RESOLVED |
| MS-KP-005 | Medium | Keeper quorum/member selection drift and reliability issues | RESOLVED |
| MS-KP-006 | Medium | Feed-level freshness/write-authority/identity validation gaps | RESOLVED |
| MS-KP-007 | High | Supply-chain attestation and lockfile trust-anchor weaknesses | RESOLVED |
| MS-KP-008 | Medium | Cross-RPC fail-stop and confirmation policy liveness risks | RESOLVED |
| MS-ON-001 | Critical | `devnet_force_reinit` privileged reset exposure (v8) | RESOLVED |
| MS-ON-002 | High | Rebalance commit ABI mismatch and commit deadlock chain | RESOLVED |
| MS-ON-003 | High | Agent governance capture: low-stake Sybil + deterministic selection | RESOLVED |
| MS-ON-004 | Medium | Keeper secret key material exposure/history hygiene | RESOLVED |
| MS-ON-005 | High | PM2 isolation and verification-path assurance gaps | RESOLVED |
| MS-ON-006 | Medium | Rebalance liveness dependence and broad account-scan pressure | RESOLVED |
| MS-ON-007 | High | Dynamic fee/slashing governance control defects (Red v5 set) | RESOLVED |
| MS-ON-008 | High | 3-feed/4-vault oracle update coverage mismatch | RESOLVED |
| MS-ON-009 | High | Deferred reveal slot/window integrity flaws (PV14–PV16 chain) | RESOLVED |

---

## 5) Detailed Findings (Consolidated)

> Note: each consolidated finding maps to one or more original cycle IDs; full ID mapping is provided in the Appendix.

### MS-LEG-001 — Unbacked mint/redeem and accounting divergence
- **Source IDs:** F-01, F-08, E2, PTV2-012, RV5-004 lineage
- **Description:** Earlier implementation phases allowed supply/accounting mutations without complete token-layer invariants.
- **Impact:** Potential for severe economic mismatch between internal liabilities and asset backing.
- **PoC / attack scenario:** Free-mint and manipulated redeem paths documented in early Red reports.
- **Remediation:** Enforced SPL transfer/mint/burn correctness paths and account binding constraints.
- **Verification commit(s):** `97d7e0c`, `59588c5`
- **Status:** **RESOLVED**

### MS-LEG-002 — Oracle authorization/freshness/feed-binding weaknesses
- **Source IDs:** PTV2-001~006, PTV2-023/024/028, A34
- **Description:** Legacy surfaces showed gaps in keeper auth checks, feed binding, freshness semantics, and test coverage.
- **Impact:** Oracle poisoning, stale replay acceptance, and degraded risk controls.
- **PoC:** Unauthorized or weakly bound oracle update paths in post-integration reports.
- **Remediation:** Quorum enforcement, feed allowlists, freshness bounds, parser and account-level checks.
- **Verification commit(s):** `0e26aad`, `fd678fc`, `6e6b623`
- **Status:** **RESOLVED**

### MS-LEG-003 — Reinitialization/migration takeover vectors
- **Source IDs:** PTV2-007~010, A35
- **Description:** Re-runnable migration/reinit paths and mutable control-plane fields created takeover/reset exposure.
- **Impact:** Keeper set remap, supply/deposit reset risk, governance instability.
- **PoC:** Migration replay and field reset scenarios in Purple v2.
- **Remediation:** One-shot protections, trusted initializer hardening, guarded migration semantics.
- **Verification commit(s):** `0e26aad`, later hardening maintained through v13+
- **Status:** **RESOLVED**

### MS-LEG-004 — Commit-reveal bypass/overwrite/predictability in app-layer flows
- **Source IDs:** PT-003, PT-004, PT-017, PT-018, PTV2-022, PTV3-012, A24
- **Description:** Multiple early bypasses allowed overwrite, split-threshold evasion, or deterministic proof construction.
- **Impact:** Rebalance/gameability and front-running surface expansion.
- **PoC:** Overwrite and threshold edge paths repeatedly reproduced in Red/Purple rounds.
- **Remediation:** Stricter commit semantics, anti-overwrite checks, threshold logic hardening, stronger proof binding.
- **Verification commit(s):** `0241f7a`, `0e26aad`, `59588c5`
- **Status:** **RESOLVED**

### MS-LEG-005 — ACP identity takeover and key-binding bypass
- **Source IDs:** PT-011, PT-012, PT-013, PTV2-018, PTV2-019, PTV3-007, PTV3-009, A16, A17, A20
- **Description:** Legacy ACP verification and key-registration paths permitted replay, spoofing, or actor/key reassignment weaknesses.
- **Impact:** Agent impersonation and governance-message forgery.
- **PoC:** `set_public_key` takeover and replay acceptance in Red/Purple traces.
- **Remediation:** Authenticated key rotation, nonce/expiry enforcement, stronger actor-key binding.
- **Verification commit(s):** `0241f7a`, `0e26aad`, `59588c5`
- **Status:** **RESOLVED**

### MS-LEG-006 — Reward claim forgery, unsigned-drain, and claim-id griefing
- **Source IDs:** PT-001, PTV3-001~003, A01, A02, A12, CT-S01, CT-S02, CT-C03
- **Description:** Reward paths previously permitted forged proofs, unsigned micro-claim abuse, and globally colliding claim IDs.
- **Impact:** Reward pool draining, honest-claim denial, incentive-system corruption.
- **PoC:** Forged claim and micro-fragmentation exploits across multiple campaigns.
- **Remediation:** Proof enforcement, cap handling, safer claim scoping and validation guards.
- **Verification commit(s):** `0241f7a`, `0e26aad`, `59588c5`
- **Status:** **RESOLVED**

### MS-LEG-007 — Watchdog resolve abuse and consensus-bypass slashing
- **Source IDs:** PT-008~010, PTV3-010, A12, CT-S06, CT-C01
- **Description:** Historical resolve flows allowed unilateral reward/slash outcomes without robust consensus gating.
- **Impact:** Monitor suppression, false-penalty griefing, bounty abuse.
- **PoC:** False resolve and reporter slash in Red/Crimson exploit chains.
- **Remediation:** Stronger resolution authorization and idempotent settlement controls.
- **Verification commit(s):** `0e26aad`, `59588c5`
- **Status:** **RESOLVED**

### MS-LEG-008 — NaN/Inf poisoning in staking, queue, insurance, and scoring
- **Source IDs:** PTV3-004, PTV3-005, PTV3-014~020, A08~A11, A14, A22, CT-N01~N07
- **Description:** Numeric edge handling in legacy Python paths allowed non-finite value propagation and DoS/state corruption.
- **Impact:** Withdrawal forgery, queue failure, treasury inflation, governance scoring corruption.
- **PoC:** NaN deposit/withdraw and invalid-claim refill chains.
- **Remediation:** Finite-value guards, stricter type/range checks, safer ordering in treasury/queue logic.
- **Verification commit(s):** `07874dd`, `59588c5`
- **Status:** **RESOLVED**

---

### MS-KP-001 — Single-RPC trust and secondary-RPC optionality
- **Source IDs:** PK-001, RK-003, PKV3-003, PKV5-002
- **Description:** Early keeper versions could regress to single-endpoint trust for read/confirmation semantics.
- **Impact:** Spoof/censor risk and false-success reporting on privileged flows.
- **PoC:** Optional secondary config and primary-only acceptance cases.
- **Remediation:** Secondary mode controls, stricter runtime policy, resilient degraded-mode boundaries.
- **Verification commit(s):** `3515167`, verified by `ebcfd74`
- **Status:** **RESOLVED**

### MS-KP-002 — Auto-emergency debounce/skip handling vulnerabilities
- **Source IDs:** PK-002, RK-005
- **Description:** One-shot trigger and skipped-cycle handling enabled abusive shutdown progression.
- **Impact:** Protocol halt liveness risk under manipulated or transient conditions.
- **PoC:** Debounce accumulation paths documented in Red-Keeper.
- **Remediation:** Debounce reset correctness, safer monitor state transitions.
- **Verification commit(s):** `bc400cc`, maintained through keeper v6
- **Status:** **RESOLVED**

### MS-KP-003 — Commit/reveal salt secrecy hardening
- **Source IDs:** PK-003
- **Description:** Deterministic reveal salt undermined secrecy objective for rebalances.
- **Impact:** Front-running and strategy inference risk.
- **PoC:** Slot-derived predictable salt reconstruction.
- **Remediation:** Entropy-backed reveal salt generation and hardened pending-reveal flow.
- **Verification commit(s):** `bc400cc`
- **Status:** **RESOLVED**

### MS-KP-004 — Key management path hardening (permissions, TOCTOU, PATH)
- **Source IDs:** PK-004, RK-009, MSV8-005
- **Description:** Filesystem trust assumptions permitted key-load race/path hijack classes.
- **Impact:** Local key substitution and signer compromise exposure.
- **PoC:** Validate-read split and PATH-dependent UID checks.
- **Remediation:** Secure file open/validation sequencing, `geteuid()` usage, stricter key file policy.
- **Verification commit(s):** `bc400cc`, `316f49b`
- **Status:** **RESOLVED**

### MS-KP-005 — Quorum selection and keeper membership reliability
- **Source IDs:** PK-005
- **Description:** Static key ordering assumptions reduced resilience during rotation/config drift.
- **Impact:** Critical operation failures and liveness fragility.
- **PoC:** First-two-key static quorum behavior.
- **Remediation:** Keeper-set aligned selection and rotation-aware checks.
- **Verification commit(s):** `bc400cc`
- **Status:** **RESOLVED**

### MS-KP-006 — Feed freshness/identity/write-authority consistency
- **Source IDs:** PK-006, PK-007
- **Description:** Configured per-feed controls and metadata validation were incompletely enforced in early keeper revisions.
- **Impact:** Stale or malformed oracle input can influence keeper decisions.
- **PoC:** Global-age-only checks and ignored feed/write-authority fields.
- **Remediation:** Per-feed max-age enforcement and strict identity/authority validation.
- **Verification commit(s):** `bc400cc`, verified in `4cd67f4`/later rounds
- **Status:** **RESOLVED**

### MS-KP-007 — Supply-chain trust anchors and dependency source policy
- **Source IDs:** RK-020, PKV2-002, PKV2-003, PKV3-002, PKV4-001
- **Description:** Attestation and lockfile policy initially depended on runtime-overridable trust and incomplete source allowlists.
- **Impact:** Trojan binary/dependency injection risk.
- **PoC:** Runtime env override and non-crates registry acceptance scenarios.
- **Remediation:** Cargo.lock hash attestation model, stricter source policy, startup enforcement.
- **Verification commit(s):** `1f31310`, `3515167`
- **Status:** **RESOLVED**

### MS-KP-008 — Cross-RPC strictness and fail-stop liveness balancing
- **Source IDs:** PKV3-001, PKV4-002, PKV5-001
- **Description:** Exact-equality and short confirmation windows produced avoidable fail-stop behavior.
- **Impact:** Keeper exit loops under benign drift or secondary instability.
- **PoC:** Persistent mismatch / split-confirmation chains.
- **Remediation:** Secondary RPC runtime modes, bounded retries, degraded-mode routing with controlled trust downgrade.
- **Verification commit(s):** `3515167`, verified by `ebcfd74`
- **Status:** **RESOLVED**

---

### MS-ON-001 — `devnet_force_reinit` privileged reset exposure
- **Source IDs:** MSV8-001
- **Description:** Reinit instruction was exposed without sufficient production gating.
- **Impact:** Governance/state reset and takeover blast radius.
- **PoC:** Force reset of protocol and keeper state via privileged call.
- **Remediation:** Feature-gated devnet-only path; non-default build exclusion.
- **Verification commit(s):** `316f49b`, re-verified in `c81eb4c`, `68cea49`
- **Status:** **RESOLVED**

### MS-ON-002 — Rebalance commit ABI mismatch and deadlock chain
- **Source IDs:** MSV8-002, MSV9-001
- **Description:** Keeper/on-chain account ABI mismatch and submitter-account assumptions caused commit-stage failures.
- **Impact:** Rebalance path stall and repeated cycle failure pressure.
- **PoC:** Missing commit accounts and invalid `agent_record` PDA usage.
- **Remediation:** ABI sync, account inclusion fixes, submitter resolution hardening.
- **Verification commit(s):** `316f49b`, `b4ab9f6`
- **Status:** **RESOLVED**

### MS-ON-003 — Agent governance capture (low stake + deterministic selection)
- **Source IDs:** MSV8-003, MSV9-002, RV5-006
- **Description:** Early governance-agent mechanics enabled cheap identity fan-out and deterministic capture.
- **Impact:** Tournament/AIG influence capture and tier/score distortion.
- **PoC:** Minimal stake registrations dominating first-N selection.
- **Remediation:** Higher minimum stake, stake-weighted randomized selection, quorum-gated score/promote/demote.
- **Verification commit(s):** `b4ab9f6`, maintained through v13+
- **Status:** **RESOLVED**

### MS-ON-004 — Keeper secret material exposure and history hygiene
- **Source IDs:** MSV8-004, MSV9-003
- **Description:** Repository-tracked key material required de-tracking and history hygiene.
- **Impact:** Potential signer compromise if keys reused.
- **PoC:** Historic key blob retrieval from prior commits.
- **Remediation:** Key rotation, history cleanup, ignore/scanning discipline.
- **Verification commit(s):** `b4ab9f6` and subsequent history verification entries
- **Status:** **RESOLVED**

### MS-ON-005 — PM2 isolation assurance gaps and script regressions
- **Source IDs:** MSV8-006, MSV9-004, MSV11-002, MSV12-001, RV5-008
- **Description:** Operational isolation and verification tooling had periods of false-assurance behavior.
- **Impact:** Cross-process metadata exposure and weakened operational trust assumptions.
- **PoC:** Shared PM2 domain observation; script parsing failure regression.
- **Remediation:** Canonicalized runtime checks, strict verification mode, script fixes and revalidation.
- **Verification commit(s):** `49dd8d8`, `b4e2e09`, re-verified in `68cea49`
- **Status:** **RESOLVED**

### MS-ON-006 — Rebalance liveness dependence and account-scan pressure
- **Source IDs:** MSV10-001, MSV10-002, MSV11-001
- **Description:** Local key custody assumptions and broad account scans created liveness/performance risk.
- **Impact:** Commit path skipping and keeper cycle reliability degradation.
- **PoC:** No local eligible signer path and unfiltered account discovery pressure.
- **Remediation:** Eligibility preflight + operator fail-fast mode, filtered scans (`memcmp` + `datasize`).
- **Verification commit(s):** `4a21f9c`, `49dd8d8`
- **Status:** **RESOLVED**

### MS-ON-007 — Dynamic fee and slashing governance defects (Red v5)
- **Source IDs:** RV5-005, RV5-013
- **Description:** Fee parameter enforcement and slashing authority/cap/cooldown were previously incomplete.
- **Impact:** Economic control bypass and unilateral punitive abuse risk.
- **PoC:** Fee-rate mismatch and trusted-initializer unilateral slash scenario.
- **Remediation:** Correct mint/redeem fee-field application, quorum-gated slash, slash cap, cooldown.
- **Verification commit(s):** `3c54de2`
- **Status:** **RESOLVED**

### MS-ON-008 — 3-feed/4-vault oracle coverage mismatch
- **Source IDs:** RV5-007
- **Description:** Keeper config could run with incomplete feed coverage relative to vault count.
- **Impact:** Global mint liveness degradation via single stale vault.
- **PoC:** 3-feed config causing persistent oracle-degraded state for one vault.
- **Remediation:** Exact feed count/index coverage enforcement and config validation hardening.
- **Verification commit(s):** `3c54de2`
- **Status:** **RESOLVED**

### MS-ON-009 — Deferred reveal slot/window integrity flaws (PV14→PV16)
- **Source IDs:** PV14-001, PV15-001, PV16-001, PV16-002
- **Description:** Multiple sequential liveness flaws were found in deferred reveal slot matching, preimage slot freezing, and window margin logic.
- **Impact:** Commit-expire loops and rebalance execution stall under normal slot drift/boundary conditions.
- **PoC:** Deterministic dead-zone and `CommitRevealMismatch` scenarios across v14–v16 audits.
- **Remediation:** Commit-time `batch_slot` freeze, deferred readiness hardening, `delay+2` window safety margin.
- **Verification commit(s):** `39262ea`, `f85d495`, `8bc3334`, final closure `2d832ce`
- **Status:** **RESOLVED**

---

## 6) Security Properties Verified

### Access control
- Keeper quorum enforcement is consistently required for privileged on-chain operations.
- Duplicate-signer quorum forgery is blocked.
- Agent eligibility checks (`Active`, tier thresholds) and registration constraints are in place.

### Oracle safety
- Feed ID binding, write-authority checks, and freshness bounds are enforced.
- Keeper-side per-feed controls and cross-RPC consistency paths were hardened over successive rounds.
- Oracle misconfiguration classes (including feed coverage gaps) were explicitly tested and remediated.

### Economic invariants
- Mint/redeem accounting is now aligned with enforced token movement and fee semantics.
- Dynamic fee parameters are wired to execution paths.
- Slashing controls now include quorum authorization and bounded punishment parameters.

### Circuit breaker behavior
- Debounce/reset logic and recovery behavior were iteratively tightened.
- Extended fail-stop and module error-handling semantics were reviewed through keeper v6.

### Commit-reveal integrity
- Commit preimage components and reveal verification were repeatedly stressed.
- Deferred reveal path underwent four consecutive rounds of hardening until v17 zero-finding closure.

### Agent governance and Sybil resistance
- Minimum stake requirements increased materially.
- Selection shifted from deterministic first-claim patterns to weighted/entropy-based logic.
- Score/promotion/demotion controls moved to stronger quorum semantics.

### Keeper isolation and runtime hardening
- PM2 domain isolation checks, strict-mode verification, and canonical path handling were added and re-verified.
- Supply-chain controls evolved from weak runtime trust assumptions toward enforceable startup checks.

---

## 7) Architecture Review

### On-chain / off-chain trust boundary
Microstable separates deterministic state enforcement on-chain from keeper-side sensing/decision orchestration. The final security posture depends on preserving this boundary:

- **On-chain:** canonical source of truth for collateral, supply, keeper quorum, and commit-reveal validity.
- **Off-chain keeper:** proposes and submits actions; must not be treated as a trusted oracle of truth itself.

The audit history demonstrates that mismatch between these layers was a recurrent risk source and is now materially reduced.

### Keeper quorum model
The current model converged from single-key operational fragility toward sustained 2-of-3 semantics with duplicate-signer rejection and staged keeper rotation controls. This materially reduced unilateral action risk.

### Oracle dependency model
Oracle trust shifted from permissive integration assumptions to explicit feed/account/write-authority/freshness controls, with feed-coverage and staleness handling integrated at both config and execution levels.

### Upgrade/migration path
Historical migration and reinit findings show that safety around upgrade and migration is a primary governance risk surface. Current controls provide stronger gating, but operational key governance remains a critical discipline requirement.

---

## 8) Recommendations

Although no unresolved Critical/High technical finding remains in scope, the following recommendations are retained for operational defense-in-depth:

1. **Mainnet-grade runtime attestation policy**  
   Keep supply-chain controls anchored to immutable CI provenance and signed release artifacts.

2. **Independent monitoring and anomaly alerts**  
   Maintain external telemetry for keeper cycle failures, repeated commit-expiry, and oracle freshness divergence.

3. **Operational key governance drills**  
   Continue periodic key rotation rehearsal, signer compromise simulations, and incident runbooks.

4. **Rebalance liveness SLOs**  
   Track commit/reveal success rate and time-to-rebalance under adversarial slot/jitter scenarios.

5. **Test regimen continuity**  
   Preserve adversarial regression suites (Purple/Red/Crimson vectors) as release gates.

---

## 9) Appendix

### A. Security cycle timeline (summary)

| Wave | Findings at round | Convergence / Outcome | Key commits |
|---|---:|---|---|
| Red v1 + Yellow | 12 + 21 exploitable vectors | Immediate critical fixes | `c0f4f2f`, `97d7e0c`, `07874dd` |
| Purple v1 | 27 | Patched in Blue v2 | `6772f19` → `0241f7a` |
| Purple v2 | 28 | Patched in Blue v3 | `82925d2` → `0e26aad` |
| Purple v3 + Red v3/v4 + Crimson | multi-wave residuals | Patched through Blue v4 series | `a62b703`, `28acf68`, `81061fb`, `dddd5e9`, `59588c5` |
| Keeper v1→v6 | 10→3→3→2→2→0 | Final zero-finding keeper closure | `a2ae01f`, `bc400cc`, `5a088fb`, `1f31310`, `3515167`, `ebcfd74` |
| Main v8→v13 | 6→5→3→2→1→0 | Final zero for main cycle | `154992f` → `316f49b` → `b4ab9f6` → `4a21f9c` → `49dd8d8` → `b4e2e09` → `68cea49` |
| Deferred v14→v17 | 1→1→2→0 | Deferred-reveal closure | `86477b3`, `39262ea`, `e47f38b`, `f85d495`, `0615edc`, `8bc3334`, `2d832ce` |

### B. Main v8–v17 closure matrix

- v8 findings: `MSV8-001..006` → closed by `316f49b`, re-verified through v13/v17.
- v9 findings: `MSV9-001..005` → closed by `b4ab9f6`, maintained.
- v10 findings: `MSV10-001..003` → closed by `4a21f9c` / `49dd8d8`.
- v11 findings: `MSV11-001..002` → closed by `49dd8d8`.
- v12 finding: `MSV12-001` → closed by `b4e2e09`.
- v14/v15/v16 findings: `PV14-001`, `PV15-001`, `PV16-001`, `PV16-002` → closed by `39262ea`, `f85d495`, `8bc3334`; verified in `2d832ce`.

### C. Keeper v1–v6 closure matrix

- `PK-001..010` (v1) — closed across Blue-Keeper hardening path (`bc400cc`).
- `PKV2-001..003` — closed/maintained by subsequent keeper rounds.
- `PKV3-001..003` — addressed in v4/v5/v6 evolution.
- `PKV4-001..002` — design-level closure via v5.
- `PKV5-001..002` — final closure in v6 (`3515167`), verified by `ebcfd74`.

### D. Test coverage and verification evidence (consolidated)

- Keeper suite (v17 report): multi-suite pass set including **231+ keeper tests** (aggregate listed in v17 evidence).
- Program library tests (v17 report): **29/29 pass**.
- Historical protocol test statements: 126+ tests passed in prior campaign summaries; no unresolved failing security regression reported in final cycles.

### E. Tooling used

- Rust compiler + cargo test/check
- Anchor framework (Anchor 0.31.1)
- Solana SDK/client 2.3.0
- Adversarial PoC harnesses (`tests/red-team-*`, `tests/crimson-team/*`)
- Keeper isolation verification script and runtime checks
- Manual source audit with commit-level traceability

### F. Source-ID coverage reference

This formal report consolidates all findings from:
- Red v1–v5,
- Purple v1–v3,
- Crimson,
- Keeper Purple v1–v6 and Red-Keeper,
- Main Purple cycles v8–v17,
- Blue remediation waves v1–v13.

Legacy IDs (PT/PTV/A/CT/F/E/G/H/I families) are grouped into `MS-LEG-*`; keeper IDs (PK/PKV/RK) into `MS-KP-*`; main/deferred IDs (MSV/PV/RV5) into `MS-ON-*`.

---

## 10) Final Statement

Based on the reviewed evidence, iterative adversarial validation, and closure verification through final rounds (Purple v17, Keeper v6), the current Microstable codebase revision demonstrates a substantially hardened security posture relative to earlier campaign phases.

No unresolved Critical/High finding remains in the audited on-chain and keeper code paths at the time of this report.
