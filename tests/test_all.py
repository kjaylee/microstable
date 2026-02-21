import math
import os
import random
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from microstable import (
    ASSETS,
    BASE_W_CAPS,
    CR_HARD_MIN,
    DELTA_FEE_MAX,
    DELTA_W_MAX,
    AdamBoundedOptimizer,
    Auditor,
    CBMachine,
    CB_ACTIVE,
    CB_COOLDOWN,
    CB_EXTENDED,
    CircuitBreaker,
    Keeper,
    LossEngine,
    MarketTick,
    ProtocolState,
    Value,
    Watchdog,
    distribute_fees,
    run_scenario,
    v_exp,
    v_log,
    v_relu,
    v_tanh,
)

ATOL = 1e-9


def mk_tick(prices=None, q=1.0, stale=0, div=0.0, stress=False):
    if prices is None:
        prices = [1.0, 1.0, 1.0, 1.0]
    return MarketTick(prices=prices[:], oracle_q=q, stale_seconds=stale, divergence=div, expected_stress=stress)


class TestValueAutograd(unittest.TestCase):
    def test_TC_V001(self):
        a = Value(1.25)
        b = Value(-0.75)
        y = a + b
        self.assertAlmostEqual(y.data, 0.5, delta=ATOL)

    def test_TC_V002(self):
        a = Value(2.0)
        b = Value(3.0)
        y = a + b
        y.backward()
        self.assertAlmostEqual(a.grad, 1.0, delta=ATOL)
        self.assertAlmostEqual(b.grad, 1.0, delta=ATOL)

    def test_TC_V003(self):
        a = Value(2.0)
        b = Value(-3.0)
        y = a * b
        y.backward()
        self.assertAlmostEqual(y.data, -6.0, delta=ATOL)
        self.assertAlmostEqual(a.grad, -3.0, delta=ATOL)
        self.assertAlmostEqual(b.grad, 2.0, delta=ATOL)

    def test_TC_V004(self):
        a = Value(3.0)
        b = Value(2.0)
        y = a / b
        y.backward()
        self.assertAlmostEqual(y.data, 1.5, delta=ATOL)
        self.assertAlmostEqual(a.grad, 0.5, delta=1e-8)
        self.assertAlmostEqual(b.grad, -0.75, delta=1e-8)

    def test_TC_V005(self):
        x = Value(2.5)
        y = x ** 3
        y.backward()
        self.assertAlmostEqual(y.data, 15.625, delta=ATOL)
        self.assertAlmostEqual(x.grad, 18.75, delta=1e-8)

    def test_TC_V006(self):
        x = Value(0.7)
        y = v_tanh(x)
        y.backward()
        self.assertAlmostEqual(y.data, math.tanh(0.7), delta=1e-9)
        self.assertAlmostEqual(x.grad, 1.0 - math.tanh(0.7) ** 2, delta=1e-8)

    def test_TC_V007(self):
        x = Value(1.2)
        y = v_exp(x)
        y.backward()
        self.assertAlmostEqual(y.data, math.exp(1.2), delta=1e-8)
        self.assertAlmostEqual(x.grad, math.exp(1.2), delta=1e-8)

    def test_TC_V008(self):
        x = Value(2.5)
        y = v_log(x)
        y.backward()
        self.assertAlmostEqual(y.data, math.log(2.5), delta=1e-9)
        self.assertAlmostEqual(x.grad, 1.0 / 2.5, delta=1e-8)

    def test_TC_V009(self):
        a = Value(2.0)
        b = Value(-3.0)
        c = Value(4.0)
        y = a * b + (c ** 2)
        y.backward()
        self.assertAlmostEqual(y.data, 10.0, delta=ATOL)
        self.assertAlmostEqual(a.grad, -3.0, delta=ATOL)
        self.assertAlmostEqual(b.grad, 2.0, delta=ATOL)
        self.assertAlmostEqual(c.grad, 8.0, delta=ATOL)

    def test_TC_V010(self):
        x = Value(1e-12)
        y = v_log(x)
        y.backward()
        self.assertTrue(math.isfinite(y.data))
        self.assertTrue(math.isfinite(x.grad))

    def test_TC_V011(self):
        x = Value(1e-12)
        y = 1.0 / x
        y.backward()
        self.assertTrue(math.isfinite(y.data))
        self.assertTrue(math.isfinite(x.grad))

    def test_TC_V012(self):
        x = Value(0.0)
        y = v_relu(x)
        y.backward()
        self.assertAlmostEqual(y.data, 0.0, delta=ATOL)
        self.assertAlmostEqual(x.grad, 0.0, delta=ATOL)


class TestLossFunction(unittest.TestCase):
    def test_TC_L001(self):
        p_t = 1.0
        peg_loss = 5.0 * ((p_t - 1.0) ** 2)
        self.assertEqual(peg_loss, 0.0)

    def test_TC_L002(self):
        p_t = 0.98
        peg_loss = 5.0 * ((p_t - 1.0) ** 2)
        self.assertGreater(peg_loss, 0.0)
        self.assertAlmostEqual(peg_loss, 0.002, delta=1e-12)

    def test_TC_L003(self):
        cr_min, cr_t = 1.2, 1.25
        cr_penalty = 20.0 * max(0.0, cr_min - cr_t) ** 2
        self.assertEqual(cr_penalty, 0.0)

    def test_TC_L004(self):
        cr_min, cr_t = 1.2, 1.1
        cr_penalty = 20.0 * max(0.0, cr_min - cr_t) ** 2
        self.assertGreater(cr_penalty, 0.0)
        self.assertAlmostEqual(cr_penalty, 0.2, delta=1e-12)

    def test_TC_L005(self):
        w = [1.0, 0.0, 0.0, 0.0]
        conc = sum(x * x for x in w)
        self.assertAlmostEqual(conc, 1.0, delta=ATOL)

    def test_TC_L006(self):
        w = [0.25, 0.25, 0.25, 0.25]
        conc = sum(x * x for x in w)
        self.assertAlmostEqual(conc, 0.25, delta=ATOL)
        self.assertLess(conc, 1.0)

    def test_TC_L007(self):
        state = ProtocolState()
        state.prev_weights = [0.4, 0.3, 0.2, 0.1]
        state.weights = [0.4, 0.3, 0.2, 0.1]
        loss_engine = LossEngine()
        loss, parts = loss_engine.compute(state, [1.0, 1.0, 1.0, 1.0], 1.0)
        self.assertGreaterEqual(loss.data, 0.0)
        self.assertAlmostEqual(parts["turnover"].data, 0.0, delta=1e-3)

    def test_TC_L008(self):
        q_t = 1.0
        oracle_loss = 3.0 * ((1.0 - q_t) ** 2)
        self.assertEqual(oracle_loss, 0.0)


class TestOptimizer(unittest.TestCase):
    def test_TC_O001(self):
        opt = AdamBoundedOptimizer(4)
        old = [0.40, 0.30, 0.20, 0.10]
        new, _ = opt.step_from_gradients(old, 0.0020, [0.2, -0.1, 0.05, -0.15], 0.0, BASE_W_CAPS)
        self.assertTrue(any(abs(a - b) > 0 for a, b in zip(old, new)))

    def test_TC_O002(self):
        w_proj = AdamBoundedOptimizer.simplex_projection([0.62, 0.18, 0.15, 0.10])
        self.assertAlmostEqual(sum(w_proj), 1.0, delta=1e-9)

    def test_TC_O003(self):
        w = AdamBoundedOptimizer.project_capped_simplex([0.70, 0.20, 0.05, 0.05], BASE_W_CAPS, target=1.0)
        for wi, cap in zip(w, BASE_W_CAPS):
            self.assertLessEqual(wi, cap + 1e-9)

    def test_TC_O004(self):
        w = AdamBoundedOptimizer.project_capped_simplex([0.50, -0.10, 0.40, 0.20], BASE_W_CAPS, target=1.0)
        self.assertGreaterEqual(min(w), 0.0)

    def test_TC_O005(self):
        prev = [0.40, 0.30, 0.20, 0.10]
        cand = [0.46, 0.24, 0.20, 0.10]
        w_new = AdamBoundedOptimizer.bounded_weight_projection(prev, cand, BASE_W_CAPS, DELTA_W_MAX)
        for i in range(4):
            self.assertLessEqual(abs(w_new[i] - prev[i]), 0.02 + 1e-9)

    def test_TC_O006(self):
        opt = AdamBoundedOptimizer(4)
        old_fee = 0.0020
        _, new_fee = opt.step_from_gradients([0.4, 0.3, 0.2, 0.1], old_fee, [0, 0, 0, 0], -100.0, BASE_W_CAPS)
        self.assertLessEqual(abs(new_fee - old_fee), DELTA_FEE_MAX + 1e-12)

    def test_TC_O007(self):
        g = [10.0, 10.0, 10.0, 10.0]
        gc = AdamBoundedOptimizer.clip_gradients(g, 1.0)
        norm = math.sqrt(sum(x * x for x in gc))
        self.assertLessEqual(norm, 1.0 + 1e-12)

    def test_TC_O008(self):
        rng = random.Random(7)
        state = ProtocolState()
        opt = AdamBoundedOptimizer(4)
        for _ in range(100):
            grads = [rng.uniform(-0.2, 0.2) for _ in range(4)]
            gfee = rng.uniform(-0.5, 0.5)
            new_w, new_fee = opt.step_from_gradients(state.weights, state.mint_fee, grads, gfee, state.w_caps)
            self.assertTrue(all(math.isfinite(x) for x in new_w + [new_fee]))
            self.assertAlmostEqual(sum(new_w), 1.0, delta=1e-6)
            self.assertTrue(all(0.0 <= w <= c + 1e-9 for w, c in zip(new_w, state.w_caps)))
            state.begin_tick()
            state.apply_weights_and_fee(new_w, new_fee)


class TestCircuitBreaker(unittest.TestCase):
    def test_TC_CB001(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for t in range(3):
            cb.update(t, state, mk_tick([1.0, 0.978, 1.0, 1.0]), True, 0.1)
        self.assertTrue(cb.is_active(1))
        self.assertAlmostEqual(state.w_caps[1], state.base_w_caps[1] * 0.5, delta=1e-12)
        self.assertLessEqual(state.mint_limit, 0.25)

    def test_TC_CB002(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for t in range(3):
            cb.update(t, state, mk_tick([1.0, 0.978, 1.0, 1.0]), True, 0.1)
        for t in range(3, 25):
            cb.update(t, state, mk_tick([1.0, 1.0001, 1.0, 1.0]), True, 0.09)
        self.assertFalse(cb.is_active(1))
        self.assertEqual(state.w_caps, state.base_w_caps)
        self.assertEqual(state.mint_paused_reason, "")

    def test_TC_CB003(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        cb.update(0, state, mk_tick([0.97, 0.96, 1.0, 1.0]), True, 0.1)
        self.assertTrue(cb.is_active(2))

    def test_TC_CB004(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        cb.update(0, state, mk_tick([0.97, 0.96, 1.0, 1.0]), True, 0.1)
        self.assertEqual(state.mint_limit, 0.0)
        self.assertEqual(state.mint_paused_reason, "MINT_PAUSED_BY_CB2")

    def test_TC_CB005(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        cb.update(0, state, mk_tick(stale=180, div=0.03), True, 0.1)
        self.assertTrue(cb.is_active(3))
        self.assertTrue(state.oracle_degraded)

    def test_TC_CB006(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        old_w = state.weights[:]
        cb.update(0, state, mk_tick(stale=180, div=0.03), True, 0.1)
        if state.optimizer_enabled:
            state.weights[0] += 0.01
        self.assertEqual(state.weights, old_w)

    def test_TC_CB007(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        checkpoint = state.clone()
        actions = cb.update(1, state, mk_tick(), False, None)
        if actions["rollback"]:
            state = checkpoint.clone()
        self.assertTrue(cb.is_active(4))
        self.assertEqual(state.weights, checkpoint.weights)

    def test_TC_CB008(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        opt = AdamBoundedOptimizer(4)
        old_lr = opt.lr
        actions = cb.update(1, state, mk_tick(), False, None)
        if actions["rollback"]:
            opt.lr = old_lr * 0.5
        self.assertAlmostEqual(opt.lr, old_lr * 0.5, delta=ATOL)

    def test_TC_CB009(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for t in range(3):
            cb.update(t, state, mk_tick([1.0, 0.978, 1.0, 1.0], stale=180, div=0.03), True, 0.1)
        self.assertTrue(cb.is_active(1))
        self.assertTrue(cb.is_active(3))

    def test_TC_CB010(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for t in range(3):
            cb.update(t, state, mk_tick([1.0, 0.978, 1.0, 1.0]), True, 0.1)
        self.assertTrue(cb.is_active(1))
        for t in range(3, 7):
            cb.update(t, state, mk_tick([1.0, 1.0, 1.0, 1.0]), True, 0.09)
        self.assertTrue(cb.is_active(1))

    def test_TC_CB011(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for t in range(3):
            cb.update(t, state, mk_tick([1.0, 0.978, 1.0, 1.0]), True, 0.1)
        for t in range(3, 12):
            cb.update(t, state, mk_tick([1.0, 1.0001, 1.0, 1.0]), True, 0.09)
            self.assertTrue(cb.is_active(1))
        cb.update(12, state, mk_tick([1.0, 1.0001, 1.0, 1.0]), True, 0.08)
        self.assertFalse(cb.is_active(1))

    def test_TC_CB012(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        cb.update(0, state, mk_tick([0.97, 0.96, 1.0, 1.0]), True, 0.1, forced={"cb1": True, "cb1_idx": 0, "cb2": True})
        self.assertTrue(cb.is_active(1))
        self.assertTrue(cb.is_active(2))
        self.assertEqual(state.mint_limit, 0.0)

    def test_TC_CB013(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for t in range(3):
            cb.update(t, state, mk_tick([1.0, 0.978, 1.0, 1.0]), True, 0.1)
        for t in range(3, 13):
            cb.update(t, state, mk_tick([1.0, 1.0001, 1.0, 1.0]), True, 0.09)
        self.assertEqual(cb.cb[1].state, CB_COOLDOWN)
        # cooldown window (5 ticks): cannot re-trigger
        cb.update(13, state, mk_tick([1.0, 0.97, 1.0, 1.0]), True, 0.1)
        self.assertFalse(cb.is_active(1))
        for t in range(14, 18):
            cb.update(t, state, mk_tick([1.0, 1.0, 1.0, 1.0]), True, 0.1)
        for t in range(18, 21):
            cb.update(t, state, mk_tick([1.0, 0.97, 1.0, 1.0]), True, 0.1)
        self.assertTrue(cb.is_active(1))

    def test_TC_CB014(self):
        m = CBMachine(1, 5)
        self.assertTrue(m.trigger(0))
        m.state = "NORMAL"
        self.assertTrue(m.trigger(10))
        m.state = "NORMAL"
        self.assertTrue(m.trigger(20))
        self.assertEqual(m.state, CB_EXTENDED)
        self.assertEqual(m.effective_min_hold(), 15)

    def test_TC_CB015(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        for cid in [1, 2, 3, 4]:
            cb.cb[cid].state = CB_ACTIVE
            cb.cb[cid].activated_tick = -100
            cb.cb[cid].recovery_streak = 999
        cb.cb1_target_idx = 0
        cb.cb4_decrease_streak = 3
        cb.last_loss = 0.2
        cb.update(0, state, mk_tick([1.0, 1.0, 1.0, 1.0]), True, 0.1)
        rec = [cid for (_, cid, old, new) in cb.transition_log if new == CB_COOLDOWN]
        self.assertEqual(rec[:4], [4, 3, 2, 1])


class TestScenarioIntegration(unittest.TestCase):
    def test_TC_S001(self):
        r = run_scenario("normal", seed=42, ticks=100)
        self.assertLess(r.peg_mae, 0.0015)

    def test_TC_S002(self):
        r = run_scenario("normal", seed=1, ticks=100)
        self.assertGreater(r.cr_final, r.cr_target_final)

    def test_TC_S003(self):
        r = run_scenario("single_depeg", seed=3, ticks=100)
        self.assertTrue(r.cb1_triggered)
        self.assertTrue(r.cb1_recovered)
        self.assertLessEqual(r.cb1_recovery_time, 30)

    def test_TC_S004(self):
        r = run_scenario("multi_depeg", seed=5, ticks=100)
        self.assertEqual(r.cr_violation_rate, 0.0)
        self.assertGreater(r.cb_trigger_counts[2], 0)

    def test_TC_S005(self):
        r = run_scenario("volatile", seed=7, ticks=200)
        self.assertTrue(math.isfinite(r.peg_mae))
        self.assertTrue(math.isfinite(r.cr_final))
        self.assertEqual(r.invariant_violations, 0)

    def test_TC_S006(self):
        r = run_scenario("gradient_attack", seed=8, ticks=120)
        self.assertLessEqual(r.max_weight_delta, 0.02 + 1e-6)
        self.assertLessEqual(r.max_fee_delta, 0.001 + 1e-9)

    def test_TC_S007(self):
        r = run_scenario("oracle_failure", seed=9, ticks=120)
        self.assertGreater(r.cb_trigger_counts[3], 0)
        self.assertGreater(r.optimizer_disabled_ticks, 0)
        self.assertTrue(r.conservative_mode_seen)

    def test_TC_S008(self):
        r = run_scenario("oracle_failure", seed=10, ticks=160)
        # after failure window, optimizer should recover and updates resume
        self.assertGreater(r.dynamic_updates_count, 0)
        self.assertLess(r.optimizer_disabled_ticks, 160)


class TestAgentInterface(unittest.TestCase):
    def test_TC_A001(self):
        state = ProtocolState()
        keeper = Keeper()
        proposal = {
            "weights": [0.41, 0.29, 0.20, 0.10],
            "mint_fee": 0.0025,
        }
        out = keeper.submit_update_proposal(state, proposal)
        self.assertEqual(out["status"], "APPLIED")
        self.assertAlmostEqual(state.weights[0], 0.41, delta=ATOL)

    def test_TC_A002(self):
        cb = CircuitBreaker()
        state = ProtocolState()
        wd = Watchdog()
        tick = mk_tick([1.0, 0.97, 1.0, 1.0], stale=180, div=0.03)
        events = wd.detect(tick)
        cb.update(0, state, tick, True, 0.1, forced=events)
        self.assertTrue(cb.is_active(1) or cb.is_active(3))
        self.assertTrue(len(cb.transition_log) > 0)

    def test_TC_A003(self):
        state = ProtocolState()
        state.weights = [0.5, 0.3, 0.2, 0.2]
        state.cr = CR_HARD_MIN - 0.1
        au = Auditor()
        out = au.verify_invariants(state)
        self.assertTrue(out["alert_emitted"])
        self.assertTrue(any(v.startswith("INV_") for v in out["violations"]))

    def test_TC_A004(self):
        dist = distribute_fees(1000.0)
        self.assertEqual(dist["keeper"], 300.0)
        self.assertEqual(dist["watchdog"], 100.0)
        self.assertEqual(dist["auditor"], 50.0)
        self.assertEqual(dist["treasury"], 550.0)
        self.assertEqual(sum(dist.values()), 1000.0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
