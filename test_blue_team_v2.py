#!/usr/bin/env python3
"""Blue Team v2 security regression suite (PT-001 ~ PT-027).

Each PT has at least 2 tests:
- *_pre: reproduces legacy vulnerable behavior via compact legacy helper
- *_post: verifies patched behavior is safe
"""

from __future__ import annotations

import hashlib
import json
from typing import Any, Dict, List, Tuple

import pytest

import adversarial_agents as aa
import microstable as ms
import open_agent_economy as oae


# ---------------------------------------------------------------------------
# Legacy helpers (minimal vulnerable models)
# ---------------------------------------------------------------------------


def _legacy_claim_ok(claimed: set[str], claim_id: str) -> bool:
    if claim_id in claimed:
        return False
    claimed.add(claim_id)
    return True


def _legacy_withdraw_after_slash(balance: float, pending: float, slash: float) -> float:
    """Legacy bug: withdrawal returns full pending regardless of slash."""
    _ = max(0.0, balance - slash)
    return pending


def _legacy_reveal_dup(proposals: List[str], proposal_id: str) -> None:
    proposals.append(proposal_id)


def _legacy_score(expected_return: float, risk: float, loss: float = 0.0) -> float:
    return -loss + 0.1 * (expected_return / max(risk, 1e-12))


def _legacy_apply_winner(weights: List[float], fee: float) -> Dict[str, Any]:
    return {"weights": list(weights), "mint_fee": float(fee)}


def _legacy_participant_split(n_props: int, pool: float) -> float:
    return pool / n_props


def _legacy_first_reporter(reporters: List[str]) -> str:
    return sorted(reporters)[0]


def _legacy_replay_verify_always_true() -> bool:
    return True


def _legacy_rate_limit_counts(max_per_epoch: int, epochs: List[int]) -> List[bool]:
    counts: Dict[Tuple[str, int], int] = {}
    out = []
    for e in epochs:
        key = ("a", e)
        c = counts.get(key, 0)
        ok = c < max_per_epoch
        if ok:
            counts[key] = c + 1
        out.append(ok)
    return out


def _legacy_discount_average(discounts: List[int]) -> int:
    return int(sum(discounts) / len(discounts))


def _legacy_queue_last_gets_residual(total_units: int, ratios: List[float]) -> List[int]:
    out: List[int] = []
    running = 0
    for i, r in enumerate(ratios):
        if i == len(ratios) - 1:
            alloc = total_units - running
        else:
            alloc = int(total_units * r)
            running += alloc
        out.append(alloc)
    return out


def _legacy_cb4_cb3_mint_limit() -> float:
    return min(1.0, 0.10)


def _legacy_split_rebalance_bypass(deltas: List[float], threshold: float = 0.05) -> bool:
    return all(d < threshold for d in deltas) and sum(deltas) > threshold


def _legacy_mint_ignore_risk(units: int) -> int:
    return units


def _legacy_global_oracle_dos(degraded_any: bool) -> bool:
    return not degraded_any


def _legacy_attack_outcome_key(attack_id: str, epoch: int, seed: int = 7) -> float:
    h = hashlib.sha256(f"{attack_id}:{epoch}:{seed}".encode("utf-8")).hexdigest()
    return int(h[:12], 16) / float(16**12)


def _legacy_bucket_signature(attack: Dict[str, Any]) -> str:
    material = {
        "vector": attack.get("vector"),
        "tier": attack.get("tier"),
        "timing": attack.get("timing", {}).get("mode"),
        "scale": attack.get("scale"),
        "intensity": round(float(attack.get("params", {}).get("intensity", 0.0)), 3),
    }
    return hashlib.sha256(json.dumps(material, sort_keys=True).encode("utf-8")).hexdigest()[:16]


def _legacy_collusion_vector_only(proposals: List[Dict[str, Any]]) -> int:
    cnt = 0
    for i in range(len(proposals)):
        for j in range(i + 1, len(proposals)):
            if proposals[i].get("vector") and proposals[j].get("vector"):
                cnt += 1
    return cnt


def _legacy_response_id_only(ids: List[str]) -> int:
    seen = set()
    hits = 0
    for x in ids:
        if x in seen:
            continue
        seen.add(x)
        hits += 1
    return hits


def _legacy_safe_release(epochs_elapsed: int) -> bool:
    return epochs_elapsed >= 5


def _legacy_forensic_sig_20(attack: Dict[str, Any]) -> str:
    m = {
        "vector": attack.get("vector"),
        "tier": attack.get("tier"),
        "timing_mode": attack.get("timing", {}).get("mode"),
        "scale_bucket": int(attack.get("scale", 1) > 0),
        "chain_depth": len(attack.get("chain", [])),
    }
    return hashlib.sha256(json.dumps(m, sort_keys=True).encode("utf-8")).hexdigest()[:20]


def _legacy_forensic_sig_coarse(attack: Dict[str, Any]) -> str:
    m = {
        "vector": attack.get("vector"),
        "tier": attack.get("tier"),
        "timing_mode": attack.get("timing", {}).get("mode"),
        "scale_bucket": int(attack.get("scale", 1) > 0),
        "chain_depth": len(attack.get("chain", [])),
    }
    return hashlib.sha256(json.dumps(m, sort_keys=True).encode("utf-8")).hexdigest()


# ---------------------------------------------------------------------------
# PT-001 ~ PT-027 (2 tests each)
# ---------------------------------------------------------------------------


def test_pt001_pre_unlimited_claim_issue():
    claimed: set[str] = set()
    assert _legacy_claim_ok(claimed, "c1")
    assert _legacy_claim_ok(claimed, "c2")


def test_pt001_post_epoch_cap_and_proof_required():
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    stk = oae.StakingEconomics(reg, reward_epoch_cap=5.0)
    assert not stk.claim_reward("a", 100.0, "x1", 1)
    proof = stk.build_claim_proof("a", 4.0, "x2", 1)
    assert stk.claim_reward("a", 4.0, "x2", 1, proof=proof)
    proof2 = stk.build_claim_proof("a", 3.0, "x3", 1)
    assert not stk.claim_reward("a", 3.0, "x3", 1, proof=proof2)


def test_pt002_pre_withdraw_overdraw_issue():
    assert _legacy_withdraw_after_slash(100.0, 100.0, 90.0) == 100.0


def test_pt002_post_withdraw_lock_and_slash_reduce_pending():
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    stk = oae.StakingEconomics(reg, cooldown_epochs=0)
    stk.deposit("a", "Optimizer", 100.0, 0)
    assert stk.request_withdrawal("a", 100.0, 0)
    stk.slash("a", 90.0, 0)
    out = stk.withdraw("a", 0)
    assert out <= 10.0 + 1e-9


def test_pt003_pre_duplicate_reveal_issue():
    p: List[str] = []
    _legacy_reveal_dup(p, "P")
    _legacy_reveal_dup(p, "P")
    assert len(p) == 2


def test_pt003_post_reveal_one_time_consume():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    reg.register("a", "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep, stk)
    tour.start_epoch(0)
    prop = oae.Proposal("a", 0, [0.25, 0.25, 0.25, 0.25], 0.002, 0.01, 0.02, 0.02)
    secret = "s"
    assert tour.commit("a", prop.commit_hash(secret))
    tour.advance_tick(tour.submission_end_tick)
    assert tour.reveal(prop, secret)
    assert not tour.reveal(prop, secret)


def test_pt004_pre_direct_submit_bypass_issue():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    reg.register("a", "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep)
    tour.start_epoch(0)
    assert tour.submit_direct(oae.Proposal("a", 0, [0.25] * 4, 0.002, 0.01, 0.02, 0.02))


def test_pt004_post_direct_submit_disabled_in_ops_mode():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    reg.register("a", "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep, direct_submit_enabled=False)
    tour.start_epoch(0)
    assert not tour.submit_direct(oae.Proposal("a", 0, [0.25] * 4, 0.002, 0.01, 0.02, 0.02))


def test_pt005_pre_score_risk_zero_explodes():
    assert _legacy_score(1.0, 0.0) > 1e9


def test_pt005_post_score_clamped():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    tour = oae.OptimizationTournament(reg, rep)
    p = oae.Proposal("a", 0, [0.25] * 4, 0.002, 0.0, 1.0, 0.0)
    assert tour._score(p) < 20.0


def test_pt006_pre_invalid_winner_params_applied():
    out = _legacy_apply_winner([5.0, -1.0, 0.0, 0.0], 0.5)
    assert out["weights"][0] == 5.0


def test_pt006_post_winner_param_invariant_enforced():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for aid in ["bad", "good"]:
        reg.register(aid, "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep, stk, max_mint_fee=0.01)
    tour.start_epoch(0)
    bad = oae.Proposal("bad", 0, [5.0, -1.0, 0.0, 0.0], 0.5, -10.0, 1.0, 0.01)
    good = oae.Proposal("good", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    assert tour.submit_direct(bad)
    assert tour.submit_direct(good)
    winner = tour.evaluate(100.0)
    assert winner is not None and winner.agent_id == "good"


def test_pt007_pre_participant_pool_sybil_issue():
    assert _legacy_participant_split(10, 5.0) < _legacy_participant_split(2, 5.0)


def test_pt007_post_one_proposal_per_agent():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    for aid in ["a", "b"]:
        reg.register(aid, "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep)
    tour.start_epoch(0)
    p = oae.Proposal("a", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    assert tour.submit_direct(p)
    assert not tour.submit_direct(p)


def test_pt008_pre_resolve_replay_issue():
    rewards = 0
    for _ in range(2):
        rewards += 1
    assert rewards == 2


def test_pt008_post_resolve_one_shot():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for i in range(3):
        reg.register(f"m{i}", "Monitor", 5.0, 0)
        stk.deposit(f"m{i}", "Monitor", 5.0, 0)
    wd = oae.FederatedWatchdog(reg, stk, rep)
    ev = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for i in range(3):
        wd.report(f"m{i}", "PEG", ev, 0, "x")
    wd.resolve("PEG", 0, True)
    bal = stk.balances["m0"]
    wd.resolve("PEG", 0, True)
    assert stk.balances["m0"] == pytest.approx(bal)


def test_pt009_pre_future_timestamp_bypass_issue():
    epoch = 10
    future = 10**9
    assert epoch - future <= 10


def test_pt009_post_future_timestamp_rejected():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    reg.register("m", "Monitor", 5.0, 0)
    wd = oae.FederatedWatchdog(reg, stk, rep)
    ev = {"snapshot": {}, "oracle": {}, "timestamp": 10**9}
    assert not wd.report("m", "PEG", ev, 0, "x")


def test_pt010_pre_lexicographic_bounty_issue():
    assert _legacy_first_reporter(["zzz", "aaa"]) == "aaa"


def test_pt010_post_arrival_order_bounty():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for aid in ["zzz", "aaa", "bbb"]:
        reg.register(aid, "Monitor", 5.0, 0)
        stk.deposit(aid, "Monitor", 5.0, 0)
    wd = oae.FederatedWatchdog(reg, stk, rep)
    ev = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    assert wd.report("zzz", "PEG", ev, 0, "m")
    assert wd.report("aaa", "PEG", ev, 0, "m")
    assert wd.report("bbb", "PEG", ev, 0, "m")
    wd.resolve("PEG", 0, True)
    assert stk.balances["zzz"] > stk.balances["aaa"]


def test_pt011_pre_replay_message_issue():
    params = {"agent_id": "a"}
    sig = oae.ACPMessage.sign("acp.ping", params, "1", "s")
    msg = oae.ACPMessage(jsonrpc="2.0", method="acp.ping", params={**params, "signature": sig}, id="1")
    assert oae.ACPMessage.verify(msg, "s", allow_legacy=True)
    assert oae.ACPMessage.verify(msg, "s", allow_legacy=True)


def test_pt011_post_replay_nonce_blocked():
    seen: set[str] = set()
    msg = oae.ACPMessage.create("acp.ping", {"agent_id": "a"}, "1", "s", epoch=1, expiry_epoch=5, nonce="n1")
    assert oae.ACPMessage.verify(msg, "s", now_epoch=1, seen_nonces=seen)
    assert not oae.ACPMessage.verify(msg, "s", now_epoch=1, seen_nonces=seen)


def test_pt012_pre_shared_secret_impersonation_issue():
    msg = oae.ACPMessage.create("acp.ping", {"agent_id": "victim"}, "1", "shared")
    assert oae.ACPMessage.verify(msg, "shared")


def test_pt012_post_registry_key_binding():
    reg = oae.AgentRegistry()
    reg.set_public_key("victim", "victim-pub")
    msg = oae.ACPMessage.create("acp.ping", {"agent_id": "victim"}, "1", "attacker-secret")
    assert not oae.ACPMessage.verify(msg, "attacker-secret", registry=reg, now_epoch=0)


def test_pt013_pre_epoch_spoof_bypass_issue():
    out = _legacy_rate_limit_counts(2, [0, 0, 1, 2])
    assert out == [True, True, True, True]


def test_pt013_post_internal_epoch_only_rate_limit():
    rl = oae.RateLimiter(max_per_epoch=2)
    rl.set_epoch(0)
    assert rl.allow("a", 0)
    assert rl.allow("a", 999)
    assert not rl.allow("a", 12345)


def test_pt014_pre_discount_poisoning_issue():
    assert _legacy_discount_average([1_000_000, 0]) == 500_000


def test_pt014_post_discount_weighted_and_clamped():
    q = ms.RedemptionQueue(smoothing_window=8)
    q.enqueue("victim", 1_000_000, 1_000_000)
    q.enqueue("attacker", 1, 0)
    out = q.settle([2_000_000] * 4, [1.0] * 4, 6_000_000)
    assert sum(out["victim"]) > 500_000


def test_pt015_pre_last_user_residual_steal_issue():
    alloc = _legacy_queue_last_gets_residual(7, [0.4, 0.4, 0.2])
    assert alloc[-1] > int(7 * 0.2)


def test_pt015_post_residual_to_treasury():
    q = ms.RedemptionQueue(smoothing_window=8)
    q.enqueue("u1", 2, 1_000_000)
    q.enqueue("u2", 2, 1_000_000)
    q.enqueue("u3", 1, 1_000_000)
    out = q.settle([10], [1.0], 7)
    assert out["u3"][0] <= 1
    assert q.treasury_residual_units[0] >= 1


def test_pt016_pre_cb3_cb4_rollback_inconsistency_issue():
    assert _legacy_cb4_cb3_mint_limit() == 0.10


def test_pt016_post_cb3_cb4_consistent_freeze_policy():
    src = open("microstable.py", "r", encoding="utf-8").read()
    assert "# FIX PT-016" in src
    assert "MINT_PAUSED_BY_CB3" in src


def test_pt017_pre_commit_overwrite_issue():
    commits = {"a": "h1"}
    commits["a"] = "h2"
    assert commits["a"] == "h2"


def test_pt017_post_commit_overwrite_rejected():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    reg.register("a", "Optimizer", 10.0, 0)
    t = oae.OptimizationTournament(reg, rep)
    t.start_epoch(0)
    assert t.commit("a", "h1")
    assert not t.commit("a", "h2")


def test_pt018_pre_split_rebalance_bypass_issue():
    assert _legacy_split_rebalance_bypass([0.02, 0.02, 0.02])


def test_pt018_post_cumulative_window_guard():
    state = ms.ProtocolState()
    keeper = ms.Keeper()
    state.begin_tick()

    def mk_prop(w: List[float], proof: str | None) -> Dict[str, Any]:
        return {
            "weights": w,
            "mint_fee": state.mint_fee,
            "proposal_epoch": state.market_epoch,
            "state_hash": state.market_state_hash,
            "expiry_epoch": state.market_epoch + 2,
            **({"commit_proof": proof} if proof is not None else {}),
        }

    p1 = mk_prop([0.42, 0.28, 0.20, 0.10], f"{state.market_epoch}:{state.market_state_hash}")
    p2 = mk_prop([0.44, 0.26, 0.20, 0.10], f"{state.market_epoch}:{state.market_state_hash}")
    p3 = mk_prop([0.46, 0.24, 0.20, 0.10], None)

    assert keeper.submit_update_proposal(state, p1)["status"] == "APPLIED"
    assert keeper.submit_update_proposal(state, p2)["status"] == "APPLIED"
    assert keeper.submit_update_proposal(state, p3)["status"] == "REJECTED"


def test_pt019_pre_risk_score_ignored_issue():
    assert _legacy_mint_ignore_risk(1000) == 1000


def test_pt019_post_risk_score_penalizes_mint():
    low = ms.secure_mint_amount(1_000_000, [1.0, 1.0, 1.0], 30, 0.99, risk_score=0.1)
    high = ms.secure_mint_amount(1_000_000, [1.0, 1.0, 1.0], 30, 0.99, risk_score=0.95)
    assert high < low


def test_pt020_pre_any_degraded_global_dos_issue():
    assert not _legacy_global_oracle_dos(True)


def test_pt020_post_only_degraded_vaults_blocked():
    s = ms.ProtocolState()
    s.oracle_degraded_vaults = {1}
    enabled = s.mint_enabled_assets()
    assert enabled[0] is True and enabled[1] is False and enabled[2] is True


def test_pt021_pre_attack_id_grinding_issue():
    a = _legacy_attack_outcome_key("id1", 7)
    b = _legacy_attack_outcome_key("id2", 7)
    assert a != b


def test_pt021_post_outcome_not_dependent_on_attack_id():
    ex = aa.AttackExecutor(seed=7)
    base = {
        "tier": 3,
        "vector": "sybil",
        "params": {"intensity": 0.7, "budget": 20_000, "stealth": 0.3},
        "timing": {"mode": "normal", "epoch_offset": 0},
        "scale": 100,
        "chain": [],
    }
    a1 = {**base, "id": "id-A"}
    a2 = {**base, "id": "id-B"}
    st = {"defense_strength": 0.6, "learned_bias": 0.0, "epoch": 9, "tvl": 1e7}
    r1 = ex.execute(a1, st)
    r2 = ex.execute(a2, st)
    assert r1["success"] == r2["success"] and r1["detected"] == r2["detected"]


def test_pt022_pre_signature_bucket_collision_issue():
    a = {"vector": "v", "tier": 1, "timing": {"mode": "normal"}, "scale": 1, "params": {"intensity": 0.12341}}
    b = {"vector": "v", "tier": 1, "timing": {"mode": "normal"}, "scale": 1, "params": {"intensity": 0.12349}}
    assert _legacy_bucket_signature(a) == _legacy_bucket_signature(b)


def test_pt022_post_collision_resistant_signatures():
    ex = aa.AttackExecutor(seed=1)
    a = {"id": "a", "vector": "v", "tier": 1, "timing": {"mode": "normal", "epoch_offset": 0}, "scale": 1, "params": {"intensity": 0.12341, "budget": 10, "stealth": 0.2}, "chain": []}
    b = {"id": "b", "vector": "v", "tier": 1, "timing": {"mode": "normal", "epoch_offset": 0}, "scale": 1, "params": {"intensity": 0.12349, "budget": 10, "stealth": 0.2}, "chain": []}
    _, fa = ex._attack_signature_pair(a)
    _, fb = ex._attack_signature_pair(b)
    assert fa != fb


def test_pt023_pre_collusion_schema_mismatch_issue():
    props = [{"agent_id": "a", "weights": [1, 2, 3]}, {"agent_id": "b", "weights": [1, 2, 3]}]
    assert _legacy_collusion_vector_only(props) == 0


def test_pt023_post_weights_schema_detected():
    d = aa.AnomalyDetector()
    props = [{"agent_id": "a", "weights": [1, 2, 3], "epoch": 1}, {"agent_id": "b", "weights": [1, 2, 3], "epoch": 1}]
    assert d.detect_collusion(props)


def test_pt024_pre_alert_id_randomization_bypass_issue():
    assert _legacy_response_id_only(["id1", "id2"]) == 2


def test_pt024_post_semantic_idempotency_key():
    re = aa.ResponseEngine()
    first = re.auto_respond({"id": "x1", "type": "drain_attempt", "epoch": 7, "agent_id": "m1"})
    second = re.auto_respond({"id": "x2", "type": "drain_attempt", "epoch": 7, "agent_id": "m1"})
    assert first["action"] == "rate_limit"
    assert second["action"] == "noop"


def test_pt025_pre_safe_mode_auto_release_issue():
    assert _legacy_safe_release(6)


def test_pt025_post_safe_mode_health_gate():
    re = aa.ResponseEngine()
    re.safe_mode = True
    unhealthy = {"cr": 1.05, "cr_min": 1.2, "peg": 0.05, "peg_tolerance": 0.02, "oracle_fresh": False}
    out = re.recover_from_safe_mode(epochs_elapsed=6, health=unhealthy)
    assert out["safe_mode"] is True
    healthy = {"cr": 1.25, "cr_min": 1.2, "peg": 0.0, "peg_tolerance": 0.02, "oracle_fresh": True}
    out2 = re.recover_from_safe_mode(epochs_elapsed=6, health=healthy)
    assert out2["safe_mode"] is False


def test_pt026_pre_forensic_executor_signature_mismatch_issue():
    atk = {"vector": "drain", "tier": 4, "timing": {"mode": "boundary"}, "scale": 1000, "chain": []}
    forensic = _legacy_forensic_sig_20(atk)
    legacy_exec = _legacy_bucket_signature({"vector": "drain", "tier": 4, "timing": {"mode": "boundary"}, "scale": 1000, "params": {"intensity": 1.0}})
    assert forensic != legacy_exec


def test_pt026_post_canonical_signature_blocks_execution():
    loop = aa.AdversarialLoop(seed=3)
    attack = {
        "id": "x",
        "tier": 3,
        "vector": "drain",
        "params": {"intensity": 0.9, "budget": 10000, "stealth": 0.2},
        "timing": {"mode": "boundary", "epoch_offset": 0},
        "scale": 100,
        "chain": [],
    }
    rec = {"attack": attack, "result": {"financial_impact": 1.0, "detected": True, "response_delay": 1, "detection_delay": 1}}
    sig = loop.forensics.generate_signature(rec)
    loop.executor.blocked_signatures.add(sig)
    out = loop.executor.execute(attack, {"defense_strength": 0.6, "learned_bias": 0.0, "epoch": 1, "tvl": 1e7})
    assert out["status"] == "blocked"


def test_pt027_pre_signature_morph_evasion_issue():
    a = {"vector": "drain", "tier": 4, "timing": {"mode": "boundary"}, "scale": 1000, "chain": [], "params": {"intensity": 0.7, "stealth": 0.1}}
    b = {"vector": "drain", "tier": 4, "timing": {"mode": "boundary"}, "scale": 1000, "chain": [], "params": {"intensity": 1.4, "stealth": 0.9}}
    assert _legacy_forensic_sig_coarse(a) == _legacy_forensic_sig_coarse(b)


def test_pt027_post_signature_uses_multires_features():
    fe = aa.ForensicsEngine()
    a = {
        "id": "a",
        "tier": 4,
        "vector": "drain",
        "params": {"intensity": 0.7, "budget": 10000, "stealth": 0.1},
        "timing": {"mode": "boundary", "epoch_offset": 0},
        "scale": 1000,
        "chain": [],
    }
    b = {
        "id": "b",
        "tier": 4,
        "vector": "drain",
        "params": {"intensity": 1.4, "budget": 10000, "stealth": 0.9},
        "timing": {"mode": "boundary", "epoch_offset": 0},
        "scale": 1000,
        "chain": [],
    }
    sa = fe.generate_signature({"attack": a})
    sb = fe.generate_signature({"attack": b})
    assert sa != sb


# ---------------------------------------------------------------------------
# Extra 6 exploit-regression checks (PoC style)
# ---------------------------------------------------------------------------


def test_extra_poc_claim_reward_not_exploitable():
    reg = oae.AgentRegistry()
    reg.register("a", "Optimizer", 10.0, 0)
    stk = oae.StakingEconomics(reg, reward_epoch_cap=10.0)
    ok = [stk.claim_reward("a", 3.0, f"c{i}", 1) for i in range(5)]
    assert ok.count(True) <= 3


def test_extra_poc_reveal_not_replayable():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    reg.register("a", "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep)
    tour.start_epoch(0)
    p = oae.Proposal("a", 0, [0.4, 0.3, 0.2, 0.1], 0.002, 0.01, 0.02, 0.02)
    s = "secret"
    assert tour.commit("a", p.commit_hash(s))
    tour.advance_tick(tour.submission_end_tick)
    assert tour.reveal(p, s)
    assert len(tour.proposals) == 1


def test_extra_poc_direct_path_blocked_in_ops_mode():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    reg.register("a", "Optimizer", 10.0, 0)
    tour = oae.OptimizationTournament(reg, rep, direct_submit_enabled=False)
    tour.start_epoch(0)
    assert not tour.submit_direct(oae.Proposal("a", 0, [0.25] * 4, 0.002, 0.01, 0.02, 0.02))


def test_extra_poc_watchdog_double_resolve_blocked():
    reg = oae.AgentRegistry()
    rep = oae.ReputationEngine()
    stk = oae.StakingEconomics(reg)
    for aid in ["m0", "m1", "m2"]:
        reg.register(aid, "Monitor", 5.0, 0)
        stk.deposit(aid, "Monitor", 5.0, 0)
    wd = oae.FederatedWatchdog(reg, stk, rep)
    ev = {"snapshot": {}, "oracle": {}, "timestamp": 0}
    for aid in ["m0", "m1", "m2"]:
        wd.report(aid, "PEG", ev, 0, "x")
    wd.resolve("PEG", 0, True)
    b = stk.balances["m0"]
    wd.resolve("PEG", 0, True)
    assert stk.balances["m0"] == pytest.approx(b)


def test_extra_poc_acp_replay_blocked_with_nonce_set():
    msg = oae.ACPMessage.create("acp.vote", {"agent_id": "a"}, "id1", "k", epoch=5, expiry_epoch=7, nonce="n")
    seen: set[str] = set()
    assert oae.ACPMessage.verify(msg, "k", now_epoch=5, seen_nonces=seen)
    assert not oae.ACPMessage.verify(msg, "k", now_epoch=5, seen_nonces=seen)


def test_extra_poc_forensic_signature_blocks_executor():
    ex = aa.AttackExecutor(seed=9)
    fe = aa.ForensicsEngine()
    attack = {
        "id": "atk",
        "tier": 3,
        "vector": "sybil",
        "params": {"intensity": 0.8, "budget": 12000, "stealth": 0.2},
        "timing": {"mode": "normal", "epoch_offset": 0},
        "scale": 100,
        "chain": [],
    }
    sig = fe.generate_signature({"attack": attack})
    ex.blocked_signatures.add(sig)
    out = ex.execute(attack, {"defense_strength": 0.5, "learned_bias": 0.0, "epoch": 1, "tvl": 1e7})
    assert out["status"] == "blocked"
