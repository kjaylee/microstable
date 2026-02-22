#!/usr/bin/env python3
"""
Extreme stress test harness for microstable.py

Implements 8 extreme scenarios (100 Monte Carlo runs each), saves:
- outputs/stress-test-results.json (raw run data + summaries)
- outputs/stress-test-report.md   (human-readable report)

Runtime target: < 10 minutes on Apple Silicon.
"""

from __future__ import annotations

import gc
import json
import math
import os
import random
import resource
import statistics
import time
import traceback
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple

import microstable as ms
from security.invariant_monitor import InvariantMonitor


RUNS_PER_SCENARIO = 100
GATE_MAE_MAX = 0.01
GATE_CR_VIOL_MAX = 0.10
GATE_FP_MAX = 0.10

BASE_OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "outputs")
RAW_JSON_PATH = os.path.join(BASE_OUTPUT_DIR, "stress-test-results.json")
REPORT_MD_PATH = os.path.join(BASE_OUTPUT_DIR, "stress-test-report.md")


@dataclass
class TickSpec:
    prices: List[float]
    oracle_q: float = 1.0
    stale_seconds: int = 0
    divergence: float = 0.0
    expected_breakers: Optional[List[int]] = None
    forced: Optional[Dict[str, bool]] = None
    peg_noise: float = 0.0
    supply: Optional[float] = None
    true_prices: Optional[List[float]] = None


def clamp(x: float, lo: float, hi: float) -> float:
    return min(hi, max(lo, x))


def summary_stats(values: Sequence[float], lower_is_worse: bool = False) -> Dict[str, float]:
    vals = [float(v) for v in values if v is not None and math.isfinite(v)]
    if not vals:
        return {"mean": 0.0, "median": 0.0, "p5": 0.0, "p95": 0.0, "worst": 0.0}
    s = ms.summarize_stats(vals)
    s["worst"] = min(vals) if lower_is_worse else max(vals)
    return s


def calc_expected_breakers(prices: Sequence[float], stale_seconds: int, divergence: float) -> List[int]:
    expected: List[int] = []
    depeg_count = sum(1 for p in prices if abs(p - 1.0) > 0.02)
    if depeg_count >= 1:
        expected.append(1)
    if depeg_count >= 2:
        expected.append(2)
    if stale_seconds > 120 or divergence > 0.02:
        expected.append(3)
    return expected


def finite_state(state: ms.ProtocolState) -> bool:
    vals = [state.cr, state.mint_fee, state.reserve_value, state.supply] + state.weights + state.w_caps
    return all(math.isfinite(v) for v in vals)


def count_value_objects() -> int:
    gc.collect()
    return sum(1 for obj in gc.get_objects() if isinstance(obj, ms.Value))


def simulate(
    seed: int,
    ticks: int,
    tick_fn: Callable[[int, random.Random, ms.ProtocolState], TickSpec],
    init_state_fn: Optional[Callable[[ms.ProtocolState], None]] = None,
    observer_fn: Optional[Callable[[int, Dict[str, Any]], None]] = None,
    collect_mem_metrics: bool = False,
) -> Dict[str, Any]:
    rng = random.Random(seed)

    state = ms.ProtocolState()
    if init_state_fn is not None:
        init_state_fn(state)

    loss_engine = ms.LossEngine()
    optimizer = ms.AdamOptimizer(n_weights=len(state.weights))
    breaker = ms.CircuitBreaker(n_assets=len(state.weights))
    keeper = ms.Keeper()
    # // BLUE-TEAM: DEF-INV - runtime invariant monitor executes every simulation tick.
    invariant_monitor = InvariantMonitor()

    peg_errors: List[float] = []
    sq_errors: List[float] = []
    cr_values: List[float] = []
    turnover_sum = 0.0
    max_turnover = 0.0
    min_cr = float("inf")
    cr_violations = 0

    activation_counts = {1: 0, 2: 0, 3: 0, 4: 0}
    false_positives = 0

    event_idx = 0
    checkpoint_state = state.clone()
    checkpoint_lr = optimizer.lr

    nan_inf_detected = False
    hang_detected = False  # bounded loop design => should remain False

    memory_start = None
    memory_end = None
    value_nodes_start = None
    value_nodes_end = None

    if collect_mem_metrics:
        value_nodes_start = count_value_objects()
        memory_start = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss

    run_start = time.perf_counter()

    try:
        for tick in range(ticks):
            state.begin_tick()
            spec = tick_fn(tick, rng, state)

            if spec.supply is not None:
                state.supply = float(spec.supply)
                state.reserve_value = state.cr * state.supply

            expected_breakers = spec.expected_breakers
            if expected_breakers is None:
                expected_breakers = calc_expected_breakers(spec.prices, spec.stale_seconds, spec.divergence)

            market = ms.MarketTick(
                tick=tick,
                prices=spec.prices[:],
                oracle_q=float(spec.oracle_q),
                stale_seconds=int(spec.stale_seconds),
                divergence=float(spec.divergence),
                expected_breakers=sorted(set(expected_breakers)),
            )

            loss_finite = True
            loss_value: Optional[float] = None
            grad_w = [0.0] * len(state.weights)
            grad_fee = 0.0

            try:
                loss, ctx = loss_engine.compute(state, market.prices, market.oracle_q)
                loss_value = loss.data
                loss.backward()
                grad_w = [wv.grad for wv in ctx["weights"]]  # type: ignore[index]
                grad_fee = float(ctx["fee"].grad)  # type: ignore[index]
                if any((not math.isfinite(g)) for g in grad_w + [grad_fee]):
                    raise ValueError("non-finite gradients")
            except Exception:
                loss_finite = False

            nav_now = state.effective_collateral_value(market.prices)
            nav_drop = nav_now - state.nav_prev

            action = breaker.update(
                tick=tick,
                state=state,
                market=market,
                nav_drop=nav_drop,
                loss_finite=loss_finite,
                loss_value=loss_value,
                forced=spec.forced or {},
            )

            # process newly logged breaker events
            new_events: List[Dict[str, Any]] = []
            while event_idx < len(breaker.events):
                ev = breaker.events[event_idx]
                new_events.append(ev)
                if ev["event"] == "activate":
                    cb_id = int(ev["cb"])
                    activation_counts[cb_id] += 1
                    if cb_id not in market.expected_breakers:
                        false_positives += 1
                    invariant_monitor.record_agent_action("watchdog", tick, action_type=f"cb{cb_id}_activate")
                event_idx += 1

            # CB-4 rollback
            if action["rollback"]:
                state = checkpoint_state.clone()
                optimizer.lr = max(1e-5, checkpoint_lr * 0.5)
                if action["cb1"]:
                    idx = breaker.cb1_target_index
                    state.w_caps[idx] = min(state.w_caps[idx], state.base_w_caps[idx] * 0.5)
                    state.mint_limit = min(state.mint_limit, 0.25)
                    state.cr_target = max(state.cr_target, 1.25)
                if action["cb3"]:
                    state.optimizer_enabled = False
                    state.conservative_mode = True
                    state.oracle_degraded = True
                    state.mint_limit = min(state.mint_limit, 0.10)
                    state.cr_target = max(state.cr_target, 1.35)
                if action["cb2"]:
                    state.mint_limit = 0.0
                    state.mint_paused_reason = "MINT_PAUSED_BY_CB2"
                    state.cr_target = max(state.cr_target, 1.30)

            if state.optimizer_enabled and state.mint_limit > 0.0 and loss_finite:
                proposal = keeper.propose(state, optimizer, grad_w, grad_fee)
                result = keeper.submit_update_proposal(state, proposal)
                if result.get("status") == "APPLIED":
                    delta_mag = sum(abs(float(proposal["weights"][i]) - state.prev_weights[i]) for i in range(len(state.weights)))  # type: ignore[index]
                    invariant_monitor.record_agent_action("keeper", tick, magnitude=delta_mag, action_type="rebalance")

            peg = state.update_from_market(market.prices, market.oracle_q, peg_noise=float(spec.peg_noise))
            invariant_monitor.check(
                tick=tick,
                state=state,
                market=market,
                weights=state.weights,
                weight_caps=state.w_caps,
                min_cr=state.cr_min,
                oracle_stale_limit=120,
                max_actions_per_window=60,
                window_ticks=25,
            )
            turnover = sum(abs(a - b) for a, b in zip(state.weights, state.prev_weights))

            err = abs(peg - 1.0)
            peg_errors.append(err)
            sq_errors.append((peg - 1.0) ** 2)
            cr_values.append(state.cr)

            turnover_sum += turnover
            max_turnover = max(max_turnover, turnover)
            min_cr = min(min_cr, state.cr)
            if state.cr < state.cr_hard_min:
                cr_violations += 1

            if (not loss_finite) or (not finite_state(state)) or (not math.isfinite(peg)):
                nan_inf_detected = True

            if observer_fn is not None:
                observer_fn(
                    tick,
                    {
                        "state": state,
                        "market": market,
                        "action": action,
                        "new_events": new_events,
                        "peg": peg,
                        "peg_error": err,
                        "nav": nav_now,
                        "turnover": turnover,
                        "true_prices": spec.true_prices,
                    },
                )

            checkpoint_state = state.clone()
            checkpoint_lr = optimizer.lr

    except Exception as e:
        elapsed = time.perf_counter() - run_start
        return {
            "seed": seed,
            "ticks": ticks,
            "crash": True,
            "error": f"{type(e).__name__}: {e}",
            "traceback": traceback.format_exc(),
            "runtime_sec": elapsed,
            "hang_detected": hang_detected,
        }

    elapsed = time.perf_counter() - run_start

    if collect_mem_metrics:
        value_nodes_end = count_value_objects()
        memory_end = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss

    mae = sum(peg_errors) / len(peg_errors) if peg_errors else 0.0
    rmse = math.sqrt(sum(sq_errors) / len(sq_errors)) if sq_errors else 0.0
    cr_violation_rate = cr_violations / float(max(1, ticks))
    total_activations = sum(activation_counts.values())
    fp_rate = false_positives / float(max(1, total_activations))

    gate_run_pass = (mae < GATE_MAE_MAX) and (cr_violation_rate < GATE_CR_VIOL_MAX) and (fp_rate < GATE_FP_MAX)

    out = {
        "seed": seed,
        "ticks": ticks,
        "crash": False,
        "mae": mae,
        "rmse": rmse,
        "min_cr": min_cr,
        "cr_violation_rate": cr_violation_rate,
        "max_turnover": max_turnover,
        "total_turnover": turnover_sum,
        "breaker_activations": activation_counts,
        "breaker_false_positives": false_positives,
        "breaker_false_positive_rate": fp_rate,
        "gate_run_pass": gate_run_pass,
        "nan_inf_detected": nan_inf_detected,
        "hang_detected": hang_detected,
        "runtime_sec": elapsed,
        "peg_error_hist_source": peg_errors,
        "cr_values": cr_values,
    }

    if collect_mem_metrics:
        out["value_nodes_start"] = value_nodes_start
        out["value_nodes_end"] = value_nodes_end
        out["value_nodes_delta"] = (value_nodes_end - value_nodes_start) if (value_nodes_start is not None and value_nodes_end is not None) else None
        out["rss_start"] = memory_start
        out["rss_end"] = memory_end
        out["rss_delta"] = (memory_end - memory_start) if (memory_start is not None and memory_end is not None) else None

    return out


def run_death_spiral(seed: int) -> Dict[str, Any]:
    ticks = 120
    prices = [1.0, 1.0, 1.0, 1.0]

    max_drawdown = 0.0
    nav_peak = 1.0
    recovery_streak = 0
    recovery_tick: Optional[int] = None
    phase_end_tick = 9

    def tick_fn(t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal prices
        new_prices: List[float] = []

        if t <= phase_end_tick:
            frac = (t + 1) / float(phase_end_tick + 1)
            target = 1.0 - 0.20 * frac
            targets = [target, target, target, target]
        elif t <= 40:
            rec_frac = min(1.0, (t - phase_end_tick) / 20.0)
            targets = [
                0.80 + 0.20 * rec_frac,
                0.80 + 0.20 * rec_frac,
                0.82,
                0.82,
            ]
        else:
            targets = [1.0, 1.0, 0.82, 0.82]

        for i in range(4):
            p = 0.65 * prices[i] + 0.35 * targets[i] + rng.gauss(0.0, 0.002)
            new_prices.append(clamp(p, 0.5, 1.5))

        prices = new_prices
        q = clamp(0.995 + rng.gauss(0.0, 0.001), 0.0, 1.0)

        return TickSpec(prices=prices[:], oracle_q=q, stale_seconds=0, divergence=abs(rng.gauss(0.0, 0.0008)))

    def observer(t: int, obs: Dict[str, Any]) -> None:
        nonlocal max_drawdown, nav_peak, recovery_streak, recovery_tick
        nav_now = float(obs["nav"])
        nav_peak = max(nav_peak, nav_now)
        if nav_peak > 0:
            dd = (nav_peak - nav_now) / nav_peak
            max_drawdown = max(max_drawdown, dd)

        if t > phase_end_tick:
            if float(obs["peg_error"]) < 0.01:
                recovery_streak += 1
                if recovery_tick is None and recovery_streak >= 10:
                    recovery_tick = t
            else:
                recovery_streak = 0

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, observer_fn=observer)
    if run["crash"]:
        return run

    run["recovery_time_ticks"] = None if recovery_tick is None else (recovery_tick - (phase_end_tick + 1))
    run["irrecoverable"] = recovery_tick is None
    run["max_drawdown"] = max_drawdown
    run["cr_floor"] = run["min_cr"]
    return run


def run_flash_crash(seed: int) -> Dict[str, Any]:
    ticks = 90
    crash_tick = 20
    prices = [1.0, 1.0, 1.0, 1.0]

    cb1_activation_tick: Optional[int] = None
    max_peg_dev_during_crash = 0.0
    false_recovery_count = 0

    def tick_fn(t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal prices
        new_prices = prices[:]

        for i in range(4):
            target = 1.0
            new_prices[i] = clamp(0.85 * new_prices[i] + 0.15 * target + rng.gauss(0.0, 0.0015), 0.5, 1.5)

        if t == crash_tick:
            new_prices[1] = 0.50
        elif t == crash_tick + 1:
            new_prices[1] = 0.66
        elif t == crash_tick + 2:
            new_prices[1] = 0.82
        elif t == crash_tick + 3:
            new_prices[1] = 0.98

        prices = new_prices
        return TickSpec(
            prices=prices[:],
            oracle_q=clamp(0.995 + rng.gauss(0.0, 0.001), 0.0, 1.0),
            stale_seconds=0,
            divergence=abs(rng.gauss(0.0, 0.001)),
        )

    def observer(t: int, obs: Dict[str, Any]) -> None:
        nonlocal cb1_activation_tick, max_peg_dev_during_crash, false_recovery_count
        for ev in obs["new_events"]:
            if ev["event"] == "activate" and int(ev["cb"]) == 1 and cb1_activation_tick is None:
                cb1_activation_tick = int(ev["tick"])
            if ev["event"] == "recover" and int(ev["cb"]) == 1:
                p = float(obs["market"].prices[1])
                if abs(p - 1.0) > 0.02:
                    false_recovery_count += 1

        if crash_tick <= t <= crash_tick + 8:
            max_peg_dev_during_crash = max(max_peg_dev_during_crash, float(obs["peg_error"]))

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, observer_fn=observer)
    if run["crash"]:
        return run

    run["reaction_latency_ticks"] = None if cb1_activation_tick is None else (cb1_activation_tick - crash_tick)
    run["cb1_activated"] = cb1_activation_tick is not None
    run["max_peg_dev_during_crash"] = max_peg_dev_during_crash
    run["false_recovery_count"] = false_recovery_count
    return run


def run_oracle_manipulation(seed: int) -> Dict[str, Any]:
    ticks = 140
    true_prices = [1.0, 1.0, 1.0, 1.0]
    correction_tick = 70

    initial_weights = ms.INITIAL_WEIGHTS[:]
    max_portfolio_drift = 0.0
    loss_from_manipulation = 0.0

    def tick_fn(t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal true_prices

        # True market stays near peg
        next_true: List[float] = []
        for i in range(4):
            p = true_prices[i] + 0.15 * (1.0 - true_prices[i]) + rng.gauss(0.0, 0.001)
            next_true.append(clamp(p, 0.7, 1.3))

        true_prices = next_true

        # Oracle-reported prices drift 1 bps/tick for 50 ticks, then snap back
        if 20 <= t < correction_tick:
            drift = (t - 19) * 0.0001
            reported = [
                clamp(true_prices[0] + drift, 0.5, 1.5),
                clamp(true_prices[1] - drift, 0.5, 1.5),
                clamp(true_prices[2] + 0.5 * drift, 0.5, 1.5),
                clamp(true_prices[3] - 0.5 * drift, 0.5, 1.5),
            ]
        else:
            reported = true_prices[:]

        return TickSpec(
            prices=reported,
            true_prices=true_prices[:],
            oracle_q=clamp(0.998 + rng.gauss(0.0, 0.0005), 0.0, 1.0),
            stale_seconds=0,
            divergence=abs(rng.gauss(0.0, 0.0008)),
            expected_breakers=[],
        )

    def observer(t: int, obs: Dict[str, Any]) -> None:
        nonlocal max_portfolio_drift, loss_from_manipulation
        state: ms.ProtocolState = obs["state"]
        drift = sum(abs(w - iw) for w, iw in zip(state.weights, initial_weights))
        max_portfolio_drift = max(max_portfolio_drift, drift)

        if t == correction_tick and obs["true_prices"] is not None:
            tp = obs["true_prices"]
            baseline_true_nav = 0.0
            actual_true_nav = 0.0
            for i in range(4):
                h = ms.ProtocolState.haircut(state.risk_scores[i])
                baseline_true_nav += initial_weights[i] * tp[i] * (1.0 - h)
                actual_true_nav += state.weights[i] * tp[i] * (1.0 - h)
            loss_from_manipulation = max(0.0, baseline_true_nav - actual_true_nav)

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, observer_fn=observer)
    if run["crash"]:
        return run

    run["max_portfolio_drift_l1"] = max_portfolio_drift
    run["loss_from_manipulation"] = loss_from_manipulation
    return run


def run_high_volatility(seed: int) -> Dict[str, Any]:
    ticks = 200
    prices = [1.0, 1.0, 1.0, 1.0]

    def tick_fn(_t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal prices
        new_prices: List[float] = []
        for i in range(4):
            swing = rng.uniform(-0.05, 0.05)
            p = prices[i] * (1.0 + swing)
            p += 0.05 * (1.0 - p)
            new_prices.append(clamp(p, 0.5, 1.5))
        prices = new_prices

        return TickSpec(
            prices=prices[:],
            oracle_q=clamp(0.985 + rng.gauss(0.0, 0.002), 0.0, 1.0),
            stale_seconds=0,
            divergence=abs(rng.gauss(0.0, 0.0025)),
        )

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn)
    if run["crash"]:
        return run

    total_acts = sum(int(v) for v in run["breaker_activations"].values())
    run["cb_activation_frequency"] = total_acts / float(ticks)
    run["turnover_cost"] = run["total_turnover"]
    run["peg_stability_mae"] = run["mae"]
    return run


def run_cascading_breakers(seed: int) -> Dict[str, Any]:
    ticks = 130
    prices = [1.0, 1.0, 1.0, 1.0]

    activation_order: List[int] = []
    recovery_order: List[int] = []
    priority_violations = 0

    def tick_fn(t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal prices

        new_prices = [clamp(0.9 * p + 0.1 * 1.0 + rng.gauss(0.0, 0.001), 0.5, 1.5) for p in prices]

        # CB-1 seed: single depeg streak
        if 0 <= t <= 8:
            new_prices[0] = 0.95

        # CB-2 seed: multi-depeg during active CB-1/CB-3 window
        if 20 <= t <= 32:
            new_prices[1] = 0.90
            new_prices[2] = 0.89

        stale = 0
        div = abs(rng.gauss(0.0, 0.001))
        # CB-3 seed while CB-1 likely active
        if 12 <= t <= 30:
            stale = 180
            div = 0.05

        prices = new_prices
        return TickSpec(
            prices=prices[:],
            oracle_q=clamp(0.99 + rng.gauss(0.0, 0.0015), 0.0, 1.0),
            stale_seconds=stale,
            divergence=div,
        )

    def observer(_t: int, obs: Dict[str, Any]) -> None:
        nonlocal priority_violations
        action = obs["action"]
        state: ms.ProtocolState = obs["state"]
        for ev in obs["new_events"]:
            if ev["event"] == "activate":
                activation_order.append(int(ev["cb"]))
            elif ev["event"] == "recover":
                recovery_order.append(int(ev["cb"]))

        # Policy consistency checks reflecting CB priority effects
        if action["cb2"] and state.mint_limit != 0.0:
            priority_violations += 1
        if action["cb3"] and (not state.conservative_mode):
            priority_violations += 1

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, observer_fn=observer)
    if run["crash"]:
        return run

    # Verify cascade happened and recovered in priority order (CB-2 then CB-3 then CB-1)
    activation_seen = {1: False, 2: False, 3: False}
    for cb in activation_order:
        if cb in activation_seen:
            activation_seen[cb] = True

    act_pos = {cb: (activation_order.index(cb) if cb in activation_order else None) for cb in (1, 3, 2)}
    activation_chain_ok = (
        act_pos[1] is not None
        and act_pos[3] is not None
        and act_pos[2] is not None
        and act_pos[1] < act_pos[3] < act_pos[2]
    )

    rec_filtered = [cb for cb in recovery_order if cb in (1, 2, 3)]
    recovery_chain_ok = True
    target = [2, 3, 1]
    idx = 0
    for cb in rec_filtered:
        if idx < len(target) and cb == target[idx]:
            idx += 1
    if idx < len(target):
        recovery_chain_ok = False

    run["activation_order"] = activation_order
    run["recovery_order"] = recovery_order
    run["activation_chain_ok"] = activation_chain_ok
    run["recovery_chain_ok"] = recovery_chain_ok
    run["priority_violations"] = priority_violations
    run["cascade_ok"] = activation_chain_ok and recovery_chain_ok and (priority_violations == 0)
    return run


def run_adversarial_weights(seed: int) -> Dict[str, Any]:
    ticks = 200
    prices = [1.0, 1.0, 1.0, 1.0]

    ticks_to_balanced: Optional[int] = None
    max_concentration = 0.0
    final_concentration = 0.0

    def init_state(state: ms.ProtocolState) -> None:
        state.base_w_caps = [0.95, 0.95, 0.95, 0.95]
        state.w_caps = [0.95, 0.95, 0.95, 0.95]
        state.weights = [0.90, 0.05, 0.03, 0.02]
        state.prev_weights = state.weights[:]

    def tick_fn(t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal prices
        new_prices = prices[:]

        if t < 80:
            # adversary nudges optimizer toward concentration on asset 0
            new_prices[0] = clamp(0.92 * new_prices[0] + 0.08 * 1.03 + rng.gauss(0.0, 0.001), 0.5, 1.5)
            for i in (1, 2, 3):
                new_prices[i] = clamp(0.92 * new_prices[i] + 0.08 * 0.985 + rng.gauss(0.0, 0.001), 0.5, 1.5)
        else:
            for i in range(4):
                new_prices[i] = clamp(0.90 * new_prices[i] + 0.10 * 1.0 + rng.gauss(0.0, 0.001), 0.5, 1.5)

        prices = new_prices
        return TickSpec(
            prices=prices[:],
            oracle_q=clamp(0.996 + rng.gauss(0.0, 0.001), 0.0, 1.0),
            stale_seconds=0,
            divergence=abs(rng.gauss(0.0, 0.0012)),
        )

    def observer(t: int, obs: Dict[str, Any]) -> None:
        nonlocal ticks_to_balanced, max_concentration, final_concentration
        w = obs["state"].weights
        hhi = sum(x * x for x in w)
        max_concentration = max(max_concentration, hhi)
        final_concentration = hhi

        # Balanced criterion for this adversarial setup
        if ticks_to_balanced is None:
            if (max(w) <= 0.60) and (min(w) >= 0.08) and (hhi <= 0.42):
                ticks_to_balanced = t

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, init_state_fn=init_state, observer_fn=observer)
    if run["crash"]:
        return run

    run["ticks_to_balanced"] = ticks_to_balanced
    run["balanced_reached"] = ticks_to_balanced is not None
    run["max_concentration_hhi"] = max_concentration
    run["final_concentration_hhi"] = final_concentration
    return run


def run_zero_liquidity(seed: int) -> Dict[str, Any]:
    ticks = 90
    prices = [1.0, 1.0, 1.0, 1.0]

    min_supply = float("inf")
    max_supply = 0.0

    def init_state(state: ms.ProtocolState) -> None:
        state.supply = 1e-6
        state.reserve_value = state.cr * state.supply

    def tick_fn(t: int, rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        nonlocal prices
        prices = [clamp(0.92 * p + 0.08 * 1.0 + rng.gauss(0.0, 0.0015), 0.5, 1.5) for p in prices]

        if t < 10:
            supply = 1e-6
        elif t < 15:
            frac = (t - 9) / 5.0
            supply = 1e-6 + frac * (1_000_000.0 - 1e-6)
        else:
            supply = 1_000_000.0

        return TickSpec(
            prices=prices[:],
            oracle_q=clamp(0.995 + rng.gauss(0.0, 0.001), 0.0, 1.0),
            stale_seconds=0,
            divergence=abs(rng.gauss(0.0, 0.001)),
            supply=supply,
        )

    def observer(_t: int, obs: Dict[str, Any]) -> None:
        nonlocal min_supply, max_supply
        s = float(obs["state"].supply)
        min_supply = min(min_supply, s)
        max_supply = max(max_supply, s)

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, init_state_fn=init_state, observer_fn=observer)
    if run["crash"]:
        return run

    run["min_supply"] = min_supply
    run["max_supply"] = max_supply
    return run


def run_long_duration(seed: int) -> Dict[str, Any]:
    ticks = 10_000
    env = ms.MarketEnv(scenario="normal", seed=seed)

    def tick_fn(t: int, _rng: random.Random, _state: ms.ProtocolState) -> TickSpec:
        m = env.step(t)
        return TickSpec(
            prices=m.prices[:],
            oracle_q=m.oracle_q,
            stale_seconds=m.stale_seconds,
            divergence=m.divergence,
            expected_breakers=m.expected_breakers[:],
            peg_noise=env.rng.gauss(0.0, 0.00010),
        )

    run = simulate(seed=seed, ticks=ticks, tick_fn=tick_fn, collect_mem_metrics=True)
    if run["crash"]:
        return run

    cr_values = run.get("cr_values", [])
    cr_std = statistics.pstdev(cr_values) if cr_values else 0.0

    run["final_peg_mae"] = run["mae"]
    run["cr_stability_std"] = cr_std
    run["total_turnover_long"] = run["total_turnover"]
    return run


SCENARIOS: List[Tuple[str, Callable[[int], Dict[str, Any]], List[str], Dict[str, bool]]] = [
    (
        "A_death_spiral",
        run_death_spiral,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "recovery_time_ticks", "max_drawdown", "cr_floor"],
        {"cr_floor": True},
    ),
    (
        "B_flash_crash_recovery",
        run_flash_crash,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "reaction_latency_ticks", "max_peg_dev_during_crash", "false_recovery_count"],
        {},
    ),
    (
        "C_oracle_manipulation",
        run_oracle_manipulation,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "max_portfolio_drift_l1", "loss_from_manipulation"],
        {},
    ),
    (
        "D_sustained_high_volatility",
        run_high_volatility,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "turnover_cost", "cb_activation_frequency"],
        {},
    ),
    (
        "E_cascading_circuit_breakers",
        run_cascading_breakers,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "priority_violations"],
        {},
    ),
    (
        "F_adversarial_weight_manipulation",
        run_adversarial_weights,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "ticks_to_balanced", "max_concentration_hhi", "final_concentration_hhi"],
        {},
    ),
    (
        "G_zero_liquidity",
        run_zero_liquidity,
        ["mae", "cr_violation_rate", "breaker_false_positive_rate", "min_supply", "max_supply"],
        {"min_supply": True},
    ),
    (
        "H_long_duration_stability",
        run_long_duration,
        [
            "mae",
            "cr_violation_rate",
            "breaker_false_positive_rate",
            "final_peg_mae",
            "cr_stability_std",
            "total_turnover_long",
            "runtime_sec",
            "value_nodes_delta",
            "rss_delta",
        ],
        {},
    ),
]


def aggregate_scenario(name: str, runs: List[Dict[str, Any]], key_metrics: List[str], lower_is_worse: Dict[str, bool]) -> Dict[str, Any]:
    crashed = [r for r in runs if r.get("crash")]
    ok_runs = [r for r in runs if not r.get("crash")]

    mae_vals = [r["mae"] for r in ok_runs]
    cr_vals = [r["cr_violation_rate"] for r in ok_runs]
    fp_vals = [r["breaker_false_positive_rate"] for r in ok_runs]

    mean_mae = (sum(mae_vals) / len(mae_vals)) if mae_vals else float("inf")
    mean_cr = (sum(cr_vals) / len(cr_vals)) if cr_vals else float("inf")
    mean_fp = (sum(fp_vals) / len(fp_vals)) if fp_vals else float("inf")

    gate_mean_pass = (mean_mae < GATE_MAE_MAX) and (mean_cr < GATE_CR_VIOL_MAX) and (mean_fp < GATE_FP_MAX)
    run_pass_rate = (
        sum(1 for r in ok_runs if r.get("gate_run_pass", False)) / float(max(1, len(ok_runs)))
        if ok_runs
        else 0.0
    )

    nan_inf_runs = sum(1 for r in ok_runs if r.get("nan_inf_detected", False))
    hang_runs = sum(1 for r in ok_runs if r.get("hang_detected", False))

    pass_fail = gate_mean_pass and (len(crashed) == 0) and (nan_inf_runs == 0) and (hang_runs == 0)

    stats: Dict[str, Dict[str, float]] = {}
    for m in key_metrics:
        vals = [r[m] for r in ok_runs if (m in r and r[m] is not None and isinstance(r[m], (int, float)))]
        stats[m] = summary_stats(vals, lower_is_worse=lower_is_worse.get(m, False))

    # Peg error distribution histogram source (all ticks across all runs)
    peg_errors: List[float] = []
    for r in ok_runs:
        peg_errors.extend(r.get("peg_error_hist_source", []))

    histogram = ms.text_histogram(peg_errors, bins=14, width=34) if peg_errors else "(empty)"

    return {
        "scenario": name,
        "pass": pass_fail,
        "gate": {
            "mean_mae": mean_mae,
            "mean_cr_violation_rate": mean_cr,
            "mean_breaker_false_positive_rate": mean_fp,
            "criteria": {
                "mae_lt": GATE_MAE_MAX,
                "cr_violation_lt": GATE_CR_VIOL_MAX,
                "breaker_fp_lt": GATE_FP_MAX,
            },
            "gate_mean_pass": gate_mean_pass,
            "run_pass_rate": run_pass_rate,
        },
        "counts": {
            "runs_total": len(runs),
            "runs_ok": len(ok_runs),
            "runs_crashed": len(crashed),
            "runs_nan_inf": nan_inf_runs,
            "runs_hang": hang_runs,
        },
        "stats": stats,
        "histogram_peg_error": histogram,
        "crashes": [
            {
                "seed": r.get("seed"),
                "error": r.get("error"),
                "traceback": r.get("traceback"),
            }
            for r in crashed
        ],
        "runs": [
            {
                k: v
                for k, v in r.items()
                if k not in ("peg_error_hist_source", "cr_values", "traceback")
            }
            for r in runs
        ],
    }


def render_report(result: Dict[str, Any]) -> str:
    lines: List[str] = []
    lines.append("# Microstable Extreme Stress Test Report")
    lines.append("")
    lines.append(f"Generated: {result['generated_at']}")
    lines.append(f"Runs per scenario: {result['config']['runs_per_scenario']}")
    lines.append("")
    lines.append("## Gate A (relaxed) criteria")
    lines.append("- MAE < 0.01")
    lines.append("- CR violation rate < 10%")
    lines.append("- Breaker false-positive rate < 10%")
    lines.append("")

    overall_pass = all(sc["pass"] for sc in result["scenarios"])
    lines.append(f"## Overall: {'PASS' if overall_pass else 'FAIL'}")
    lines.append("")

    for sc in result["scenarios"]:
        lines.append(f"## {sc['scenario']}: {'PASS' if sc['pass'] else 'FAIL'}")
        gate = sc["gate"]
        cnt = sc["counts"]
        lines.append(
            f"- Gate means: MAE={gate['mean_mae']:.6f}, CR_violation={gate['mean_cr_violation_rate']:.4f}, "
            f"Breaker_FP={gate['mean_breaker_false_positive_rate']:.4f}"
        )
        lines.append(f"- Run pass rate: {gate['run_pass_rate']*100:.1f}%")
        lines.append(
            f"- Runs: total={cnt['runs_total']}, ok={cnt['runs_ok']}, crashed={cnt['runs_crashed']}, "
            f"NaN/Inf={cnt['runs_nan_inf']}, hangs={cnt['runs_hang']}"
        )
        lines.append("")
        lines.append("### Statistical summary")
        lines.append("| Metric | mean | median | p5 | p95 | worst |")
        lines.append("|---|---:|---:|---:|---:|---:|")
        for m, s in sc["stats"].items():
            lines.append(
                f"| {m} | {s['mean']:.6f} | {s['median']:.6f} | {s['p5']:.6f} | {s['p95']:.6f} | {s['worst']:.6f} |"
            )
        lines.append("")

        if sc["counts"]["runs_crashed"] > 0:
            lines.append("### Crashes")
            for c in sc["crashes"][:5]:
                lines.append(f"- seed={c['seed']}: {c['error']}")
            if len(sc["crashes"]) > 5:
                lines.append(f"- ... {len(sc['crashes'])-5} more")
            lines.append("")

        lines.append("### Peg deviation histogram (|peg-1|)")
        lines.append("```")
        lines.append(sc["histogram_peg_error"])
        lines.append("```")
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def main() -> None:
    os.makedirs(BASE_OUTPUT_DIR, exist_ok=True)

    seed_rng = random.Random(20260222)
    scenario_outputs: List[Dict[str, Any]] = []

    global_start = time.perf_counter()

    print("microstable extreme stress test")
    print(f"runs/scenario={RUNS_PER_SCENARIO}")

    for scenario_name, runner, key_metrics, lower_is_worse in SCENARIOS:
        print(f"\n[{scenario_name}] running {RUNS_PER_SCENARIO} Monte Carlo runs...")
        runs: List[Dict[str, Any]] = []

        for i in range(RUNS_PER_SCENARIO):
            seed = seed_rng.randint(1, 2**31 - 1)
            run = runner(seed)
            runs.append(run)
            if (i + 1) % 10 == 0:
                ok_count = sum(1 for r in runs if not r.get("crash"))
                print(f"  progress {i+1:3d}/{RUNS_PER_SCENARIO} (ok={ok_count})")

        aggregated = aggregate_scenario(scenario_name, runs, key_metrics, lower_is_worse)
        scenario_outputs.append(aggregated)
        print(
            f"  -> {'PASS' if aggregated['pass'] else 'FAIL'} "
            f"(MAE={aggregated['gate']['mean_mae']:.5f}, "
            f"CRv={aggregated['gate']['mean_cr_violation_rate']:.4f}, "
            f"FP={aggregated['gate']['mean_breaker_false_positive_rate']:.4f})"
        )

    total_runtime = time.perf_counter() - global_start

    result = {
        "generated_at": datetime.now().isoformat(),
        "config": {
            "runs_per_scenario": RUNS_PER_SCENARIO,
            "gate_relaxed": {
                "mae_lt": GATE_MAE_MAX,
                "cr_violation_lt": GATE_CR_VIOL_MAX,
                "breaker_fp_lt": GATE_FP_MAX,
            },
            "runtime_sec": total_runtime,
        },
        "scenarios": scenario_outputs,
    }

    with open(RAW_JSON_PATH, "w", encoding="utf-8") as f:
        json.dump(result, f, ensure_ascii=False, indent=2)

    report = render_report(result)
    with open(REPORT_MD_PATH, "w", encoding="utf-8") as f:
        f.write(report)

    print("\n=== completed ===")
    print(f"runtime_sec={total_runtime:.2f}")
    print(f"raw_json={RAW_JSON_PATH}")
    print(f"report_md={REPORT_MD_PATH}")


if __name__ == "__main__":
    main()
