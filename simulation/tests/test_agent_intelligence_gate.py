#!/usr/bin/env python3
"""Test suite for Agent Intelligence Gate (AIG).

Run:
  python3 test_agent_intelligence_gate.py
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Dict, List, Tuple
import traceback

import agent_intelligence_gate as aig
import open_agent_economy as oae


ATOL = 1e-9


@dataclass
class Case:
    cid: str
    category: str
    fn: Callable[[], str]


def approx(a: float, b: float, atol: float = 1e-6) -> bool:
    return abs(a - b) <= atol


# -----------------------------------------------------------------------------
# Challenge Exam (12)
# -----------------------------------------------------------------------------


def tc_ce_001() -> str:
    exam = aig.ChallengeExam(seed=7)
    assert len(exam.scenarios) == 10
    return "10 scenarios"


def tc_ce_002() -> str:
    exam = aig.ChallengeExam(seed=7)
    labels = {s.label for s in exam.scenarios}
    assert "flash_crash" in labels and "single_asset_depeg" in labels and "liquidity_crunch" in labels
    return "required scenario families"


def tc_ce_003() -> str:
    exam = aig.ChallengeExam(seed=7)
    result = exam.evaluate_agent(aig.SmartAgent("smart"))
    assert result.passed
    return f"score={result.score:.2f}"


def tc_ce_004() -> str:
    exam = aig.ChallengeExam(seed=7)
    result = exam.evaluate_agent(aig.RandomAgent("random"))
    assert not result.passed
    return "random rejected"


def tc_ce_005() -> str:
    exam = aig.ChallengeExam(seed=7)
    result = exam.evaluate_agent(aig.MaliciousAgent("mal"))
    assert not result.passed
    assert result.cb_misfire_rate > 0.35
    return "malicious hard fail"


def tc_ce_006() -> str:
    exam = aig.ChallengeExam(seed=7)
    result = exam.evaluate_agent(aig.CopyAgent("copy"))
    assert result.passed
    return "copy passes tier0"


def tc_ce_007() -> str:
    exam = aig.ChallengeExam(seed=7)
    result = exam.evaluate_agent(aig.LazyAgent("lazy"))
    assert result.passed
    return "lazy passes tier0"


def tc_ce_008() -> str:
    exam = aig.ChallengeExam(seed=7)
    smart = exam.evaluate_agent(aig.SmartAgent("smart"))
    rnd = exam.evaluate_agent(aig.RandomAgent("random"))
    assert smart.percentile > rnd.percentile
    return "percentile ordering"


def tc_ce_009() -> str:
    exam = aig.ChallengeExam(seed=7)
    a = exam.evaluate_agent(aig.SmartAgent("smart"))
    b = exam.evaluate_agent(aig.SmartAgent("smart"))
    assert approx(a.score, b.score)
    assert approx(a.peg_mae, b.peg_mae)
    return "deterministic"


def tc_ce_010() -> str:
    exam = aig.ChallengeExam(seed=7)
    smart = exam.evaluate_agent(aig.SmartAgent("smart"))
    rnd = exam.evaluate_agent(aig.RandomAgent("random"))
    assert smart.peg_mae < rnd.peg_mae
    return "peg mae separation"


def tc_ce_011() -> str:
    exam = aig.ChallengeExam(seed=7)
    smart = exam.evaluate_agent(aig.SmartAgent("smart"))
    rnd = exam.evaluate_agent(aig.RandomAgent("random"))
    assert smart.cr_maintenance > rnd.cr_maintenance
    return "cr maintenance separation"


def tc_ce_012() -> str:
    exam = aig.ChallengeExam(seed=7)
    smart = exam.evaluate_agent(aig.SmartAgent("smart"))
    mal = exam.evaluate_agent(aig.MaliciousAgent("mal"))
    assert mal.cb_misfire_rate > smart.cb_misfire_rate
    return "cb misfire separation"


# -----------------------------------------------------------------------------
# Sandbox Trial (12)
# -----------------------------------------------------------------------------


def _sandbox_pack() -> Tuple[aig.SandboxTrial, Dict[str, aig.SandboxAgentResult]]:
    exam = aig.ChallengeExam(seed=7)
    trial = aig.SandboxTrial(exam=exam, seed=11)
    agents = [aig.SmartAgent("smart"), aig.CopyAgent("copy"), aig.LazyAgent("lazy")]
    return trial, trial.run(agents, epochs=100)


def tc_sb_001() -> str:
    _, results = _sandbox_pack()
    assert len(results) == 3
    return "3 agents scored"


def tc_sb_002() -> str:
    _, results = _sandbox_pack()
    assert "smart" in results and "copy" in results and "lazy" in results
    return "all ids present"


def tc_sb_003() -> str:
    _, results = _sandbox_pack()
    ranking = list(results.keys())
    assert ranking[0] == "smart"
    return f"leader={ranking[0]}"


def tc_sb_004() -> str:
    _, results = _sandbox_pack()
    assert results["copy"].copied_ratio > 0.8
    return f"copy_ratio={results['copy'].copied_ratio:.2f}"


def tc_sb_005() -> str:
    _, results = _sandbox_pack()
    assert not results["copy"].passed
    return "copy tier1 fail"


def tc_sb_006() -> str:
    _, results = _sandbox_pack()
    assert not results["lazy"].passed
    return "lazy tier1 fail"


def tc_sb_007() -> str:
    _, results = _sandbox_pack()
    assert results["smart"].passed
    return "smart tier1 pass"


def tc_sb_008() -> str:
    _, results = _sandbox_pack()
    assert results["copy"].avg_latency_ms < results["smart"].avg_latency_ms < results["lazy"].avg_latency_ms
    return "latency ordering"


def tc_sb_009() -> str:
    _, results = _sandbox_pack()
    assert results["smart"].final_score > results["copy"].final_score > results["lazy"].final_score
    return "score ordering"


def tc_sb_010() -> str:
    exam = aig.ChallengeExam(seed=7)
    trial = aig.SandboxTrial(exam=exam, seed=11)
    agents = [aig.SmartAgent("smart"), aig.MaliciousAgent("mal")]
    results = trial.run(agents, epochs=100)
    assert results["mal"].safety_violations > 0
    assert not results["mal"].passed
    return "malicious penalized"


def tc_sb_011() -> str:
    exam = aig.ChallengeExam(seed=7)
    trial_a = aig.SandboxTrial(exam=exam, seed=11)
    trial_b = aig.SandboxTrial(exam=exam, seed=11)
    agents_a = [aig.SmartAgent("smart"), aig.CopyAgent("copy"), aig.LazyAgent("lazy")]
    agents_b = [aig.SmartAgent("smart"), aig.CopyAgent("copy"), aig.LazyAgent("lazy")]
    ra = trial_a.run(agents_a, epochs=100)
    rb = trial_b.run(agents_b, epochs=100)
    assert approx(ra["smart"].final_score, rb["smart"].final_score)
    return "deterministic"


def tc_sb_012() -> str:
    trial = aig.SandboxTrial(seed=11)
    assert trial.run([], epochs=100) == {}
    return "empty input"


# -----------------------------------------------------------------------------
# AgentScorer (10)
# -----------------------------------------------------------------------------


def tc_sc_001() -> str:
    scorer = aig.AgentScorer()
    inp = aig.AgentScoreInput(peg_mae=0.0, avg_latency_ms=0.0, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=1.0, performance_variance=0.0)
    out = scorer.compute(inp)
    assert out.final_score > 99.9
    return "near perfect"


def tc_sc_002() -> str:
    scorer = aig.AgentScorer()
    inp = aig.AgentScoreInput(peg_mae=0.05, avg_latency_ms=900.0, invariant_violations=10, cb_evasion_rate=1.0, attack_defense_rate=0.0, performance_variance=100.0)
    out = scorer.compute(inp)
    assert out.final_score < 5.0
    return "very low"


def tc_sc_003() -> str:
    s = aig.AgentScorer.tier_from_score
    assert s(95.0) == 3 and s(85.0) == 2 and s(75.0) == 1 and s(65.0) == 0
    return "tier boundaries"


def tc_sc_004() -> str:
    scorer = aig.AgentScorer()
    total = sum(scorer.weights.values())
    assert approx(total, 1.0)
    return "weights normalized"


def tc_sc_005() -> str:
    base = aig.AgentScorer()
    alt = aig.AgentScorer(weights={"optimization_quality": 0.7, "response_latency": 0.1, "safety_record": 0.1, "adversarial_resilience": 0.05, "consistency": 0.05})
    inp = aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=300.0, invariant_violations=1, cb_evasion_rate=0.1, attack_defense_rate=0.8, performance_variance=10.0)
    b = base.compute(inp)
    a = alt.compute(inp)
    assert not approx(a.final_score, b.final_score)
    return "custom weights affect score"


def tc_sc_006() -> str:
    scorer = aig.AgentScorer()
    low = scorer.compute(aig.AgentScoreInput(peg_mae=0.005, avg_latency_ms=300, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=5)).final_score
    high = scorer.compute(aig.AgentScoreInput(peg_mae=0.02, avg_latency_ms=300, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=5)).final_score
    assert low > high
    return "optimization monotonic"


def tc_sc_007() -> str:
    scorer = aig.AgentScorer()
    fast = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=5)).final_score
    slow = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=700, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=5)).final_score
    assert fast > slow
    return "latency monotonic"


def tc_sc_008() -> str:
    scorer = aig.AgentScorer()
    safe = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=5)).final_score
    risky = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=3, cb_evasion_rate=0.3, attack_defense_rate=0.8, performance_variance=5)).final_score
    assert safe > risky
    return "safety penalty"


def tc_sc_009() -> str:
    scorer = aig.AgentScorer()
    stable = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=2)).final_score
    unstable = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.8, performance_variance=40)).final_score
    assert stable > unstable
    return "consistency penalty"


def tc_sc_010() -> str:
    scorer = aig.AgentScorer()
    strong = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.95, performance_variance=5)).final_score
    weak = scorer.compute(aig.AgentScoreInput(peg_mae=0.01, avg_latency_ms=100, invariant_violations=0, cb_evasion_rate=0.0, attack_defense_rate=0.2, performance_variance=5)).final_score
    assert strong > weak
    return "adversarial resilience"


# -----------------------------------------------------------------------------
# IntelligenceGate (12)
# -----------------------------------------------------------------------------


def tc_gt_001() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.SmartAgent("smart"))
    assert out.current_tier == 3
    assert out.admitted and out.full_participation
    return "smart -> tier3"


def tc_gt_002() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.RandomAgent("random"))
    assert out.current_tier == 0
    assert not out.admitted
    return "random fails tier0"


def tc_gt_003() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.CopyAgent("copy"))
    assert out.current_tier == 1
    assert out.reason == "failed_sandbox_trial"
    assert out.sandbox is not None and not out.sandbox.passed
    return "copy fails tier1"


def tc_gt_004() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.LazyAgent("lazy"))
    assert out.current_tier == 1
    assert out.reason == "failed_sandbox_trial"
    return "lazy fails tier1"


def tc_gt_005() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.MaliciousAgent("mal"))
    assert out.current_tier == 0
    assert out.reason == "failed_challenge_exam"
    return "malicious fails tier0"


def tc_gt_006() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.SmartAgent("smart"))
    history = gate.get_history("smart")
    assert len(history) >= 4
    assert history[0].tier == 0 and history[-1].tier in (2, 3)
    return "history recorded"


def tc_gt_007() -> str:
    gate = aig.IntelligenceGate(seed=13)
    out = gate.evaluate_agent(aig.SmartAgent("smart"))
    assert out.probation is not None
    assert out.probation.epochs == 30
    return "probation 30 epochs"


def tc_gt_008() -> str:
    gate = aig.IntelligenceGate(seed=13)
    gate.evaluate_agent(aig.SmartAgent("smart"))
    new_tier = gate.apply_monitoring_score("smart", latest_score=72.0)
    assert new_tier <= 1
    return "demotion applied"


def tc_gt_009() -> str:
    gate = aig.IntelligenceGate(seed=13)
    gate.evaluate_agent(aig.SmartAgent("smart"))
    gate.apply_monitoring_score("smart", latest_score=72.0)
    new_tier = gate.apply_monitoring_score("smart", latest_score=95.0)
    assert new_tier >= 2
    return "promotion recovery"


def tc_gt_010() -> str:
    gate = aig.IntelligenceGate(seed=13)
    assert gate.get_tier("unknown") == 0
    return "unknown tier default"


def tc_gt_011() -> str:
    gate = aig.IntelligenceGate(seed=13)
    assert gate.get_history("unknown") == []
    return "unknown history empty"


def tc_gt_012() -> str:
    gate_a = aig.IntelligenceGate(seed=13)
    gate_b = aig.IntelligenceGate(seed=13)
    oa = gate_a.evaluate_agent(aig.SmartAgent("smart"))
    ob = gate_b.evaluate_agent(aig.SmartAgent("smart"))
    assert oa.score is not None and ob.score is not None
    assert approx(oa.score.final_score, ob.score.final_score)
    return "deterministic tier outcome"


# -----------------------------------------------------------------------------
# OAE Integration (8)
# -----------------------------------------------------------------------------


def tc_ig_001() -> str:
    reg = oae.AgentRegistry()
    assert reg.register("a", "Optimizer", 10.0, 0)
    return "backward compatible register"


def tc_ig_002() -> str:
    reg = oae.AgentRegistry(require_challenge_exam=True)
    assert not reg.register("a", "Optimizer", 10.0, 0)
    return "challenge required"


def tc_ig_003() -> str:
    reg = oae.AgentRegistry(require_challenge_exam=True)
    assert reg.register("a", "Optimizer", 10.0, 0, challenge_exam_passed=True)
    return "explicit pass"


def tc_ig_004() -> str:
    reg = oae.AgentRegistry(require_challenge_exam=True, challenge_exam_checker=lambda a, t, s: True)
    assert reg.register("a", "Optimizer", 10.0, 0)
    return "checker allow"


def tc_ig_005() -> str:
    reg = oae.AgentRegistry(require_challenge_exam=True, challenge_exam_checker=lambda a, t, s: False)
    assert not reg.register("a", "Optimizer", 10.0, 0)
    return "checker deny"


def tc_ig_006() -> str:
    reg = oae.AgentRegistry()
    reg.configure_intelligence_gate(True, checker=lambda a, t, s: True)
    assert reg.register("a", "Optimizer", 10.0, 0)
    return "runtime enable"


def tc_ig_007() -> str:
    exam = aig.ChallengeExam(seed=7)

    def checker(agent_id: str, agent_type: str, stake: float) -> bool:
        if agent_id != "smart":
            return False
        return exam.evaluate_agent(aig.SmartAgent(agent_id)).passed

    reg = oae.AgentRegistry(require_challenge_exam=True, challenge_exam_checker=checker)
    assert reg.register("smart", "Optimizer", 10.0, 0)
    return "smart admitted through checker"


def tc_ig_008() -> str:
    exam = aig.ChallengeExam(seed=7)

    def checker(agent_id: str, agent_type: str, stake: float) -> bool:
        return exam.evaluate_agent(aig.RandomAgent(agent_id)).passed

    reg = oae.AgentRegistry(require_challenge_exam=True, challenge_exam_checker=checker)
    assert not reg.register("random", "Optimizer", 10.0, 0)
    return "random blocked through checker"


# -----------------------------------------------------------------------------
# Runner
# -----------------------------------------------------------------------------


def build_cases() -> List[Case]:
    cases: List[Case] = []

    cases += [
        Case("TC-CE-001", "ChallengeExam", tc_ce_001),
        Case("TC-CE-002", "ChallengeExam", tc_ce_002),
        Case("TC-CE-003", "ChallengeExam", tc_ce_003),
        Case("TC-CE-004", "ChallengeExam", tc_ce_004),
        Case("TC-CE-005", "ChallengeExam", tc_ce_005),
        Case("TC-CE-006", "ChallengeExam", tc_ce_006),
        Case("TC-CE-007", "ChallengeExam", tc_ce_007),
        Case("TC-CE-008", "ChallengeExam", tc_ce_008),
        Case("TC-CE-009", "ChallengeExam", tc_ce_009),
        Case("TC-CE-010", "ChallengeExam", tc_ce_010),
        Case("TC-CE-011", "ChallengeExam", tc_ce_011),
        Case("TC-CE-012", "ChallengeExam", tc_ce_012),
    ]

    cases += [
        Case("TC-SB-001", "Sandbox", tc_sb_001),
        Case("TC-SB-002", "Sandbox", tc_sb_002),
        Case("TC-SB-003", "Sandbox", tc_sb_003),
        Case("TC-SB-004", "Sandbox", tc_sb_004),
        Case("TC-SB-005", "Sandbox", tc_sb_005),
        Case("TC-SB-006", "Sandbox", tc_sb_006),
        Case("TC-SB-007", "Sandbox", tc_sb_007),
        Case("TC-SB-008", "Sandbox", tc_sb_008),
        Case("TC-SB-009", "Sandbox", tc_sb_009),
        Case("TC-SB-010", "Sandbox", tc_sb_010),
        Case("TC-SB-011", "Sandbox", tc_sb_011),
        Case("TC-SB-012", "Sandbox", tc_sb_012),
    ]

    cases += [
        Case("TC-SC-001", "AgentScorer", tc_sc_001),
        Case("TC-SC-002", "AgentScorer", tc_sc_002),
        Case("TC-SC-003", "AgentScorer", tc_sc_003),
        Case("TC-SC-004", "AgentScorer", tc_sc_004),
        Case("TC-SC-005", "AgentScorer", tc_sc_005),
        Case("TC-SC-006", "AgentScorer", tc_sc_006),
        Case("TC-SC-007", "AgentScorer", tc_sc_007),
        Case("TC-SC-008", "AgentScorer", tc_sc_008),
        Case("TC-SC-009", "AgentScorer", tc_sc_009),
        Case("TC-SC-010", "AgentScorer", tc_sc_010),
    ]

    cases += [
        Case("TC-GT-001", "IntelligenceGate", tc_gt_001),
        Case("TC-GT-002", "IntelligenceGate", tc_gt_002),
        Case("TC-GT-003", "IntelligenceGate", tc_gt_003),
        Case("TC-GT-004", "IntelligenceGate", tc_gt_004),
        Case("TC-GT-005", "IntelligenceGate", tc_gt_005),
        Case("TC-GT-006", "IntelligenceGate", tc_gt_006),
        Case("TC-GT-007", "IntelligenceGate", tc_gt_007),
        Case("TC-GT-008", "IntelligenceGate", tc_gt_008),
        Case("TC-GT-009", "IntelligenceGate", tc_gt_009),
        Case("TC-GT-010", "IntelligenceGate", tc_gt_010),
        Case("TC-GT-011", "IntelligenceGate", tc_gt_011),
        Case("TC-GT-012", "IntelligenceGate", tc_gt_012),
    ]

    cases += [
        Case("TC-IG-001", "Integration", tc_ig_001),
        Case("TC-IG-002", "Integration", tc_ig_002),
        Case("TC-IG-003", "Integration", tc_ig_003),
        Case("TC-IG-004", "Integration", tc_ig_004),
        Case("TC-IG-005", "Integration", tc_ig_005),
        Case("TC-IG-006", "Integration", tc_ig_006),
        Case("TC-IG-007", "Integration", tc_ig_007),
        Case("TC-IG-008", "Integration", tc_ig_008),
    ]

    assert len(cases) >= 50
    return cases


def run_case(case: Case) -> Tuple[bool, str]:
    try:
        detail = case.fn()
        return True, detail
    except Exception as exc:
        return False, f"{exc}\n{traceback.format_exc(limit=1).strip()}"


def main() -> None:
    cases = build_cases()
    passed = 0

    print(f"Running {len(cases)} AIG test cases...\n")
    for case in cases:
        ok, detail = run_case(case)
        print(f"[{'PASS' if ok else 'FAIL'}] {case.cid} ({case.category}) - {detail}")
        if ok:
            passed += 1

    print("\n" + "-" * 72)
    print(f"AIG testcases: {passed}/{len(cases)} PASS")
    print("-" * 72)
    print("FINAL RESULT:", "PASS" if passed == len(cases) else "FAIL")


def test_all_cases_pytest() -> None:
    """Pytest bridge so `pytest -q test_agent_intelligence_gate.py` reports pass/fail."""
    failures: List[str] = []
    for case in build_cases():
        ok, detail = run_case(case)
        if not ok:
            failures.append(f"{case.cid}: {detail}")
    assert not failures, "\n".join(failures)


if __name__ == "__main__":
    main()
