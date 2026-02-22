# Microstable Red Team v4 Report (Post Blue Team v3)

## Scope
- Baseline commit: `dd20a9a` (Blue Team v3)
- Repo: `/Users/kjaylee/.openclaw/workspace/microstable/`
- Campaign harness: `tests/red-team-v4/exploit_campaign.py`

## Executive Summary
- Total exploit attempts: **24**
- Successful exploits: **13**
- Blocked by defenses: **11**
- Success rate: **54.17%**

Blue v3 closed many previously known holes, but this campaign found multiple **new, practical bypasses** in Python-layer economics/auth logic and one major on-chain logic contradiction (commit/reveal threshold unreachable).

## Highest-Risk New Findings

1. **A10 (CRITICAL) — NaN balance withdrawal forgery**
   - `StakingEconomics.deposit()` accepts NaN, `request_withdrawal()` and `withdraw()` then allow unbacked withdrawals.
   - PoC evidence: `withdrawn=1000000.0` from NaN-poisoned balance.

2. **A18 (CRITICAL) — Registry lifecycle race enables identity takeover**
   - `deregister()` / `finalize_deregistration()` lack caller auth; attacker can recycle victim ID and rebind ACP key.
   - PoC evidence: `deregister=True, finalize=True, re_register=True, forged_verify=True`.

3. **A06 (HIGH) — Solana large-rebalance commit/reveal unreachable**
   - `WEIGHT_STEP_LIMIT=20_000` implies max turnover `40_000`, but commit threshold is `50_000`.
   - Result: commit/reveal lane is effectively dead code.

4. **A12 (HIGH) — Unsig claim fragmentation drains full epoch reward cap**
   - `legacy_unsigned_claim_limit=1.0` can be split into 1000 unsigned claims.
   - PoC evidence: `approved=1000, epoch_used=1000.0`.

5. **A16 (HIGH) — Anti-sybil fingerprint evasion**
   - Dampening uses 2-decimal rounded fingerprint buckets; sybils spread across many near-collusive buckets.
   - PoC evidence: `sybil_total=14.7619, unique_buckets=20`.

6. **A22 (HIGH) — Insurance invalid-claim auto-refill trigger**
   - Auto-refill executes before claim validation; invalid claims can still inflate treasury if below trigger.
   - PoC evidence: `before=240000.0 ... after=440000.0`.

7. **A05 (HIGH) — Huge finite oracle values over-mint in Python path**
   - NaN/inf checks exist, but no upper bound for finite oracle samples.
   - PoC evidence: `mint_normal=831666`, `mint_huge=831666666666666666`.

## Defense Coverage vs Requested Targets

1. **Pyth staleness 60s**: boundary tested.
   - A01 success (exact 60 accepted), A02 blocked (>60 rejected).
2. **Feed ID validation**: tested.
   - A03 blocked (feed_id binding active), A04 blocked (feed account allowlist active).
3. **NaN/inf + numeric edge**: tested.
   - A09/A10/A11/A14/A05 succeeded; A13 blocked.
4. **Anti-sybil dampening**: tested.
   - A15 blocked for exact clone clusters; A16 bypassed with bucket spray.
5. **Rebalance commit secrets**: tested.
   - A07 blocked (cross-epoch replay), A08 boundary bypass, A06 structural unreachable commit gate.
6. **Public key binding / race**: tested.
   - A17 blocked direct overwrite; A18 succeeded via lifecycle race + ID takeover.
7. **`allow_legacy=False` paths**: tested.
   - A19 blocked legacy default replay, A20 succeeded when caller omits `now_epoch` (expiry check skipped).
8. **Insurance cooldown boundary**: tested.
   - A21/A23 blocked, A22 succeeded via pre-validation refill ordering.
9. **Migration one-shot / CPI path**: tested.
   - A24 blocked (trusted initializer + signer + one-shot discriminator guard present).

## Attempt Matrix (All 24)

| ID | Outcome | Severity | Focus | Evidence |
|---|---|---|---|---|
| A01 | SUCCESS | MEDIUM | Pyth staleness boundary | `age==60 accepted` |
| A02 | BLOCKED | MEDIUM | Pyth staleness boundary | `age_guard_present=True` |
| A03 | BLOCKED | HIGH | Feed ID validation | `feed_id_binding_present=True` |
| A04 | BLOCKED | HIGH | Feed account validation | `account_binding=True, allowlist=True` |
| A05 | SUCCESS | HIGH | Huge finite numeric | `mint_huge=831666666666666666` |
| A06 | SUCCESS | HIGH | Solana commit/reveal | `turnover_max=40000 < threshold=50000` |
| A07 | BLOCKED | MEDIUM | Commit replay cross-epoch | `r2=REJECTED missing_commit_reveal_proof` |
| A08 | SUCCESS | MEDIUM | Commit threshold boundary | `cumulative_turnover=0.04999999999999993` |
| A09 | SUCCESS | CRITICAL | NaN deposit | `balance=nan` |
| A10 | SUCCESS | CRITICAL | NaN withdraw forge | `withdrawn=1000000.0` |
| A11 | SUCCESS | HIGH | Inf deposit | `balance=inf` |
| A12 | SUCCESS | HIGH | Unsig claim split | `approved=1000` |
| A13 | BLOCKED | LOW | -0.0 slash edge | `after=100.0` |
| A14 | SUCCESS | MEDIUM | Redemption queue DoS | `victim delayed to second batch` |
| A15 | BLOCKED | MEDIUM | Anti-sybil baseline | `sybil_total=2.5` |
| A16 | SUCCESS | HIGH | Anti-sybil evasion | `sybil_total=14.7619` |
| A17 | BLOCKED | HIGH | Direct key hijack | `set_public_key_ok=False` |
| A18 | SUCCESS | CRITICAL | Registry race takeover | `forged_verify=True` |
| A19 | BLOCKED | MEDIUM | Legacy default | `legacy_verify_default=False` |
| A20 | SUCCESS | MEDIUM | Expiry bypass path | `late_no_now=True` |
| A21 | BLOCKED | MEDIUM | Insurance sybil rotation | `global_cooldown` |
| A22 | SUCCESS | HIGH | Insurance refill ordering | `treasury +200k on invalid claim` |
| A23 | BLOCKED | MEDIUM | Insurance epoch boundary | `global_cooldown` |
| A24 | BLOCKED | HIGH | Migration one-shot/CPI | `trusted_initializer=True, signer=True, one_shot=True` |

## Reproduction
```bash
cd /Users/kjaylee/.openclaw/workspace/microstable
PYTHONPATH=. python3 tests/red-team-v4/exploit_campaign.py
cat tests/red-team-v4/results.json
cat tests/red-team-v4/results.md
```

## Artifacts
- PoC harness: `tests/red-team-v4/exploit_campaign.py`
- Raw JSON results: `tests/red-team-v4/results.json`
- Markdown results table: `tests/red-team-v4/results.md`

## Notes
- This report intentionally documents both **successful and failed** exploit attempts.
- Solana findings in this v4 pass are static-analysis PoCs (consistent with prior red-team static checks in v3 for on-chain paths).
