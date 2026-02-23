# Microstable Security Audit — Purple Team v9 (Full Re-Verification, MAX)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v9
- Scope:
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper daemon: `solana/keeper/src/*.rs` (19 files)
  - Deployment runtime: MiniPC PM2 + keypair handling
  - Prior reports: `docs/security/purple-red-v8-max.md`, `docs/security/ops-hardening.md`

---

## MSV8 Patch Re-Verification Matrix

| ID | Prior Severity | Patch Claim | v9 Result |
|---|---:|---|---|
| MSV8-001 | CRITICAL | `devnet_force_reinit` gated by `#[cfg(feature="devnet-admin")]` | **VERIFIED (code gate present, default feature off)** |
| MSV8-002 | HIGH | `ix_commit_rebalance` includes `agent_record` + `submitting_agent` | **VERIFIED (ABI sync done)** |
| MSV8-003 | HIGH | Agent loop reads on-chain registered agents, excludes keeper keys, sybil-resistant | **PARTIAL / REOPENED (keeper exclusion done, sybil resistance still weak)** |
| MSV8-004 | MEDIUM | `.gitignore` keypair update + history cleanup | **PARTIAL (ignore+de-track done, history cleanup not done)** |
| MSV8-005 | MEDIUM | `effective_uid()` switched to `libc::geteuid()` | **VERIFIED** |
| MSV8-006 | HIGH | PM2 env isolation operationalized on MiniPC | **NOT OPERATIONAL (doc exists, runtime isolation incomplete)** |

---

## Finding: MSV9-001
- Severity: HIGH
- Component:
  - `solana/keeper/src/rebalance.rs:280-290, 292-299`
  - `solana/programs/microstable/src/lib.rs:2150-2157, 1212-1215, 3383-3396`
  - `solana/keeper/src/main.rs:352-355, 467-470, 199-210`
- Attack Vector:
  Keeper rebalance commit path hardcodes `submitting_agent = k1.pubkey()` and derives `agent_record` PDA from that keeper key. On-chain `CommitRebalance` now *requires* a real `AgentRecord` account for `submitting_agent`, `status == Active`, and `tier >= 2`.
  If keeper keys are not registered as tier-2 agents, commit fails every cycle.
- Impact:
  Rebalance pipeline becomes structurally unavailable; repeated cycle failures trigger daemon protective exit (`max_consecutive_failed_cycles`). PM2 restart churn follows, keeping rebalance unavailable.
- PoC:
  1. Keeper sends commit with:
     - `submitting_agent = k1.pubkey()` (`rebalance.rs:280`)
     - `agent_record = PDA("agent", submitting_agent)` (`rebalance.rs:284`)
  2. On-chain expects seeded `Account<AgentRecord>` (`lib.rs:2150-2155`) and eligibility (`lib.rs:1212-1215`, `3383-3396`).
  3. Runtime evidence (MiniPC): `/home/spritz/.pm2/logs/microstable-keeper-out.log` repeatedly shows:
     - `AnchorError ... agent_record ... AccountOwnedByWrongProgram` (system account at PDA)
     - `cycle failed ... one or more module steps failed in cycle: rebalance`
     - repeated consecutive-failure progression to guardrail exit.
- Remediation:
  1. Do not hardcode keeper key as `submitting_agent`.
  2. Resolve eligible external agent (`Active && tier>=2`) from on-chain registry before commit.
  3. Add preflight check in keeper to verify `agent_record` existence/eligibility before tx send.

## Finding: MSV9-002 (MSV8-003 Reopened)
- Severity: HIGH
- Component:
  - `solana/programs/microstable/src/lib.rs:72, 3301-3305, 405-406, 415-416, 426-427`
  - `solana/keeper/src/agent_loop.rs:320-321, 324-352, 241-251`
- Attack Vector:
  Patch switched participant source from keepers to registered agents, but sybil-cost and selection rules remain weak:
  - Registration minimum stake is still `1 lamport` (`lib.rs:72`, `3301-3305`).
  - Agent selection is deterministic-first (`select_candidate_agent = first`, tournament `truncate(2)`) (`agent_loop.rs:320-321`, `250`).
  - Score/tier mutation remains single keeper signer (membership-only, not quorum) (`lib.rs:405-406`, `415-416`, `426-427`).
- Impact:
  Low-cost sybil set can dominate AIG/tournament candidate slots and associated score/tier path, undermining intended anti-sybil governance hardening.
- PoC:
  1. Create many agent accounts with minimal stake (1 lamport each).
  2. `fetch_registered_agents()` includes all active non-keeper records (`agent_loop.rs:324-352`) and sorts list.
  3. AIG uses first entry (`320-321`), tournament uses first two (`241-251`), enabling deterministic capture by sybil set.
- Remediation:
  1. Raise minimum stake materially and enforce economic anti-sybil constraints.
  2. Replace deterministic first-N selection with stake/reputation-weighted randomized selection.
  3. Move score/tier mutation to keeper quorum.

## Finding: MSV9-003 (MSV8-004 Incomplete)
- Severity: MEDIUM
- Component:
  - `.gitignore:10-14`
  - Git history: commit `96b5d8e` still contains `solana/keeper/keeper2.json`, `solana/keeper/keeper3.json` (64-byte secret arrays)
- Attack Vector:
  Current tree de-tracks key files, but historical commits still preserve private key material.
- Impact:
  Anyone with repository history can recover old signer secrets; if keys were reused operationally, signer compromise risk remains.
- PoC:
  - `git log --all -- solana/keeper/keeper2.json solana/keeper/keeper3.json` returns historical commits.
  - `git show 96b5d8e:solana/keeper/keeper2.json` returns a 64-element secret array.
- Remediation:
  1. Perform repository history rewrite (`git filter-repo`/BFG) for exposed key files.
  2. Rotate/revoke all exposed keypairs and verify zero reuse.

## Finding: MSV9-004 (MSV8-006 Not Operational)
- Severity: HIGH
- Component:
  - Policy doc: `docs/security/ops-hardening.md:19-26, 40, 61-63`
  - MiniPC runtime: `/home/spritz/microstable-keeper/ecosystem.config.js:1-15`, PM2 process domain
- Attack Vector:
  Hardening doc requires `.env`-based secret loading, strict file permissions, and PM2 isolation. Actual MiniPC runtime remains shared PM2 domain (`~/.pm2`, namespace `default`) with co-tenant apps, and `pm2 jlist` still exposes environment metadata for other processes.
- Impact:
  Cross-app credential exposure/lateral movement risk remains on shared PM2 runtime.
- PoC:
  - `/home/spritz/microstable-keeper/.env` is missing (runtime check).
  - `PM2_HOME` unset; keeper process runs in default shared PM2 home/namespace.
  - `pm2 jlist` output includes sensitive env key names for other apps (e.g., `CLERK_SECRET_KEY`, `OPENAI_API_KEY`) while keeper shares same PM2 domain.
- Remediation:
  1. Dedicated OS user + dedicated `PM2_HOME` for keeper.
  2. Remove keeper from shared `default` PM2 domain.
  3. Enforce `.env` with `600` permissions and least-privilege secret access.

## Finding: MSV9-005
- Severity: LOW
- Component:
  - On-chain struct: `solana/programs/microstable/src/lib.rs:2251-2254`
  - Keeper wire struct: `solana/keeper/src/wire.rs:20-24`
  - Decoder behavior: `solana/keeper/src/wire.rs:150-151`
- Attack Vector:
  Keeper-side `wire::ProtocolState` omits on-chain fields `pending_keeper_set` and `pending_keeper_activation_slot`; decode path does not enforce full payload consumption.
- Impact:
  Silent ABI drift: keeper ignores pending keeper-rotation fields and interprets trailing bytes with stale layout assumptions.
- PoC:
  - On-chain layout includes extra fields before `bump` (`lib.rs:2251-2254`).
  - Keeper struct jumps directly to `bump` after `pending_rebalance_expiry` (`wire.rs:20-24`).
  - `decode_account()` deserializes without checking remainder (`wire.rs:150-151`), so drift is not rejected.
- Remediation:
  1. Synchronize keeper wire struct with on-chain account layout.
  2. Reject decode when payload remainder is non-empty.

---

## Verified Fixed Items

### MSV8-001 — Verified
- `devnet_force_reinit` function and account context are both behind `#[cfg(feature = "devnet-admin")]`:
  - `solana/programs/microstable/src/lib.rs:1639-1643`
  - `solana/programs/microstable/src/lib.rs:2220-2223`
- `devnet-admin` is **not** in default features:
  - `solana/programs/microstable/Cargo.toml:11-22`

### MSV8-002 (ABI sync) — Verified
- On-chain `CommitRebalance` requires `agent_record` + `submitting_agent`:
  - `solana/programs/microstable/src/lib.rs:2145-2159`
- Keeper builder now includes those accounts:
  - `solana/keeper/src/wire.rs:203-230`

### MSV8-005 — Verified
- `effective_uid()` uses `libc::geteuid()`; no PATH command call remains:
  - `solana/keeper/src/utils.rs:393-394`

---

## Final Assessment

- **ZERO NEW FINDINGS**: **NO**
- New/Reopened findings: **5** (HIGH×3, MEDIUM×1, LOW×1)
- Most urgent: **MSV9-001 (rebalance commit deadlock)** and **MSV9-004 (PM2 isolation not operational)**.
