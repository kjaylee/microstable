#!/usr/bin/env python3
"""
mega_stress_test.py

Microstable mega stress harness:
- 80 extreme scenarios
- Monte Carlo runs per scenario (target 100, auto-fallback to 50 if projected runtime > 30 min)
- Per-run exception isolation (harness never crashes)
- Multiprocessing support

Outputs:
- outputs/mega-stress-results.json
- outputs/mega-stress-report.md
"""

from __future__ import annotations

import argparse
import json
import math
import os
import random
import time
import traceback
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Dict, List, Optional, Sequence, Tuple

import microstable as ms


# -----------------------------------------------------------------------------
# Config
# -----------------------------------------------------------------------------

GATE_MAE_MAX = 0.05
GATE_CR_VIOL_MAX = 0.20

OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "outputs")
RESULTS_JSON_PATH = os.path.join(OUTPUT_DIR, "mega-stress-results.json")
REPORT_MD_PATH = os.path.join(OUTPUT_DIR, "mega-stress-report.md")

DEFAULT_REQUESTED_RUNS = 100
FALLBACK_RUNS = 50
TARGET_RUNTIME_SEC = 30 * 60


# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------


def clamp(x: float, lo: float, hi: float) -> float:
    return min(hi, max(lo, x))


def mean(values: Sequence[float]) -> float:
    return sum(values) / float(len(values)) if values else 0.0


def calc_expected_breakers(prices: Sequence[float], stale_seconds: int, divergence: float, forced: Optional[Dict[str, bool]] = None) -> List[int]:
    out: List[int] = []
    depeg_count = sum(1 for p in prices if abs(p - 1.0) > 0.02)
    if depeg_count >= 1:
        out.append(1)
    if depeg_count >= 2:
        out.append(2)
    if stale_seconds > 120 or divergence > 0.02:
        out.append(3)
    if forced:
        for i in (1, 2, 3, 4):
            if forced.get(f"cb{i}", False):
                out.append(i)
    return sorted(set(out))


def finite_state(state: ms.ProtocolState) -> bool:
    vals = [
        state.cr,
        state.mint_fee,
        state.reserve_value,
        state.supply,
        state.cr_target,
        state.cr_min,
        state.cr_hard_min,
    ] + state.weights + state.w_caps
    return all(math.isfinite(v) for v in vals)


def evolve_prices(
    prices: Sequence[float],
    targets: Sequence[float],
    vol: float,
    rng: random.Random,
    reversion: float = 0.22,
    lo: float = 0.0001,
    hi: float = 1.5,
) -> List[float]:
    out: List[float] = []
    for p, t in zip(prices, targets):
        step = p + reversion * (t - p) + rng.gauss(0.0, vol)
        out.append(clamp(step, lo, hi))
    return out


def percentile(values: Sequence[float], p: float) -> float:
    if not values:
        return 0.0
    s = sorted(values)
    if p <= 0:
        return s[0]
    if p >= 100:
        return s[-1]
    k = (len(s) - 1) * (p / 100.0)
    lo = int(math.floor(k))
    hi = int(math.ceil(k))
    if lo == hi:
        return s[lo]
    frac = k - lo
    return s[lo] * (1.0 - frac) + s[hi] * frac


# -----------------------------------------------------------------------------
# Scenario specs
# -----------------------------------------------------------------------------


@dataclass(frozen=True)
class ScenarioSpec:
    sid: int
    name: str
    category: str
    ticks: int


SCENARIOS: List[ScenarioSpec] = [
    # Cat 1: Depeg Variations
    ScenarioSpec(1, "single_usdc_depeg_095_mild", "Depeg Variations", 180),
    ScenarioSpec(2, "single_usdc_depeg_080_severe", "Depeg Variations", 180),
    ScenarioSpec(3, "single_usdt_depeg_050_extreme", "Depeg Variations", 180),
    ScenarioSpec(4, "dai_wobble_097_103", "Depeg Variations", 220),
    ScenarioSpec(5, "usds_drop_070_instant_recovery", "Depeg Variations", 150),
    ScenarioSpec(6, "sequential_depegs_usdc_usdt_dai", "Depeg Variations", 220),
    ScenarioSpec(7, "all_depeg_090_simultaneous", "Depeg Variations", 180),
    ScenarioSpec(8, "all_depeg_060_catastrophic", "Depeg Variations", 180),
    ScenarioSpec(9, "inverse_depeg_premium_120", "Depeg Variations", 180),
    ScenarioSpec(10, "slow_bleed_01pct_500ticks", "Depeg Variations", 560),
    # Cat 2: Volatility Regimes
    ScenarioSpec(11, "low_spike_low_vol", "Volatility Regimes", 220),
    ScenarioSpec(12, "increasing_vol_ramp", "Volatility Regimes", 220),
    ScenarioSpec(13, "decreasing_vol_ramp", "Volatility Regimes", 220),
    ScenarioSpec(14, "bimodal_vol", "Volatility Regimes", 220),
    ScenarioSpec(15, "correlated_vol_all_assets", "Volatility Regimes", 220),
    ScenarioSpec(16, "anti_correlated_usdc_usdt", "Volatility Regimes", 220),
    ScenarioSpec(17, "fat_tail_cauchy_jumps", "Volatility Regimes", 220),
    ScenarioSpec(18, "microstructure_jitter", "Volatility Regimes", 220),
    ScenarioSpec(19, "weekend_effect", "Volatility Regimes", 280),
    ScenarioSpec(20, "vol_clustering_garch_like", "Volatility Regimes", 220),
    # Cat 3: Oracle Attacks
    ScenarioSpec(21, "oracle_stale_20ticks", "Oracle Attacks", 180),
    ScenarioSpec(22, "oracle_stale_100ticks", "Oracle Attacks", 260),
    ScenarioSpec(23, "oracle_drift_plus1bps_200", "Oracle Attacks", 280),
    ScenarioSpec(24, "oracle_drift_minus5bps_50", "Oracle Attacks", 180),
    ScenarioSpec(25, "oracle_noise_injection_2pct", "Oracle Attacks", 180),
    ScenarioSpec(26, "oracle_front_running_1tick_lead", "Oracle Attacks", 180),
    ScenarioSpec(27, "oracle_sandwich_high_low", "Oracle Attacks", 200),
    ScenarioSpec(28, "multi_oracle_disagreement", "Oracle Attacks", 180),
    ScenarioSpec(29, "oracle_outage_50_then_recover", "Oracle Attacks", 220),
    ScenarioSpec(30, "oracle_intermittent_every_3rd", "Oracle Attacks", 220),
    # Cat 4: Circuit Breaker Stress
    ScenarioSpec(31, "cb1_rapid_toggle_attempt", "Circuit Breaker Stress", 200),
    ScenarioSpec(32, "all_cbs_simultaneous", "Circuit Breaker Stress", 200),
    ScenarioSpec(33, "cb_cascade_1_3_2_4", "Circuit Breaker Stress", 200),
    ScenarioSpec(34, "cb_extended_mode_trigger", "Circuit Breaker Stress", 200),
    ScenarioSpec(35, "cb_recovery_race", "Circuit Breaker Stress", 200),
    ScenarioSpec(36, "cb_during_rebalance", "Circuit Breaker Stress", 200),
    ScenarioSpec(37, "cb_with_max_weight_concentration", "Circuit Breaker Stress", 200),
    ScenarioSpec(38, "cb_with_min_weight_collateral", "Circuit Breaker Stress", 200),
    ScenarioSpec(39, "cb_cooldown_boundary", "Circuit Breaker Stress", 240),
    ScenarioSpec(40, "cb_hysteresis_edge_threshold", "Circuit Breaker Stress", 200),
    # Cat 5: Optimizer Adversarial
    ScenarioSpec(41, "gradient_explosion", "Optimizer Adversarial", 220),
    ScenarioSpec(42, "gradient_vanishing", "Optimizer Adversarial", 220),
    ScenarioSpec(43, "saddle_point_zero_grad", "Optimizer Adversarial", 220),
    ScenarioSpec(44, "oscillating_loss_direction", "Optimizer Adversarial", 220),
    ScenarioSpec(45, "adversarial_gradient_direction", "Optimizer Adversarial", 220),
    ScenarioSpec(46, "learning_rate_high_005", "Optimizer Adversarial", 220),
    ScenarioSpec(47, "learning_rate_low_0001", "Optimizer Adversarial", 220),
    ScenarioSpec(48, "weight_stuck_at_boundary", "Optimizer Adversarial", 220),
    ScenarioSpec(49, "simplex_projection_stress", "Optimizer Adversarial", 220),
    ScenarioSpec(50, "adam_momentum_trap_beta1_099", "Optimizer Adversarial", 220),
    # Cat 6: Liquidity & Supply
    ScenarioSpec(51, "supply_near_zero", "Liquidity & Supply", 220),
    ScenarioSpec(52, "supply_huge_1b", "Liquidity & Supply", 220),
    ScenarioSpec(53, "rapid_mint_0_to_1m", "Liquidity & Supply", 220),
    ScenarioSpec(54, "rapid_redeem_1m_to_0", "Liquidity & Supply", 220),
    ScenarioSpec(55, "oscillating_supply_every_tick", "Liquidity & Supply", 220),
    ScenarioSpec(56, "one_sided_mints_200", "Liquidity & Supply", 260),
    ScenarioSpec(57, "one_sided_redeems_200", "Liquidity & Supply", 260),
    ScenarioSpec(58, "supply_spike_then_plateau", "Liquidity & Supply", 220),
    ScenarioSpec(59, "supply_growth_1pct_per_tick", "Liquidity & Supply", 260),
    ScenarioSpec(60, "supply_crash_90pct_3ticks", "Liquidity & Supply", 220),
    # Cat 7: Multi-Factor Stress
    ScenarioSpec(61, "depeg_plus_oracle_failure", "Multi-Factor Stress", 240),
    ScenarioSpec(62, "high_vol_plus_low_liquidity", "Multi-Factor Stress", 240),
    ScenarioSpec(63, "flash_crash_cb_cascade_oracle_stale", "Multi-Factor Stress", 240),
    ScenarioSpec(64, "triple_depeg_plus_gradient_attack", "Multi-Factor Stress", 240),
    ScenarioSpec(65, "max_concentration_then_depeg", "Multi-Factor Stress", 240),
    ScenarioSpec(66, "all_mild_compound_stress", "Multi-Factor Stress", 240),
    ScenarioSpec(67, "recovery_from_max_stress", "Multi-Factor Stress", 260),
    ScenarioSpec(68, "normal_crisis_oscillation_20ticks", "Multi-Factor Stress", 260),
    ScenarioSpec(69, "gradual_degradation_1000ticks", "Multi-Factor Stress", 1100),
    ScenarioSpec(70, "sudden_improvement_after_500", "Multi-Factor Stress", 760),
    # Cat 8: Edge Cases & Boundary
    ScenarioSpec(71, "perfect_peg_500ticks", "Edge Cases & Boundary", 500),
    ScenarioSpec(72, "equal_weights_025", "Edge Cases & Boundary", 220),
    ScenarioSpec(73, "single_weight_at_max_cap", "Edge Cases & Boundary", 220),
    ScenarioSpec(74, "cr_exactly_target", "Edge Cases & Boundary", 220),
    ScenarioSpec(75, "fee_zero", "Edge Cases & Boundary", 220),
    ScenarioSpec(76, "fee_max_10pct", "Edge Cases & Boundary", 220),
    ScenarioSpec(77, "price_to_00001", "Edge Cases & Boundary", 220),
    ScenarioSpec(78, "very_long_50000ticks", "Edge Cases & Boundary", 50000),
    ScenarioSpec(79, "oracle_confidence_0_1_alternating", "Edge Cases & Boundary", 220),
    ScenarioSpec(80, "all_params_extreme_boundary", "Edge Cases & Boundary", 260),
]

SCENARIO_MAP: Dict[int, ScenarioSpec] = {s.sid: s for s in SCENARIOS}


# -----------------------------------------------------------------------------
# Tick spec and scenario engine
# -----------------------------------------------------------------------------


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


def init_state_for_scenario(sid: int, state: ms.ProtocolState, opt_cfg: Dict[str, float]) -> None:
    if sid in (37, 65):
        state.base_w_caps = [0.95, 0.95, 0.95, 0.95]
        state.w_caps = [0.95, 0.95, 0.95, 0.95]
        state.weights = [0.90, 0.05, 0.03, 0.02]
        state.prev_weights = state.weights[:]

    if sid == 38:
        state.weights = [0.50, 0.30, 0.199, 0.001]
        state.prev_weights = state.weights[:]

    if sid == 48:
        state.base_w_caps = [0.85, 0.85, 0.85, 0.85]
        state.w_caps = [0.85, 0.85, 0.85, 0.85]
        state.weights = [0.85, 0.10, 0.03, 0.02]
        state.prev_weights = state.weights[:]

    if sid == 72:
        state.weights = [0.25, 0.25, 0.25, 0.25]
        state.prev_weights = state.weights[:]

    if sid == 73:
        state.weights = [0.55, 0.30, 0.10, 0.05]
        state.prev_weights = state.weights[:]

    if sid == 74:
        state.cr = state.cr_target

    if sid == 75:
        state.mint_fee = 0.0

    if sid == 76:
        state.mint_fee = 0.10

    if sid == 51:
        state.supply = 0.001
        state.reserve_value = state.cr * state.supply

    if sid == 52:
        state.supply = 1_000_000_000.0
        state.reserve_value = state.cr * state.supply

    if sid == 53:
        state.supply = 0.0
        state.reserve_value = state.cr * state.supply

    if sid == 54:
        state.supply = 1_000_000.0
        state.reserve_value = state.cr * state.supply

    if sid == 67:
        state.base_w_caps = [0.95, 0.95, 0.95, 0.95]
        state.w_caps = [0.95, 0.95, 0.95, 0.95]
        state.weights = [0.80, 0.10, 0.05, 0.05]
        state.prev_weights = state.weights[:]
        state.supply = 200_000.0
        state.reserve_value = state.cr * state.supply

    if sid == 80:
        state.base_w_caps = [1.0, 1.0, 1.0, 1.0]
        state.w_caps = [1.0, 1.0, 1.0, 1.0]
        state.weights = [0.97, 0.02, 0.01, 0.00]
        state.prev_weights = state.weights[:]
        state.mint_fee = 0.10
        state.supply = 0.001
        state.cr_target = 1.50
        state.cr_min = 1.50
        state.cr_hard_min = 1.20
        state.cr = 1.50
        state.reserve_value = state.cr * state.supply

    if sid == 46:
        opt_cfg["lr"] = 0.05
    if sid == 47:
        opt_cfg["lr"] = 0.0001
    if sid == 50:
        opt_cfg["beta1"] = 0.99


def scenario_tick_spec(sid: int, tick: int, ticks: int, rng: random.Random, state: ms.ProtocolState, ctx: Dict[str, Any]) -> TickSpec:
    prices = ctx.get("prices", [1.0, 1.0, 1.0, 1.0])
    targets = [1.0, 1.0, 1.0, 1.0]
    oracle_q = 0.997 + rng.gauss(0.0, 0.001)
    stale_seconds = 0
    divergence = abs(rng.gauss(0.0, 0.0012))
    forced: Dict[str, bool] = {}
    supply: Optional[float] = None
    peg_noise = rng.gauss(0.0, 0.0002)

    # ------------------------------------------------------------------
    # Cat 1: Depeg Variations
    # ------------------------------------------------------------------
    if 1 <= sid <= 10:
        vol = 0.001
        if sid == 1 and 20 <= tick <= 90:
            targets[0] = 0.95
        elif sid == 2 and 20 <= tick <= 90:
            targets[0] = 0.80
        elif sid == 3 and 20 <= tick <= 90:
            targets[1] = 0.50
        elif sid == 4:
            targets[2] = 1.0 + 0.03 * math.sin(2.0 * math.pi * tick / 12.0)
        elif sid == 5:
            if tick == 50:
                prices = prices[:]
                prices[3] = 0.70
            elif tick == 51:
                prices = prices[:]
                prices[3] = 1.00
        elif sid == 6:
            if 20 <= tick < 30:
                targets[0] = 0.82
            if 30 <= tick < 40:
                targets[1] = 0.82
            if 40 <= tick < 50:
                targets[2] = 0.82
        elif sid == 7 and 25 <= tick <= 90:
            targets = [0.90, 0.90, 0.90, 0.90]
        elif sid == 8 and 25 <= tick <= 100:
            targets = [0.60, 0.60, 0.60, 0.60]
        elif sid == 9 and 25 <= tick <= 90:
            targets[0] = 1.20
        elif sid == 10:
            bleed = max(0.0, 1.0 - 0.001 * min(tick, 500))
            targets[0] = bleed

        prices = evolve_prices(prices, targets, vol, rng, reversion=0.25)

    # ------------------------------------------------------------------
    # Cat 2: Volatility Regimes
    # ------------------------------------------------------------------
    elif 11 <= sid <= 20:
        if sid == 11:
            vol = 0.0004 if tick < 60 or tick > 120 else 0.02
            prices = evolve_prices(prices, targets, vol, rng, reversion=0.18)
        elif sid == 12:
            vol = 0.001 + (0.10 - 0.001) * (tick / float(max(1, ticks - 1)))
            prices = evolve_prices(prices, targets, vol, rng, reversion=0.15)
        elif sid == 13:
            vol = 0.10 - (0.10 - 0.001) * (tick / float(max(1, ticks - 1)))
            prices = evolve_prices(prices, targets, vol, rng, reversion=0.20)
        elif sid == 14:
            vol = 0.0005 if (tick // 12) % 2 == 0 else 0.03
            prices = evolve_prices(prices, targets, vol, rng, reversion=0.15)
        elif sid == 15:
            common = rng.gauss(0.0, 0.02)
            prices = [clamp(p + common + rng.gauss(0.0, 0.001), 0.0001, 1.5) for p in prices]
            prices = evolve_prices(prices, targets, 0.0005, rng, reversion=0.08)
        elif sid == 16:
            s = rng.gauss(0.0, 0.03)
            prices = prices[:]
            prices[0] = clamp(prices[0] + s, 0.0001, 1.5)
            prices[1] = clamp(prices[1] - s, 0.0001, 1.5)
            prices[2] = clamp(prices[2] + rng.gauss(0.0, 0.005), 0.0001, 1.5)
            prices[3] = clamp(prices[3] + rng.gauss(0.0, 0.005), 0.0001, 1.5)
            prices = evolve_prices(prices, targets, 0.0008, rng, reversion=0.10)
        elif sid == 17:
            prices = prices[:]
            for i in range(4):
                u = clamp(rng.random(), 1e-9, 1.0 - 1e-9)
                jump = 0.01 * math.tan(math.pi * (u - 0.5))
                jump = clamp(jump, -0.12, 0.12)
                prices[i] = clamp(prices[i] + jump, 0.0001, 1.5)
            prices = evolve_prices(prices, targets, 0.0008, rng, reversion=0.10)
        elif sid == 18:
            prices = [clamp(p + rng.uniform(-0.0001, 0.0001), 0.0001, 1.5) for p in prices]
            prices = evolve_prices(prices, targets, 0.00005, rng, reversion=0.40)
        elif sid == 19:
            day = tick % 7
            vol = 0.0005 if day in (5, 6) else (0.03 if day == 0 else 0.002)
            prices = evolve_prices(prices, targets, vol, rng, reversion=0.16)
        elif sid == 20:
            vol_prev = float(ctx.get("vol", 0.003))
            last_ret = float(ctx.get("ret", 0.0))
            omega, alpha, beta = 1e-6, 0.15, 0.80
            vol2 = omega + alpha * (last_ret * last_ret) + beta * (vol_prev * vol_prev)
            vol = clamp(math.sqrt(max(vol2, 1e-10)), 0.0003, 0.08)
            new_prices = evolve_prices(prices, targets, vol, rng, reversion=0.14)
            ret = sum(abs(new_prices[i] - prices[i]) for i in range(4)) / 4.0
            ctx["vol"] = vol
            ctx["ret"] = ret
            prices = new_prices

    # ------------------------------------------------------------------
    # Cat 3: Oracle Attacks
    # ------------------------------------------------------------------
    elif 21 <= sid <= 30:
        true_prices = ctx.get("true_prices", [1.0, 1.0, 1.0, 1.0])
        true_prices = evolve_prices(true_prices, [1.0, 1.0, 1.0, 1.0], 0.002, rng, reversion=0.20)
        reported = true_prices[:]

        if sid == 21 and 30 <= tick < 50:
            if tick == 30:
                ctx["frozen"] = reported[:]
            reported = ctx.get("frozen", reported)[:]
            stale_seconds = 180
            divergence = 0.03
        elif sid == 22 and 20 <= tick < 120:
            if tick == 20:
                ctx["frozen"] = reported[:]
            reported = ctx.get("frozen", reported)[:]
            stale_seconds = 180
            divergence = 0.04
        elif sid == 23 and 20 <= tick < 220:
            drift = (tick - 19) * 0.0001
            reported = [clamp(tp + drift, 0.0001, 1.5) for tp in true_prices]
            divergence = max(divergence, abs(drift) * 1.8)
        elif sid == 24 and 20 <= tick < 70:
            drift = -(tick - 19) * 0.0005
            reported = [clamp(tp + drift, 0.0001, 1.5) for tp in true_prices]
            divergence = max(divergence, abs(drift) * 1.6)
        elif sid == 25:
            reported = [clamp(tp * (1.0 + rng.uniform(-0.02, 0.02)), 0.0001, 1.5) for tp in true_prices]
            divergence = max(divergence, 0.02)
        elif sid == 26:
            prev_true = ctx.get("prev_true", true_prices)
            reported = [clamp(tp + (tp - pt), 0.0001, 1.5) for tp, pt in zip(true_prices, prev_true)]
            divergence = max(divergence, 0.015)
        elif sid == 27:
            phase = tick % 6
            if phase in (0, 1):
                reported = [clamp(tp * 1.03, 0.0001, 1.5) for tp in true_prices]
            elif phase in (2, 3):
                reported = [clamp(tp * 0.97, 0.0001, 1.5) for tp in true_prices]
            divergence = max(divergence, 0.03)
        elif sid == 28:
            reported = [clamp(tp * 1.02, 0.0001, 1.5) for tp in true_prices]
            divergence = 0.05
        elif sid == 29 and 40 <= tick < 90:
            if tick == 40:
                ctx["frozen"] = reported[:]
            reported = ctx.get("frozen", reported)[:]
            stale_seconds = 180
            divergence = 0.06
        elif sid == 30:
            if tick % 3 != 0:
                if "last_reported" in ctx:
                    reported = ctx["last_reported"][:]
                stale_seconds = 150
                divergence = max(divergence, 0.025)

        ctx["prev_true"] = true_prices[:]
        ctx["true_prices"] = true_prices[:]
        ctx["last_reported"] = reported[:]
        prices = reported[:]

        oracle_q = clamp(1.0 - min(0.8, stale_seconds / 500.0) - min(0.6, divergence / 0.08) + rng.gauss(0.0, 0.002), 0.0, 1.0)

    # ------------------------------------------------------------------
    # Cat 4: Circuit Breaker Stress
    # ------------------------------------------------------------------
    elif 31 <= sid <= 40:
        prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.20)

        if sid == 31:
            phase = tick % 10
            if phase in (0, 1, 2):
                prices[0] = 0.95
        elif sid == 32:
            if tick == 20:
                forced = {"cb1": True, "cb2": True, "cb3": True, "cb4": True}
                prices = [0.80, 0.80, 0.80, 0.80]
                stale_seconds = 180
                divergence = 0.05
        elif sid == 33:
            if tick == 10:
                forced["cb1"] = True
            if tick == 20:
                forced["cb3"] = True
                stale_seconds = 180
                divergence = 0.05
            if tick == 30:
                forced["cb2"] = True
                prices = [0.88, 0.89, 1.00, 1.00]
            if tick == 40:
                forced["cb4"] = True
        elif sid == 34:
            if tick in (5, 20, 35):
                forced["cb1"] = True
                prices[0] = 0.95
        elif sid == 35:
            if tick == 10:
                forced["cb1"] = True
                forced["cb3"] = True
                stale_seconds = 180
                divergence = 0.05
            if tick > 25:
                prices = [1.0, 1.0, 1.0, 1.0]
                stale_seconds = 0
                divergence = 0.0
        elif sid == 36:
            prices = evolve_prices(prices, targets, 0.012, rng, reversion=0.12)
            if tick == 30:
                forced["cb2"] = True
                prices[0] = 0.85
                prices[1] = 0.88
        elif sid == 37:
            if 15 <= tick <= 60:
                prices[0] = clamp(prices[0] - 0.03, 0.0001, 1.5)
        elif sid == 38:
            if 15 <= tick <= 50:
                prices[3] = 0.80
        elif sid == 39:
            if tick in (5, 35):
                forced["cb1"] = True
                prices[0] = 0.95
        elif sid == 40:
            prices[0] = 0.98  # exact threshold edge (strict > 0.02 should not trigger)
            if 120 <= tick < 123:
                prices[0] = 0.9799  # just beyond edge

        if stale_seconds > 0:
            oracle_q = clamp(0.8 + rng.gauss(0.0, 0.01), 0.0, 1.0)

    # ------------------------------------------------------------------
    # Cat 5: Optimizer Adversarial
    # ------------------------------------------------------------------
    elif 41 <= sid <= 50:
        prices = evolve_prices(prices, targets, 0.0012, rng, reversion=0.22)
        if sid == 44:
            # induce loss shape oscillation via alternating oracle confidence
            oracle_q = 1.0 if tick % 2 == 0 else 0.85
        elif sid == 48 and tick < 100:
            prices[0] = clamp(prices[0] + 0.002, 0.0001, 1.5)
        elif sid == 49:
            prices = [clamp(p + 0.003, 0.0001, 1.5) for p in prices]
        elif sid == 50:
            oracle_q = clamp(0.95 + rng.gauss(0.0, 0.01), 0.0, 1.0)

    # ------------------------------------------------------------------
    # Cat 6: Liquidity & Supply
    # ------------------------------------------------------------------
    elif 51 <= sid <= 60:
        prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)

        if sid == 51:
            supply = 0.001
        elif sid == 52:
            supply = 1_000_000_000.0
        elif sid == 53:
            supply = 0.0 if tick == 0 else 1_000_000.0
        elif sid == 54:
            supply = 1_000_000.0 if tick == 0 else 0.0
        elif sid == 55:
            supply = 1_000_000.0 if tick % 2 == 0 else 100_000.0
        elif sid == 56:
            supply = 100_000.0 * (1.0 + 0.02 * min(tick, 200))
        elif sid == 57:
            frac = max(0.0, 1.0 - min(tick, 200) / 200.0)
            supply = max(1_000.0, 1_000_000.0 * frac)
        elif sid == 58:
            if tick < 30:
                supply = 200_000.0
            elif tick == 30:
                supply = 5_000_000.0
            else:
                supply = 5_000_000.0
        elif sid == 59:
            supply = 100_000.0 * (1.01 ** min(tick, 220))
        elif sid == 60:
            if tick < 40:
                supply = 1_000_000.0
            elif tick == 40:
                supply = 700_000.0
            elif tick == 41:
                supply = 300_000.0
            elif tick == 42:
                supply = 100_000.0
            else:
                supply = 100_000.0

    # ------------------------------------------------------------------
    # Cat 7: Multi-Factor Stress
    # ------------------------------------------------------------------
    elif 61 <= sid <= 70:
        prices = evolve_prices(prices, targets, 0.002, rng, reversion=0.18)

        if sid == 61:
            if 30 <= tick <= 90:
                prices[0] = 0.80
                stale_seconds = 180
                divergence = 0.05
        elif sid == 62:
            prices = evolve_prices(prices, targets, 0.025, rng, reversion=0.12)
            supply = 100.0
        elif sid == 63:
            if tick == 25:
                prices = [0.50, 0.55, 0.60, 0.65]
                forced = {"cb1": True, "cb2": True, "cb3": True}
            if 25 <= tick <= 70:
                stale_seconds = 180
                divergence = 0.05
            if tick in (35, 45):
                forced["cb4"] = True
        elif sid == 64:
            if 20 <= tick <= 120:
                prices[0] = 0.82
                prices[1] = 0.84
                prices[2] = 0.86
        elif sid == 65:
            if 15 <= tick <= 120:
                prices[0] = clamp(prices[0] - 0.04, 0.0001, 1.5)
        elif sid == 66:
            prices[0] = clamp(prices[0] - 0.01, 0.0001, 1.5)
            stale_seconds = 60
            divergence = 0.015
            supply = 900_000.0 + 100_000.0 * math.sin(tick / 10.0)
        elif sid == 67:
            if tick < 80:
                prices = [0.60, 0.65, 0.62, 0.64]
                stale_seconds = 180
                divergence = 0.06
                supply = 50_000.0
            else:
                prices = evolve_prices(prices, [1.0, 1.0, 1.0, 1.0], 0.002, rng, reversion=0.20)
                stale_seconds = 0
                divergence = 0.001
                supply = 800_000.0
        elif sid == 68:
            crisis = (tick // 20) % 2 == 1
            if crisis:
                prices = evolve_prices(prices, [0.85, 0.88, 0.90, 0.87], 0.015, rng, reversion=0.12)
                stale_seconds = 150
                divergence = 0.03
            else:
                prices = evolve_prices(prices, [1.0, 1.0, 1.0, 1.0], 0.001, rng, reversion=0.20)
        elif sid == 69:
            frac = tick / float(max(1, ticks - 1))
            lvl = 1.0 - 0.30 * frac
            prices = evolve_prices(prices, [lvl, lvl, lvl, lvl], 0.004 + 0.01 * frac, rng, reversion=0.10)
            stale_seconds = int(180 * frac)
            divergence = 0.005 + 0.03 * frac
        elif sid == 70:
            if tick < 500:
                prices = evolve_prices(prices, [0.72, 0.75, 0.78, 0.74], 0.012, rng, reversion=0.12)
                stale_seconds = 150
                divergence = 0.03
            else:
                prices = evolve_prices(prices, [1.0, 1.0, 1.0, 1.0], 0.0015, rng, reversion=0.22)
                stale_seconds = 0
                divergence = 0.001

        oracle_q = clamp(1.0 - min(0.8, stale_seconds / 500.0) - min(0.6, divergence / 0.08) + rng.gauss(0.0, 0.003), 0.0, 1.0)

    # ------------------------------------------------------------------
    # Cat 8: Edge Cases & Boundary
    # ------------------------------------------------------------------
    elif 71 <= sid <= 80:
        if sid == 71:
            prices = [1.0, 1.0, 1.0, 1.0]
            oracle_q = 1.0
            peg_noise = 0.0
        elif sid == 72:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
        elif sid == 73:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
        elif sid == 74:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
        elif sid == 75:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
        elif sid == 76:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
        elif sid == 77:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
            if 20 <= tick <= 120:
                prices[1] = 0.0001
        elif sid == 78:
            prices = evolve_prices(prices, targets, 0.0006, rng, reversion=0.24)
            if tick % 10000 == 0 and tick > 0:
                prices[2] = clamp(prices[2] * 0.98, 0.0001, 1.5)
            peg_noise = rng.gauss(0.0, 0.00005)
        elif sid == 79:
            prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.22)
            oracle_q = 0.0 if tick % 2 == 0 else 1.0
        elif sid == 80:
            if tick % 2 == 0:
                prices = [0.0001, 1.5, 0.0001, 1.5]
                stale_seconds = 180
                divergence = 0.08
                forced = {"cb1": True, "cb2": True, "cb3": True, "cb4": True}
                supply = 1_000_000_000.0
                oracle_q = 0.0
            else:
                prices = [1.5, 0.0001, 1.5, 0.0001]
                stale_seconds = 180
                divergence = 0.08
                forced = {"cb1": True, "cb2": True, "cb3": True, "cb4": True}
                supply = 0.001
                oracle_q = 1.0

    # fallback generic path
    else:
        prices = evolve_prices(prices, targets, 0.001, rng, reversion=0.20)

    prices = [clamp(p, 0.0001, 1.5) for p in prices]
    oracle_q = clamp(oracle_q, 0.0, 1.0)

    ctx["prices"] = prices[:]

    expected_breakers = calc_expected_breakers(prices, stale_seconds, divergence, forced)

    return TickSpec(
        prices=prices,
        oracle_q=oracle_q,
        stale_seconds=stale_seconds,
        divergence=divergence,
        expected_breakers=expected_breakers,
        forced=forced,
        peg_noise=peg_noise,
        supply=supply,
    )


def modify_gradients_for_scenario(
    sid: int,
    tick: int,
    grad_w: List[float],
    grad_fee: float,
    rng: random.Random,
) -> Tuple[List[float], float]:
    gw = [float(g) for g in grad_w]
    gf = float(grad_fee)

    if sid == 41:
        gw = [g * 1_000_000.0 for g in gw]
        gf = gf * 1_000_000.0
    elif sid == 42:
        gw = [g * 1e-12 for g in gw]
        gf = gf * 1e-12
    elif sid == 43:
        gw = [0.0 for _ in gw]
        gf = 0.0
    elif sid == 44:
        s = -1.0 if tick % 2 else 1.0
        gw = [g * s for g in gw]
        gf = gf * s
    elif sid == 45:
        gw = [-g for g in gw]
        gf = -gf
    elif sid == 49:
        gw = [-(abs(g) + 1.0 + rng.random()) for g in gw]
        gf = -(abs(gf) + 1.0)
    elif sid == 64:
        gw = [-(abs(g) + 0.5) for g in gw]
        gf = -(abs(gf) + 0.2)

    return gw, gf


def apply_state_pre_tick_overrides(sid: int, state: ms.ProtocolState) -> None:
    if sid == 75:
        state.mint_fee = 0.0
    elif sid == 76:
        state.mint_fee = 0.10


def apply_proposal_overrides(sid: int, tick: int, state: ms.ProtocolState, proposal: Dict[str, Any]) -> Dict[str, Any]:
    out = dict(proposal)

    if sid == 48 and tick < 100:
        w0 = min(state.w_caps[0], 0.85)
        rem = max(0.0, 1.0 - w0)
        others = state.weights[1:]
        denom = sum(others)
        if denom <= 0:
            tail = [rem / 3.0, rem / 3.0, rem / 3.0]
        else:
            tail = [max(0.0, rem * (x / denom)) for x in others]
        out["weights"] = [w0] + tail

    if sid in (75, 76):
        out["mint_fee"] = state.mint_fee

    return out


# -----------------------------------------------------------------------------
# Single-run simulation (worker)
# -----------------------------------------------------------------------------


def run_single_scenario(seed: int, sid: int, ticks: int) -> Dict[str, Any]:
    rng = random.Random(seed)

    state = ms.ProtocolState()
    opt_cfg: Dict[str, float] = {"lr": 0.005, "beta1": 0.9, "beta2": 0.999}
    init_state_for_scenario(sid, state, opt_cfg)

    loss_engine = ms.LossEngine()
    optimizer = ms.AdamOptimizer(
        n_weights=len(state.weights),
        lr=opt_cfg["lr"],
        beta1=opt_cfg["beta1"],
        beta2=opt_cfg["beta2"],
    )
    breaker = ms.CircuitBreaker(n_assets=len(state.weights))
    keeper = ms.Keeper()

    ctx: Dict[str, Any] = {"prices": [1.0, 1.0, 1.0, 1.0], "true_prices": [1.0, 1.0, 1.0, 1.0]}

    peg_errors: List[float] = []
    sq_errors: List[float] = []
    cr_violations = 0
    min_cr = float("inf")
    max_turnover = 0.0

    activation_counts = {1: 0, 2: 0, 3: 0, 4: 0}
    false_positives = 0
    event_idx = 0

    nan_inf_detected = False

    checkpoint_state = state.clone()
    checkpoint_lr = optimizer.lr

    t0 = time.perf_counter()

    try:
        for tick in range(ticks):
            state.begin_tick()
            apply_state_pre_tick_overrides(sid, state)

            spec = scenario_tick_spec(sid, tick, ticks, rng, state, ctx)

            if spec.supply is not None:
                state.supply = float(spec.supply)
                state.reserve_value = state.cr * state.supply

            market = ms.MarketTick(
                tick=tick,
                prices=spec.prices[:],
                oracle_q=float(spec.oracle_q),
                stale_seconds=int(spec.stale_seconds),
                divergence=float(spec.divergence),
                expected_breakers=(spec.expected_breakers[:] if spec.expected_breakers is not None else []),
            )

            loss_finite = True
            loss_value: Optional[float] = None
            grad_w = [0.0] * len(state.weights)
            grad_fee = 0.0

            try:
                loss, loss_ctx = loss_engine.compute(state, market.prices, market.oracle_q)
                loss_value = loss.data
                loss.backward()
                grad_w = [wv.grad for wv in loss_ctx["weights"]]  # type: ignore[index]
                grad_fee = float(loss_ctx["fee"].grad)  # type: ignore[index]
                if any((not math.isfinite(g)) for g in grad_w + [grad_fee]):
                    loss_finite = False
            except Exception:
                loss_finite = False

            grad_w, grad_fee = modify_gradients_for_scenario(sid, tick, grad_w, grad_fee, rng)

            nav_now = state.effective_collateral_value(market.prices)
            nav_drop = nav_now - state.nav_prev

            action = breaker.update(
                tick=tick,
                state=state,
                market=market,
                nav_drop=nav_drop,
                loss_finite=loss_finite,
                loss_value=loss_value,
                forced=(spec.forced or {}),
            )

            while event_idx < len(breaker.events):
                ev = breaker.events[event_idx]
                if ev.get("event") == "activate":
                    cb_id = int(ev["cb"])
                    activation_counts[cb_id] += 1
                    if cb_id not in market.expected_breakers:
                        false_positives += 1
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
                proposal = apply_proposal_overrides(sid, tick, state, proposal)
                keeper.submit_update_proposal(state, proposal)

            apply_state_pre_tick_overrides(sid, state)

            peg = state.update_from_market(market.prices, market.oracle_q, peg_noise=float(spec.peg_noise))
            turnover = sum(abs(a - b) for a, b in zip(state.weights, state.prev_weights))
            max_turnover = max(max_turnover, turnover)

            err = abs(peg - 1.0)
            peg_errors.append(err)
            sq_errors.append((peg - 1.0) ** 2)

            min_cr = min(min_cr, state.cr)
            if state.cr < state.cr_hard_min:
                cr_violations += 1

            if (not finite_state(state)) or (not math.isfinite(peg)):
                nan_inf_detected = True

            checkpoint_state = state.clone()
            checkpoint_lr = optimizer.lr

    except Exception as e:
        return {
            "seed": seed,
            "sid": sid,
            "ticks": ticks,
            "crash": True,
            "error": f"{type(e).__name__}: {e}",
            "traceback": traceback.format_exc(),
            "runtime_sec": time.perf_counter() - t0,
        }

    mae = mean(peg_errors)
    rmse = math.sqrt(mean(sq_errors)) if sq_errors else 0.0
    cr_violation_rate = cr_violations / float(max(1, ticks))
    total_activations = sum(activation_counts.values())
    fp_rate = false_positives / float(max(1, total_activations))

    return {
        "seed": seed,
        "sid": sid,
        "ticks": ticks,
        "crash": False,
        "mae": mae,
        "rmse": rmse,
        "min_cr": min_cr,
        "cr_violation_rate": cr_violation_rate,
        "max_turnover": max_turnover,
        "breaker_activations": activation_counts,
        "breaker_false_positives": false_positives,
        "breaker_false_positive_rate": fp_rate,
        "nan_inf_detected": nan_inf_detected,
        "runtime_sec": time.perf_counter() - t0,
    }


# -----------------------------------------------------------------------------
# Aggregation + reporting
# -----------------------------------------------------------------------------


def aggregate_scenario(spec: ScenarioSpec, runs: List[Dict[str, Any]]) -> Dict[str, Any]:
    crashed = [r for r in runs if r.get("crash")]
    ok_runs = [r for r in runs if not r.get("crash")]
    nan_runs = [r for r in ok_runs if r.get("nan_inf_detected")]

    failures = len(crashed) + len(nan_runs)
    failure_rate = failures / float(max(1, len(runs)))
    critical = failure_rate > 0.50

    maes = [r["mae"] for r in ok_runs]
    crs = [r["cr_violation_rate"] for r in ok_runs]
    rmses = [r["rmse"] for r in ok_runs]
    runtimes = [r["runtime_sec"] for r in runs]

    mean_mae = mean(maes) if maes else float("inf")
    mean_cr = mean(crs) if crs else float("inf")

    pass_gate = (
        mean_mae < GATE_MAE_MAX
        and mean_cr < GATE_CR_VIOL_MAX
        and len(crashed) == 0
        and len(nan_runs) == 0
    )

    status = "CRITICAL" if critical else ("PASS" if pass_gate else "FAIL")

    return {
        "sid": spec.sid,
        "name": spec.name,
        "category": spec.category,
        "ticks": spec.ticks,
        "status": status,
        "pass": pass_gate,
        "critical": critical,
        "counts": {
            "runs_total": len(runs),
            "runs_ok": len(ok_runs),
            "runs_crashed": len(crashed),
            "runs_nan_inf": len(nan_runs),
            "run_failures": failures,
            "failure_rate": failure_rate,
        },
        "gate": {
            "mean_mae": mean_mae,
            "mean_cr_violation_rate": mean_cr,
            "criteria": {
                "mae_lt": GATE_MAE_MAX,
                "cr_violation_lt": GATE_CR_VIOL_MAX,
                "no_crash_nan_inf": True,
            },
        },
        "stats": {
            "mae": {
                "mean": mean(maes),
                "median": percentile(maes, 50),
                "p95": percentile(maes, 95),
                "worst": max(maes) if maes else float("inf"),
            },
            "rmse": {
                "mean": mean(rmses),
                "median": percentile(rmses, 50),
                "p95": percentile(rmses, 95),
                "worst": max(rmses) if rmses else float("inf"),
            },
            "cr_violation_rate": {
                "mean": mean(crs),
                "median": percentile(crs, 50),
                "p95": percentile(crs, 95),
                "worst": max(crs) if crs else float("inf"),
            },
            "runtime_sec": {
                "mean": mean(runtimes),
                "median": percentile(runtimes, 50),
                "p95": percentile(runtimes, 95),
                "worst": max(runtimes) if runtimes else 0.0,
            },
        },
        "crashes": [
            {
                "seed": c.get("seed"),
                "error": c.get("error"),
            }
            for c in crashed[:10]
        ],
        "runs": runs,
    }


def render_report(result: Dict[str, Any]) -> str:
    lines: List[str] = []
    lines.append("# Microstable Mega Stress Test Report")
    lines.append("")
    lines.append(f"Generated: {result['generated_at']}")
    lines.append(f"Requested runs/scenario: {result['config']['requested_runs_per_scenario']}")
    lines.append(f"Actual runs/scenario: {result['config']['actual_runs_per_scenario']}")
    lines.append(f"Workers: {result['config']['workers']}")
    lines.append(f"Runtime (sec): {result['config']['runtime_sec']:.2f}")
    lines.append("")
    lines.append("## Gate criteria (relaxed)")
    lines.append("- MAE < 0.05")
    lines.append("- CR violation rate < 20%")
    lines.append("- No crashes / NaN / Inf")
    lines.append("")

    overall_pass = all(sc["pass"] for sc in result["scenarios"])
    critical_count = sum(1 for sc in result["scenarios"] if sc.get("critical"))
    lines.append(f"## Overall: {'PASS' if overall_pass else 'FAIL'}")
    lines.append(f"Critical scenarios (>50% run failures): {critical_count}")
    lines.append("")

    lines.append("## Scenario Summary")
    lines.append("| # | Scenario | Category | Status | MAE(mean) | CRv(mean) | Crashes | NaN/Inf |")
    lines.append("|---:|---|---|---|---:|---:|---:|---:|")
    for sc in result["scenarios"]:
        lines.append(
            f"| {sc['sid']} | {sc['name']} | {sc['category']} | {sc['status']} "
            f"| {sc['gate']['mean_mae']:.6f} | {sc['gate']['mean_cr_violation_rate']:.6f} "
            f"| {sc['counts']['runs_crashed']} | {sc['counts']['runs_nan_inf']} |"
        )

    lines.append("")
    lines.append("## Critical Scenarios")
    criticals = [sc for sc in result["scenarios"] if sc.get("critical")]
    if not criticals:
        lines.append("- None")
    else:
        for sc in criticals:
            lines.append(
                f"- [{sc['sid']}] {sc['name']} failure_rate={sc['counts']['failure_rate']*100:.1f}% "
                f"(crashed={sc['counts']['runs_crashed']}, nan_inf={sc['counts']['runs_nan_inf']})"
            )

    lines.append("")
    lines.append("## Per-scenario details")
    for sc in result["scenarios"]:
        lines.append("")
        lines.append(f"### [{sc['sid']}] {sc['name']} — {sc['status']}")
        lines.append(
            f"- runs={sc['counts']['runs_total']} ok={sc['counts']['runs_ok']} crashed={sc['counts']['runs_crashed']} "
            f"nan_inf={sc['counts']['runs_nan_inf']} failure_rate={sc['counts']['failure_rate']*100:.2f}%"
        )
        lines.append(
            f"- MAE mean={sc['stats']['mae']['mean']:.6f}, p95={sc['stats']['mae']['p95']:.6f}, "
            f"worst={sc['stats']['mae']['worst']:.6f}"
        )
        lines.append(
            f"- CR violation mean={sc['stats']['cr_violation_rate']['mean']:.6f}, "
            f"p95={sc['stats']['cr_violation_rate']['p95']:.6f}, "
            f"worst={sc['stats']['cr_violation_rate']['worst']:.6f}"
        )
        lines.append(
            f"- Runtime mean={sc['stats']['runtime_sec']['mean']:.3f}s, p95={sc['stats']['runtime_sec']['p95']:.3f}s"
        )

    return "\n".join(lines).rstrip() + "\n"


# -----------------------------------------------------------------------------
# Runtime estimation
# -----------------------------------------------------------------------------


def estimate_runtime_seconds(workers: int, requested_runs: int) -> float:
    # Quick empirical benchmark from a short run (scenario 1, 200 ticks)
    bench_ticks = 200
    t0 = time.perf_counter()
    _ = run_single_scenario(seed=20260222, sid=1, ticks=bench_ticks)
    elapsed = max(1e-6, time.perf_counter() - t0)
    sec_per_tick = elapsed / float(bench_ticks)

    total_ticks = sum(s.ticks for s in SCENARIOS) * requested_runs

    # efficiency factor for process pool + imbalance + Python overhead
    eff_workers = max(1.0, workers * 0.72)
    return total_ticks * sec_per_tick / eff_workers


# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Microstable mega stress test (80 scenarios × Monte Carlo)")
    parser.add_argument("--runs", type=int, default=DEFAULT_REQUESTED_RUNS, help="Requested Monte Carlo runs per scenario (default: 100)")
    parser.add_argument("--workers", type=int, default=0, help="Worker processes (default: cpu_count-1, capped)")
    parser.add_argument("--no-auto-fallback", action="store_true", help="Disable automatic 100->50 fallback based on projected runtime")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    cpu = os.cpu_count() or 2
    default_workers = max(1, min(cpu - 1, 12))
    workers = args.workers if args.workers > 0 else default_workers

    requested_runs = max(1, int(args.runs))
    actual_runs = requested_runs

    fallback_reason = ""
    if (not args.no_auto_fallback) and requested_runs >= 100:
        try:
            projected = estimate_runtime_seconds(workers=workers, requested_runs=requested_runs)
            if projected > TARGET_RUNTIME_SEC:
                actual_runs = FALLBACK_RUNS
                fallback_reason = (
                    f"Projected runtime {projected:.1f}s exceeds target {TARGET_RUNTIME_SEC}s; "
                    f"auto-reducing runs/scenario to {FALLBACK_RUNS}."
                )
        except Exception as e:
            fallback_reason = f"Runtime estimation failed ({type(e).__name__}: {e}); keeping runs={actual_runs}."

    print("microstable mega stress test", flush=True)
    print(f"scenarios={len(SCENARIOS)} requested_runs/scenario={requested_runs} actual_runs/scenario={actual_runs} workers={workers}", flush=True)
    if fallback_reason:
        print(f"[auto] {fallback_reason}", flush=True)

    seed_rng = random.Random(20260222)
    all_summaries: List[Dict[str, Any]] = []

    global_start = time.perf_counter()

    with ProcessPoolExecutor(max_workers=workers) as ex:
        for spec in SCENARIOS:
            print(f"\n[{spec.sid:02d}] {spec.name} ({spec.category}) ticks={spec.ticks} runs={actual_runs}", flush=True)

            seeds = [seed_rng.randint(1, 2**31 - 1) for _ in range(actual_runs)]
            futures = [ex.submit(run_single_scenario, sd, spec.sid, spec.ticks) for sd in seeds]

            runs: List[Dict[str, Any]] = []
            done = 0
            for fut in as_completed(futures):
                try:
                    r = fut.result()
                except Exception as e:
                    r = {
                        "seed": None,
                        "sid": spec.sid,
                        "ticks": spec.ticks,
                        "crash": True,
                        "error": f"WorkerException: {type(e).__name__}: {e}",
                        "traceback": "",
                        "runtime_sec": 0.0,
                    }
                runs.append(r)
                done += 1
                if done % 10 == 0 or done == actual_runs:
                    ok = sum(1 for x in runs if not x.get("crash"))
                    print(f"  progress {done:3d}/{actual_runs} (ok={ok})", flush=True)

            summary = aggregate_scenario(spec, runs)
            all_summaries.append(summary)

            print(
                f"  -> {summary['status']} "
                f"(MAE={summary['gate']['mean_mae']:.5f}, "
                f"CRv={summary['gate']['mean_cr_violation_rate']:.5f}, "
                f"crash={summary['counts']['runs_crashed']}, nan_inf={summary['counts']['runs_nan_inf']})",
                flush=True,
            )

    runtime_sec = time.perf_counter() - global_start

    result = {
        "generated_at": datetime.now().isoformat(),
        "config": {
            "requested_runs_per_scenario": requested_runs,
            "actual_runs_per_scenario": actual_runs,
            "workers": workers,
            "runtime_sec": runtime_sec,
            "gate": {
                "mae_lt": GATE_MAE_MAX,
                "cr_violation_lt": GATE_CR_VIOL_MAX,
                "no_crash_nan_inf": True,
            },
            "auto_fallback_note": fallback_reason,
        },
        "scenarios": all_summaries,
    }

    with open(RESULTS_JSON_PATH, "w", encoding="utf-8") as f:
        json.dump(result, f, ensure_ascii=False, indent=2)

    report = render_report(result)
    with open(REPORT_MD_PATH, "w", encoding="utf-8") as f:
        f.write(report)

    passed = sum(1 for s in all_summaries if s["status"] == "PASS")
    failed = sum(1 for s in all_summaries if s["status"] == "FAIL")
    critical = sum(1 for s in all_summaries if s["status"] == "CRITICAL")

    print("\n=== completed ===", flush=True)
    print(f"runtime_sec={runtime_sec:.2f}", flush=True)
    print(f"PASS={passed} FAIL={failed} CRITICAL={critical}", flush=True)
    print(f"results_json={RESULTS_JSON_PATH}", flush=True)
    print(f"report_md={REPORT_MD_PATH}", flush=True)


if __name__ == "__main__":
    main()
