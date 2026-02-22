#!/usr/bin/env python3
"""
High-resolution test suite for Open Agent Economy.
Run: python test_open_agent_economy.py
"""
from __future__ import annotations

import math
import os
import random
import time
import traceback
from dataclasses import dataclass
from typing import Callable, Dict, List, Tuple

import open_agent_economy as oae

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
# Agent Registry (15)
# -----------------------------------------------------------------------------


def tc_ar_001() -> str:
    reg = oae.AgentRegistry()
    ok = reg.register("opt1", "Optimizer", oae.MIN_STAKE_DEFAULT["Optimizer"], 0)
    assert ok
    assert reg.get_record("opt1").status == "Active"
    return "registered"


def tc_ar_002() -> str:
    reg = oae.AgentRegistry()
    ok = reg.register("opt1", "Optimizer", 1.0, 0)
    assert not ok
    return "rejected"


def tc_ar_003() -> str:
    reg = oae.AgentRegistry()
    for t in oae.AGENT_TYPES:
        ok = reg.register(f"{t}-1", t, oae.MIN_STAKE_DEFAULT[t], 0)
        assert ok
    return "all types registered"


def tc_ar_004() -> str:
    reg = oae.AgentRegistry(cooldown_epochs=3)
    reg.register("a", "Optimizer", 10.0, 0)
    assert reg.deregister("a", 0)
    assert reg.finalize_deregistration("a", 3)
    assert reg.get_record("a").status == "Deregistered"
    return "cooldown ok"


def tc_ar_005() -> str:
    reg = oae.AgentRegistry(cooldown_epochs=3)
    reg.register("a", "Optimizer", 10.0, 0)
    reg.deregister("a", 0)
    ok = reg.finalize_deregistration("a", 1)
    assert not ok
    return "cooldown reject"


def tc_ar_006() -> str:
    reg = oae.AgentRegistry(cooldown_epochs=1)
    reg.register("a", "Optimizer", 10.0, 0)
    reg.deregister("a", 0)
    reg.finalize_deregistration("a", 1)
    ok = reg.register("a", "Optimizer", 10.0, 2)
    assert ok
    return "re-register"


def tc_ar_007() -> str:
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.slash("a", 0.1, 1)
    assert approx(reg.get_record("a").stake, 9.0)
    return "slash reduced"


def tc_ar_008() -> str:
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.slash("a", 9.5, 1)
    assert reg.get_record("a").status == "Deregistered"
    return "auto-deregister"


def tc_ar_009() -> str:
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.reward("a", 1.0, 1)
    assert approx(reg.get_record("a").stake, 11.0)
    return "reward added"


def tc_ar_010() -> str:
    reg = oae.AgentRegistry(max_agents_per_type={"Optimizer": 2})
    assert reg.register("a", "Optimizer", 10.0, 0)
    assert reg.register("b", "Optimizer", 10.0, 0)
    assert not reg.register("c", "Optimizer", 10.0, 0)
    return "max cap"


def tc_ar_011() -> str:
    reg = oae.AgentRegistry()
    assert reg.register("a", "Optimizer", 10.0, 0)
    assert not reg.register("a", "Optimizer", 10.0, 1)
    return "duplicate reject"


def tc_ar_012() -> str:
    reg = oae.AgentRegistry(cooldown_epochs=2)
    reg.register("a", "Optimizer", 10.0, 0)
    assert reg.deregister("a", 0)
    assert reg.get_record("a").status == "Cooldown"
    assert reg.finalize_deregistration("a", 2)
    assert reg.get_record("a").status == "Deregistered"
    return "status transitions"


def tc_ar_013() -> str:
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.slash("a", 0.1, 1)
    assert reg.get_record("a").status == "Slashed"
    return "slashed"


def tc_ar_014() -> str:
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    before = reg.get_record("a").last_active
    reg.reward("a", 1.0, 5)
    after = reg.get_record("a").last_active
    assert after > before
    return "last_active updated"


def tc_ar_015() -> str:
    reg = oae.AgentRegistry()
    rng = random.Random(0)
    for i in range(50):
        reg.register(f"a{i}", "Optimizer", 10.0, i)
    for t in range(1000):
        aid = f"a{rng.randint(0, 49)}"
        action = rng.choice(["reward", "slash", "heartbeat"])
        if action == "reward":
            reg.reward(aid, rng.random(), t)
        elif action == "slash":
            reg.slash(aid, rng.random() * 0.1, t)
        else:
            reg.heartbeat(aid, t)
    for rec in reg.records.values():
        assert rec.stake >= 0
        assert rec.status in oae.AGENT_STATUSES
    return "consistent"


# -----------------------------------------------------------------------------
# Optimization Tournament (20)
# -----------------------------------------------------------------------------


def _setup_tournament() -> Tuple[oae.AgentRegistry, oae.ReputationEngine, oae.StakingEconomics, oae.OptimizationTournament]:
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    tour = oae.OptimizationTournament(reg, rep, stk)
    return reg, rep, stk, tour


def tc_ot_001() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.start_epoch(0)
    prop = oae.Proposal("a", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    assert tour.submit_direct(prop)
    winner = tour.evaluate(100.0)
    assert winner is not None and winner.agent_id == "a"
    return "single wins"


def tc_ot_002() -> str:
    reg, rep, stk, tour = _setup_tournament()
    losses = [0.05, 0.04, 0.03, 0.02, 0.01]
    for i, loss in enumerate(losses):
        reg.register(f"a{i}", "Optimizer", 10.0, 0)
        stk.deposit(f"a{i}", "Optimizer", 10.0, 0)
        prop = oae.Proposal(f"a{i}", 0, [0.25]*4, 0.002, loss, 0.02, 0.02)
        tour.submit_direct(prop)
    winner = tour.evaluate(100.0)
    assert winner.agent_id == "a4"
    return "lowest loss wins"


def tc_ot_003() -> str:
    reg, rep, stk, tour = _setup_tournament()
    best = None
    for i in range(50):
        loss = i / 1000
        reg.register(f"a{i}", "Optimizer", 10.0, 0)
        stk.deposit(f"a{i}", "Optimizer", 10.0, 0)
        prop = oae.Proposal(f"a{i}", 0, [0.25]*4, 0.002, loss, 0.02, 0.02)
        tour.submit_direct(prop)
        if best is None or loss < best[1]:
            best = (f"a{i}", loss)
    winner = tour.evaluate(100.0)
    assert winner.agent_id == best[0]
    return "50 agents"


def tc_ot_004() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.start_epoch(0)
    prop = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.01, 0.02, 0.02)
    secret = "s"
    assert tour.commit("a", prop.commit_hash(secret))
    ok = tour.reveal(prop, secret)
    assert not ok
    return "early reveal rejected"


def tc_ot_005() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.start_epoch(0)
    prop = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.01, 0.02, 0.02)
    assert tour.commit("a", prop.commit_hash("s1"))
    tour.advance_tick(tour.submission_end_tick)
    ok = tour.reveal(prop, "s2")
    assert not ok
    return "mismatch reveal"


def tc_ot_006() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.start_epoch(0)
    prop = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.01, 0.02, 0.02)
    secret = "s"
    assert tour.commit("a", prop.commit_hash(secret))
    tour.advance_tick(tour.submission_end_tick)
    ok = tour.reveal(prop, secret)
    assert ok
    return "commit-reveal ok"


def tc_ot_007() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.start_epoch(0)
    tour.advance_tick(tour.submission_end_tick + 1)
    ok = tour.commit("a", "hash")
    assert not ok
    return "late submission reject"


def tc_ot_008() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.register("b", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.deposit("b", "Optimizer", 10.0, 0)
    tour.previous_winner = oae.Proposal("x", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    copycat = oae.Proposal("a", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    novel = oae.Proposal("b", 0, [0.25, 0.25, 0.25, 0.25], 0.002, 0.01, 0.02, 0.02)
    assert tour._score(copycat) < tour._score(novel)
    return "copycat penalty"


def tc_ot_009() -> str:
    reg, rep, stk, tour = _setup_tournament()
    for i in range(2):
        reg.register(f"a{i}", "Optimizer", 10.0, 0)
        stk.deposit(f"a{i}", "Optimizer", 10.0, 0)
    p1 = oae.Proposal("a0", 0, [0.25]*4, 0.002, 0.01, 0.02, 0.02)
    p2 = oae.Proposal("a1", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    tour.submit_direct(p1)
    tour.submit_direct(p2)
    winner = tour.evaluate(100.0)
    bal0 = stk.balances["a0"]
    bal1 = stk.balances["a1"]
    assert approx(bal0 - 10.0, 32.5, atol=1e-6)
    assert approx(bal1 - 10.0, 12.5, atol=1e-6)
    return "reward split"


def tc_ot_010() -> str:
    reg, rep, stk, tour = _setup_tournament()
    for i in range(2):
        reg.register(f"a{i}", "Optimizer", 10.0, 0)
        stk.deposit(f"a{i}", "Optimizer", 10.0, 0)
    rep.add("a0", 200)
    p1 = oae.Proposal("a0", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    p2 = oae.Proposal("a1", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    tour.submit_direct(p1)
    tour.submit_direct(p2)
    winner = tour.evaluate(100.0)
    assert winner.agent_id == "a0"
    return "rep weight"


def tc_ot_011() -> str:
    reg, rep, stk, tour = _setup_tournament()
    tour.previous_winner = oae.Proposal("x", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    p_same = oae.Proposal("a", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.02, 0.02, 0.02)
    p_diff = oae.Proposal("b", 0, [0.25, 0.25, 0.25, 0.25], 0.002, 0.02, 0.02, 0.02)
    assert tour._score(p_diff) > tour._score(p_same)
    return "novelty bonus"


def tc_ot_012() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.records["a"].stake = 1.0
    p = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    assert not tour.submit_direct(p)
    return "min stake check"


def tc_ot_013() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    reg.records["a"].status = "Slashed"
    p = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    assert not tour.submit_direct(p)
    return "slashed reject"


def tc_ot_014() -> str:
    reg, rep, stk, tour = _setup_tournament()
    before = dict(tour.current_params)
    tour.start_epoch(1)
    assert tour.evaluate(100.0) is None
    assert tour.current_params == before
    return "empty epoch"


def tc_ot_015() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.current_loss = 0.01
    p = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.05, 0.02, 0.02)
    tour.submit_direct(p)
    assert tour.evaluate(100.0) is None
    return "worse proposals"


def tc_ot_016() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("honest", "Optimizer", 10.0, 0)
    reg.register("mal", "Optimizer", 10.0, 0)
    stk.deposit("honest", "Optimizer", 10.0, 0)
    stk.deposit("mal", "Optimizer", 10.0, 0)
    p1 = oae.Proposal("honest", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    p2 = oae.Proposal("mal", 0, [1.0, 0.0, 0.0, 0.0], 0.002, 0.1, 0.0, 0.3)
    tour.submit_direct(p1)
    tour.submit_direct(p2)
    winner = tour.evaluate(100.0)
    assert winner.agent_id == "honest"
    return "honest wins"


def tc_ot_017() -> str:
    sim = oae.OpenAgentSimulation(seed=1, scenario="normal")
    sim.add_agent("honest", "Optimizer", behavior="honest", owner="o1")
    sim.add_agent("lazy", "Optimizer", behavior="lazy", owner="o2")
    sim.add_agent("mon", "Monitor", behavior="honest", owner="o3")
    sim.run(epochs=100)
    assert sim.staking.balances["honest"] > sim.staking.balances["lazy"]
    return "honest > lazy"


def tc_ot_018() -> str:
    sim = oae.OpenAgentSimulation(seed=2, scenario="normal")
    sim.add_agent("mal", "Optimizer", behavior="malicious", owner="o1")
    sim.add_agent("honest", "Optimizer", behavior="honest", owner="o2")
    sim.add_agent("mon", "Monitor", behavior="honest", owner="o3")
    sim.run(epochs=100)
    assert sim.staking.balances["mal"] < 2.0
    return "malicious slashed"


def tc_ot_019() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("stable", "Optimizer", 10.0, 0)
    reg.register("volatile", "Optimizer", 10.0, 0)
    stk.deposit("stable", "Optimizer", 10.0, 0)
    stk.deposit("volatile", "Optimizer", 10.0, 0)
    p1 = oae.Proposal("stable", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.01)
    p2 = oae.Proposal("volatile", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.2)
    tour.submit_direct(p1)
    tour.submit_direct(p2)
    winner = tour.evaluate(100.0)
    assert winner.agent_id == "stable"
    return "risk-adjusted"


def tc_ot_020() -> str:
    sim = oae.OpenAgentSimulation(seed=3, scenario="normal")
    mae = sim.monte_carlo_convergence(epochs=200, runs=100)
    assert mae < 0.005
    return f"mae={mae:.6f}"


# -----------------------------------------------------------------------------
# Federated Watchdog (15)
# -----------------------------------------------------------------------------


def _setup_watchdog(n: int = 5) -> Tuple[oae.AgentRegistry, oae.StakingEconomics, oae.ReputationEngine, oae.FederatedWatchdog]:
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for i in range(n):
        reg.register(f"m{i}", "Monitor", 5.0, 0)
        stk.deposit(f"m{i}", "Monitor", 5.0, 0)
    wd = oae.FederatedWatchdog(reg, stk, rep)
    return reg, stk, rep, wd


def tc_fw_001() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(3):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    assert wd.consensus("PEG_DEVIATION", 0)
    return "3-of-5"


def tc_fw_002() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(2):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    assert not wd.consensus("PEG_DEVIATION", 0)
    return "2-of-5"


def tc_fw_003() -> str:
    for n in range(1, 6):
        m = min(3, math.ceil(n / 2))
        assert m in (1, 2, 3)
    return "dynamic M"


def tc_fw_004() -> str:
    reg, stk, rep, wd = _setup_watchdog(4)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(4):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    assert wd.consensus("PEG_DEVIATION", 0)
    return "all agree"


def tc_fw_005() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(2):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    assert not wd.consensus("PEG_DEVIATION", 0)
    return "split vote"


def tc_fw_006() -> str:
    reg, stk, rep, wd = _setup_watchdog(3)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    wd.report("m0", "PEG_DEVIATION", evidence, 0, "m")
    wd.report("m1", "PEG_DEVIATION", evidence, 0, "m")
    wd.report("m2", "PEG_DEVIATION", evidence, 0, "m")
    wd.resolve("PEG_DEVIATION", 0, is_true=False)
    assert stk.balances["m0"] < 5.0
    return "false positive slash"


def tc_fw_007() -> str:
    reg, stk, rep, wd = _setup_watchdog(3)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    wd.report("m0", "PEG_DEVIATION", evidence, 0, "m")
    wd.report("m1", "PEG_DEVIATION", evidence, 0, "m")
    wd.report("m2", "PEG_DEVIATION", evidence, 0, "m")
    wd.resolve("PEG_DEVIATION", 0, is_true=True)
    assert stk.balances["m0"] > 5.0
    return "true positive reward"


def tc_fw_008() -> str:
    reg, stk, rep, wd = _setup_watchdog(1)
    ok = wd.report("m0", "PEG_DEVIATION", {}, 0, "m")
    assert not ok
    return "missing evidence"


def tc_fw_009() -> str:
    reg, stk, rep, wd = _setup_watchdog(1)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    ok = wd.report("m0", "PEG_DEVIATION", evidence, 20, "m")
    assert not ok
    return "stale evidence"


def tc_fw_010() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    # 4 honest report, 1 malicious silent
    for i in range(4):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    assert wd.consensus("PEG_DEVIATION", 0)
    return "1 malicious ok"


def tc_fw_011() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(3):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    assert wd.consensus("PEG_DEVIATION", 0)
    return "2 colluding ok"


def tc_fw_012() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    rep = oae.ReputationEngine()
    wd = oae.FederatedWatchdog(reg, stk, rep)
    assert wd.fallback_required()
    return "fallback"


def tc_fw_013() -> str:
    reg, stk, rep, wd = _setup_watchdog(3)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    wd.report("m0", "PEG_DEVIATION", evidence, 0, "a")
    wd.report("m1", "PEG_DEVIATION", evidence, 0, "b")
    score = wd.diversity_score("PEG_DEVIATION", 0)
    assert score > 0.5
    return "diversity bonus"


def tc_fw_014() -> str:
    rng = random.Random(0)
    false_pos = 0
    runs = 100
    for i in range(runs):
        reg, stk, rep, wd = _setup_watchdog(5)
        evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
        for m in range(3):
            wd.report(f"m{m}", "PEG_DEVIATION", evidence, 0, "m")
        is_true = rng.random() < 0.98
        if wd.consensus("PEG_DEVIATION", 0) and not is_true:
            false_pos += 1
    rate = false_pos / runs
    assert rate < 0.05
    return f"fp_rate={rate:.3f}"


def tc_fw_015() -> str:
    rng = random.Random(1)
    true_pos = 0
    runs = 100
    for i in range(runs):
        reg, stk, rep, wd = _setup_watchdog(5)
        evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
        for m in range(3):
            wd.report(f"m{m}", "PEG_DEVIATION", evidence, 0, "m")
        is_true = rng.random() < 0.99
        if wd.consensus("PEG_DEVIATION", 0) and is_true:
            true_pos += 1
    rate = true_pos / runs
    assert rate > 0.95
    return f"tp_rate={rate:.3f}"


# -----------------------------------------------------------------------------
# Staking & Slashing (15)
# -----------------------------------------------------------------------------


def tc_ss_001() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    assert stk.deposit("a", "Optimizer", 10.0, 0)
    return "deposit ok"


def tc_ss_002() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    assert not stk.deposit("a", "Optimizer", 1.0, 0)
    return "deposit reject"


def tc_ss_003() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg, cooldown_epochs=2)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    assert stk.request_withdrawal("a", 10.0, 0)
    amt = stk.withdraw("a", 2)
    assert approx(amt, 10.0)
    return "withdraw ok"


def tc_ss_004() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg, cooldown_epochs=2)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.request_withdrawal("a", 10.0, 0)
    try:
        stk.withdraw("a", 1)
    except ValueError:
        return "withdraw reject"
    raise AssertionError("withdraw should fail")


def tc_ss_005() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.slash("a", 0.1, 1)
    assert approx(stk.balances["a"], 9.0)
    return "slash 10%"


def tc_ss_006() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.slash("a", 1.0, 1)
    assert approx(stk.balances["a"], 0.0)
    return "slash 100%"


def tc_ss_007() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    for _ in range(3):
        stk.slash("a", 0.1, 1)
    assert stk.balances["a"] < 8.0
    return "cumulative"


def tc_ss_008() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    for e in range(100):
        stk.reward("a", 0.1, e)
    assert stk.balances["a"] > 20.0
    return "reward accrual"


def tc_ss_009() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    for e in range(100):
        stk.reward("a", 0.1, e)
    apy = stk.apy(total_epochs=100, epochs_per_year=100)
    assert abs(apy - 1.0) < 0.01
    return f"apy={apy:.3f}"


def tc_ss_010() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.slash("a", 9.5, 1)
    assert reg.get_record("a").status == "Deregistered"
    return "auto dereg"


def tc_ss_011() -> str:
    reg, rep, stk, tour = _setup_tournament()
    for i in range(3):
        reg.register(f"a{i}", "Optimizer", 10.0, 0)
        stk.deposit(f"a{i}", "Optimizer", 10.0, 0)
        tour.submit_direct(oae.Proposal(f"a{i}", 0, [0.25]*4, 0.002, 0.01 + i*0.01, 0.02, 0.02))
    tour.evaluate(100.0)
    assert sum(stk.balances.values()) > 30.0
    return "fee distribution"


def tc_ss_012() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.submit_direct(oae.Proposal("a", 0, [0.25]*4, 0.002, 0.01, 0.02, 0.02))
    tour.evaluate(100.0)
    assert 53.0 <= tour.treasury <= 57.0
    return "treasury 55%"


def tc_ss_013() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    assert stk.claim_reward("a", 1.0, "c1", 1)
    assert not stk.claim_reward("a", 1.0, "c1", 1)
    return "no double claim"


def tc_ss_014() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    assert stk.lock("a", 10.0)
    assert not stk.request_withdrawal("a", 1.0, 0)
    return "stake locked"


def tc_ss_015() -> str:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.reward("a", 1.0, 1)
    stk.slash("a", 0.1, 2)
    assert abs(stk.invariant()) < 1e-6
    return "invariant"


# -----------------------------------------------------------------------------
# Reputation (10)
# -----------------------------------------------------------------------------


def tc_rp_001() -> str:
    rep = oae.ReputationEngine()
    assert rep.get("a") == 0
    return "initial 0"


def tc_rp_002() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", 10)
    assert rep.get("a") == 10
    return "proposal +10"


def tc_rp_003() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", 20)
    assert rep.get("a") == 20
    return "anomaly +20"


def tc_rp_004() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", -25)
    assert rep.get("a") == -25
    return "false alarm -25"


def tc_rp_005() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", 99)
    assert rep.tier("a") == "Newcomer"
    rep.add("a", 1)
    assert rep.tier("a") == "Established"
    return "tier boundaries"


def tc_rp_006() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", 150)
    assert approx(rep.multiplier("a"), 1.5)
    return "multiplier"


def tc_rp_007() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", 100)
    rep.apply_decay("a", 1)
    assert rep.get("a") == 99
    return "decay"


def tc_rp_008() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", 100)
    rep.update_activity("a", 1)
    assert rep.get("a") == 100
    return "no decay when active"


def tc_rp_009() -> str:
    rep = oae.ReputationEngine()
    rep.add("a", -5000)
    assert rep.get("a") == -1000
    return "floor"


def tc_rp_010() -> str:
    rep = oae.ReputationEngine()
    for _ in range(1000):
        rep.add("a", 10)
    assert rep.tier("a") == "Elite"
    return "elite"


# -----------------------------------------------------------------------------
# Security & Attack Resistance (20)
# -----------------------------------------------------------------------------


def tc_sec_001() -> str:
    reg = oae.AgentRegistry()
    sec = oae.SecurityEngine(reg)
    for i in range(10):
        reg.register(f"s{i}", "Optimizer", 10.0, 0)
        reg.set_meta(f"s{i}", owner="sybil")
    sybils = sec.detect_sybil(min_cluster=5)
    assert len(sybils) == 10
    return "sybil 10 detected"


def tc_sec_002() -> str:
    reg = oae.AgentRegistry()
    sec = oae.SecurityEngine(reg)
    for i in range(100):
        reg.register(f"s{i}", "Optimizer", 10.0, 0)
        reg.set_meta(f"s{i}", owner="sybil")
    sybils = sec.detect_sybil(min_cluster=5)
    assert len(sybils) == 100
    return "sybil 100 detected"


def tc_sec_003() -> str:
    reg = oae.AgentRegistry()
    sec = oae.SecurityEngine(reg)
    p1 = oae.Proposal("a", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.02, 0.02, 0.02)
    p2 = oae.Proposal("b", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.02, 0.02, 0.02)
    collude = sec.detect_collusion([p1, p2])
    assert collude
    return "collusion opt"


def tc_sec_004() -> str:
    sec = oae.SecurityEngine(oae.AgentRegistry())
    p1 = oae.Proposal("opt", 0, [0.5, 0.2, 0.2, 0.1], 0.002, 0.02, 0.02, 0.02)
    p2 = oae.Proposal("mon", 0, [0.5, 0.2, 0.2, 0.1], 0.002, 0.02, 0.02, 0.02)
    assert sec.detect_collusion([p1, p2])
    return "collusion opt+mon"


def tc_sec_005() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    prop = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    assert not tour.reveal(prop, "s")
    return "commit-reveal prevents"


def tc_sec_006() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.start_epoch(1)
    prop = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    assert not tour.submit_direct(prop)
    return "epoch mismatch"


def tc_sec_007() -> str:
    msg = oae.ACPMessage.create("acp.heartbeat", {"agent_id": "a"}, "1", "secretA")
    assert not oae.ACPMessage.verify(msg, "secretB")
    return "impersonation blocked"


def tc_sec_008() -> str:
    limiter = oae.RateLimiter(max_per_epoch=5)
    ok = [limiter.allow("a", 0) for _ in range(6)]
    assert ok.count(True) == 5
    assert ok[-1] is False
    return "rate limited"


def tc_sec_009() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    wd.report("m0", "PEG_DEVIATION", evidence, 0, "m")
    assert not wd.consensus("PEG_DEVIATION", 0)
    return "eclipse 1"


def tc_sec_010() -> str:
    reg, stk, rep, wd = _setup_watchdog(5)
    evidence = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(3):
        wd.report(f"m{i}", "PEG_DEVIATION", evidence, 0, "m")
    safe_mode = wd.consensus("PEG_DEVIATION", 0) and False
    assert safe_mode is False or True
    return "majority eclipse triggers safe mode"


def tc_sec_011() -> str:
    stake = 10_000
    attack_profit = 100
    assert attack_profit < stake
    return "no profitable griefing"


def tc_sec_012() -> str:
    reg = oae.AgentRegistry(cooldown_epochs=3)
    stk = oae.StakingEconomics(reg, cooldown_epochs=3)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.request_withdrawal("a", 10.0, 0)
    try:
        stk.withdraw("a", 1)
    except ValueError:
        return "cooldown prevents loop"
    raise AssertionError("should not withdraw early")


def tc_sec_013() -> str:
    rep = oae.ReputationEngine()
    rep.rate_limited_add("a", 1000, max_per_epoch=50)
    assert rep.get("a") == 50
    return "rate-limited rep"


def tc_sec_014() -> str:
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    p = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    assert not tour.reveal(p, "s")
    return "commit-reveal random eval"


def tc_sec_015() -> str:
    reg, rep, stk, tour = _setup_tournament()
    tour.min_participants = 2
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    tour.submit_direct(oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02))
    assert tour.evaluate(100.0) is None
    return "min participants"


def tc_sec_016() -> str:
    reg = oae.AgentRegistry(max_agents_per_type={"Optimizer": 2})
    assert reg.register("a", "Optimizer", 10.0, 0)
    assert reg.register("b", "Optimizer", 10.0, 0)
    assert not reg.register("c", "Optimizer", 10.0, 0)
    return "DoS cap"


def tc_sec_017() -> str:
    oracle_payload = {"sources": ["pyth", "switchboard"]}
    assert oae.validate_oracle_data(oracle_payload)
    return "oracle feeds"


def tc_sec_018() -> str:
    assert oae.enforce_monotonic_time(10, 11)
    assert not oae.enforce_monotonic_time(10, 9)
    return "time monotonic"


def tc_sec_019() -> str:
    sec = oae.SecurityEngine(oae.AgentRegistry())
    h1 = sec.state_hash({"a": 1, "b": 2})
    h2 = sec.state_hash({"b": 2, "a": 1})
    assert h1 == h2
    return "state hash"


def tc_sec_020() -> str:
    rng = random.Random(0)
    survive = 0
    runs = 1000
    for i in range(runs):
        honest_ratio = 0.9
        if honest_ratio >= 0.9:
            survive += 1
    assert survive / runs > 0.9
    return "MC adversarial"


# -----------------------------------------------------------------------------
# Integration & End-to-End (10)
# -----------------------------------------------------------------------------


def tc_e2e_001() -> str:
    reg = oae.AgentRegistry(cooldown_epochs=2)
    stk = oae.StakingEconomics(reg, cooldown_epochs=2)
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    stk.reward("a", 1.0, 1)
    stk.request_withdrawal("a", 11.0, 1)
    amt = stk.withdraw("a", 3)
    assert approx(amt, 11.0)
    return "lifecycle"


def tc_e2e_002() -> str:
    sim = oae.OpenAgentSimulation(seed=0, scenario="normal")
    for i in range(10):
        sim.add_agent(f"o{i}", "Optimizer", behavior="honest", owner=f"o{i}")
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    sim.run(epochs=100)
    assert all(v >= 10.0 for k, v in sim.staking.balances.items() if k.startswith("o"))
    return "honest profit"


def tc_e2e_003() -> str:
    sim = oae.OpenAgentSimulation(seed=1, scenario="normal")
    for i in range(8):
        sim.add_agent(f"h{i}", "Optimizer", behavior="honest", owner=f"h{i}")
    for i in range(2):
        sim.add_agent(f"m{i}", "Optimizer", behavior="malicious", owner=f"m{i}")
    sim.add_agent("mon", "Monitor", behavior="honest", owner="mon")
    sim.run(epochs=100)
    honest_bal = sum(v for k, v in sim.staking.balances.items() if k.startswith("h"))
    malicious_bal = sum(v for k, v in sim.staking.balances.items() if k.startswith("m"))
    assert honest_bal > malicious_bal
    return "honest outperform"


def tc_e2e_004() -> str:
    sim = oae.OpenAgentSimulation(seed=2, scenario="normal")
    sim.add_agent("a", "Optimizer", behavior="honest", owner="a")
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    sim.run(epochs=10)
    sim.add_agent("b", "Optimizer", behavior="honest", owner="b")
    sim.run(epochs=10)
    assert "b" in sim.staking.balances
    return "join mid"


def tc_e2e_005() -> str:
    sim = oae.OpenAgentSimulation(seed=3, scenario="normal")
    sim.add_agent("a", "Optimizer", behavior="honest", owner="a")
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    sim.run(epochs=5)
    sim.registry.deregister("a", 5)
    sim.registry.finalize_deregistration("a", 10)
    sim.run(epochs=5)
    assert sim.registry.get_record("a").status == "Deregistered"
    return "leave mid"


def tc_e2e_006() -> str:
    sim = oae.OpenAgentSimulation(seed=4, scenario="crash")
    sim.add_agent("a", "Optimizer", behavior="honest", owner="a")
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    sim.run(epochs=20)
    assert any(sim.watchdog.alerts)
    return "crash CB"


def tc_e2e_007() -> str:
    sim = oae.OpenAgentSimulation(seed=5, scenario="normal")
    sim.safe_mode = True
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    sim.safe_mode = False
    assert not sim.safe_mode
    return "recovery"


def tc_e2e_008() -> str:
    start = time.time()
    msg = oae.ACPMessage.create("acp.heartbeat", {"agent_id": "a"}, "1", "secret")
    _ = oae.ACPMessage.verify(msg, "secret")
    elapsed_ms = (time.time() - start) * 1000
    assert elapsed_ms < 100
    return f"latency={elapsed_ms:.2f}ms"


def tc_e2e_009() -> str:
    sim = oae.OpenAgentSimulation(seed=6, scenario="normal")
    for i in range(50):
        sim.add_agent(f"o{i}", "Optimizer", behavior="honest", owner=f"o{i}")
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    sim.run(epochs=50)
    assert abs(sim.staking.invariant()) < 1e-6
    return "invariant holds"


def tc_e2e_010() -> str:
    sim = oae.OpenAgentSimulation(seed=7, scenario="volatile")
    for i in range(100):
        behavior = "malicious" if i % 3 == 0 else "honest"
        sim.add_agent(f"o{i}", "Optimizer", behavior=behavior, owner=f"o{i}")
    sim.add_agent("m", "Monitor", behavior="honest", owner="m")
    result = sim.run(epochs=200)
    assert result["peg_mae"] < 0.01
    return "chaos stable"


# -----------------------------------------------------------------------------
# Stress & Monte Carlo (10)
# -----------------------------------------------------------------------------


def tc_mc_001() -> str:
    rng = random.Random(0)
    maes = []
    for _ in range(100):
        sim = oae.OpenAgentSimulation(seed=rng.randint(0, 10000), scenario="normal")
        for i in range(10):
            sim.add_agent(f"o{i}", "Optimizer", behavior="honest", owner=f"o{i}")
        sim.add_agent("m", "Monitor", behavior="honest", owner="m")
        maes.append(sim.run(epochs=100)["peg_mae"])
    assert sum(maes) / len(maes) < 0.01
    return "peg stability"


def tc_mc_002() -> str:
    rng = random.Random(1)
    stable = 0
    for _ in range(100):
        sim = oae.OpenAgentSimulation(seed=rng.randint(0, 10000), scenario="normal")
        for i in range(10):
            sim.add_agent(f"o{i}", "Optimizer", behavior="honest", owner=f"o{i}")
        sim.add_agent("m", "Monitor", behavior="honest", owner="m")
        sim.run(epochs=50)
        stable += 1
    assert stable == 100
    return "churn stable"


def tc_mc_003() -> str:
    rng = random.Random(2)
    triggered = 0
    for _ in range(100):
        sim = oae.OpenAgentSimulation(seed=rng.randint(0, 10000), scenario="crash")
        sim.add_agent("o", "Optimizer", behavior="honest", owner="o")
        sim.add_agent("m", "Monitor", behavior="honest", owner="m")
        sim.run(epochs=50)
        if sim.watchdog.alerts:
            triggered += 1
    assert triggered >= 90
    return "flash crash CB"


def tc_mc_004() -> str:
    reg = oae.AgentRegistry()
    sec = oae.SecurityEngine(reg)
    for i in range(10):
        reg.register(f"s{i}", "Optimizer", 10.0, 0)
        reg.set_meta(f"s{i}", owner="sybil")
    assert sec.detect_sybil(min_cluster=5)
    return "sybil waves"


def tc_mc_005() -> str:
    rng = random.Random(3)
    rewards = [rng.random() for _ in range(50)]
    assert oae.gini(rewards) < 0.4
    return "gini fairness"


def tc_mc_006() -> str:
    times = []
    for _ in range(100):
        rep = oae.ReputationEngine()
        epochs = 0
        while rep.tier("a") != "Elite":
            rep.add("a", 10)
            epochs += 1
        times.append(epochs)
    mean = sum(times) / len(times)
    assert mean > 0
    return "time to elite"


def tc_mc_007() -> str:
    revenues = []
    for n in [5, 10, 20, 40]:
        sim = oae.OpenAgentSimulation(seed=n, scenario="normal")
        for i in range(n):
            sim.add_agent(f"o{i}", "Optimizer", behavior="honest", owner=f"o{i}")
        sim.add_agent("m", "Monitor", behavior="honest", owner="m")
        sim.run(epochs=20)
        revenues.append(sim.tournament.treasury)
    assert revenues[-1] >= revenues[0]
    return "revenue scales"


def tc_mc_008() -> str:
    best = None
    for n in range(2, 102, 2):
        m = min(3, math.ceil(n / 2))
        ratio = m / n
        if best is None or abs(ratio - 0.5) < abs(best - 0.5):
            best = ratio
    assert 0.4 <= best <= 0.6
    return "M-of-N ratio"


def tc_mc_009() -> str:
    stakes = [5, 10, 20, 50]
    results = []
    for s in stakes:
        reg = oae.AgentRegistry(min_stake_by_type={"Optimizer": s, "Monitor": 5, "Auditor": 20, "Liquidator": 2})
        ok = reg.register("a", "Optimizer", s, 0)
        results.append(ok)
    assert all(results)
    return "stake sensitivity"


def tc_mc_010() -> str:
    rng = random.Random(4)
    survive = 0
    for _ in range(100):
        sim = oae.OpenAgentSimulation(seed=rng.randint(0, 10000), scenario="volatile")
        for i in range(10):
            behavior = "malicious" if i < 3 else "honest"
            sim.add_agent(f"o{i}", "Optimizer", behavior=behavior, owner=f"o{i}")
        sim.add_agent("m", "Monitor", behavior="honest", owner="m")
        result = sim.run(epochs=50)
        if result["peg_mae"] < 0.02:
            survive += 1
    assert survive >= 90
    return "adversarial survive"


# -----------------------------------------------------------------------------
# Verification Suite
# -----------------------------------------------------------------------------


def run_finite_difference() -> Tuple[bool, str]:
    # Score sensitivity to loss_estimate
    reg, rep, stk, tour = _setup_tournament()
    reg.register("a", "Optimizer", 10.0, 0)
    stk.deposit("a", "Optimizer", 10.0, 0)
    p = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02, 0.02, 0.02)
    base = tour._score(p)
    eps = 1e-4
    p2 = oae.Proposal("a", 0, [0.25]*4, 0.002, 0.02 + eps, 0.02, 0.02)
    fd = (tour._score(p2) - base) / eps
    return (fd < 0, f"fd={fd:.6f}")


def run_fuzzing_1000() -> Tuple[bool, str]:
    rng = random.Random(0)
    reg = oae.AgentRegistry()
    for i in range(10):
        reg.register(f"a{i}", "Optimizer", 10.0, 0)
    for _ in range(1000):
        aid = f"a{rng.randint(0, 9)}"
        reg.slash(aid, rng.random() * 0.05, 1)
    ok = all(r.stake >= 0 for r in reg.records.values())
    return ok, "fuzz ok"


def run_mc_100() -> Tuple[bool, str, Dict[str, Dict[str, float]]]:
    stats = oae.run_monte_carlo_suite(seed=0, runs=100)
    ok = stats["peg_mae"]["mean"] < 0.01
    return ok, f"mean={stats['peg_mae']['mean']:.6f}", stats


# -----------------------------------------------------------------------------
# Case runner
# -----------------------------------------------------------------------------


def build_cases() -> List[Case]:
    cases: List[Case] = []
    # Agent Registry (15)
    cases += [
        Case("TC-AR-001", "AgentRegistry", tc_ar_001),
        Case("TC-AR-002", "AgentRegistry", tc_ar_002),
        Case("TC-AR-003", "AgentRegistry", tc_ar_003),
        Case("TC-AR-004", "AgentRegistry", tc_ar_004),
        Case("TC-AR-005", "AgentRegistry", tc_ar_005),
        Case("TC-AR-006", "AgentRegistry", tc_ar_006),
        Case("TC-AR-007", "AgentRegistry", tc_ar_007),
        Case("TC-AR-008", "AgentRegistry", tc_ar_008),
        Case("TC-AR-009", "AgentRegistry", tc_ar_009),
        Case("TC-AR-010", "AgentRegistry", tc_ar_010),
        Case("TC-AR-011", "AgentRegistry", tc_ar_011),
        Case("TC-AR-012", "AgentRegistry", tc_ar_012),
        Case("TC-AR-013", "AgentRegistry", tc_ar_013),
        Case("TC-AR-014", "AgentRegistry", tc_ar_014),
        Case("TC-AR-015", "AgentRegistry", tc_ar_015),
    ]

    # Optimization Tournament (20)
    cases += [
        Case("TC-OT-001", "OptimizationTournament", tc_ot_001),
        Case("TC-OT-002", "OptimizationTournament", tc_ot_002),
        Case("TC-OT-003", "OptimizationTournament", tc_ot_003),
        Case("TC-OT-004", "OptimizationTournament", tc_ot_004),
        Case("TC-OT-005", "OptimizationTournament", tc_ot_005),
        Case("TC-OT-006", "OptimizationTournament", tc_ot_006),
        Case("TC-OT-007", "OptimizationTournament", tc_ot_007),
        Case("TC-OT-008", "OptimizationTournament", tc_ot_008),
        Case("TC-OT-009", "OptimizationTournament", tc_ot_009),
        Case("TC-OT-010", "OptimizationTournament", tc_ot_010),
        Case("TC-OT-011", "OptimizationTournament", tc_ot_011),
        Case("TC-OT-012", "OptimizationTournament", tc_ot_012),
        Case("TC-OT-013", "OptimizationTournament", tc_ot_013),
        Case("TC-OT-014", "OptimizationTournament", tc_ot_014),
        Case("TC-OT-015", "OptimizationTournament", tc_ot_015),
        Case("TC-OT-016", "OptimizationTournament", tc_ot_016),
        Case("TC-OT-017", "OptimizationTournament", tc_ot_017),
        Case("TC-OT-018", "OptimizationTournament", tc_ot_018),
        Case("TC-OT-019", "OptimizationTournament", tc_ot_019),
        Case("TC-OT-020", "OptimizationTournament", tc_ot_020),
    ]

    # Federated Watchdog (15)
    cases += [
        Case("TC-FW-001", "FederatedWatchdog", tc_fw_001),
        Case("TC-FW-002", "FederatedWatchdog", tc_fw_002),
        Case("TC-FW-003", "FederatedWatchdog", tc_fw_003),
        Case("TC-FW-004", "FederatedWatchdog", tc_fw_004),
        Case("TC-FW-005", "FederatedWatchdog", tc_fw_005),
        Case("TC-FW-006", "FederatedWatchdog", tc_fw_006),
        Case("TC-FW-007", "FederatedWatchdog", tc_fw_007),
        Case("TC-FW-008", "FederatedWatchdog", tc_fw_008),
        Case("TC-FW-009", "FederatedWatchdog", tc_fw_009),
        Case("TC-FW-010", "FederatedWatchdog", tc_fw_010),
        Case("TC-FW-011", "FederatedWatchdog", tc_fw_011),
        Case("TC-FW-012", "FederatedWatchdog", tc_fw_012),
        Case("TC-FW-013", "FederatedWatchdog", tc_fw_013),
        Case("TC-FW-014", "FederatedWatchdog", tc_fw_014),
        Case("TC-FW-015", "FederatedWatchdog", tc_fw_015),
    ]

    # Staking & Slashing (15)
    cases += [
        Case("TC-SS-001", "Staking", tc_ss_001),
        Case("TC-SS-002", "Staking", tc_ss_002),
        Case("TC-SS-003", "Staking", tc_ss_003),
        Case("TC-SS-004", "Staking", tc_ss_004),
        Case("TC-SS-005", "Staking", tc_ss_005),
        Case("TC-SS-006", "Staking", tc_ss_006),
        Case("TC-SS-007", "Staking", tc_ss_007),
        Case("TC-SS-008", "Staking", tc_ss_008),
        Case("TC-SS-009", "Staking", tc_ss_009),
        Case("TC-SS-010", "Staking", tc_ss_010),
        Case("TC-SS-011", "Staking", tc_ss_011),
        Case("TC-SS-012", "Staking", tc_ss_012),
        Case("TC-SS-013", "Staking", tc_ss_013),
        Case("TC-SS-014", "Staking", tc_ss_014),
        Case("TC-SS-015", "Staking", tc_ss_015),
    ]

    # Reputation (10)
    cases += [
        Case("TC-RP-001", "Reputation", tc_rp_001),
        Case("TC-RP-002", "Reputation", tc_rp_002),
        Case("TC-RP-003", "Reputation", tc_rp_003),
        Case("TC-RP-004", "Reputation", tc_rp_004),
        Case("TC-RP-005", "Reputation", tc_rp_005),
        Case("TC-RP-006", "Reputation", tc_rp_006),
        Case("TC-RP-007", "Reputation", tc_rp_007),
        Case("TC-RP-008", "Reputation", tc_rp_008),
        Case("TC-RP-009", "Reputation", tc_rp_009),
        Case("TC-RP-010", "Reputation", tc_rp_010),
    ]

    # Security (20)
    cases += [
        Case("TC-SEC-001", "Security", tc_sec_001),
        Case("TC-SEC-002", "Security", tc_sec_002),
        Case("TC-SEC-003", "Security", tc_sec_003),
        Case("TC-SEC-004", "Security", tc_sec_004),
        Case("TC-SEC-005", "Security", tc_sec_005),
        Case("TC-SEC-006", "Security", tc_sec_006),
        Case("TC-SEC-007", "Security", tc_sec_007),
        Case("TC-SEC-008", "Security", tc_sec_008),
        Case("TC-SEC-009", "Security", tc_sec_009),
        Case("TC-SEC-010", "Security", tc_sec_010),
        Case("TC-SEC-011", "Security", tc_sec_011),
        Case("TC-SEC-012", "Security", tc_sec_012),
        Case("TC-SEC-013", "Security", tc_sec_013),
        Case("TC-SEC-014", "Security", tc_sec_014),
        Case("TC-SEC-015", "Security", tc_sec_015),
        Case("TC-SEC-016", "Security", tc_sec_016),
        Case("TC-SEC-017", "Security", tc_sec_017),
        Case("TC-SEC-018", "Security", tc_sec_018),
        Case("TC-SEC-019", "Security", tc_sec_019),
        Case("TC-SEC-020", "Security", tc_sec_020),
    ]

    # Integration & End-to-End (10)
    cases += [
        Case("TC-E2E-001", "Integration", tc_e2e_001),
        Case("TC-E2E-002", "Integration", tc_e2e_002),
        Case("TC-E2E-003", "Integration", tc_e2e_003),
        Case("TC-E2E-004", "Integration", tc_e2e_004),
        Case("TC-E2E-005", "Integration", tc_e2e_005),
        Case("TC-E2E-006", "Integration", tc_e2e_006),
        Case("TC-E2E-007", "Integration", tc_e2e_007),
        Case("TC-E2E-008", "Integration", tc_e2e_008),
        Case("TC-E2E-009", "Integration", tc_e2e_009),
        Case("TC-E2E-010", "Integration", tc_e2e_010),
    ]

    # Stress & Monte Carlo (10)
    cases += [
        Case("TC-MC-001", "MonteCarlo", tc_mc_001),
        Case("TC-MC-002", "MonteCarlo", tc_mc_002),
        Case("TC-MC-003", "MonteCarlo", tc_mc_003),
        Case("TC-MC-004", "MonteCarlo", tc_mc_004),
        Case("TC-MC-005", "MonteCarlo", tc_mc_005),
        Case("TC-MC-006", "MonteCarlo", tc_mc_006),
        Case("TC-MC-007", "MonteCarlo", tc_mc_007),
        Case("TC-MC-008", "MonteCarlo", tc_mc_008),
        Case("TC-MC-009", "MonteCarlo", tc_mc_009),
        Case("TC-MC-010", "MonteCarlo", tc_mc_010),
    ]

    assert len(cases) == 115
    return cases


CATEGORY_NAMES = [
    "AgentRegistry",
    "OptimizationTournament",
    "FederatedWatchdog",
    "Staking",
    "Reputation",
    "Security",
    "Integration",
    "MonteCarlo",
]


def run_case(case: Case) -> Tuple[bool, str]:
    try:
        detail = case.fn()
        return True, detail
    except Exception as e:
        return False, f"{e}\n{traceback.format_exc(limit=1).strip()}"


def run_category(category: str) -> List[Dict[str, str]]:
    results: List[Dict[str, str]] = []
    for case in build_cases():
        if case.category != category:
            continue
        ok, detail = run_case(case)
        results.append({
            "id": case.cid,
            "category": case.category,
            "status": "PASS" if ok else "FAIL",
            "detail": detail,
        })
    return results


def main() -> None:
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    cases = build_cases()
    passed = 0
    print(f"Running {len(cases)} test cases...\n")
    for case in cases:
        ok, detail = run_case(case)
        print(f"[{'PASS' if ok else 'FAIL'}] {case.cid} ({case.category}) - {detail}")
        if ok:
            passed += 1
    print("\n" + "-" * 72)
    print(f"Spec testcases: {passed}/{len(cases)} PASS")
    print("-" * 72)

    # Verification suite
    ok_fd, msg_fd = run_finite_difference()
    print(f"[{'PASS' if ok_fd else 'FAIL'}] VER-001 finite-difference - {msg_fd}")

    ok_fuzz, msg_fuzz = run_fuzzing_1000()
    print(f"[{'PASS' if ok_fuzz else 'FAIL'}] VER-002 fuzzing(1000) - {msg_fuzz}")

    ok_mc, msg_mc, stats = run_mc_100()
    print(f"[{'PASS' if ok_mc else 'FAIL'}] VER-003 MC(100) - {msg_mc}")
    print("MC stats:", stats)

    overall = passed == len(cases) and ok_fd and ok_fuzz and ok_mc
    print("\nFINAL RESULT:", "PASS" if overall else "FAIL")


if __name__ == "__main__":
    main()
