#!/usr/bin/env python3
"""Crimson Team advanced numeric edge-case attack scenarios."""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, asdict
from typing import Any, Dict, List

import microstable as ms
import open_agent_economy as oae


@dataclass
class Attempt:
    attempt_id: str
    name: str
    category: str
    success: bool
    severity: str
    evidence: Dict[str, Any]
    defense_or_failure_reason: str


def n01_nan_deposit_unbounded_withdraw() -> Attempt:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    stk.deposit("att", "Optimizer", float("nan"), 0)
    req_ok = stk.request_withdrawal("att", 1_000_000.0, 0)
    out = stk.withdraw("att", 5)

    success = req_ok and out >= 1_000_000.0
    return Attempt(
        attempt_id="CT-N01",
        name="NaN deposit enables unbacked large withdrawal",
        category="D",
        success=success,
        severity="CRITICAL" if success else "NONE",
        evidence={"request_ok": req_ok, "withdrawn": out, "final_balance": stk.balances.get("att")},
        defense_or_failure_reason="deposit/request_withdrawal lack finite-number validation.",
    )


def n02_nan_deposit_infinite_withdraw() -> Attempt:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    stk.deposit("att", "Optimizer", float("nan"), 0)
    req_ok = stk.request_withdrawal("att", float("inf"), 0)
    out = stk.withdraw("att", 5)

    success = req_ok and math.isinf(out)
    return Attempt(
        attempt_id="CT-N02",
        name="NaN deposit + inf withdrawal amount returns inf",
        category="D",
        success=success,
        severity="CRITICAL" if success else "NONE",
        evidence={"request_ok": req_ok, "withdrawn": out, "is_inf": math.isinf(out)},
        defense_or_failure_reason="No finite bound checks on withdrawal amount.",
    )


def n03_claim_reward_nan_epoch_crash() -> Attempt:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)

    crashed = False
    err = ""
    try:
        stk.claim_reward("a", 1.0, "cid", float("nan"), proof=None)
    except Exception as e:  # noqa: BLE001
        crashed = True
        err = f"{type(e).__name__}:{e}"

    return Attempt(
        attempt_id="CT-N03",
        name="NaN epoch input crashes claim_reward",
        category="D",
        success=crashed,
        severity="MEDIUM" if crashed else "NONE",
        evidence={"crashed": crashed, "error": err},
        defense_or_failure_reason="claim_reward unconditionally int() casts epoch for HMAC payload generation.",
    )


def n04_keeper_nan_fee_poison() -> Attempt:
    s = ms.ProtocolState()
    k = ms.Keeper()
    s.begin_tick()

    prop = {
        "weights": s.weights[:],
        "mint_fee": float("nan"),
        "proposal_epoch": s.market_epoch,
        "state_hash": s.market_state_hash,
        "expiry_epoch": s.market_epoch + 2,
    }
    res = k.submit_update_proposal(s, prop)

    success = res.get("status") == "APPLIED" and not math.isfinite(s.mint_fee)
    return Attempt(
        attempt_id="CT-N04",
        name="Keeper path accepts NaN mint_fee",
        category="D",
        success=success,
        severity="HIGH" if success else "NONE",
        evidence={"result": res, "mint_fee": s.mint_fee},
        defense_or_failure_reason="submit_update_proposal does not enforce finite mint_fee.",
    )


def n05_keeper_nan_weight_poison() -> Attempt:
    s = ms.ProtocolState()
    k = ms.Keeper()
    s.begin_tick()

    prop = {
        "weights": [float("nan"), 0.31, 0.19, 0.10],
        "mint_fee": 0.002,
        "proposal_epoch": s.market_epoch,
        "state_hash": s.market_state_hash,
        "expiry_epoch": s.market_epoch + 2,
    }
    res = k.submit_update_proposal(s, prop)

    success = res.get("status") == "APPLIED" and any(not math.isfinite(w) for w in s.weights)
    return Attempt(
        attempt_id="CT-N05",
        name="Keeper path accepts NaN in weights",
        category="D",
        success=success,
        severity="HIGH" if success else "NONE",
        evidence={"result": res, "weights": s.weights},
        defense_or_failure_reason="sum/limit checks do not reject NaN comparisons.",
    )


def n06_negative_price_residual_drain() -> Attempt:
    q = ms.RedemptionQueue(smoothing_window=8)
    q.enqueue("att", 333, 1_000_000)
    out = q.settle([1, 1_000_000], [1.0, -1.0], 1_000)

    drained = out.get("att", [0, 0])[1]
    success = drained == 1_000_000
    return Attempt(
        attempt_id="CT-N06",
        name="Negative oracle price drives residual into full-asset drain",
        category="C/D",
        success=success,
        severity="CRITICAL" if success else "NONE",
        evidence={"settle_output": out, "drained_asset_1": drained},
        defense_or_failure_reason="redeem_by_value lacks positive/finite oracle-price validation.",
    )


def n07_negative_staleness_accepted() -> Attempt:
    minted = 0
    err = ""
    try:
        minted = ms.secure_mint_amount(1_000, [1.0], -100, 1.0)
    except Exception as exc:  # noqa: BLE001
        err = f"{type(exc).__name__}:{exc}"
    success = minted > 0
    return Attempt(
        attempt_id="CT-N07",
        name="Negative stale_seconds accepted by secure_mint_amount",
        category="C/D/E",
        success=success,
        severity="MEDIUM" if success else "NONE",
        evidence={"minted": minted, "error": err},
        defense_or_failure_reason="validated_oracle_price checks only stale_seconds > max, not negative values.",
    )


def n08_stale_oracle_rejected_defense() -> Attempt:
    blocked = False
    err = ""
    try:
        ms.secure_mint_amount(1_000, [1.0], 500, 1.0)
    except Exception as e:  # noqa: BLE001
        blocked = True
        err = f"{type(e).__name__}:{e}"

    return Attempt(
        attempt_id="CT-N08",
        name="Defense check: stale oracle rejected",
        category="D",
        success=not blocked,
        severity="NONE" if blocked else "HIGH",
        evidence={"blocked": blocked, "error": err},
        defense_or_failure_reason="Defense held when success=False (oracle_stale raised).",
    )


def n09_toxic_collateral_rejected_defense() -> Attempt:
    minted = ms.secure_mint_amount(1_000_000, [1.0], 10, 1.0, risk_score=0.999)
    blocked = minted == 0
    return Attempt(
        attempt_id="CT-N09",
        name="Defense check: toxic collateral hard reject",
        category="D",
        success=not blocked,
        severity="NONE" if blocked else "HIGH",
        evidence={"minted": minted},
        defense_or_failure_reason="Defense held when success=False (risk_score threshold blocks mint).",
    )


def n10_reward_cap_epsilon_drift() -> Attempt:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("att", "Optimizer", 10.0, 0)

    ok_count = 0
    for i in range(20_000):
        ok = stk.claim_reward("att", 0.1, f"drift-{i}", 3, proof=None)
        if not ok:
            break
        ok_count += 1

    used = stk.claimed_by_epoch.get(3, 0.0)
    success = used > stk.reward_epoch_cap
    return Attempt(
        attempt_id="CT-N10",
        name="Floating-point epsilon drift exceeds nominal epoch cap",
        category="D",
        success=success,
        severity="LOW" if success else "NONE",
        evidence={"ok_count": ok_count, "used": used, "cap": stk.reward_epoch_cap},
        defense_or_failure_reason="Cap compare allows +EPS tolerance; repeated 0.1 additions accumulate slight overshoot.",
    )


def run_attempts() -> List[Dict[str, Any]]:
    attempts = [
        n01_nan_deposit_unbounded_withdraw(),
        n02_nan_deposit_infinite_withdraw(),
        n03_claim_reward_nan_epoch_crash(),
        n04_keeper_nan_fee_poison(),
        n05_keeper_nan_weight_poison(),
        n06_negative_price_residual_drain(),
        n07_negative_staleness_accepted(),
        n08_stale_oracle_rejected_defense(),
        n09_toxic_collateral_rejected_defense(),
        n10_reward_cap_epsilon_drift(),
    ]
    return [asdict(a) for a in attempts]


if __name__ == "__main__":
    print(json.dumps(run_attempts(), indent=2))
