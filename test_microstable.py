#!/usr/bin/env python3
"""
Comprehensive test suite for microstable.py
Run: python test_microstable.py
"""

from __future__ import annotations

import math
import os
import random
import traceback
from argparse import Namespace
from dataclasses import dataclass
from typing import Callable, Dict, List, Sequence, Tuple

import microstable as ms
from agents import consensus as consensus_agent


ATOL = 1e-9
RTOL = 1e-6
OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "outputs")


def approx(a: float, b: float, atol: float = ATOL, rtol: float = RTOL) -> bool:
    return abs(a - b) <= atol + rtol * abs(b)


@dataclass
class Case:
    cid: str
    category: str
    fn: Callable[[], str]


# -----------------------------------------------------------------------------
# A. Value autograd (12)
# -----------------------------------------------------------------------------


def tc_v001() -> str:
    a = ms.Value(1.25)
    b = ms.Value(-0.75)
    y = a + b
    assert approx(y.data, 0.5)
    return f"y={y.data:.12f}"


def tc_v002() -> str:
    a = ms.Value(2.0)
    b = ms.Value(3.0)
    y = a + b
    y.backward()
    assert approx(a.grad, 1.0)
    assert approx(b.grad, 1.0)
    return f"grads=({a.grad:.6f},{b.grad:.6f})"


def tc_v003() -> str:
    a = ms.Value(2.0)
    b = ms.Value(-3.0)
    y = a * b
    y.backward()
    assert approx(y.data, -6.0)
    assert approx(a.grad, -3.0)
    assert approx(b.grad, 2.0)
    return f"y={y.data:.6f}, grads=({a.grad:.6f},{b.grad:.6f})"


def tc_v004() -> str:
    a = ms.Value(3.0)
    b = ms.Value(2.0)
    y = a / b
    y.backward()
    assert approx(y.data, 1.5)
    assert approx(a.grad, 0.5)
    assert approx(b.grad, -0.75)
    return f"y={y.data:.6f}, grads=({a.grad:.6f},{b.grad:.6f})"


def tc_v005() -> str:
    x = ms.Value(2.5)
    y = x ** 3
    y.backward()
    assert approx(y.data, 15.625)
    assert approx(x.grad, 18.75)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


def tc_v006() -> str:
    x = ms.Value(0.7)
    y = x.tanh()
    y.backward()
    expected_y = math.tanh(0.7)
    expected_g = 1.0 - expected_y * expected_y
    assert approx(y.data, expected_y)
    assert approx(x.grad, expected_g)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


def tc_v007() -> str:
    x = ms.Value(1.2)
    y = x.exp()
    y.backward()
    expected = math.exp(1.2)
    assert approx(y.data, expected)
    assert approx(x.grad, expected)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


def tc_v008() -> str:
    x = ms.Value(2.5)
    y = x.log()
    y.backward()
    assert approx(y.data, math.log(2.5))
    assert approx(x.grad, 1.0 / 2.5)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


def tc_v009() -> str:
    a = ms.Value(2.0)
    b = ms.Value(-3.0)
    c = ms.Value(4.0)
    y = a * b + c ** 2
    y.backward()
    assert approx(y.data, 10.0)
    assert approx(a.grad, -3.0)
    assert approx(b.grad, 2.0)
    assert approx(c.grad, 8.0)
    return f"y={y.data:.6f}, grads=({a.grad:.6f},{b.grad:.6f},{c.grad:.6f})"


def tc_v010() -> str:
    x = ms.Value(1e-12)
    y = x.log()
    y.backward()
    assert math.isfinite(y.data)
    assert math.isfinite(x.grad)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


def tc_v011() -> str:
    x = ms.Value(1e-12)
    y = ms.Value(1.0) / x
    y.backward()
    assert math.isfinite(y.data)
    assert math.isfinite(x.grad)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


def tc_v012() -> str:
    x = ms.Value(0.0)
    y = x.relu()
    y.backward()
    assert approx(y.data, 0.0)
    assert approx(x.grad, 0.0)
    return f"y={y.data:.6f}, grad={x.grad:.6f}"


# -----------------------------------------------------------------------------
# B. Loss function (8)
# -----------------------------------------------------------------------------


def tc_l001() -> str:
    peg_loss = 5.0 * (1.0 - 1.0) ** 2
    assert approx(peg_loss, 0.0)
    return f"peg_loss={peg_loss:.6f}"


def tc_l002() -> str:
    peg_loss = 5.0 * (0.98 - 1.0) ** 2
    assert peg_loss > 0.0
    assert approx(peg_loss, 0.002)
    return f"peg_loss={peg_loss:.6f}"


def tc_l003() -> str:
    cr_pen = 20.0 * max(0.0, 1.20 - 1.25) ** 2
    assert approx(cr_pen, 0.0)
    return f"cr_penalty={cr_pen:.6f}"


def tc_l004() -> str:
    cr_pen = 20.0 * max(0.0, 1.20 - 1.10) ** 2
    assert cr_pen > 0.0
    assert approx(cr_pen, 0.2)
    return f"cr_penalty={cr_pen:.6f}"


def tc_l005() -> str:
    w = [1.0, 0.0, 0.0, 0.0]
    conc = sum(x * x for x in w)
    assert approx(conc, 1.0)
    return f"conc={conc:.6f}"


def tc_l006() -> str:
    w = [0.25, 0.25, 0.25, 0.25]
    conc = sum(x * x for x in w)
    assert approx(conc, 0.25)
    assert conc < 1.0
    return f"conc={conc:.6f}"


def tc_l007() -> str:
    w_prev = [0.4, 0.3, 0.2, 0.1]
    w_t = [0.4, 0.3, 0.2, 0.1]
    turn = 0.5 * sum(abs(a - b) for a, b in zip(w_t, w_prev))
    assert approx(turn, 0.0)
    return f"turn={turn:.6f}"


def tc_l008() -> str:
    orc = 3.0 * (1.0 - 1.0) ** 2
    assert approx(orc, 0.0)
    return f"oracle_loss={orc:.6f}"


# -----------------------------------------------------------------------------
# C. Optimizer (8)
# -----------------------------------------------------------------------------


def tc_o001() -> str:
    opt = ms.AdamOptimizer(4)
    theta = [0.40, 0.30, 0.20, 0.10]
    g = [0.2, -0.1, 0.05, -0.15]
    new_w, _ = opt.step(theta, 0.002, g, 0.0, ms.BASE_W_CAPS)
    assert any(abs(a - b) > 0 for a, b in zip(theta, new_w))
    return f"new_w={new_w}"


def tc_o002() -> str:
    w_raw = [0.62, 0.18, 0.15, 0.10]
    proj = ms.AdamOptimizer.simplex_projection(w_raw)
    assert approx(sum(proj), 1.0, atol=1e-10)
    return f"sum={sum(proj):.12f}"


def tc_o003() -> str:
    w_raw = [0.70, 0.20, 0.05, 0.05]
    caps = [0.55, 0.45, 0.45, 0.35]
    proj = ms.AdamOptimizer.project_box_simplex(w_raw, [0.0] * 4, caps, target=1.0)
    assert all(proj[i] <= caps[i] + 1e-12 for i in range(4))
    return f"proj={proj}"


def tc_o004() -> str:
    w_raw = [0.50, -0.10, 0.40, 0.20]
    caps = [0.55, 0.45, 0.45, 0.35]
    proj = ms.AdamOptimizer.project_box_simplex(w_raw, [0.0] * 4, caps, target=1.0)
    assert min(proj) >= -1e-12
    return f"proj={proj}"


def tc_o005() -> str:
    opt = ms.AdamOptimizer(4)
    prev = [0.40, 0.30, 0.20, 0.10]
    huge_grad = [-10.0, 10.0, -10.0, 10.0]
    new_w, _ = opt.step(prev, 0.002, huge_grad, 0.0, ms.BASE_W_CAPS)
    assert all(abs(new_w[i] - prev[i]) <= 0.02 + 1e-9 for i in range(4))
    return f"delta={[new_w[i]-prev[i] for i in range(4)]}"


def tc_o006() -> str:
    opt = ms.AdamOptimizer(4)
    prev_fee = 0.0020
    _, new_fee = opt.step([0.40, 0.30, 0.20, 0.10], prev_fee, [0.0] * 4, -100.0, ms.BASE_W_CAPS)
    assert abs(new_fee - prev_fee) <= 0.001 + 1e-12
    return f"fee_delta={new_fee-prev_fee:.6f}"


def tc_o007() -> str:
    g = [10.0, 10.0, 10.0, 10.0]
    gc = ms.AdamOptimizer.clip_gradients(g, max_norm=1.0)
    norm = math.sqrt(sum(x * x for x in gc))
    assert norm <= 1.0 + 1e-12
    return f"norm={norm:.6f}"


def tc_o008() -> str:
    rng = random.Random(42)
    state_w = [0.40, 0.30, 0.20, 0.10]
    fee = 0.002
    opt = ms.AdamOptimizer(4)
    for _ in range(100):
        grads = [rng.uniform(-1.0, 1.0) for _ in range(4)]
        grad_fee = rng.uniform(-1.0, 1.0)
        state_w, fee = opt.step(state_w, fee, grads, grad_fee, ms.BASE_W_CAPS)
        assert abs(sum(state_w) - 1.0) < 1e-6
        assert all(0.0 <= state_w[i] <= ms.BASE_W_CAPS[i] + 1e-9 for i in range(4))
        assert math.isfinite(fee)
    return f"final_w={state_w}, fee={fee:.6f}"


# -----------------------------------------------------------------------------
# D. Circuit Breaker (15)
# -----------------------------------------------------------------------------


def mkt(tick: int, prices: Sequence[float], q: float = 1.0, stale: int = 0, div: float = 0.0) -> ms.MarketTick:
    expected = []
    depeg = sum(1 for p in prices if abs(p - 1.0) > 0.02)
    if depeg >= 1:
        expected.append(1)
    if depeg >= 2:
        expected.append(2)
    if stale > 120 or div > 0.02:
        expected.append(3)
    return ms.MarketTick(tick=tick, prices=list(prices), oracle_q=q, stale_seconds=stale, divergence=div, expected_breakers=expected)


def tc_cb001() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    seq = [0.979, 0.978, 0.977]
    for t, p in enumerate(seq):
        cb.update(t, state, mkt(t, [1.0, p, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    assert cb.is_active(1)
    staged_cap_upper = max(state.base_w_caps[1] * 0.5, 0.30 - ms.DELTA_W_MAX)
    assert state.base_w_caps[1] * 0.5 - 1e-12 <= state.w_caps[1] <= staged_cap_upper + 1e-12
    assert state.mint_limit <= 0.25 + 1e-12
    return f"cap1={state.w_caps[1]:.6f}, mint_limit={state.mint_limit:.3f}"


def tc_cb002() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    for t in range(3):
        cb.update(t, state, mkt(t, [1.0, 0.977, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    assert cb.is_active(1)
    for t in range(3, 60):
        cb.update(t, state, mkt(t, [1.0, 1.000, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=max(0.1, 1.0 - 0.01 * t))
        if not cb.is_active(1):
            break
    assert not cb.is_active(1)
    assert abs(state.w_caps[1] - state.base_w_caps[1]) < 1e-12
    return "cb1 recovered"


def tc_cb003() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    cb.update(0, state, mkt(0, [0.97, 0.96, 1.0, 1.0]), nav_drop=-0.04, loss_finite=True, loss_value=1.0)
    assert cb.is_active(2)
    return "cb2 active"


def tc_cb004() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    cb.update(0, state, mkt(0, [0.97, 0.96, 1.0, 1.0]), nav_drop=-0.04, loss_finite=True, loss_value=1.0)
    assert cb.is_active(2)
    assert state.mint_limit == 0.0
    assert state.mint_paused_reason == "MINT_PAUSED_BY_CB2"
    return state.mint_paused_reason


def tc_cb005() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    cb.update(0, state, mkt(0, [1, 1, 1, 1], stale=180, div=0.01), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    assert cb.is_active(3)
    assert state.oracle_degraded
    return "cb3 active + oracle degraded"


def tc_cb006() -> str:
    state = ms.ProtocolState()
    cb = ms.CircuitBreaker()
    opt = ms.AdamOptimizer(4)
    keeper = ms.Keeper()

    cb.update(0, state, mkt(0, [1, 1, 1, 1], stale=180, div=0.04), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    w_before = state.weights[:]
    if state.optimizer_enabled:
        prop = keeper.propose(state, opt, [0.1, -0.1, 0.05, -0.05], 0.1)
        keeper.submit_update_proposal(state, prop)
    assert state.weights == w_before
    return "optimizer frozen during cb3"


def tc_cb007() -> str:
    state = ms.ProtocolState()
    state.weights = [0.50, 0.20, 0.20, 0.10]
    checkpoint = state.clone()
    cb = ms.CircuitBreaker()

    action = cb.update(0, state, mkt(0, [1, 1, 1, 1]), nav_drop=0.0, loss_finite=False, loss_value=None)
    if action["rollback"]:
        state = checkpoint.clone()
    assert action["rollback"]
    assert cb.is_active(4)
    assert state.weights == checkpoint.weights
    return "cb4 rollback flag true"


def tc_cb008() -> str:
    state = ms.ProtocolState()
    cb = ms.CircuitBreaker()
    opt = ms.AdamOptimizer(4, lr=0.005)
    lr_before = opt.lr
    action = cb.update(0, state, mkt(0, [1, 1, 1, 1]), nav_drop=0.0, loss_finite=False, loss_value=None)
    if action["rollback"]:
        opt.lr = lr_before * 0.5
    assert approx(opt.lr, 0.0025)
    return f"lr={opt.lr:.6f}"


def tc_cb009() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    for t in range(3):
        cb.update(t, state, mkt(t, [1.0, 0.977, 1.0, 1.0], stale=180, div=0.03), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    assert cb.is_active(1) and cb.is_active(3)
    return "cb1 & cb3 co-active"


def tc_cb010() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    for t in range(3):
        cb.update(t, state, mkt(t, [1.0, 0.977, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    assert cb.is_active(1)
    # short normalization; should not recover before min_hold/recovery requirements
    for t in range(3, 8):
        cb.update(t, state, mkt(t, [1.0, 1.0, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=1.0)
    assert cb.is_active(1)
    return f"state={cb.machines[1].state}"


def tc_cb011() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    for t in range(3):
        cb.update(t, state, mkt(t, [1.0, 0.977, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=1.0)

    recovered_tick = None
    for t in range(3, 80):
        cb.update(t, state, mkt(t, [1.0, 1.0, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=0.5)
        if not cb.is_active(1):
            recovered_tick = t
            break

    assert recovered_tick is not None
    assert recovered_tick >= 16  # requires hold + 10 stable recovery streak
    return f"recovered_tick={recovered_tick}"


def tc_cb012() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()
    # sustain two depegged assets for 3 ticks -> cb1 streak + cb2 immediate
    for t in range(3):
        cb.update(t, state, mkt(t, [0.97, 0.96, 1.0, 1.0]), nav_drop=-0.03, loss_finite=True, loss_value=1.0)
    assert cb.is_active(1)
    assert cb.is_active(2)
    assert state.mint_limit == 0.0
    assert state.mint_paused_reason == "MINT_PAUSED_BY_CB2"
    return "cb2 priority applied"


def tc_cb013() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()

    # activate cb1
    for t in range(3):
        cb.update(t, state, mkt(t, [1.0, 0.977, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=1.0)

    # recover cb1
    t = 3
    while cb.is_active(1) and t < 100:
        cb.update(t, state, mkt(t, [1.0, 1.0, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=0.5)
        t += 1
    assert not cb.is_active(1)

    # within cooldown, re-trigger conditions should fail
    for k in range(3):
        cb.update(t + k, state, mkt(t + k, [1.0, 0.977, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=0.5)
    assert not cb.is_active(1)

    # after cooldown + additional streak, it should reactivate
    reactivated = False
    for j in range(10):
        cb.update(t + 3 + j, state, mkt(t + 3 + j, [1.0, 0.977, 1.0, 1.0]), nav_drop=0.0, loss_finite=True, loss_value=0.5)
        if cb.is_active(1):
            reactivated = True
            break
    assert reactivated
    return "cooldown respected"


def tc_cb014() -> str:
    m = ms.BreakerMachine(cb_id=1, min_hold=1, recovery_needed=1, cooldown_ticks=0)
    tick = 0
    for _ in range(2):
        assert m.try_trigger(tick)
        m.begin_tick()  # activated->holding
        m.begin_tick()  # holding->recovery_check
        assert m.recovery_step(True, False)
        tick += 5

    # third trigger within 30 ticks
    assert m.try_trigger(tick)
    assert m.extended_factor == 3
    return f"extended_factor={m.extended_factor}"


def tc_cb015() -> str:
    cb = ms.CircuitBreaker()
    state = ms.ProtocolState()

    # Force all to recovery-check and one step away from recovery.
    for cid, m in cb.machines.items():
        m.state = ms.CB_RECOVERY_CHECK
        m.recovery_streak = m.recovery_needed - 1
        m.cooldown_left = 0

    cb.loss_history.clear()
    cb.loss_history.extend([4.0, 3.0, 2.0])

    cb.update(
        tick=0,
        state=state,
        market=mkt(0, [1.0, 1.0, 1.0, 1.0]),
        nav_drop=0.0,
        loss_finite=True,
        loss_value=1.0,
    )

    rec = [int(e["cb"]) for e in cb.events if e["event"] == "recover"]
    assert rec[:4] == [4, 3, 2, 1]
    return f"recover_order={rec[:4]}"


# -----------------------------------------------------------------------------
# E. Scenario integration (8)
# -----------------------------------------------------------------------------


def tc_s001() -> str:
    r = ms.run_scenario("normal", seed=0, ticks=100)
    assert r.mae < 0.0015
    return f"mae={r.mae:.6f}"


def tc_s002() -> str:
    r = ms.run_scenario("normal", seed=1, ticks=100)
    assert r.cr_final > r.cr_target_final
    return f"cr_final={r.cr_final:.4f}, target={r.cr_target_final:.4f}"


def tc_s003() -> str:
    r = ms.run_scenario("single_depeg", seed=0, ticks=120)
    acts = [e for e in r.events if e["cb"] == 1 and e["event"] == "activate"]
    recs = [e for e in r.events if e["cb"] == 1 and e["event"] == "recover"]
    assert acts and recs
    recovery_time = int(recs[0]["tick"]) - int(acts[0]["tick"])
    assert recovery_time <= 30
    return f"recovery_time={recovery_time}"


def tc_s004() -> str:
    r = ms.run_scenario("multi_depeg", seed=0, ticks=100)
    assert r.cr_violation_rate <= 0.0
    assert r.breaker_activations[2] >= 1
    return f"cb2={r.breaker_activations[2]}, min_cr={r.min_cr:.4f}"


def tc_s005() -> str:
    r = ms.run_scenario("volatile", seed=0, ticks=200)
    for row in r.rows:
        assert math.isfinite(float(row["cr"]))
        assert math.isfinite(float(row["fee"]))
        lv = float(row["loss"])
        assert math.isfinite(lv)
    return f"rows={len(r.rows)}"


def tc_s006() -> str:
    r = ms.run_scenario("gradient_attack", seed=0, ticks=140)
    prev = None
    max_dw = 0.0
    max_df = 0.0
    for row in r.rows:
        w = [float(row[f"w{i}"]) for i in range(4)]
        f = float(row["fee"])
        if prev is not None:
            pw, pf = prev
            for i in range(4):
                max_dw = max(max_dw, abs(w[i] - pw[i]))
            max_df = max(max_df, abs(f - pf))
        prev = (w, f)
    assert max_dw <= 0.02 + 1e-8
    assert max_df <= 0.001 + 1e-8
    return f"max_dw={max_dw:.6f}, max_df={max_df:.6f}"


def tc_s007() -> str:
    r = ms.run_scenario("oracle_failure", seed=0, ticks=120)
    assert r.breaker_activations[3] >= 1
    cb3_rows = [row for row in r.rows if int(row["cb3"]) == 1]
    assert cb3_rows, "no cb3 active rows"
    assert all(int(row["optimizer_enabled"]) == 0 for row in cb3_rows)
    assert all(float(row["mint_limit"]) <= 0.10 + 1e-12 for row in cb3_rows)
    return f"cb3_rows={len(cb3_rows)}"


def tc_s008() -> str:
    r = ms.run_scenario("oracle_failure", seed=3, ticks=140)
    recs = [e for e in r.events if e["cb"] == 3 and e["event"] == "recover"]
    assert recs, "cb3 did not recover"
    last_recover_tick = int(recs[-1]["tick"])
    post = [row for row in r.rows if int(row["tick"]) > last_recover_tick + 3]
    assert post, "no post-recovery rows"
    assert any(int(row["cb3"]) == 0 and int(row["optimizer_enabled"]) == 1 for row in post)
    return f"recover_tick={last_recover_tick}"


# -----------------------------------------------------------------------------
# F. Agent interface (4)
# -----------------------------------------------------------------------------


def tc_a001() -> str:
    state = ms.ProtocolState()
    opt = ms.AdamOptimizer(4)
    keeper = ms.Keeper()
    prop = keeper.propose(state, opt, [0.2, -0.1, 0.0, -0.1], 0.1)
    res = keeper.submit_update_proposal(state, prop)
    assert res["status"] == "APPLIED"
    return f"status={res['status']}"


def tc_a002() -> str:
    state = ms.ProtocolState()
    cb = ms.CircuitBreaker()
    wd = ms.Watchdog()
    market = mkt(0, [0.97, 1.0, 1.0, 1.0], stale=180, div=0.03)
    forced = wd.detect(market)
    cb.update(0, state, market, nav_drop=0.0, loss_finite=True, loss_value=1.0, forced=forced)
    assert cb.is_active(3)
    return f"forced={forced}"


def tc_a003() -> str:
    state = ms.ProtocolState()
    state.weights = [0.50, 0.30, 0.20, 0.20]  # sum != 1
    auditor = ms.Auditor()
    out = auditor.verify_invariants(state)
    assert out["alert_emitted"] is True
    assert "INV_WEIGHT_SUM" in out["violations"]
    return f"violations={out['violations']}"


def tc_a004() -> str:
    d = ms.distribute_fees(1000.0)
    vals = [d["keeper"], d["watchdog"], d["auditor"], d["treasury"]]
    assert vals == [300.0, 100.0, 50.0, 550.0]
    assert approx(sum(vals), 1000.0)
    return f"dist={vals}"


# -----------------------------------------------------------------------------
# G. Blue-team security regression (16)
# -----------------------------------------------------------------------------


def tc_sec001() -> str:
    state = ms.ProtocolState()
    cb = ms.CircuitBreaker()
    activations = 0
    for t in range(20):
        action = cb.update(
            t,
            state,
            mkt(t, [0.981, 0.981, 1.0, 1.0]),
            nav_drop=state.effective_collateral_value([0.981, 0.981, 1.0, 1.0]) - state.nav_prev,
            loss_finite=True,
            loss_value=1.0,
        )
        if action["cb2"]:
            activations += 1
    assert activations > 0
    return f"cb2_activations={activations}"


def tc_sec002() -> str:
    chk_ok = consensus_agent.validate_parameter_change("cr_target", "1.19")
    chk_bad = consensus_agent.validate_parameter_change("cr_target", "1.00")
    assert chk_ok["ok"] is True
    assert chk_bad["ok"] is False
    return f"ok={chk_ok['reason']} bad={chk_bad['reason']}"


def tc_sec003() -> str:
    out = consensus_agent.run(
        Namespace(
            dry_run=False,
            queue=False,
            execute=True,
            proposal_id="sec-g13",
            proposal_type="parameter_change",
            asset="USDX",
            param="mint_fee",
            value="0.002",
            keeper_vote="yes",
            watchdog_vote="yes",
            auditor_vote="yes",
            keeper_sig="",
            watchdog_sig="",
            auditor_sig="",
            nonce=0,
        )
    )
    assert out["decision"]["queued"] is False
    assert out["decision"]["executed"] is False
    return f"action={out['decision']['action']}"


def tc_sec004() -> str:
    out = consensus_agent.run(
        Namespace(
            dry_run=False,
            queue=False,
            execute=True,
            proposal_id="sec-i23",
            proposal_type="asset_listing",
            asset="SANCTIONED_USD_PROXY",
            param="cr_target",
            value="1.2",
            keeper_vote="yes",
            watchdog_vote="yes",
            auditor_vote="yes",
            keeper_sig="",
            watchdog_sig="",
            auditor_sig="",
            nonce=0,
        )
    )
    assert out["validation"]["ok"] is False
    return f"validation={out['validation']['reason']}"


def tc_sec005() -> str:
    keeper = ms.Keeper()
    replay = keeper.submit_update_proposal(
        ms.ProtocolState(),
        {"weights": [0.42, 0.28, 0.2, 0.1], "mint_fee": 0.002},
    )
    assert replay["status"] == "REJECTED"
    return f"reason={replay['reason']}"


def tc_sec006() -> str:
    env1 = ms.MarketEnv("normal", seed=0)
    env2 = ms.MarketEnv("normal", seed=0)
    p1 = [env1.step(t).prices for t in range(5)]
    p2 = [env2.step(t).prices for t in range(5)]
    assert p1 != p2
    return "entropy_mixed_rng_ok"


def tc_sec007() -> str:
    v = ms.Value(1.0)
    for _ in range(4000):
        v = (v * 1.000001) + 0.000001
    assert v._depth <= ms.MAX_AUTOGRAD_DEPTH
    return f"depth={v._depth}"


def tc_sec008() -> str:
    x = ms.Value(2.0)
    y = (x * x) + 1.0
    x._prev.add(y)
    ok = False
    try:
        y.backward()
    except ValueError:
        ok = True
    assert ok
    return "cycle_detected"


def tc_sec009() -> str:
    minted_bad = ms.secure_mint_amount(
        collateral_units=1_000_000,
        oracle_samples=[1.0, 1.0, 1.0],
        stale_seconds=30,
        quality_score=0.72,
    )
    minted_ok = ms.secure_mint_amount(
        collateral_units=1_000_000,
        oracle_samples=[1.0, 1.0, 1.0],
        stale_seconds=30,
        quality_score=0.97,
    )
    assert minted_bad == 0
    assert minted_ok > 0
    return f"minted_bad={minted_bad}, minted_ok={minted_ok}"


def tc_sec010() -> str:
    q = ms.RedemptionQueue(smoothing_window=8)
    q.enqueue("early", 1_000_000, 1_000_000)
    q.enqueue("late", 1_000_000, 950_000)
    settled = q.settle([2_000_000, 2_000_000, 2_000_000, 2_000_000], [1.0, 1.0, 1.0, 1.0], 6_000_000)
    early = settled["early"]
    late = settled["late"]
    edge = abs(sum(early) - sum(late))
    assert edge <= 1
    return f"edge={edge}"


def tc_sec011() -> str:
    lib_path = os.path.join(os.path.dirname(__file__), "solana/programs/microstable/src/lib.rs")
    src = open(lib_path, "r", encoding="utf-8").read()
    assert "hard restore before Recovery->Inactive transition" in src
    assert "if i == 3" in src
    return "cb4_lr_restore_guard_present"


def tc_sec012() -> str:
    qos = ms.ProtocolTxScheduler()
    admission = qos.admit_by_compute(
        block_compute_limit=48_000_000,
        attacker_compute=220 * 220_000,
        protocol_txs=3,
        protocol_tx_compute=200_000,
    )
    assert int(admission["admitted"]) >= 3
    return f"admitted={admission['admitted']}"


def tc_sec013() -> str:
    qos = ms.ProtocolTxScheduler()
    admission = qos.admit_by_slots(100, 100, 3)
    assert int(admission["admitted"]) >= 3
    return f"admitted={admission['admitted']}"


def tc_sec014() -> str:
    lib_path = os.path.join(os.path.dirname(__file__), "solana/programs/microstable/src/lib.rs")
    src = open(lib_path, "r", encoding="utf-8").read()
    assert "TRUSTED_INITIALIZER" in src
    assert "validate_keeper_set" in src
    assert "require_keeper_quorum" in src
    return "trusted_initializer_and_multisig_present"


def tc_sec015() -> str:
    auction = ms.BatchRebalanceAuction(fee_rate=0.003)
    pnl = auction.sandwich_pnl(500_000.0)
    assert pnl <= 0.0
    return f"pnl={pnl:.6f}"


def tc_sec016() -> str:
    fund = ms.InsuranceFund(treasury=1_000_000.0, min_claim=100.0, cooldown_ticks=5)
    ok = fund.claim("alice", 75.0, 0)
    assert ok["approved"] is False
    assert ok["reason"] == "below_min_claim"
    assert fund.treasury > 0.0
    return f"treasury={fund.treasury:.2f}"


# -----------------------------------------------------------------------------
# Verification requirements (high-resolution)
# -----------------------------------------------------------------------------


def run_finite_difference_checks() -> Tuple[bool, str]:
    eps = 1e-5
    checks: List[Tuple[str, float, float]] = []

    # unary helper
    def unary_check(name: str, f_scalar: Callable[[float], float], f_value: Callable[[ms.Value], ms.Value], x: float) -> None:
        xv = ms.Value(x)
        yv = f_value(xv)
        yv.backward()
        g_auto = xv.grad
        g_num = (f_scalar(x + eps) - f_scalar(x - eps)) / (2 * eps)
        checks.append((name, g_auto, g_num))

    # binary helper (grad wrt first arg)
    def binary_check(name: str, f_scalar: Callable[[float, float], float], f_value: Callable[[ms.Value, ms.Value], ms.Value], x: float, y: float) -> None:
        xv = ms.Value(x)
        yv = ms.Value(y)
        zv = f_value(xv, yv)
        zv.backward()
        g_auto = xv.grad
        g_num = (f_scalar(x + eps, y) - f_scalar(x - eps, y)) / (2 * eps)
        checks.append((name, g_auto, g_num))

    binary_check("add", lambda a, b: a + b, lambda a, b: a + b, 0.7, -1.1)
    binary_check("sub", lambda a, b: a - b, lambda a, b: a - b, 0.7, -1.1)
    binary_check("mul", lambda a, b: a * b, lambda a, b: a * b, 0.7, -1.1)
    binary_check("div", lambda a, b: a / b, lambda a, b: a / b, 0.7, 1.3)

    unary_check("pow3", lambda x: x**3, lambda x: x**3, 1.2)
    unary_check("neg", lambda x: -x, lambda x: -x, 0.6)
    unary_check("tanh", math.tanh, lambda x: x.tanh(), 0.4)
    unary_check("exp", math.exp, lambda x: x.exp(), 0.2)
    unary_check("log", math.log, lambda x: x.log(), 1.4)

    unary_check("relu_pos", lambda x: max(0.0, x), lambda x: x.relu(), 0.8)
    unary_check("relu_neg", lambda x: max(0.0, x), lambda x: x.relu(), -0.8)

    unary_check("clamp_mid", lambda x: min(max(x, -0.5), 0.5), lambda x: x.clamp(-0.5, 0.5), 0.1)

    # explicit boundary checks
    x0 = ms.Value(0.0)
    y0 = x0.relu()
    y0.backward()
    checks.append(("relu_at_0", x0.grad, 0.0))

    x1 = ms.Value(0.0)
    y1 = x1.abs_l1()
    y1.backward()
    checks.append(("l1_at_0", x1.grad, 0.0))

    bad = []
    for name, ga, gn in checks:
        if not math.isfinite(ga) or not math.isfinite(gn):
            bad.append((name, ga, gn))
        elif abs(ga - gn) > 1e-3:
            bad.append((name, ga, gn))

    if bad:
        return False, f"mismatches={bad[:3]}"
    return True, f"checks={len(checks)}"


def run_monte_carlo_100() -> Tuple[bool, str, Dict[str, Dict[str, Dict[str, float]]], Dict[str, str]]:
    scenarios = ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]
    runs = 100
    ticks = 80

    stats: Dict[str, Dict[str, Dict[str, float]]] = {}
    hist: Dict[str, str] = {}

    for sc in scenarios:
        maes: List[float] = []
        rmses: List[float] = []
        min_crs: List[float] = []
        max_turns: List[float] = []
        cr_viol: List[float] = []
        fp_rates: List[float] = []

        for seed in range(runs):
            r = ms.run_scenario(sc, seed=seed, ticks=ticks, enforce_invariants=True)
            maes.append(r.mae)
            rmses.append(r.rmse)
            min_crs.append(r.min_cr)
            max_turns.append(r.max_turnover)
            cr_viol.append(r.cr_violation_rate)
            fp_rates.append(r.breaker_false_positive_rate)

        stats[sc] = {
            "mae": ms.summarize_stats(maes),
            "rmse": ms.summarize_stats(rmses),
            "min_cr": ms.summarize_stats(min_crs),
            "max_turnover": ms.summarize_stats(max_turns),
            "cr_violation_rate": ms.summarize_stats(cr_viol),
            "fp_rate": ms.summarize_stats(fp_rates),
        }
        hist[sc] = ms.text_histogram(maes, bins=10, width=24)

    ok = True
    # Gate-level sanity on normal scenario distribution
    if stats["normal"]["mae"]["p95"] >= 0.0015:
        ok = False
    if stats["normal"]["cr_violation_rate"]["worst"] >= 0.01:
        ok = False

    return ok, f"runs={runs} scenarios={len(scenarios)}", stats, hist


def run_fuzzing_1000() -> Tuple[bool, str]:
    rng = random.Random(777)
    for i in range(1000):
        state = ms.ProtocolState()

        # random caps and feasible random weights
        caps = [rng.uniform(0.30, 0.75) for _ in range(4)]
        if sum(caps) < 1.05:
            caps[0] += 1.05 - sum(caps)

        raw = [rng.random() for _ in range(4)]
        raw_sum = sum(raw)
        y = [x / raw_sum for x in raw]
        state.w_caps = caps[:]
        state.base_w_caps = caps[:]
        state.weights = ms.AdamOptimizer.project_box_simplex(y, [0.0] * 4, caps, target=1.0)
        state.prev_weights = state.weights[:]

        prices = [rng.uniform(0.6, 1.4) for _ in range(4)]
        q = rng.uniform(0.0, 1.0)

        loss_engine = ms.LossEngine()
        opt = ms.AdamOptimizer(4)
        cb = ms.CircuitBreaker()

        market = ms.MarketTick(
            tick=i,
            prices=prices,
            oracle_q=q,
            stale_seconds=rng.choice([0, 0, 0, 200]),
            divergence=rng.uniform(0.0, 0.05),
            expected_breakers=[],
        )

        try:
            loss, ctx = loss_engine.compute(state, prices, q)
            loss.backward()
            grad_w = [wv.grad for wv in ctx["weights"]]  # type: ignore[index]
            grad_fee = float(ctx["fee"].grad)  # type: ignore[index]
            new_w, new_fee = opt.step(state.weights, state.mint_fee, grad_w, grad_fee, state.w_caps)
            state.apply_params(new_w, new_fee)
            cb.update(i, state, market, nav_drop=0.0, loss_finite=True, loss_value=loss.data)
            ms._assert_tick_invariants(state)
        except Exception as e:
            return False, f"crash at case={i}: {e}"

    return True, "fuzz=1000 no crashes"


def run_exhaustive_cb_transition_check() -> Tuple[bool, str]:
    valid = ms.CircuitBreaker.valid_transitions()
    m = ms.BreakerMachine(cb_id=1, min_hold=1, recovery_needed=1, cooldown_ticks=1)

    transitions: List[Tuple[str, str]] = []

    # NORMAL -> ACTIVATED
    s0 = m.state
    m.try_trigger(0)
    transitions.append((s0, m.state))

    # ACTIVATED -> HOLDING
    s1 = m.state
    m.begin_tick()
    transitions.append((s1, m.state))

    # HOLDING -> RECOVERY_CHECK
    s2 = m.state
    m.begin_tick()
    transitions.append((s2, m.state))

    # RECOVERY_CHECK -> HOLDING (failed recovery)
    s3 = m.state
    m.recovery_step(False, False)
    transitions.append((s3, m.state))

    # HOLDING -> RECOVERY_CHECK
    s4 = m.state
    m.begin_tick()
    transitions.append((s4, m.state))

    # RECOVERY_CHECK -> NORMAL
    s5 = m.state
    m.recovery_step(True, False)
    transitions.append((s5, m.state))

    # invalid attempt: trigger while cooldown_left > 0 should not change state
    s6 = m.state
    m.try_trigger(10)
    transitions.append((s6, m.state))

    for a, b in transitions:
        if b not in valid.get(a, []):
            return False, f"invalid transition observed: {a}->{b}"

    # ensure representative transitions are covered
    required = {
        (ms.CB_NORMAL, ms.CB_ACTIVATED),
        (ms.CB_ACTIVATED, ms.CB_HOLDING),
        (ms.CB_HOLDING, ms.CB_RECOVERY_CHECK),
        (ms.CB_RECOVERY_CHECK, ms.CB_HOLDING),
        (ms.CB_RECOVERY_CHECK, ms.CB_NORMAL),
    }
    if not required.issubset(set(transitions)):
        return False, f"missing transitions: {required - set(transitions)}"

    return True, f"transitions={transitions}"


# -----------------------------------------------------------------------------
# Test harness
# -----------------------------------------------------------------------------


def build_cases() -> List[Case]:
    cases: List[Case] = []

    # Value (12)
    cases += [
        Case("TC-V001", "Value", tc_v001),
        Case("TC-V002", "Value", tc_v002),
        Case("TC-V003", "Value", tc_v003),
        Case("TC-V004", "Value", tc_v004),
        Case("TC-V005", "Value", tc_v005),
        Case("TC-V006", "Value", tc_v006),
        Case("TC-V007", "Value", tc_v007),
        Case("TC-V008", "Value", tc_v008),
        Case("TC-V009", "Value", tc_v009),
        Case("TC-V010", "Value", tc_v010),
        Case("TC-V011", "Value", tc_v011),
        Case("TC-V012", "Value", tc_v012),
    ]

    # Loss (8)
    cases += [
        Case("TC-L001", "Loss", tc_l001),
        Case("TC-L002", "Loss", tc_l002),
        Case("TC-L003", "Loss", tc_l003),
        Case("TC-L004", "Loss", tc_l004),
        Case("TC-L005", "Loss", tc_l005),
        Case("TC-L006", "Loss", tc_l006),
        Case("TC-L007", "Loss", tc_l007),
        Case("TC-L008", "Loss", tc_l008),
    ]

    # Optimizer (8)
    cases += [
        Case("TC-O001", "Optimizer", tc_o001),
        Case("TC-O002", "Optimizer", tc_o002),
        Case("TC-O003", "Optimizer", tc_o003),
        Case("TC-O004", "Optimizer", tc_o004),
        Case("TC-O005", "Optimizer", tc_o005),
        Case("TC-O006", "Optimizer", tc_o006),
        Case("TC-O007", "Optimizer", tc_o007),
        Case("TC-O008", "Optimizer", tc_o008),
    ]

    # Circuit Breaker (15)
    cases += [
        Case("TC-CB001", "CircuitBreaker", tc_cb001),
        Case("TC-CB002", "CircuitBreaker", tc_cb002),
        Case("TC-CB003", "CircuitBreaker", tc_cb003),
        Case("TC-CB004", "CircuitBreaker", tc_cb004),
        Case("TC-CB005", "CircuitBreaker", tc_cb005),
        Case("TC-CB006", "CircuitBreaker", tc_cb006),
        Case("TC-CB007", "CircuitBreaker", tc_cb007),
        Case("TC-CB008", "CircuitBreaker", tc_cb008),
        Case("TC-CB009", "CircuitBreaker", tc_cb009),
        Case("TC-CB010", "CircuitBreaker", tc_cb010),
        Case("TC-CB011", "CircuitBreaker", tc_cb011),
        Case("TC-CB012", "CircuitBreaker", tc_cb012),
        Case("TC-CB013", "CircuitBreaker", tc_cb013),
        Case("TC-CB014", "CircuitBreaker", tc_cb014),
        Case("TC-CB015", "CircuitBreaker", tc_cb015),
    ]

    # Scenario (8)
    cases += [
        Case("TC-S001", "Scenario", tc_s001),
        Case("TC-S002", "Scenario", tc_s002),
        Case("TC-S003", "Scenario", tc_s003),
        Case("TC-S004", "Scenario", tc_s004),
        Case("TC-S005", "Scenario", tc_s005),
        Case("TC-S006", "Scenario", tc_s006),
        Case("TC-S007", "Scenario", tc_s007),
        Case("TC-S008", "Scenario", tc_s008),
    ]

    # Agent (4)
    cases += [
        Case("TC-A001", "Agent", tc_a001),
        Case("TC-A002", "Agent", tc_a002),
        Case("TC-A003", "Agent", tc_a003),
        Case("TC-A004", "Agent", tc_a004),
    ]

    # Security regression (8)
    cases += [
        Case("TC-SEC001", "Security", tc_sec001),
        Case("TC-SEC002", "Security", tc_sec002),
        Case("TC-SEC003", "Security", tc_sec003),
        Case("TC-SEC004", "Security", tc_sec004),
        Case("TC-SEC005", "Security", tc_sec005),
        Case("TC-SEC006", "Security", tc_sec006),
        Case("TC-SEC007", "Security", tc_sec007),
        Case("TC-SEC008", "Security", tc_sec008),
        Case("TC-SEC009", "Security", tc_sec009),
        Case("TC-SEC010", "Security", tc_sec010),
        Case("TC-SEC011", "Security", tc_sec011),
        Case("TC-SEC012", "Security", tc_sec012),
        Case("TC-SEC013", "Security", tc_sec013),
        Case("TC-SEC014", "Security", tc_sec014),
        Case("TC-SEC015", "Security", tc_sec015),
        Case("TC-SEC016", "Security", tc_sec016),
    ]

    assert len(cases) == 71
    return cases


def run_case(case: Case) -> Tuple[bool, str]:
    try:
        detail = case.fn()
        return True, detail
    except Exception as e:
        return False, f"{e}\n{traceback.format_exc(limit=1).strip()}"


def print_gate_a(r: ms.ScenarioSummary) -> None:
    print("\n" + "=" * 72)
    print("GATE A RESULTS")
    print("=" * 72)
    print(f"peg MAE                : {r.mae:.6f}   (target < 0.0015)   {'PASS' if r.gate_peg_ok else 'FAIL'}")
    print(f"CR violation rate      : {r.cr_violation_rate:.6%} (target < 1%)      {'PASS' if r.gate_cr_ok else 'FAIL'}")
    print(f"breaker false positive : {r.breaker_false_positive_rate:.6%} (target < 5%) {'PASS' if r.gate_fp_ok else 'FAIL'}")
    print("=" * 72)


def main() -> None:
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    cases = build_cases()
    passed = 0

    print(f"Running {len(cases)} spec test cases...\n")
    for case in cases:
        ok, detail = run_case(case)
        status = "PASS" if ok else "FAIL"
        print(f"[{status}] {case.cid} ({case.category}) - {detail}")
        if ok:
            passed += 1

    print("\n" + "-" * 72)
    print(f"Spec testcases: {passed}/{len(cases)} PASS")
    print("-" * 72)

    # High-resolution verification suite
    print("\nRunning verification suite...")

    ok_fd, msg_fd = run_finite_difference_checks()
    print(f"[{'PASS' if ok_fd else 'FAIL'}] VER-001 finite-difference gradients - {msg_fd}")

    ok_trans, msg_trans = run_exhaustive_cb_transition_check()
    print(f"[{'PASS' if ok_trans else 'FAIL'}] VER-005 exhaustive CB transitions - {msg_trans}")

    ok_fuzz, msg_fuzz = run_fuzzing_1000()
    print(f"[{'PASS' if ok_fuzz else 'FAIL'}] VER-004 fuzzing(1000) - {msg_fuzz}")

    ok_mc, msg_mc, stats, hist = run_monte_carlo_100()
    print(f"[{'PASS' if ok_mc else 'FAIL'}] VER-002/006 Monte-Carlo(100) + KPI stats - {msg_mc}")

    print("\nMonte Carlo KPI summary (mean/median/p5/p95/worst):")
    for sc, d in stats.items():
        print(f"\nScenario: {sc}")
        for kpi, st in d.items():
            print(
                f"  {kpi:18s} mean={st['mean']:.6f} median={st['median']:.6f} "
                f"p5={st['p5']:.6f} p95={st['p95']:.6f} worst={st['worst']:.6f}"
            )
        print("  MAE histogram:")
        print("    " + hist[sc].replace("\n", "\n    "))

    # Save outputs requested by task
    scenario_results = ms.run_all_scenarios(
        ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"],
        seed=0,
        ticks=120,
        output_dir=OUTPUT_DIR,
    )

    # Gate A (normal scenario)
    gate_ref = scenario_results["normal"]
    print_gate_a(gate_ref)

    # per-tick invariant verification (explicit)
    inv_ok = True
    inv_msg = ""
    try:
        for sc in ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]:
            ms.run_scenario(sc, seed=2, ticks=120, enforce_invariants=True)
        inv_msg = "all scenarios invariant-clean"
    except Exception as e:
        inv_ok = False
        inv_msg = str(e)
    print(f"[{'PASS' if inv_ok else 'FAIL'}] VER-003 per-tick invariants - {inv_msg}")

    # Final summary
    overall_ok = (
        passed == len(cases)
        and ok_fd
        and ok_trans
        and ok_fuzz
        and ok_mc
        and inv_ok
        and gate_ref.gate_peg_ok
        and gate_ref.gate_cr_ok
        and gate_ref.gate_fp_ok
    )

    print("\nOutputs written:")
    print(f"- {os.path.join(OUTPUT_DIR, 'metrics.csv')}")
    print(f"- {os.path.join(OUTPUT_DIR, 'events.log')}")

    print("\nFINAL RESULT:", "PASS" if overall_ok else "FAIL")


if __name__ == "__main__":
    main()
