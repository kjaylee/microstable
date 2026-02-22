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

from microstable import AdamOptimizer, Keeper, LossEngine, MarketEnv, ProtocolState, distribute_fees


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
    return p


def run(args: argparse.Namespace) -> Dict[str, Any]:
    state = ProtocolState()
    env = MarketEnv(scenario=args.scenario, seed=args.seed)
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
    should_submit = turnover_l1 >= args.rebalance_threshold or fee_delta >= args.fee_threshold

    execution_result: Dict[str, Any]
    if args.execute and should_submit:
        execution_result = keeper.submit_update_proposal(state, proposal)
    else:
        execution_result = {
            "status": "DRY_RUN" if args.dry_run else "SKIPPED",
            "reason": "below_threshold" if not should_submit else "execution_disabled",
        }

    fee_split = distribute_fees(1.0)

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
        "decision": execution_result,
        "valid": math.isfinite(loss.data) and all(math.isfinite(g) for g in grad_w + [grad_fee]),
    }


def main() -> None:
    args = build_parser().parse_args()
    result = run(args)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
