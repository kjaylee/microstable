#!/usr/bin/env python3
"""Watchdog Agent interface for microstable protocol."""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from typing import Any, Dict, List

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from microstable import LossEngine, MarketEnv, ProtocolState, Watchdog, distribute_fees


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="microstable watchdog agent")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="simulate alerts only")
    mode.add_argument("--execute", action="store_true", help="emit activation recommendation")
    p.add_argument("--scenario", default="normal", help="market scenario for simulation input")
    p.add_argument("--seed", type=int, default=0, help="random seed")
    p.add_argument("--tick", type=int, default=0, help="market tick")
    p.add_argument(
        "--prev-loss",
        type=float,
        default=None,
        help="previous loss value used for CB-4 divergence detection",
    )
    return p


def run(args: argparse.Namespace) -> Dict[str, Any]:
    state = ProtocolState()
    env = MarketEnv(scenario=args.scenario, seed=args.seed)
    market = env.step(args.tick)

    watchdog = Watchdog()
    cb_flags = watchdog.detect(market)

    loss_engine = LossEngine()
    loss, _ = loss_engine.compute(state, market.prices, market.oracle_q)
    loss_value = float(loss.data)
    loss_finite = math.isfinite(loss_value)

    cb4 = not loss_finite
    if args.prev_loss is not None and args.prev_loss > 0 and loss_finite:
        cb4 = cb4 or (loss_value > args.prev_loss * 20.0 and (loss_value - args.prev_loss) > 5.0)

    cb_status = {
        "cb1": bool(cb_flags.get("cb1", False)),
        "cb2": bool(cb_flags.get("cb2", False)),
        "cb3": bool(cb_flags.get("cb3", False)),
        "cb4": bool(cb4),
        "cb1_collateral_index": cb_flags.get("cb1_idx"),
    }

    active: List[int] = [i for i in range(1, 5) if cb_status[f"cb{i}"]]
    recommendation = "ACTIVATE_CIRCUIT_BREAKER" if active else "NO_ACTION"

    fee_split = distribute_fees(1.0)

    return {
        "agent": "watchdog",
        "mode": "execute" if args.execute else "dry-run",
        "revenue_share": fee_split["watchdog"],
        "market_snapshot": {
            "tick": args.tick,
            "prices": market.prices,
            "oracle_q": market.oracle_q,
            "stale_seconds": market.stale_seconds,
            "divergence": market.divergence,
        },
        "oracle_health": {
            "healthy": market.stale_seconds <= 120 and market.divergence <= 0.02,
            "stale_seconds": market.stale_seconds,
            "divergence": market.divergence,
        },
        "cb_detection": cb_status,
        "decision": {
            "recommended_action": recommendation,
            "target_cb_ids": active,
            "status": "ALERT" if active else "STABLE",
            "dispatch": bool(args.execute and active),
        },
        "diagnostics": {
            "loss": loss_value,
            "loss_finite": loss_finite,
            "prev_loss": args.prev_loss,
        },
    }


def main() -> None:
    args = build_parser().parse_args()
    result = run(args)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
