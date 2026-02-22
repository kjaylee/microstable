#!/usr/bin/env python3
"""Multi-agent governance consensus interface for microstable protocol."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from typing import Any, Dict

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from microstable import ProtocolState

TIMELOCK_SECONDS = 48 * 60 * 60
REQUIRED_YES = 3
AGENTS = ("keeper", "watchdog", "auditor")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="microstable governance consensus")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="preview governance decision")
    mode.add_argument("--execute", action="store_true", help="queue governance action")

    p.add_argument("--proposal-id", default="proposal-001", help="proposal identifier")
    p.add_argument(
        "--proposal-type",
        choices=["asset_listing", "parameter_change"],
        default="asset_listing",
        help="governance proposal type",
    )
    p.add_argument("--asset", default="USDX", help="asset symbol for listing proposals")
    p.add_argument("--param", default="cr_target", help="parameter name for parameter change proposals")
    p.add_argument("--value", default="1.25", help="parameter value for parameter change proposals")

    p.add_argument("--keeper-vote", choices=["yes", "no"], default="yes")
    p.add_argument("--watchdog-vote", choices=["yes", "no"], default="yes")
    p.add_argument("--auditor-vote", choices=["yes", "no"], default="yes")
    return p


def validate_parameter_change(param: str, value: str) -> Dict[str, Any]:
    state = ProtocolState()
    ok = True
    reason = "ok"

    if param == "cr_target":
        v = float(value)
        ok = 1.0 <= v <= 2.5
        reason = "ok" if ok else "cr_target_out_of_range"
    elif param == "mint_fee":
        v = float(value)
        ok = 0.0 <= v <= 0.05
        reason = "ok" if ok else "mint_fee_out_of_range"
    elif param.startswith("weight_"):
        idx = int(param.split("_", 1)[1])
        if idx < 0 or idx >= len(state.weights):
            return {"ok": False, "reason": "weight_index_invalid"}
        v = float(value)
        ok = 0.0 <= v <= state.w_caps[idx]
        reason = "ok" if ok else "weight_out_of_cap"
    else:
        ok = False
        reason = "unsupported_param"

    return {"ok": ok, "reason": reason}


def run(args: argparse.Namespace) -> Dict[str, Any]:
    votes = {
        "keeper": args.keeper_vote == "yes",
        "watchdog": args.watchdog_vote == "yes",
        "auditor": args.auditor_vote == "yes",
    }

    yes_votes = sum(1 for a in AGENTS if votes[a])
    consensus_reached = yes_votes == REQUIRED_YES

    validation = {"ok": True, "reason": "ok"}
    if args.proposal_type == "parameter_change":
        validation = validate_parameter_change(args.param, args.value)

    now = int(time.time())
    eta = now + TIMELOCK_SECONDS if consensus_reached and validation["ok"] else None

    can_queue = consensus_reached and validation["ok"]
    queued = bool(args.execute and can_queue)

    return {
        "agent": "consensus",
        "mode": "execute" if args.execute else "dry-run",
        "proposal": {
            "proposal_id": args.proposal_id,
            "type": args.proposal_type,
            "asset": args.asset if args.proposal_type == "asset_listing" else None,
            "param": args.param if args.proposal_type == "parameter_change" else None,
            "value": args.value if args.proposal_type == "parameter_change" else None,
        },
        "voting": {
            "required_yes": REQUIRED_YES,
            "votes": votes,
            "yes_votes": yes_votes,
            "consensus_reached_3_of_3": consensus_reached,
        },
        "validation": validation,
        "timelock": {
            "seconds": TIMELOCK_SECONDS,
            "eta_unix": eta,
            "eta_iso": datetime.fromtimestamp(eta, tz=timezone.utc).isoformat() if eta else None,
        },
        "decision": {
            "action": "QUEUE_GOVERNANCE_ACTION" if can_queue else "REJECT_OR_WAIT",
            "queued": queued,
        },
    }


def main() -> None:
    args = build_parser().parse_args()
    result = run(args)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
