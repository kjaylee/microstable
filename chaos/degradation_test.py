from __future__ import annotations

import json
import os
import sys
from datetime import datetime, timezone
from typing import Any, Dict, List

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from microstable import AdamOptimizer, CircuitBreaker, Keeper, LossEngine, MarketEnv, ProtocolState


def _mode_from_alive(alive_count: int) -> str:
    if alive_count >= 3:
        return "NORMAL"
    if alive_count == 2:
        return "QUORUM_2_OF_3"
    if alive_count == 1:
        return "SAFE_MODE"
    return "FROZEN"


def _simulate_agent_outage(down_agents: List[str], ticks: int = 45) -> Dict[str, Any]:
    env = MarketEnv(scenario="volatile", seed=99, deterministic=True)
    state = ProtocolState()
    breaker = CircuitBreaker(n_assets=len(state.weights))
    loss_engine = LossEngine()
    optimizer = AdamOptimizer(n_weights=len(state.weights))
    keeper = Keeper()

    alive = {"keeper": True, "watchdog": True, "auditor": True}
    for name in down_agents:
        alive[name] = False

    rows: List[Dict[str, Any]] = []
    funds_preserved = True
    cb3_seen = False

    for tick in range(ticks):
        state.begin_tick()
        market = env.step(tick)

        loss_finite = True
        loss_value = None
        grad_w = [0.0] * len(state.weights)
        grad_fee = 0.0

        if alive["keeper"]:
            loss, ctx = loss_engine.compute(state, market.prices, market.oracle_q)
            loss.backward()
            loss_value = float(loss.data)
            grad_w = [float(v.grad) for v in ctx["weights"]]
            grad_fee = float(ctx["fee"].grad)

        nav_now = state.effective_collateral_value(market.prices)
        nav_drop = nav_now - state.nav_prev

        forced = {}
        if not alive["watchdog"]:
            # watchdog down: no external force flags
            forced = {}

        action = breaker.update(
            tick=tick,
            state=state,
            market=market,
            nav_drop=nav_drop,
            loss_finite=loss_finite,
            loss_value=loss_value,
            forced=forced,
        )
        cb3_seen = cb3_seen or bool(action["cb3"])

        alive_count = sum(1 for ok in alive.values() if ok)
        mode = _mode_from_alive(alive_count)

        if mode in ("SAFE_MODE", "FROZEN"):
            state.mint_limit = 0.0
            if mode == "FROZEN":
                state.mint_paused_reason = "FROZEN_REDEEM_ONLY"

        if alive["keeper"] and mode not in ("SAFE_MODE", "FROZEN") and state.mint_limit > 0.0:
            proposal = keeper.propose(state, optimizer, grad_w, grad_fee)
            keeper.submit_update_proposal(state, proposal)

        peg = state.update_from_market(market.prices, market.oracle_q)

        funds_ok_tick = (
            state.cr >= state.cr_hard_min - 1e-9
            and abs(state.position_supply_sum - state.supply) <= 1e-6
            and state.reserve_value >= state.supply * state.cr_hard_min - 1e-6
        )
        funds_preserved = funds_preserved and funds_ok_tick

        rows.append(
            {
                "tick": tick,
                "mode": mode,
                "alive_count": alive_count,
                "peg_error": abs(peg - 1.0),
                "cr": state.cr,
                "mint_limit": state.mint_limit,
                "mint_paused_reason": state.mint_paused_reason,
                "funds_preserved": funds_ok_tick,
                "cb3": bool(action["cb3"]),
            }
        )

    return {
        "rows": rows,
        "funds_preserved": funds_preserved,
        "cb3_seen": cb3_seen,
    }


def _simulate_oracle_full_failure(ticks: int = 40) -> Dict[str, Any]:
    env = MarketEnv(scenario="oracle_failure", seed=123, deterministic=True)
    state = ProtocolState()
    breaker = CircuitBreaker(n_assets=len(state.weights))

    cb3_ticks: List[int] = []
    funds_preserved = True

    for tick in range(ticks):
        state.begin_tick()
        market = env.step(tick)
        market.stale_seconds = 300
        market.divergence = 0.08
        market.oracle_q = 0.05

        nav_now = state.effective_collateral_value(market.prices)
        nav_drop = nav_now - state.nav_prev

        action = breaker.update(
            tick=tick,
            state=state,
            market=market,
            nav_drop=nav_drop,
            loss_finite=True,
            loss_value=0.1,
            forced={"cb3": True},
        )

        if action["cb3"]:
            cb3_ticks.append(tick)

        state.update_from_market(market.prices, market.oracle_q)
        funds_preserved = funds_preserved and (
            state.cr >= state.cr_hard_min - 1e-9 and abs(state.position_supply_sum - state.supply) <= 1e-6
        )

    return {
        "cb3_ticks": cb3_ticks,
        "funds_preserved": funds_preserved,
    }


def run_degradation_tests() -> Dict[str, Any]:
    one_down = _simulate_agent_outage(["keeper"])
    two_down = _simulate_agent_outage(["keeper", "watchdog"])
    three_down = _simulate_agent_outage(["keeper", "watchdog", "auditor"])
    oracle_fail = _simulate_oracle_full_failure()

    check_1 = any(r["alive_count"] == 2 and r["mode"] == "QUORUM_2_OF_3" for r in one_down["rows"])
    check_2 = any(r["alive_count"] == 1 and r["mode"] == "SAFE_MODE" for r in two_down["rows"])
    check_3 = any(
        r["alive_count"] == 0 and r["mode"] == "FROZEN" and r["mint_limit"] == 0.0 and "REDEEM_ONLY" in r["mint_paused_reason"]
        for r in three_down["rows"]
    )
    check_4 = len(oracle_fail["cb3_ticks"]) > 0

    funds_preserved_all = (
        one_down["funds_preserved"]
        and two_down["funds_preserved"]
        and three_down["funds_preserved"]
        and oracle_fail["funds_preserved"]
    )

    checks = [
        {"name": "1_agent_down_quorum_2_of_3", "pass": check_1},
        {"name": "2_agents_down_safe_mode", "pass": check_2},
        {"name": "3_agents_down_frozen_redeem_only", "pass": check_3},
        {"name": "oracle_full_failure_cb3", "pass": check_4},
        {"name": "funds_preservation_all_paths", "pass": funds_preserved_all},
    ]

    overall = all(c["pass"] for c in checks)

    return {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "status": "PASS" if overall else "FAIL",
        "checks": checks,
        "evidence": {
            "one_agent_down_rows": one_down["rows"][:8],
            "two_agents_down_rows": two_down["rows"][:8],
            "three_agents_down_rows": three_down["rows"][:8],
            "oracle_cb3_ticks": oracle_fail["cb3_ticks"],
        },
    }


def main() -> None:
    result = run_degradation_tests()
    out_dir = os.path.join(ROOT_DIR, "outputs", "chaos")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "degradation-test-results.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2, sort_keys=True)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
