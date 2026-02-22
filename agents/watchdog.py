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

from agents.security_controls import check_min_interval, load_state, save_state
from microstable import LossEngine, MarketEnv, ProtocolState, ProtocolTxScheduler, Watchdog, distribute_fees


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
    p.add_argument(
        "--cb-cooldown-ticks",
        type=int,
        default=10,
        help="minimum ticks between repeated CB activation dispatches",
    )
    return p


def run(args: argparse.Namespace) -> Dict[str, Any]:
    state = ProtocolState()
    env = MarketEnv(scenario=args.scenario, seed=args.seed, deterministic=True)
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

    # // BLUE-TEAM: AGENT-RL-03 - cooldown between repeated CB activation dispatches.
    sec_state = load_state()
    wd_state = sec_state.setdefault("watchdog", {})
    last_ticks = wd_state.setdefault("cb_last_activation_tick", {})

    cb_cooldown_ticks = int(getattr(args, "cb_cooldown_ticks", 10))
    cooldown_ok = True
    blocked_cb_ids: List[int] = []
    for cb_id in active:
        ok, _ = check_min_interval(last_ticks.get(str(cb_id)), int(args.tick), cb_cooldown_ticks)
        if not ok:
            cooldown_ok = False
            blocked_cb_ids.append(cb_id)

    recommendation = "ACTIVATE_CIRCUIT_BREAKER" if active and cooldown_ok else "NO_ACTION"
    dispatch = bool(args.execute and active and cooldown_ok)
    if dispatch:
        for cb_id in active:
            last_ticks[str(cb_id)] = int(args.tick)
        save_state(sec_state)

    fee_split = distribute_fees(1.0)
    qos = ProtocolTxScheduler()

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
        "rate_limits": {
            "cooldown_ok": cooldown_ok,
            "blocked_cb_ids": blocked_cb_ids,
            "cb_cooldown_ticks": cb_cooldown_ticks,
        },
        "qos": {
            "priority_fee_microlamports": qos.priority_fee_microlamports,
            "reserved_tx_slots": qos.reserved_tx_slots,
            "reserved_compute_units": qos.reserved_compute_units,
        },
        "decision": {
            "recommended_action": recommendation,
            "target_cb_ids": active,
            "status": "ALERT" if active else "STABLE",
            "dispatch": dispatch,
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
