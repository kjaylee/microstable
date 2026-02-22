#!/usr/bin/env python3
"""Optimizer Keeper Agent interface for microstable protocol."""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from typing import Any, Dict

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from agents.security_controls import check_min_interval, load_state, save_state
from microstable import (
    AdamOptimizer,
    DELTA_FEE_MAX,
    DELTA_W_MAX,
    Keeper,
    LossEngine,
    MarketEnv,
    ProtocolState,
    ProtocolTxScheduler,
    distribute_fees,
)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="microstable keeper agent")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="simulate decision without applying")
    mode.add_argument("--execute", action="store_true", help="apply proposed parameter update")
    p.add_argument("--scenario", default="normal", help="market scenario for simulation input")
    p.add_argument("--seed", type=int, default=0, help="random seed")
    p.add_argument("--tick", type=int, default=0, help="market tick")
    p.add_argument(
        "--rebalance-threshold",
        type=float,
        default=0.001,
        help="minimum L1 weight delta to recommend rebalance",
    )
    p.add_argument(
        "--fee-threshold",
        type=float,
        default=0.00005,
        help="minimum fee delta to recommend update",
    )
    p.add_argument(
        "--min-rebalance-interval",
        type=int,
        default=5,
        help="minimum ticks between keeper rebalances",
    )
    return p


def run(args: argparse.Namespace) -> Dict[str, Any]:
    state = ProtocolState()
    env = MarketEnv(scenario=args.scenario, seed=args.seed, deterministic=True)
    market = env.step(args.tick)

    loss_engine = LossEngine()
    optimizer = AdamOptimizer(n_weights=len(state.weights))
    keeper = Keeper()

    loss, ctx = loss_engine.compute(state, market.prices, market.oracle_q)
    loss.backward()

    grad_w = [float(w.grad) for w in ctx["weights"]]
    grad_fee = float(ctx["fee"].grad)

    proposal = keeper.propose(state, optimizer, grad_w, grad_fee)
    proposed_weights = [float(w) for w in proposal["weights"]]
    proposed_fee = float(proposal["mint_fee"])

    turnover_l1 = float(sum(abs(a - b) for a, b in zip(proposed_weights, state.weights)))
    fee_delta = abs(proposed_fee - state.mint_fee)

    # // BLUE-TEAM: AGENT-RL-01 - max rebalance frequency + per-epoch magnitude guardrails.
    sec_state = load_state()
    keeper_state = sec_state.setdefault("keeper", {})
    min_interval = int(getattr(args, "min_rebalance_interval", 5))
    interval_ok, wait_ticks = check_min_interval(
        keeper_state.get("last_rebalance_tick"),
        int(args.tick),
        min_interval,
    )

    param_delta_ok = (
        all(abs(proposed_weights[i] - state.weights[i]) <= DELTA_W_MAX + 1e-12 for i in range(len(state.weights)))
        and abs(proposed_fee - state.mint_fee) <= DELTA_FEE_MAX + 1e-12
    )

    should_submit = (
        (turnover_l1 >= args.rebalance_threshold or fee_delta >= args.fee_threshold)
        and interval_ok
        and param_delta_ok
    )

    execution_result: Dict[str, Any]
    if args.execute and should_submit:
        execution_result = keeper.submit_update_proposal(state, proposal)
        if execution_result.get("status") == "APPLIED":
            keeper_state["last_rebalance_tick"] = int(args.tick)
            save_state(sec_state)
    else:
        reason = "below_threshold"
        if not interval_ok:
            reason = f"rate_limited_wait_{wait_ticks}_ticks"
        elif not param_delta_ok:
            reason = "param_delta_guardrail"
        execution_result = {
            "status": "DRY_RUN" if args.dry_run else "SKIPPED",
            "reason": reason,
        }

    fee_split = distribute_fees(1.0)
    qos = ProtocolTxScheduler()

    return {
        "agent": "keeper",
        "mode": "execute" if args.execute else "dry-run",
        "revenue_share": fee_split["keeper"],
        "state_snapshot": {
            "weights": state.prev_weights,
            "mint_fee": state.mint_fee,
            "cr": state.cr,
            "cr_target": state.cr_target,
        },
        "market_snapshot": {
            "tick": args.tick,
            "prices": market.prices,
            "oracle_q": market.oracle_q,
        },
        "optimization": {
            "loss": float(loss.data),
            "grad_weights": grad_w,
            "grad_fee": grad_fee,
            "proposed_weights": proposed_weights,
            "proposed_fee": proposed_fee,
            "turnover_l1": turnover_l1,
            "fee_delta": fee_delta,
            "should_submit_rebalance": should_submit,
        },
        "rate_limits": {
            "interval_ok": interval_ok,
            "wait_ticks": wait_ticks,
            "param_delta_ok": param_delta_ok,
            "min_rebalance_interval": min_interval,
        },
        "qos": {
            "priority_fee_microlamports": qos.priority_fee_microlamports,
            "reserved_tx_slots": qos.reserved_tx_slots,
            "reserved_compute_units": qos.reserved_compute_units,
        },
        "decision": execution_result,
        "valid": math.isfinite(loss.data) and all(math.isfinite(g) for g in grad_w + [grad_fee]),
    }


def main() -> None:
    args = build_parser().parse_args()
    result = run(args)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
