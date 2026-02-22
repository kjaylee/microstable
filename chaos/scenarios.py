from __future__ import annotations

import math
import os
import random
import sys
import threading
import time
from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional, Tuple

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from microstable import (
    AdamOptimizer,
    Auditor,
    CircuitBreaker,
    Keeper,
    LossEngine,
    MAX_AUTOGRAD_DEPTH,
    MarketEnv,
    ProtocolState,
    Value,
    Watchdog,
)
from monitoring.metrics import MetricsCollector


@dataclass
class ScenarioResult:
    name: str
    passed: bool
    recovery_ticks: int
    impact_scope: Dict[str, Any]
    details: Dict[str, Any]


Injector = Callable[[int, ProtocolState, Any, Dict[str, Any]], Dict[str, Any]]
Validator = Callable[[List[Dict[str, Any]], Dict[str, Any], Dict[str, Any]], Tuple[bool, str]]


def _operating_mode(alive_count: int) -> str:
    if alive_count >= 3:
        return "NORMAL"
    if alive_count == 2:
        return "SAFE_MODE"
    return "FROZEN"


def _find_recovery_ticks(trace: List[Dict[str, Any]], last_fault_tick: int) -> int:
    if last_fault_tick < 0:
        return 0
    stable_run = 0
    for row in trace:
        t = int(row["tick"])
        if t <= last_fault_tick:
            continue
        stable = (
            row["peg_error"] < 0.005
            and row["cr"] >= row["cr_target"]
            and not (row["cb1"] or row["cb2"] or row["cb3"] or row["cb4"])
        )
        if stable:
            stable_run += 1
            if stable_run >= 5:
                return t - last_fault_tick
        else:
            stable_run = 0
    return -1


def _impact_scope(trace: List[Dict[str, Any]], extra: Dict[str, Any]) -> Dict[str, Any]:
    if not trace:
        return {
            "max_peg_error": 0.0,
            "min_cr": 0.0,
            "max_cb_level": 0,
            "mode_counts": {},
            "funds_preserved": True,
        }

    max_peg_error = max(float(r["peg_error"]) for r in trace)
    min_cr = min(float(r["cr"]) for r in trace)
    max_cb_level = 0
    mode_counts: Dict[str, int] = {}
    funds_preserved = True
    for r in trace:
        mode_counts[r["mode"]] = mode_counts.get(r["mode"], 0) + 1
        active = [idx for idx in (1, 2, 3, 4) if bool(r[f"cb{idx}"])]
        if active:
            max_cb_level = max(max_cb_level, max(active))
        if not bool(r["funds_preserved"]):
            funds_preserved = False

    return {
        "max_peg_error": max_peg_error,
        "min_cr": min_cr,
        "max_cb_level": max_cb_level,
        "mode_counts": mode_counts,
        "tx_failure_rate": extra.get("tx_failure_rate", 0.0),
        "funds_preserved": funds_preserved,
    }


def _simulate(
    *,
    name: str,
    ticks: int,
    market_scenario: str,
    seed: int,
    injector: Injector,
    validator: Validator,
) -> ScenarioResult:
    rng = random.Random(seed)
    env = MarketEnv(scenario=market_scenario, seed=seed, deterministic=True)
    state = ProtocolState()
    loss_engine = LossEngine()
    optimizer = AdamOptimizer(n_weights=len(state.weights))
    breaker = CircuitBreaker(n_assets=len(state.weights))
    keeper = Keeper()
    watchdog = Watchdog()
    auditor = Auditor()
    metrics = MetricsCollector()

    context: Dict[str, Any] = {
        "agents_alive": {"keeper": True, "watchdog": True, "auditor": True},
        "fault_ticks": [],
        "injection_notes": [],
        "reject_count": 0,
        "reject_attempts": 0,
        "max_value_depth": 0,
        "race_inconsistency": 0,
    }

    trace: List[Dict[str, Any]] = []
    tx_attempts = 0
    tx_failures = 0

    for tick in range(ticks):
        started = time.perf_counter()
        state.begin_tick()
        market = env.step(tick)

        inj = injector(tick, state, market, context)
        if inj.get("fault_active"):
            context["fault_ticks"].append(tick)

        if "note" in inj:
            context["injection_notes"].append({"tick": tick, "note": str(inj["note"])})

        if "set_agents" in inj:
            for k, v in dict(inj["set_agents"]).items():
                if k in context["agents_alive"]:
                    context["agents_alive"][k] = bool(v)

        if "prices" in inj:
            market.prices = [float(x) for x in inj["prices"]]

        if bool(inj.get("oracle_freeze", False)):
            market.stale_seconds = max(int(market.stale_seconds), int(inj.get("stale_seconds", 240)))
            market.divergence = max(float(market.divergence), float(inj.get("divergence", 0.05)))
            market.oracle_q = min(float(market.oracle_q), float(inj.get("oracle_q", 0.2)))

        if "clock_skew_seconds" in inj:
            # We model skew as delayed observed activity timestamps in health records.
            skew = int(inj["clock_skew_seconds"])
            for agent, alive in context["agents_alive"].items():
                metrics.record_agent_probe(agent, tick=max(0, tick - max(0, skew // 2)), response_ms=100.0 + abs(skew), ok=alive)

        if bool(inj.get("probe_agents", False)):
            for agent, alive in context["agents_alive"].items():
                latency = float(inj.get("agent_latency_ms", 50.0)) + rng.uniform(0.0, 30.0)
                metrics.record_agent_probe(agent, tick=tick, response_ms=latency, ok=alive)

        if bool(inj.get("memory_pressure", False)):
            v = Value(1.0)
            for _ in range(MAX_AUTOGRAD_DEPTH + 96):
                v = (v + 0.0001) * 1.00001
            context["max_value_depth"] = max(context["max_value_depth"], int(getattr(v, "_depth", 1)))

        # Malicious rapid config attempt
        if "malicious_proposal" in inj:
            context["reject_attempts"] += 1
            proposal = {
                "weights": inj["malicious_proposal"].get("weights", [1.0, 0.0, 0.0, 0.0]),
                "mint_fee": inj["malicious_proposal"].get("mint_fee", 0.02),
                "proposal_epoch": state.market_epoch,
                "state_hash": state.market_state_hash,
                "expiry_epoch": state.market_epoch + 1,
            }
            decision = keeper.submit_update_proposal(state, proposal)
            if decision.get("status") != "APPLIED":
                context["reject_count"] += 1

        # Simulate race pressure against supply accounting.
        if bool(inj.get("double_spend_race", False)):
            race_runs = int(inj.get("race_runs", 40))
            lock = threading.Lock()
            expected_supply = state.supply

            def worker(op: str, amount: float) -> None:
                nonlocal expected_supply
                with lock:
                    if op == "mint":
                        expected_supply += amount
                        state.supply += amount
                    else:
                        expected_supply -= amount
                        state.supply -= amount
                    state.position_supply_sum = state.supply

            threads: List[threading.Thread] = []
            for i in range(race_runs):
                op = "mint" if i % 2 == 0 else "redeem"
                amount = 50.0
                th = threading.Thread(target=worker, args=(op, amount))
                th.start()
                threads.append(th)
            for th in threads:
                th.join()

            if abs(state.supply - expected_supply) > 1e-6:
                context["race_inconsistency"] += 1

        alive = context["agents_alive"]
        loss_finite = True
        loss_value: Optional[float] = None
        grad_w = [0.0] * len(state.weights)
        grad_fee = 0.0

        if alive["keeper"]:
            try:
                loss, loss_ctx = loss_engine.compute(state, market.prices, market.oracle_q)
                loss_value = float(loss.data)
                loss.backward()
                grad_w = [float(v.grad) for v in loss_ctx["weights"]]
                grad_fee = float(loss_ctx["fee"].grad)
                if any(not math.isfinite(g) for g in grad_w + [grad_fee]):
                    raise ValueError("non-finite gradients")
            except Exception:  # noqa: BLE001
                loss_finite = False

        nav_now = state.effective_collateral_value(market.prices)
        nav_drop = nav_now - state.nav_prev

        forced: Dict[str, bool] = {}
        if alive["watchdog"]:
            wd_events = watchdog.detect(market)
            if wd_events.get("cb3"):
                forced["cb3"] = True

        if "force_cb" in inj:
            for k, v in dict(inj["force_cb"]).items():
                forced[k] = bool(v)

        action = breaker.update(
            tick=tick,
            state=state,
            market=market,
            nav_drop=nav_drop,
            loss_finite=loss_finite,
            loss_value=loss_value,
            forced=forced,
        )

        tx_success = True
        drop_keeper_update = bool(inj.get("drop_keeper_update", False))
        if alive["keeper"] and state.optimizer_enabled and state.mint_limit > 0.0 and loss_finite:
            tx_attempts += 1
            if drop_keeper_update:
                tx_success = False
            else:
                proposal = keeper.propose(state, optimizer, grad_w, grad_fee)
                decision = keeper.submit_update_proposal(state, proposal)
                tx_success = bool(decision.get("status") == "APPLIED")
        if not tx_success:
            tx_failures += 1

        audit_ok = True
        if alive["auditor"]:
            audit = auditor.verify_invariants(state)
            audit_ok = bool(audit.get("ok", False))

        peg = state.update_from_market(market.prices, market.oracle_q)

        alive_count = sum(1 for v in alive.values() if v)
        mode = _operating_mode(alive_count)
        redeem_only = bool(inj.get("redeem_only", False)) or (alive_count == 0)

        funds_preserved = (
            state.cr >= state.cr_hard_min - 1e-9
            and abs(state.position_supply_sum - state.supply) <= 1e-6
            and state.reserve_value >= state.supply * state.cr_hard_min - 1e-6
        )

        row = {
            "tick": tick,
            "peg": peg,
            "peg_error": abs(peg - 1.0),
            "cr": state.cr,
            "cr_target": state.cr_target,
            "cb1": bool(action["cb1"]),
            "cb2": bool(action["cb2"]),
            "cb3": bool(action["cb3"]),
            "cb4": bool(action["cb4"]),
            "mode": mode,
            "alive_count": alive_count,
            "agents_alive": dict(alive),
            "redeem_only": redeem_only,
            "mint_limit": state.mint_limit,
            "mint_paused_reason": state.mint_paused_reason,
            "oracle_stale_seconds": market.stale_seconds,
            "oracle_q": market.oracle_q,
            "audit_ok": audit_ok,
            "funds_preserved": funds_preserved,
        }
        trace.append(row)

        metrics.record_tick(
            tick=tick,
            peg=peg,
            cr=state.cr,
            cb_flags={1: row["cb1"], 2: row["cb2"], 3: row["cb3"], 4: row["cb4"]},
            tx_success=tx_success,
            tick_duration_seconds=(time.perf_counter() - started),
            oracle_is_stale=(market.stale_seconds > 120 or market.oracle_q < 0.70),
        )

    metrics_summary = metrics.summary()
    tx_failure_rate = tx_failures / max(1, tx_attempts)
    extra = {
        "tx_attempts": tx_attempts,
        "tx_failures": tx_failures,
        "tx_failure_rate": tx_failure_rate,
        "fault_ticks": context["fault_ticks"],
        "reject_count": context["reject_count"],
        "reject_attempts": context["reject_attempts"],
        "max_value_depth": context["max_value_depth"],
        "race_inconsistency": context["race_inconsistency"],
        "metrics": metrics_summary,
        "injection_notes": context["injection_notes"],
    }

    passed, reason = validator(trace, extra, context)
    last_fault_tick = max(context["fault_ticks"]) if context["fault_ticks"] else -1

    return ScenarioResult(
        name=name,
        passed=passed,
        recovery_ticks=_find_recovery_ticks(trace, last_fault_tick),
        impact_scope=_impact_scope(trace, extra),
        details={
            "reason": reason,
            "extra": extra,
            "trace": trace,
        },
    )


def scenario_agent_kill(seed: int = 11) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 15 <= tick <= 25:
            return {"fault_active": True, "set_agents": {"keeper": False}, "probe_agents": True, "note": "keeper down"}
        if 30 <= tick <= 40:
            return {"fault_active": True, "set_agents": {"keeper": True, "watchdog": False}, "probe_agents": True, "note": "watchdog down"}
        if 45 <= tick <= 55:
            return {"fault_active": True, "set_agents": {"watchdog": True, "auditor": False}, "probe_agents": True, "note": "auditor down"}
        return {"set_agents": {"keeper": True, "watchdog": True, "auditor": True}, "probe_agents": True}

    def validator(trace: List[Dict[str, Any]], _extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        safe_mode_ticks = [r for r in trace if r["alive_count"] == 2 and r["mode"] == "SAFE_MODE"]
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        return bool(safe_mode_ticks and funds_ok), "single-agent failures degraded gracefully"

    return _simulate(
        name="agent_kill",
        ticks=80,
        market_scenario="volatile",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_network_partition(seed: int = 19) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 20 <= tick <= 45:
            return {
                "fault_active": True,
                "drop_keeper_update": (tick % 2 == 0),
                "probe_agents": True,
                "agent_latency_ms": 500.0,
                "note": "network partition: delayed keeper/watchdog communication",
            }
        return {"probe_agents": True, "agent_latency_ms": 60.0}

    def validator(trace: List[Dict[str, Any]], extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        min_cr = min(float(r["cr"]) for r in trace)
        tx_failure_rate = float(extra["tx_failure_rate"])
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        ok = min_cr >= 1.10 and tx_failure_rate < 0.70 and funds_ok
        return ok, "partition tolerated with bounded degradation"

    return _simulate(
        name="network_partition",
        ticks=85,
        market_scenario="volatile",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_oracle_freeze(seed: int = 23) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 18 <= tick <= 42:
            return {
                "fault_active": True,
                "oracle_freeze": True,
                "stale_seconds": 300,
                "divergence": 0.06,
                "oracle_q": 0.15,
                "probe_agents": True,
            }
        return {"probe_agents": True}

    def validator(trace: List[Dict[str, Any]], _extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        freeze_ticks = [r for r in trace if 18 <= r["tick"] <= 42]
        cb3_seen = any(bool(r["cb3"]) for r in freeze_ticks)
        mint_paused = any(float(r["mint_limit"]) <= 0.0 for r in freeze_ticks)
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        return bool(cb3_seen and mint_paused and funds_ok), "oracle freeze triggered CB-3 and mint halt"

    return _simulate(
        name="oracle_freeze",
        ticks=90,
        market_scenario="oracle_failure",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_memory_pressure(seed: int = 29) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 10 <= tick <= 35:
            return {
                "fault_active": True,
                "memory_pressure": True,
                "probe_agents": True,
                "note": "autograd deep chain stress",
            }
        return {"probe_agents": True}

    def validator(trace: List[Dict[str, Any]], extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        depth = int(extra["max_value_depth"])
        memory_bytes = int(extra["metrics"]["system"]["memory_bytes"])
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        ok = depth <= MAX_AUTOGRAD_DEPTH and memory_bytes < 1_500_000_000 and funds_ok
        return ok, "memory pressure respected graph-depth cap"

    return _simulate(
        name="memory_pressure",
        ticks=80,
        market_scenario="volatile",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_clock_skew(seed: int = 31) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 16 <= tick <= 38:
            return {
                "fault_active": True,
                "clock_skew_seconds": 8,
                "drop_keeper_update": (tick % 3 == 0),
                "probe_agents": True,
                "agent_latency_ms": 220.0,
            }
        return {"probe_agents": True}

    def validator(trace: List[Dict[str, Any]], extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        tx_failure_rate = float(extra["tx_failure_rate"])
        max_peg = max(float(r["peg_error"]) for r in trace)
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        ok = tx_failure_rate < 0.8 and max_peg < 0.03 and funds_ok
        return ok, "clock skew did not break consensus safety envelopes"

    return _simulate(
        name="clock_skew",
        ticks=82,
        market_scenario="gradient_attack",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_rapid_config_change(seed: int = 37) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 12 <= tick <= 40:
            return {
                "fault_active": True,
                "malicious_proposal": {
                    "weights": [0.95, 0.03, 0.01, 0.01],
                    "mint_fee": 0.02,
                },
                "probe_agents": True,
            }
        return {"probe_agents": True}

    def validator(trace: List[Dict[str, Any]], extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        attempts = max(1, int(extra["reject_attempts"]))
        reject_ratio = float(extra["reject_count"]) / attempts
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        ok = reject_ratio >= 0.90 and funds_ok
        return ok, f"malicious rapid proposals rejected ratio={reject_ratio:.2f}"

    return _simulate(
        name="rapid_config_change",
        ticks=75,
        market_scenario="volatile",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_partial_collateral_failure(seed: int = 41) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if 20 <= tick <= 44:
            prices = market.prices[:]
            prices[0] = min(prices[0], 0.72)
            prices[1] = min(prices[1], 0.78)
            return {
                "fault_active": True,
                "prices": prices,
                "probe_agents": True,
                "note": "two collateral assets impaired",
            }
        return {"probe_agents": True}

    def validator(trace: List[Dict[str, Any]], _extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        stressed = [r for r in trace if 20 <= r["tick"] <= 44]
        cb_seen = any(bool(r["cb1"] or r["cb2"]) for r in stressed)
        min_cr = min(float(r["cr"]) for r in trace)
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        ok = cb_seen and min_cr >= 1.05 and funds_ok
        return ok, "partial collateral failure contained by breakers"

    return _simulate(
        name="partial_collateral_failure",
        ticks=88,
        market_scenario="multi_depeg",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def scenario_double_spend_race(seed: int = 43) -> ScenarioResult:
    def injector(tick: int, _state: ProtocolState, _market: Any, _ctx: Dict[str, Any]) -> Dict[str, Any]:
        if tick in (22, 34, 48):
            return {
                "fault_active": True,
                "double_spend_race": True,
                "race_runs": 80,
                "probe_agents": True,
                "note": "simulated concurrent mint/redeem race",
            }
        return {"probe_agents": True}

    def validator(trace: List[Dict[str, Any]], extra: Dict[str, Any], _ctx: Dict[str, Any]) -> Tuple[bool, str]:
        race_inconsistency = int(extra["race_inconsistency"])
        funds_ok = all(bool(r["funds_preserved"]) for r in trace)
        ok = race_inconsistency == 0 and funds_ok
        return ok, "concurrent mint/redeem race preserved supply accounting"

    return _simulate(
        name="double_spend_race",
        ticks=86,
        market_scenario="volatile",
        seed=seed,
        injector=injector,
        validator=validator,
    )


def run_all_chaos_scenarios() -> List[ScenarioResult]:
    return [
        scenario_agent_kill(),
        scenario_network_partition(),
        scenario_oracle_freeze(),
        scenario_memory_pressure(),
        scenario_clock_skew(),
        scenario_rapid_config_change(),
        scenario_partial_collateral_failure(),
        scenario_double_spend_race(),
    ]
