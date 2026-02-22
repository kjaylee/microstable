#!/usr/bin/env python3
"""Test suite for protocol_resilience.py.

Goals:
- Prove current protocol is vulnerable (expected FAIL posture)
- Prove hardened protocol is resilient (expected PASS posture)
- Cover boundaries and report schema
- >=80 total test cases
"""

from __future__ import annotations

import unittest

from protocol_resilience import (
    BankRunSimulator,
    CBCascadeSimulator,
    CollateralFreezeSimulator,
    CorrelatedDepegSimulator,
    EconomicSustainabilitySimulator,
    GovernanceCaptureSimulator,
    MEVAttackSimulator,
    OffchainCollusionSimulator,
    run_all,
)


SIMULATOR_CLASSES = [
    CorrelatedDepegSimulator,
    CollateralFreezeSimulator,
    BankRunSimulator,
    OffchainCollusionSimulator,
    GovernanceCaptureSimulator,
    MEVAttackSimulator,
    CBCascadeSimulator,
    EconomicSustainabilitySimulator,
]


class TestProtocolResilienceGlobal(unittest.TestCase):
    def test_run_all_contains_all_simulators(self) -> None:
        out = run_all(iterations=10)
        self.assertEqual(set(out.keys()), {cls.__name__ for cls in SIMULATOR_CLASSES})

    def test_total_generated_test_count_guard(self) -> None:
        # 8 simulators * 12 per-simulator tests = 96
        expected_min = 80
        generated = len(SIMULATOR_CLASSES) * 12
        self.assertGreaterEqual(generated, expected_min)


class TestProtocolResiliencePerSimulator(unittest.TestCase):
    pass


def _make_current_vulnerable_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=120)
        self.assertTrue(report["current_protocol"]["vulnerable"])

    return test


def _make_hardened_resilient_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=120)
        self.assertTrue(report["hardened_protocol"]["resilient"])

    return test


def _make_hardened_better_mean_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=120)
        cur = report["current_protocol"]["stats"]["mean"]
        hard = report["hardened_protocol"]["stats"]["mean"]
        self.assertGreater(cur, hard)

    return test


def _make_report_keys_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=8)
        self.assertIn("simulator", report)
        self.assertIn("metric", report)
        self.assertIn("iterations", report)
        self.assertIn("current_protocol", report)
        self.assertIn("hardened_protocol", report)
        self.assertIn("improvement", report)

    return test


def _make_current_stats_fields_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=15)
        stats = report["current_protocol"]["stats"]
        self.assertEqual(set(stats.keys()), {"mean", "std", "worst", "p95"})

    return test


def _make_hardened_stats_fields_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=15)
        stats = report["hardened_protocol"]["stats"]
        self.assertEqual(set(stats.keys()), {"mean", "std", "worst", "p95"})

    return test


def _make_iteration_boundary_one_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=1)
        self.assertEqual(report["iterations"], 1)
        self.assertEqual(len(report["current_protocol"]["samples"]), 1)
        self.assertEqual(len(report["hardened_protocol"]["samples"]), 1)

    return test


def _make_invalid_iteration_raises_test(sim_cls):
    def test(self):
        with self.assertRaises(ValueError):
            sim_cls().run(iterations=0)

    return test


def _make_seed_determinism_test(sim_cls):
    def test(self):
        r1 = sim_cls(seed=99).run(iterations=40)
        r2 = sim_cls(seed=99).run(iterations=40)
        self.assertAlmostEqual(r1["current_protocol"]["stats"]["mean"], r2["current_protocol"]["stats"]["mean"], places=12)
        self.assertAlmostEqual(r1["hardened_protocol"]["stats"]["mean"], r2["hardened_protocol"]["stats"]["mean"], places=12)

    return test


def _make_stress_case_gap_test(sim_cls):
    def test(self):
        sim = sim_cls(seed=123)
        current, hardened = sim.stress_case()
        self.assertGreater(current, hardened)

    return test


def _make_p95_le_worst_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=50)
        cur = report["current_protocol"]["stats"]
        hard = report["hardened_protocol"]["stats"]
        self.assertLessEqual(cur["p95"], cur["worst"] + 1e-12)
        self.assertLessEqual(hard["p95"], hard["worst"] + 1e-12)

    return test


def _make_non_negative_metric_test(sim_cls):
    def test(self):
        report = sim_cls().run(iterations=30)
        self.assertGreaterEqual(min(report["current_protocol"]["samples"]), 0.0)
        self.assertGreaterEqual(min(report["hardened_protocol"]["samples"]), 0.0)

    return test


for cls in SIMULATOR_CLASSES:
    prefix = f"test_{cls.__name__.lower()}"
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_current_vulnerable", _make_current_vulnerable_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_hardened_resilient", _make_hardened_resilient_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_hardened_better_mean", _make_hardened_better_mean_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_report_keys", _make_report_keys_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_current_stats_fields", _make_current_stats_fields_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_hardened_stats_fields", _make_hardened_stats_fields_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_iterations_boundary_one", _make_iteration_boundary_one_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_invalid_iterations_raise", _make_invalid_iteration_raises_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_seed_determinism", _make_seed_determinism_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_stress_case_gap", _make_stress_case_gap_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_p95_le_worst", _make_p95_le_worst_test(cls))
    setattr(TestProtocolResiliencePerSimulator, f"{prefix}_non_negative_metrics", _make_non_negative_metric_test(cls))


if __name__ == "__main__":
    unittest.main(verbosity=2)
