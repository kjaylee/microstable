import math
import os
import random
import statistics
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from microstable import (
    CB_ACTIVE,
    CB_COOLDOWN,
    CB_EXTENDED,
    CB_NORMAL,
    CR_HARD_MIN,
    CircuitBreaker,
    LossEngine,
    ProtocolState,
    Value,
    percentile,
    run_scenario,
    v_exp,
    v_log,
    v_relu,
    v_tanh,
)


def numerical_gradient_check(func, x0, eps=1e-5, tol=1e-4):
    x = Value(x0)
    y = func(x)
    y.backward()
    analytical = x.grad

    yp = func(Value(x0 + eps)).data
    ym = func(Value(x0 - eps)).data
    numerical = (yp - ym) / (2.0 * eps)

    if abs(analytical - numerical) >= tol:
        raise AssertionError(f"Gradient mismatch at x={x0}: {analytical} vs {numerical}")


def run_gradient_suite():
    checks = 0

    for x0 in [0.2, 0.7, 1.4]:
        numerical_gradient_check(lambda x: x + 2.0, x0)
        checks += 1
    for x0 in [0.3, 1.1]:
        numerical_gradient_check(lambda x: x - 1.5, x0)
        checks += 1
    for x0 in [0.2, 0.6, 1.3]:
        numerical_gradient_check(lambda x: x * 1.7, x0)
        checks += 1
    for x0 in [0.3, 0.9, 1.5]:
        numerical_gradient_check(lambda x: x / 1.4, x0)
        checks += 1
    for x0 in [0.4, 0.8, 1.2]:
        numerical_gradient_check(lambda x: x ** 2.5, x0)
        checks += 1
    for x0 in [0.2, 0.9, 1.6]:
        numerical_gradient_check(lambda x: v_tanh(x), x0)
        checks += 1
    for x0 in [0.1, 0.5, 1.0]:
        numerical_gradient_check(lambda x: v_exp(x), x0)
        checks += 1
    for x0 in [0.2, 0.8, 1.5]:
        numerical_gradient_check(lambda x: v_log(x), x0)
        checks += 1
    for x0 in [0.2, 1.1]:
        numerical_gradient_check(lambda x: v_relu(x), x0)
        checks += 1

    # relu boundary exact check
    x = Value(0.0)
    y = v_relu(x)
    y.backward()
    assert x.grad == 0.0
    checks += 1

    # composite chain checks
    samples = [
        (1.2, -0.7, 0.8, 1.6),
        (0.9, 0.4, 1.1, 1.3),
        (1.7, -0.3, 0.6, 1.8),
    ]
    for a0, b0, c0, d0 in samples:
        a = Value(a0)
        b = Value(b0)
        c = Value(c0)
        d = Value(d0)
        z = a * b + (c ** 2) - (d / 1.5)
        z.backward()

        eps = 1e-5
        zp = (a0 + eps) * b0 + (c0 ** 2) - (d0 / 1.5)
        zm = (a0 - eps) * b0 + (c0 ** 2) - (d0 / 1.5)
        num = (zp - zm) / (2 * eps)
        assert abs(a.grad - num) < 1e-4
        checks += 1

    assert checks >= 20, f"need >=20 gradient points, got {checks}"
    return checks


def run_invariant_per_tick_suite():
    scenarios = ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]
    for sc in scenarios:
        r = run_scenario(sc, seed=42, ticks=120, enforce_invariants=True)
        assert r.invariant_violations == 0, f"Invariant violated in {sc}"


def run_monte_carlo_suite():
    scenarios = ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]
    stats = {}
    for scenario in scenarios:
        results = [run_scenario(scenario, seed=s, ticks=100, enforce_invariants=True) for s in range(100)]
        peg_maes = [r.peg_mae for r in results]
        cr_rates = [r.cr_violation_rate for r in results]
        fprs = [r.breaker_false_positive_rate for r in results]

        stats[scenario] = {
            "mean": statistics.mean(peg_maes),
            "std": statistics.pstdev(peg_maes),
            "p5": percentile(peg_maes, 5),
            "p50": statistics.median(peg_maes),
            "p95": percentile(peg_maes, 95),
            "worst": max(peg_maes),
            "cr_p95": percentile(cr_rates, 95),
            "fpr_p95": percentile(fprs, 95),
        }

    assert stats["normal"]["p95"] < 0.0015, f"normal peg p95 too high: {stats['normal']['p95']}"

    stress = ["single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]
    for sc in stress:
        assert stats[sc]["cr_p95"] < 0.01, f"{sc} CR breach p95 too high: {stats[sc]['cr_p95']}"
        assert stats[sc]["fpr_p95"] < 0.05, f"{sc} breaker FPR p95 too high: {stats[sc]['fpr_p95']}"

    return stats


def run_fuzz_suite():
    rng = random.Random(2026)
    loss_engine = LossEngine()
    for _ in range(1000):
        state = ProtocolState()
        prices = [rng.uniform(0.5, 1.5) for _ in range(4)]
        oracle_q = rng.uniform(0.0, 1.0)
        state.weights = [rng.random() for _ in range(4)]
        s = sum(state.weights)
        state.weights = [w / s for w in state.weights]
        state.prev_weights = state.weights[:]
        state.cr = rng.uniform(CR_HARD_MIN, 1.8)

        loss, ctx = loss_engine.compute(state, prices, oracle_q)
        loss.backward()

        vals = [loss.data, state.cr] + [w.grad for w in ctx["weights"]] + [ctx["fee"].grad]
        assert all(math.isfinite(v) for v in vals), "fuzz generated non-finite"


def run_cb_exhaustive_suite():
    states = [CB_NORMAL, CB_ACTIVE, CB_COOLDOWN, CB_EXTENDED]
    events = ["trigger", "recover", "cooldown_done", "escalate", "noop"]

    expected = {
        (CB_NORMAL, "trigger"): CB_ACTIVE,
        (CB_ACTIVE, "recover"): CB_COOLDOWN,
        (CB_EXTENDED, "recover"): CB_COOLDOWN,
        (CB_COOLDOWN, "cooldown_done"): CB_NORMAL,
        (CB_ACTIVE, "escalate"): CB_EXTENDED,
    }

    for s in states:
        for e in events:
            nxt = CircuitBreaker.preview_transition(s, e)
            exp = expected.get((s, e), s)
            assert nxt == exp, f"transition mismatch: {s} + {e} -> {nxt}, expected {exp}"

    # reachability check
    reachable = {CB_NORMAL}
    changed = True
    while changed:
        changed = False
        for s in list(reachable):
            for e in events:
                nxt = CircuitBreaker.preview_transition(s, e)
                if nxt not in reachable:
                    reachable.add(nxt)
                    changed = True
    assert reachable == set(states), f"unreachable states: {set(states) - reachable}"


if __name__ == "__main__":
    print("[1] Gradient numerical verification...")
    n = run_gradient_suite()
    print(f"  gradient checks: {n} points (PASS)")

    print("[2] Per-tick invariants...")
    run_invariant_per_tick_suite()
    print("  invariants per tick PASS")

    print("[3] Monte Carlo (100 seeds x 6 scenarios)...")
    stats = run_monte_carlo_suite()
    for sc, st in stats.items():
        print(f"  {sc}: peg MAE={st['mean']:.6f} ± {st['std']:.6f}, p95={st['p95']:.6f}, worst={st['worst']:.6f}, cr_p95={st['cr_p95']:.4f}, fpr_p95={st['fpr_p95']:.4f}")

    print("[4] Edge-case fuzzing (1000 inputs)...")
    run_fuzz_suite()
    print("  fuzz PASS")

    print("[5] CB exhaustive transition scan...")
    run_cb_exhaustive_suite()
    print("  CB exhaustive PASS")

    print("ALL VERIFICATION CHECKS PASSED")
