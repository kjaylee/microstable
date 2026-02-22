#!/usr/bin/env python3
"""Auditor Agent interface for microstable protocol."""

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

from microstable import Auditor, DELTA_FEE_MAX, DELTA_W_MAX, ProtocolState, distribute_fees


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="microstable auditor agent")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="simulate audit without signing")
    mode.add_argument("--execute", action="store_true", help="finalize audit report")
    p.add_argument(
        "--proposal-json",
        type=str,
        default="",
        help="optional keeper proposal JSON: {'weights':[...],'mint_fee':...}",
    )
    return p


def validate_keeper_proposal(state: ProtocolState, proposal: Dict[str, Any]) -> Dict[str, Any]:
    violations: List[str] = []
    weights = proposal.get("weights")
    mint_fee = proposal.get("mint_fee")

    if not isinstance(weights, list) or len(weights) != len(state.weights):
        violations.append("PROPOSAL_WEIGHTS_INVALID")
        weights = state.weights

    weight_vals = [float(w) for w in weights]
    if any(not math.isfinite(w) for w in weight_vals):
        violations.append("PROPOSAL_WEIGHTS_NON_FINITE")

    if abs(sum(weight_vals) - 1.0) > 1e-6:
        violations.append("PROPOSAL_WEIGHT_SUM")

    for i, (cur, new, cap) in enumerate(zip(state.weights, weight_vals, state.w_caps)):
        if new < -1e-12 or new > cap + 1e-12:
            violations.append(f"PROPOSAL_CAP_{i}")
        if abs(new - cur) > DELTA_W_MAX + 1e-9:
            violations.append(f"PROPOSAL_DELTA_W_{i}")

    if mint_fee is None:
        fee_val = state.mint_fee
    else:
        fee_val = float(mint_fee)
        if not math.isfinite(fee_val):
            violations.append("PROPOSAL_FEE_NON_FINITE")
        if abs(fee_val - state.mint_fee) > DELTA_FEE_MAX + 1e-12:
            violations.append("PROPOSAL_DELTA_FEE")

    return {
        "ok": len(violations) == 0,
        "violations": violations,
        "proposal": {
            "weights": weight_vals,
            "mint_fee": fee_val,
        },
    }


def run(args: argparse.Namespace) -> Dict[str, Any]:
    state = ProtocolState()
    auditor = Auditor()

    invariants = auditor.verify_invariants(state)
    finite_values = [state.cr, state.cr_target, state.mint_fee] + state.weights
    no_nan = all(math.isfinite(v) for v in finite_values)

    spec_checks = {
        "weight_sum_is_one": abs(sum(state.weights) - 1.0) <= 1e-6,
        "cr_above_target": state.cr > state.cr_target,
        "no_nan": no_nan,
    }

    proposal_report: Dict[str, Any] = {"ok": True, "violations": [], "proposal": None}
    if args.proposal_json:
        parsed = json.loads(args.proposal_json)
        if not isinstance(parsed, dict):
            raise ValueError("proposal-json must decode to an object")
        proposal_report = validate_keeper_proposal(state, parsed)

    overall_ok = invariants["ok"] and all(spec_checks.values()) and proposal_report["ok"]
    fee_split = distribute_fees(1.0)

    return {
        "agent": "auditor",
        "mode": "execute" if args.execute else "dry-run",
        "revenue_share": fee_split["auditor"],
        "invariants": invariants,
        "spec_checks": spec_checks,
        "keeper_proposal_validation": proposal_report,
        "decision": {
            "audit_passed": overall_ok,
            "report_status": "FINAL" if args.execute else "PREVIEW",
            "action": "APPROVE" if overall_ok else "REJECT",
        },
    }


def main() -> None:
    args = build_parser().parse_args()
    result = run(args)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
