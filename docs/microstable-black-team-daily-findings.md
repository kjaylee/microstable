# Microstable Black Team — Daily Security Scan Findings

> Format: date-stamped sections, vectors verdict (✅ DEFENDED / ⚠️ PARTIAL / ❌ UNDEFENDED), CRITICAL/HIGH trigger Discord alert.

---

## 2026-03-20 (KST 03:00)

**Evolution delta**: +0 new vectors; 3 reinforcements applied (D26 Permit2 persistence, A41 AM/USDT burn, A4 Injective chain-level bypass).  
**Sources scanned**: rekt.news, hacked.slowmist.io, SearXNG fallback, dev.to (dTRINITY analysis), ainvest.com (Injective), cryptonewsz.com (Neutrl), CryptoTimes.  
**New incidents scanned**: Neutrl DNS hijack (2026-03-19), Injective $500M access control bypass (disclosed 2026-03-16), AM/USDT pool burn manipulation (2026-03-12), dTRINITY dLEND index attack (2026-03-17/18, A68 already logged in prior run).

### New Vector/Reinforcement Assessment (D26 + A41 + A4 reinforcement)

| Vector | Verdict | Evidence |
|---|---|---|
| D26 (Neutrl DNS hijack + Permit2 persistence) | ✅ N/A (Solana) | Neutrl (2026-03-19): DNS provider social-engineered → domain redirected. Permit2 (Ethereum EIP-2612 universal approval contract by Uniswap) persistence makes frontend compromises vastly more dangerous: users may have granted persistent cross-protocol spending permissions that survive the dApp session. Microstable is Solana-native; no Permit2 equivalent exists on Solana. SPL Token approvals (via `delegate`) are bounded per-ATA and would only be an issue if users pre-delegated their collateral ATA — covered by B44 (open MEDIUM). |
| D26 (dashboard CSP server-level) | ⚠️ PARTIAL carry-forward | CSP meta-tag present; server-level HTTP header unconfirmed. No Ethereum wallet or Permit2 exposure. No change from prior cycle. |
| A41 (AM/USDT burn-reserve manipulation) | ✅ DEFENDED | AM/USDT pool (BSC, 2026-03-12, $131K): manipulated `toBurnAmount` in burn logic to distort reserves, then sold at inflated price. Microstable has no AMM pool, no burn-before-payout mechanism, and no mining/reward contract. Redeem path: fee computed before burn, CEI pattern enforced. |
| A4 (Injective chain-level auth bypass) | ✅ DEFENDED | Injective (2026-03-16, $500M at risk, $0 lost): any user could drain any account on the Cosmos SDK chain without special permissions — chain-level authorization module bug, not a contract-level exploit. Microstable is Solana-native. All operations gate on `TRUSTED_INITIALIZER` + 2-of-3 keeper quorum + Anchor `has_one`/`constraint` enforcement. No analog to Injective's cross-account drain path. |
| A68 (dTRINITY lending pool index inflation) | ✅ N/A | dTRINITY (2026-03-17, $257K, A68 previously logged). Microstable is not a lending pool and has no liquidity index mechanism. Not applicable. |

### Carry-Forward Open Findings

| Vector | Severity | Status | Detail |
|---|---|---|---|
| B45 Post-Audit Deployment Delta | **HIGH** | ⚠️ OPEN — DAY 15 | `audit-attestation.json` absent. 3,281+ lines of code post-audit have no formal sign-off document. Delta includes oracle confidence penalty, TWAP, flow controls, keeper rotation. Priority: commission attestation or formal incremental review. |
| A43 Commit/Reveal Threshold Circumvention | MEDIUM | ⚠️ OPEN | No cumulative drift accumulator across rebalance calls. Per-call `WEIGHT_STEP_LIMIT=2%` enforced; no epoch-level tracking of total weight movement. Multiple small calls can cumulatively drift weights beyond safe bounds. |
| B44 SPL Token Delegate Drain | MEDIUM | ⚠️ OPEN | No `delegate.is_none()` guard on user ATAs in mint/redeem. A user with a pre-set SPL delegate on collateral ATA could see collateral withdrawn by the delegate after depositing. Low probability but completable without victim interaction post-setup. |
| D26/D28 Static Asset Integrity | LOW | ⚠️ OPEN | `vendor/solana-web3-1.95.3.iife.min.js` self-hosted without SRI integrity hash. Carry-forward. |

### Summary (2026-03-20)

| Severity | Count | Vectors |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | B45 (attestation gap, carry-forward, DAY 15) |
| MEDIUM | 2 | A43, B44 |
| LOW / INFO | 1 | D26/D28 SRI |
| NEW TODAY | 0 | (all 3 reinforcements ✅ N/A or DEFENDED for Microstable) |

**No CRITICAL or HIGH new findings today. No Discord alert triggered.**  
**Carry-forward alert**: B45 HIGH has been open 15 days. Recommend scheduling formal attestation review this sprint.

---

## 2026-03-13 (KST 03:00)

**Evolution delta**: +1 new vector today (A61 ERC-2771 Meta-Transaction Sender Context Inconsistency from DBXen $150K exploit 2026-03-12); D26 reinforced (bonk.fun domain hijack 2026-03-12).  
**Sources scanned**: rekt.news, hacked.slowmist.io, CryptoTimes, Brave Search, BlockSec Phalcon.  
**New incidents**: DBXen (ERC-2771, $150K), bonk.fun (domain hijacking, unknown loss).

### New Vector Assessment (A61, D26 escalation)

| Vector | Verdict | Evidence |
|---|---|---|
| A61 ERC-2771 Sender Mismatch | ✅ N/A | Microstable is Solana-native (Anchor framework). No meta-transaction relay abstraction, no `msg.sender`/`_msgSender()` duality. All signer resolution via `ctx.accounts.*.is_signer` — immune to ERC-2771 pattern. |
| D26 (domain hijack escalation) | ⚠️ PARTIAL | Dashboard has `script-src 'self'` CSP via HTML **meta tag** — blocks XSS-injected scripts but NOT server-level script injection if domain is compromised. No server-level HTTP `Content-Security-Policy` header confirmed (static file serving via GitHub Pages / custom domain). Operational carry-forward: keeper domain account MFA status unknown. LOW severity. |

### Carry-Forward Open Findings

| Vector | Severity | Status | Detail |
|---|---|---|---|
| B45 Post-Audit Deployment Delta | **HIGH** | ⚠️ OPEN | `audit-attestation.json` absent. 3,281+ lines of code post-audit have no formal sign-off document. Unattested delta includes oracle confidence penalty, TWAP, flow controls, and keeper rotation logic. Priority: commission attestation or formal incremental review. |
| A43 Commit/Reveal Threshold Circumvention | MEDIUM | ⚠️ OPEN | No cumulative drift accumulator across rebalance calls. Per-call `WEIGHT_STEP_LIMIT=2%` enforced; `TURNOVER_LIMIT=15%` per call; but no epoch-level tracking of total weight movement. Multiple small calls can cumulatively drift weights beyond safe bounds without triggering the large-rebalance commit-reveal gate. |
| B44 SPL Token Delegate Drain | MEDIUM | ⚠️ OPEN | No `delegate.is_none()` guard on user ATAs in mint/redeem paths. A user with a previously-set SPL delegate on their collateral ATA could see their collateral withdrawn by the delegate after depositing into Microstable. Requires pre-existing victim delegation (low probability) but is completable without victim interaction after setup. |
| D26/D28 Static Asset Integrity | LOW | ⚠️ OPEN | `vendor/solana-web3-1.95.3.iife.min.js` self-hosted but lacks SRI integrity hash in HTML; Cargo.lock transitive dep attestation not confirmed in CI. |

### Summary (2026-03-13)

| Severity | Count | Vectors |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | B45 (attestation gap, pre-existing carry-forward) |
| MEDIUM | 2 | A43, B44 |
| LOW / INFO | 2 | D26 domain risk, D28 SRI |
| NEW TODAY | 0 (A61 N/A, D26 reinforced only) | |

**A61 verdict for Microstable**: ✅ NOT APPLICABLE. ERC-2771 is an EVM-only pattern.  
**D26 domain hijack escalation**: LOW carry-forward. No code change needed; operational: ensure keeper/dashboard domain account has MFA and isolated credentials.  
**No CRITICAL or HIGH new findings today. No Discord alert triggered.**

---

## 2026-03-02 (KST 03:00)

**Evolution delta**: +1 new incident (Holdstation DeFAI $462K), B15 DeFAI amplification note added.  
**Sources scanned**: rekt.news, hacked.slowmist.io, immunefi.com/blog, Brave Search.

### Full Vector Scan (38+ vectors)

#### A. Smart Contract Vectors

| # | Vector | Verdict | Evidence |
|---|---|---|---|
| A1 | Reentrancy | ✅ DEFENDED | No external calls before state updates; Anchor CEI pattern; Solana single-TX atomicity |
| A2 | Flash Loan + Price Manipulation | ✅ DEFENDED | Per-TX 2%/1.5% caps, per-slot 6%/3% caps, TWAP 2.5% deviation cap, staleness + confidence guards |
| A3 | Oracle Manipulation | ✅ DEFENDED | Pyth V2 publish_time freshness (60s), per-path staleness (20/45/8/16 slots), feed ID binding, TWAP, progressive haircuts |
| A4 | Access Control | ✅ DEFENDED | 2-of-3 keeper quorum (`require_keeper_quorum`), TRUSTED_INITIALIZER hardcoded, Anchor constraints |
| A5 | Integer Overflow | ✅ DEFENDED | `mul_div_floor` with u128 intermediates throughout |
| A6 | Account Substitution | ✅ DEFENDED | `PYTH_FEED_ID_*` allowlist + `require_keys_eq!` + Anchor `Account<>` owner checks |
| A7 | Signature Replay | ✅ DEFENDED | Anchor discriminator + nonce accounts + state-machine gating |
| A8 | Front-running/Sandwich | ✅ DEFENDED | Commit-reveal for rebalances >4% (`COMMIT_REVEAL_DELAY_SLOTS=5`), batch window |
| A9 | Proxy Upgrade | ✅ DEFENDED | Deploy requires TRUSTED_INITIALIZER; BPF upgrade authority hardcoded |
| A10 | Logic Bug (Redeem) | ✅ DEFENDED | `assert_invariants()` post-op, TWAP + staleness + confidence on redeem path, per-TX/slot caps |
| A11 | Rent/Lamport Drain | ✅ DEFENDED | Anchor `close =` constraint, explicit lamport accounting |
| A12 | CPI Confusion | ✅ DEFENDED | `Program<'info, Token>` with program ID pinning; no user-supplied CPI target |
| A13 | PDA Seed Collision | ✅ DEFENDED | Anchor type discriminators + unique prefix seeds |
| A32 | Cross-Chain Bridge Forgery | ✅ N/A | No cross-chain bridge; pure Solana-native |
| A33 | Audit-Scope-Exclusion Exploitation | ✅ DEFENDED | Oracle uses Pyth (not spot AMM); no manipulable pool price oracle |
| A34 | Fragmented Security Stack | ⚠️ PARTIAL | No confirmed vuln-class propagation registry across all entry points; low immediate risk |
| A35 | AI-Assisted Oracle Regression | ✅ DEFENDED | All collaterals are USD-native stablecoins; no ratio-feed composition; feed IDs bound to constants |
| A36 | Thin-Liquidity Collateral Admission | ✅ DEFENDED | Static collateral set (USDC/USDT/DAI/USDS) hardcoded; no dynamic admission |
| A38 | ZK Verifier Key Misbinding | ✅ N/A | No ZK proofs in Microstable |

#### B. Off-chain / Keeper Vectors

| # | Vector | Verdict | Evidence |
|---|---|---|---|
| B14 | RPC Manipulation | ✅ DEFENDED | `RPC_ALLOWLIST` + secondary RPC cross-validation in rebalance cycle |
| B15 | Key Compromise | ⚠️ PARTIAL | 2-of-3 quorum limits blast radius; no HSM enforcement at runtime. DeFAI amplification note: AI orchestration layer (this session) + keeper key sharing context is a potential compound vector — mitigated by separate key management |
| B16 | Race Condition | ✅ DEFENDED | Keeper quorum + serial Solana TX processing + state-machine gating |
| B17 | Checkpoint Poisoning | ✅ DEFENDED | HMAC-authenticated state files (`MICROSTABLE_STATE_HMAC_KEY`), config `.sig` signature |
| B18 | Config Injection | ✅ DEFENDED | Config HMAC verification, RPC_ALLOWLIST, bounds validation on all config fields |
| B19 | Memory/Log Leak | ✅ DEFENDED (INFO) | Structured tracing logs; no keypair material logged in reviewed code paths |
| B20 | Denial of Service | ✅ DEFENDED | Circuit breakers, watchdog restart, `MAX_CONSECUTIVE_FAILED_CYCLES=5` |
| B29 | AI Prompt-Injection Confused-Deputy | ✅ N/A | Keeper is deterministic Rust binary, not LLM-based |
| B35 | Keeper Slippage Misconfiguration | ✅ DEFENDED | On-chain hard cap `MAX_REBALANCE_SLIPPAGE_BPS=1500`; config validation `<= 10000`; default `200 bps` |
| B36 | Social Engineering Stake Authority | ⚠️ PARTIAL | No validator stake accounts; but TRUSTED_INITIALIZER + 2 keeper keys are hot. 2-of-3 is the only protection against compromised operator device |
| B37 | AI Steganographic Evasion | ✅ N/A | Keeper is Rust binary |
| B38 | Multi-turn Tool-Return Boundary | ✅ N/A | Keeper is deterministic Rust binary |

#### C. Economic Vectors

| # | Vector | Verdict | Evidence |
|---|---|---|---|
| C21 | Bank Run / Depeg | ✅ DEFENDED | Per-slot/TX redeem caps (3%/1.5%), velocity surcharge fees, circuit breakers, 120% CR target |
| C22 | Collateral Manipulation | ✅ DEFENDED | USDC/USDT/DAI/USDS only; deep-liquidity stablecoins; TWAP smooths short-term depegs |
| C23 | Governance Attack | ✅ DEFENDED | No governance token; admin requires TRUSTED_INITIALIZER + 2-of-3 keeper quorum |
| C24 | Sybil | ✅ DEFENDED | `AGENT_MIN_STAKE_LAMPORTS = 1 SOL` |
| C25 | MEV Extraction | ✅ DEFENDED | Commit-reveal for large rebalances; fee floors on mint/redeem |
| C30 | Liquidity-Exhaustion Griefing | ✅ DEFENDED | Per-slot limits prevent grinding attacks |

#### D. Infrastructure Vectors

| # | Vector | Verdict | Evidence |
|---|---|---|---|
| D26 | Frontend XSS/Injection | ✅ DEFENDED | CSP: `script-src 'self'`; no external CDN scripts |
| D27 | RPC Endpoint Takeover | ✅ DEFENDED | `RPC_ALLOWLIST`, multi-RPC consensus |
| D28 | Supply Chain | ⚠️ PARTIAL | `vendor/solana-web3-1.95.3.iife.min.js` lacks SRI hash; Cargo.lock attestation not verified in visible CI config |
| D31 | Protocol-Metadata Confusion | ✅ DEFENDED | IDL advisory only; runtime enforces all invariants |
| D32 | AI Skill/Identity Poisoning | ✅ N/A | On-chain/keeper not affected; relevant to orchestration layer only |
| D33 | Transitive Typosquat | ⚠️ PARTIAL | Keeper uses Rust/Cargo; transitive dep integrity verification not confirmed from code review |
| D34 | WASI Hostcall Exhaustion | ✅ N/A | No untrusted Wasm embedding |

### Summary

| Severity | Count | Vectors |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM (PARTIAL) | 4 | B15, B36, D28, D33 |
| INFO/N/A | 34 | All others |

**Today's new vector relevance:** Holdstation DeFAI ($462K, B15 tentative) — Microstable does not use AI-integrated wallet architecture; 2-of-3 keeper quorum is the existing mitigation. B15 PARTIAL rating unchanged.

**Conclusion:** No CRITICAL or HIGH findings. No Discord alert triggered. 4 pre-existing MEDIUM items unchanged from prior cycles.

---

## 2026-03-22 Daily Black Team Check

**New vector added today:** D45 — Blockchain-as-C2 Channel via Malicious Developer Toolchain Extension (Bitdefender/Windsurf IDE, 2026-03-20)

### D45 Assessment vs. Microstable Keeper

| # | Vector | Verdict | Evidence |
|---|---|---|---|
| D45 | Blockchain-as-C2 via Malicious IDE Extension | ⚠️ PARTIAL | 2-of-3 keeper quorum limits blast radius from single machine compromise. However: default keeper keypair path is `~/.config/solana/devnet-keypair.json` (flat file on dev workstation). No documented hardware-wallet enforcement policy for keeper operators. No IDE extension allowlist policy. RPC API keys (Helius/QuickNode) stored in `config.toml` / `.env` extractable. |

**Attack path via D45 → Microstable:**
1. Operator installs typosquatted IDE extension on keeper dev machine
2. Extension exfiltrates 1 of 3 keeper keypairs + RPC API keys
3. Attacker can: (a) monitor all keeper oracle writes, (b) DoS oracle via rate-limit exhaustion of stolen API key, (c) attempt social-engineering for 2nd keypair (2-of-3 means 1 is insufficient for treasury drain)
4. Net: RPC monitoring + partial DoS possible immediately; treasury drain requires 2-of-3 compromise

**D45 → B36 escalation path:** If 2 of 3 keeper operators use IDEs on keeper machines, full keeper quorum compromisable without any on-chain exploit.

**Verdict: ⚠️ MEDIUM** — Quorum hardening prevents immediate treasury drain, but RPC credential exposure + multi-machine escalation risk warrants documented operator security policy.

**Recommendation:**
- Document explicit rule: keeper signing keys must use hardware wallet paths (`/dev/ledger`) in production config
- Add `keeper_keypairs` path validation to reject `~/.config/solana/` default in production mode (already has `validate_keypair_path_policy()` — extend to check for non-hardware paths)
- Issue IDE extension allowlist guidance to all keeper operators
- Rotate RPC API keys periodically; treat as secrets with same rigor as keypairs

### Full Vector Sweep (today's incremental)

All previously-checked vectors remain unchanged. No new CRITICAL/HIGH findings.

**New vector summary:** 1 added (D45 — MEDIUM, keeper operator workstation security)
**CRITICAL/HIGH count: 0**
**No Discord alert triggered (no CRITICAL/HIGH).**
