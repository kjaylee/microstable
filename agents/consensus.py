#!/usr/bin/env python3
"""Multi-agent governance consensus interface for microstable protocol."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
from datetime import datetime, timezone
from typing import Any, Dict

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from agents.security_controls import load_state, save_state
from microstable import DELTA_FEE_MAX, DELTA_W_MAX, ProtocolState

TIMELOCK_SECONDS = 48 * 60 * 60
REQUIRED_YES = 2
AGENTS = ("keeper", "watchdog", "auditor")
ALLOWED_ASSET_PREFIXES = ("USD", "US", "DAI")
DENYLISTED_ASSETS = {"SANCTIONED_USD_PROXY", "OFAC_BLOCKED_USD", "MIXER_USD"}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="microstable governance consensus")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="preview governance decision")
    mode.add_argument("--execute", action="store_true", help="execute queued governance action")
    mode.add_argument("--queue", action="store_true", help="queue governance action (subject to timelock)")

    p.add_argument("--proposal-id", default="proposal-001", help="proposal identifier")
    p.add_argument(
        "--proposal-type",
        choices=["asset_listing", "parameter_change", "emergency_shutdown"],
        default="asset_listing",
        help="governance proposal type",
    )
    p.add_argument("--asset", default="USDX", help="asset symbol for listing proposals")
    p.add_argument("--param", default="cr_target", help="parameter name for parameter change proposals")
    p.add_argument("--value", default="1.25", help="parameter value for parameter change proposals")

    p.add_argument("--keeper-vote", choices=["yes", "no"], default="yes")
    p.add_argument("--watchdog-vote", choices=["yes", "no"], default="yes")
    p.add_argument("--auditor-vote", choices=["yes", "no"], default="yes")

    p.add_argument("--keeper-sig", default="", help="keeper signature token")
    p.add_argument("--watchdog-sig", default="", help="watchdog signature token")
    p.add_argument("--auditor-sig", default="", help="auditor signature token")
    p.add_argument("--nonce", type=int, default=0, help="proposal nonce for replay protection")
    return p


def validate_parameter_change(param: str, value: str) -> Dict[str, Any]:
    state = ProtocolState()

    if param == "cr_target":
        v = float(value)
        # // BLUE-TEAM: E6 - bounded per-epoch governance change + safe floor.
        if not (1.15 <= v <= 2.5):
            return {"ok": False, "reason": "cr_target_out_of_range"}
        if abs(v - state.cr_target) > 0.02 + 1e-12:
            return {"ok": False, "reason": "cr_target_step_too_large"}
        return {"ok": True, "reason": "ok"}

    if param == "mint_fee":
        v = float(value)
        if not (0.001 <= v <= 0.05):
            return {"ok": False, "reason": "mint_fee_out_of_range"}
        if abs(v - state.mint_fee) > DELTA_FEE_MAX + 1e-12:
            return {"ok": False, "reason": "mint_fee_step_too_large"}
        return {"ok": True, "reason": "ok"}

    if param.startswith("weight_"):
        idx = int(param.split("_", 1)[1])
        if idx < 0 or idx >= len(state.weights):
            return {"ok": False, "reason": "weight_index_invalid"}
        v = float(value)
        if not (0.0 <= v <= state.w_caps[idx]):
            return {"ok": False, "reason": "weight_out_of_cap"}
        if abs(v - state.weights[idx]) > DELTA_W_MAX + 1e-12:
            return {"ok": False, "reason": "weight_step_too_large"}
        return {"ok": True, "reason": "ok"}

    return {"ok": False, "reason": "unsupported_param"}


def validate_asset_listing(asset: str) -> Dict[str, Any]:
    symbol = str(asset or "").upper().strip()
    # // BLUE-TEAM: I23 - compliance/jurisdiction gate before queueing listings.
    if symbol in DENYLISTED_ASSETS:
        return {"ok": False, "reason": "asset_denylisted"}
    if len(symbol) < 3 or len(symbol) > 24:
        return {"ok": False, "reason": "asset_symbol_length"}
    if not symbol.replace("_", "").isalnum():
        return {"ok": False, "reason": "asset_symbol_invalid_chars"}
    if not symbol.startswith(ALLOWED_ASSET_PREFIXES):
        return {"ok": False, "reason": "asset_prefix_not_allowed"}
    return {"ok": True, "reason": "ok"}


def _proposal_hash(args: argparse.Namespace) -> str:
    payload = {
        "proposal_id": args.proposal_id,
        "proposal_type": args.proposal_type,
        "asset": args.asset,
        "param": args.param,
        "value": args.value,
    }
    raw = json.dumps(payload, sort_keys=True).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def _expected_sig(agent: str, proposal_hash: str, nonce: int) -> str:
    secret = os.getenv(f"MICROSTABLE_{agent.upper()}_SECRET", "")
    if not secret:
        return ""
    raw = f"{agent}|{proposal_hash}|{nonce}|{secret}".encode("utf-8")
    return hashlib.sha256(raw).hexdigest()[:24]


def _auth_ok(votes: Dict[str, bool], proposal_hash: str, nonce: int, args: argparse.Namespace) -> bool:
    sigs = {
        "keeper": getattr(args, "keeper_sig", ""),
        "watchdog": getattr(args, "watchdog_sig", ""),
        "auditor": getattr(args, "auditor_sig", ""),
    }
    for agent, voted_yes in votes.items():
        if not voted_yes:
            continue
        expected = _expected_sig(agent, proposal_hash, nonce)
        if not expected:
            return False
        if sigs.get(agent, "") != expected:
            return False
    return True


def run(args: argparse.Namespace) -> Dict[str, Any]:
    votes = {
        "keeper": getattr(args, "keeper_vote", "yes") == "yes",
        "watchdog": getattr(args, "watchdog_vote", "yes") == "yes",
        "auditor": getattr(args, "auditor_vote", "yes") == "yes",
    }

    yes_votes = sum(1 for a in AGENTS if votes[a])

    is_emergency = args.proposal_type == "emergency_shutdown"
    if is_emergency:
        consensus_reached = yes_votes >= 1
        required_yes = 1
    else:
        consensus_reached = yes_votes >= REQUIRED_YES
        required_yes = REQUIRED_YES

    validation = {"ok": True, "reason": "ok"}
    if args.proposal_type == "parameter_change":
        validation = validate_parameter_change(args.param, args.value)
    elif args.proposal_type == "asset_listing":
        validation = validate_asset_listing(args.asset)

    proposal_hash = _proposal_hash(args)
    sec_state = load_state()
    cstate = sec_state.setdefault("consensus", {})
    queued_store = cstate.setdefault("queued", {})

    queue_requested = bool(getattr(args, "queue", False))
    execute_requested = bool(getattr(args, "execute", False))
    dry_run = bool(getattr(args, "dry_run", False))
    nonce = int(getattr(args, "nonce", 0))

    # // BLUE-TEAM: G13/G14 - signatures + nonce are required for queue/execute paths.
    auth_ok = _auth_ok(votes, proposal_hash, nonce, args)

    now = int(time.time())
    timelock_seconds = TIMELOCK_SECONDS if args.proposal_type == "parameter_change" else 0
    eta = now + timelock_seconds if consensus_reached and validation["ok"] else None

    queued = False
    executed = False

    can_act = consensus_reached and validation["ok"] and auth_ok and nonce >= int(cstate.get("proposal_nonce", 0))

    if queue_requested and can_act and not dry_run:
        queued_store[proposal_hash] = {
            "proposal_id": args.proposal_id,
            "type": args.proposal_type,
            "eta": eta or now,
            "queued_at": now,
            "nonce": nonce,
            "param": args.param,
            "value": args.value,
        }
        cstate["proposal_nonce"] = nonce + 1
        queued = True
        save_state(sec_state)

    if execute_requested and can_act and not dry_run:
        rec = queued_store.get(proposal_hash)
        if rec is not None and now >= int(rec.get("eta", now)):
            executed = True
            queued_store.pop(proposal_hash, None)
            if args.proposal_type == "parameter_change":
                last_params = cstate.setdefault("last_params", {})
                last_params[args.param] = float(args.value)
            save_state(sec_state)

    action = "REJECT_OR_WAIT"
    if queued:
        action = "QUEUE_GOVERNANCE_ACTION"
    elif executed:
        action = "EXECUTE_GOVERNANCE_ACTION"
    elif queue_requested and can_act:
        action = "WAIT_FOR_QUEUE_CONFIRMATION"

    return {
        "agent": "consensus",
        "mode": "execute" if execute_requested else ("queue" if queue_requested else "dry-run"),
        "proposal": {
            "proposal_id": args.proposal_id,
            "type": args.proposal_type,
            "asset": args.asset if args.proposal_type == "asset_listing" else None,
            "param": args.param if args.proposal_type == "parameter_change" else None,
            "value": args.value if args.proposal_type == "parameter_change" else None,
            "proposal_hash": proposal_hash,
        },
        "voting": {
            "required_yes": required_yes,
            "votes": votes,
            "yes_votes": yes_votes,
            "consensus_reached": consensus_reached,
            "consensus_reached_2_of_3": (consensus_reached if not is_emergency else None),
        },
        "validation": validation,
        "auth": {
            "ok": auth_ok,
            "nonce": nonce,
        },
        "timelock": {
            "seconds": timelock_seconds,
            "eta_unix": eta,
            "eta_iso": datetime.fromtimestamp(eta, tz=timezone.utc).isoformat() if eta else None,
        },
        "decision": {
            "action": action,
            "queued": queued,
            "executed": executed,
        },
    }


def main() -> None:
    args = build_parser().parse_args()
    result = run(args)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
