# Microstable Black Team R5 Report (Blue v17, commit afcc248)

**Scope reviewed**
- On-chain program: `solana/programs/microstable/src/lib.rs`
- Keeper daemon: `solana/keeper/src/*.rs`

## Finding Summary
| Severity | Count |
|---|---:|
| Critical | 0 |
| High | 0 |
| Medium | 0 |
| Low | 0 |
| Informational | 0 |

## Convergence Tracking (R1 → R5)
- **R1:** C3 / H12
- **R2:** C0 / H11
- **R3:** C0 / H8
- **R4:** C0 / H2
- **R5:** **C0 / H0** (no new Highs found in v17)

## Fix Verification (R4 items)
### R4-01 — TWAP monotonic slots + slot-delta alpha (verified)
- Manual oracle path and Pyth path now require **strictly increasing observed slots** and enforce per-vault slot gating via `last_twap_update_slots`.
- TWAP alpha is derived from **slot delta** to prevent same-slot spam from dominating the EWMA.
- **References:**
  - Per-vault TWAP slot storage: `ProtocolState.last_twap_update_slots` (lib.rs **L2624–L2626**)
  - Strict slot monotonicity + slot-delta alpha: `update_vault_oracle` (lib.rs **L3086–L3116**)
  - Manual oracle update writes per-vault last slot (lib.rs **L682–L722**)
  - Pyth oracle update writes per-vault last slot (lib.rs **L818–L864**)

**Result:** No viable bypass found for same-slot TWAP manipulation or slot regression.

### R4-02 — Key usage guard raised to 50k + epoch reset + graceful degradation (verified)
- Default per-epoch signature budget now **50,000** with upper bounds enforced by config validation.
- Guard **resets on epoch boundary**, and if RPC epoch info fails it **degrades to observe-only** to avoid operational DoS.
- **References:**
  - Default key budget = 50k (config.rs **L28–L31**)
  - Policy initialization (main.rs **L176–L183**)
  - Epoch-based reset + observe-only mode (utils.rs **L708–L779**)

**Result:** Key-usage guard no longer deadlocks on exhaustion; no exploitable path found.

### Medium Fixes (verified)
- **Velocity fee pre‑tx snapshot:** redeem fee now computed from **pre‑tx** flow counters.
  - Reference: `redeemed_in_flow_slot_before_tx` in redeem flow (lib.rs **L1233–L1238**)
- **Passphrase file permissions:** passphrase files must be **mode 600** and owned by effective UID (Unix).
  - Reference: `validate_passphrase_file_security` (utils.rs **L564–L596**)
- **TWAP freeze scoping:** per-vault `last_twap_update_slots` prevents one vault from freezing others.
  - Reference: ProtocolState field + per-vault updates (lib.rs **L2624–L2626**, **L682–L722**)

## New Attack Surface Review (v17 additions)
- **Pending reveal checkpoint** now HMAC‑protected and written with restrictive perms.
  - Reference: rebalance checkpoint save + verify (rebalance.rs **L1159–L1210**)
- **Key rotation grace logic** enforced at startup to prevent stale keys post‑cutover.
  - Reference: main startup checks (main.rs **L185–L208**)

**Result:** No new High/Medium vulnerabilities discovered in newly introduced code paths.

## Findings
**None.** No new exploitable issues identified in the reviewed v17 changes.

## Verdict
**ZERO HIGH achieved.**
