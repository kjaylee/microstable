#!/usr/bin/env python3
from __future__ import annotations

import json
import math
import os
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


class BridgeError(Exception):
    pass


def _microstable_root() -> Path:
    env = os.getenv("MICROSTABLE_ROOT")
    if env:
        return Path(env).expanduser().resolve()
    # .../microstable/mcp-server/scripts/microstable-bridge.py -> microstable root
    return Path(__file__).resolve().parents[2]


def _state_path() -> Path:
    return _microstable_root() / "mcp-server" / ".state" / "microstable-bridge-state.json"


def _default_state() -> Dict[str, Any]:
    return {
        "protocol": {
            "weights": [0.4, 0.3, 0.2, 0.1],
            "mint_fee": 0.002,
            "cr": 1.28,
            "cr_target": 1.2,
            "supply": 1_000_000.0,
            "reserve_value": 1_280_000.0,
            "current_loss": None,
            "last_epoch": 0,
            "last_update_ms": int(time.time() * 1000),
        },
        "agents": {},
        "tournaments": {},
        "anomaly_reports": [],
        "resolved_alerts": [],
        "heartbeats": {},
    }


def _load_state() -> Dict[str, Any]:
    path = _state_path()
    if not path.exists():
        return _default_state()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise BridgeError("state file root must be object")
        base = _default_state()
        base.update(data)
        return base
    except Exception as e:
        raise BridgeError(f"failed to load bridge state: {e}") from e


def _save_state(state: Dict[str, Any]) -> None:
    path = _state_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    state.setdefault("protocol", {})["last_update_ms"] = int(time.time() * 1000)
    path.write_text(json.dumps(state, indent=2, sort_keys=True), encoding="utf-8")


def _import_modules():
    root = _microstable_root()
    root_str = str(root)
    if root_str not in sys.path:
        sys.path.insert(0, root_str)
    try:
        import microstable as ms  # type: ignore
        import open_agent_economy as oae  # type: ignore
    except Exception as e:
        raise BridgeError(f"failed to import core modules from {root}: {e}") from e
    return ms, oae


def _canonical_agent_type(raw: str) -> str:
    lut = {
        "optimizer": "Optimizer",
        "monitor": "Monitor",
        "auditor": "Auditor",
        "liquidator": "Liquidator",
    }
    key = str(raw).strip().lower()
    if key not in lut:
        raise BridgeError(f"invalid agent type: {raw}")
    return lut[key]


def _protocol_state_from_store(ms: Any, store: Dict[str, Any]):
    ps = ms.ProtocolState()
    proto = store.get("protocol", {})
    for key in ["weights", "mint_fee", "cr", "cr_target", "supply", "reserve_value"]:
        if key in proto:
            setattr(ps, key, proto[key])
    return ps


def _build_runtime_from_state(oae: Any, state: Dict[str, Any]) -> Tuple[Any, Any, Any]:
    registry = oae.AgentRegistry()
    for agent_id, row in state.get("agents", {}).items():
        registry.records[agent_id] = oae.AgentRecord(
            agent_id=agent_id,
            agent_type=row["agent_type"],
            stake=float(row.get("stake", 0.0)),
            reputation=int(row.get("reputation", 0)),
            registered_at=int(row.get("registered_at", 0)),
            last_active=int(row.get("last_active", 0)),
            total_rewards=float(row.get("total_rewards", 0.0)),
            total_slashed=float(row.get("total_slashed", 0.0)),
            proposals_submitted=int(row.get("proposals_submitted", 0)),
            proposals_accepted=int(row.get("proposals_accepted", 0)),
            status=row.get("status", "Active"),
        )

    reputation = oae.ReputationEngine()
    for agent_id, row in state.get("agents", {}).items():
        reputation.scores[agent_id] = int(row.get("reputation", 0))

    staking = oae.StakingEconomics(registry)
    for agent_id, row in state.get("agents", {}).items():
        staking.balances[agent_id] = float(row.get("balance", row.get("stake", 0.0)))

    return registry, reputation, staking


def _sync_runtime_to_state(state: Dict[str, Any], registry: Any, reputation: Any, staking: Any) -> None:
    agents = state.setdefault("agents", {})
    for agent_id, rec in registry.records.items():
        prev = agents.get(agent_id, {})
        prev_total_rewards = float(prev.get("total_rewards", 0.0))
        new_total_rewards = float(rec.total_rewards)
        delta_rewards = max(0.0, new_total_rewards - prev_total_rewards)

        agents[agent_id] = {
            "agent_type": rec.agent_type,
            "stake": float(rec.stake),
            "balance": float(staking.balances.get(agent_id, rec.stake)),
            "status": rec.status,
            "reputation": int(reputation.get(agent_id)),
            "registered_at": int(rec.registered_at),
            "last_active": int(rec.last_active),
            "total_rewards": new_total_rewards,
            "total_slashed": float(rec.total_slashed),
            "proposals_submitted": int(rec.proposals_submitted),
            "proposals_accepted": int(rec.proposals_accepted),
            "claimable_rewards": float(prev.get("claimable_rewards", 0.0)) + delta_rewards,
            "claimed_rewards": float(prev.get("claimed_rewards", 0.0)),
        }


def cmd_state(params: Dict[str, Any]) -> Dict[str, Any]:
    ms, _oae = _import_modules()
    state = _load_state()

    scenario = str(params.get("scenario", "normal"))
    seed = int(params.get("seed", 0))
    epoch = int(params.get("epoch", state["protocol"].get("last_epoch", 0)))

    protocol = _protocol_state_from_store(ms, state)
    env = ms.MarketEnv(scenario=scenario, seed=seed, deterministic=True)
    market = env.step(epoch)

    nav = protocol.effective_collateral_value(market.prices)
    peg = 1.0 + protocol.peg_sensitivity(market.prices) * (nav - 1.0) + 0.0010 * (market.oracle_q - 1.0)
    peg = min(1.10, max(0.90, peg))

    wd = ms.Watchdog().detect(market)
    breakers = {
        "cb1": bool(wd.get("cb1", False)),
        "cb2": bool(wd.get("cb2", False)),
        "cb3": bool(wd.get("cb3", False)),
        "cb4": False,
    }

    return {
        "epoch": epoch,
        "scenario": scenario,
        "peg": float(peg),
        "cr": float(protocol.cr),
        "crTarget": float(protocol.cr_target),
        "weights": [float(x) for x in protocol.weights],
        "mintFee": float(protocol.mint_fee),
        "totalSupply": float(protocol.supply),
        "reserveValue": float(protocol.reserve_value),
        "circuitBreaker": breakers,
        "market": {
            "prices": [float(x) for x in market.prices],
            "oracleQ": float(market.oracle_q),
            "staleSeconds": int(market.stale_seconds),
            "divergence": float(market.divergence),
        },
    }


def _normalize_shocks(ms: Any, shocks: Any) -> Dict[Tuple[int, int], float]:
    out: Dict[Tuple[int, int], float] = {}
    if not isinstance(shocks, list):
        return out
    asset_idx = {name: i for i, name in enumerate(ms.ASSETS)}
    for row in shocks:
        if not isinstance(row, dict):
            continue
        tick = int(row.get("tick", -1))
        if tick < 0:
            continue
        asset_raw = row.get("asset", row.get("assetIndex", 0))
        if isinstance(asset_raw, str):
            idx = asset_idx.get(asset_raw.upper())
            if idx is None:
                idx = asset_idx.get(asset_raw)
            if idx is None:
                continue
        else:
            idx = int(asset_raw)
        if idx < 0 or idx >= len(ms.ASSETS):
            continue
        delta = float(row.get("delta", 0.0))
        out[(tick, idx)] = out.get((tick, idx), 0.0) + delta
    return out


def cmd_simulate(params: Dict[str, Any]) -> Dict[str, Any]:
    ms, _oae = _import_modules()
    state = _load_state()

    scenario = str(params.get("scenario", "normal"))
    seed = int(params.get("seed", 0))
    ticks = int(params.get("ticks", 120))
    if ticks <= 0:
        raise BridgeError("ticks must be > 0")

    shocks = _normalize_shocks(ms, params.get("shocks", []))
    original_shock = ms.MarketEnv._shock

    if shocks:
        def patched_shock(self, tick: int, asset_index: int) -> float:  # type: ignore[no-untyped-def]
            base = float(original_shock(self, tick, asset_index))
            extra = float(shocks.get((tick, asset_index), 0.0))
            return base + extra

        ms.MarketEnv._shock = patched_shock  # type: ignore[assignment]

    try:
        summary = ms.run_scenario(scenario=scenario, seed=seed, ticks=ticks, enforce_invariants=True)
    finally:
        ms.MarketEnv._shock = original_shock  # type: ignore[assignment]

    last_row = summary.rows[-1] if summary.rows else {}
    if last_row:
        state["protocol"]["weights"] = [
            float(last_row.get("w0", 0.4)),
            float(last_row.get("w1", 0.3)),
            float(last_row.get("w2", 0.2)),
            float(last_row.get("w3", 0.1)),
        ]
        state["protocol"]["mint_fee"] = float(last_row.get("fee", 0.002))
        state["protocol"]["cr"] = float(last_row.get("cr", 1.28))
        state["protocol"]["cr_target"] = float(last_row.get("cr_target", 1.2))
        state["protocol"]["last_epoch"] = int(last_row.get("tick", ticks - 1))
        _save_state(state)

    return {
        "scenario": summary.scenario,
        "seed": summary.seed,
        "ticks": summary.ticks,
        "mae": float(summary.mae),
        "rmse": float(summary.rmse),
        "minCR": float(summary.min_cr),
        "maxTurnover": float(summary.max_turnover),
        "breakerActivations": {str(k): int(v) for k, v in summary.breaker_activations.items()},
        "breakerFalsePositives": int(summary.breaker_false_positives),
        "breakerFalsePositiveRate": float(summary.breaker_false_positive_rate),
        "crViolationRate": float(summary.cr_violation_rate),
        "final": {
            "cr": float(summary.cr_final),
            "crTarget": float(summary.cr_target_final),
            "mintFee": float(summary.final_fee),
            "weights": state["protocol"]["weights"],
        },
        "gates": {
            "peg": bool(summary.gate_peg_ok),
            "cr": bool(summary.gate_cr_ok),
            "falsePositive": bool(summary.gate_fp_ok),
        },
        "appliedShocks": [
            {"tick": t, "assetIndex": i, "delta": d}
            for (t, i), d in sorted(shocks.items(), key=lambda x: (x[0][0], x[0][1]))
        ],
        "events": summary.events,
    }


def cmd_agent_register(params: Dict[str, Any]) -> Dict[str, Any]:
    _ms, oae = _import_modules()
    state = _load_state()

    agent_id = str(params.get("agent_id") or params.get("agentId") or "").strip()
    if not agent_id:
        raise BridgeError("agent_id is required")

    agent_type = _canonical_agent_type(str(params.get("type") or params.get("agent_type") or params.get("agentType") or ""))
    stake = float(params.get("stake", 0.0))
    if stake <= 0:
        raise BridgeError("stake must be > 0")

    min_stake = float(oae.MIN_STAKE_DEFAULT[agent_type])
    if stake < min_stake:
        raise BridgeError(f"stake too low for {agent_type}; required >= {min_stake}")

    existing = state.get("agents", {}).get(agent_id)
    if existing and existing.get("status") != "Deregistered":
        raise BridgeError(f"agent already exists: {agent_id}")

    epoch = int(params.get("epoch", state["protocol"].get("last_epoch", 0)))

    state["agents"][agent_id] = {
        "agent_type": agent_type,
        "stake": stake,
        "balance": stake,
        "status": "Active",
        "reputation": 0,
        "registered_at": epoch,
        "last_active": epoch,
        "total_rewards": 0.0,
        "total_slashed": 0.0,
        "proposals_submitted": 0,
        "proposals_accepted": 0,
        "claimable_rewards": 0.0,
        "claimed_rewards": 0.0,
    }
    state.setdefault("heartbeats", {})[agent_id] = epoch
    state["protocol"]["last_epoch"] = max(int(state["protocol"].get("last_epoch", 0)), epoch)
    _save_state(state)

    return {
        "status": "registered",
        "agent_id": agent_id,
        "agent_type": agent_type,
        "stake": stake,
        "required_min_stake": min_stake,
        "epoch": epoch,
    }


def _pick_active_optimizer(state: Dict[str, Any]) -> Optional[str]:
    for aid, row in state.get("agents", {}).items():
        if row.get("agent_type") == "Optimizer" and row.get("status") == "Active":
            return aid
    return None


def _pick_active_monitor(state: Dict[str, Any]) -> Optional[str]:
    for aid, row in state.get("agents", {}).items():
        if row.get("agent_type") == "Monitor" and row.get("status") == "Active":
            return aid
    return None


def cmd_propose(params: Dict[str, Any]) -> Dict[str, Any]:
    ms, oae = _import_modules()
    state = _load_state()

    agent_id = str(params.get("agent_id") or params.get("agentId") or "").strip()
    if not agent_id:
        agent_id = _pick_active_optimizer(state) or ""
    if not agent_id:
        raise BridgeError("agent_id is required (or register an active Optimizer first)")

    agent = state.get("agents", {}).get(agent_id)
    if not agent:
        raise BridgeError(f"agent not found: {agent_id}")
    if agent.get("status") != "Active":
        raise BridgeError(f"agent is not active: {agent_id}")
    if agent.get("agent_type") != "Optimizer":
        raise BridgeError("only Optimizer agents can submit proposals")

    epoch = int(params.get("epoch"))
    weights_raw = params.get("weights")
    if not isinstance(weights_raw, list):
        raise BridgeError("weights must be an array of 4 numbers")
    weights = [float(x) for x in weights_raw]

    fees_raw = params.get("fees", {})
    mint_fee: float
    if isinstance(fees_raw, dict):
        if "mint_fee" in fees_raw:
            mint_fee = float(fees_raw["mint_fee"])
        elif "mintFee" in fees_raw:
            mint_fee = float(fees_raw["mintFee"])
        else:
            mint_fee = float(params.get("mint_fee", params.get("mintFee", 0.002)))
    else:
        mint_fee = float(fees_raw)

    if len(weights) != 4:
        raise BridgeError("weights must have exactly 4 elements")
    if any((not math.isfinite(w)) or w < 0.0 for w in weights):
        raise BridgeError("weights must be finite and >= 0")
    if abs(sum(weights) - 1.0) > 1e-6:
        raise BridgeError("sum(weights) must equal 1.0")
    if mint_fee < 0.0 or mint_fee > 0.02:
        raise BridgeError("mint fee out of range [0.0, 0.02]")

    scenario = str(params.get("scenario", "normal"))
    seed = int(params.get("seed", 0))

    protocol = _protocol_state_from_store(ms, state)
    protocol.weights = list(weights)
    protocol.mint_fee = mint_fee

    env = ms.MarketEnv(scenario=scenario, seed=seed, deterministic=True)
    market = env.step(epoch)
    loss_engine = ms.LossEngine()
    loss, _ = loss_engine.compute(protocol, market.prices, market.oracle_q)
    loss_estimate = float(loss.data)

    expected_return = params.get("expected_return", params.get("expectedReturn"))
    if expected_return is None:
        expected_return = max(0.0, 0.03 - loss_estimate)
    expected_return = float(expected_return)

    risk = params.get("risk")
    if risk is None:
        risk = sum((w - 0.25) ** 2 for w in weights) + 0.01
    risk = float(risk)

    proposal = oae.Proposal(
        agent_id=agent_id,
        epoch=epoch,
        weights=list(weights),
        mint_fee=mint_fee,
        loss_estimate=loss_estimate,
        expected_return=expected_return,
        risk=risk,
    )

    secret = str(params.get("secret") or f"secret-{agent_id}")
    commit_hash = proposal.commit_hash(secret)

    tkey = str(epoch)
    tentry = state.setdefault("tournaments", {}).setdefault(tkey, {"proposals": [], "evaluated": False, "winner": None})
    tentry.setdefault("proposals", []).append(
        {
            "agent_id": proposal.agent_id,
            "epoch": proposal.epoch,
            "weights": proposal.weights,
            "mint_fee": proposal.mint_fee,
            "loss_estimate": proposal.loss_estimate,
            "expected_return": proposal.expected_return,
            "risk": proposal.risk,
            "metadata": {"commit_hash": commit_hash, "revealed": True},
        }
    )

    state["protocol"]["last_epoch"] = max(int(state["protocol"].get("last_epoch", 0)), epoch)
    _save_state(state)

    return {
        "status": "proposal_submitted",
        "agent_id": agent_id,
        "epoch": epoch,
        "commit_hash": commit_hash,
        "proposal": {
            "weights": weights,
            "mint_fee": mint_fee,
            "loss_estimate": loss_estimate,
            "expected_return": expected_return,
            "risk": risk,
        },
        "tournament_proposal_count": len(tentry.get("proposals", [])),
    }


def cmd_tournament(params: Dict[str, Any]) -> Dict[str, Any]:
    _ms, oae = _import_modules()
    state = _load_state()

    epoch = int(params.get("epoch"))
    epoch_fees = float(params.get("epoch_fees", params.get("epochFees", 100.0)))
    force = bool(params.get("force", False))

    tkey = str(epoch)
    tentry = state.get("tournaments", {}).get(tkey)
    if not tentry:
        raise BridgeError(f"no tournament found for epoch={epoch}")

    if tentry.get("evaluated") and not force:
        return {
            "epoch": epoch,
            "status": "already_evaluated",
            "winner": tentry.get("winner"),
            "ranking": tentry.get("ranking", []),
            "hint": "use force=true to re-evaluate",
        }

    proposals_raw = tentry.get("proposals", [])
    if not proposals_raw:
        raise BridgeError("tournament has no proposals")

    registry, reputation, staking = _build_runtime_from_state(oae, state)
    tournament = oae.OptimizationTournament(registry, reputation, staking)
    tournament.start_epoch(epoch)
    tournament.current_params = {
        "weights": list(state["protocol"].get("weights", [0.4, 0.3, 0.2, 0.1])),
        "mint_fee": float(state["protocol"].get("mint_fee", 0.002)),
    }
    tournament.current_loss = state["protocol"].get("current_loss")

    proposals: List[Any] = []
    for row in proposals_raw:
        prop = oae.Proposal(
            agent_id=row["agent_id"],
            epoch=int(row["epoch"]),
            weights=[float(x) for x in row["weights"]],
            mint_fee=float(row["mint_fee"]),
            loss_estimate=float(row["loss_estimate"]),
            expected_return=float(row["expected_return"]),
            risk=float(row["risk"]),
            metadata=dict(row.get("metadata", {})),
        )
        if tournament.submit_direct(prop):
            proposals.append(prop)

    if not proposals:
        raise BridgeError("no valid active proposals available for evaluation")

    winner = tournament.evaluate(epoch_fees)
    ranking = sorted(
        [
            {
                "agent_id": p.agent_id,
                "score": float(tournament._score(p)),
                "loss_estimate": float(p.loss_estimate),
                "expected_return": float(p.expected_return),
                "risk": float(p.risk),
            }
            for p in proposals
        ],
        key=lambda x: x["score"],
        reverse=True,
    )

    _sync_runtime_to_state(state, registry, reputation, staking)

    if winner is not None:
        state["protocol"]["weights"] = list(winner.weights)
        state["protocol"]["mint_fee"] = float(winner.mint_fee)
        state["protocol"]["current_loss"] = float(winner.loss_estimate)

    tentry["evaluated"] = True
    tentry["winner"] = (
        {
            "agent_id": winner.agent_id,
            "weights": list(winner.weights),
            "mint_fee": float(winner.mint_fee),
            "loss_estimate": float(winner.loss_estimate),
        }
        if winner is not None
        else None
    )
    tentry["ranking"] = ranking
    state["tournaments"][tkey] = tentry
    _save_state(state)

    return {
        "epoch": epoch,
        "status": "evaluated",
        "winner": tentry["winner"],
        "ranking": ranking,
        "epoch_fees": epoch_fees,
    }


def cmd_report_anomaly(params: Dict[str, Any]) -> Dict[str, Any]:
    _ms, oae = _import_modules()
    state = _load_state()

    agent_id = str(params.get("agent_id") or params.get("agentId") or "").strip()
    if not agent_id:
        agent_id = _pick_active_monitor(state) or ""
    if not agent_id:
        raise BridgeError("agent_id is required (or register an active Monitor first)")

    agent = state.get("agents", {}).get(agent_id)
    if not agent:
        raise BridgeError(f"agent not found: {agent_id}")
    if agent.get("status") != "Active":
        raise BridgeError(f"agent is not active: {agent_id}")

    anomaly_type = str(params.get("anomaly_type") or params.get("anomalyType") or "ANOMALY")
    evidence = params.get("evidence")
    if evidence is None:
        evidence = {}
    if not isinstance(evidence, dict):
        raise BridgeError("evidence must be an object")

    epoch = int(params.get("epoch", state["protocol"].get("last_epoch", 0)))
    method = str(params.get("method", "default"))

    evidence.setdefault("timestamp", epoch)
    evidence.setdefault("snapshot", {"protocol": state.get("protocol", {})})
    evidence.setdefault("oracle", {"source": "simulation"})

    report_row = {
        "agent_id": agent_id,
        "alert_type": anomaly_type,
        "epoch": epoch,
        "evidence": evidence,
        "method": method,
    }
    state.setdefault("anomaly_reports", []).append(report_row)

    active_monitors = [
        aid
        for aid, row in state.get("agents", {}).items()
        if row.get("agent_type") == "Monitor" and row.get("status") == "Active"
    ]
    unique_voters = {
        r["agent_id"]
        for r in state.get("anomaly_reports", [])
        if int(r.get("epoch", -1)) == epoch and r.get("alert_type") == anomaly_type
    }

    n = len(active_monitors)
    threshold = min(3, math.ceil(n / 2)) if n > 0 else 0
    consensus = n > 0 and len(unique_voters) >= threshold

    resolve = bool(params.get("resolve", False))
    is_true = bool(params.get("is_true", params.get("isTrue", False)))

    resolved_key = f"{epoch}:{anomaly_type}"
    already_resolved = resolved_key in set(state.get("resolved_alerts", []))
    resolved_now = False

    if consensus and resolve and not already_resolved:
        registry, reputation, staking = _build_runtime_from_state(oae, state)
        watchdog = oae.FederatedWatchdog(registry, staking, reputation)

        for row in state.get("anomaly_reports", []):
            if int(row.get("epoch", -1)) != epoch or row.get("alert_type") != anomaly_type:
                continue
            watchdog.report(
                agent_id=row["agent_id"],
                alert_type=row["alert_type"],
                evidence=row["evidence"],
                epoch=epoch,
                method=row.get("method", "default"),
            )

        watchdog.resolve(anomaly_type, epoch, is_true=is_true)
        _sync_runtime_to_state(state, registry, reputation, staking)
        state.setdefault("resolved_alerts", []).append(resolved_key)
        resolved_now = True

    state["protocol"]["last_epoch"] = max(int(state["protocol"].get("last_epoch", 0)), epoch)
    _save_state(state)

    return {
        "status": "reported",
        "agent_id": agent_id,
        "alert_type": anomaly_type,
        "epoch": epoch,
        "consensus": {
            "active_monitors": n,
            "required_votes": threshold,
            "votes": len(unique_voters),
            "reached": consensus,
        },
        "resolution": {
            "requested": resolve,
            "is_true": is_true,
            "already_resolved": already_resolved,
            "resolved_now": resolved_now,
        },
    }


def dispatch(action: str, params: Dict[str, Any]) -> Dict[str, Any]:
    handlers = {
        "state": cmd_state,
        "simulate": cmd_simulate,
        "agent_register": cmd_agent_register,
        "propose": cmd_propose,
        "tournament": cmd_tournament,
        "report_anomaly": cmd_report_anomaly,
    }
    fn = handlers.get(action)
    if fn is None:
        raise BridgeError(f"unknown action: {action}")
    return fn(params)


def main() -> None:
    raw = sys.stdin.read()
    if not raw.strip():
        print(json.dumps({"ok": False, "error": {"message": "empty stdin"}}))
        raise SystemExit(1)

    try:
        req = json.loads(raw)
        if not isinstance(req, dict):
            raise BridgeError("request must be an object")
        action = str(req.get("action") or "").strip()
        params = req.get("params", {})
        if not action:
            raise BridgeError("action is required")
        if not isinstance(params, dict):
            raise BridgeError("params must be an object")

        result = dispatch(action, params)
        print(json.dumps({"ok": True, "result": result}, ensure_ascii=False))
    except Exception as e:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": {
                        "type": e.__class__.__name__,
                        "message": str(e),
                    },
                },
                ensure_ascii=False,
            )
        )
        raise SystemExit(1)


if __name__ == "__main__":
    main()
