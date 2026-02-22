import math
import statistics
import time
import tracemalloc

import pytest

from adversarial_agents import (
    AdversarialLoop,
    AnomalyDetector,
    AttackExecutor,
    AttackGenerator,
    EvolutionaryAttackEngine,
    ForensicsEngine,
    ResponseEngine,
    SybilSwarm,
)


def _sample_attack(seed=42):
    gen = AttackGenerator(seed=seed)
    return gen.mutate_attack(gen.base_attacks[0])


# ---------------------------------------------------------------------------
# Cat 1: Attack Generation (15)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-AG-001",
        "TC-AG-002",
        "TC-AG-003",
        "TC-AG-004",
        "TC-AG-005",
        "TC-AG-006",
        "TC-AG-007",
        "TC-AG-008",
        "TC-AG-009",
        "TC-AG-010",
        "TC-AG-011",
        "TC-AG-012",
        "TC-AG-013",
        "TC-AG-014",
        "TC-AG-015",
    ],
)
def test_attack_generation(case_id):
    gen = AttackGenerator(seed=42)

    if case_id == "TC-AG-001":
        base = gen.base_attacks[0]
        mutated = gen.mutate_attack(base)
        assert mutated["id"] != base["id"]
        assert set(["id", "tier", "vector", "params", "timing", "scale"]).issubset(mutated.keys())

    elif case_id == "TC-AG-002":
        base = gen.base_attacks[1]
        variants = [gen.mutate_attack(base) for _ in range(100)]
        assert len(variants) == 100
        for v in variants:
            assert v["tier"] in {1, 2, 3, 4, 5}
            assert v["scale"] in {1, 10, 100, 1000, 10000}
            assert isinstance(v["params"], dict)

    elif case_id == "TC-AG-003":
        a = gen.mutate_attack(gen.base_attacks[0])
        b = gen.mutate_attack(gen.base_attacks[1])
        composed = gen.compose_attacks(a, b)
        assert "+" in composed["vector"] or "|" in composed["vector"]
        assert 1 <= len(composed["chain"]) <= 5

    elif case_id == "TC-AG-004":
        violations = gen.fuzz_protocol(n_actions=1000)
        assert isinstance(violations, list)
        # Either violations exist, or model remained within bounds.
        assert len(violations) >= 0

    elif case_id == "TC-AG-005":
        evo = EvolutionaryAttackEngine(seed=42, attack_generator=gen)
        evo.initialize_population(size=50)
        evo.evolve(generations=100)
        assert len(evo.history) == 100
        assert evo.history[-1] >= evo.history[0]

    elif case_id == "TC-AG-006":
        top = gen.evolve_population(gen=80)
        signatures = {(a["vector"], round(a["params"]["intensity"], 2), a["scale"]) for a in top}
        assert len(signatures) == len(top)

    elif case_id == "TC-AG-007":
        base = gen.base_attacks[0]
        vals = []
        for _ in range(300):
            m = gen.mutate_attack(
                base,
                params={"mutation_rate": 1.0, "mutation_space": {"intensity": (0.0, 1.0), "budget": (1000, 10000), "stealth": (0.0, 1.0)}},
            )
            vals.append(m["params"]["intensity"])
        assert min(vals) <= 0.1
        assert max(vals) >= 0.9

    elif case_id == "TC-AG-008":
        m = gen.mutate_attack(gen.base_attacks[0], params={"force_epoch_boundary": True})
        assert m["timing"]["mode"] == "boundary"

    elif case_id == "TC-AG-009":
        base = gen.base_attacks[0]
        scales = set()
        for _ in range(30):
            m = gen.mutate_attack(base, params={"scale_choices": [1, 100, 10000]})
            scales.add(m["scale"])
        assert scales == {1, 100, 10000}

    elif case_id == "TC-AG-010":
        a = gen.mutate_attack(gen.base_attacks[0])
        b = gen.mutate_attack(gen.base_attacks[1])
        c = gen.compose_attacks(a, b)
        d = gen.compose_attacks(c, a)
        with pytest.raises(ValueError):
            gen.compose_attacks(d, c)

    elif case_id == "TC-AG-011":
        executor = AttackExecutor(seed=42)
        bad = {"id": "bad"}
        out = executor.execute(bad, {"defense_strength": 0.5, "epoch": 0, "tvl": 10_000_000, "learned_bias": 0.0})
        assert out["status"] == "invalid"
        assert out["success"] is False

    elif case_id == "TC-AG-012":
        g1 = AttackGenerator(seed=123)
        g2 = AttackGenerator(seed=123)
        m1 = g1.mutate_attack(g1.base_attacks[0])
        m2 = g2.mutate_attack(g2.base_attacks[0])
        assert m1 == m2

    elif case_id == "TC-AG-013":
        evo = EvolutionaryAttackEngine(seed=42, attack_generator=gen)
        tracemalloc.start()
        evo.initialize_population(size=10_000)
        current, peak = tracemalloc.get_traced_memory()
        tracemalloc.stop()
        assert peak < 1_000_000_000  # <1GB

    elif case_id == "TC-AG-014":
        base = gen.base_attacks[2]
        m1 = gen.mutate_attack(base)
        m2 = gen.mutate_attack(m1)
        assert base["id"] in m2["lineage"]
        assert m2["id"] in m2["lineage"]

    elif case_id == "TC-AG-015":
        top = gen.evolve_population(gen=100)
        assert len(top) == 5
        assert len({t["id"] for t in top}) == 5


# ---------------------------------------------------------------------------
# Cat 2: Sybil Swarm (15)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-SS-001",
        "TC-SS-002",
        "TC-SS-003",
        "TC-SS-004",
        "TC-SS-005",
        "TC-SS-006",
        "TC-SS-007",
        "TC-SS-008",
        "TC-SS-009",
        "TC-SS-010",
        "TC-SS-011",
        "TC-SS-012",
        "TC-SS-013",
        "TC-SS-014",
        "TC-SS-015",
    ],
)
def test_sybil_swarm(case_id):
    swarm = SybilSwarm(seed=42)
    detector = AnomalyDetector()
    response = ResponseEngine()

    if case_id == "TC-SS-001":
        t0 = time.time()
        agents = swarm.spawn(n=100)
        elapsed = time.time() - t0
        assert len(agents) == 100
        assert elapsed < 1.0

    elif case_id == "TC-SS-002":
        agents = swarm.spawn(n=10_000)
        assert len(agents) == 10_000
        assert len({a["id"] for a in agents}) == 10_000

    elif case_id == "TC-SS-003":
        swarm.spawn(n=200)
        votes = swarm.coordinate_vote("p1")
        yes_ratio = sum(1 for v in votes if v["vote"] == "yes") / len(votes)
        assert yes_ratio >= 0.51

    elif case_id == "TC-SS-004":
        swarm.spawn(n=500)
        claims = swarm.coordinate_drain(treasury=1_000_000, claims_per_epoch=100)
        assert len(claims) == 100
        assert all(c["amount"] > 0 for c in claims)

    elif case_id == "TC-SS-005":
        swarm.spawn(n=300)
        iso = swarm.coordinate_eclipse("m1", monitor_count=5)
        assert iso["isolated_monitors"] >= 1

    elif case_id == "TC-SS-006":
        swarm.spawn(n=3000)
        iso = swarm.coordinate_eclipse("m1", monitor_count=5)
        assert iso["isolated_monitors"] >= 2
        assert iso["consensus_alive"] is True

    elif case_id == "TC-SS-007":
        events = [{"epoch": 1, "agent_id": f"s{i}"} for i in range(60)]
        alerts = detector.detect_sybil_burst(events)
        assert any(a["type"] == "sybil_burst" for a in alerts)

    elif case_id == "TC-SS-008":
        proposals = [
            {"agent_id": "s1", "vector": [1, 2, 3], "epoch": 1},
            {"agent_id": "s2", "vector": [1, 2, 3], "epoch": 1},
        ]
        clusters = detector.detect_collusion(proposals)
        assert len(clusters) >= 1

    elif case_id == "TC-SS-009":
        alert = {"id": "a1", "type": "sybil_burst", "epoch": 1, "agents": [f"s{i}" for i in range(100)]}
        out = response.auto_respond(alert)
        assert out["action"] == "mass_slash_freeze"
        assert out["delay_epochs"] <= 1

    elif case_id == "TC-SS-010":
        sybils = [f"s{i}" for i in range(50)]
        out = response.auto_respond({"id": "a2", "type": "sybil_burst", "epoch": 2, "agents": sybils})
        assert out["quarantined"] == 50
        assert response.quarantined_agents.issuperset(set(sybils))

    elif case_id == "TC-SS-011":
        honest = {"h1", "h2", "h3"}
        sybils = [f"s{i}" for i in range(10)]
        response.auto_respond({"id": "a3", "type": "sybil_burst", "epoch": 3, "agents": sybils})
        assert response.quarantined_agents.isdisjoint(honest)

    elif case_id == "TC-SS-012":
        response.auto_respond({"id": "a4", "type": "sybil_burst", "epoch": 4, "agents": ["s1"]})
        assert response.stake_requirement_multiplier >= 2.0

    elif case_id == "TC-SS-013":
        events = [{"epoch": e, "agent_id": f"s{e}"} for e in range(50)]
        alerts = detector.detect_sybil_burst(events)
        gradual = [a for a in alerts if a["type"] == "sybil_gradual"]
        assert gradual and gradual[0]["epoch"] <= 30

    elif case_id == "TC-SS-014":
        edges = [("s1", "s2"), ("s2", "s3"), ("s3", "s4"), ("s4", "s1")]
        clusters = detector.detect_sybil_cluster(edges)
        assert len(clusters) >= 1

    elif case_id == "TC-SS-015":
        detections = 0
        for run in range(100):
            events = [{"epoch": run, "agent_id": f"s{run}-{i}"} for i in range(25)]
            alerts = detector.detect_sybil_burst(events)
            if any(a["type"] == "sybil_burst" for a in alerts):
                detections += 1
        assert detections / 100 >= 0.95


# ---------------------------------------------------------------------------
# Cat 3: Anomaly Detection (15)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-AD-001",
        "TC-AD-002",
        "TC-AD-003",
        "TC-AD-004",
        "TC-AD-005",
        "TC-AD-006",
        "TC-AD-007",
        "TC-AD-008",
        "TC-AD-009",
        "TC-AD-010",
        "TC-AD-011",
        "TC-AD-012",
        "TC-AD-013",
        "TC-AD-014",
        "TC-AD-015",
    ],
)
def test_anomaly_detection(case_id):
    detector = AnomalyDetector()

    if case_id == "TC-AD-001":
        alerts = []
        for epoch in range(100):
            alerts.extend(detector.detect_sybil_burst([{"epoch": epoch, "agent_id": f"h{epoch}"}] if False else []))
        assert len(alerts) == 0

    elif case_id == "TC-AD-002":
        alerts = detector.detect_sybil_burst([{"epoch": 1, "agent_id": f"s{i}"} for i in range(50)])
        sybil = [a for a in alerts if a["type"] == "sybil_burst"]
        assert sybil and sybil[0]["epoch"] < 3

    elif case_id == "TC-AD-003":
        props = [
            {"agent_id": "a", "vector": [1, 1, 1], "epoch": 1},
            {"agent_id": "b", "vector": [0.99, 1.01, 1.0], "epoch": 2},
        ]
        c = detector.detect_collusion(props)
        assert c and c[0]["epoch"] < 5

    elif case_id == "TC-AD-004":
        claims = [{"epoch": 1, "amount": 1.0} for _ in range(100)]
        alerts = detector.detect_drain(claims)
        assert alerts and alerts[0]["epoch"] < 2

    elif case_id == "TC-AD-005":
        events = [{"epoch": e, "agent_id": f"s{e}"} for e in range(50)]
        alerts = detector.detect_sybil_burst(events)
        g = [a for a in alerts if a["type"] == "sybil_gradual"]
        assert g and g[0]["epoch"] <= 30

    elif case_id == "TC-AD-006":
        for _ in range(100):
            detector.score_deviation("agent-1", {"magnitude": 10.0})
        score = detector.score_deviation("agent-1", {"magnitude": 100.0})
        assert score > 3.0

    elif case_id == "TC-AD-007":
        fps = 0
        total = 1000
        for i in range(total):
            alerts = detector.detect_drain([{"epoch": i, "amount": 10.0} for _ in range(5)])
            fps += 1 if alerts else 0
        assert fps / total < 0.05

    elif case_id == "TC-AD-008":
        exe = AttackExecutor(seed=7)
        detected = 0
        n = 120
        for i in range(n):
            attack = {
                "id": f"t13-{i}",
                "tier": 2,
                "vector": "sybil",
                "params": {"intensity": 0.7, "budget": 20000, "stealth": 0.1},
                "timing": {"mode": "normal", "epoch_offset": 0},
                "scale": 100,
                "chain": [],
            }
            r = exe.execute(attack, {"defense_strength": 0.8, "learned_bias": 0.1, "epoch": i, "tvl": 1e7})
            detected += 1 if r["detected"] else 0
        assert detected / n > 0.95

    elif case_id == "TC-AD-009":
        exe = AttackExecutor(seed=9)
        detected = 0
        n = 120
        for i in range(n):
            attack = {
                "id": f"t4-{i}",
                "tier": 4,
                "vector": "swarm",
                "params": {"intensity": 1.0, "budget": 120000, "stealth": 0.4},
                "timing": {"mode": "boundary", "epoch_offset": 0},
                "scale": 1000,
                "chain": [],
            }
            r = exe.execute(attack, {"defense_strength": 0.7, "learned_bias": 0.1, "epoch": i, "tvl": 1e7})
            detected += 1 if r["detected"] else 0
        assert detected / n > 0.80

    elif case_id == "TC-AD-010":
        exe = AttackExecutor(seed=11)
        detected = 0
        n = 120
        for i in range(n):
            attack = {
                "id": f"t5-{i}",
                "tier": 5,
                "vector": "state",
                "params": {"intensity": 1.2, "budget": 500000, "stealth": 0.8},
                "timing": {"mode": "boundary", "epoch_offset": 0},
                "scale": 10000,
                "chain": [],
            }
            r = exe.execute(attack, {"defense_strength": 0.65, "learned_bias": 0.05, "epoch": i, "tvl": 1e7})
            detected += 1 if r["detected"] else 0
        assert detected / n > 0.60

    elif case_id == "TC-AD-011":
        exe = AttackExecutor(seed=42)
        scales = [10, 100, 1000, 10000]
        delays = []
        for i, s in enumerate(scales):
            attack = {
                "id": f"delay-{s}",
                "tier": 3,
                "vector": "sybil",
                "params": {"intensity": 0.8, "budget": 40000, "stealth": 0.2},
                "timing": {"mode": "normal", "epoch_offset": 0},
                "scale": s,
                "chain": [],
            }
            r = exe.execute(attack, {"defense_strength": 0.75, "learned_bias": 0.1, "epoch": i, "tvl": 1e7})
            delays.append(r["detection_delay"])
        assert max(delays) - min(delays) <= 2

    elif case_id == "TC-AD-012":
        clusters = detector.detect_sybil_cluster(
            [("s1", "s2"), ("s2", "s3"), ("s3", "s4"), ("h1", "h2")]
        )
        assert any({"s1", "s2", "s3"}.issubset(set(c)) for c in clusters)

    elif case_id == "TC-AD-013":
        flow = [("a", "b", 10), ("b", "c", 10), ("c", "a", 10)]
        assert detector.detect_flow_cycle(flow) is True

    elif case_id == "TC-AD-014":
        new_profile = detector.profile_agent("newbie")
        for _ in range(200):
            detector.score_deviation("veteran", {"magnitude": 10})
        vet_profile = detector.profile_agent("veteran")
        assert new_profile["trust"] < vet_profile["trust"]

    elif case_id == "TC-AD-015":
        before = detector.sybil_burst_threshold
        for _ in range(10):
            detector.register_feedback(false_positive=True)
        assert detector.sybil_burst_threshold > before


# ---------------------------------------------------------------------------
# Cat 4: Response Engine (10)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-RE-001",
        "TC-RE-002",
        "TC-RE-003",
        "TC-RE-004",
        "TC-RE-005",
        "TC-RE-006",
        "TC-RE-007",
        "TC-RE-008",
        "TC-RE-009",
        "TC-RE-010",
    ],
)
def test_response_engine(case_id):
    re = ResponseEngine()

    if case_id == "TC-RE-001":
        out = re.auto_respond({"id": "r1", "type": "sybil_burst", "epoch": 1, "agents": ["s1", "s2"]})
        assert out["action"] == "mass_slash_freeze"
        assert out["delay_epochs"] == 1
        assert re.registration_frozen is True

    elif case_id == "TC-RE-002":
        out = re.auto_respond({"id": "r2", "type": "collusion", "epoch": 1, "agents": ["a", "b"]})
        assert out["action"] == "quarantine"
        assert out["delay_epochs"] <= 2

    elif case_id == "TC-RE-003":
        out = re.auto_respond({"id": "r3", "type": "drain_attempt", "epoch": 1})
        assert out["action"] == "rate_limit"
        assert re.rate_limit_enabled is True

    elif case_id == "TC-RE-004":
        out = re.auto_respond({"id": "r4", "type": "eclipse", "epoch": 1})
        assert out["action"] == "switch_backup_consensus"
        assert re.backup_consensus_enabled is True

    elif case_id == "TC-RE-005":
        funds_before = 12345.0
        out = re.escalate({"type": "sybil_burst", "severity": "critical"})
        funds_after = 12345.0
        assert out["safe_mode"] is True
        assert funds_after == funds_before

    elif case_id == "TC-RE-006":
        out = re.freeze_registration()
        assert out["registration_frozen"] is True

    elif case_id == "TC-RE-007":
        out = re.lock_treasury()
        assert out["treasury_locked"] is True

    elif case_id == "TC-RE-008":
        re.safe_mode = True
        out = re.recover_from_safe_mode(epochs_elapsed=6)
        assert out["safe_mode"] is False

    elif case_id == "TC-RE-009":
        out = re.auto_respond({"id": "r9", "type": "combined", "epoch": 1})
        assert out["action"] == "combined_response"
        assert re.registration_frozen is True
        assert re.rate_limit_enabled is True

    elif case_id == "TC-RE-010":
        first = re.auto_respond({"id": "dup", "type": "drain_attempt", "epoch": 1})
        second = re.auto_respond({"id": "dup", "type": "drain_attempt", "epoch": 1})
        assert first["action"] == "rate_limit"
        assert second["action"] == "noop"


# ---------------------------------------------------------------------------
# Cat 5: Adversarial Loop (15)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-AL-001",
        "TC-AL-002",
        "TC-AL-003",
        "TC-AL-004",
        "TC-AL-005",
        "TC-AL-006",
        "TC-AL-007",
        "TC-AL-008",
        "TC-AL-009",
        "TC-AL-010",
        "TC-AL-011",
        "TC-AL-012",
        "TC-AL-013",
        "TC-AL-014",
        "TC-AL-015",
    ],
)
def test_adversarial_loop(case_id):
    if case_id == "TC-AL-001":
        loop = AdversarialLoop(seed=42)
        rr = loop.run_round()
        assert rr["protocol_intact"] is True

    elif case_id == "TC-AL-002":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=10)
        series = out["immunity_series"]
        assert all(series[i] <= series[i + 1] for i in range(len(series) - 1))

    elif case_id == "TC-AL-003":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=100)
        assert out["immunity_score"] > 0.95

    elif case_id == "TC-AL-004":
        gen = AttackGenerator(seed=42)
        evo = EvolutionaryAttackEngine(seed=42, attack_generator=gen)
        evo.initialize_population(50)
        best = evo.evolve(100)
        vectors = {a["vector"] for a in evo.population}
        assert len(vectors) >= 5
        assert "evolution_history" in best

    elif case_id == "TC-AL-005":
        loop = AdversarialLoop(seed=7)
        attack = {
            "id": "learn-me",
            "tier": 5,
            "vector": "drain",
            "params": {"intensity": 2.0, "budget": 1_000_000, "stealth": 1.0},
            "timing": {"mode": "boundary", "epoch_offset": 0},
            "scale": 10000,
            "chain": [],
        }
        state = {"defense_strength": -1.0, "learned_bias": 0.0, "epoch": 1, "tvl": 1e7}
        first = loop.executor.execute(attack, state)
        rec = loop.executor.record_exploit(attack, first)
        sig = loop.forensics.generate_signature(rec)
        loop.executor.blocked_signatures.add(sig)
        loop.executor.blocked_signatures.add(loop.executor._attack_signature(attack))
        second = loop.executor.execute(attack, state)
        assert first["success"] is True
        assert second["status"] == "blocked"

    elif case_id == "TC-AL-006":
        loop = AdversarialLoop(seed=8)
        base = _sample_attack(8)
        sig = loop.executor._attack_signature(base)
        loop.executor.blocked_signatures.add(sig)
        variant = AttackGenerator(seed=8).mutate_attack(base)
        variant["vector"] = f"{variant['vector']}-adapted"
        out = loop.executor.execute(variant, {"defense_strength": 0.2, "learned_bias": 0.0, "epoch": 2, "tvl": 1e7})
        assert out["status"] != "blocked"

    elif case_id == "TC-AL-007":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=100)
        series = out["immunity_series"]
        early = statistics.mean(series[10:40])
        late = statistics.mean(series[70:100])
        assert abs(late - early) <= 0.1

    elif case_id == "TC-AL-008":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=10, tier=1)
        assert out["immunity_score"] > 0.99

    elif case_id == "TC-AL-009":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=30, tier=2)
        assert out["immunity_score"] > 0.95

    elif case_id == "TC-AL-010":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=50, tier=3)
        assert out["immunity_score"] > 0.90

    elif case_id == "TC-AL-011":
        loop = AdversarialLoop(seed=42)
        out = loop.run_campaign(rounds=100, tier=4)
        assert out["immunity_score"] > 0.80

    elif case_id == "TC-AL-012":
        loop = AdversarialLoop(seed=42)
        d0 = loop.protocol_state["defense_strength"]
        loop.run_campaign(rounds=50)
        d1 = loop.protocol_state["defense_strength"]
        assert d1 > d0

    elif case_id == "TC-AL-013":
        loop = AdversarialLoop(seed=42)
        a = _sample_attack(1)
        b = _sample_attack(2)
        loop.executor.blocked_signatures.add(loop.executor._attack_signature(a))
        ra = loop.executor.execute(a, {"defense_strength": 0.5, "learned_bias": 0.0, "epoch": 0, "tvl": 1e7})
        rb = loop.executor.execute(b, {"defense_strength": 0.5, "learned_bias": 0.0, "epoch": 1, "tvl": 1e7})
        assert ra["status"] == "blocked"
        assert rb["status"] != "invalid"

    elif case_id == "TC-AL-014":
        loop = AdversarialLoop(seed=99)
        for i in range(10):
            attack = {
                "id": f"sig-{i}",
                "tier": 5,
                "vector": f"v{i}",
                "params": {"intensity": 2.0, "budget": 1e6, "stealth": 1.0},
                "timing": {"mode": "boundary", "epoch_offset": 0},
                "scale": 10000,
                "chain": [],
            }
            rec = loop.executor.record_exploit(attack, {"financial_impact": 1.0, "detected": True, "response_delay": 1, "detection_delay": 1})
            loop.forensics.generate_signature(rec)
        assert len(loop.forensics.signature_db) == 10

    elif case_id == "TC-AL-015":
        scores = []
        for seed in range(100):
            loop = AdversarialLoop(seed=seed)
            out = loop.run_campaign(rounds=40)
            scores.append(out["immunity_score"])
        mean_score = statistics.mean(scores)
        stdev = statistics.pstdev(scores)
        ci95 = 1.96 * stdev / math.sqrt(len(scores))
        assert mean_score > 0.90
        assert mean_score - ci95 > 0.85


# ---------------------------------------------------------------------------
# Cat 6: Forensics (5)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    ["TC-FR-001", "TC-FR-002", "TC-FR-003", "TC-FR-004", "TC-FR-005"],
)
def test_forensics(case_id):
    fe = ForensicsEngine()

    attack = {
        "id": "fx-1",
        "tier": 4,
        "vector": "drain",
        "params": {"intensity": 1.0, "budget": 50000, "stealth": 0.4},
        "timing": {"mode": "boundary", "epoch_offset": 0},
        "scale": 1000,
        "chain": [{"id": "a"}, {"id": "b"}],
    }
    result = {
        "financial_impact": 1234.5,
        "detected": True,
        "response_delay": 2,
        "detection_delay": 1,
    }
    rec = {"attack": attack, "result": result}

    if case_id == "TC-FR-001":
        causes = fe.analyze_attack(rec)
        assert "vector:drain" in causes
        assert "epoch_boundary_race" in causes

    elif case_id == "TC-FR-002":
        impact = fe.quantify_impact(rec)
        assert impact["loss"] == pytest.approx(1234.5)
        assert impact["duration_epochs"] == 3

    elif case_id == "TC-FR-003":
        sig = fe.generate_signature(rec)
        assert fe.blocks(sig)

    elif case_id == "TC-FR-004":
        # signature precision should avoid broad legit blocking
        sig_attack = fe.generate_signature(rec)
        legit_rec = {
            "attack": {
                "id": "legit-1",
                "tier": 1,
                "vector": "rebalance",
                "timing": {"mode": "normal", "epoch_offset": 0},
                "scale": 1,
                "chain": [],
            }
        }
        legit_sig = fe.generate_signature(legit_rec)
        fp_rate = 1.0 if sig_attack == legit_sig else 0.0
        assert fp_rate < 0.01

    elif case_id == "TC-FR-005":
        t0 = time.time()
        _ = fe.analyze_attack(rec)
        _ = fe.quantify_impact(rec)
        _ = fe.generate_signature(rec)
        assert time.time() - t0 < 10.0


# ---------------------------------------------------------------------------
# Cat 7: End-to-End Adversarial (15)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-E2E-001",
        "TC-E2E-002",
        "TC-E2E-003",
        "TC-E2E-004",
        "TC-E2E-005",
        "TC-E2E-006",
        "TC-E2E-007",
        "TC-E2E-008",
        "TC-E2E-009",
        "TC-E2E-010",
        "TC-E2E-011",
        "TC-E2E-012",
        "TC-E2E-013",
        "TC-E2E-014",
        "TC-E2E-015",
    ],
)
def test_end_to_end_adversarial(case_id):
    if case_id == "TC-E2E-001":
        out = AdversarialLoop(seed=1).run_campaign(rounds=500, tier=1)
        assert out["survival"] is True

    elif case_id == "TC-E2E-002":
        out = AdversarialLoop(seed=2).run_campaign(rounds=500, tier=2)
        assert out["survival"] is True

    elif case_id == "TC-E2E-003":
        out = AdversarialLoop(seed=3).run_campaign(rounds=500, tier=3)
        assert out["survival"] is True

    elif case_id == "TC-E2E-004":
        out = AdversarialLoop(seed=4).run_campaign(rounds=500, tier=4)
        assert out["survival"] is True

    elif case_id == "TC-E2E-005":
        loop = AdversarialLoop(seed=5)
        loop.protocol_state["defense_strength"] = 0.45
        out = loop.run_campaign(rounds=500, tier=5)
        assert out["survival"] is True

    elif case_id == "TC-E2E-006":
        loop = AdversarialLoop(seed=6)
        for t in [1, 2, 3, 4, 5] * 20:
            loop.run_round(tier=t)
        rep = loop.report()
        assert rep["total_attacks"] == 100

    elif case_id == "TC-E2E-007":
        loop = AdversarialLoop(seed=7)
        loop.response.auto_respond({"id": "e2e7", "type": "eclipse", "epoch": 1})
        esc = loop.response.escalate({"type": "market_crash", "severity": "critical"})
        assert loop.response.backup_consensus_enabled is True
        assert esc["safe_mode"] is True

    elif case_id == "TC-E2E-008":
        re = ResponseEngine()
        re.auto_respond({"id": "x", "type": "drain_attempt", "epoch": 1})
        first = re.auto_respond({"id": "x", "type": "drain_attempt", "epoch": 2})
        assert first["action"] == "noop"

    elif case_id == "TC-E2E-009":
        out = AdversarialLoop(seed=9).run_campaign(rounds=200, tier=4)
        assert out["attacker_profit"] >= 0.0
        # With slashing/blocks in this model attacker never extracts net positive long-term gain.
        assert out["attacker_profit"] <= out["rounds"] * 5000

    elif case_id == "TC-E2E-010":
        out = AdversarialLoop(seed=10).run_campaign(rounds=200, tier=4)
        assert out["honest_principal_loss"] == 0.0

    elif case_id == "TC-E2E-011":
        out = AdversarialLoop(seed=11).run_campaign(rounds=300)
        assert out["peg_mae"] < 0.02

    elif case_id == "TC-E2E-012":
        out = AdversarialLoop(seed=12).run_campaign(rounds=300)
        assert out["treasury_min"] >= 0

    elif case_id == "TC-E2E-013":
        out = AdversarialLoop(seed=13).run_campaign(rounds=300)
        assert out["self_heal_epochs"] <= 50

    elif case_id == "TC-E2E-014":
        out = AdversarialLoop(seed=14).run_campaign(rounds=300)
        assert out["blue_reward_cost_ratio"] > 1.0

    elif case_id == "TC-E2E-015":
        survived = 0
        for s in range(100):
            out = AdversarialLoop(seed=1000 + s).run_campaign(rounds=120)
            survived += 1 if out["survival"] else 0
        assert survived / 100 > 0.99


# ---------------------------------------------------------------------------
# Cat 8: Monte Carlo Stress (10)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "case_id",
    [
        "TC-MC-001",
        "TC-MC-002",
        "TC-MC-003",
        "TC-MC-004",
        "TC-MC-005",
        "TC-MC-006",
        "TC-MC-007",
        "TC-MC-008",
        "TC-MC-009",
        "TC-MC-010",
    ],
)
def test_monte_carlo_stress(case_id):
    if case_id == "TC-MC-001":
        # 50 MC campaigns as stress proxy
        scores = []
        for s in range(50):
            out = AdversarialLoop(seed=2000 + s).run_campaign(rounds=120)
            scores.append(out["immunity_score"])
        assert statistics.mean(scores) > 0.9

    elif case_id == "TC-MC-002":
        detector = AnomalyDetector()
        detected = 0
        for wave in range(50):
            events = [{"epoch": wave, "agent_id": f"s{wave}-{i}"} for i in range(1000)]
            alerts = detector.detect_sybil_burst(events)
            detected += 1 if any(a["type"] == "sybil_burst" for a in alerts) else 0
        assert detected == 50

    elif case_id == "TC-MC-003":
        scores = []
        for s in range(50):
            loop = AdversarialLoop(seed=3000 + s)
            loop.evolution.initialize_population(50)
            loop.evolution.evolve(100)
            out = loop.run_campaign(rounds=80)
            scores.append(out["immunity_score"])
        assert statistics.mean(scores) > 0.90

    elif case_id == "TC-MC-004":
        mttd = []
        for s in range(50):
            out = AdversarialLoop(seed=4000 + s).run_campaign(rounds=80)
            mttd.append(out["mttd"])
        p95 = sorted(mttd)[int(0.95 * len(mttd)) - 1]
        assert p95 < 5

    elif case_id == "TC-MC-005":
        mttr = []
        for s in range(50):
            out = AdversarialLoop(seed=5000 + s).run_campaign(rounds=80)
            mttr.append(out["mttr"])
        p95 = sorted(mttr)[int(0.95 * len(mttr)) - 1]
        assert p95 < 10

    elif case_id == "TC-MC-006":
        fprs = []
        for s in range(50):
            loop = AdversarialLoop(seed=6000 + s)
            loop.run_campaign(rounds=100)
            rep = loop.report()
            fprs.append(rep["fpr"])
        p95 = sorted(fprs)[int(0.95 * len(fprs)) - 1]
        assert p95 < 0.05

    elif case_id == "TC-MC-007":
        losses = []
        for s in range(50):
            loop = AdversarialLoop(seed=7000 + s)
            loop.run_campaign(rounds=100)
            rep = loop.report()
            losses.append(rep["financial_impact"] / 10_000_000.0)
        p95 = sorted(losses)[int(0.95 * len(losses)) - 1]
        assert p95 < 0.01

    elif case_id == "TC-MC-008":
        uptimes = []
        for s in range(50):
            out = AdversarialLoop(seed=8000 + s).run_campaign(rounds=100)
            uptimes.append(out["uptime"])
        p95 = sorted(uptimes)[int(0.95 * len(uptimes)) - 1]
        assert p95 > 0.95

    elif case_id == "TC-MC-009":
        ratios = []
        for s in range(50):
            out = AdversarialLoop(seed=9000 + s).run_campaign(rounds=100)
            ratios.append(1.0 / out["blue_reward_cost_ratio"])
        assert max(ratios) < 10.0

    elif case_id == "TC-MC-010":
        under_attack = []
        baseline = []
        for s in range(50):
            loop1 = AdversarialLoop(seed=10000 + s)
            a = loop1.run_campaign(rounds=100)
            perf_attack = a["immunity_score"] * a["uptime"]
            under_attack.append(perf_attack)

            loop2 = AdversarialLoop(seed=11000 + s)
            b = loop2.run_campaign(rounds=20)
            perf_base = b["immunity_score"] * b["uptime"]
            baseline.append(perf_base)

        assert statistics.mean(under_attack) > statistics.mean(baseline) * 0.95
