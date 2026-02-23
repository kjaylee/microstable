# Microstable Security Audit — Purple Team v17 (Final Verification of Blue v13)

- Date: 2026-02-23 (KST)
- Auditor: Purple Team v17
- Target patch: Blue v13 (`8bc3334`)
- Previous chain: PV14-001 → Blue v11, PV15-001 → Blue v12, PV16-001+002 → Blue v13
- Scope:
  - `solana/keeper/src/rebalance.rs`
  - `solana/programs/microstable/src/lib.rs`
  - Prior findings closure verification (PV14, PV15, PV16)

---

## Executive Result

Blue v13 verification result: **ZERO FINDINGS**.

- **PV16-001: CLOSED**
- **PV16-002: CLOSED**
- **PV14-001: CLOSED (still closed)**
- **PV15-001: CLOSED (still closed)**
- Commit-reveal path / slot sync / batch window sweep: **no new actionable issues found**.

**ZERO NEW FINDINGS: YES**

---

## Verification Details

## 1) PV16-001 — `batch_slot` frozen at commit time (no overwrite before reveal)

### Requirement
`batch_slot` used for `commit_hash` must remain identical until reveal preimage submission.

### Evidence
In `solana/keeper/src/rebalance.rs`:
- Commit hash constructed with `batch_slot`: lines **344-351**
- Deferred reveal stores the same `batch_slot` in memory immediately after commit: lines **385-394**
- Deferred reveal sends `local_pending.batch_slot` (same stored value): lines **201-211**
- No post-commit resync/rebind of `batch_slot` to `protocol.pending_rebalance_slot` remains in code path.

### Verdict
**PASS / CLOSED** — commit-time preimage slot is frozen and reused.

---

## 2) PV16-002 — `batch_window_has_room()` margin is `delay + 2`

### Requirement
Window-room guard must account for tolerated slot drift and avoid dead-zone commits.

### Evidence
In `solana/keeper/src/rebalance.rs`:
- `batch_window_has_room()` uses:
  - `remaining > reveal_delay_slots.saturating_add(2)` at lines **890-895**
- Helper comment explicitly ties this to tolerated ±2 landing drift.

### Verdict
**PASS / CLOSED** — guard now enforces `delay + 2` margin.

---

## 3) Sweep for new issues (commit-reveal path, slot sync, batch window)

### Checked
- Keeper deferred reveal gating:
  - `deferred_reveal_ready()` drift and same-window checks (lines **897-908**)
- On-chain commit/reveal verification:
  - `pending_rebalance_slot` set at commit time (`lib.rs` **1264-1266**)
  - reveal hash verification against committed hash (`lib.rs` **1333-1337**, hash fn **3017-3023**)
  - batch-window enforcement (`lib.rs` **3027-3032**)
- Keeper commit pre-check:
  - deferred commit only when `batch_window_has_room()` passes (lines **331-342**)

### Outcome
No new exploitable issue identified in the reviewed path.

---

## 4) Prior findings closure status (PV14, PV15, PV16)

- **PV14-001 (deferred path unreachable): CLOSED**
  - `select_batch_slot(current_slot)` now uses commit slot semantics (lines **883-885**)
  - deferred reveal uses readiness logic instead of impossible equality (lines **192-199**, **897-908**)

- **PV15-001 (slot-equality brittleness + boundary dead zone): CLOSED**
  - slot drift tolerance added (`slot_drift <= 2`, lines **903-906**)
  - boundary dead-zone prevented by `delay + 2` room gate (lines **890-895**)

- **PV16-001 (post-commit slot overwrite): CLOSED**
  - confirmed absent; `batch_slot` frozen in `PendingReveal` and reused.

- **PV16-002 (insufficient room margin): CLOSED**
  - margin fixed to `delay + 2`.

---

## 5) Test Execution

### Keeper tests
```bash
cd solana/keeper && cargo test --quiet
```
Result: **PASS**
- 132 + 8 + 8 + 28 + 25 + 27 + 3 tests passed
- 0 failed

### Program library tests
```bash
cd solana && cargo test -p microstable --lib --quiet
```
Result: **PASS**
- 29 tests passed
- 0 failed

---

## Final Assessment

- **ZERO NEW FINDINGS:** **YES**
- Blue v13 patch set for PV16 is verified, and prior PV14/PV15/PV16 findings remain closed under current code and tests.
