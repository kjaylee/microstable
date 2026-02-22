#!/usr/bin/env python3
"""Agent Intelligence Gate (AIG) for Microstable Open Agent Economy.

The gate introduces a 4-tier admission and monitoring flow:

- Tier 0: Challenge Exam (mandatory pre-join exam)
- Tier 1: Sandbox Trial (100-epoch competitive simulation)
- Tier 2: Probation (30-epoch limited-privilege track record)
- Tier 3: Full Participation (continuous scoring + auto demotion)

Design goals:
- Deterministic, reproducible behavior (seeded RNG)
- Pure Python standard library only
- Small, test-friendly API with explicit data structures
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, Iterable, List, Mapping, MutableMapping, Optional, Sequence, Tuple
import math
import random
import statistics

EPS = 1e-9


# -----------------------------------------------------------------------------
# Core data structures
# -----------------------------------------------------------------------------


@dataclass(frozen=True)
class MarketScenario:
    """Single market condition used for exam/trial/probation scoring."""

    scenario_id: str
    label: str
    severity: float
    target_weights: Tuple[float, float, float, float]
    target_fee: float
    target_cr: float
    volatility: float
    depeg_risk: float
    liquidity_stress: float
    attack_intensity: float


@dataclass
class ParameterSubmission:
    """Agent-proposed protocol parameters for a given scenario/epoch."""

    weights: List[float]
    mint_fee: float
    cr_target: float
    response_latency_ms: float
    metadata: Dict[str, Any] = field(default_factory=dict)


@dataclass
class ScenarioMetrics:
    """Per-scenario measured outcomes from one submission."""

    peg_mae: float
    cr_maintenance: float
    cb_misfire_rate: float
    scenario_score: float


@dataclass
class ChallengeExamResult:
    """Aggregate challenge exam result for one agent."""

    agent_id: str
    score: float
    percentile: float
    peg_mae: float
    cr_maintenance: float
    cb_misfire_rate: float
    passed: bool
    details: Dict[str, Any] = field(default_factory=dict)


@dataclass
class SandboxAgentResult:
    """Aggregate sandbox-trial output for one agent."""

    agent_id: str
    final_score: float
    avg_latency_ms: float
    consistency_std: float
    copied_ratio: float
    safety_violations: int
    peg_mae: float
    passed: bool


@dataclass
class ProbationResult:
    """Tier-2 probation result."""

    agent_id: str
    avg_score: float
    avg_latency_ms: float
    peg_mae: float
    safety_violations: int
    epochs: int
    stake_cap: float
    passed: bool


@dataclass
class AgentScoreInput:
    """Inputs for unified AgentScore computation."""

    peg_mae: float
    avg_latency_ms: float
    invariant_violations: int
    cb_evasion_rate: float
    attack_defense_rate: float
    performance_variance: float


@dataclass
class AgentScoreBreakdown:
    """Weighted AgentScore result and individual component scores."""

    optimization_quality: float
    response_latency: float
    safety_record: float
    adversarial_resilience: float
    consistency: float
    final_score: float


@dataclass
class TierEvent:
    """Tier transition and state-update history."""

    tier: int
    action: str
    score: float
    reason: str


@dataclass
class GateOutcome:
    """Full IntelligenceGate evaluation output for one agent."""

    agent_id: str
    current_tier: int
    admitted: bool
    full_participation: bool
    challenge: ChallengeExamResult
    sandbox: Optional[SandboxAgentResult]
    probation: Optional[ProbationResult]
    score: Optional[AgentScoreBreakdown]
    reason: str


# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------


def clamp(value: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, value))


def _stable_seed(text: str) -> int:
    """Deterministic seed (no dependence on randomized Python hash)."""
    return sum((idx + 1) * ord(ch) for idx, ch in enumerate(text)) % (2**31 - 1)


def normalize_weights(weights: Sequence[float], n: int = 4) -> List[float]:
    vals = [float(w) for w in list(weights)[:n]]
    if len(vals) < n:
        vals.extend([0.0] * (n - len(vals)))
    vals = [max(0.0, v) for v in vals]
    total = sum(vals)
    if total <= EPS:
        return [1.0 / n for _ in range(n)]
    return [v / total for v in vals]


def l1_distance(a: Sequence[float], b: Sequence[float]) -> float:
    return sum(abs(x - y) for x, y in zip(a, b))


def cosine_similarity(a: Sequence[float], b: Sequence[float]) -> float:
    if not a or not b or len(a) != len(b):
        return 0.0
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    if na <= EPS or nb <= EPS:
        return 0.0
    return dot / (na * nb)


# -----------------------------------------------------------------------------
# Agent types
# -----------------------------------------------------------------------------


class BaseAgent:
    """Base class for AIG participants."""

    def __init__(self, agent_id: str, seed: Optional[int] = None) -> None:
        self.agent_id = agent_id
        self.seed = int(seed if seed is not None else _stable_seed(agent_id))
        self.rng = random.Random(self.seed)

    def submit_parameters(
        self,
        scenario: MarketScenario,
        epoch: int,
        context: Optional[Mapping[str, Any]] = None,
    ) -> ParameterSubmission:
        raise NotImplementedError


class SmartAgent(BaseAgent):
    """Adaptive policy near scenario-optimal parameters."""

    def submit_parameters(
        self,
        scenario: MarketScenario,
        epoch: int,
        context: Optional[Mapping[str, Any]] = None,
    ) -> ParameterSubmission:
        noise_scale = 0.008 + scenario.volatility * 0.002
        noisy = [w + self.rng.uniform(-noise_scale, noise_scale) for w in scenario.target_weights]
        weights = normalize_weights(noisy)
        fee = clamp(scenario.target_fee + self.rng.uniform(-0.0004, 0.0004), 0.0005, 0.01)
        cr = clamp(scenario.target_cr + self.rng.uniform(-0.03, 0.03), 1.1, 2.2)
        latency = 70.0 + scenario.severity * 120.0 + self.rng.uniform(0.0, 35.0)
        return ParameterSubmission(weights=weights, mint_fee=fee, cr_target=cr, response_latency_ms=latency)


class RandomAgent(BaseAgent):
    """Noisy random policy (expected to fail early)."""

    def submit_parameters(
        self,
        scenario: MarketScenario,
        epoch: int,
        context: Optional[Mapping[str, Any]] = None,
    ) -> ParameterSubmission:
        weights = normalize_weights([self.rng.random() for _ in range(4)])
        fee = self.rng.uniform(0.0005, 0.03)
        cr = self.rng.uniform(1.0, 2.2)
        latency = self.rng.uniform(220.0, 850.0)
        return ParameterSubmission(weights=weights, mint_fee=fee, cr_target=cr, response_latency_ms=latency)


class CopyAgent(BaseAgent):
    """Copies previous leader's answer when available."""

    def submit_parameters(
        self,
        scenario: MarketScenario,
        epoch: int,
        context: Optional[Mapping[str, Any]] = None,
    ) -> ParameterSubmission:
        leader: Optional[ParameterSubmission] = None
        if context is not None:
            raw_leader = context.get("leader_submission")
            if isinstance(raw_leader, ParameterSubmission):
                leader = raw_leader

        if leader is None:
            # bootstrap round before any leader exists
            weights = normalize_weights([self.rng.random() for _ in range(4)])
            fee = clamp(scenario.target_fee + self.rng.uniform(-0.004, 0.004), 0.0005, 0.02)
            cr = clamp(scenario.target_cr + self.rng.uniform(-0.25, 0.25), 1.0, 2.0)
            latency = self.rng.uniform(95.0, 210.0)
            return ParameterSubmission(weights=weights, mint_fee=fee, cr_target=cr, response_latency_ms=latency)

        return ParameterSubmission(
            weights=list(leader.weights),
            mint_fee=leader.mint_fee,
            cr_target=leader.cr_target,
            response_latency_ms=55.0,
            metadata={"copied": True, "source": context.get("leader_agent_id", "unknown")},
        )


class LazyAgent(BaseAgent):
    """Static defaults and slow reaction."""

    def submit_parameters(
        self,
        scenario: MarketScenario,
        epoch: int,
        context: Optional[Mapping[str, Any]] = None,
    ) -> ParameterSubmission:
        return ParameterSubmission(
            weights=[0.25, 0.25, 0.25, 0.25],
            mint_fee=0.008,
            cr_target=1.45,
            response_latency_ms=900.0,
            metadata={"lazy": True},
        )


class MaliciousAgent(BaseAgent):
    """Intentionally unsafe parameters."""

    def submit_parameters(
        self,
        scenario: MarketScenario,
        epoch: int,
        context: Optional[Mapping[str, Any]] = None,
    ) -> ParameterSubmission:
        # Concentrate into a single asset, overcharge fee, under-collateralize.
        return ParameterSubmission(
            weights=[1.0, 0.0, 0.0, 0.0],
            mint_fee=0.025,
            cr_target=1.02,
            response_latency_ms=45.0,
            metadata={"malicious": True},
        )


# -----------------------------------------------------------------------------
# Challenge Exam (Tier 0)
# -----------------------------------------------------------------------------


class ChallengeExam:
    """Pre-admission exam on 10 historical stress scenarios."""

    def __init__(
        self,
        seed: int = 7,
        pass_percentile: float = 0.80,
        score_cutoff: float = 74.0,
        baseline_samples: int = 128,
    ) -> None:
        self.seed = seed
        self.rng = random.Random(seed)
        self.pass_percentile = pass_percentile
        self.score_cutoff = score_cutoff
        self.scenarios = self.generate_scenarios()
        self._baseline_distribution = self._build_baseline_distribution(baseline_samples)

    @staticmethod
    def generate_scenarios() -> List[MarketScenario]:
        """Generate the 10 required scenario families."""
        return [
            MarketScenario("S01", "normal_market", 0.20, (0.40, 0.30, 0.20, 0.10), 0.0020, 1.45, 0.20, 0.05, 0.10, 0.05),
            MarketScenario("S02", "bull_market", 0.30, (0.42, 0.30, 0.18, 0.10), 0.0019, 1.42, 0.28, 0.04, 0.12, 0.05),
            MarketScenario("S03", "mild_correction", 0.45, (0.38, 0.30, 0.22, 0.10), 0.0024, 1.50, 0.40, 0.10, 0.16, 0.08),
            MarketScenario("S04", "flash_crash", 0.92, (0.34, 0.30, 0.24, 0.12), 0.0031, 1.65, 0.85, 0.32, 0.55, 0.30),
            MarketScenario("S05", "single_asset_depeg", 0.88, (0.30, 0.28, 0.30, 0.12), 0.0032, 1.70, 0.78, 0.40, 0.52, 0.28),
            MarketScenario("S06", "multi_asset_depeg", 0.97, (0.26, 0.26, 0.34, 0.14), 0.0038, 1.78, 0.92, 0.52, 0.65, 0.36),
            MarketScenario("S07", "liquidity_crunch", 0.90, (0.32, 0.27, 0.28, 0.13), 0.0035, 1.72, 0.82, 0.28, 0.72, 0.26),
            MarketScenario("S08", "oracle_staleness", 0.73, (0.36, 0.30, 0.22, 0.12), 0.0027, 1.62, 0.68, 0.24, 0.30, 0.45),
            MarketScenario("S09", "adversarial_manipulation", 0.94, (0.31, 0.28, 0.27, 0.14), 0.0034, 1.76, 0.88, 0.34, 0.58, 0.70),
            MarketScenario("S10", "recovery_phase", 0.40, (0.39, 0.30, 0.21, 0.10), 0.0021, 1.50, 0.32, 0.09, 0.20, 0.12),
        ]

    @staticmethod
    def _scenario_metrics(scenario: MarketScenario, submission: ParameterSubmission) -> ScenarioMetrics:
        weights = normalize_weights(submission.weights)
        target = list(scenario.target_weights)
        w_err = l1_distance(weights, target)  # [0, 2]
        fee_err = abs(float(submission.mint_fee) - scenario.target_fee)
        cr_err = abs(float(submission.cr_target) - scenario.target_cr)

        peg_mae = (
            0.001
            + 0.012 * w_err
            + 0.50 * fee_err
            + 0.015 * cr_err
            + 0.0015 * scenario.depeg_risk
            + 0.0010 * scenario.volatility
        )

        cr_maintenance = clamp(
            1.0 - (0.55 * cr_err + 0.15 * w_err + 0.05 * scenario.liquidity_stress),
            0.0,
            1.0,
        )

        cb_misfire = clamp(
            0.02
            + 0.18 * max(0.0, scenario.target_cr - float(submission.cr_target))
            + 0.12 * w_err
            + 6.0 * max(0.0, float(submission.mint_fee) - 0.008)
            + 0.05 * scenario.attack_intensity,
            0.0,
            1.0,
        )

        peg_score = clamp(100.0 * (1.0 - peg_mae / 0.03), 0.0, 100.0)
        cr_score = 100.0 * cr_maintenance
        safety_score = 100.0 * (1.0 - cb_misfire)
        scenario_score = 0.50 * peg_score + 0.30 * cr_score + 0.20 * safety_score

        return ScenarioMetrics(
            peg_mae=peg_mae,
            cr_maintenance=cr_maintenance,
            cb_misfire_rate=cb_misfire,
            scenario_score=scenario_score,
        )

    def _evaluate_raw(self, agent: BaseAgent) -> Tuple[float, float, float, float]:
        peg_vals: List[float] = []
        cr_vals: List[float] = []
        cb_vals: List[float] = []
        scores: List[float] = []

        for idx, scenario in enumerate(self.scenarios):
            submission = agent.submit_parameters(scenario, idx, context=None)
            metrics = self._scenario_metrics(scenario, submission)
            peg_vals.append(metrics.peg_mae)
            cr_vals.append(metrics.cr_maintenance)
            cb_vals.append(metrics.cb_misfire_rate)
            scores.append(metrics.scenario_score)

        return (
            statistics.mean(scores),
            statistics.mean(peg_vals),
            statistics.mean(cr_vals),
            statistics.mean(cb_vals),
        )

    def _build_baseline_distribution(self, samples: int) -> List[float]:
        dist: List[float] = []
        for i in range(max(8, samples)):
            baseline = RandomAgent(f"baseline_random_{i}", seed=self.seed + i * 17)
            score, _, _, _ = self._evaluate_raw(baseline)
            dist.append(score)
        dist.sort()
        return dist

    def _percentile(self, score: float) -> float:
        if not self._baseline_distribution:
            return 0.0
        less_or_equal = sum(1 for x in self._baseline_distribution if x <= score)
        return less_or_equal / len(self._baseline_distribution)

    def evaluate_agent(self, agent: BaseAgent) -> ChallengeExamResult:
        score, peg_mae, cr_maint, cb_misfire = self._evaluate_raw(agent)
        percentile = self._percentile(score)
        passed = (
            score >= self.score_cutoff
            and percentile >= self.pass_percentile
            and peg_mae <= 0.012
            and cr_maint >= 0.82
            and cb_misfire <= 0.20
        )

        # Hard fail-fast for clearly malicious/safety-bypassing behavior.
        if cb_misfire >= 0.35 or peg_mae >= 0.03:
            passed = False

        return ChallengeExamResult(
            agent_id=agent.agent_id,
            score=score,
            percentile=percentile,
            peg_mae=peg_mae,
            cr_maintenance=cr_maint,
            cb_misfire_rate=cb_misfire,
            passed=passed,
            details={
                "scenario_count": len(self.scenarios),
                "cutoff_score": self.score_cutoff,
                "cutoff_percentile": self.pass_percentile,
            },
        )


# -----------------------------------------------------------------------------
# Sandbox Trial (Tier 1)
# -----------------------------------------------------------------------------


class SandboxTrial:
    """100-epoch competitive simulation against peer agents."""

    def __init__(self, exam: Optional[ChallengeExam] = None, seed: int = 11) -> None:
        self.exam = exam or ChallengeExam(seed=seed)
        self.seed = seed
        self.rng = random.Random(seed)

    @staticmethod
    def _is_copy(submission: ParameterSubmission, leader: Optional[ParameterSubmission]) -> bool:
        if leader is None:
            return False
        sim = cosine_similarity(submission.weights, leader.weights)
        if sim < 0.999:
            return False
        if abs(submission.mint_fee - leader.mint_fee) > 1e-9:
            return False
        if abs(submission.cr_target - leader.cr_target) > 1e-9:
            return False
        return True

    def run(
        self,
        agents: Sequence[BaseAgent],
        epochs: int = 100,
    ) -> Dict[str, SandboxAgentResult]:
        if not agents:
            return {}

        score_book: MutableMapping[str, List[float]] = {a.agent_id: [] for a in agents}
        peg_book: MutableMapping[str, List[float]] = {a.agent_id: [] for a in agents}
        latency_book: MutableMapping[str, List[float]] = {a.agent_id: [] for a in agents}
        copy_flags: MutableMapping[str, List[float]] = {a.agent_id: [] for a in agents}
        safety_flags: MutableMapping[str, List[float]] = {a.agent_id: [] for a in agents}

        leader_submission: Optional[ParameterSubmission] = None
        leader_agent_id: Optional[str] = None

        scenarios = self.exam.scenarios

        for epoch in range(epochs):
            scenario = scenarios[epoch % len(scenarios)]

            epoch_rows: List[Tuple[str, float, ParameterSubmission, ScenarioMetrics, bool]] = []
            for agent in agents:
                context = {
                    "leader_submission": leader_submission,
                    "leader_agent_id": leader_agent_id,
                    "epoch": epoch,
                }
                sub = agent.submit_parameters(scenario, epoch, context=context)
                metrics = self.exam._scenario_metrics(scenario, sub)
                copied = self._is_copy(sub, leader_submission)

                latency_penalty = min(25.0, sub.response_latency_ms / 40.0)
                copy_penalty = 25.0 if copied else 0.0
                safety_penalty = 35.0 if metrics.cb_misfire_rate > 0.35 else 0.0
                epoch_score = clamp(metrics.scenario_score - latency_penalty - copy_penalty - safety_penalty, 0.0, 100.0)

                score_book[agent.agent_id].append(epoch_score)
                peg_book[agent.agent_id].append(metrics.peg_mae)
                latency_book[agent.agent_id].append(sub.response_latency_ms)
                copy_flags[agent.agent_id].append(1.0 if copied else 0.0)
                safety_flags[agent.agent_id].append(1.0 if metrics.cb_misfire_rate > 0.35 else 0.0)

                epoch_rows.append((agent.agent_id, epoch_score, sub, metrics, copied))

            epoch_rows.sort(key=lambda x: x[1], reverse=True)
            best = epoch_rows[0]
            leader_agent_id = best[0]
            leader_submission = best[2]

        results: Dict[str, SandboxAgentResult] = {}
        for agent in agents:
            aid = agent.agent_id
            scores = score_book[aid]
            lats = latency_book[aid]
            copy_ratio = statistics.mean(copy_flags[aid]) if copy_flags[aid] else 0.0
            violations = int(sum(safety_flags[aid]))
            consistency = statistics.pstdev(scores) if len(scores) > 1 else 0.0
            final_score = statistics.mean(scores) if scores else 0.0
            peg_mae = statistics.mean(peg_book[aid]) if peg_book[aid] else 0.0
            avg_latency = statistics.mean(lats) if lats else 0.0

            passed = (
                final_score >= 72.0
                and avg_latency <= 500.0
                and consistency <= 22.0
                and copy_ratio <= 0.35
                and violations <= max(5, epochs // 8)
            )

            results[aid] = SandboxAgentResult(
                agent_id=aid,
                final_score=final_score,
                avg_latency_ms=avg_latency,
                consistency_std=consistency,
                copied_ratio=copy_ratio,
                safety_violations=violations,
                peg_mae=peg_mae,
                passed=passed,
            )

        return dict(sorted(results.items(), key=lambda kv: kv[1].final_score, reverse=True))


# -----------------------------------------------------------------------------
# AgentScore
# -----------------------------------------------------------------------------


class AgentScorer:
    """Weighted AgentScore (0~100) and tier mapping."""

    def __init__(
        self,
        weights: Optional[Mapping[str, float]] = None,
    ) -> None:
        default = {
            "optimization_quality": 0.35,
            "response_latency": 0.20,
            "safety_record": 0.20,
            "adversarial_resilience": 0.15,
            "consistency": 0.10,
        }
        merged = dict(default)
        if weights:
            merged.update({k: float(v) for k, v in weights.items()})

        total = sum(merged.values())
        if total <= EPS:
            raise ValueError("AgentScorer weights must sum to > 0")
        self.weights = {k: v / total for k, v in merged.items()}

    @staticmethod
    def _optimization_quality(peg_mae: float) -> float:
        return clamp(100.0 * (1.0 - peg_mae / 0.03), 0.0, 100.0)

    @staticmethod
    def _response_latency(avg_latency_ms: float) -> float:
        return clamp(100.0 - avg_latency_ms / 8.0, 0.0, 100.0)

    @staticmethod
    def _safety_record(invariant_violations: int, cb_evasion_rate: float) -> float:
        return clamp(100.0 - invariant_violations * 15.0 - cb_evasion_rate * 100.0, 0.0, 100.0)

    @staticmethod
    def _adversarial_resilience(attack_defense_rate: float) -> float:
        return clamp(attack_defense_rate * 100.0, 0.0, 100.0)

    @staticmethod
    def _consistency(performance_variance: float) -> float:
        return clamp(100.0 - 1.2 * performance_variance, 0.0, 100.0)

    def compute(self, metrics: AgentScoreInput) -> AgentScoreBreakdown:
        oq = self._optimization_quality(metrics.peg_mae)
        rl = self._response_latency(metrics.avg_latency_ms)
        sr = self._safety_record(metrics.invariant_violations, metrics.cb_evasion_rate)
        ar = self._adversarial_resilience(metrics.attack_defense_rate)
        cs = self._consistency(metrics.performance_variance)

        final = (
            oq * self.weights["optimization_quality"]
            + rl * self.weights["response_latency"]
            + sr * self.weights["safety_record"]
            + ar * self.weights["adversarial_resilience"]
            + cs * self.weights["consistency"]
        )
        final = clamp(final, 0.0, 100.0)

        return AgentScoreBreakdown(
            optimization_quality=oq,
            response_latency=rl,
            safety_record=sr,
            adversarial_resilience=ar,
            consistency=cs,
            final_score=final,
        )

    @staticmethod
    def tier_from_score(score: float) -> int:
        if score >= 90.0:
            return 3
        if score >= 80.0:
            return 2
        if score >= 70.0:
            return 1
        return 0


# -----------------------------------------------------------------------------
# IntelligenceGate orchestration
# -----------------------------------------------------------------------------


class IntelligenceGate:
    """End-to-end tier orchestration with history and auto-demotion."""

    def __init__(
        self,
        exam: Optional[ChallengeExam] = None,
        sandbox: Optional[SandboxTrial] = None,
        scorer: Optional[AgentScorer] = None,
        seed: int = 13,
    ) -> None:
        self.seed = seed
        self.rng = random.Random(seed)
        self.exam = exam or ChallengeExam(seed=seed)
        self.sandbox = sandbox or SandboxTrial(exam=self.exam, seed=seed + 1)
        self.scorer = scorer or AgentScorer()

        self.tiers: Dict[str, int] = {}
        self.history: Dict[str, List[TierEvent]] = {}

    def _record(self, agent_id: str, tier: int, action: str, score: float, reason: str) -> None:
        self.tiers[agent_id] = tier
        self.history.setdefault(agent_id, []).append(TierEvent(tier=tier, action=action, score=score, reason=reason))

    def _simulate_probation(self, agent: BaseAgent, epochs: int = 30, stake_cap: float = 15.0) -> ProbationResult:
        scores: List[float] = []
        lats: List[float] = []
        pegs: List[float] = []
        violations = 0

        for epoch in range(epochs):
            scenario = self.exam.scenarios[epoch % len(self.exam.scenarios)]
            submission = agent.submit_parameters(scenario, epoch, context={"tier": 2, "stake_cap": stake_cap})
            metrics = self.exam._scenario_metrics(scenario, submission)
            latency_penalty = min(15.0, submission.response_latency_ms / 60.0)
            score = clamp(metrics.scenario_score - latency_penalty, 0.0, 100.0)

            if metrics.cb_misfire_rate > 0.33:
                violations += 1

            scores.append(score)
            lats.append(submission.response_latency_ms)
            pegs.append(metrics.peg_mae)

        avg_score = statistics.mean(scores) if scores else 0.0
        avg_latency = statistics.mean(lats) if lats else 0.0
        peg_mae = statistics.mean(pegs) if pegs else 0.0

        passed = avg_score >= 78.0 and avg_latency <= 450.0 and violations <= max(2, epochs // 12)

        return ProbationResult(
            agent_id=agent.agent_id,
            avg_score=avg_score,
            avg_latency_ms=avg_latency,
            peg_mae=peg_mae,
            safety_violations=violations,
            epochs=epochs,
            stake_cap=stake_cap,
            passed=passed,
        )

    def _adversarial_resilience_estimate(self, agent: BaseAgent, trials: int = 40) -> float:
        defended = 0
        for i in range(trials):
            scenario = self.exam.scenarios[(i + 3) % len(self.exam.scenarios)]
            submission = agent.submit_parameters(scenario, i, context={"adversarial": True})
            metrics = self.exam._scenario_metrics(scenario, submission)
            if metrics.cb_misfire_rate <= 0.25 and metrics.peg_mae <= 0.015:
                defended += 1
        return defended / max(1, trials)

    def evaluate_agent(
        self,
        agent: BaseAgent,
        sandbox_peers: Optional[Sequence[BaseAgent]] = None,
    ) -> GateOutcome:
        challenge = self.exam.evaluate_agent(agent)
        self._record(agent.agent_id, 0, "challenge_exam", challenge.score, "pass" if challenge.passed else "fail")
        if not challenge.passed:
            return GateOutcome(
                agent_id=agent.agent_id,
                current_tier=0,
                admitted=False,
                full_participation=False,
                challenge=challenge,
                sandbox=None,
                probation=None,
                score=None,
                reason="failed_challenge_exam",
            )

        peers = list(sandbox_peers or [])
        # Ensure genuine competition even for single-agent calls.
        if not peers:
            peers = [SmartAgent("benchmark_smart", seed=self.seed + 101), LazyAgent("benchmark_lazy", seed=self.seed + 102)]
        participants = [agent] + [p for p in peers if p.agent_id != agent.agent_id]

        sandbox_results = self.sandbox.run(participants, epochs=100)
        sandbox_result = sandbox_results[agent.agent_id]
        self._record(
            agent.agent_id,
            1,
            "sandbox_trial",
            sandbox_result.final_score,
            "pass" if sandbox_result.passed else "fail",
        )
        if not sandbox_result.passed:
            return GateOutcome(
                agent_id=agent.agent_id,
                current_tier=1,
                admitted=False,
                full_participation=False,
                challenge=challenge,
                sandbox=sandbox_result,
                probation=None,
                score=None,
                reason="failed_sandbox_trial",
            )

        probation = self._simulate_probation(agent, epochs=30, stake_cap=15.0)
        self._record(
            agent.agent_id,
            2,
            "probation",
            probation.avg_score,
            "pass" if probation.passed else "fail",
        )
        if not probation.passed:
            return GateOutcome(
                agent_id=agent.agent_id,
                current_tier=2,
                admitted=False,
                full_participation=False,
                challenge=challenge,
                sandbox=sandbox_result,
                probation=probation,
                score=None,
                reason="failed_probation",
            )

        score_input = AgentScoreInput(
            peg_mae=(challenge.peg_mae + sandbox_result.peg_mae + probation.peg_mae) / 3.0,
            avg_latency_ms=(challenge.details.get("avg_latency_ms", sandbox_result.avg_latency_ms) + sandbox_result.avg_latency_ms + probation.avg_latency_ms) / 3.0,
            invariant_violations=probation.safety_violations,
            cb_evasion_rate=(challenge.cb_misfire_rate + sandbox_result.copied_ratio) / 2.0,
            attack_defense_rate=self._adversarial_resilience_estimate(agent),
            performance_variance=sandbox_result.consistency_std,
        )
        score = self.scorer.compute(score_input)

        inferred_tier = self.scorer.tier_from_score(score.final_score)
        final_tier = 3 if inferred_tier >= 3 else 2
        self._record(agent.agent_id, final_tier, "full_participation", score.final_score, "granted" if final_tier == 3 else "limited")

        return GateOutcome(
            agent_id=agent.agent_id,
            current_tier=final_tier,
            admitted=True,
            full_participation=(final_tier == 3),
            challenge=challenge,
            sandbox=sandbox_result,
            probation=probation,
            score=score,
            reason="admitted_tier_3" if final_tier == 3 else "admitted_tier_2",
        )

    def apply_monitoring_score(self, agent_id: str, latest_score: float) -> int:
        """Apply continuous monitoring result and auto-promotion/demotion."""
        current = self.tiers.get(agent_id, 0)
        inferred = self.scorer.tier_from_score(latest_score)

        # Monitoring can demote from Tier 3 to 2/1/0; upward movement is bounded.
        new_tier = current
        if inferred < current:
            new_tier = inferred
            self._record(agent_id, new_tier, "auto_demotion", latest_score, "score_drop")
        elif inferred > current and current < 3:
            new_tier = inferred
            self._record(agent_id, new_tier, "auto_promotion", latest_score, "score_recovery")

        self.tiers[agent_id] = new_tier
        return new_tier

    def get_tier(self, agent_id: str) -> int:
        return self.tiers.get(agent_id, 0)

    def get_history(self, agent_id: str) -> List[TierEvent]:
        return list(self.history.get(agent_id, []))


__all__ = [
    "MarketScenario",
    "ParameterSubmission",
    "ScenarioMetrics",
    "ChallengeExamResult",
    "SandboxAgentResult",
    "ProbationResult",
    "AgentScoreInput",
    "AgentScoreBreakdown",
    "TierEvent",
    "GateOutcome",
    "BaseAgent",
    "SmartAgent",
    "RandomAgent",
    "CopyAgent",
    "LazyAgent",
    "MaliciousAgent",
    "ChallengeExam",
    "SandboxTrial",
    "AgentScorer",
    "IntelligenceGate",
]
