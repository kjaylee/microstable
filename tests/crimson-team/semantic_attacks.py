#!/usr/bin/env python3
"""Crimson Team semantic/logic attack scenarios."""

from __future__ import annotations

import json
from dataclasses import dataclass, asdict
from typing import Any, Dict, List

import adversarial_agents as aa
import open_agent_economy as oae


@dataclass
class Attempt:
    attempt_id: str
    name: str
    category: str
    success: bool
    severity: str
    evidence: Dict[str, Any]
    defense_or_failure_reason: str


def s01_unsigned_micro_claim_budget_exhaustion() -> Attempt:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("att", "Optimizer", 10.0, 0)

    ok_count = 0
    for i in range(1_200):
        if stk.claim_reward("att", 1.0, f"micro-{i}", 1, proof=None):
            ok_count += 1

    # Expected exploit behavior: full epoch cap consumed without signed proofs.
    success = ok_count == 1_000 and abs(stk.balances.get("att", 0.0) - 1_000.0) < 1e-6
    return Attempt(
        attempt_id="CT-S01",
        name="Unsigned micro-claims consume full epoch budget",
        category="B/F",
        success=success,
        severity="MEDIUM" if success else "NONE",
        evidence={"ok_count": ok_count, "att_balance": stk.balances.get("att", 0.0)},
        defense_or_failure_reason="Legacy unsigned-claim lane remains usable up to full cap when split into micro-claims.",
    )


def s02_claim_id_scope_griefing() -> Attempt:
    reg = oae.AgentRegistry()
    stk = oae.StakingEconomics(reg)
    reg.register("att", "Optimizer", 10.0, 0)
    reg.register("vic", "Optimizer", 10.0, 0)

    att_ok = stk.claim_reward("att", 1.0, "shared-id", 2, proof=None)
    proof = stk.build_claim_proof("vic", 100.0, "shared-id", 2)
    vic_ok = stk.claim_reward("vic", 100.0, "shared-id", 2, proof=proof)

    success = att_ok and (not vic_ok)
    return Attempt(
        attempt_id="CT-S02",
        name="Global claim_id namespace enables griefing",
        category="B",
        success=success,
        severity="HIGH" if success else "NONE",
        evidence={"attacker_ok": att_ok, "victim_ok": vic_ok},
        defense_or_failure_reason="claim_id uniqueness is global and unauthenticated at namespace level.",
    )


def s03_tournament_loss_estimate_semantic_injection() -> Attempt:
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for aid in ("honest", "evil"):
        reg.register(aid, "Optimizer", 10.0, 0)

    t = oae.OptimizationTournament(reg, rep, stk)
    t.start_epoch(0)
    t.submit_direct(oae.Proposal("honest", 0, [0.25, 0.25, 0.25, 0.25], 0.002, 0.01, 0.02, 0.2))
    t.submit_direct(oae.Proposal("evil", 0, [0.25, 0.25, 0.25, 0.25], 0.002, float("-inf"), 0.0, 0.2))

    winner = t.evaluate(100.0)
    success = winner is not None and winner.agent_id == "evil"
    return Attempt(
        attempt_id="CT-S03",
        name="-inf loss_estimate semantic injection wins tournament",
        category="B",
        success=success,
        severity="HIGH" if success else "NONE",
        evidence={
            "winner": winner.agent_id if winner else None,
            "current_loss": t.current_loss,
            "evil_reward": stk.balances.get("evil", 0.0),
        },
        defense_or_failure_reason="Winner parameter validation excludes score-driving fields (loss_estimate).",
    )


def s04_sybil_fingerprint_boundary_evasion() -> Attempt:
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)

    reg.register("hon", "Optimizer", 10.0, 0)
    for i in range(20):
        reg.register(f"s{i}", "Optimizer", 10.0, 0)

    t = oae.OptimizationTournament(reg, rep, stk)
    t.start_epoch(0)
    t.submit_direct(oae.Proposal("hon", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.2))

    for i in range(20):
        d = (i % 5) * 0.004
        w = [0.395 + d, 0.305 - d, 0.2, 0.1]
        s = sum(w)
        t.submit_direct(oae.Proposal(f"s{i}", 0, [x / s for x in w], 0.002, 0.011, 0.02, 0.2))

    t.evaluate(100.0)
    sybil_total = sum(v for k, v in stk.balances.items() if k.startswith("s"))
    success = sybil_total > 4.0
    return Attempt(
        attempt_id="CT-S04",
        name="Fingerprint boundary evasion in participant pool",
        category="A/B/F",
        success=success,
        severity="MEDIUM" if success else "NONE",
        evidence={"sybil_total": sybil_total, "hon_balance": stk.balances.get("hon", 0.0)},
        defense_or_failure_reason="2-decimal bucket clustering can be gamed with boundary jitter.",
    )


def s05_response_engine_order_based_idempotency_bypass() -> Attempt:
    re = aa.ResponseEngine()

    r1 = re.auto_respond({"type": "collusion", "epoch": 9, "agents": ["a", "b"]})
    r2 = re.auto_respond({"type": "collusion", "epoch": 9, "agents": ["b", "a"]})

    success = r1.get("action") == "quarantine" and r2.get("action") == "quarantine"
    return Attempt(
        attempt_id="CT-S05",
        name="Collusion alert ordering bypasses semantic idempotency",
        category="B/F",
        success=success,
        severity="MEDIUM" if success else "NONE",
        evidence={"r1": r1, "r2": r2, "handled_alerts": sorted(re.handled_alerts)},
        defense_or_failure_reason="Idempotency key uses first agent order for collusion alerts.",
    )


def s06_watchdog_false_resolution_without_consensus_guard() -> Attempt:
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for aid in ("m1", "m2", "m3"):
        reg.register(aid, "Monitor", 5.0, 0)
        stk.deposit(aid, "Monitor", 100.0, 0)

    wd = oae.FederatedWatchdog(reg, stk, rep)
    ev = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for aid in ("m1", "m2", "m3"):
        wd.report(aid, "PEG", ev, 0, "method")

    before = {aid: stk.balances[aid] for aid in ("m1", "m2", "m3")}
    wd.resolve("PEG", 0, False)
    after = {aid: stk.balances[aid] for aid in ("m1", "m2", "m3")}

    success = all(after[a] < before[a] for a in before)
    return Attempt(
        attempt_id="CT-S06",
        name="False resolution can slash reporters without truth quorum",
        category="B/E",
        success=success,
        severity="HIGH" if success else "NONE",
        evidence={"before": before, "after": after},
        defense_or_failure_reason="resolve(false) path has no consensus/auth guard.",
    )


def s07_public_key_hijack_fix_holds() -> Attempt:
    reg = oae.AgentRegistry()
    reg.register("victim", "Optimizer", 10.0, 0)
    ok = reg.set_public_key("victim", "attacker-key", actor_id="attacker")

    return Attempt(
        attempt_id="CT-S07",
        name="Defense check: set_public_key actor mismatch",
        category="B",
        success=ok,
        severity="NONE" if not ok else "HIGH",
        evidence={"set_public_key_ok": ok},
        defense_or_failure_reason="Defense held when success=False (actor must match agent_id).",
    )


def s08_acp_nonce_replay_fix_holds() -> Attempt:
    msg = oae.ACPMessage.create("acp.ping", {"agent_id": "a"}, "id-1", "secret", epoch=5, expiry_epoch=8, nonce="nonce-1")
    oae.ACPMessage._seen_nonces.clear()
    v1 = oae.ACPMessage.verify(msg, "secret", now_epoch=5, expected_epoch=5)
    v2 = oae.ACPMessage.verify(msg, "secret", now_epoch=5, expected_epoch=5)

    replay_succeeded = v1 and v2
    return Attempt(
        attempt_id="CT-S08",
        name="Defense check: ACP nonce replay",
        category="E",
        success=replay_succeeded,
        severity="NONE" if not replay_succeeded else "HIGH",
        evidence={"first_verify": v1, "second_verify": v2},
        defense_or_failure_reason="Defense held when success=False (nonce replay blocked).",
    )


def s09_watchdog_inactive_monitor_report_fix_holds() -> Attempt:
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    reg.register("m", "Monitor", 5.0, 0)
    reg.deregister("m", 0)

    wd = oae.FederatedWatchdog(reg, stk, rep)
    ok = wd.report("m", "PEG", {"snapshot": {}, "oracle": {}, "timestamp": 0}, 0, "x")

    return Attempt(
        attempt_id="CT-S09",
        name="Defense check: inactive monitor cannot report",
        category="B",
        success=ok,
        severity="NONE" if not ok else "MEDIUM",
        evidence={"report_ok": ok},
        defense_or_failure_reason="Defense held when success=False (report requires active Monitor).",
    )


def run_attempts() -> List[Dict[str, Any]]:
    attempts = [
        s01_unsigned_micro_claim_budget_exhaustion(),
        s02_claim_id_scope_griefing(),
        s03_tournament_loss_estimate_semantic_injection(),
        s04_sybil_fingerprint_boundary_evasion(),
        s05_response_engine_order_based_idempotency_bypass(),
        s06_watchdog_false_resolution_without_consensus_guard(),
        s07_public_key_hijack_fix_holds(),
        s08_acp_nonce_replay_fix_holds(),
        s09_watchdog_inactive_monitor_report_fix_holds(),
    ]
    return [asdict(a) for a in attempts]


if __name__ == "__main__":
    print(json.dumps(run_attempts(), indent=2))
